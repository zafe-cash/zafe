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

fn uncached(relay: &RelayClient) -> RelayClient {
    RelayClient::new(relay.base_url().replace("127.0.0.1", "localhost"))
}

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

    let words_c = compare_words(&relay, &ids[2], &x).await;
    let _ = approve_replacement(&relay, &ids[2], c, bob, &request, &words_c, &mut rng).await;

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

#[tokio::test]
async fn a_repair_survives_a_helpers_seat_moving() {
    let relay = RelayClient::new(start_relay().await);
    let (ids, materials) = vault(&relay, 4).await;
    let (a, d) = (&materials[0], &materials[3]);
    let mut rng = StdRng::seed_from_u64(1340);
    let pk = |i: usize| ids[i].public().sig_pk;
    let label = Some(a.label());
    let (c_new, b_new) = (Identity::generate(&mut rng), Identity::generate(&mut rng));

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

fn setup_key(id: &zafe_proto::Identity) -> zafe_core::nonce_store::NonceKey {
    zafe_core::nonce_store::NonceKey::derive(&id.public().sig_pk, b"setup")
}

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

fn pins() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("zafe-recovery-pins-{}", std::process::id()))
}

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

#[tokio::test]
async fn a_second_round_to_the_same_phone_after_a_part_sent_early() {
    let relay = RelayClient::new(start_relay().await);
    let (ids, m) = vault(&relay, 3).await;
    let mut rng = StdRng::seed_from_u64(1344);
    let (old, new) = (ids[0].public().sig_pk, Identity::generate(&mut rng));
    assert!(approve_all(&relay, &ids, &m, &[1, 2], old, &new, &mut rng).await);
    let dir = tempfile::tempdir().unwrap();
    let (d1, d2) = (dir.path().join("1"), dir.path().join("2"));
    let held = help(&relay, &ids[2], &m[2], &d2, SEAT_MOVE_WAIT, &mut rng).await;
    assert_eq!((held.deltas_sent, held.held), (1, 1));
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

#[tokio::test]
async fn no_part_goes_before_the_switch() {
    let relay = RelayClient::new(start_relay().await);
    let (ids, m) = vault(&relay, 3).await;
    let mut rng = StdRng::seed_from_u64(1345);
    let (old, x) = (ids[0].public().sig_pk, Identity::generate(&mut rng));
    assert!(approve_all(&relay, &ids, &m, &[1, 2], old, &x, &mut rng).await);
    let dir = tempfile::tempdir().unwrap();
    help(
        &relay,
        &ids[2],
        &m[2],
        &dir.path().join("2"),
        SEAT_MOVE_WAIT,
        &mut rng,
    )
    .await;
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

#[tokio::test]
async fn a_refused_move_doesnt_stop_the_others() {
    use zafe_core::{node::append_event, vault::VaultEvent};
    use zafe_proto::relay::SeatMoveApproval;
    let relay = RelayClient::new(start_relay().await);
    let (ids, m) = vault(&relay, 4).await;
    let mut rng = StdRng::seed_from_u64(1348);
    let pk = |i: usize| ids[i].public().sig_pk;
    let (x1, y) = (Identity::generate(&mut rng), Identity::generate(&mut rng));
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
    help(&relay, &ids[2], &m[2], &d2, Duration::ZERO, &mut rng).await;
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
    let (_, theirs) = load_state(&relay, &ids[1], &m[1]).await.unwrap();
    assert!(!zafe_core::repair::seat_finishing(&relay, &ids[0], &m[0], &theirs).await);
    help(&relay, &ids[1], &m[1], &d1, Duration::ZERO, &mut rng).await;
    zafe_core::repair::mark_repair_done(&relay, &x, &material, &mut rng)
        .await
        .unwrap();
    let (_, mine) = load_state(&uncached(&relay), &x, &material).await.unwrap();
    assert!(mine.replacements[0].done);
}

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
    assert!(!approve_all(&relay, &ids, &m, &[0], bob, &z, &mut rng).await);
    assert!(approve_all(&relay, &ids, &m, &[0, 2], bob, &x, &mut rng).await);
    assert!(keep_seat(&relay, &ids[1], &m[1], &mut rng).await.unwrap());
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
    help(&relay, &ids[2], &m[2], &d2, Duration::ZERO, &mut rng).await;
    let (_, state) = load_state(&uncached(&relay), &ids[1], &m[1]).await.unwrap();
    assert!(
        state.replacement_to(&x.public().sig_pk).is_some(),
        "switched"
    );
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

#[tokio::test]
async fn a_relay_row_from_a_dropped_approval_doesnt_lock_the_vault() {
    let relay = RelayClient::new(start_relay().await);
    let (ids, m) = vault(&relay, 3).await;
    let mut rng = StdRng::seed_from_u64(1361);
    let old = ids[0].public().sig_pk;
    let (z, x) = (Identity::generate(&mut rng), Identity::generate(&mut rng));
    assert!(!approve_all(&relay, &ids, &m, &[2], old, &z, &mut rng).await);
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
    assert!(approve_all(&relay, &ids, &m, &[1, 2], old, &x, &mut rng).await);
    for i in 0..3 {
        if let Err(e) = load_state(&uncached(&relay), &ids[i], &m[i]).await {
            panic!("member {i} is locked out: {e:?}");
        }
    }
}

#[tokio::test]
async fn a_seat_that_moves_twice_while_the_relay_lags() {
    let relay = RelayClient::new(start_relay().await);
    let (ids, m) = vault(&relay, 3).await;
    let mut rng = StdRng::seed_from_u64(1370);
    let a = ids[0].public().sig_pk;
    let (x, y) = (Identity::generate(&mut rng), Identity::generate(&mut rng));
    let dir = tempfile::tempdir().unwrap();
    let (dh, dm) = (dir.path().join("h"), dir.path().join("m"));
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
    m_holds(&mut rng).await;
    help(&relay, &ids[1], &m[1], &dh, Duration::ZERO, &mut rng).await;
    let (_, state) = load_state(&uncached(&relay), &ids[1], &m[1]).await.unwrap();
    assert!(state.descriptor.member(&x.public().sig_pk).is_some());
    assert!(approve_all(&relay, &ids, &m, &[2, 1], x.public().sig_pk, &y, &mut rng).await);
    let info = relay.mailbox_info(&ids[1], m[1].label()).await.unwrap();
    assert!(
        info.members.iter().any(|k| k.sig_pk == a),
        "the relay still has A (M held its finish)"
    );
    help(&relay, &ids[1], &m[1], &dh, Duration::ZERO, &mut rng).await;
    m_holds(&mut rng).await;
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
