//! Seat moves against a dishonest member and after a second loss (security review
//! 2026-11-13, findings A, B and E; docs/security-review-2026-11.md "Seat moves"). Every
//! test here fails on 8bdef51, each for the reason its comment states. Setup helpers are
//! copied from `tests/repair.rs`. Helpers take their wait as a parameter: the real 72 h
//! where the test is about the wait, zero where it is about what follows it.

use std::{collections::BTreeMap, time::Duration};

use futures::FutureExt;
use pasta_curves::pallas;
use rand::{rngs::StdRng, RngCore, SeedableRng};
use zafe_core::{
    node::{
        create_vault, join_vault, load_state, run_keygen, seal, set_name, KeygenOptions,
        VaultMaterial,
    },
    relay_client::RelayClient,
    repair::{
        approve_replacement, help_repairs, keep_seat, new_phone_words, recovery_checks,
        try_recover, RecoveryRequest, RecoveryStatus, SEAT_MOVE_WAIT,
    },
    signing,
    wallet::regtest_network,
};
use zafe_proto::{relay::SeatMoveApproval, Identity};
use zafe_relay::limits::Limits;

async fn start_relay() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            listener,
            zafe_relay::Relay::new()
                .with_limits(Limits::hosted())
                .router(),
        )
        .await
        .unwrap();
    });
    format!("http://{addr}")
}

/// A 2-of-`n` vault over the relay: the members' identities and material.
async fn vault(relay: &RelayClient, n: u16) -> (Vec<Identity>, Vec<VaultMaterial>) {
    let mut rng = StdRng::seed_from_u64(70);
    let ids: Vec<Identity> = (0..n).map(|_| Identity::generate(&mut rng)).collect();
    let invite = create_vault(relay, &ids[0], "Grants", 2, n, &mut rng)
        .await
        .unwrap();
    for id in &ids[1..] {
        join_vault(relay, id, &invite).await.unwrap();
    }
    seal(relay, &ids[0], &invite).await.unwrap();
    let number = futures::future::join_all(ids.iter().map(|id| safety_words(relay, id, &invite)))
        .await
        .into_iter()
        .map(Result::unwrap)
        .reduce(|a, b| {
            assert_eq!(a, b, "every member sees the same safety words");
            a
        })
        .unwrap();
    let net = regtest_network();
    let timeout = Duration::from_secs(60);
    let mut rngs: Vec<StdRng> = (0..n)
        .map(|i| StdRng::seed_from_u64(i as u64 + 1))
        .collect();
    // The creator picks the birthday (regtest isn't running here).
    let runs = ids
        .iter()
        .zip(rngs.iter_mut())
        .enumerate()
        .map(|(i, (id, rng))| {
            let birthday = (i == 0).then_some(2);
            run_keygen(
                relay,
                id,
                &invite,
                &number,
                &net,
                "regtest",
                KeygenOptions {
                    creator_birthday_height: birthday,
                    creator_expiry_blocks: None,
                    max_birthday: None,
                },
                member_dir(relay, id),
                setup_key(id),
                rng,
                timeout,
            )
        });
    let materials = futures::future::join_all(runs)
        .await
        .into_iter()
        .map(Result::unwrap)
        .collect();
    (ids, materials)
}

/// Two members sign a message with their key packages; aggregation verifies every share
/// and the signature under the group key.
fn sign_together(a: &VaultMaterial, b: &VaultMaterial, rng: &mut StdRng) {
    let alpha = pallas::Scalar::from(7u64);
    let (ka, kb) = (a.key_package().unwrap(), b.key_package().unwrap());
    let (na, ca) = signing::commit(&ka, rng);
    let (nb, cb) = signing::commit(&kb, rng);
    let package = signing::signing_package(
        BTreeMap::from([(*ka.identifier(), ca), (*kb.identifier(), cb)]),
        &[9; 32],
    );
    let shares = BTreeMap::from([
        (
            *ka.identifier(),
            signing::sign(&package, na, &ka, alpha).unwrap(),
        ),
        (
            *kb.identifier(),
            signing::sign(&package, nb, &kb, alpha).unwrap(),
        ),
    ]);
    signing::aggregate(&package, &shares, &b.public_key_package().unwrap(), alpha)
        .expect("a valid signature from the two shares");
}

/// The safety check between a co-signer and the new phone, step by step as the two apps
/// poll it: both end with the same words, which the co-signer then approves with.
async fn compare_words(relay: &RelayClient, cosigner: &Identity, new: &Identity) -> String {
    let mut words = None;
    for _ in 0..4 {
        let shown_new = recovery_checks(relay, new, &pins()).await.unwrap();
        if let Some(w) = new_phone_words(relay, cosigner, new.public())
            .await
            .unwrap()
        {
            words = Some((w, shown_new));
            break;
        }
    }
    let (w, _) = words.expect("the check finishes in a few polls");
    let shown = recovery_checks(relay, new, &pins()).await.unwrap();
    let on_new = shown
        .iter()
        .find(|c| c.cosigner == cosigner.public().sig_pk)
        .expect("the new phone shows this co-signer's words");
    assert_eq!(on_new.words, w, "both phones show the same words");
    assert_eq!(w.split(' ').count(), 4);
    w
}

/// The membership check is cached per (relay URL, label) for the whole process, for every
/// member; a client spelled differently reads the relay afresh.
fn uncached(relay: &RelayClient) -> RelayClient {
    RelayClient::new(relay.base_url().replace("127.0.0.1", "localhost"))
}

