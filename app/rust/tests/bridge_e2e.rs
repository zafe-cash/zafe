//! The app's whole payment flow through the Flutter bridge API (what the Dart side calls),
//! for three members on a live Ironwood regtest chain: create/join/seal, keygen, fund,
//! sync, propose, review, approve, answer signing requests, and send.
//!
//! Needs Docker and `ths` (see `infra/regtest/up.sh`). `cargo test -p rust_lib_zafe --test bridge_e2e -- --ignored --nocapture`

use std::{path::PathBuf, process::Command, thread, time::Duration};

use rust_lib_zafe::api::{
    backup,
    error::ZafeErrorKind,
    proposals::{self, MyVote, PaymentInput, ProposalStage, SendStage},
    received, vault,
};

const NAME: &str = "zafe-bridge";
// ths moves every port by this offset: Zakura RPC 48332, lightwalletd 39167.
const PORT_OFFSET: u16 = 30100;
const LWD_PORT: u16 = 9067 + PORT_OFFSET;
const RELAY_PORT: u16 = 48887;
/// 5 ZEC faucet notes the vault starts with.
const FUND_NOTES: u32 = 6;

/// A `ths` (thus-spoke-zakura) environment started by `infra/regtest/up.sh`.
struct Regtest;

impl Regtest {
    fn script(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../infra/regtest")
            .join(name)
    }

    fn start() -> Self {
        let status = Command::new(Self::script("up.sh"))
            .env("ZAFE_REGTEST_NAME", NAME)
            .env("ZAFE_REGTEST_PORT_OFFSET", PORT_OFFSET.to_string())
            .status()
            .expect("run up.sh");
        assert!(status.success(), "regtest failed to start");
        Self
    }

    /// Several 5 ZEC faucet notes, confirmed.
    fn fund(&self, address: &str, notes: u32) {
        let status = Command::new(Self::script("fund.sh"))
            .args([address, &notes.to_string()])
            .env("ZAFE_REGTEST_NAME", NAME)
            .status()
            .expect("run fund.sh");
        assert!(status.success(), "funding failed");
    }

