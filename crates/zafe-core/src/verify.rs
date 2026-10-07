//! Independent verification of a proposal's PCZT by each member (spec §9.3).
//!
//! A member's app runs this before showing Approve and again before contributing a
//! signature share. It never trusts the proposer, the leader or the relay: everything is
//! recomputed from the PCZT against the vault's own keys.
//!
//! What is checked:
//! 1. Network and format: v6 transaction, expected consensus branch ID, sane expiry.
//! 2. Pools: only Ironwood actions; no transparent, Sapling or Orchard-pool components.
//! 3. Spends: every spend is either a vault note (nullifier and `rk` checked against the
//!    vault FVK) or a zero-value padding spend already signed by the IO Finalizer.
//! 4. Outputs: every non-zero output is either change to the vault's own address that the
//!    vault can decrypt, or an
//!    exact match (recipient, amount, memo) for one proposed payment, recovered by
//!    decrypting with the vault's outgoing viewing key. Every payment is matched once.
//! 5. Value: spends − outputs = fee, and the fee equals the ZIP 317 conventional fee.
//! 6. Sighash: computed locally; this is the only value a member may sign.

use orchard::{
    keys::{FullViewingKey, Scope},
    note_encryption::IronwoodDomain,
    Address,
};
use pczt::{roles::verifier::Verifier, Pczt};
use zcash_note_encryption::{try_note_decryption, try_output_recovery_with_ovk};
use zcash_protocol::constants::V6_TX_VERSION;

use crate::tx::{self, SpendToSign, TxError};

/// ZIP 317 marginal fee per logical action, in zatoshis.
pub const ZIP317_MARGINAL_FEE: u64 = 5_000;
/// ZIP 317 grace actions.
pub const ZIP317_GRACE_ACTIONS: u64 = 2;

/// A payment the proposal claims to make.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Payment {
    pub recipient: Address,
    pub amount_zat: u64,
    /// The full 512-byte memo field (ZIP 302 encoding).
    pub memo: [u8; 512],
}

/// What the member expects, independent of the PCZT.
#[derive(Clone, Debug)]
pub struct Expectations {
    pub payments: Vec<Payment>,
    /// Consensus branch ID for the height the transaction will be mined at.
    pub consensus_branch_id: u32,
    /// Current chain tip height.
    pub tip_height: u32,
    /// Largest acceptable `expiry_height - tip_height`.
    pub max_expiry_delta: u32,
}