/// Finding A. 2-of-3: Alice, Bob, Carol (dishonest). Bob's phone works. Carol makes a
/// fresh identity X and tells Alice "Bob lost his phone"; Alice compares words with X
/// (Carol). Today Carol's and Alice's approvals move Bob's seat at once and the helpers
/// rebuild Bob's share onto X: Carol holds two shares and signs alone.
///
/// Wanted: the t-th approval leaves the move pending (Bob stays a member, X isn't one), no
/// helper sends X anything before its wait, and Bob's `KeepSeat` then voids the move, so X
/// never gets Bob's share here, however often the helpers run.
#[tokio::test]
async fn a_live_seat_owner_keeps_its_seat() {
    let relay = RelayClient::new(start_relay().await);
    let (ids, materials) = vault(&relay, 3).await;
    let (a, b, c) = (&materials[0], &materials[1], &materials[2]);
    let mut rng = StdRng::seed_from_u64(1337);
    let (alice, bob, carol) = (
        ids[0].public().sig_pk,
        ids[1].public().sig_pk,
        ids[2].public().sig_pk,
    );
    let x = Identity::generate(&mut rng);
    let request = RecoveryRequest {
        identity: *x.public(),
    };

    // Carol approves first (she holds X).
    let words_c = compare_words(&relay, &ids[2], &x).await;
    let _ = approve_replacement(&relay, &ids[2], c, bob, &request, &words_c, &mut rng).await;

    // Alice, fooled, approves too (refusing here is also a pass).
    let words_a = compare_words(&relay, &ids[0], &x).await;
    let _ = approve_replacement(&relay, &ids[0], a, bob, &request, &words_a, &mut rng).await;

    let (chain, state) = load_state(&uncached(&relay), &ids[0], a).await.unwrap();
    assert!(
        state.descriptor.member(&bob).is_some(),
        "the t-th approval must leave a live owner's seat where it is"
    );
    assert!(state.descriptor.member(&x.public().sig_pk).is_none());
    assert!(
        state.pending_seat_move(&bob).is_some(),
        "the move is pending"
    );
    load_state(&uncached(&relay), &ids[1], b)
        .await
        .expect("Bob still reads his vault");

    // Within the wait the helpers send nothing that completes anything.
    let dir = tempfile::tempdir().unwrap().keep();
    for _ in 0..3 {
        let _ = help_repairs(
            &relay,
            &ids[0],
            a,
            &chain,
            &state,
            &dir.join("a"),
            SEAT_MOVE_WAIT,
            &mut rng,
        )
        .await;
        let _ = help_repairs(
            &relay,
            &ids[2],
            c,
            &chain,
            &state,
            &dir.join("c"),
            SEAT_MOVE_WAIT,
            &mut rng,
        )
        .await;
    }
    let (_, waiting) = load_state(&uncached(&relay), &ids[0], a).await.unwrap();
    assert!(waiting.pending_seat_move(&bob).unwrap().helped.is_empty());

    // Bob taps Keep my seat: the pending move is void, in the log and on the relay.
    assert!(keep_seat(&relay, &ids[1], b, &mut rng).await.unwrap());
    let (_, kept) = load_state(&uncached(&relay), &ids[0], a).await.unwrap();
    assert!(
        kept.pending_seat_move(&bob).is_none(),
        "KeepSeat voids the move"
    );
    assert!(kept.descriptor.member(&bob).is_some());
    assert_eq!(kept.seat_round(&bob), 1);
    let info = relay.mailbox_info(&ids[0], a.label()).await.unwrap();
    assert!(info.pending.is_empty(), "the relay dropped X");

    // And X never gets Bob's share, however often the helpers run, even past the wait.
    for _ in 0..3 {
        let _ = help_repairs(
            &relay,
            &ids[0],
            a,
            &chain,
            &state,
            &dir.join("a"),
            Duration::ZERO,
            &mut rng,
        )
        .await;
        let _ = help_repairs(
            &relay,
            &ids[2],
            c,
            &chain,
            &state,
            &dir.join("c"),
            Duration::ZERO,
            &mut rng,
        )
        .await;
    }
    let (_, after) = load_state(&uncached(&relay), &ids[0], a).await.unwrap();
    assert!(after.descriptor.member(&bob).is_some());
    assert!(after.descriptor.member(&x.public().sig_pk).is_none());
    let status = try_recover(&relay, &x, Some(c.label()), &[alice, carol]).await;
    assert!(
        !matches!(status, Ok(RecoveryStatus::Done { .. })),
        "X must not receive Bob's share"
    );
}

/// Finding A, the window: the t-th approval alone doesn't move the seat, so the owner's
/// phone can still answer. An ordinary write by the old key (here a name) doesn't void it:
/// only `KeepSeat` does, so a lost phone that is still online can't block a recovery.
#[tokio::test]
async fn the_last_approval_doesnt_move_a_seat_at_once() {
    let relay = RelayClient::new(start_relay().await);
    let (ids, materials) = vault(&relay, 3).await;
    let (a, b, c) = (&materials[0], &materials[1], &materials[2]);
    let mut rng = StdRng::seed_from_u64(1338);
    let bob = ids[1].public().sig_pk;
    let x = Identity::generate(&mut rng);
    let request = RecoveryRequest {
        identity: *x.public(),
    };
    let words_c = compare_words(&relay, &ids[2], &x).await;
    let _ = approve_replacement(&relay, &ids[2], c, bob, &request, &words_c, &mut rng).await;
    let words_a = compare_words(&relay, &ids[0], &x).await;
    let _ = approve_replacement(&relay, &ids[0], a, bob, &request, &words_a, &mut rng).await;

    let (_, state) = load_state(&uncached(&relay), &ids[0], a).await.unwrap();
    assert!(
        state.descriptor.member(&bob).is_some(),
        "the seat must not move the moment the t-th approval lands"
    );
    assert!(state.descriptor.member(&x.public().sig_pk).is_none());
    // The owner can still write in the window, and an ordinary entry leaves the move
    // pending; only `KeepSeat` voids it.
    set_name(&relay, &ids[1], b, "Bob", &mut rng)
        .await
        .expect("the owner can still write");
    let (_, state) = load_state(&uncached(&relay), &ids[0], a).await.unwrap();
    assert!(state.descriptor.member(&bob).is_some());
    assert!(
        state.pending_seat_move(&bob).is_some(),
        "an ordinary entry by the old key doesn't void the move"
    );
    assert!(keep_seat(&relay, &ids[1], b, &mut rng).await.unwrap());
    let (_, state) = load_state(&uncached(&relay), &ids[0], a).await.unwrap();
    assert!(state.pending_seat_move(&bob).is_none());
    assert!(state.descriptor.member(&bob).is_some());
}

/// Finding B. 2-of-4: Alice, Bob, Mallory (dishonest), Lee (lost phone). Alice approves
/// moving Lee's seat to N (pending: one approval). Mallory takes Alice's signature from the
/// log, adds her own and sends both to the relay only: the relay (blind, counts current
/// members' signatures) swaps Lee for N while the log never moved the seat. Today every
/// member's `load_state` then fails with `RelayMembership`, and nobody can approve the
/// move in the log any more (that needs `load_state`).
///
/// Wanted: honest members keep working (the relay refuses, or its extra member is a
/// pending move), and Bob can still approve the move in the log.
#[tokio::test]
async fn a_seat_move_made_only_on_the_relay_doesnt_brick_the_vault() {
    let relay = RelayClient::new(start_relay().await);
    let (ids, materials) = vault(&relay, 4).await;
    let mut rng = StdRng::seed_from_u64(1339);
    let (alice, mallory, lee) = (
        ids[0].public().sig_pk,
        ids[2].public().sig_pk,
        ids[3].public().sig_pk,
    );
    let n = Identity::generate(&mut rng);
    let request = RecoveryRequest {
        identity: *n.public(),
    };
    let words_a = compare_words(&relay, &ids[0], &n).await;
    assert!(!approve_replacement(
        &relay,
        &ids[0],
        &materials[0],
        lee,
        &request,
        &words_a,
        &mut rng
    )
    .await
    .unwrap());

    // Mallory: Alice's logged signature plus her own, to the relay only.
    let (_, state) = load_state(&relay, &ids[2], &materials[2]).await.unwrap();
    let mut approvals = state.pending_replacements[&(lee, n.public().sig_pk)]
        .approvals
        .clone();
    assert_eq!(approvals[0].0, alice);
    let approval = SeatMoveApproval {
        vault_id: materials[2].descriptor.vault_id,
        old: lee,
        new: *n.public(),
        round: 0,
    };
    approvals.push((mallory, approval.sign(&ids[2]).unwrap()));
    let _ = relay
        .replace_member_at(&ids[2], materials[2].label(), approval, approvals)
        .await;

    for i in [0, 1] {
        let r = load_state(&uncached(&relay), &ids[i], &materials[i]).await;
        assert!(r.is_ok(), "member {i} is locked out: {:?}", r.err());
    }
    let on_relay = relay
        .mailbox_info(&ids[0], materials[0].label())
        .await
        .unwrap();
    assert!(
        on_relay.members.iter().any(|m| m.sig_pk == lee),
        "approvals alone never take the old key off the relay"
    );
    let words_b = compare_words(&relay, &ids[1], &n).await;
    approve_replacement(
        &relay,
        &ids[1],
        &materials[1],
        lee,
        &request,
        &words_b,
        &mut rng,
    )
    .await
    .expect("Bob can approve the move in the log");
}

