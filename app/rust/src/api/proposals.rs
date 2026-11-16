use std::{collections::BTreeSet, fs, path::PathBuf, time::Duration};

use rand::rngs::OsRng;
use zafe_core::{
    durable,
    node::{self, VaultMaterial},
    nonce_store::{FileNonceStore, FilePoolStore, NonceKey},
    relay_client::RelayClient,
    repair,
    session::{NonceStore, ProposalId},
    state_dir,
    vault::{ProposalStatus, ProposedPayment, VaultState},
    wallet::{
        self as zwallet, connect_pooled as connect, Client, PaymentRequest, VaultWallet,
        ZafeNetwork,
    },
};
use zafe_proto::{Chain, Identity};
use zcash_protocol::memo::{Memo, MemoBytes};

use super::{
    error::{ZafeError, ZafeErrorKind},
    names::SignerName,
    vault::{
        identity, material, network, open_wallet, runtime, sent_txs, vault_relay, wallet_lock,
    },
};
use crate::frb_generated::StreamSink;

type Result<T, E = ZafeError> = std::result::Result<T, E>;

const COLLECT_TIMEOUT: Duration = Duration::from_secs(90);

pub struct AddressCheck {
    pub valid: bool,
    pub reason: String,
}

#[flutter_rust_bridge::frb(sync)]
pub fn check_address(network_name: String, address: String) -> AddressCheck {
    let result = network(&network_name)
        .map_err(ZafeError::from)
        .and_then(|net| node::orchard_receiver(&net, address.trim()).map_err(ZafeError::from));
    match result {
        Ok(_) => AddressCheck {
            valid: true,
            reason: String::new(),
        },
        Err(_) => AddressCheck {
            valid: false,
            reason: {
                let decodes_on = |name: &str| {
                    ZafeNetwork::from_name(name).is_some_and(|net| {
                        zcash_keys::address::Address::decode(&net, address.trim()).is_some()
                    })
                };
                if decodes_on(&network_name) {
                    "Vaults can only pay shielded unified addresses".into()
                } else if ["main", "test", "regtest"]
                    .iter()
                    .any(|other| *other != network_name && decodes_on(other))
                {
                    "This address is for a different Zcash network".into()
                } else {
                    "Invalid address".into()
                }
            },
        },
    }
}

pub struct ScannedPayment {
    pub address: String,
    pub amount_zat: u64,
    pub memo: String,
    pub label: String,
    pub message: String,
}

pub struct ScannedRequest {
    pub payments: Vec<ScannedPayment>,
    pub problem: String,
}

