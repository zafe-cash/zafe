//! Payment proposals: propose, review, approve/reject, answer signing requests, and (as the
//! leader) collect signatures and broadcast.
//!
//! `state_dir` must be app-private and excluded from backups: it holds single-use FROST
//! nonces (`nonces/`) and the leader's signing rounds (`leader/`). Reusing a nonce leaks the
//! member's key share, so a restored backup must never bring old nonces back.

use std::{collections::BTreeSet, fs, path::PathBuf, time::Duration};

use rand::rngs::OsRng;
use zafe_core::{
    node::{self, VaultMaterial},
    nonce_store::{FileNonceStore, FilePoolStore},
    relay_client::RelayClient,
    repair,
    session::{NonceStore, ProposalId},
    state_dir,
    vault::{ProposalStatus, ProposedPayment, VaultState},
    wallet::{connect, PaymentRequest, ZafeNetwork},
};
use zcash_protocol::memo::{Memo, MemoBytes};

use super::{
    error::{ZafeError, ZafeErrorKind},
    names::SignerName,
    vault::{identity, material, network, open_wallet, runtime, sent_txs, wallet_lock},
};
use crate::frb_generated::StreamSink;

type Result<T, E = ZafeError> = std::result::Result<T, E>;

/// How long the leader waits for signature shares per attempt. Retrying reuses the same
/// signing request, so shares that arrive later are not wasted.
const COLLECT_TIMEOUT: Duration = Duration::from_secs(90);

// --- Input helpers ----------------------------------------------------------------------

pub struct AddressCheck {
    pub valid: bool,
    /// Why the address can't be paid (empty when valid).
    pub reason: String,
}

/// Vault payments go to shielded (Orchard-receiver) unified addresses only.
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
                    // Same wording as the send recipient step.
                    "This address is for a different Zcash network".into()
                } else {
                    "Invalid address".into()
                }
            },
        },
    }
}

/// One payment read from a scanned QR code or pasted text.
pub struct ScannedPayment {
    pub address: String,
    /// 0 when the request leaves the amount to the payer.
    pub amount_zat: u64,
    /// Text memo (empty if none or not text).
    pub memo: String,
    /// What the link calls the recipient (ZIP 321 `label`; empty if none). Written by
    /// whoever made the link: show it as the link's claim, never as verified.
    pub label: String,
    /// The link's note to the payer (ZIP 321 `message`; empty if none). Same caveat.
    pub message: String,
}

pub struct ScannedRequest {
    /// Empty when `problem` is set.
    pub payments: Vec<ScannedPayment>,
    /// Why it can't be paid from this vault (empty when fine).
    pub problem: String,
}

/// Reads a scanned or pasted payment target: a plain address, or a ZIP 321 `zcash:` URI
/// (amount, memo, several recipients). Every address must be payable from a vault on
/// `network_name` (see `check_address`).
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
                    label: p.label().cloned().unwrap_or_default(),
                    message: p.message().cloned().unwrap_or_default(),
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

/// Parses a ZEC amount ("1.5", "0,25") to zatoshis. `None` if malformed or above 8 decimals.
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

/// Bytes a memo takes (the limit is 512).
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
            Memo::Text(t) => Some(t.to_string()),
            _ => None,
        })
        .unwrap_or_default()
}

// --- Listing ----------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProposalStage {
    /// Waiting for approvals.
    Open,
    /// Enough approvals; waiting for a member to collect signatures and send.
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
    /// Proposer's clock (unix seconds, display only).
    pub created_at: u64,
    pub txid: Option<String>,
    /// This device already sent a signing request for it (it can resume collecting).
    pub signing_started: bool,
    /// Signed at approval time (one tap): approvals carry the signatures.
    pub one_tap: bool,
    /// Some signer group has every signature: any member can send it now, alone.
    pub ready: bool,
    /// This member's approval completed the signatures (it sends when `auto_send`).
    pub completed_by_me: bool,
    /// The proposer asked for it to be sent as soon as the signatures are complete.
    pub auto_send: bool,
    /// Last block the transaction can be mined in; after it the proposal is expired.
    pub expiry_height: u32,
    /// This member approved with signing commitments whose nonces are gone from this
    /// device (used in a signing round that didn't finish, or restored from a backup): it
    /// must approve again before a new round can include it.
    pub needs_reapproval: bool,
    /// Cancelled, but every signature is already in: anyone holding them could still send
    /// it until it expires (offer to make it unsendable).
    pub still_sendable: bool,
    /// A live proposal (id) that spends this cancelled one's notes back to the vault.
    pub invalidated_by: Option<String>,
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