/// Finding E. 2-of-4: Alice, Bob, Carol, Dave. Carol loses her phone; Alice and Bob move
/// her seat to C' (helpers [Alice, Bob], attempt 0). Bob's seat then moves to B' (Bob lost
/// his phone too; Alice and Dave approve) while Alice is mid-repair: she saved her part 1
/// for [Alice, Bob] from a state read before Bob's move and sent its delta to Bob's old
/// key. On 8bdef51 `move_seat` rekeys Carol's repair to [Alice, B'] without a new attempt,
/// so Alice's saved part 1 (keyed Alice, Bob) is reused and `help_repairs` panics at
/// `hs.deltas[&B']`; B' could never read Alice's delta (sealed to Bob) and the repair
/// would wait forever. (There the first run also failed to reach Bob, who left the relay
/// at the second approval; with pending moves he stays until his move finishes.)
///
/// Wanted: no panic; a helper's seat moving restarts that repair (new attempt), and both
/// new phones end up with working shares.
#[tokio::test]
async fn a_repair_survives_a_helpers_seat_moving() {
    let relay = RelayClient::new(start_relay().await);
    let (ids, materials) = vault(&relay, 4).await;
    let (a, d) = (&materials[0], &materials[3]);
    let mut rng = StdRng::seed_from_u64(1340);
    let pk = |i: usize| ids[i].public().sig_pk;
    let label = Some(a.label());
    let (c_new, b_new) = (Identity::generate(&mut rng), Identity::generate(&mut rng));

    // Carol's seat moves to C' (Alice and Bob approve).
    let c_request = RecoveryRequest {
        identity: *c_new.public(),
    };
    for i in [0, 1] {
        let words = compare_words(&relay, &ids[i], &c_new).await;
        approve_replacement(
            &relay,
            &ids[i],
            &materials[i],
            pk(2),
            &c_request,
            &words,
            &mut rng,
        )
        .await
        .unwrap();
    }
    // Alice reads the vault; Bob's seat moves (Alice and Dave approve) before she acts.
    let (chain, before) = load_state(&relay, &ids[0], a).await.unwrap();
    let b_request = RecoveryRequest {
        identity: *b_new.public(),
    };
    for i in [0, 3] {
        let words = compare_words(&relay, &ids[i], &b_new).await;
        approve_replacement(
            &relay,
            &ids[i],
            &materials[i],
            pk(1),
            &b_request,
            &words,
            &mut rng,
        )
        .await
        .unwrap();
    }
    let dir = tempfile::tempdir().unwrap().keep();
    // Part 1 for [Alice, Bob] is saved and its delta goes to Bob's old key.
    let _ = help_repairs(
        &relay,
        &ids[0],
        a,
        &chain,
        &before,
        &dir.join("a"),
        Duration::ZERO,
        &mut rng,
    )
    .await;

    // Everyone runs as their apps would, until both new phones have their keys.
    let (mut b_mat, mut c_mat): (Option<VaultMaterial>, Option<VaultMaterial>) = (None, None);
    for _ in 0..8 {
        let mut helpers: Vec<(&Identity, VaultMaterial, &str)> =
            vec![(&ids[0], a.clone(), "a"), (&ids[3], d.clone(), "d")];
        if let Some(m) = &b_mat {
            helpers.push((&b_new, m.clone(), "b2"));
        }
        for (id, m, name) in helpers {
            let Ok((chain, state)) = load_state(&relay, id, &m).await else {
                continue;
            };
            let run = std::panic::AssertUnwindSafe(help_repairs(
                &relay,
                id,
                &m,
                &chain,
                &state,
                &dir.join(name),
                Duration::ZERO,
                &mut rng,
            ))
            .catch_unwind()
            .await;
            assert!(run.is_ok(), "help_repairs panicked for {name}");
        }
        if b_mat.is_none() {
            if let Ok(RecoveryStatus::Done { material, .. }) =
                try_recover(&relay, &b_new, label, &[pk(0), pk(3)]).await
            {
                b_mat = Some(*material);
            }
        }
        if c_mat.is_none() {
            match try_recover(&relay, &c_new, label, &[pk(0), pk(1)]).await {
                Ok(RecoveryStatus::Done { material, .. }) => c_mat = Some(*material),
                Ok(_) => {}
                Err(e) => panic!("C' can't recover: {e:?}"),
            }
        }
        if b_mat.is_some() && c_mat.is_some() {
            break;
        }
    }
    let b_mat = b_mat.expect("B' recovers");
    let c_mat = c_mat.expect("C' recovers after the helper change");
    let (_, state) = load_state(&relay, &ids[0], a).await.unwrap();
    let r = state.replacement_to(&c_new.public().sig_pk).unwrap();
    assert_eq!(
        (r.attempt, r.helpers.clone()),
        (1, vec![pk(0), b_new.public().sig_pk]),
        "Bob's seat moving restarted Carol's repair under B'"
    );
    sign_together(&b_mat, a, &mut rng);
    sign_together(&c_mat, a, &mut rng);
}

/// One member's setup state dir in this test's relay: the safety check and key generation
/// share it (the safety value lives there).
fn member_dir(
    relay: &zafe_core::relay_client::RelayClient,
    id: &zafe_proto::Identity,
) -> std::path::PathBuf {
    let port = relay
        .base_url()
        .rsplit(':')
        .next()
        .unwrap_or_default()
        .to_owned();
    std::env::temp_dir().join(format!(
        "zafe-setup-{}-{port}-{}",
        std::process::id(),
        hex::encode(&id.public().sig_pk[..8])
    ))
}

/// The key that seals one member's setup state (any fixed key per member will do).
fn setup_key(id: &zafe_proto::Identity) -> zafe_core::nonce_store::NonceKey {
    zafe_core::nonce_store::NonceKey::derive(&id.public().sig_pk, b"setup")
}

/// One member's safety words for `invite`, polled from its setup state dir.
async fn safety_words(
    relay: &zafe_core::relay_client::RelayClient,
    id: &zafe_proto::Identity,
    invite: &zafe_core::node::Invite,
) -> Result<String, zafe_core::node::NodeError> {
    zafe_core::node::wait_for_safety_words(
        relay,
        id,
        invite,
        &member_dir(relay, id),
        &setup_key(id),
        std::time::Duration::from_secs(30),
    )
    .await
}

/// Where new phones in these tests keep their seat recovery pins (one file per identity
/// and co-signer, so one directory serves every test).
fn pins() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("zafe-recovery-pins-{}", std::process::id()))
}