    fn mine(&self, blocks: u32) {
        let ths = std::env::var("THS").unwrap_or_else(|_| "ths".into());
        let out = Command::new(ths)
            .args(["--name", NAME, "mine", &blocks.to_string()])
            .output()
            .expect("run ths mine");
        assert!(
            out.status.success(),
            "ths mine failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

impl Drop for Regtest {
    fn drop(&mut self) {
        let _ = Command::new(Self::script("down.sh"))
            .env("ZAFE_REGTEST_NAME", NAME)
            .status();
    }
}

/// A throwaway regtest unified address that is not the vault's (as examples/vault_address).
fn outside_address() -> String {
    use orchard::keys::{FullViewingKey, Scope, SpendingKey};
    use zafe_core::keys::{VaultKeys, VaultSecret};
    let ak: [u8; 32] = FullViewingKey::from(&SpendingKey::from_bytes([42; 32]).unwrap()).to_bytes()
        [..32]
        .try_into()
        .unwrap();
    let keys = VaultKeys::derive(&VaultSecret::from_bytes([1; 32]), &ak).unwrap();
    let address = keys.fvk().address_at(0u32, Scope::External);
    zcash_keys::address::UnifiedAddress::from_receivers(Some(address), None, None)
        .unwrap()
        .encode(&zafe_core::wallet::regtest_network())
}

#[derive(Clone)]
struct Member {
    seeds: Vec<u8>,
    material: Vec<u8>,
    db_dir: String,
    db_key: Vec<u8>,
    state_dir: String,
}

fn start_relay() {
    thread::spawn(|| {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let listener = tokio::net::TcpListener::bind(("127.0.0.1", RELAY_PORT))
                .await
                .unwrap();
            axum::serve(
                listener,
                zafe_relay::Relay::new()
                    .with_limits(zafe_relay::limits::Limits::hosted())
                    .router(),
            )
            .await
            .unwrap();
        })
    });
    thread::sleep(Duration::from_millis(300));
}

#[test]
#[ignore = "needs Docker (Ironwood regtest)"]
fn payment_flow_through_bridge() {
    let relay = format!("http://127.0.0.1:{RELAY_PORT}");
    let lwd = format!("http://127.0.0.1:{LWD_PORT}");
    let tmp = std::env::temp_dir().join(format!("zafe-bridge-{}", std::process::id()));
    // Each phone keeps a copy of the vault logs (the app does this at startup).
    rust_lib_zafe::api::app::init_log_cache(tmp.join("log").to_string_lossy().into_owned());
    start_relay();

    // Setup: A creates a 2-of-3 vault, B and C join, A seals.
    let seeds: Vec<Vec<u8>> = (0..3).map(|_| vault::generate_identity().seeds).collect();
    let invite =
        vault::create_vault(relay.clone(), seeds[0].clone(), "Grants".into(), 2, 3).unwrap();
    for s in &seeds[1..] {
        vault::join_vault(relay.clone(), s.clone(), invite.clone()).unwrap();
    }
    vault::seal_vault(relay.clone(), seeds[0].clone(), invite.clone()).unwrap();
    let safety: Vec<String> = seeds
        .iter()
        .map(|s| {
            vault::vault_membership(relay.clone(), s.clone(), invite.clone())
                .unwrap()
                .safety_number
        })
        .collect();
    assert!(safety.iter().all(|n| n == &safety[0]));

    // Keygen: all three at once (A picks birthday 2: regtest isn't up yet).
    // Each member's signing state dir, where keygen leaves its one-tap pool nonces. C
    // publishes none (as if it closed the app right after keygen), so the first payment
    // falls back to interactive signing and this test covers that path.
    let handles: Vec<_> = seeds
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let (relay, lwd, s, invite, sn) = (
                relay.clone(),
                lwd.clone(),
                s.clone(),
                invite.clone(),
                safety[0].clone(),
            );
            let state = (i != 2).then(|| {
                let dir = tmp.join(format!("m{i}")).join("signing");
                dir.to_string_lossy().into_owned()
            });
            thread::spawn(move || {
                vault::run_keygen(
                    relay,
                    lwd,
                    "regtest".into(),
                    s,
                    invite,
                    sn,
                    120,
                    Some(2),
                    None,
                    state,
                )
                .unwrap()
            })
        })
        .collect();
    let materials: Vec<Vec<u8>> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    let summary = vault::vault_summary(materials[0].clone()).unwrap();
    for m in &materials[1..] {
        assert_eq!(
            vault::vault_summary(m.clone()).unwrap().address,
            summary.address
        );
    }
    let members: Vec<Member> = seeds
        .into_iter()
        .zip(materials)
        .enumerate()
        .map(|(i, (seeds, material))| {
            let dir = tmp.join(format!("m{i}"));
            let state = dir.join("signing");
            std::fs::create_dir_all(&state).unwrap();
            Member {
                seeds,
                material,
                db_dir: dir.to_string_lossy().into(),
                db_key: rand::random::<[u8; 32]>().to_vec(),
                state_dir: state.to_string_lossy().into(),
            }
        })
        .collect();

    // Fund the vault from the ths faucet: one note per proposal that holds notes at once.
    let chain = Regtest::start();
    chain.fund(&summary.address, FUND_NOTES);
    let sync = |m: &Member| {
        vault::sync_vault(
            m.db_dir.clone(),
            m.db_key.clone(),
            lwd.clone(),
            relay.clone(),
            m.seeds.clone(),
            m.material.clone(),
        )
        .unwrap()
    };
    let mut balance = sync(&members[0]);
    for _ in 0..60 {
        if balance.spendable_zat > 0 {
            break;
        }
        thread::sleep(Duration::from_secs(1));
        balance = sync(&members[0]);
    }
    assert!(
        balance.spendable_zat > 100_000_000,
        "vault not funded: {}",
        balance.spendable_zat
    );
    for m in &members[1..] {
        sync(m);
    }
    // Wallet databases are encrypted at rest.
    let db_file =
        std::path::Path::new(&members[0].db_dir).join(format!("vault-{}.sqlite", summary.vault_id));
    assert!(!std::fs::read(&db_file)
        .unwrap()
        .starts_with(b"SQLite format 3"));

    // The faucet payments show up as received payments (mined, with block times).
    let received_list = |m: &Member| {
        received::list_received(m.db_dir.clone(), m.db_key.clone(), m.material.clone()).unwrap()
    };
    let incoming = received_list(&members[0]);
    assert!(!incoming.is_empty(), "no received payments listed");
    assert!(incoming.iter().all(|r| !r.is_coinbase
        && r.mined_height > 0
        && r.block_time_secs > 0
        && r.confirmations >= 1));
    assert!(incoming.iter().map(|r| r.amount_zat).sum::<u64>() >= balance.total_zat);
    assert!(incoming
        .windows(2)
        .all(|w| w[0].mined_height >= w[1].mined_height));

    // Inputs as the Dart screens validate them.
    let payee = outside_address();
    assert!(proposals::check_address("regtest".into(), payee.clone()).valid);
    assert_eq!(proposals::parse_zec("1.5".into()), Some(150_000_000));
    assert_eq!(proposals::parse_zec("0,00000001".into()), Some(1));
    assert_eq!(proposals::parse_zec("1.000000001".into()), None);

    // Paying the vault itself is refused up front (every member's check would fail).
    let a0 = &members[0];
    let own = proposals::propose_payment(
        relay.clone(),
        lwd.clone(),
        a0.db_dir.clone(),
        a0.db_key.clone(),
        a0.seeds.clone(),
        a0.material.clone(),
        vec![PaymentInput {
            address: summary.address.clone(),
            amount_zat: 100_000_000,
            memo: String::new(),
        }],
        false,
    );
    assert_eq!(own.err().map(|e| e.kind), Some(ZafeErrorKind::InvalidInput));

    // A proposes 1 ZEC.
    let b_before = sync(&members[1]);
    let a = &members[0];
    let id = proposals::propose_payment(
        relay.clone(),
        lwd.clone(),
        a.db_dir.clone(),
        a.db_key.clone(),
        a.seeds.clone(),
        a.material.clone(),
        vec![PaymentInput {
            address: payee.clone(),
            amount_zat: 100_000_000,
            memo: "grant #1".into(),
        }],
        false,
    )
    .unwrap();
    let list = |m: &Member| {
        proposals::list_proposals(
            relay.clone(),
            m.state_dir.clone(),
            m.seeds.clone(),
            m.material.clone(),
            None,
        )
        .unwrap()
        .items
    };
    // B's next sync holds back the notes A's open proposal spends: B's spendable balance
    // (what B could propose) drops, the total doesn't.
    let b_after = sync(&members[1]);
    assert!(b_after.spendable_zat < b_before.spendable_zat);
    assert_eq!(b_after.total_zat, b_before.total_zat);
    let p = &list(&members[1])[0];
    assert_eq!(p.id, id);
    assert_eq!(p.stage, ProposalStage::Open);
    assert_eq!(p.payments[0].memo, "grant #1");
    assert!(!p.is_mine);

    // Approvals are asynchronous: let 300 blocks pass (the old 40-block wallet default would
    // have expired the proposal after ~50 minutes; the vault window is 7 days).
    chain.mine(300);
    let before = balance.height;
    for m in &members {
        let mut b = sync(m);
        for _ in 0..60 {
            if b.height >= before + 300 {
                break;
            }
            thread::sleep(Duration::from_secs(1));
            b = sync(m);
        }
        assert!(b.height >= before + 300, "member did not reach the new tip");
    }

    // A (the proposer) and B review independently, then approve; C stays out. So the
    // leader is one of the two signers and must sign its own part locally.
    for m in &members[..2] {
        let r = proposals::review_proposal(
            relay.clone(),
            lwd.clone(),
            m.db_dir.clone(),
            m.db_key.clone(),
            m.seeds.clone(),
            m.material.clone(),
            id.clone(),
        )
        .unwrap();
        assert!(r.verified, "review failed: {}", r.problem);
        assert_eq!(r.fee_zat, 10_000);
        assert!(
            r.expiry_height > r.tip_height + 7_000,
            "expiry {} too close to tip {}",
            r.expiry_height,
            r.tip_height
        );
        proposals::approve_proposal(
            relay.clone(),
            lwd.clone(),
            m.db_dir.clone(),
            m.db_key.clone(),
            m.state_dir.clone(),
            m.seeds.clone(),
            m.material.clone(),
            id.clone(),
        )
        .unwrap();
    }
    let p = &list(&members[1])[0];
    assert_eq!(p.stage, ProposalStage::Approved);
    assert_eq!(p.my_vote, MyVote::Approved);

    // Approving twice is a NotReady error, not a crash.
    let again = proposals::approve_proposal(
        relay.clone(),
        lwd.clone(),
        members[1].db_dir.clone(),
        members[1].db_key.clone(),
        members[1].state_dir.clone(),
        members[1].seeds.clone(),
        members[1].material.clone(),
        id.clone(),
    );
    assert!(matches!(again, Err(e) if e.kind == ZafeErrorKind::NotReady));

    // The leader's send, on its own thread (it waits `secs` for signature shares).
    let start_send = |m: &Member, secs: u64| {
        let (relay, lwd, db, key, st, s, mat, id) = (
            relay.clone(),
            lwd.clone(),
            m.db_dir.clone(),
            m.db_key.clone(),
            m.state_dir.clone(),
            m.seeds.clone(),
            m.material.clone(),
            id.clone(),
        );
        thread::spawn(move || {
            let mut events = Vec::new();
            proposals::send_with_progress(
                relay,
                lwd,
                db,
                key,
                st,
                s,
                mat,
                id,
                Duration::from_secs(secs),
                |p| {
                    println!("progress {:?} {}/{}", p.stage, p.received, p.needed);
                    events.push((p.stage, p.txid));
                },
            )
            .map(|_| events)
        })
    };
    let kind = |r: Result<_, rust_lib_zafe::api::error::ZafeError>| r.err().map(|e| e.kind);

    // Round 1: A leads and signs its own part; B never answers, so it times out.
    let round1 = start_send(a, 5).join().unwrap();
    assert_eq!(kind(round1), Some(ZafeErrorKind::Timeout));
    assert!(list(a)[0].signing_started);

    // A starts over. Its own signature went into round 1, so it must approve again; B
    // still holds its nonces, but its commitments count as used, so C takes its place.
    proposals::restart_signing(a.state_dir.clone(), id.clone()).unwrap();
    let p = &list(a)[0];
    assert!(!p.signing_started && p.needs_reapproval);
    assert!(!list(&members[1])[0].needs_reapproval);
    let not_ready = start_send(a, 5).join().unwrap();
    assert_eq!(kind(not_ready), Some(ZafeErrorKind::NotReady));
    for m in [&members[0], &members[2]] {
        proposals::approve_proposal(
            relay.clone(),
            lwd.clone(),
            m.db_dir.clone(),
            m.db_key.clone(),
            m.state_dir.clone(),
            m.seeds.clone(),
            m.material.clone(),
            id.clone(),
        )
        .unwrap();
    }
    assert!(!list(a)[0].needs_reapproval);

    // Round 2: A and C. C answers on its next poll; A aggregates and broadcasts.
    let leader = start_send(a, 90);
    let mut answered = 0;
    for _ in 0..60 {
        answered += proposals::answer_signing_requests(
            relay.clone(),
            lwd.clone(),
            members[2].db_dir.clone(),
            members[2].db_key.clone(),
            members[2].state_dir.clone(),
            members[2].seeds.clone(),
            members[2].material.clone(),
            None,
        )
        .unwrap();
        if answered >= 1 || leader.is_finished() {
            break;
        }
        thread::sleep(Duration::from_millis(500));
    }
    let events = leader.join().unwrap().expect("send failed");
    let (stage, txid) = events.last().cloned().unwrap();
    assert_eq!(stage, SendStage::Sent);
    let txid = txid.unwrap();
    println!("broadcast {txid}");

    // The leader kept the raw transaction so it can resend it if it drops out of the
    // mempool; it goes away once mined (checked after mining below).
    let sent_file = std::path::Path::new(&a.db_dir)
        .join(format!("sent-{}", summary.vault_id))
        .join(format!("{}.tx", {
            let mut b = hex::decode(&txid).unwrap();
            b.reverse(); // display order -> protocol order
            hex::encode(b)
        }));
    assert!(sent_file.exists(), "leader keeps {}", sent_file.display());

    let p = &list(&members[2])[0];
    assert_eq!(p.stage, ProposalStage::Sent);
    assert_eq!(p.txid.as_deref(), Some(txid.as_str()));
    assert!(!p.signing_started);

    // Mined: the vault's balance drops by amount + fee (unless it paid itself).
    chain.mine(1);
    thread::sleep(Duration::from_secs(3));
    let after = sync(&members[2]);
    println!("after: height {} total {}", after.height, after.total_zat);
    sync(a);
    assert!(
        !sent_file.exists(),
        "the kept transaction is deleted once mined"
    );
    // The vault's own payment (and its change) is not an incoming payment.
    let incoming_after = received_list(&members[2]);
    assert!(incoming_after.iter().all(|r| r.txid != txid));
    assert_eq!(incoming_after.len(), incoming.len());

    // --- One tap. Every member refreshed its proposal list above, which published its
    // commitments, so this proposal is signed at approval time: A and B each approve once,
    // B's approval completes the signatures, and B sends alone (no request round, C and A
    // don't need to be online).
    chain.mine(3);
    for m in &members {
        let h = after.height + 3;
        let mut b = sync(m);
        for _ in 0..60 {
            if b.height >= h {
                break;
            }
            thread::sleep(Duration::from_secs(1));
            b = sync(m);
        }
    }
    for m in &members {
        list(m); // tops up pools
    }
    let id2 = proposals::propose_payment(
        relay.clone(),
        lwd.clone(),
        a.db_dir.clone(),
        a.db_key.clone(),
        a.seeds.clone(),
        a.material.clone(),
        vec![PaymentInput {
            address: payee.clone(),
            amount_zat: 50_000_000,
            memo: "grant #2".into(),
        }],
        true,
    )
    .unwrap();
    let p = list(&members[2]).into_iter().find(|p| p.id == id2).unwrap();
    assert!(p.one_tap, "second proposal should be one-tap");
    assert!(p.auto_send);
    let approve = |m: &Member| {
        proposals::approve_proposal(
            relay.clone(),
            lwd.clone(),
            m.db_dir.clone(),
            m.db_key.clone(),
            m.state_dir.clone(),
            m.seeds.clone(),
            m.material.clone(),
            id2.clone(),
        )
        .unwrap()
    };
    let r_a = approve(&members[0]);
    assert!(r_a.signed && !r_a.completed);
    let r_b = approve(&members[1]);
    assert!(r_b.signed && r_b.completed && r_b.auto_send);
    let p = list(&members[2]).into_iter().find(|p| p.id == id2).unwrap();
    assert!(p.ready && !p.completed_by_me);

    let b = &members[1];
    let mut sent = None;
    proposals::send_with_progress(
        relay.clone(),
        lwd.clone(),
        b.db_dir.clone(),
        b.db_key.clone(),
        b.state_dir.clone(),
        b.seeds.clone(),
        b.material.clone(),
        id2.clone(),
        Duration::from_secs(90),
        |p| {
            if p.stage == SendStage::Sent {
                sent = p.txid;
            }
        },
    )
    .expect("one-tap send");
    let txid2 = sent.expect("txid");
    println!("one-tap broadcast {txid2}");
    let p = list(&members[0]).into_iter().find(|p| p.id == id2).unwrap();
    assert_eq!(p.stage, ProposalStage::Sent);
    assert_eq!(p.txid.as_deref(), Some(txid2.as_str()));

    // Cancel: only the author can; afterwards nobody can vote on it.
    let id3 = proposals::propose_payment(
        relay.clone(),
        lwd.clone(),
        a.db_dir.clone(),
        a.db_key.clone(),
        a.seeds.clone(),
        a.material.clone(),
        vec![PaymentInput {
            address: payee.clone(),
            amount_zat: 10_000_000,
            memo: String::new(),
        }],
        false,
    )
    .unwrap();
    let cancel = |m: &Member| {
        proposals::cancel_proposal(
            relay.clone(),
            m.seeds.clone(),
            m.material.clone(),
            id3.clone(),
        )
    };
    assert!(cancel(&members[1]).is_err(), "only the author cancels");
    cancel(a).unwrap();
    let p = list(&members[2]).into_iter().find(|p| p.id == id3).unwrap();
    assert_eq!(p.stage, ProposalStage::Cancelled);
    assert!(p.expiry_height > 0);

    // Make the cancelled payment unsendable: a proposal with no payments spends its notes
    // back to the vault. Every member's check accepts it (all outputs are the vault's).
    let sweep = proposals::invalidate_proposal(
        relay.clone(),
        lwd.clone(),
        a.db_dir.clone(),
        a.db_key.clone(),
        a.seeds.clone(),
        a.material.clone(),
        id3.clone(),
    )
    .unwrap();
    let cancelled = list(&members[1]).into_iter().find(|p| p.id == id3).unwrap();
    assert_eq!(cancelled.invalidated_by.as_deref(), Some(sweep.as_str()));
    let s = list(&members[1])
        .into_iter()
        .find(|p| p.id == sweep)
        .unwrap();
    assert!(s.payments.is_empty() && s.auto_send);
    let b = &members[1];
    let r = proposals::review_proposal(
        relay.clone(),
        lwd.clone(),
        b.db_dir.clone(),
        b.db_key.clone(),
        b.seeds.clone(),
        b.material.clone(),
        sweep.clone(),
    )
    .unwrap();
    assert!(r.verified, "sweep review failed: {}", r.problem);
    for m in [a, b] {
        proposals::approve_proposal(
            relay.clone(),
            lwd.clone(),
            m.db_dir.clone(),
            m.db_key.clone(),
            m.state_dir.clone(),
            m.seeds.clone(),
            m.material.clone(),
            sweep.clone(),
        )
        .unwrap();
    }
    // One tap (pools are topped up) or interactive: B sends; A answers if asked.
    let sender = {
        let (relay, lwd, b) = (relay.clone(), lwd.clone(), b.clone());
        let sweep = sweep.clone();
        thread::spawn(move || {
            let mut sent = None;
            proposals::send_with_progress(
                relay,
                lwd,
                b.db_dir,
                b.db_key,
                b.state_dir,
                b.seeds,
                b.material,
                sweep,
                Duration::from_secs(90),
                |p| {
                    if p.stage == SendStage::Sent {
                        sent = p.txid;
                    }
                },
            )
            .map(|_| sent)
        })
    };
    for _ in 0..60 {
        if sender.is_finished() {
            break;
        }
        let _ = proposals::answer_signing_requests(
            relay.clone(),
            lwd.clone(),
            a.db_dir.clone(),
            a.db_key.clone(),
            a.state_dir.clone(),
            a.seeds.clone(),
            a.material.clone(),
            None,
        );
        thread::sleep(Duration::from_millis(500));
    }
    let swept = sender.join().unwrap().expect("sweep send").expect("txid");
    println!("sweep broadcast {swept}");
    let s = list(&members[2])
        .into_iter()
        .find(|p| p.id == sweep)
        .unwrap();
    assert_eq!(s.stage, ProposalStage::Sent);

    // History export (CSV): both sent payments with payee, amount, fee and memo, plus the
    // faucet payments received. A signer named on this device shows as "Name (hexkey)".
    let a_key = rust_lib_zafe::api::vault::identity_public_key(a.seeds.clone()).unwrap();
    let csv = rust_lib_zafe::api::history::export_history_csv(
        relay.clone(),
        a.db_dir.clone(),
        a.db_key.clone(),
        a.seeds.clone(),
        a.material.clone(),
        vec![rust_lib_zafe::api::names::SignerName {
            key_hex: a_key.clone(),
            name: "Alice".into(),
        }],
    )
    .unwrap();
    assert!(csv.contains(&format!("Alice ({a_key})")), "{csv}");
    let lines: Vec<&str> = csv.lines().collect();
    assert!(lines[0].starts_with("date,txid,direction,counterparty"));
    let sent: Vec<&&str> = lines.iter().filter(|l| l.contains(",sent,")).collect();
    assert_eq!(sent.len(), 2, "{csv}");
    assert!(sent.iter().any(|l| l.contains(&txid)
        && l.contains(&payee)
        && l.contains(",-1.00000000,0.00010000,grant #1,")));
    assert!(sent
        .iter()
        .any(|l| l.contains(&txid2) && l.contains(",-0.50000000,")));
    assert_eq!(
        lines.iter().filter(|l| l.contains(",received,")).count(),
        FUND_NOTES as usize,
        "{csv}"
    );

    // --- Pending incoming payments. A second vault (2-of-2: D creates, E joins) is paid by
    // the first one. E's app watches lightwalletd's mempool, so the payment is listed as
    // received (unmined) before any block includes it.
    let seeds2: Vec<Vec<u8>> = (0..2).map(|_| vault::generate_identity().seeds).collect();
    let invite2 =
        vault::create_vault(relay.clone(), seeds2[0].clone(), "Payroll".into(), 2, 2).unwrap();
    vault::join_vault(relay.clone(), seeds2[1].clone(), invite2.clone()).unwrap();
    vault::seal_vault(relay.clone(), seeds2[0].clone(), invite2.clone()).unwrap();
    let safety2 = vault::vault_membership(relay.clone(), seeds2[0].clone(), invite2.clone())
        .unwrap()
        .safety_number;
    // The creator picks lightwalletd's tip + 1 as the birthday (as the app does).
    let handles: Vec<_> = seeds2
        .iter()
        .zip(["v2-a", "v2-e"])
        .map(|(s, who)| {
            let (relay, lwd, s, invite, sn) = (
                relay.clone(),
                lwd.clone(),
                s.clone(),
                invite2.clone(),
                safety2.clone(),
            );
            let state = tmp.join(who).join("signing");
            thread::spawn(move || {
                let state = Some(state.to_string_lossy().into());
                vault::run_keygen(
                    relay,
                    lwd,
                    "regtest".into(),
                    s,
                    invite,
                    sn,
                    120,
                    None,
                    None,
                    state,
                )
                .unwrap()
            })
        })
        .collect();
    let materials2: Vec<Vec<u8>> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    let summary2 = vault::vault_summary(materials2[1].clone()).unwrap();
    let e = {
        let dir = tmp.join("v2-e");
        let state = dir.join("signing");
        std::fs::create_dir_all(&state).unwrap();
        Member {
            seeds: seeds2[1].clone(),
            material: materials2[1].clone(),
            db_dir: dir.to_string_lossy().into(),
            db_key: rand::random::<[u8; 32]>().to_vec(),
            state_dir: state.to_string_lossy().into(),
        }
    };

    // Past vault 2's birthday; everyone synced (vault 1's earlier sends mine here too).
    chain.mine(3);
    let tip = after.height + 6;
    for m in members.iter().chain([&e]) {
        let mut b = sync(m);
        for _ in 0..60 {
            if b.height >= tip {
                break;
            }
            thread::sleep(Duration::from_secs(1));
            b = sync(m);
        }
        assert!(b.height >= tip, "member did not reach {tip}");
    }
    assert!(received_list(&e).is_empty());

    // E's watch, as the app runs it while in the foreground.
    let watch_id = rust_lib_zafe::api::mempool::begin_mempool_watch();
    let (events_tx, events) = std::sync::mpsc::channel();
    let watcher = {
        let (lwd, e) = (lwd.clone(), e.clone());
        thread::spawn(move || {
            rust_lib_zafe::api::mempool::watch_mempool_with(
                watch_id,
                lwd,
                e.db_dir,
                e.db_key,
                e.material,
                move |ev| {
                    println!("mempool {:?} {:?}", ev.status, ev.txid);
                    let _ = events_tx.send((ev.status, ev.txid));
                },
            )
        })
    };
    use rust_lib_zafe::api::mempool::MempoolStatus;
    let (status, _) = events
        .recv_timeout(Duration::from_secs(30))
        .expect("mempool watch never connected");
    assert_eq!(status, MempoolStatus::Connected);

    // Vault 1 pays vault 2: A proposes, A and B approve, B sends (one tap, or interactive
    // with A answering).
    for m in &members {
        list(m); // tops up commitment pools
    }
    let id4 = proposals::propose_payment(
        relay.clone(),
        lwd.clone(),
        a.db_dir.clone(),
        a.db_key.clone(),
        a.seeds.clone(),
        a.material.clone(),
        vec![PaymentInput {
            address: summary2.address.clone(),
            amount_zat: 20_000_000,
            memo: "payroll top-up".into(),
        }],
        true,
    )
    .unwrap();
    for m in [a, b] {
        proposals::approve_proposal(
            relay.clone(),
            lwd.clone(),
            m.db_dir.clone(),
            m.db_key.clone(),
            m.state_dir.clone(),
            m.seeds.clone(),
            m.material.clone(),
            id4.clone(),
        )
        .unwrap();
    }
    let sender = {
        let (relay, lwd, b) = (relay.clone(), lwd.clone(), b.clone());
        let id4 = id4.clone();
        thread::spawn(move || {
            let mut sent = None;
            proposals::send_with_progress(
                relay,
                lwd,
                b.db_dir,
                b.db_key,
                b.state_dir,
                b.seeds,
                b.material,
                id4,
                Duration::from_secs(90),
                |p| {
                    if p.stage == SendStage::Sent {
                        sent = p.txid;
                    }
                },
            )
            .map(|_| sent)
        })
    };
    for _ in 0..240 {
        if sender.is_finished() {
            break;
        }
        let _ = proposals::answer_signing_requests(
            relay.clone(),
            lwd.clone(),
            a.db_dir.clone(),
            a.db_key.clone(),
            a.state_dir.clone(),
            a.seeds.clone(),
            a.material.clone(),
            None,
        );
        thread::sleep(Duration::from_millis(500));
    }
    let paid = sender.join().unwrap().expect("payment send").expect("txid");
    println!("payment to vault 2 broadcast {paid}");

    // No block is mined: the watch alone must bring it in.
    let stored = (|| {
        let deadline = std::time::Instant::now() + Duration::from_secs(60);
        while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
            match events.recv_timeout(left) {
                Ok((MempoolStatus::Stored, Some(t))) if t == paid => return true,
                Ok(_) => {}
                Err(_) => return false,
            }
        }
        false
    })();
    assert!(stored, "the mempool watch never stored {paid}");
    let pending = received_list(&e);
    let r = pending
        .iter()
        .find(|r| r.txid == paid)
        .expect("pending payment listed");
    assert_eq!(r.mined_height, 0, "listed before it is mined");
    assert_eq!(r.confirmations, 0);
    assert_eq!(r.amount_zat, 20_000_000);
    assert_eq!(r.memo, "payroll top-up");
    assert!(!r.is_coinbase);

    // Stopping ends the watch promptly.
    rust_lib_zafe::api::mempool::stop_mempool_watch();
    let stop_started = std::time::Instant::now();
    while !watcher.is_finished() && stop_started.elapsed() < Duration::from_secs(5) {
        thread::sleep(Duration::from_millis(50));
    }
    assert!(watcher.is_finished(), "the watch did not stop");
    watcher.join().unwrap().unwrap();

    // Once mined, block sync gives it a height.
    chain.mine(1);
    let mut mined = None;
    for _ in 0..30 {
        sync(&e);
        mined = received_list(&e)
            .into_iter()
            .find(|r| r.txid == paid && r.mined_height > 0);
        if mined.is_some() {
            break;
        }
        thread::sleep(Duration::from_secs(1));
    }
    assert_eq!(mined.expect("payment mined").confirmations, 1);

    // Backup health: A exports a backup (checked to open) and attests it; every member
    // sees it, and attesting again doesn't grow the log. Last, because listing proposals
    // tops up the one-tap pools, which would change the signing flows above.
    let backed_up = |m: &Member| {
        proposals::list_proposals(
            relay.clone(),
            m.state_dir.clone(),
            m.seeds.clone(),
            m.material.clone(),
            None,
        )
        .unwrap()
        .backed_up
    };
    assert!(backed_up(&members[1]).is_empty());
    let a = &members[0];
    let exported = backup::export_vault_backup(
        a.seeds.clone(),
        a.material.clone(),
        invite.clone(),
        vec![],
        "correct horse battery staple orbit lantern violet harbor cactus".into(),
    )
    .unwrap();
    assert!(exported.text.starts_with("zafe-backup-v1:"));
    for _ in 0..2 {
        proposals::attest_backup(relay.clone(), a.seeds.clone(), a.material.clone()).unwrap();
    }
    let a_key = vault::identity_public_key(a.seeds.clone()).unwrap();
    for m in &members {
        assert_eq!(backed_up(m), vec![a_key.clone()]);
    }

    let _ = std::fs::remove_dir_all(&tmp);
}