#[flutter_rust_bridge::frb(sync)]
pub fn parse_payment_request(network_name: String, text: String) -> ScannedRequest {
    let text = text.trim();
    let fail = |problem: String| ScannedRequest {
        payments: vec![],
        problem,
    };
    let payments = if text.len() > 6 && text[..6].eq_ignore_ascii_case("zcash:") {
        match zip321::TransactionRequest::from_uri(text) {
            Ok(request) => request
                .payments()
                .values()
                .map(|p| ScannedPayment {
                    address: p.recipient_address().encode(),
                    amount_zat: p.amount().map_or(0, u64::from),
                    memo: p
                        .memo()
                        .map(|m| memo_text(m.as_slice()))
                        .unwrap_or_default(),
                    label: visible_text(p.label().map_or("", String::as_str)),
                    message: visible_text(p.message().map_or("", String::as_str)),
                })
                .collect::<Vec<_>>(),
            Err(_) => return fail("This payment link can't be read".into()),
        }
    } else {
        vec![ScannedPayment {
            address: text.to_owned(),
            amount_zat: 0,
            memo: String::new(),
            label: String::new(),
            message: String::new(),
        }]
    };
    if payments.is_empty() {
        return fail("This payment link has no recipient".into());
    }
    for p in &payments {
        let check = check_address(network_name.clone(), p.address.clone());
        if !check.valid {
            return fail(check.reason);
        }
    }
    ScannedRequest {
        payments,
        problem: String::new(),
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn parse_zec(text: String) -> Option<u64> {
    let t = text.trim().replace(',', ".");
    let (whole, frac) = t.split_once('.').unwrap_or((&t, ""));
    if (whole.is_empty() && frac.is_empty())
        || frac.len() > 8
        || !whole.chars().all(|c| c.is_ascii_digit())
        || !frac.chars().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    let whole: u64 = if whole.is_empty() {
        0
    } else {
        whole.parse().ok()?
    };
    let frac: u64 = format!("{frac:0<8}").parse().ok()?;
    whole.checked_mul(100_000_000)?.checked_add(frac)
}

#[flutter_rust_bridge::frb(sync)]
pub fn memo_length(memo: String) -> u32 {
    memo.len() as u32
}

fn memo_bytes(text: &str) -> Result<Option<MemoBytes>, ZafeError> {
    if text.is_empty() {
        return Ok(None);
    }
    Memo::from_bytes(text.as_bytes())
        .map(|m| Some(m.encode()))
        .map_err(|_| ZafeError::invalid("Message is too long"))
}

fn memo_text(bytes: &[u8]) -> String {
    MemoBytes::from_bytes(bytes)
        .ok()
        .and_then(|b| Memo::try_from(b).ok())
        .and_then(|m| match m {
            Memo::Text(t) => Some(visible_text(&t)),
            _ => None,
        })
        .unwrap_or_default()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProposalStage {
    Open,
    Approved,
    Rejected,
    Cancelled,
    Sent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MyVote {
    None,
    Approved,
    Rejected,
}

pub struct PaymentInfo {
    pub address: String,
    pub amount_zat: u64,
    pub memo: String,
}

pub struct ProposalInfo {
    pub id: String,
    pub author: String,
    pub is_mine: bool,
    pub payments: Vec<PaymentInfo>,
    pub total_zat: u64,
    pub stage: ProposalStage,
    pub approvals: Vec<String>,
    pub rejections: Vec<String>,
    pub my_vote: MyVote,
    pub threshold: u16,
    pub rejection_threshold: u16,
    pub created_at: u64,
    pub txid: Option<String>,
    pub signing_started: bool,
    pub one_tap: bool,
    pub ready: bool,
    pub completed_by_me: bool,
    pub auto_send: bool,
    pub expiry_height: u32,
    pub needs_reapproval: bool,
    pub still_sendable: bool,
    pub sent_anyway: bool,
    pub maybe_signed: bool,
    pub invalidated_by: Option<String>,
    pub needs_new_proposal: bool,
}

fn parse_id(hex_id: &str) -> Result<ProposalId, ZafeError> {
    hex::decode(hex_id)
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or_else(|| ZafeError::invalid("bad proposal id"))
}

fn leader_dir(state_dir: &str) -> PathBuf {
    PathBuf::from(state_dir).join(state_dir::LEADER)
}

fn request_file(state_dir: &str, id: &ProposalId) -> PathBuf {
    leader_dir(state_dir).join(format!("{}.{}", hex::encode(id), state_dir::REQUEST_EXT))
}

fn nonce_key(seeds: &[u8], m: &VaultMaterial) -> NonceKey {
    NonceKey::derive(seeds, &m.descriptor.vault_id)
}

fn pool_store(state_dir: &str, seeds: &[u8], m: &VaultMaterial) -> FilePoolStore {
    FilePoolStore::new(
        PathBuf::from(state_dir).join(state_dir::POOL),
        nonce_key(seeds, m),
    )
}

fn nonce_store(state_dir: &str, seeds: &[u8], m: &VaultMaterial) -> FileNonceStore {
    FileNonceStore::new(
        PathBuf::from(state_dir).join(state_dir::NONCES),
        nonce_key(seeds, m),
    )
}

fn info(
    state: &VaultState,
    me: [u8; 32],
    state_dir: &str,
    seeds: &[u8],
    m: &VaultMaterial,
) -> Vec<ProposalInfo> {
    let d = &state.descriptor;
    let nonces = nonce_store(state_dir, seeds, m);
    let pool = pool_store(state_dir, seeds, m);
    let view = super::vault::synced_view(&m.descriptor.vault_id);
    let anyway = view
        .as_ref()
        .map(|v| zafe_core::spend_watch::sent_anyway(state, &v.spends))
        .unwrap_or_default();
    let sendable: Vec<ProposalId> = match &view {
        Some(view) => node::still_sendable(state, view)
            .iter()
            .map(|p| p.id)
            .collect(),
        None => state
            .proposals
            .values()
            .filter(|p| {
                p.possibly_signed
                    && matches!(
                        p.status,
                        ProposalStatus::Cancelled | ProposalStatus::Rejected
                    )
            })
            .map(|p| p.id)
            .collect(),
    };
    let mut out: Vec<_> = state
        .proposals
        .values()
        .map(|p| ProposalInfo {
            id: hex::encode(p.id),
            author: hex::encode(p.author),
            is_mine: p.author == me,
            payments: p
                .payments
                .iter()
                .map(|x: &ProposedPayment| PaymentInfo {
                    address: x.address.clone(),
                    amount_zat: x.amount_zat,
                    memo: memo_text(&x.memo),
                })
                .collect(),
            total_zat: p.payments.iter().map(|x| x.amount_zat).sum(),
            stage: match p.status {
                _ if anyway.contains_key(&p.id) => ProposalStage::Sent,
                ProposalStatus::Open => ProposalStage::Open,
                ProposalStatus::Approved => ProposalStage::Approved,
                ProposalStatus::Rejected => ProposalStage::Rejected,
                ProposalStatus::Cancelled => ProposalStage::Cancelled,
                ProposalStatus::Broadcast => ProposalStage::Sent,
            },
            approvals: p.approvals.keys().map(hex::encode).collect(),
            rejections: p.rejections.keys().map(hex::encode).collect(),
            my_vote: if p.approvals.contains_key(&me) {
                MyVote::Approved
            } else if p.rejections.contains_key(&me) {
                MyVote::Rejected
            } else {
                MyVote::None
            },
            threshold: d.threshold,
            rejection_threshold: d.rejection_threshold() as u16,
            created_at: p.created_at,
            sent_anyway: anyway.contains_key(&p.id),
            txid: p.txid.or_else(|| anyway.get(&p.id).copied()).map(|t| {
                let mut t = t;
                t.reverse();
                hex::encode(t)
            }),
            signing_started: request_file(state_dir, &p.id).exists(),
            one_tap: p.preprocessed.is_some(),
            ready: p.ready_group.is_some() && p.status == ProposalStatus::Approved,
            completed_by_me: p.completed_by == Some(me),
            auto_send: p.auto_send,
            expiry_height: p.expiry_height,
            still_sendable: sendable.contains(&p.id) && !anyway.contains_key(&p.id),
            maybe_signed: p.possibly_signed,
            invalidated_by: (matches!(
                p.status,
                ProposalStatus::Cancelled | ProposalStatus::Rejected
            ) && sendable.contains(&p.id))
            .then(|| {
                state.proposals.values().find(|q| {
                    matches!(
                        q.status,
                        ProposalStatus::Open | ProposalStatus::Approved | ProposalStatus::Broadcast
                    ) && q.payments.is_empty()
                        && q.nullifiers.iter().any(|nf| p.nullifiers.contains(nf))
                })
            })
            .flatten()
            .map(|q| hex::encode(q.id)),
            needs_reapproval: matches!(p.status, ProposalStatus::Open | ProposalStatus::Approved)
                && match p.approvals.get(&me) {
                    Some(c) if !c.is_empty() => !nonces.contains(&p.id, &p.pczt_hash),
                    Some(_) => p.shares.contains_key(&me) && p.one_tap_blocked(),
                    None => false,
                },
            needs_new_proposal: node::one_tap_stuck(state, &p.id, &me, &pool),
        })
        .collect();
    out.sort_by_key(|p| {
        std::cmp::Reverse(state.proposals[&parse_id(&p.id).expect("own id")].log_index)
    });
    out
}

pub struct ProposalList {
    pub items: Vec<ProposalInfo>,
    pub newer_version_entries: u32,
    pub shared_names: Vec<SignerName>,
    pub backed_up: Vec<String>,
    pub seat_finishing: bool,
    pub seat_moves: Vec<SeatMove>,
    pub pending_seat_moves: Vec<PendingSeatMove>,
    pub seat_keeps: Vec<SeatKeep>,
    pub repairs: Vec<RepairInfo>,
    pub moves: Vec<RelayMoveRecord>,
    pub updated_material: Option<Vec<u8>>,
}

pub struct RepairInfo {
    pub replacement: u64,
    pub pending: bool,
    pub new_key_hex: String,
    pub helpers: Vec<String>,
    pub attempt: u32,
    pub restarts: Vec<RepairRestart>,
}

pub struct RepairRestart {
    pub helpers: Vec<String>,
    pub asked: Vec<String>,
}

pub struct SeatMove {
    pub old_key_hex: String,
    pub new_key_hex: String,
    pub approvals: Vec<String>,
    pub needed: u32,
    pub code: String,
    pub kept_at: Option<u64>,
}

pub struct SeatKeep {
    pub old_key_hex: String,
    pub at: u64,
}

pub struct PendingSeatMove {
    pub replacement: u64,
    pub old_key_hex: String,
    pub new_key_hex: String,
    pub can_keep: bool,
    pub kept_at: Option<u64>,
    pub helpers: Vec<String>,
    pub helped: Vec<String>,
    pub seen_at: Option<u64>,
    pub completes_at: Option<u64>,
    pub stopped: Vec<String>,
}

#[flutter_rust_bridge::frb(sync)]
pub fn nu7_activation_height(network: String) -> Option<u32> {
    use zcash_protocol::consensus::{NetworkUpgrade, Parameters};
    let net = super::vault::network(&network).ok()?;
    net.activation_height(NetworkUpgrade::Nu7).map(u32::from)
}

pub fn mark_pool_reset(state_dir: String) -> Result<(), ZafeError> {
    fs::create_dir_all(&state_dir).map_err(anyhow::Error::from)?;
    fs::write(PathBuf::from(&state_dir).join(state_dir::POOL_RESET), b"1")
        .map_err(anyhow::Error::from)?;
    Ok(())
}

pub fn list_proposals(
    relay_url: String,
    state_dir: String,
    seeds: Vec<u8>,
    material: Vec<u8>,
    tip_height: Option<u32>,
) -> Result<ProposalList, ZafeError> {
    let me = identity(&seeds)?;
    let m = self::material(&material)?;
    let relay = vault_relay(&relay_url, &m);
    let mut pool = pool_store(&state_dir, &seeds, &m);
    let reset_marker = PathBuf::from(&state_dir).join(state_dir::POOL_RESET);
    let reset = reset_marker.exists();
    let mut watch = crate::diag::Stopwatch::new("proposals");
    let state = runtime().block_on(async {
        let notes = super::vault::note_count(&m.descriptor.vault_id);
        let (mut chain, mut state) = node::load_state(&relay, &me, &m).await?;
        watch.lap("log");
        if repair::seat_finishing(&relay, &me, &m, &state).await {
            return Ok::<_, ZafeError>((state, true));
        }
        match node::top_up_pool_loaded(
            &relay, &me, &m, &mut pool, reset, notes, &mut chain, &mut state, &mut OsRng,
        )
        .await
        {
            Ok(_) => {
                if reset {
                    let _ = std::fs::remove_file(&reset_marker);
                }
            }
            Err(
                node::NodeError::VaultMoved { .. }
                | node::NodeError::MoveUnconfirmed { .. }
                | node::NodeError::MoveConflict,
            ) => {}
            Err(e) => return Err(e.into()),
        }
        watch.lap("pool");
        if let Err(e) =
            node::announce_version(&relay, &me, &m, &mut chain, &mut state, &mut OsRng).await
        {
            eprintln!("announce version: {e}");
        }
        if node::move_state(&relay, &m).is_some_and(|ms| ms.arrived && !ms.switched) {
            if let Err(e) = Box::pin(node::move_progress(&relay, &me, &m, &mut OsRng)).await {
                eprintln!("relay move: {e}");
            }
        }
        let repair_dir = PathBuf::from(&state_dir).join(state_dir::REPAIR);
        if let Err(e) = repair::help_repairs(
            &relay,
            &me,
            &m,
            &chain,
            &state,
            &repair_dir,
            crate::testing::seat_move_wait(),
            &mut OsRng,
        )
        .await
        {
            eprintln!("share repair: {e}");
        }
        if state
            .replacement_to(&me.public().sig_pk)
            .is_some_and(|r| !r.done)
        {
            if let Err(e) = repair::mark_repair_done(&relay, &me, &m, &mut OsRng).await {
                eprintln!("repair done: {e}");
            }
            let (_, fresh) = node::load_state(&relay, &me, &m).await?;
            return Ok::<_, ZafeError>((fresh, false));
        }
        Ok::<_, ZafeError>((state, false))
    });
    watch.finish();
    let (state, seat_finishing) = state?;
    node::forget_closed(&state, &me.public().sig_pk, tip_height, &mut pool);
    node::forget_closed_nonces(&state, tip_height, &mut nonce_store(&state_dir, &seeds, &m));
    Ok(ProposalList {
        items: info(&state, me.public().sig_pk, &state_dir, &seeds, &m),
        newer_version_entries: state.newer_version_entries() as u32,
        shared_names: shared_names(&state),
        backed_up: backed_up(&state),
        seat_finishing,
        seat_moves: seat_moves(&state),
        seat_keeps: state
            .seat_keeps
            .iter()
            .filter_map(|(old, keeps)| {
                Some(SeatKeep {
                    old_key_hex: hex::encode(old),
                    at: keeps.last()?.at,
                })
            })
            .collect(),
        pending_seat_moves: pending_seat_moves(
            &state,
            &me,
            &PathBuf::from(&state_dir).join(state_dir::REPAIR),
        ),
        repairs: repairs(&state),
        moves: state
            .moves
            .iter()
            .map(|mv| RelayMoveRecord {
                relay_url: mv.target.relay.clone(),
                host: mv.target.host(),
                at: mv.at,
                log_index: mv.index,
            })
            .collect(),
        updated_material: updated_material(&material, &m, &state)?,
    })
}

pub struct RelayMoveRecord {
    pub relay_url: String,
    pub host: String,
    pub at: u64,
    pub log_index: u64,
}

fn updated_material(
    stored: &[u8],
    m: &VaultMaterial,
    state: &VaultState,
) -> Result<Option<Vec<u8>>, ZafeError> {
    let other = |e: zafe_core::node::NodeError| ZafeError::new(ZafeErrorKind::Other, e.to_string());
    let current = repair::current_material(m, state);
    let relay_changed = VaultMaterial::from_bytes(stored).is_ok_and(|s| s.relay != m.relay);
    match current {
        Some(c) => Ok(Some(c.to_bytes().map_err(other)?)),
        None if relay_changed => Ok(Some(m.to_bytes().map_err(other)?)),
        None => Ok(None),
    }
}

fn repairs(state: &VaultState) -> Vec<RepairInfo> {
    state
        .replacements
        .iter()
        .filter(|r| !r.done)
        .map(|r| RepairInfo {
            replacement: r.index,
            pending: r.is_pending(),
            new_key_hex: hex::encode(r.new.sig_pk),
            helpers: r.helpers.iter().map(hex::encode).collect(),
            attempt: r.attempt,
            restarts: r
                .retries
                .iter()
                .map(|(helpers, asked)| RepairRestart {
                    helpers: helpers.iter().map(hex::encode).collect(),
                    asked: asked.iter().map(hex::encode).collect(),
                })
                .collect(),
        })
        .collect()
}

pub fn retry_repair(
    relay_url: String,
    seeds: Vec<u8>,
    material: Vec<u8>,
    replacement: u64,
    stalled_key_hex: String,
    taker_key_hex: String,
) -> Result<(), ZafeError> {
    let me = identity(&seeds)?;
    let m = self::material(&material)?;
    let key = |hex_key: &str| -> Result<[u8; 32], ZafeError> {
        hex::decode(hex_key.trim())
            .ok()
            .and_then(|b| b.try_into().ok())
            .ok_or_else(|| ZafeError::invalid("not a signer key"))
    };
    runtime().block_on(repair::retry_repair(
        &vault_relay(&relay_url, &m),
        &me,
        &m,
        replacement,
        key(&stalled_key_hex)?,
        key(&taker_key_hex)?,
        &mut OsRng,
    ))?;
    Ok(())
}

fn kept_at(state: &VaultState, old: &[u8; 32]) -> Option<u64> {
    state.seat_keeps.get(old)?.last().map(|k| k.at)
}

fn seat_moves(state: &VaultState) -> Vec<SeatMove> {
    state
        .pending_replacements
        .values()
        .map(|p| {
            let request = repair::RecoveryRequest { identity: p.new };
            SeatMove {
                old_key_hex: hex::encode(p.old),
                new_key_hex: hex::encode(p.new.sig_pk),
                approvals: p.approvals.iter().map(|(pk, _)| hex::encode(pk)).collect(),
                needed: u32::from(state.descriptor.threshold),
                code: request.encode(),
                kept_at: kept_at(state, &p.old),
            }
        })
        .collect()
}

fn pending_seat_moves(
    state: &VaultState,
    me: &Identity,
    repair_dir: &std::path::Path,
) -> Vec<PendingSeatMove> {
    let vault_id = state.descriptor.vault_id;
    let wait = crate::testing::seat_move_wait().as_secs();
    state
        .replacements
        .iter()
        .filter(|r| r.is_pending())
        .map(|r| {
            let seen_at = r
                .move_id(&vault_id)
                .and_then(|id| repair::seat_move_seen(repair_dir, me, &vault_id, &id));
            PendingSeatMove {
                replacement: r.index,
                old_key_hex: hex::encode(r.old),
                new_key_hex: hex::encode(r.new.sig_pk),
                can_keep: r.round == Some(0),
                kept_at: kept_at(state, &r.old),
                helpers: r.helpers.iter().map(hex::encode).collect(),
                helped: r.helped.iter().map(hex::encode).collect(),
                seen_at,
                completes_at: seen_at.map(|at| at.saturating_add(wait)),
                stopped: r.stopped.iter().map(hex::encode).collect(),
            }
        })
        .collect()
}

fn backed_up(state: &VaultState) -> Vec<String> {
    state
        .backups
        .iter()
        .filter(|(_, b)| b.epoch == state.descriptor.epoch)
        .map(|(pk, _)| hex::encode(pk))
        .collect()
}

fn shared_names(state: &VaultState) -> Vec<SignerName> {
    let folded = |name: &str| name.trim().to_lowercase();
    let mut seen = std::collections::BTreeMap::<String, usize>::new();
    for name in state.names.values() {
        *seen.entry(folded(name)).or_default() += 1;
    }
    state
        .names
        .iter()
        .filter(|(_, name)| plain_name(name) && seen[&folded(name)] == 1)
        .map(|(pk, name)| SignerName {
            key_hex: hex::encode(pk),
            name: name.clone(),
        })
        .collect()
}

fn hidden_char(c: char) -> bool {
    matches!(c,
        '\u{00AD}' | '\u{061C}' | '\u{180E}' | '\u{200B}'..='\u{200F}'
        | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{206F}' | '\u{FE00}'..='\u{FE0F}'
        | '\u{FEFF}' | '\u{E0000}'..='\u{E007F}')
}

pub(crate) fn visible_text(text: &str) -> String {
    text.chars()
        .map(|c| if hidden_char(c) { '\u{FFFD}' } else { c })
        .collect()
}

fn plain_name(name: &str) -> bool {
    let lower = name.trim().to_lowercase();
    let you = lower == "you" || lower.starts_with("you ") || lower.starts_with("you(");
    !you && !name.contains("  ")
        && name.chars().all(|c| {
            c.is_alphabetic() || c.is_numeric() || matches!(c, ' ' | '-' | '\'' | '\u{2019}' | '&')
        })
}

#[cfg(test)]
mod name_tests {
    use super::{plain_name, visible_text};

    #[test]
    fn names_that_could_pass_for_someone_else_are_not_plain() {
        assert!(plain_name("Alice"));
        assert!(plain_name("Younes"));
        assert!(!plain_name("You"));
        assert!(!plain_name(" you (CFO)"));
        assert!(!plain_name("Al\u{202E}ecila"));
        assert_eq!(visible_text("pay \u{202E}01 ZEC"), "pay \u{FFFD}01 ZEC");
        assert_eq!(visible_text("plain memo"), "plain memo");
        assert!(!plain_name("Ali\u{200B}ce"));
        assert!(!plain_name("Alice (a1a1a1...a1a1)"));
        assert!(!plain_name("Alice a1a1a1\u{2026}a1a1"));
        assert!(!plain_name("Alice\u{00A0}\u{00A0}\u{00A0}"));
        assert!(!plain_name("Alice    x"));
        assert!(plain_name("Anne-Marie O\u{2019}Neil & Co 2"));
        assert!(plain_name("\u{0905}\u{0928}\u{092E}\u{094B}\u{0932}"));
        assert!(plain_name("José"));
    }
}

pub struct PaymentInput {
    pub address: String,
    pub amount_zat: u64,
    pub memo: String,
}

fn payment_requests(
    m: &VaultMaterial,
    payments: &[PaymentInput],
) -> Result<Vec<PaymentRequest>, ZafeError> {
    let net = network(&m.descriptor.network)?;
    if payments.is_empty() {
        return Err(ZafeError::invalid("Add at least one payment"));
    }
    payments
        .iter()
        .map(|p| {
            node::orchard_receiver(&net, p.address.trim())?;
            if p.amount_zat == 0 {
                return Err(ZafeError::invalid("Amount must be more than zero"));
            }
            Ok(PaymentRequest {
                address: p.address.trim().to_string(),
                amount_zat: p.amount_zat,
                memo: memo_bytes(&p.memo)?,
            })
        })
        .collect()
}

pub fn estimate_fee(
    lightwalletd_url: String,
    db_dir: String,
    db_key: Vec<u8>,
    material: Vec<u8>,
    payments: Vec<PaymentInput>,
) -> Result<u64, ZafeError> {
    let m = self::material(&material)?;
    let requests = payment_requests(&m, &payments)?;
    let mut watch = crate::diag::Stopwatch::new("fee");
    let _guard = wallet_lock();
    watch.lap("lock");
    let fee = runtime().block_on(async {
        let mut wallet = open_wallet(&db_dir, &db_key, &lightwalletd_url, &m).await?;
        watch.lap("open");
        Ok(wallet.fee_for(&requests)?)
    });
    watch.lap("build");
    watch.finish();
    fee
}

pub fn propose_payment(
    relay_url: String,
    lightwalletd_url: String,
    db_dir: String,
    db_key: Vec<u8>,
    seeds: Vec<u8>,
    material: Vec<u8>,
    payments: Vec<PaymentInput>,
    auto_send: bool,
) -> Result<String, ZafeError> {
    let me = identity(&seeds)?;
    let m = self::material(&material)?;
    let requests = payment_requests(&m, &payments)?;
    let mut watch = crate::diag::Stopwatch::new("propose");
    let relay = vault_relay(&relay_url, &m);
    let proposed = propose_inner(
        &mut watch,
        &relay,
        &me,
        &m,
        &lightwalletd_url,
        &db_dir,
        &db_key,
        &requests,
        auto_send,
    );
    watch.finish();
    Ok(hex::encode(proposed?.id))
}

pub struct ProposeOutcome {
    pub id: String,
    pub approved: Option<ApproveResult>,
    pub approve_error: Option<ZafeError>,
    pub items: Vec<ProposalInfo>,
}

pub fn propose_and_approve(
    relay_url: String,
    lightwalletd_url: String,
    db_dir: String,
    db_key: Vec<u8>,
    state_dir: String,
    seeds: Vec<u8>,
    material: Vec<u8>,
    payments: Vec<PaymentInput>,
    auto_send: bool,
) -> Result<ProposeOutcome, ZafeError> {
    let me = identity(&seeds)?;
    let m = self::material(&material)?;
    let net = network(&m.descriptor.network)?;
    let requests = payment_requests(&m, &payments)?;
    let mut watch = crate::diag::Stopwatch::new("propose-approve");
    let relay = vault_relay(&relay_url, &m);
    let proposed = propose_inner(
        &mut watch,
        &relay,
        &me,
        &m,
        &lightwalletd_url,
        &db_dir,
        &db_key,
        &requests,
        auto_send,
    );
    let mut p = match proposed {
        Ok(p) => p,
        Err(e) => {
            watch.finish();
            return Err(e);
        }
    };
    let mut store = nonce_store(&state_dir, &seeds, &m);
    let mut pool = pool_store(&state_dir, &seeds, &m);
    let approved = runtime().block_on(node::approve_after_propose(
        &relay,
        &me,
        &m,
        &net,
        &p.view,
        p.id,
        &mut store,
        &mut pool,
        &mut OsRng,
        &mut p.chain,
        &mut p.state,
    ));
    watch.lap("approve");
    watch.finish();
    let (approved, approve_error) = match approved {
        Ok(a) => (
            Some(ApproveResult {
                signed: a.signed,
                completed: a.completed,
                auto_send: a.auto_send,
            }),
            None,
        ),
        Err(e) => (None, Some(e.into())),
    };
    Ok(ProposeOutcome {
        id: hex::encode(p.id),
        approved,
        approve_error,
        items: info(&p.state, me.public().sig_pk, &state_dir, &seeds, &m),
    })
}

struct Proposed {
    id: ProposalId,
    chain: Chain,
    state: VaultState,
    view: node::ChainView,
}

fn propose_inner(
    watch: &mut crate::diag::Stopwatch,
    relay: &RelayClient,
    me: &Identity,
    m: &VaultMaterial,
    lightwalletd_url: &str,
    db_dir: &str,
    db_key: &[u8],
    requests: &[PaymentRequest],
    auto_send: bool,
) -> Result<Proposed, ZafeError> {
    let params = network(&m.descriptor.network)?;
    node::check_payees(m, &params, requests)?;
    let (server, loaded) = runtime().block_on(async {
        let server = async {
            let mut client = connect(lightwalletd_url).await?;
            let tip = zwallet::tip_on_current_branch(&mut client, &params).await?;
            Ok::<_, ZafeError>((client, tip))
        };
        tokio::join!(server, node::load_state(relay, me, m))
    });
    watch.lap("network");
    let (mut client, server_tip) = server?;
    let (mut chain, mut state) = loaded?;
    let _guard = wallet_lock();
    watch.lap("lock");
    runtime().block_on(async {
        let mut wallet = open_wallet(db_dir, db_key, lightwalletd_url, m).await?;
        watch.lap("open");
        let scanned = catch_up_to(&mut wallet, &mut client, server_tip, true).await?;
        watch.lap(if scanned { "scan" } else { "noscan" });
        let sent = sent_txs(db_dir, m);
        let first = node::propose_loaded(
            relay,
            me,
            m,
            &mut wallet,
            &mut client,
            &sent,
            requests,
            auto_send,
            &mut chain,
            &mut state,
            &mut OsRng,
        )
        .await;
        let id = match first {
            Err(node::NodeError::Wallet(_)) if !scanned => {
                wallet.sync(&mut client).await?;
                watch.lap("rescan");
                node::propose_loaded(
                    relay,
                    me,
                    m,
                    &mut wallet,
                    &mut client,
                    &sent,
                    requests,
                    auto_send,
                    &mut chain,
                    &mut state,
                    &mut OsRng,
                )
                .await?
            }
            other => other?,
        };
        watch.lap("build");
        let view = node::ChainView::of(&wallet)?
            .with_mempool(&state, &mut client)
            .await?;
        Ok(Proposed {
            id,
            chain,
            state,
            view,
        })
    })
}

async fn catch_up(
    wallet: &mut VaultWallet<ZafeNetwork>,
    client: &mut Client,
    lenient: bool,
) -> Result<bool, ZafeError> {
    let tip = zwallet::latest_height(client).await?;
    catch_up_to(wallet, client, tip, lenient).await
}

async fn catch_up_to(
    wallet: &mut VaultWallet<ZafeNetwork>,
    client: &mut Client,
    tip: u32,
    lenient: bool,
) -> Result<bool, ZafeError> {
    let skip = if lenient {
        wallet.can_build_without_scan(tip)?
    } else {
        !wallet.needs_scan(tip)?
    };
    if skip {
        return Ok(false);
    }
    wallet.sync(client).await?;
    Ok(true)
}

pub fn propose_again(
    relay_url: String,
    lightwalletd_url: String,
    db_dir: String,
    db_key: Vec<u8>,
    seeds: Vec<u8>,
    material: Vec<u8>,
    proposal_id: String,
) -> Result<String, ZafeError> {
    let me = identity(&seeds)?;
    let m = self::material(&material)?;
    let target = parse_id(&proposal_id)?;
    let mut watch = crate::diag::Stopwatch::new("propose-again");
    let _guard = wallet_lock();
    watch.lap("lock");
    let id = runtime().block_on(async {
        let mut wallet = open_wallet(&db_dir, &db_key, &lightwalletd_url, &m).await?;
        watch.lap("open");
        let mut client = connect(&lightwalletd_url).await?;
        let scanned = catch_up(&mut wallet, &mut client, false).await?;
        watch.lap(if scanned { "scan" } else { "noscan" });
        Ok::<_, ZafeError>(
            node::propose_again(
                &vault_relay(&relay_url, &m),
                &me,
                &m,
                &mut wallet,
                &mut client,
                &sent_txs(&db_dir, &m),
                target,
                &mut OsRng,
            )
            .await?,
        )
    });
    watch.lap("build");
    watch.finish();
    Ok(hex::encode(id?))
}

pub fn invalidate_proposal(
    relay_url: String,
    lightwalletd_url: String,
    db_dir: String,
    db_key: Vec<u8>,
    seeds: Vec<u8>,
    material: Vec<u8>,
    proposal_id: String,
) -> Result<String, ZafeError> {
    let me = identity(&seeds)?;
    let m = self::material(&material)?;
    let target = parse_id(&proposal_id)?;
    let mut watch = crate::diag::Stopwatch::new("sweep");
    let _guard = wallet_lock();
    watch.lap("lock");
    let id = runtime().block_on(async {
        let mut wallet = open_wallet(&db_dir, &db_key, &lightwalletd_url, &m).await?;
        watch.lap("open");
        let mut client = connect(&lightwalletd_url).await?;
        let scanned = catch_up(&mut wallet, &mut client, false).await?;
        watch.lap(if scanned { "scan" } else { "noscan" });
        Ok::<_, ZafeError>(
            node::invalidate(
                &vault_relay(&relay_url, &m),
                &me,
                &m,
                &mut wallet,
                &mut client,
                &sent_txs(&db_dir, &m),
                target,
                &mut OsRng,
            )
            .await?,
        )
    });
    watch.lap("build");
    watch.finish();
    Ok(hex::encode(id?))
}

pub struct ReviewInfo {
    pub verified: bool,
    pub problem: String,
    pub fee_zat: u64,
    pub change_zat: u64,
    pub input_zat: u64,
    pub spends: u32,
    pub expiry_height: u32,
    pub tip_height: u32,
}

fn local_tip(
    db_dir: &str,
    db_key: &[u8],
    lightwalletd_url: &str,
    m: &VaultMaterial,
) -> Result<u32, ZafeError> {
    if let Some(tip) = super::vault::synced_tip(&m.descriptor.vault_id) {
        return Ok(tip);
    }
    let _guard = wallet_lock();
    let wallet = runtime().block_on(open_wallet(db_dir, db_key, lightwalletd_url, m))?;
    wallet
        .chain_height()?
        .ok_or_else(|| ZafeError::new(ZafeErrorKind::NotReady, "The vault has not synced yet"))
}

fn chain_view(
    db_dir: &str,
    db_key: &[u8],
    lightwalletd_url: &str,
    m: &VaultMaterial,
    state: Option<&VaultState>,
) -> Result<node::ChainView, ZafeError> {
    if let Some(view) = super::vault::synced_view(&m.descriptor.vault_id) {
        return Ok(view);
    }
    let _guard = wallet_lock();
    runtime().block_on(async {
        let wallet = open_wallet(db_dir, db_key, lightwalletd_url, m).await?;
        let view = node::ChainView::of(&wallet)?;
        Ok(match state {
            Some(state) => {
                view.with_mempool(state, &mut connect(lightwalletd_url).await?)
                    .await?
            }
            None => view,
        })
    })
}

pub fn review_proposal(
    relay_url: String,
    lightwalletd_url: String,
    db_dir: String,
    db_key: Vec<u8>,
    seeds: Vec<u8>,
    material: Vec<u8>,
    proposal_id: String,
) -> Result<ReviewInfo, ZafeError> {
    let me = identity(&seeds)?;
    let m = self::material(&material)?;
    let net = network(&m.descriptor.network)?;
    let id = parse_id(&proposal_id)?;
    let mut watch = crate::diag::Stopwatch::new("review");
    let (_, state) = runtime().block_on(node::load_state(&vault_relay(&relay_url, &m), &me, &m))?;
    watch.lap("log");
    let view = chain_view(&db_dir, &db_key, &lightwalletd_url, &m, Some(&state))?;
    let tip = view.tip;
    watch.lap("tip");
    let reviewed = node::review(&state, &m, &net, &view, id);
    watch.lap("check");
    watch.finish();
    match reviewed {
        Ok(v) => Ok(ReviewInfo {
            verified: true,
            problem: String::new(),
            fee_zat: v.fee_zat,
            change_zat: v.change_total_zat,
            input_zat: v.input_total_zat,
            spends: v.spends_to_sign.len() as u32,
            expiry_height: v.expiry_height,
            tip_height: tip,
        }),
        Err(node::NodeError::Verification(problem)) => Ok(ReviewInfo {
            verified: false,
            problem,
            fee_zat: 0,
            change_zat: 0,
            input_zat: 0,
            spends: 0,
            expiry_height: 0,
            tip_height: tip,
        }),
        Err(e) => Err(e.into()),
    }
}

pub struct ApproveResult {
    pub signed: bool,
    pub completed: bool,
    pub auto_send: bool,
}

pub fn approve_proposal(
    relay_url: String,
    lightwalletd_url: String,
    db_dir: String,
    db_key: Vec<u8>,
    state_dir: String,
    seeds: Vec<u8>,
    material: Vec<u8>,
    proposal_id: String,
) -> Result<ApproveResult, ZafeError> {
    let me = identity(&seeds)?;
    let m = self::material(&material)?;
    let net = network(&m.descriptor.network)?;
    let id = parse_id(&proposal_id)?;
    let mut watch = crate::diag::Stopwatch::new("approve");
    let view = chain_view(&db_dir, &db_key, &lightwalletd_url, &m, None)?;
    watch.lap("tip");
    let mut store = nonce_store(&state_dir, &seeds, &m);
    let mut pool = pool_store(&state_dir, &seeds, &m);
    let approved = runtime().block_on(node::approve(
        &vault_relay(&relay_url, &m),
        &me,
        &m,
        &net,
        &view,
        id,
        &mut store,
        &mut pool,
        &mut OsRng,
    ))?;
    watch.steps(&approved.timings);
    watch.finish();
    Ok(ApproveResult {
        signed: approved.signed,
        completed: approved.completed,
        auto_send: approved.auto_send,
    })
}

pub fn reject_proposal(
    relay_url: String,
    state_dir: String,
    seeds: Vec<u8>,
    material: Vec<u8>,
    proposal_id: String,
) -> Result<(), ZafeError> {
    let me = identity(&seeds)?;
    let m = self::material(&material)?;
    let id = parse_id(&proposal_id)?;
    let mut watch = crate::diag::Stopwatch::new("reject");
    let result = runtime().block_on(node::reject(
        &vault_relay(&relay_url, &m),
        &me,
        &m,
        id,
        &mut nonce_store(&state_dir, &seeds, &m),
        &pool_store(&state_dir, &seeds, &m),
        &mut OsRng,
    ));
    watch.lap("relay");
    watch.finish();
    result?;
    Ok(())
}

pub fn set_my_name(
    relay_url: String,
    seeds: Vec<u8>,
    material: Vec<u8>,
    name: String,
) -> Result<(), ZafeError> {
    let me = identity(&seeds)?;
    let m = self::material(&material)?;
    runtime().block_on(node::set_name(
        &vault_relay(&relay_url, &m),
        &me,
        &m,
        &name,
        &mut OsRng,
    ))?;
    Ok(())
}

pub fn attest_backup(
    relay_url: String,
    seeds: Vec<u8>,
    material: Vec<u8>,
) -> Result<(), ZafeError> {
    let me = identity(&seeds)?;
    let m = self::material(&material)?;
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    runtime().block_on(node::attest_backup(
        &vault_relay(&relay_url, &m),
        &me,
        &m,
        at,
        &mut OsRng,
    ))?;
    Ok(())
}

pub fn cancel_proposal(
    relay_url: String,
    seeds: Vec<u8>,
    material: Vec<u8>,
    proposal_id: String,
) -> Result<(), ZafeError> {
    let me = identity(&seeds)?;
    let m = self::material(&material)?;
    let id = parse_id(&proposal_id)?;
    let mut watch = crate::diag::Stopwatch::new("cancel");
    let result = runtime().block_on(node::cancel(
        &vault_relay(&relay_url, &m),
        &me,
        &m,
        id,
        &mut OsRng,
    ));
    watch.lap("relay");
    watch.finish();
    result?;
    Ok(())
}

#[flutter_rust_bridge::frb(sync)]
pub fn restart_signing(state_dir: String, proposal_id: String) -> Result<(), ZafeError> {
    let id = parse_id(&proposal_id)?;
    for ext in [state_dir::REQUEST_EXT, state_dir::OWN_SHARES_EXT] {
        let path = leader_dir(&state_dir).join(format!("{}.{ext}", hex::encode(id)));
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(ZafeError::new(ZafeErrorKind::Other, e.to_string())),
        }
    }
    Ok(())
}

fn store_own_send(db_dir: &str, db_key: &[u8], m: &VaultMaterial, tip: u32, raw: &[u8]) {
    let (Ok(net), Ok(key)) = (
        network(&m.descriptor.network),
        super::vault::wallet_key(db_key),
    ) else {
        return;
    };
    let Some(tx) = zafe_core::mempool::parse_mempool_tx(&net, tip, raw) else {
        return;
    };
    let _guard = wallet_lock();
    if let Ok(mut wallet) = VaultWallet::open(&super::vault::wallet_path(db_dir, m), &key, net) {
        if let Err(e) = wallet.store_mempool_tx(&tx) {
            eprintln!("storing the sent payment: {e}");
        }
    }
}

pub fn prewarm_prover() {
    node::proving_key();
    node::verifying_key();
}

pub fn answer_signing_requests(
    relay_url: String,
    lightwalletd_url: String,
    db_dir: String,
    db_key: Vec<u8>,
    state_dir: String,
    seeds: Vec<u8>,
    material: Vec<u8>,
    tip_height: Option<u32>,
) -> Result<u32, ZafeError> {
    let me = identity(&seeds)?;
    let m = self::material(&material)?;
    let net = network(&m.descriptor.network)?;
    let mut store = nonce_store(&state_dir, &seeds, &m);
    let mut watch = crate::diag::Stopwatch::new("respond");
    let tip = match tip_height {
        Some(tip) => tip,
        None => local_tip(&db_dir, &db_key, &lightwalletd_url, &m)?,
    };
    watch.lap("tip");
    let report = runtime().block_on(node::respond(
        &vault_relay(&relay_url, &m),
        &me,
        &m,
        &net,
        tip,
        &mut store,
        &mut OsRng,
    ));
    watch.lap("relay");
    watch.finish();
    Ok(report?.answered.len() as u32)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendStage {
    Collecting,
    Sent,
    Failed,
}

pub struct SendProgress {
    pub stage: SendStage,
    pub received: u32,
    pub needed: u32,
    pub txid: Option<String>,
    pub error: Option<ZafeError>,
}

pub fn send_proposal(
    relay_url: String,
    lightwalletd_url: String,
    db_dir: String,
    db_key: Vec<u8>,
    state_dir: String,
    seeds: Vec<u8>,
    material: Vec<u8>,
    proposal_id: String,
    sink: StreamSink<SendProgress>,
) -> Result<(), ZafeError> {
    let result = send_with_progress(
        relay_url,
        lightwalletd_url,
        db_dir,
        db_key,
        state_dir,
        seeds,
        material,
        proposal_id,
        COLLECT_TIMEOUT,
        |p| {
            let _ = sink.add(p);
        },
    );
    if let Err(e) = result {
        let _ = sink.add(SendProgress {
            stage: SendStage::Failed,
            received: 0,
            needed: 0,
            txid: None,
            error: Some(e),
        });
    }
    Ok(())
}

#[flutter_rust_bridge::frb(ignore)]
pub fn send_with_progress(
    relay_url: String,
    lightwalletd_url: String,
    db_dir: String,
    db_key: Vec<u8>,
    state_dir: String,
    seeds: Vec<u8>,
    material: Vec<u8>,
    proposal_id: String,
    collect_timeout: Duration,
    mut on_progress: impl FnMut(SendProgress),
) -> Result<(), ZafeError> {
    let me = identity(&seeds)?;
    let m = self::material(&material)?;
    let net = network(&m.descriptor.network)?;
    let id = parse_id(&proposal_id)?;
    let relay = vault_relay(&relay_url, &m);
    let mut watch = crate::diag::Stopwatch::new("send");
    let tip = local_tip(&db_dir, &db_key, &lightwalletd_url, &m)?;
    watch.lap("tip");
    let req_path = request_file(&state_dir, &id);
    let silent = node::SilentSigners::at(leader_dir(&state_dir).join(state_dir::SILENT_SIGNERS));
    let io = |e: std::io::Error| ZafeError::new(ZafeErrorKind::Other, e.to_string());

    let (_, state) = runtime().block_on(node::load_state(&relay, &me, &m))?;
    watch.lap("log");
    if let Some(p) = state.proposals.get(&id) {
        if let (ProposalStatus::Broadcast, Some(mut display)) = (p.status, p.txid) {
            display.reverse();
            on_progress(SendProgress {
                stage: SendStage::Sent,
                received: u32::from(m.descriptor.threshold),
                needed: u32::from(m.descriptor.threshold),
                txid: Some(hex::encode(display)),
                error: None,
            });
            return Ok(());
        }
    }
    if node::is_ready(&state, &id) {
        on_progress(SendProgress {
            stage: SendStage::Collecting,
            received: u32::from(m.descriptor.threshold),
            needed: u32::from(m.descriptor.threshold),
            txid: None,
            error: None,
        });
        let txid = runtime().block_on(async {
            let mut client = connect(&lightwalletd_url).await?;
            Ok::<_, ZafeError>({
                let sent = Box::pin(node::send_ready(
                    &relay,
                    &me,
                    &m,
                    &net,
                    tip,
                    id,
                    &mut client,
                    &mut OsRng,
                ))
                .await?;
                sent_txs(&db_dir, &m).put(&sent);
                sent
            })
        })?;
        watch.lap("prove+broadcast");
        store_own_send(&db_dir, &db_key, &m, tip, &txid.raw);
        watch.lap("store");
        watch.finish();
        let txid = txid.txid;
        let mut display = txid;
        display.reverse();
        on_progress(SendProgress {
            stage: SendStage::Sent,
            received: u32::from(m.descriptor.threshold),
            needed: u32::from(m.descriptor.threshold),
            txid: Some(hex::encode(display)),
            error: None,
        });
        return Ok(());
    }

    let request = match fs::read(&req_path) {
        Ok(bytes) => node::decode_request(&bytes)?,
        Err(_) => {
            let used_path = leader_dir(&state_dir).join(state_dir::USED_COMMITMENTS);
            let mut used: BTreeSet<[u8; 32]> = match fs::read(&used_path) {
                Ok(bytes) => node::decode_used_commitments(&bytes)?,
                Err(_) => BTreeSet::new(),
            };
            let sent = runtime().block_on(Box::pin(node::request_signatures(
                &relay, &me, &m, &net, tip, id, &used, &silent, &mut OsRng,
            )))?;
            used.extend(sent.used_commitments);
            fs::create_dir_all(leader_dir(&state_dir)).map_err(io)?;
            durable::write(&used_path, &node::encode_used_commitments(&used)?).map_err(io)?;
            durable::write(&req_path, &node::encode_request(&sent.request)?).map_err(io)?;
            sent.request
        }
    };
    on_progress(SendProgress {
        stage: SendStage::Collecting,
        received: 0,
        needed: request.signers.len() as u32,
        txid: None,
        error: None,
    });

    runtime().block_on(async {
        let own_path = leader_dir(&state_dir).join(format!(
            "{}.{}",
            hex::encode(id),
            state_dir::OWN_SHARES_EXT
        ));
        let own: Option<Vec<Vec<u8>>> = match fs::read(&own_path) {
            Ok(bytes) => Some(node::decode_own_shares(&bytes)?),
            Err(_) => {
                let mut store = nonce_store(&state_dir, &seeds, &m);
                let own = Box::pin(node::sign_own_shares(
                    &relay, &me, &m, &net, tip, &request, &mut store,
                ))
                .await?;
                if let Some(own) = &own {
                    durable::write(&own_path, &node::encode_own_shares(own)?).map_err(io)?;
                }
                own
            }
        };
        let mut client = connect(&lightwalletd_url).await?;
        let finalized = Box::pin(node::finalize(
            &relay,
            &me,
            &m,
            &request,
            own.as_deref(),
            &mut client,
            collect_timeout,
            |p| {
                on_progress(SendProgress {
                    stage: SendStage::Collecting,
                    received: p.received as u32,
                    needed: p.needed as u32,
                    txid: None,
                    error: None,
                });
            },
            &mut OsRng,
        ))
        .await;
        let signers: Vec<_> = request.signers.iter().copied().collect();
        let sent = match finalized {
            Ok(sent) => {
                silent.update(&[], &signers);
                sent
            }
            Err(e @ node::NodeError::Timeout(_)) => {
                if let Ok(quiet) =
                    Box::pin(node::unanswered_signers(&relay, &me, &m, &request)).await
                {
                    let answered: Vec<_> = signers
                        .iter()
                        .filter(|s| !quiet.contains(s))
                        .copied()
                        .collect();
                    silent.update(&quiet, &answered);
                }
                return Err(e.into());
            }
            Err(node::NodeError::InvalidShares(culprits)) => {
                silent.update(&culprits, &[]);
                let _ = fs::remove_file(&req_path);
                let _ = fs::remove_file(&own_path);
                return Err(ZafeError::new(
                    ZafeErrorKind::NotReady,
                    "A signer sent an invalid signature. Those who signed approve again, then \
                     send",
                ));
            }
            Err(e) => return Err(e.into()),
        };
        sent_txs(&db_dir, &m).put(&sent);
        store_own_send(&db_dir, &db_key, &m, tip, &sent.raw);
        let txid = sent.txid;
        let _ = fs::remove_file(&req_path);
        let _ = fs::remove_file(&own_path);
        let mut display = txid;
        display.reverse();
        on_progress(SendProgress {
            stage: SendStage::Sent,
            received: request.signers.len() as u32,
            needed: request.signers.len() as u32,
            txid: Some(hex::encode(display)),
            error: None,
        });
        Ok::<_, ZafeError>(())
    })
}