/// Approvals from `approvers` moving `old`'s seat to `new`, each after comparing words.
async fn approve_all(
    relay: &RelayClient,
    ids: &[Identity],
    materials: &[VaultMaterial],
    approvers: &[usize],
    old: [u8; 32],
    new: &Identity,
    rng: &mut StdRng,
) -> bool {
    let request = RecoveryRequest {
        identity: *new.public(),
    };
    let mut pending = false;
    for &i in approvers {
        let words = compare_words(relay, &ids[i], new).await;
        pending = approve_replacement(relay, &ids[i], &materials[i], old, &request, &words, rng)
            .await
            .unwrap();
    }
    pending
}

/// One helper's refresh as the app runs it (read the log, then do its part).
async fn help(
    relay: &RelayClient,
    id: &Identity,
    m: &VaultMaterial,
    dir: &std::path::Path,
    wait: Duration,
    rng: &mut StdRng,
) -> zafe_core::repair::RepairReport {
    let (chain, state) = load_state(relay, id, m).await.unwrap();
    help_repairs(relay, id, m, &chain, &state, dir, wait, rng)
        .await
        .unwrap()
}

/// The wait runs on each helper's own clock from when its phone first saw the move
/// pending: the deltas go at once, the part that completes the move only after the wait,
/// and a record the phone lost starts the wait again.
#[tokio::test]
async fn a_helper_waits_from_when_it_first_saw_the_move() {
    let relay = RelayClient::new(start_relay().await);
    let (ids, m) = vault(&relay, 3).await;
    let mut rng = StdRng::seed_from_u64(1341);
    let x = Identity::generate(&mut rng);
    let old = ids[0].public().sig_pk;
    assert!(approve_all(&relay, &ids, &m, &[1, 2], old, &x, &mut rng).await);
    let dir = tempfile::tempdir().unwrap();
    let wait = Duration::from_secs(3);
    let first = help(&relay, &ids[1], &m[1], dir.path(), wait, &mut rng).await;
    assert_eq!((first.deltas_sent, first.held, first.helped), (1, 1, 0));
    tokio::time::sleep(Duration::from_millis(3200)).await;
    // The record went missing (a reinstall, a wiped directory): the wait starts over.
    for f in std::fs::read_dir(dir.path()).unwrap() {
        let f = f.unwrap().path();
        if f.extension().is_some_and(|e| e == "seen") {
            std::fs::remove_file(f).unwrap();
        }
    }
    let again = help(&relay, &ids[1], &m[1], dir.path(), wait, &mut rng).await;
    assert_eq!((again.held, again.helped), (1, 0));
    tokio::time::sleep(Duration::from_millis(3200)).await;
    let done = help(&relay, &ids[1], &m[1], dir.path(), wait, &mut rng).await;
    assert_eq!((done.held, done.helped), (0, 1));
    let (_, state) = load_state(&relay, &ids[1], &m[1]).await.unwrap();
    assert_eq!(
        state.pending_seat_move(&old).unwrap().helped,
        vec![ids[1].public().sig_pk]
    );
}

/// A pending key the log never approved (two members sign a move and give it to the relay
/// alone) is tolerated and gets nothing: members still load (it can't write, and the
/// member list matches the log), and no honest phone sends it anything.
#[tokio::test]
async fn a_pending_key_the_log_never_approved_gets_nothing() {
    let relay = RelayClient::new(start_relay().await);
    let (ids, m) = vault(&relay, 3).await;
    let mut rng = StdRng::seed_from_u64(1342);
    let z = Identity::generate(&mut rng);
    let approval = SeatMoveApproval {
        vault_id: m[0].descriptor.vault_id,
        old: ids[0].public().sig_pk,
        new: *z.public(),
        round: 0,
    };
    let approvals = [1, 2]
        .iter()
        .map(|&i| (ids[i].public().sig_pk, approval.sign(&ids[i]).unwrap()))
        .collect();
    relay
        .replace_member_at(&ids[1], m[1].label(), approval, approvals)
        .await
        .unwrap();
    assert_nothing_for(&relay, &ids, &m, &z, &mut rng).await;
}

/// Every member loads and runs its refresh (wait over); the pending key `z` receives
/// nothing and recovers nothing.
async fn assert_nothing_for(
    relay: &RelayClient,
    ids: &[Identity],
    m: &[VaultMaterial],
    z: &Identity,
    rng: &mut StdRng,
) {
    let dir = tempfile::tempdir().unwrap();
    for _ in 0..2 {
        for i in 0..ids.len() {
            load_state(&uncached(relay), &ids[i], &m[i])
                .await
                .unwrap_or_else(|e| panic!("member {i} is locked out: {e:?}"));
            let _ = try_help(
                relay,
                &ids[i],
                &m[i],
                &dir.path().join(i.to_string()),
                Duration::ZERO,
                rng,
            )
            .await;
        }
    }
    let inbox = relay.inbox(z, m[0].label(), 0).await.unwrap();
    assert!(
        inbox.is_empty(),
        "honest phones sent it {} message(s)",
        inbox.len()
    );
    assert!(!matches!(
        try_recover(relay, z, Some(m[0].label()), &[ids[1].public().sig_pk]).await,
        Ok(RecoveryStatus::Done { .. })
    ));
}

/// Finding A, the second round: after the owner kept the seat, the co-signers may ask
/// again (round 1); the old phone can't keep it twice, approvers see when it was kept,
/// and after the wait the move completes and the new phone recovers.
#[tokio::test]
async fn a_second_round_moves_the_seat_after_a_keep() {
    let relay = RelayClient::new(start_relay().await);
    let (ids, m) = vault(&relay, 3).await;
    let mut rng = StdRng::seed_from_u64(1343);
    let (old, new) = (ids[0].public().sig_pk, Identity::generate(&mut rng));
    assert!(approve_all(&relay, &ids, &m, &[1, 2], old, &new, &mut rng).await);
    assert!(keep_seat(&relay, &ids[0], &m[0], &mut rng).await.unwrap());
    let (_, state) = load_state(&relay, &ids[1], &m[1]).await.unwrap();
    assert_eq!(state.seat_round(&old), 1);
    assert!(
        state.seat_keeps[&old][0].at > 0,
        "approvers see when it was kept"
    );

    assert!(approve_all(&relay, &ids, &m, &[1, 2], old, &new, &mut rng).await);
    assert!(matches!(
        keep_seat(&relay, &ids[0], &m[0], &mut rng).await,
        Err(zafe_core::node::NodeError::NotReady(_))
    ));
    let dir = tempfile::tempdir().unwrap();
    let confirmed = [ids[1].public().sig_pk];
    let mut done = None;
    for _ in 0..4 {
        for i in [1, 2] {
            help(
                &relay,
                &ids[i],
                &m[i],
                &dir.path().join(i.to_string()),
                Duration::ZERO,
                &mut rng,
            )
            .await;
        }
        if let Ok(RecoveryStatus::Done { material, .. }) =
            try_recover(&relay, &new, Some(m[1].label()), &confirmed).await
        {
            done = Some(material);
            break;
        }
    }
    let material = done.expect("the second round completes");
    assert_eq!(
        material.key_package().unwrap().signing_share(),
        m[0].key_package().unwrap().signing_share()
    );
    sign_together(&material, &m[1], &mut rng);
}

