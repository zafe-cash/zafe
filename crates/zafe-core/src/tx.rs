//! PCZT inspection and signature injection for vault spends (spec §9.3, §9.5).
//!
//! Zafe vaults are `use_qsk = true` accounts created after NU6.3, so they only ever hold
//! Ironwood-pool notes (ZIP 326). Any unsigned Orchard-pool spend is rejected.

use orchard::{keys::FullViewingKey, primitives::redpallas};
use pasta_curves::pallas;
use pczt::{
    roles::{signer::Signer, verifier::Verifier},
    Pczt,
};

#[derive(Debug, thiserror::Error)]
pub enum TxError {
    #[error("PCZT could not be parsed or verified: {0}")]
    Parse(String),
    #[error("Ironwood action {0} has an unsigned spend without a randomizer (alpha)")]
    MissingAlpha(usize),
    #[error("Ironwood action {0} spends a note that does not belong to this vault")]
    NotVaultSpend(usize),
    #[error("Ironwood action {index}: rk does not match the vault key and alpha: {reason}")]
    RkMismatch { index: usize, reason: String },
    #[error(
        "the PCZT has unsigned Orchard-pool spends; Zafe vaults never hold Orchard-pool notes"
    )]
    UnexpectedOrchardSpend,
    #[error("signing: {0}")]
    Sign(String),
}

/// A spend that needs a FROST signature from the vault.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpendToSign {
    /// Index of the action within the Ironwood bundle.
    pub action_index: usize,
    /// The randomizer fixed by the transaction builder; the FROST randomizer (spec §9.5.1).
    pub alpha: pallas::Scalar,
    /// `rk = ak + [alpha]G`, as committed in the transaction.
    pub rk: [u8; 32],
}

/// Lists every Ironwood spend still awaiting a spend authorization signature, checking
/// that each one belongs to the vault and that its `rk` matches the vault key and `alpha`.
///
/// Uses the full Verifier parse, which derives each spend's FVK from the wire data.
/// Dummy spends are already signed by the IO Finalizer and are skipped. No filtering by
/// value: zero-value spends of vault notes still need signatures.
pub fn spends_to_sign(
    pczt: &Pczt,
    vault_fvk: &FullViewingKey,
) -> Result<Vec<SpendToSign>, TxError> {
    use pczt::roles::verifier::OrchardError;

    if has_unsigned_orchard_spends(pczt)? {
        return Err(TxError::UnexpectedOrchardSpend);
    }

    let mut found = Vec::new();
    Verifier::new(pczt.clone())
        .with_ironwood::<TxError, _>(|bundle| {
            for (index, action) in bundle.actions().iter().enumerate() {
                let spend = action.spend();
                if spend.spend_auth_sig().is_some() {
                    continue;
                }
                let alpha = spend
                    .alpha()
                    .ok_or(OrchardError::Custom(TxError::MissingAlpha(index)))?;
                if spend.fvk().as_ref() != Some(vault_fvk) {
                    return Err(OrchardError::Custom(TxError::NotVaultSpend(index)));
                }
                spend.verify_rk(Some(vault_fvk)).map_err(|e| {
                    OrchardError::Custom(TxError::RkMismatch {
                        index,
                        reason: format!("{e:?}"),
                    })
                })?;
                // orchard 0.16 uses pasta_curves 0.6, reddsa (FROST) 0.5: same field,
                // moved through its canonical bytes.
                let alpha = <pallas::Scalar as ff::PrimeField>::from_repr(
                    ff14::PrimeField::to_repr(&alpha),
                )
                .into_option()
                .ok_or(OrchardError::Custom(TxError::MissingAlpha(index)))?;
                found.push(SpendToSign {
                    action_index: index,
                    alpha,
                    rk: spend.rk().into(),
                });
            }
            Ok(())
        })
        .map_err(orchard_error)?;
    Ok(found)
}

/// Nullifiers of every Ironwood spend in the PCZT (dummy spends included: their random
/// nullifiers match no wallet note).
pub fn spent_nullifiers(pczt: &Pczt) -> Result<Vec<[u8; 32]>, TxError> {
    let mut found = Vec::new();
    Verifier::new(pczt.clone())
        .with_ironwood::<TxError, _>(|bundle| {
            found.extend(
                bundle
                    .actions()
                    .iter()
                    .map(|a| a.spend().nullifier().to_bytes()),
            );
            Ok(())
        })
        .map_err(orchard_error)?;
    Ok(found)
}