fn pool_store(state_dir: &str) -> FilePoolStore {
    FilePoolStore::new(PathBuf::from(state_dir).join(state_dir::POOL))
}

fn nonce_store(state_dir: &str) -> FileNonceStore {
    FileNonceStore::new(PathBuf::from(state_dir).join(state_dir::NONCES))
}

fn info(state: &VaultState, me: [u8; 32], state_dir: &str) -> Vec<ProposalInfo> {
    let d = &state.descriptor;
    let nonces = nonce_store(state_dir);
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
            txid: p.txid.map(|t| {
                let mut t = t;
                t.reverse(); // display order, like block explorers
                hex::encode(t)
            }),
            signing_started: request_file(state_dir, &p.id).exists(),
            one_tap: p.preprocessed.is_some(),
            ready: p.ready_group.is_some() && p.status == ProposalStatus::Approved,
            completed_by_me: p.completed_by == Some(me),
            auto_send: p.auto_send,
            expiry_height: p.expiry_height,
            still_sendable: p.status == ProposalStatus::Cancelled && p.ready_group.is_some(),
            invalidated_by: (p.status == ProposalStatus::Cancelled)
                .then(|| {
                    state.proposals.values().find(|q| {
                        matches!(
                            q.status,
                            ProposalStatus::Open
                                | ProposalStatus::Approved
                                | ProposalStatus::Broadcast
                        ) && q.payments.is_empty()
                            && q.nullifiers.iter().any(|nf| p.nullifiers.contains(nf))
                    })
                })
                .flatten()
                .map(|q| hex::encode(q.id)),
            needs_reapproval: matches!(p.status, ProposalStatus::Open | ProposalStatus::Approved)
                && p.approvals.get(&me).is_some_and(|c| !c.is_empty())
                && !nonces.contains(&p.id, &p.pczt_hash),
        })
        .collect();
    out.sort_by_key(|p| {
        std::cmp::Reverse(state.proposals[&parse_id(&p.id).expect("own id")].log_index)
    });
    out
}

pub struct ProposalList {
    /// Newest first.
    pub items: Vec<ProposalInfo>,
    /// Log entries written by a newer version of Zafe that this build skipped: other
    /// members may see something this device can't, so the app asks to update.
    pub newer_version_entries: u32,
    /// Names members gave themselves in the log (this member's included). The app shows
    /// its own local label first, then these.
    pub shared_names: Vec<SignerName>,
    /// Members (hex signing keys) who attested a backup of their current keys
    /// (backup health, spec §12.2).
    pub backed_up: Vec<String>,
    /// Seat moves waiting for approvals (a signer who lost their phone, spec §10.1).
    pub seat_moves: Vec<SeatMove>,
    /// Moved seats whose key isn't rebuilt yet (a helper may have stalled).
    pub repairs: Vec<RepairInfo>,
    /// This member's vault material with the current membership, when a seat moved since
    /// the stored copy: save it in place of the old one.
    pub updated_material: Option<Vec<u8>>,
}

/// A moved seat whose key the helpers are still rebuilding on the new phone.
pub struct RepairInfo {
    /// The move (pass to `retry_repair`).
    pub replacement: u64,
    /// The new phone's key (hex).
    pub new_key_hex: String,
    /// The members rebuilding it in the current attempt (hex keys).
    pub helpers: Vec<String>,
    /// 0 for the approvers' attempt, +1 per retry.
    pub attempt: u32,
}

/// A signer moving to a new phone, waiting for approvals.
pub struct SeatMove {
    /// The signer who lost their phone (hex key).
    pub old_key_hex: String,
    /// The new phone's key (hex).
    pub new_key_hex: String,
    /// What the new phone shows ("1234 5678"): compare before approving.
    pub safety_code: String,
    /// Members who approved so far (hex keys).
    pub approvals: Vec<String>,
    pub needed: u32,
    /// The recovery code, rebuilt from the log, for approving from this list.
    pub code: String,
}