/// A dishonest helper sends its part before the switch (here: honest code fed a state where
/// the move already switched), the owner keeps the seat, and a second round asks for the
/// same new phone: the first round's part (same helper, same attempt number) must not stand
/// in for the second's, or the new phone waits forever.
#[tokio::test]
async fn a_second_round_to_the_same_phone_after_a_part_sent_early() {
    let relay = RelayClient::new(start_relay().await);
    let (ids, m) = vault(&relay, 3).await;
    let mut rng = StdRng::seed_from_u64(1344);
    let (old, new) = (ids[0].public().sig_pk, Identity::generate(&mut rng));
    assert!(approve_all(&relay, &ids, &m, &[1, 2], old, &new, &mut rng).await);
    let dir = tempfile::tempdir().unwrap();
    let (d1, d2) = (dir.path().join("1"), dir.path().join("2"));
    // Helper 2 is still in its wait (its delta goes anyway).
    let held = help(&relay, &ids[2], &m[2], &d2, SEAT_MOVE_WAIT, &mut rng).await;
    assert_eq!((held.deltas_sent, held.held), (1, 1));
    // Helper 1 cheats: it acts as if the seat had switched and sends its part.
    let (chain, mut forged) = load_state(&relay, &ids[1], &m[1]).await.unwrap();
    forged.replacements[0].switched = true;
    let sent = help_repairs(
        &relay,
        &ids[1],
        &m[1],
        &chain,
        &forged,
        &d1,
        Duration::ZERO,
        &mut rng,
    )
    .await
    .unwrap();
    assert_eq!(sent.sigmas_sent, 1);
    // The owner keeps the seat: void in the log (the early part alone gives nothing).
    assert!(keep_seat(&relay, &ids[0], &m[0], &mut rng).await.unwrap());
    assert!(approve_all(&relay, &ids, &m, &[1, 2], old, &new, &mut rng).await);
    let confirmed = [ids[1].public().sig_pk];
    let mut done = None;
    for _ in 0..4 {
        for (i, d) in [(1, &d1), (2, &d2)] {
            help(&relay, &ids[i], &m[i], d, Duration::ZERO, &mut rng).await;
        }
        if let Ok(RecoveryStatus::Done { material, .. }) =
            try_recover(&relay, &new, Some(m[1].label()), &confirmed).await
        {
            done = Some(material);
            break;
        }
    }
    let material = done.expect("the second round's parts are used");
    sign_together(&material, &m[2], &mut rng);
}

/// [`help`] that hands back the result (some steps are expected to fail).
async fn try_help(
    relay: &RelayClient,
    id: &Identity,
    m: &VaultMaterial,
    dir: &std::path::Path,
    wait: Duration,
    rng: &mut StdRng,
) -> Result<zafe_core::repair::RepairReport, zafe_core::node::NodeError> {
    let (chain, state) = load_state(relay, id, m).await?;
    help_repairs(relay, id, m, &chain, &state, dir, wait, rng).await
}

/// Coordinator fix 1: the sigma goes only once the seat switched. A co-helper that holds
/// back its `RepairHelped` (but sent its delta) can't make an honest helper leak the share
/// while the move still looks pending; the owner's keep is then the truth.
#[tokio::test]
async fn no_part_goes_before_the_switch() {
    let relay = RelayClient::new(start_relay().await);
    let (ids, m) = vault(&relay, 3).await;
    let mut rng = StdRng::seed_from_u64(1345);
    let (old, x) = (ids[0].public().sig_pk, Identity::generate(&mut rng));
    assert!(approve_all(&relay, &ids, &m, &[1, 2], old, &x, &mut rng).await);
    let dir = tempfile::tempdir().unwrap();
    // Helper 2 sends its delta and never logs its part.
    help(
        &relay,
        &ids[2],
        &m[2],
        &dir.path().join("2"),
        SEAT_MOVE_WAIT,
        &mut rng,
    )
    .await;
    // Honest helper 1, past its wait, logs its part and holds its sigma.
    let honest = help(
        &relay,
        &ids[1],
        &m[1],
        &dir.path().join("1"),
        Duration::ZERO,
        &mut rng,
    )
    .await;
    assert_eq!((honest.helped, honest.sigmas_sent), (1, 0));
    let again = help(
        &relay,
        &ids[1],
        &m[1],
        &dir.path().join("1"),
        Duration::ZERO,
        &mut rng,
    )
    .await;
    assert_eq!(again.sigmas_sent, 0);
    assert!(keep_seat(&relay, &ids[0], &m[0], &mut rng).await.unwrap());
    assert!(!matches!(
        try_recover(&relay, &x, Some(m[1].label()), &[ids[1].public().sig_pk]).await,
        Ok(RecoveryStatus::Done { .. })
    ));
}

/// Coordinator fix 2: a keep is one object. The old key cancels on the relay only; another
/// member's refresh logs the `KeepSeat` its signature is, and a second round can start.
#[tokio::test]
async fn a_relay_only_keep_reaches_the_log() {
    use zafe_proto::relay::SeatCancel;
    let relay = RelayClient::new(start_relay().await);
    let (ids, m) = vault(&relay, 3).await;
    let mut rng = StdRng::seed_from_u64(1346);
    let (old, x) = (ids[0].public().sig_pk, Identity::generate(&mut rng));
    assert!(approve_all(&relay, &ids, &m, &[1, 2], old, &x, &mut rng).await);
    let (_, state) = load_state(&relay, &ids[1], &m[1]).await.unwrap();
    let vault_id = state.descriptor.vault_id;
    let cancel = SeatCancel {
        mailbox: m[0].label(),
        vault_id,
        move_id: state
            .pending_seat_move(&old)
            .unwrap()
            .move_id(&vault_id)
            .unwrap(),
        old,
        new: x.public().sig_pk,
        at: 1_790_000_000,
    };
    relay
        .cancel_seat_move(cancel, cancel.sign(&ids[0]).unwrap())
        .await
        .unwrap();
    let _ = try_help(
        &relay,
        &ids[2],
        &m[2],
        tempfile::tempdir().unwrap().path(),
        Duration::ZERO,
        &mut rng,
    )
    .await;
    let (_, state) = load_state(&uncached(&relay), &ids[1], &m[1]).await.unwrap();
    assert!(
        state.pending_seat_move(&old).is_none(),
        "the keep is in the log"
    );
    assert_eq!(state.seat_round(&old), 1);
    assert!(approve_all(&relay, &ids, &m, &[1, 2], old, &x, &mut rng).await);
}

