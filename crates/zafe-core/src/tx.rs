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
    #[error("Ironwood action {0} still carries a dummy spending key (dummy_sk)")]
    DummyKey(usize),
    #[error("Ironwood action {0}: alpha is zero, so rk would reveal the vault key")]
    ZeroAlpha(usize),
    #[error("Ironwood action {0}: alpha repeats another spend's, so their rk would match")]
    RepeatedAlpha(usize),
    #[error(
        "the PCZT has unsigned Orchard-pool spends; Zafe vaults never hold Orchard-pool notes"
    )]
    UnexpectedOrchardSpend,
    #[error("signing: {0}")]
    Sign(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpendToSign {
    pub action_index: usize,
    pub alpha: pallas::Scalar,
    pub rk: [u8; 32],
}

pub fn spends_to_sign(
    pczt: &Pczt,
    vault_fvk: &FullViewingKey,
) -> Result<Vec<SpendToSign>, TxError> {
    use pczt::roles::verifier::OrchardError;

    if has_unsigned_orchard_spends(pczt)? {
        return Err(TxError::UnexpectedOrchardSpend);
    }
    if pczt
        .orchard()
        .actions()
        .iter()
        .any(|action| action.spend().dummy_sk().is_some())
    {
        return Err(TxError::UnexpectedOrchardSpend);
    }
    if let Some(index) = pczt
        .ironwood()
        .actions()
        .iter()
        .position(|action| action.spend().dummy_sk().is_some())
    {
        return Err(TxError::DummyKey(index));
    }

    let mut found: Vec<SpendToSign> = Vec::new();
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
                let alpha = <pallas::Scalar as ff::PrimeField>::from_repr(
                    ff14::PrimeField::to_repr(&alpha),
                )
                .into_option()
                .ok_or(OrchardError::Custom(TxError::MissingAlpha(index)))?;
                if bool::from(ff::Field::is_zero(&alpha)) {
                    return Err(OrchardError::Custom(TxError::ZeroAlpha(index)));
                }
                if found.iter().any(|s| s.alpha == alpha) {
                    return Err(OrchardError::Custom(TxError::RepeatedAlpha(index)));
                }
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

pub fn shielded_sighash(pczt: &Pczt) -> Result<[u8; 32], TxError> {
    Ok(Signer::new(pczt.clone())
        .map_err(|e| TxError::Parse(format!("{e:?}")))?
        .shielded_sighash())
}

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
