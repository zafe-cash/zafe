use std::collections::BTreeMap;

use crate::{
    session::ProposalId,
    vault::{ProposalStatus, VaultState},
    wallet::VaultSpend,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnapprovedSpend {
    pub txid: [u8; 32],
    pub mined_height: Option<u32>,
}

pub fn unapproved_spends(state: &VaultState, spends: &[VaultSpend]) -> Vec<UnapprovedSpend> {
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