/// Coordinator fix 2, the other half: the old phone logged its keep and stopped before the
/// cancel; another member's refresh posts it, and the relay drops the pending key.
#[tokio::test]
async fn a_log_only_keep_reaches_the_relay() {
    use zafe_core::{node::append_event, vault::VaultEvent};
    use zafe_proto::relay::SeatCancel;
    let relay = RelayClient::new(start_relay().await);
    let (ids, m) = vault(&relay, 3).await;
    let mut rng = StdRng::seed_from_u64(1347);
    let (old, x) = (ids[0].public().sig_pk, Identity::generate(&mut rng));
    assert!(approve_all(&relay, &ids, &m, &[1, 2], old, &x, &mut rng).await);
    let (mut chain, mut state) = load_state(&relay, &ids[0], &m[0]).await.unwrap();
    let vault_id = state.descriptor.vault_id;
    let r = state.pending_seat_move(&old).unwrap().clone();
    let cancel = SeatCancel {
        mailbox: m[0].label(),
        vault_id,
        move_id: r.move_id(&vault_id).unwrap(),
        old,
        new: x.public().sig_pk,
        at: 1_790_000_000,
    };
    let event = VaultEvent::KeepSeat {
        replacement: r.index,
        at: cancel.at,
        mailbox: cancel.mailbox,
        signature: cancel.sign(&ids[0]).unwrap(),
    };
    append_event(
        &relay, &ids[0], &m[0], &mut chain, &mut state, &event, &mut rng,
    )
    .await
    .unwrap();
    assert_eq!(
        relay
            .mailbox_info(&ids[1], m[1].label())
            .await
            .unwrap()
            .pending
            .len(),
        1
    );
    let _ = try_help(
        &relay,
        &ids[2],
        &m[2],
        tempfile::tempdir().unwrap().path(),
        Duration::ZERO,
        &mut rng,
    )
    .await;
    let info = relay.mailbox_info(&ids[1], m[1].label()).await.unwrap();
    assert!(info.pending.is_empty(), "the relay took the cancel");
    assert_eq!(info.cancelled.len(), 1);
}

/// Coordinator fix 3: one move the relay refuses doesn't stop the others. Move 1 was
/// logged and switched without the relay ever holding it (its finish is refused for good);
/// move 2 still completes.
#[tokio::test]
async fn a_refused_move_doesnt_stop_the_others() {
    use zafe_core::{node::append_event, vault::VaultEvent};
    use zafe_proto::relay::SeatMoveApproval;
    let relay = RelayClient::new(start_relay().await);
    let (ids, m) = vault(&relay, 4).await;
    let mut rng = StdRng::seed_from_u64(1348);
    let pk = |i: usize| ids[i].public().sig_pk;
    let (x1, y) = (Identity::generate(&mut rng), Identity::generate(&mut rng));
    // Move 1 (seat 3 to X1), in the log only.
    for i in [0, 1] {
        let (mut chain, mut state) = load_state(&relay, &ids[i], &m[i]).await.unwrap();
        let approval = SeatMoveApproval {
            vault_id: state.descriptor.vault_id,
            old: pk(3),
            new: *x1.public(),
            round: 0,
        };
        let event = VaultEvent::ReplaceApproval {
            old: pk(3),
            new: *x1.public(),
            signature: approval.sign(&ids[i]).unwrap(),
        };
        append_event(
            &relay, &ids[i], &m[i], &mut chain, &mut state, &event, &mut rng,
        )
        .await
        .unwrap();
    }
    for i in [0, 1] {
        let (mut chain, mut state) = load_state(&relay, &ids[i], &m[i]).await.unwrap();
        let r = state.pending_seat_move(&pk(3)).unwrap().clone();
        let event = VaultEvent::RepairHelped {
            replacement: r.index,
            attempt: 0,
        };
        append_event(
            &relay, &ids[i], &m[i], &mut chain, &mut state, &event, &mut rng,
        )
        .await
        .unwrap();
    }
    // Move 2 (seat 2 to Y), as the apps do it.
    assert!(approve_all(&relay, &ids, &m, &[0, 1], pk(2), &y, &mut rng).await);
    let dir = tempfile::tempdir().unwrap();
    let mut done = false;
    for _ in 0..6 {
        for i in [0, 1] {
            let _ = try_help(
                &relay,
                &ids[i],
                &m[i],
                &dir.path().join(i.to_string()),
                Duration::ZERO,
                &mut rng,
            )
            .await;
        }
        if matches!(
            try_recover(&relay, &y, Some(m[0].label()), &[pk(0)]).await,
            Ok(RecoveryStatus::Done { .. })
        ) {
            done = true;
            break;
        }
    }
    assert!(done, "move 2 completes despite move 1");
}

/// Coordinator item 4: a first sighting that can't be recorded for one move doesn't stop
/// another move's part.
#[tokio::test]
async fn one_moves_record_failing_doesnt_stop_another() {
    let relay = RelayClient::new(start_relay().await);
    let (ids, m) = vault(&relay, 4).await;
    let mut rng = StdRng::seed_from_u64(1349);
    let pk = |i: usize| ids[i].public().sig_pk;
    let (a, b) = (Identity::generate(&mut rng), Identity::generate(&mut rng));
    assert!(approve_all(&relay, &ids, &m, &[0, 1], pk(2), &a, &mut rng).await);
    assert!(approve_all(&relay, &ids, &m, &[0, 1], pk(3), &b, &mut rng).await);
    let (_, state) = load_state(&relay, &ids[0], &m[0]).await.unwrap();
    let vault_id = state.descriptor.vault_id;
    let first = state
        .pending_seat_move(&pk(2))
        .unwrap()
        .move_id(&vault_id)
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    // The record's place is taken by a directory: it can be neither read nor written.
    std::fs::create_dir_all(
        dir.path()
            .join(format!("{}.seen", hex::encode(first)))
            .join("x"),
    )
    .unwrap();
    assert!(
        try_help(&relay, &ids[0], &m[0], dir.path(), Duration::ZERO, &mut rng)
            .await
            .is_err()
    );
    let (_, state) = load_state(&relay, &ids[0], &m[0]).await.unwrap();
    assert_eq!(
        state.pending_seat_move(&pk(3)).unwrap().helped,
        vec![pk(0)],
        "the other move got this helper's part"
    );
}

