//! Unapproved-spend alert (spec §10.4.4): every member's wallet sees the vault's spends,
//! so a spend the vault log doesn't account for means keys were used outside Zafe (old
//! shares from a removed device, or t compromised phones). Detecting it can't stop the
//! theft; it tells the group at once, while the funds that are left can still be moved.
//!
//! A spending transaction is **accounted for** when its txid is
//! - logged as sent for a proposal (its broadcast entry), or
//! - the txid the PCZT of a proposal that **may be signed** fixes
//!   (`ProposalState::possibly_signed`: it reached Approved or a vote carried shares),
//!   whatever was decided after. A v6 txid commits to every action (spends, outputs,
//!   `rk`), so this is exactly the transaction members verified: one sent before its
//!   broadcast entry was logged, or one signed and then cancelled, rejected or withdrawn
//!   that someone sent anyway (rule 5).
//!
//! Anything else is an **unapproved spend**, flagged at once: no proposal at all (keys used
//! outside Zafe), a proposal nobody could have signed, or a transaction that spends a
//! proposal's notes but isn't its transaction. There is no grace window: it sheltered the
//! moments before a broadcast entry is logged, which the txid now covers whenever the
//! transaction mined, and a proposal whose PCZT doesn't parse has no txid and can't be
//! signed.

use std::collections::BTreeMap;

use crate::{
    session::ProposalId,
    vault::{ProposalStatus, VaultState},
    wallet::VaultSpend,
};

/// A transaction that spent vault notes the log doesn't account for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnapprovedSpend {
    /// Protocol byte order (as in `VaultState`); `TxId::from_bytes(..).to_string()` for display.
    pub txid: [u8; 32],
    pub mined_height: Option<u32>,
}

/// Vault spends in `spends` (what the wallet saw) that `state` (the replayed log) doesn't
/// account for, one per transaction.
pub fn unapproved_spends(state: &VaultState, spends: &[VaultSpend]) -> Vec<UnapprovedSpend> {
    // One entry per transaction (the wallet reports one row per spent note).
    let mut txs = BTreeMap::new();
    for s in spends {
        txs.entry(s.txid).or_insert(s.mined_height);
    }
    txs.into_iter()
        .filter(|(txid, _)| {
            !state.proposals.values().any(|p| {
                p.txid.as_ref() == Some(txid)
                    || (p.possibly_signed && p.expected_txid.as_ref() == Some(txid))
            })
        })
        .map(|(txid, mined_height)| UnapprovedSpend { txid, mined_height })
        .collect()
}

/// Cancelled or rejected proposals whose transaction was mined anyway (signed before it
/// was cancelled or rejected), with that txid: the app shows them as sent.
pub fn sent_anyway(state: &VaultState, spends: &[VaultSpend]) -> BTreeMap<ProposalId, [u8; 32]> {
    state
        .proposals
        .values()
        .filter(|p| {
            p.possibly_signed
                && matches!(
                    p.status,
                    ProposalStatus::Cancelled | ProposalStatus::Rejected
                )
        })
        .filter_map(|p| {
            let txid = p.expected_txid?;
            spends
                .iter()
                .any(|s| s.txid == txid && s.mined_height.is_some())
                .then_some((p.id, txid))
        })
        .collect()
}
