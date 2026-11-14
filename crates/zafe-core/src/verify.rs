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
//! 4. Outputs: every output to the vault is change, recovered with the vault's internal
//!    outgoing viewing key and exactly the vault's change address (internal, index 0), its
//!    value and an empty memo, and decryptable by the vault; every other non-zero output is
//!    an exact match (recipient, amount, memo) for one proposed payment, recovered with the
//!    vault's external outgoing viewing key. Every payment is matched once. So every
//!    ciphertext the vault writes is fixed by what members see (zero-value dummy outputs'
//!    stay random by construction: a residual channel).
//! 5. Value: spends − outputs = fee, and the fee equals the ZIP 317 conventional fee. The
//!    Ironwood bundle's value balance (`value_sum`, what the network charges) equals that
//!    fee, its flags are exactly the builder's, and every other pool is empty.
//!    The action count it is charged for may not exceed what the transaction needs:
//!    max(2, vault spends worth more than the marginal fee, payments + change outputs),
//!    with at most [`MAX_CHANGE_OUTPUTS`] change outputs, and a zero-value change output
//!    counted only when there is no other change. Padding beyond that (dummy spends,
//!    zero-value or dust vault spends, zero-value outputs) would raise the fee out of
//!    change. Every vault spend is still verified and signed, whatever its value.
//! 6. Sighash: computed locally; this is the only value a member may sign.

use orchard::{
    keys::{FullViewingKey, Scope},
    note_encryption::IronwoodDomain,
    Address,
};
use pczt::{roles::verifier::Verifier, Pczt};
use zcash_note_encryption::{try_note_decryption, try_output_recovery_with_ovk};
use zcash_protocol::{constants::V6_TX_VERSION, memo::MemoBytes};

use crate::tx::{self, SpendToSign, TxError};