/// Coordinator item 5: the new phone sees its move pending, reaches Done once the parts
/// arrive even before the relay's finish, and until the finish its writes are "still
/// finishing" (retryable), never stuck. A phone whose own seat moved away isn't
/// "finishing".
#[tokio::test]
async fn the_new_phone_before_and_after_the_relays_finish() {
    let relay = RelayClient::new(start_relay().await);
    let (ids, m) = vault(&relay, 3).await;
    let mut rng = StdRng::seed_from_u64(1350);
    let (old, x) = (ids[0].public().sig_pk, Identity::generate(&mut rng));
    assert!(approve_all(&relay, &ids, &m, &[1, 2], old, &x, &mut rng).await);
    let confirmed = [ids[1].public().sig_pk, ids[2].public().sig_pk];
    assert!(matches!(
        try_recover(&relay, &x, Some(m[1].label()), &confirmed)
            .await
            .unwrap(),
        RecoveryStatus::Pending
    ));
    let dir = tempfile::tempdir().unwrap();
    let (d1, d2) = (dir.path().join("1"), dir.path().join("2"));
    help(&relay, &ids[1], &m[1], &d1, Duration::ZERO, &mut rng).await;
    // Helper 2's part switches the seat; it sends its sigma and its finish (1 of 2).
    help(&relay, &ids[2], &m[2], &d2, Duration::ZERO, &mut rng).await;
    // Helper 1's sigma goes, its finish doesn't land (a state that hides the old key from
    // the relay sync, as a failed request would).
    let (chain, mut state) = load_state(&relay, &ids[1], &m[1]).await.unwrap();
    state.replacements[0].old = [0; 32];
    help_repairs(
        &relay,
        &ids[1],
        &m[1],
        &chain,
        &state,
        &d1,
        Duration::ZERO,
        &mut rng,
    )
    .await
    .unwrap();
    let RecoveryStatus::Done { material, .. } =
        try_recover(&relay, &x, Some(m[1].label()), &confirmed)
            .await
            .unwrap()
    else {
        panic!("done before the relay's finish");
    };
    let (_, mine) = load_state(&relay, &x, &material).await.unwrap();
    assert!(zafe_core::repair::seat_finishing(&relay, &x, &material, &mine).await);
    assert!(matches!(
        zafe_core::repair::mark_repair_done(&relay, &x, &material, &mut rng).await,
        Err(zafe_core::node::NodeError::NotReady(_))
    ));
    // The old key's seat moved away: that is never "finishing".
    let (_, theirs) = load_state(&relay, &ids[1], &m[1]).await.unwrap();
    assert!(!zafe_core::repair::seat_finishing(&relay, &ids[0], &m[0], &theirs).await);
    // Helper 1's next refresh lands its finish; the new phone's write goes through.
    help(&relay, &ids[1], &m[1], &d1, Duration::ZERO, &mut rng).await;
    zafe_core::repair::mark_repair_done(&relay, &x, &material, &mut rng)
        .await
        .unwrap();
    let (_, mine) = load_state(&uncached(&relay), &x, &material).await.unwrap();
    assert!(mine.replacements[0].done);
}

/// Coordinator item 6: a helper stops helping (round 1 can't be kept): its part never
/// counts, no sigma goes, and a restored phone (new state dir, same identity) doesn't help
/// either.
#[tokio::test]
async fn a_helper_that_stopped_never_helps() {
    let relay = RelayClient::new(start_relay().await);
    let (ids, m) = vault(&relay, 3).await;
    let mut rng = StdRng::seed_from_u64(1351);
    let (old, x) = (ids[0].public().sig_pk, Identity::generate(&mut rng));
    assert!(approve_all(&relay, &ids, &m, &[1, 2], old, &x, &mut rng).await);
    assert!(keep_seat(&relay, &ids[0], &m[0], &mut rng).await.unwrap());
    assert!(approve_all(&relay, &ids, &m, &[1, 2], old, &x, &mut rng).await);
    let (_, state) = load_state(&relay, &ids[2], &m[2]).await.unwrap();
    let index = state.pending_seat_move(&old).unwrap().index;
    zafe_core::repair::stop_helping(&relay, &ids[2], &m[2], index, &mut rng)
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    for _ in 0..3 {
        help(
            &relay,
            &ids[1],
            &m[1],
            &dir.path().join("1"),
            Duration::ZERO,
            &mut rng,
        )
        .await;
        // A restore: a fresh directory, the same identity.
        let fresh = dir.path().join(format!("2-{}", rand::random::<u32>()));
        let report = help(&relay, &ids[2], &m[2], &fresh, Duration::ZERO, &mut rng).await;
        assert_eq!((report.helped, report.sigmas_sent), (0, 0));
    }
    let (_, state) = load_state(&relay, &ids[1], &m[1]).await.unwrap();
    let r = state.pending_seat_move(&old).expect("never switches");
    assert_eq!(r.stopped, vec![ids[2].public().sig_pk]);
    assert!(!matches!(
        try_recover(&relay, &x, Some(m[1].label()), &[ids[1].public().sig_pk]).await,
        Ok(RecoveryStatus::Done { .. })
    ));
}

/// A relay that lists a pending key for a stale approval (Alice's single approval of moving
/// Bob's seat to Z, dropped when Bob kept his seat in a move to X) is tolerated: the key
/// gets nothing, and every member still loads.
#[tokio::test]
async fn a_pending_key_for_a_stale_approval_gets_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("relay.sqlite");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = zafe_relay::Relay::open(&db)
        .unwrap()
        .with_limits(Limits::hosted())
        .router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let relay = RelayClient::new(format!("http://{addr}"));
    let (ids, m) = vault(&relay, 3).await;
    let mut rng = StdRng::seed_from_u64(1352);
    let bob = ids[1].public().sig_pk;
    let (x, z) = (Identity::generate(&mut rng), Identity::generate(&mut rng));
    // Alice approves Bob -> Z once (round 0); then Bob -> X gets every approval and Bob keeps.
    assert!(!approve_all(&relay, &ids, &m, &[0], bob, &z, &mut rng).await);
    assert!(approve_all(&relay, &ids, &m, &[0, 2], bob, &x, &mut rng).await);
    assert!(keep_seat(&relay, &ids[1], &m[1], &mut rng).await.unwrap());
    // A relay that lists Bob -> Z (round 0) as pending regardless.
    let move_id = zafe_proto::relay::seat_move_id(&m[0].descriptor.vault_id, &bob, z.public(), 0);
    rusqlite::Connection::open(&db)
        .unwrap()
        .execute(
            "INSERT INTO seat_moves (mailbox, move_id, old, new_sig, new_enc, round, status)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, 0)",
            rusqlite::params![
                &m[0].label()[..],
                &move_id[..],
                &bob[..],
                &z.public().sig_pk[..],
                &z.public().enc_pk[..]
            ],
        )
        .unwrap();
    assert_nothing_for(&relay, &ids, &m, &z, &mut rng).await;
}

/// Review of batch 2: the relay takes the old key's cancel while its row is pending, and the
/// row stays pending after the log switched until t finishes land. A thief holding the old
/// phone (it reads the log) posts its cancel in that gap: the relay then refuses every
/// finish and every sigma to the new key, while the log already gave the seat to the new
/// key. The seat must still reach the new phone.
#[tokio::test]
async fn a_relay_cancel_after_the_switch_doesnt_strand_the_seat() {
    use zafe_proto::relay::SeatCancel;
    let relay = RelayClient::new(start_relay().await);
    let (ids, m) = vault(&relay, 3).await;
    let mut rng = StdRng::seed_from_u64(1360);
    let (old, x) = (ids[0].public().sig_pk, Identity::generate(&mut rng));
    assert!(approve_all(&relay, &ids, &m, &[1, 2], old, &x, &mut rng).await);
    let (_, state) = load_state(&relay, &ids[1], &m[1]).await.unwrap();
    let vault_id = state.descriptor.vault_id;
    let move_id = state
        .pending_seat_move(&old)
        .unwrap()
        .move_id(&vault_id)
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let (d1, d2) = (dir.path().join("1"), dir.path().join("2"));
    help(&relay, &ids[1], &m[1], &d1, Duration::ZERO, &mut rng).await;
    // Helper 2's part switches the seat in the log; its finish is 1 of 2.
    help(&relay, &ids[2], &m[2], &d2, Duration::ZERO, &mut rng).await;
    let (_, state) = load_state(&uncached(&relay), &ids[1], &m[1]).await.unwrap();
    assert!(
        state.replacement_to(&x.public().sig_pk).is_some(),
        "switched"
    );
    // The old key, still on the relay until the second finish, cancels now.
    let cancel = SeatCancel {
        mailbox: m[0].label(),
        vault_id,
        move_id,
        old,
        new: x.public().sig_pk,
        at: 1_790_000_000,
    };
    relay
        .cancel_seat_move(cancel, cancel.sign(&ids[0]).unwrap())
        .await
        .unwrap();
    for _ in 0..3 {
        let _ = try_help(&relay, &ids[1], &m[1], &d1, Duration::ZERO, &mut rng).await;
        let _ = try_help(&relay, &ids[2], &m[2], &d2, Duration::ZERO, &mut rng).await;
    }
    let confirmed = [ids[1].public().sig_pk, ids[2].public().sig_pk];
    let status = try_recover(&relay, &x, Some(m[1].label()), &confirmed).await;
    let Ok(RecoveryStatus::Done { material, .. }) = status else {
        panic!("the log gave the seat to the new phone, the relay stranded it: {status:?}");
    };
    zafe_core::repair::mark_repair_done(&relay, &x, &material, &mut rng)
        .await
        .unwrap();
}