/// The facts a member approved: exactly what the approval screen must show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedTx {
    pub sighash: [u8; 32],
    pub spends_to_sign: Vec<SpendToSign>,
    pub input_total_zat: u64,
    pub payments: Vec<Payment>,
    pub change_total_zat: u64,
    pub fee_zat: u64,
    pub expiry_height: u32,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum VerifyError {
    #[error("not a v6 transaction (version {0})")]
    WrongTxVersion(u32),
    #[error("consensus branch ID {actual:#x} does not match expected {expected:#x}")]
    WrongBranchId { expected: u32, actual: u32 },
    #[error("expiry height {expiry} is outside ({tip}, {tip} + {max_delta}]")]
    BadExpiry {
        expiry: u32,
        tip: u32,
        max_delta: u32,
    },
    #[error("transaction has {0} components; only Ironwood actions are allowed")]
    ForeignPool(&'static str),
    #[error("action {0}: spend is not a vault note and not a zero-value dummy")]
    ForeignSpend(usize),
    #[error("action {0}: spent note is inconsistent with its nullifier")]
    BadSpendNote(usize),
    #[error("action {0}: output note is inconsistent with its commitment or value commitment")]
    BadOutputNote(usize),
    #[error("action {0}: change output cannot be decrypted by the vault")]
    UndecryptableChange(usize),
    #[error("action {0}: output cannot be recovered with the vault's outgoing viewing key")]
    UnrecoverableOutput(usize),
    #[error("action {0}: output does not match any remaining proposed payment")]
    UnexpectedOutput(usize),
    #[error("{0} proposed payment(s) have no matching output")]
    MissingPayments(usize),
    #[error("outputs exceed inputs")]
    NegativeFee,
    #[error("the transaction's values overflow")]
    ValueOverflow,
    #[error("fee {actual} zat does not equal the ZIP 317 fee {expected} zat")]
    WrongFee { expected: u64, actual: u64 },
    #[error("{0}")]
    Tx(String),
}

impl From<TxError> for VerifyError {
    fn from(e: TxError) -> Self {
        match e {
            TxError::NotVaultSpend(i) => VerifyError::ForeignSpend(i),
            other => VerifyError::Tx(other.to_string()),
        }
    }
}

/// Runs every §9.3 check. Returns the verified facts, or the first failure.
pub fn verify_pczt(
    pczt: &Pczt,
    vault_fvk: &FullViewingKey,
    expected: &Expectations,
) -> Result<VerifiedTx, VerifyError> {
    use pczt::roles::verifier::OrchardError;

    // 1. Network and format.
    let global = pczt.global();
    if *global.tx_version() != V6_TX_VERSION {
        return Err(VerifyError::WrongTxVersion(*global.tx_version()));
    }
    if *global.consensus_branch_id() != expected.consensus_branch_id {
        return Err(VerifyError::WrongBranchId {
            expected: expected.consensus_branch_id,
            actual: *global.consensus_branch_id(),
        });
    }
    let expiry = *global.expiry_height();
    if expiry <= expected.tip_height || expiry - expected.tip_height > expected.max_expiry_delta {
        return Err(VerifyError::BadExpiry {
            expiry,
            tip: expected.tip_height,
            max_delta: expected.max_expiry_delta,
        });
    }

    // 2. Pools.
    if !pczt.transparent().inputs().is_empty() || !pczt.transparent().outputs().is_empty() {
        return Err(VerifyError::ForeignPool("transparent"));
    }
    if !pczt.sapling().spends().is_empty() || !pczt.sapling().outputs().is_empty() {
        return Err(VerifyError::ForeignPool("Sapling"));
    }
    if !pczt.orchard().actions().is_empty() {
        return Err(VerifyError::ForeignPool("Orchard-pool"));
    }

    // 3–5. Walk every Ironwood action with the full Verifier parse.
    let ovk = vault_fvk.to_ovk(Scope::External);
    let mut remaining = expected.payments.clone();
    let mut input_total: u64 = 0;
    let mut output_total: u64 = 0;
    let mut change_total: u64 = 0;
    let mut action_count: u64 = 0;

    Verifier::new(pczt.clone())
        .with_ironwood::<VerifyError, _>(|bundle| {
            let custom = OrchardError::Custom;
            action_count = bundle.actions().len() as u64;
            // Pass 1: every spend. Checked before any output so a foreign spend is always
            // reported as such, whatever order the builder shuffled the actions into.
            for (i, action) in bundle.actions().iter().enumerate() {
                action
                    .verify_cv_net()
                    .map_err(|_| custom(VerifyError::BadOutputNote(i)))?;

                let spend = action.spend();
                let spend_value = spend.value().map(|v| v.inner());
                if spend.fvk().as_ref() == Some(vault_fvk) {
                    spend
                        .verify_nullifier(Some(vault_fvk))
                        .map_err(|_| custom(VerifyError::BadSpendNote(i)))?;
                    let value = spend_value.ok_or(custom(VerifyError::BadSpendNote(i)))?;
                    input_total = input_total
                        .checked_add(value)
                        .ok_or(custom(VerifyError::ValueOverflow))?;
                } else {
                    // A padding (dummy) spend: zero value, already signed by the IO
                    // Finalizer with its own random key, and internally consistent.
                    // It moves no value and needs nothing from the vault.
                    let is_dummy = spend_value == Some(0)
                        && spend.spend_auth_sig().is_some()
                        && spend.verify_nullifier(None).is_ok();
                    if !is_dummy {
                        return Err(custom(VerifyError::ForeignSpend(i)));
                    }
                }
            }

            // Pass 2: every output.
            for (i, action) in bundle.actions().iter().enumerate() {
                let spend = action.spend();
                // Output side: plaintext fields must match the note commitment.
                let output = action.output();
                output
                    .verify_note_commitment(spend)
                    .map_err(|_| custom(VerifyError::BadOutputNote(i)))?;
                let value = output
                    .value()
                    .expect("checked by verify_note_commitment")
                    .inner();
                let recipient = output
                    .recipient()
                    .expect("checked by verify_note_commitment");
                if value == 0 {
                    continue; // dummy or zero-value output: moves no funds
                }
                output_total = output_total
                    .checked_add(value)
                    .ok_or(custom(VerifyError::ValueOverflow))?;

                let domain = IronwoodDomain::for_pczt_action(action);
                if let Some(scope) = vault_fvk.scope_for_address(&recipient) {
                    // Change must be decryptable by the vault, or the wallet would never
                    // detect it and the funds would be lost.
                    let ivk = vault_fvk.to_ivk(scope).prepare();
                    let (note, address, _) = try_note_decryption(&domain, &ivk, action)
                        .ok_or(custom(VerifyError::UndecryptableChange(i)))?;
                    if address != recipient || note.value().inner() != value {
                        return Err(custom(VerifyError::UndecryptableChange(i)));
                    }
                    change_total = change_total
                        .checked_add(value)
                        .ok_or(custom(VerifyError::ValueOverflow))?;
                    continue;
                }

                // A payment: recover it with the vault OVK. Recovery decrypts the real
                // ciphertext and checks it against the note commitment, so the recipient,
                // value and memo compared below are what the recipient will receive.
                let (note, address, memo) = try_output_recovery_with_ovk(
                    &domain,
                    &ovk,
                    action,
                    action.cv_net(),
                    &output.encrypted_note().out_ciphertext,
                )
                .ok_or(custom(VerifyError::UnrecoverableOutput(i)))?;
                if address != recipient || note.value().inner() != value {
                    return Err(custom(VerifyError::BadOutputNote(i)));
                }
                let pos = remaining
                    .iter()
                    .position(|p| p.recipient == address && p.amount_zat == value && p.memo == memo)
                    .ok_or(custom(VerifyError::UnexpectedOutput(i)))?;
                remaining.swap_remove(pos);
            }
            Ok(())
        })
        .map_err(|e| match e {
            OrchardError::Custom(inner) => inner,
            other => VerifyError::Tx(format!("{other:?}")),
        })?;

    if !remaining.is_empty() {
        return Err(VerifyError::MissingPayments(remaining.len()));
    }

    // 5. Fee.
    let fee = input_total
        .checked_sub(output_total)
        .ok_or(VerifyError::NegativeFee)?;
    let expected_fee = ZIP317_MARGINAL_FEE * action_count.max(ZIP317_GRACE_ACTIONS);
    if fee != expected_fee {
        return Err(VerifyError::WrongFee {
            expected: expected_fee,
            actual: fee,
        });
    }

    // 3 (signing list) and 6 (sighash).
    let spends_to_sign = tx::spends_to_sign(pczt, vault_fvk)?;
    let sighash = tx::shielded_sighash(pczt)?;

    Ok(VerifiedTx {
        sighash,
        spends_to_sign,
        input_total_zat: input_total,
        payments: expected.payments.clone(),
        change_total_zat: change_total,
        fee_zat: fee,
        expiry_height: expiry,
    })
}