/// The shielded sighash, computed locally from the PCZT (v6 for Ironwood transactions).
/// This is the only value a member may use as the FROST message.
pub fn shielded_sighash(pczt: &Pczt) -> Result<[u8; 32], TxError> {
    Ok(Signer::new(pczt.clone())
        .map_err(|e| TxError::Parse(format!("{e:?}")))?
        .shielded_sighash())
}

/// Checks that `tx` is `pczt`'s transaction, signed, and returns its txid: it parses for
/// the PCZT's consensus branch with nothing left over, it has no transparent part, its
/// txid is the PCZT's shielded sighash, and every Ironwood action's spend authorization
/// signature verifies against its `rk` over that sighash (dummy spends are signed over it
/// too, by the IO Finalizer). The txid commits to every action's effecting data (`rk`,
/// nullifiers, notes), so a matching txid means the same actions; signatures aren't in the
/// txid, so the check is what proves the vault signed. Proofs and the binding signature
/// are not checked (cost): a transaction that fails them can't be mined, so it can't pay
/// anyone either.
///
/// Without a transparent part, a v6 transaction's shielded sighash is its txid: both hash
/// `hash_transparent_txid_data(None)` with the same header and bundle digests
/// (zcash_primitives 0.31.0-pre.1 `sighash_v5.rs:38-40`, `sighash_v6.rs:24-35`,
/// `txid.rs:467-480`).
pub fn check_signed(pczt: &Pczt, tx: &[u8]) -> Result<[u8; 32], TxError> {
    use zcash_primitives::transaction::Transaction;
    let bad = |what: &str| TxError::Parse(what.to_owned());
    let branch =
        zcash_protocol::consensus::BranchId::try_from(*pczt.global().consensus_branch_id())
            .map_err(|_| bad("unknown consensus branch"))?;
    let mut reader = tx;
    let tx = Transaction::read(&mut reader, branch).map_err(|e| bad(&e.to_string()))?;
    if !reader.is_empty() {
        return Err(bad("bytes after the transaction"));
    }
    if tx.transparent_bundle().is_some() {
        return Err(bad("a transparent part"));
    }
    let txid: [u8; 32] = *tx.txid().as_ref();
    if txid != shielded_sighash(pczt)? {
        return Err(bad("another transaction"));
    }
    let bundle = tx
        .ironwood_bundle()
        .ok_or_else(|| bad("no Ironwood actions"))?;
    for (i, action) in bundle.actions().iter().enumerate() {
        action
            .rk()
            .verify(&txid, action.authorization())
            .map_err(|_| TxError::Sign(format!("action {i}: spend authorization")))?;
    }
    Ok(txid)
}

/// Injects aggregated spend authorization signatures into the PCZT. Each signature is
/// checked against its action's `rk` and the sighash before it is accepted.
pub fn apply_signatures(pczt: Pczt, signatures: &[(usize, [u8; 64])]) -> Result<Pczt, TxError> {
    let mut signer = Signer::new(pczt).map_err(|e| TxError::Parse(format!("{e:?}")))?;
    for (index, sig) in signatures {
        signer
            .apply_ironwood_signature(*index, redpallas::Signature::from(*sig))
            .map_err(|e| TxError::Sign(format!("action {index}: {e:?}")))?;
    }
    Ok(signer.finish())
}

fn has_unsigned_orchard_spends(pczt: &Pczt) -> Result<bool, TxError> {
    if pczt.orchard().actions().is_empty() {
        return Ok(false);
    }
    let mut unsigned = false;
    Verifier::new(pczt.clone())
        .with_orchard::<TxError, _>(|bundle| {
            unsigned = bundle
                .actions()
                .iter()
                .any(|a| a.spend().spend_auth_sig().is_none());
            Ok(())
        })
        .map_err(orchard_error)?;
    Ok(unsigned)
}

fn orchard_error(e: pczt::roles::verifier::OrchardError<TxError>) -> TxError {
    use pczt::roles::verifier::OrchardError;
    match e {
        OrchardError::Custom(inner) => inner,
        other => TxError::Parse(format!("{other:?}")),
    }
}