/// ZIP 317 marginal fee per logical action, in zatoshis.
pub const ZIP317_MARGINAL_FEE: u64 = 5_000;
/// ZIP 317 grace actions.
pub const ZIP317_GRACE_ACTIONS: u64 = 2;
/// The most change outputs a proposal may count toward its action bound: the wallet's
/// split policy never makes more ([`crate::wallet::CHANGE_NOTES_TARGET`]).
pub const MAX_CHANGE_OUTPUTS: u64 = crate::wallet::CHANGE_NOTES_TARGET as u64;

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
    #[error(
        "action {0}: change output isn't exactly the vault's change (address, value, empty memo)"
    )]
    UnexpectedChange(usize),
    #[error("Ironwood value balance {actual} zat does not equal the fee {fee} zat")]
    WrongValueBalance { actual: i64, fee: u64 },
    #[error("Ironwood bundle flags are not the ones the wallet sets")]
    WrongFlags,
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
    #[error("transaction has {actions} actions where {needed} suffice: padding raises the fee")]
    TooManyActions { actions: u64, needed: u64 },
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
    // Empty bundles move nothing: their value sums must say so too.
    if *pczt.sapling().value_sum() != 0 {
        return Err(VerifyError::ForeignPool("Sapling"));
    }
    if *pczt.orchard().value_sum() != (0, false) {
        return Err(VerifyError::ForeignPool("Orchard-pool"));
    }

    // 3–5. Walk every Ironwood action with the full Verifier parse.
    let ovk = vault_fvk.to_ovk(Scope::External);
    let internal_ovk = vault_fvk.to_ovk(Scope::Internal);
    let change_address = vault_fvk.address_at(0u32, Scope::Internal);
    let empty_memo = *MemoBytes::empty().as_array();
    let mut value_balance: Option<i64> = None;
    let mut remaining = expected.payments.clone();
    let mut input_total: u64 = 0;
    let mut output_total: u64 = 0;
    let mut change_total: u64 = 0;
    let mut action_count: u64 = 0;
    // What the transaction needs, for the action bound (step 5). Only vault spends worth
    // more than the marginal fee count: see there.
    let mut vault_spends: u64 = 0;
    let mut payment_outputs: u64 = 0;
    let mut change_outputs: u64 = 0;
    let mut zero_change_outputs: u64 = 0;

    Verifier::new(pczt.clone())
        .with_ironwood::<VerifyError, _>(|bundle| {
            let custom = OrchardError::Custom;
            action_count = bundle.actions().len() as u64;
            // The flags the wallet's builder sets for an Ironwood bundle: spends, outputs and
            // cross-address transfers on (zcash_primitives 0.31.0-pre.1
            // `transaction/builder.rs:388-400`, `BundleVersion::ironwood_v3().default_flags()`;
            // orchard 0.16 `bundle.rs:175-176`). Others change the sighash (spends off makes
            // a transaction that can't be mined) without changing what is shown.
            if *bundle.flags() != orchard::bundle::BundleVersion::ironwood_v3().default_flags() {
                return Err(custom(VerifyError::WrongFlags));
            }
            value_balance = i64::try_from(*bundle.value_sum()).ok();
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
                    if value > ZIP317_MARGINAL_FEE {
                        vault_spends += 1;
                    }
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
                let domain = IronwoodDomain::for_pczt_action(action);
                if let Some(scope) = vault_fvk.scope_for_address(&recipient) {
                    // Change (zero-value change too: the wallet adds it when nothing is left
                    // over). It must be decryptable by the vault, or the wallet would never
                    // detect it and the funds would be lost.
                    let ivk = vault_fvk.to_ivk(scope).prepare();
                    let (note, address, _) = try_note_decryption(&domain, &ivk, action)
                        .ok_or(custom(VerifyError::UndecryptableChange(i)))?;
                    if address != recipient || note.value().inner() != value {
                        return Err(custom(VerifyError::UndecryptableChange(i)));
                    }
                    // And recovered with the internal OVK as exactly what the wallet makes,
                    // so its `out_ciphertext` (and memo) carry nothing else.
                    let (note, address, memo) = try_output_recovery_with_ovk(
                        &domain,
                        &internal_ovk,
                        action,
                        action.cv_net(),
                        &output.encrypted_note().out_ciphertext,
                    )
                    .ok_or(custom(VerifyError::UnexpectedChange(i)))?;
                    if address != change_address
                        || note.value().inner() != value
                        || memo != empty_memo
                    {
                        return Err(custom(VerifyError::UnexpectedChange(i)));
                    }
                    if value == 0 {
                        zero_change_outputs += 1;
                        continue;
                    }
                    output_total = output_total
                        .checked_add(value)
                        .ok_or(custom(VerifyError::ValueOverflow))?;
                    change_total = change_total
                        .checked_add(value)
                        .ok_or(custom(VerifyError::ValueOverflow))?;
                    change_outputs += 1;
                    continue;
                }
                if value == 0 {
                    // A dummy: moves no funds, to a random address.
                    continue;
                }
                output_total = output_total
                    .checked_add(value)
                    .ok_or(custom(VerifyError::ValueOverflow))?;

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
                payment_outputs += 1;
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

    // 5. Value, part 1: the action bound. ZIP 317 charges every action, and whoever builds
    // the PCZT chooses how many there are: dummy spends and zero-value outputs move no
    // funds, so without a bound a builder could pad the bundle and take the extra fee out
    // of change. Ironwood bundles enable cross-address transfers, so the builder pairs
    // spends with outputs and pads only up to 2 (orchard 0.16 `BundleType::num_actions`,
    // `BundlePadding::DEFAULT`): an honest transaction has max(2, spends, outputs) actions.
    // The wallet adds a zero-value change output only when there is no other change, and
    // splits change into at most `MAX_CHANGE_OUTPUTS` notes. Fewer actions are fine: the
    // fee only goes down.
    //
    // A vault spend counts only when the note is worth more than the marginal fee. A
    // zero-value spend needs no real note (the action circuit skips the Merkle check when
    // v_old = 0, orchard 0.16 `circuit.rs` "Either v_old = 0, or root = anchor"), so
    // counting it would let a builder pad with vault-addressed zero-value notes. Honest
    // builders never spend such a note in an action of its own: zcash_client_sqlite never
    // selects notes at or below `MARGINAL_FEE` (`wallet/common.rs`, `rn.value > :min_value`),
    // zcash_client_backend admits them only where they add no action
    // (`fees/common.rs`, `check_for_uneconomic_inputs`), and Ironwood bundles always allow
    // cross-address transfers, so no builder pairs change with a wallet-controlled
    // zero-value spend (zcash_primitives `transaction/builder.rs`, `default_flags()`).
    let change_counted = if change_outputs > 0 {
        change_outputs
    } else {
        zero_change_outputs.min(1)
    };
    let needed = ZIP317_GRACE_ACTIONS
        .max(vault_spends)
        .max(payment_outputs + change_counted.min(MAX_CHANGE_OUTPUTS));
    if action_count > needed {
        return Err(VerifyError::TooManyActions {
            actions: action_count,
            needed,
        });
    }

    // 5. Value, part 2: the fee.
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
    // What the network charges is the bundle's value balance, not the sum of the actions
    // above: they must agree, or the signed transaction is another one (unmineable, since
    // the binding signature ties actions to the balance) than the fee shown.
    if value_balance != i64::try_from(fee).ok() {
        return Err(VerifyError::WrongValueBalance {
            actual: value_balance.unwrap_or(i64::MIN),
            fee,
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