/// Every proposal in the vault log, newest first. Also keeps this device ready for
/// one-tap signing: tops up its pre-published commitments when they run low, and deletes
/// nonces of proposals that closed or expired (as of `tip_height`, the synced tip, when
/// known).
pub fn list_proposals(
    relay_url: String,
    state_dir: String,
    seeds: Vec<u8>,
    material: Vec<u8>,
    tip_height: Option<u32>,
) -> Result<ProposalList, ZafeError> {
    let me = identity(&seeds)?;
    let m = self::material(&material)?;
    let relay = RelayClient::new(relay_url);
    let mut pool = pool_store(&state_dir);
    let state = runtime().block_on(async {
        node::top_up_pool(&relay, &me, &m, &mut pool, &mut OsRng).await?;
        let (_, state) = node::load_state(&relay, &me, &m).await?;
        // This member's part in repairing a moved seat's key (best effort: retried on
        // every refresh).
        let repair_dir = PathBuf::from(&state_dir).join(state_dir::REPAIR);
        if let Err(e) = repair::help_repairs(&relay, &me, &m, &state, &repair_dir, &mut OsRng).await
        {
            eprintln!("share repair: {e}");
        }
        // A recovered phone tells the others its key is back (once).
        if state
            .replacement_to(&me.public().sig_pk)
            .is_some_and(|r| !r.done)
        {
            if let Err(e) = repair::mark_repair_done(&relay, &me, &m, &mut OsRng).await {
                eprintln!("repair done: {e}");
            }
            let (_, fresh) = node::load_state(&relay, &me, &m).await?;
            return Ok::<_, ZafeError>(fresh);
        }
        Ok::<_, ZafeError>(state)
    })?;
    node::forget_closed(&state, &me.public().sig_pk, tip_height, &mut pool);
    node::forget_closed_nonces(&state, tip_height, &mut nonce_store(&state_dir));
    Ok(ProposalList {
        items: info(&state, me.public().sig_pk, &state_dir),
        newer_version_entries: state.newer_version_entries() as u32,
        shared_names: shared_names(&state),
        backed_up: backed_up(&state),
        seat_moves: seat_moves(&state),
        repairs: repairs(&state),
        updated_material: repair::current_material(&m, &state)
            .map(|m| m.to_bytes())
            .transpose()
            .map_err(|e| ZafeError::new(ZafeErrorKind::Other, e.to_string()))?,
    })
}

fn repairs(state: &VaultState) -> Vec<RepairInfo> {
    state
        .replacements
        .iter()
        .filter(|r| !r.done)
        .map(|r| RepairInfo {
            replacement: r.index,
            new_key_hex: hex::encode(r.new.sig_pk),
            helpers: r.helpers.iter().map(hex::encode).collect(),
            attempt: r.attempt,
        })
        .collect()
}

/// Takes over from `stalled_key_hex`, a helper not doing its part in rebuilding a moved
/// seat's key: a new attempt starts with this member in its place.
pub fn retry_repair(
    relay_url: String,
    seeds: Vec<u8>,
    material: Vec<u8>,
    replacement: u64,
    stalled_key_hex: String,
) -> Result<(), ZafeError> {
    let me = identity(&seeds)?;
    let m = self::material(&material)?;
    let stalled: [u8; 32] = hex::decode(stalled_key_hex.trim())
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or_else(|| ZafeError::invalid("not a signer key"))?;
    runtime().block_on(repair::retry_repair(
        &RelayClient::new(relay_url),
        &me,
        &m,
        replacement,
        stalled,
        &mut OsRng,
    ))?;
    Ok(())
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
                safety_code: request.safety_code(),
                approvals: p.approvals.iter().map(|(pk, _)| hex::encode(pk)).collect(),
                needed: u32::from(state.descriptor.threshold),
                code: request.encode(),
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
    state
        .names
        .iter()
        .map(|(pk, name)| SignerName {
            key_hex: hex::encode(pk),
            name: name.clone(),
        })
        .collect()
}

// --- Proposing and reviewing ------------------------------------------------------------

pub struct PaymentInput {
    pub address: String,
    pub amount_zat: u64,
    pub memo: String,
}

/// Syncs, builds the transaction from the vault's notes, and logs it as a proposal. Returns
/// the proposal id.
#[allow(clippy::too_many_arguments)]
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
    let net = network(&m.descriptor.network)?;
    if payments.is_empty() {
        return Err(ZafeError::invalid("Add at least one payment"));
    }
    let requests = payments
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
        .collect::<Result<Vec<_>>>()?;
    let _guard = wallet_lock();
    let id = runtime().block_on(async {
        let mut wallet = open_wallet(&db_dir, &db_key, &lightwalletd_url, &m).await?;
        let mut client = connect(&lightwalletd_url).await?;
        wallet.sync(&mut client).await?;
        let relay = RelayClient::new(relay_url);
        Ok::<_, ZafeError>(
            node::propose(
                &relay,
                &me,
                &m,
                &mut wallet,
                &mut client,
                &sent_txs(&db_dir, &m),
                &requests,
                auto_send,
                &mut OsRng,
            )
            .await?,
        )
    })?;
    Ok(hex::encode(id))
}

