//! Unapproved-spend alert (spec §10.4.4): every member's wallet sees the vault's spends,
//! so a spend the vault log doesn't account for means keys were used outside Zafe (old
//! shares from a removed device, or t compromised phones). Detecting it can't stop the
//! theft; it tells the group at once, while the funds that are left can still be moved.
//!
//! A spending transaction is **accounted for** when
//! - a proposal in the log has been marked broadcast with exactly this txid, or
//! - it is recent (unmined, or mined at most [`GRACE_BLOCKS`] blocks ago) and some proposal
//!   in the log spends every note it spends. That covers the moments between a leader
//!   broadcasting and the log entry saying so, and a send that crashed before logging.
//!
//! Anything else is an **unapproved spend**: no proposal at all (keys used outside Zafe),
//! or a proposal that was never logged as sent with this txid long after the transaction
//! was mined (a cancelled or rejected payment that was sent anyway, or a transaction that
//! spends a proposal's notes but pays somewhere else).

use std::collections::BTreeMap;

use crate::{vault::VaultState, wallet::VaultSpend};

/// How long after a spend appears (blocks) the log may still lack its broadcast entry
/// before it is flagged: about 30 minutes at 75 s per block.
pub const GRACE_BLOCKS: u32 = 24;

/// A transaction that spent vault notes the log doesn't account for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnapprovedSpend {
    /// Protocol byte order (as in `VaultState`); `TxId::from_bytes(..).to_string()` for display.
    pub txid: [u8; 32],
    pub mined_height: Option<u32>,
}

/// Vault spends in `spends` (what the wallet saw) that `state` (the replayed log) doesn't
/// account for. `tip` is the wallet's synced height.
pub fn unapproved_spends(
    state: &VaultState,
    spends: &[VaultSpend],
    tip: u32,
) -> Vec<UnapprovedSpend> {
    // One entry per transaction: all the notes it spends.
    let mut txs: BTreeMap<[u8; 32], (Option<u32>, Vec<[u8; 32]>)> = BTreeMap::new();
    for s in spends {
        let entry = txs.entry(s.txid).or_insert((s.mined_height, Vec::new()));
        entry.1.extend_from_slice(&s.nullifiers);
    }
    txs.into_iter()
        .filter(|(txid, (mined, nullifiers))| {
            let logged_as_sent = state
                .proposals
                .values()
                .any(|p| p.txid.as_ref() == Some(txid));
            if logged_as_sent {
                return false;
            }
            let recent = mined.is_none_or(|h| tip.saturating_sub(h) <= GRACE_BLOCKS);
            let known_notes = !nullifiers.is_empty()
                && state
                    .proposals
                    .values()
                    .any(|p| nullifiers.iter().all(|nf| p.nullifiers.contains(nf)));
            !(recent && known_notes)
        })
        .map(|(txid, (mined_height, _))| UnapprovedSpend { txid, mined_height })
        .collect()
}