/// Review of batch 2: a relay's pending key is explained by a current approval short of t
/// (finding B: a dishonest member gives the relay its own approval plus an honest one from
/// the log). When another move of the same seat later reaches t, the log drops those
/// approvals, and the relay's row must not lock every member out then.
#[tokio::test]
async fn a_relay_row_from_a_dropped_approval_doesnt_lock_the_vault() {
    let relay = RelayClient::new(start_relay().await);
    let (ids, m) = vault(&relay, 3).await;
    let mut rng = StdRng::seed_from_u64(1361);
    let old = ids[0].public().sig_pk;
    let (z, x) = (Identity::generate(&mut rng), Identity::generate(&mut rng));
    // Member 2 approves old -> Z in the log (1 of 2).
    assert!(!approve_all(&relay, &ids, &m, &[2], old, &z, &mut rng).await);
    // Dishonest member 1 gives the relay its approval plus member 2's: Z is pending there.
    let approval = SeatMoveApproval {
        vault_id: m[0].descriptor.vault_id,
        old,
        new: *z.public(),
        round: 0,
    };
    let approvals = [1, 2]
        .iter()
        .map(|&i| (ids[i].public().sig_pk, approval.sign(&ids[i]).unwrap()))
        .collect();
    relay
        .replace_member_at(&ids[1], m[1].label(), approval, approvals)
        .await
        .unwrap();
    load_state(&uncached(&relay), &ids[2], &m[2]).await.unwrap();
    // Both approve old -> X: it reaches t and the log drops the approval of Z.
    assert!(approve_all(&relay, &ids, &m, &[1, 2], old, &x, &mut rng).await);
    for i in 0..3 {
        if let Err(e) = load_state(&uncached(&relay), &ids[i], &m[i]).await {
            panic!("member {i} is locked out: {e:?}");
        }
    }
}

/// Review round 3: a seat moves twice while the relay lags. 2-of-3 (A, H, M): A -> X
/// switches in the log, H posts its finish, M holds its own, so the relay keeps A (X
/// pending). X's phone breaks too; H and M move X -> Y, which switches in the log while the
/// relay has no row for it (X isn't its member). Every member must still load (the relay's
/// A stands in for Y two moves back), and once M finishes A -> X, members' refreshes give
/// the relay X -> Y's approvals and finishes, and Y gets its key and can write.
#[tokio::test]
async fn a_seat_that_moves_twice_while_the_relay_lags() {
    let relay = RelayClient::new(start_relay().await);
    let (ids, m) = vault(&relay, 3).await;
    let mut rng = StdRng::seed_from_u64(1370);
    let a = ids[0].public().sig_pk;
    let (x, y) = (Identity::generate(&mut rng), Identity::generate(&mut rng));
    let dir = tempfile::tempdir().unwrap();
    let (dh, dm) = (dir.path().join("h"), dir.path().join("m"));
    // M's refresh that holds back its finishes (a state that hides the old keys from the
    // relay sync, as M simply never sending them would).
    let m_holds = |rng: &mut StdRng| {
        let relay = relay.clone();
        let (id, mat, dm) = (&ids[2], m[2].clone(), dm.clone());
        let mut rng = StdRng::seed_from_u64(rng.next_u64());
        async move {
            let (chain, mut state) = load_state(&relay, id, &mat).await.unwrap();
            for r in &mut state.replacements {
                r.old = [0; 32];
            }
            let _ = help_repairs(
                &relay,
                id,
                &mat,
                &chain,
                &state,
                &dm,
                Duration::ZERO,
                &mut rng,
            )
            .await;
        }
    };
    assert!(approve_all(&relay, &ids, &m, &[1, 2], a, &x, &mut rng).await);
    help(&relay, &ids[1], &m[1], &dh, Duration::ZERO, &mut rng).await;
    m_holds(&mut rng).await; // switches A -> X in the log
    help(&relay, &ids[1], &m[1], &dh, Duration::ZERO, &mut rng).await; // H's finish: 1 of 2
    let (_, state) = load_state(&uncached(&relay), &ids[1], &m[1]).await.unwrap();
    assert!(state.descriptor.member(&x.public().sig_pk).is_some());
    // X's phone breaks before it ever recovered: H and M move X -> Y.
    // M approves first (an approval that completes nothing syncs nothing), H completes it.
    assert!(approve_all(&relay, &ids, &m, &[2, 1], x.public().sig_pk, &y, &mut rng).await);
    let info = relay.mailbox_info(&ids[1], m[1].label()).await.unwrap();
    assert!(
        info.members.iter().any(|k| k.sig_pk == a),
        "the relay still has A (M held its finish)"
    );
    help(&relay, &ids[1], &m[1], &dh, Duration::ZERO, &mut rng).await;
    m_holds(&mut rng).await; // switches X -> Y in the log
    let (_, state) = load_state(&uncached(&relay), &ids[1], &m[1]).await.unwrap();
    assert!(
        state.descriptor.member(&y.public().sig_pk).is_some(),
        "Y in the log"
    );
    let info = relay.mailbox_info(&ids[1], m[1].label()).await.unwrap();
    assert!(
        info.members.iter().any(|k| k.sig_pk == a),
        "relay two moves behind"
    );
    for i in [1, 2] {
        load_state(&uncached(&relay), &ids[i], &m[i])
            .await
            .unwrap_or_else(|e| panic!("member {i} is locked out: {e:?}"));
    }
    // M stops holding back: its refreshes finish A -> X, then carry X -> Y.
    let confirmed = [ids[1].public().sig_pk];
    let mut done = None;
    for _ in 0..4 {
        let _ = try_help(&relay, &ids[2], &m[2], &dm, Duration::ZERO, &mut rng).await;
        let _ = try_help(&relay, &ids[1], &m[1], &dh, Duration::ZERO, &mut rng).await;
        if let Ok(RecoveryStatus::Done { material, .. }) =
            try_recover(&relay, &y, Some(m[1].label()), &confirmed).await
        {
            done = Some(material);
            break;
        }
    }
    let material = done.expect("Y gets its key");
    zafe_core::repair::mark_repair_done(&relay, &y, &material, &mut rng)
        .await
        .expect("Y writes once the relay finished");
}