/// Proposes spending a cancelled proposal's notes back to the vault, so the cancelled
/// transaction can never be sent (see `node::invalidate`). Returns the new proposal id.
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
    let _guard = wallet_lock();
    let id = runtime().block_on(async {
        let mut wallet = open_wallet(&db_dir, &db_key, &lightwalletd_url, &m).await?;
        let mut client = connect(&lightwalletd_url).await?;
        wallet.sync(&mut client).await?;
        Ok::<_, ZafeError>(
            node::invalidate(
                &RelayClient::new(relay_url),
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
    })?;
    Ok(hex::encode(id))
}

/// This device's independent check of a proposal (spec §9.3).
pub struct ReviewInfo {
    /// Whether the transaction pays exactly the proposed payments, with change back to
    /// the vault and the standard fee. If false, `problem` says why: do not approve.
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
    let _guard = wallet_lock();
    let wallet = runtime().block_on(open_wallet(db_dir, db_key, lightwalletd_url, m))?;
    wallet
        .chain_height()?
        .ok_or_else(|| ZafeError::new(ZafeErrorKind::NotReady, "The vault has not synced yet"))
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
    let tip = local_tip(&db_dir, &db_key, &lightwalletd_url, &m)?;
    let (_, state) = runtime().block_on(node::load_state(&RelayClient::new(relay_url), &me, &m))?;
    match node::review(&state, &m, &net, tip, id) {
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

// --- Voting -----------------------------------------------------------------------------

pub struct ApproveResult {
    /// The approval carried this member's signatures (one tap).
    pub signed: bool,
    /// This approval completed the signatures: the proposal can be sent now.
    pub completed: bool,
    /// The proposer asked for the completing member to send right away.
    pub auto_send: bool,
}

/// Verifies the proposal on this device and, only if it passes, approves it. For one-tap
/// proposals the approval also signs; see `ApproveResult`.
#[allow(clippy::too_many_arguments)]
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
    let tip = local_tip(&db_dir, &db_key, &lightwalletd_url, &m)?;
    let mut store = nonce_store(&state_dir);
    let mut pool = pool_store(&state_dir);
    let approved = runtime().block_on(node::approve(
        &RelayClient::new(relay_url),
        &me,
        &m,
        &net,
        tip,
        id,
        &mut store,
        &mut pool,
        &mut OsRng,
    ))?;
    Ok(ApproveResult {
        signed: approved.signed,
        completed: approved.completed,
        auto_send: approved.auto_send,
    })
}

pub fn reject_proposal(
    relay_url: String,
    seeds: Vec<u8>,
    material: Vec<u8>,
    proposal_id: String,
) -> Result<(), ZafeError> {
    let me = identity(&seeds)?;
    let m = self::material(&material)?;
    let id = parse_id(&proposal_id)?;
    runtime().block_on(node::reject(
        &RelayClient::new(relay_url),
        &me,
        &m,
        id,
        &mut OsRng,
    ))?;
    Ok(())
}

/// Sets this member's display name for the other members (empty clears it). Logged only
/// when it changes.
pub fn set_my_name(
    relay_url: String,
    seeds: Vec<u8>,
    material: Vec<u8>,
    name: String,
) -> Result<(), ZafeError> {
    let me = identity(&seeds)?;
    let m = self::material(&material)?;
    runtime().block_on(node::set_name(
        &RelayClient::new(relay_url),
        &me,
        &m,
        &name,
        &mut OsRng,
    ))?;
    Ok(())
}

/// Records in the vault log that this member saved a backup that opens (backup health).
/// Logged once per key epoch.
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
        &RelayClient::new(relay_url),
        &me,
        &m,
        at,
        &mut OsRng,
    ))?;
    Ok(())
}

/// Cancels a proposal this member authored (open or approved, not yet sent).
pub fn cancel_proposal(
    relay_url: String,
    seeds: Vec<u8>,
    material: Vec<u8>,
    proposal_id: String,
) -> Result<(), ZafeError> {
    let me = identity(&seeds)?;
    let m = self::material(&material)?;
    let id = parse_id(&proposal_id)?;
    runtime().block_on(node::cancel(
        &RelayClient::new(relay_url),
        &me,
        &m,
        id,
        &mut OsRng,
    ))?;
    Ok(())
}

// --- Signing ----------------------------------------------------------------------------

/// Leader: abandons this device's unfinished signing round for a proposal (a chosen
/// signer never answered), so the next send starts a new one. Commitment sets the old
/// round used stay recorded as used; signers who took part approve again for fresh ones.
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

/// Builds the proving and verifying keys now (seconds on a phone, then kept for the
/// process), so sending an approved payment starts proving at once. Returns when built.
pub fn prewarm_prover() {
    node::proving_key();
    node::verifying_key();
}

/// Answers signing requests for proposals this member approved (each is re-verified first)
/// and lets the relay delete what this member has handled. Called on every poll.
/// `tip_height` is the synced tip when the caller knows it (saves opening the wallet).
/// Returns how many were answered.
#[allow(clippy::too_many_arguments)]
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
    let mut store = nonce_store(&state_dir);
    // Runs even without stored nonces (one-tap members rarely hold any): the inbox still
    // has keygen messages, finished requests and old shares for the relay to delete.
    let tip = match tip_height {
        Some(tip) => tip,
        None => local_tip(&db_dir, &db_key, &lightwalletd_url, &m)?,
    };
    let report = runtime().block_on(node::respond(
        &RelayClient::new(relay_url),
        &me,
        &m,
        &net,
        tip,
        &mut store,
        &mut OsRng,
    ))?;
    Ok(report.answered.len() as u32)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendStage {
    /// Signing requests are out; the transaction proof is being built meanwhile.
    Collecting,
    Sent,
    /// Not sent; `error` says why. Retrying resumes the same signing round.
    Failed,
}

pub struct SendProgress {
    pub stage: SendStage,
    pub received: u32,
    pub needed: u32,
    pub txid: Option<String>,
    pub error: Option<ZafeError>,
}

/// Leader: asks the approvers for signatures (once per proposal on this device), signs its
/// own part, collects the rest while proving, and broadcasts. On a timeout, call again: it
/// resumes the same signing round.
#[allow(clippy::too_many_arguments)]
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
    // Failures travel as a typed event: a streaming function's own error never reaches the
    // Dart listener, and `add_error` is decoded as an untyped anyhow string.
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

/// `send_proposal` with a plain callback and how long to wait for signature shares
/// (tests and non-Flutter callers).
#[flutter_rust_bridge::frb(ignore)]
#[allow(clippy::too_many_arguments)]
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
    let relay = RelayClient::new(relay_url);
    let tip = local_tip(&db_dir, &db_key, &lightwalletd_url, &m)?;
    let req_path = request_file(&state_dir, &id);
    let io = |e: std::io::Error| ZafeError::new(ZafeErrorKind::Other, e.to_string());

    // One tap: the approvals already carry every signature; aggregate and send alone.
    let (_, state) = runtime().block_on(node::load_state(&relay, &me, &m))?;
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
                let sent =
                    node::send_ready(&relay, &me, &m, &net, tip, id, &mut client, &mut OsRng)
                        .await?;
                sent_txs(&db_dir, &m).put(&sent);
                sent.txid
            })
        })?;
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
            // Commitment sets already put in a request must never be reused.
            let used_path = leader_dir(&state_dir).join(state_dir::USED_COMMITMENTS);
            let mut used: BTreeSet<[u8; 32]> = match fs::read(&used_path) {
                Ok(bytes) => node::decode_used_commitments(&bytes)?,
                Err(_) => BTreeSet::new(),
            };
            let sent = runtime().block_on(node::request_signatures(
                &relay, &me, &m, &net, tip, id, &used, &mut OsRng,
            ))?;
            used.extend(sent.used_commitments);
            fs::create_dir_all(leader_dir(&state_dir)).map_err(io)?;
            fs::write(&used_path, node::encode_used_commitments(&used)?).map_err(io)?;
            fs::write(&req_path, node::encode_request(&sent.request)?).map_err(io)?;
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
        // Our own shares, if we are one of the chosen signers: signed once (that consumes
        // the nonces) and kept until the broadcast, so a retry reuses them.
        let own_path = leader_dir(&state_dir).join(format!(
            "{}.{}",
            hex::encode(id),
            state_dir::OWN_SHARES_EXT
        ));
        let own: Option<Vec<Vec<u8>>> = match fs::read(&own_path) {
            Ok(bytes) => Some(node::decode_own_shares(&bytes)?),
            Err(_) => {
                let mut store = nonce_store(&state_dir);
                let own =
                    node::sign_own_shares(&relay, &me, &m, &net, tip, &request, &mut store).await?;
                if let Some(own) = &own {
                    fs::write(&own_path, node::encode_own_shares(own)?).map_err(io)?;
                }
                own
            }
        };
        let mut client = connect(&lightwalletd_url).await?;
        let sent = node::finalize(
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
        )
        .await?;
        sent_txs(&db_dir, &m).put(&sent);
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
