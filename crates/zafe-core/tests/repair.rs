use std::{collections::BTreeMap, time::Duration};

use pasta_curves::pallas;
use rand::{rngs::StdRng, SeedableRng};
use zafe_core::{
    node::{
        create_vault, join_vault, load_state, run_keygen, seal, set_name, KeygenOptions, NodeError,
        VaultMaterial,
    },
    relay_client::RelayClient,
    repair::{
        approve_replacement, current_material, help_repairs, new_phone_words, recovery_checks,
        retry_repair, try_recover, RecoveryRequest, RecoveryStatus, SeatLink,
    },
    signing,
    wallet::regtest_network,
};
use zafe_proto::Identity;
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

async fn help(
    relay: &RelayClient,
    id: &Identity,
    m: &VaultMaterial,
    dir: &std::path::Path,
) -> zafe_core::repair::RepairReport {
    let mut rng = StdRng::seed_from_u64(7);
    let (chain, state) = load_state(relay, id, m).await.unwrap();
    help_repairs(relay, id, m, &chain, &state, dir, Duration::ZERO, &mut rng)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_swapped_recovery_code_shows_words_the_new_phone_does_not() {
    let relay = RelayClient::new(start_relay().await);
    let (ids, materials) = vault(&relay, 3).await;
    let mut rng = StdRng::seed_from_u64(73);
    let real = Identity::generate(&mut rng);
    let attacker = Identity::generate(&mut rng);
    recovery_checks(&relay, &real, &pins()).await.unwrap();
    let words = compare_words(&relay, &ids[1], &attacker).await;
    let on_real = recovery_checks(&relay, &real, &pins()).await.unwrap();
    assert!(on_real.is_empty(), "the real phone was never challenged");
    let old = ids[0].public().sig_pk;
    let request = RecoveryRequest {
        identity: *attacker.public(),
    };
    let wrong = "abandon ability able about";
    assert_ne!(words, wrong);
    assert!(matches!(
        approve_replacement(
            &relay,
            &ids[1],
            &materials[1],
            old,
            &request,
            wrong,
            &mut rng
        )
        .await,
        Err(NodeError::Verification(_))
    ));
}

#[tokio::test]
async fn a_lost_seat_moves_to_a_new_device() {
    let relay = RelayClient::new(start_relay().await);
    let (ids, materials) = vault(&relay, 3).await;
    let (a, b, c) = (&materials[0], &materials[1], &materials[2]);
    let mut rng = StdRng::seed_from_u64(71);
    let dir = std::env::temp_dir().join(format!("zafe-repair-{}", std::process::id()));
    let (dir_b, dir_c) = (dir.join("b"), dir.join("c"));
    let link =
        SeatLink::decode(&SeatLink::for_vault(b, "https://unused.example").encode()).unwrap();
    assert_eq!(link.relay_url, relay.base_url());
    assert_eq!(link.label, b.label());
    let label = Some(link.label);
    set_name(&relay, &ids[0], a, "Alice", &mut rng)
        .await
        .unwrap();

    let fresh = Identity::generate(&mut rng);
    let request = RecoveryRequest {
        identity: *fresh.public(),
    };
    let code = request.encode();
    assert!(code.starts_with("zafe-recover-v1:"));
    let request = RecoveryRequest::decode(&code).unwrap();
    let words_b = compare_words(&relay, &ids[1], &fresh).await;
    let words_c = compare_words(&relay, &ids[2], &fresh).await;
    assert_ne!(words_b, words_c, "each co-signer has its own words");
    let confirmed = [ids[1].public().sig_pk, ids[2].public().sig_pk];
    assert!(matches!(
        try_recover(&relay, &fresh, label, &confirmed)
            .await
            .unwrap(),
        RecoveryStatus::Waiting
    ));

    let old = ids[0].public().sig_pk;
    assert!(
        matches!(
            approve_replacement(&relay, &ids[0], a, old, &request, &words_b, &mut rng).await,
            Err(NodeError::NotReady(_))
        ),
        "approving before the words were compared is refused"
    );
    let words_a = compare_words(&relay, &ids[0], &fresh).await;
    assert!(matches!(
        approve_replacement(&relay, &ids[0], a, old, &request, &words_a, &mut rng).await,
        Err(NodeError::Invalid(_))
    ));
    assert!(
        !approve_replacement(&relay, &ids[1], b, old, &request, &words_b, &mut rng)
            .await
            .unwrap()
    );
    assert!(matches!(
        approve_replacement(&relay, &ids[1], b, old, &request, &words_b, &mut rng).await,
        Err(NodeError::Invalid(_))
    ));
    assert!(matches!(
        try_recover(&relay, &fresh, label, &confirmed)
            .await
            .unwrap(),
        RecoveryStatus::Waiting
    ));

    assert!(
        approve_replacement(&relay, &ids[2], c, old, &request, &words_c, &mut rng)
            .await
            .unwrap()
    );
    let (_, state) = load_state(&relay, &ids[1], b).await.unwrap();
    assert!(state.descriptor.member(&old).is_some());
    assert!(state.descriptor.member(&fresh.public().sig_pk).is_none());
    assert!(state.pending_seat_move(&old).is_some());
    let info = relay.mailbox_info(&ids[1], a.label()).await.unwrap();
    assert!(info.members.iter().any(|m| m.sig_pk == old));
    assert_eq!(info.pending.len(), 1);
    assert_eq!(info.pending[0].new, *fresh.public());
    load_state(&relay, &ids[0], a)
        .await
        .expect("the old device still reads the vault");
    assert!(matches!(
        try_recover(&relay, &fresh, label, &confirmed)
            .await
            .unwrap(),
        RecoveryStatus::Pending
    ));

    let first = help(&relay, &ids[1], b, &dir_b).await;
    assert_eq!(
        (
            first.deltas_sent,
            first.helped,
            first.sigmas_sent,
            first.waiting
        ),
        (1, 1, 0, 1)
    );
    let second = help(&relay, &ids[2], c, &dir_c).await;
    assert_eq!(
        (second.deltas_sent, second.helped, second.sigmas_sent),
        (1, 1, 1)
    );
    let (_, state) = load_state(&relay, &ids[1], b).await.unwrap();
    assert!(state.descriptor.member(&old).is_none(), "the log switched");
    assert!(state.descriptor.member(&fresh.public().sig_pk).is_some());
    assert_eq!(
        state.names[&fresh.public().sig_pk],
        "Alice",
        "the name moves too"
    );
    let on_relay = relay.members(&ids[1], a.label()).await.unwrap();
    assert!(on_relay.members.iter().any(|m| m.sig_pk == old));
    let third = help(&relay, &ids[1], b, &dir_b).await;
    assert_eq!((third.deltas_sent, third.sigmas_sent), (0, 1));
    let on_relay = relay.members(&ids[1], a.label()).await.unwrap();
    assert!(on_relay.members.contains(fresh.public()));
    assert!(!on_relay.members.iter().any(|m| m.sig_pk == old));
    assert!(
        load_state(&relay, &ids[0], a).await.is_err(),
        "the lost device is out"
    );
    let again = help(&relay, &ids[1], b, &dir_b).await;
    assert_eq!(again, Default::default());
    recorded_sigmas_hide_the_vault_secret(&relay, &fresh, &ids, a).await;

    let RecoveryStatus::Done { material, invite } = try_recover(&relay, &fresh, label, &confirmed)
        .await
        .unwrap()
    else {
        panic!("not recovered");
    };
    assert_eq!(invite.vault_id, a.descriptor.vault_id);
    assert_eq!(invite.label, a.label());
    assert_ne!(
        invite.label, invite.vault_id,
        "a new vault's mailbox is a label"
    );
    assert_eq!(material.ids(), a.ids());
    assert_eq!(material.relay.url.as_deref(), Some(relay.base_url()));
    assert_eq!(invite.threshold, 2);
    let (repaired, original) = (material.key_package().unwrap(), a.key_package().unwrap());
    assert_eq!(repaired.identifier(), original.identifier());
    assert_eq!(repaired.signing_share(), original.signing_share());
    assert_eq!(material.vault_secret, a.vault_secret);
    assert!(material.descriptor.member(&fresh.public().sig_pk).is_some());

    sign_together(&material, c, &mut rng);
    let (_, mine) = load_state(&relay, &fresh, &material).await.unwrap();
    assert_eq!(mine.replacements.len(), 1);
    set_name(&relay, &fresh, &material, "Alice (new phone)", &mut rng)
        .await
        .unwrap();
    let (_, state) = load_state(&relay, &ids[2], c).await.unwrap();
    assert_eq!(state.names[&fresh.public().sig_pk], "Alice (new phone)");

    let updated = current_material(b, &state).expect("membership changed");
    assert!(updated.descriptor.member(&fresh.public().sig_pk).is_some());
    assert_eq!(updated.key_package, b.key_package);
    assert!(current_material(&updated, &state).is_none());
    load_state(&relay, &ids[1], &updated).await.unwrap();

    zafe_core::repair::mark_repair_done(&relay, &fresh, &material, &mut rng)
        .await
        .unwrap();
    let (_, state) = load_state(&relay, &ids[1], &updated).await.unwrap();
    assert!(state.replacements[0].done);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_stalled_repair_is_taken_over_by_another_member() {
    let relay = RelayClient::new(start_relay().await);
    let (ids, m) = vault(&relay, 4).await;
    let mut rng = StdRng::seed_from_u64(72);
    let dir = std::env::temp_dir().join(format!("zafe-repair-retry-{}", std::process::id()));
    let fresh = Identity::generate(&mut rng);
    let request = RecoveryRequest {
        identity: *fresh.public(),
    };
    let old = ids[0].public().sig_pk;
    for i in [1, 2] {
        let words = compare_words(&relay, &ids[i], &fresh).await;
        approve_replacement(&relay, &ids[i], &m[i], old, &request, &words, &mut rng)
            .await
            .unwrap();
    }
    let (chain, state) = load_state(&relay, &ids[1], &m[1]).await.unwrap();
    let index = state.replacements[0].index;
    let report = help_repairs(
        &relay,
        &ids[1],
        &m[1],
        &chain,
        &state,
        &dir.join("1"),
        Duration::ZERO,
        &mut rng,
    )
    .await
    .unwrap();
    assert_eq!(report.waiting, 1);

    let stalled = ids[2].public().sig_pk;
    let (helping, taker) = (ids[1].public().sig_pk, ids[3].public().sig_pk);
    assert!(
        retry_repair(&relay, &ids[1], &m[1], index, stalled, helping, &mut rng)
            .await
            .is_err()
    );
    assert!(
        retry_repair(&relay, &ids[3], &m[3], index, taker, taker, &mut rng)
            .await
            .is_err()
    );
    let keyless = retry_repair(&relay, &ids[1], &m[1], index, stalled, taker, &mut rng)
        .await
        .unwrap_err();
    assert!(
        matches!(&keyless, NodeError::NotReady(m) if m.contains("hasn't joined this recovery")),
        "a stand-in that never published a post-quantum key isn't named: {keyless:?}"
    );
    retry_repair(&relay, &ids[3], &m[3], index, stalled, taker, &mut rng)
        .await
        .unwrap();
    let (_, state) = load_state(&relay, &ids[1], &m[1]).await.unwrap();
    assert_eq!(state.replacements[0].attempt, 0);
    retry_repair(&relay, &ids[1], &m[1], index, stalled, taker, &mut rng)
        .await
        .unwrap();
    let (_, state) = load_state(&relay, &ids[1], &m[1]).await.unwrap();
    let r = &state.replacements[0];
    assert_eq!(r.attempt, 1);
    assert_eq!(
        r.helpers,
        vec![ids[1].public().sig_pk, ids[3].public().sig_pk]
    );

    for (i, sigmas) in [(1usize, 0usize), (3, 1), (1, 1)] {
        let (chain, state) = load_state(&relay, &ids[i], &m[i]).await.unwrap();
        let report = help_repairs(
            &relay,
            &ids[i],
            &m[i],
            &chain,
            &state,
            &dir.join(i.to_string()),
            Duration::ZERO,
            &mut rng,
        )
        .await
        .unwrap();
        assert_eq!(report.sigmas_sent, sigmas, "member {i}");
    }
    let confirmed = [ids[1].public().sig_pk, ids[2].public().sig_pk];
    let RecoveryStatus::Done { material, .. } =
        try_recover(&relay, &fresh, None, &confirmed).await.unwrap()
    else {
        panic!("not recovered");
    };
    assert_eq!(
        material.key_package().unwrap().signing_share(),
        m[0].key_package().unwrap().signing_share()
    );
    sign_together(&material, &m[3], &mut rng);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_late_challenger_cant_predict_the_new_phones_words() {
    use zafe_proto::relay::RecoveryValueKind;
    let relay = RelayClient::new(start_relay().await);
    let mut rng = StdRng::seed_from_u64(75);
    let (new, cosigner, late) = (
        Identity::generate(&mut rng),
        Identity::generate(&mut rng),
        Identity::generate(&mut rng),
    );
    compare_words(&relay, &cosigner, &new).await;
    let (subject, late_pk) = (new.public().sig_pk, late.public().sig_pk);
    let seen = relay.recovery_read(&late, subject).await.unwrap();
    let revealed = seen
        .iter()
        .find(|v| v.payload.kind == RecoveryValueKind::Reveal)
        .unwrap()
        .payload
        .value;
    relay
        .recovery_put(
            &late,
            subject,
            late_pk,
            RecoveryValueKind::Announce,
            [0; 32],
        )
        .await
        .unwrap();
    recovery_checks(&relay, &new, &pins()).await.unwrap();
    let challenge = [7; 32];
    relay
        .recovery_put(
            &late,
            subject,
            late_pk,
            RecoveryValueKind::Challenge,
            challenge,
        )
        .await
        .unwrap();
    let shown = recovery_checks(&relay, &new, &pins()).await.unwrap();
    let shown = shown.iter().find(|c| c.cosigner == late_pk).unwrap();
    let predicted = zafe_core::sas::recovery_words(new.public(), &late_pk, &revealed, &challenge);
    assert_ne!(shown.words, predicted);
}

#[tokio::test]
async fn a_challenge_swapped_after_the_reveal_is_refused() {
    use zafe_proto::relay::{RecoveryPut, RecoveryValueKind, Signed};
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("relay.sqlite");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = zafe_relay::Relay::open(&db).unwrap().router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let relay = RelayClient::new(format!("http://{addr}"));
    let mut rng = StdRng::seed_from_u64(78);
    let (new, cosigner) = (Identity::generate(&mut rng), Identity::generate(&mut rng));
    let words = compare_words(&relay, &cosigner, &new).await;

    let (subject, key) = (new.public().sig_pk, cosigner.public().sig_pk);
    let swapped = Signed::new(
        &cosigner,
        RecoveryPut {
            subject,
            cosigner: key,
            kind: RecoveryValueKind::Challenge,
            value: [9; 32],
            timestamp: 1,
        },
    )
    .unwrap()
    .to_bytes()
    .unwrap();
    let n = rusqlite::Connection::open(&db)
        .unwrap()
        .execute(
            "UPDATE recovery_values SET signed = ?1 WHERE subject = ?2 AND cosigner = ?3 AND kind = ?4",
            rusqlite::params![swapped, &subject[..], &key[..], RecoveryValueKind::Challenge as i64],
        )
        .unwrap();
    assert_eq!(n, 1);
    let rows = recovery_checks(&relay, &new, &pins()).await.unwrap();
    assert!(
        rows.iter().all(|c| c.cosigner != key),
        "shown {rows:?}, compared {words}"
    );
}

async fn restarted_repair(
    relay: &RelayClient,
    log_key: Option<[u8; 32]>,
) -> (Vec<Identity>, Vec<VaultMaterial>, Identity) {
    let (ids, m) = vault(relay, 4).await;
    let mut rng = StdRng::seed_from_u64(80);
    let dir = tempfile::tempdir().unwrap().keep();
    let fresh = Identity::generate(&mut rng);
    let request = RecoveryRequest {
        identity: *fresh.public(),
    };
    let old = ids[0].public().sig_pk;
    for i in [1, 2] {
        let words = compare_words(relay, &ids[i], &fresh).await;
        approve_replacement(relay, &ids[i], &m[i], old, &request, &words, &mut rng)
            .await
            .unwrap();
    }
    let help = |i: usize, sent: VaultMaterial| {
        let (id, dir, material) = (&ids[i], dir.join(i.to_string()), &m[i]);
        async move {
            let mut rng = StdRng::seed_from_u64(81 + i as u64);
            let (chain, state) = load_state(relay, id, material).await.unwrap();
            help_repairs(
                relay,
                id,
                &sent,
                &chain,
                &state,
                &dir,
                Duration::ZERO,
                &mut rng,
            )
            .await
            .unwrap();
        }
    };
    for i in [1, 2, 1] {
        help(i, m[i].clone()).await;
    }
    let (_, state) = load_state(relay, &ids[1], &m[1]).await.unwrap();
    let index = state.replacements[0].index;
    let (stalled, taker) = (ids[2].public().sig_pk, ids[3].public().sig_pk);
    for i in [3, 1] {
        retry_repair(relay, &ids[i], &m[i], index, stalled, taker, &mut rng)
            .await
            .unwrap();
    }
    for i in [1, 3, 1] {
        let mut material = m[i].clone();
        if let Some(key) = log_key {
            material.log_key = key;
        }
        help(i, material).await;
    }
    (ids, m, fresh)
}

#[tokio::test]
async fn a_confirmed_helper_that_didnt_approve_the_move_is_refused() {
    let relay = RelayClient::new(start_relay().await);
    let (ids, _, fresh) = restarted_repair(&relay, None).await;
    let confirmed = [ids[3].public().sig_pk];
    let err = try_recover(&relay, &fresh, None, &confirmed)
        .await
        .unwrap_err();
    assert!(matches!(err, NodeError::Verification(_)), "{err:?}");
}

#[tokio::test]
async fn a_confirmed_cosigner_that_is_no_longer_a_member_is_refused() {
    use zafe_core::{node::append_event, vault::VaultEvent};
    use zafe_proto::relay::SeatMoveApproval;
    let relay = RelayClient::new(start_relay().await);
    let (ids, m, fresh) = restarted_repair(&relay, None).await;
    let mut rng = StdRng::seed_from_u64(82);
    let elsewhere = Identity::generate(&mut rng);
    let seat = ids[2].public().sig_pk;
    for i in [1, 3] {
        let (mut chain, mut state) = load_state(&relay, &ids[i], &m[i]).await.unwrap();
        let approval = SeatMoveApproval {
            vault_id: state.descriptor.vault_id,
            old: seat,
            new: *elsewhere.public(),
            round: 0,
        };
        let event = VaultEvent::ReplaceApproval {
            old: approval.old,
            new: approval.new,
            signature: approval.sign(&ids[i]).unwrap(),
        };
        append_event(
            &relay, &ids[i], &m[i], &mut chain, &mut state, &event, &mut rng,
        )
        .await
        .unwrap();
    }
    for i in [1, 3] {
        let (mut chain, mut state) = load_state(&relay, &ids[i], &m[i]).await.unwrap();
        let r = state.pending_seat_move(&seat).unwrap();
        let event = VaultEvent::RepairHelped {
            replacement: r.index,
            attempt: r.attempt,
        };
        append_event(
            &relay, &ids[i], &m[i], &mut chain, &mut state, &event, &mut rng,
        )
        .await
        .unwrap();
    }
    let (_, state) = load_state(&relay, &ids[1], &m[1]).await.unwrap();
    assert!(state.descriptor.member(&seat).is_none());
    let confirmed = [ids[2].public().sig_pk];
    let err = try_recover(&relay, &fresh, None, &confirmed)
        .await
        .unwrap_err();
    assert!(matches!(err, NodeError::Verification(_)), "{err:?}");
}

#[tokio::test]
async fn helpers_with_another_log_key_than_the_confirmed_cosigner_are_refused() {
    let relay = RelayClient::new(start_relay().await);
    let (ids, _, fresh) = restarted_repair(&relay, Some([9; 32])).await;
    let confirmed = [ids[2].public().sig_pk];
    let err = try_recover(&relay, &fresh, None, &confirmed)
        .await
        .unwrap_err();
    assert!(matches!(err, NodeError::Verification(_)), "{err:?}");
    let relay = RelayClient::new(start_relay().await);
    let (ids, m, fresh) = restarted_repair(&relay, None).await;
    let RecoveryStatus::Done { material, .. } =
        try_recover(&relay, &fresh, None, &[ids[2].public().sig_pk])
            .await
            .unwrap()
    else {
        panic!("not recovered");
    };
    assert_eq!(material.log_key, m[1].log_key);
}

#[tokio::test]
async fn a_vault_the_confirmed_cosigner_isnt_in_is_refused() {
    let relay = RelayClient::new(start_relay().await);
    let (ids, m) = vault(&relay, 3).await;
    let mut rng = StdRng::seed_from_u64(76);
    let dir = std::env::temp_dir().join(format!("zafe-repair-fake-{}", std::process::id()));
    let fresh = Identity::generate(&mut rng);
    let request = RecoveryRequest {
        identity: *fresh.public(),
    };
    let old = ids[0].public().sig_pk;
    for i in [1, 2] {
        let words = compare_words(&relay, &ids[i], &fresh).await;
        approve_replacement(&relay, &ids[i], &m[i], old, &request, &words, &mut rng)
            .await
            .unwrap();
    }
    for i in [1, 2, 1] {
        let (chain, state) = load_state(&relay, &ids[i], &m[i]).await.unwrap();
        help_repairs(
            &relay,
            &ids[i],
            &m[i],
            &chain,
            &state,
            &dir.join(i.to_string()),
            Duration::ZERO,
            &mut rng,
        )
        .await
        .unwrap();
    }
    let own_cosigner = Identity::generate(&mut rng).public().sig_pk;
    assert!(!matches!(
        try_recover(&relay, &fresh, None, &[own_cosigner])
            .await
            .unwrap(),
        RecoveryStatus::Done { .. }
    ));
    let theirs = [ids[1].public().sig_pk];
    assert!(matches!(
        try_recover(&relay, &fresh, None, &theirs).await.unwrap(),
        RecoveryStatus::Done { .. }
    ));
    let _ = std::fs::remove_dir_all(&dir);
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

async fn recorded_sigmas_hide_the_vault_secret(
    relay: &RelayClient,
    new: &Identity,
    ids: &[Identity],
    material: &VaultMaterial,
) {
    use zafe_proto::envelope::Kind;
    let by_pk: BTreeMap<[u8; 32], &Identity> =
        ids.iter().map(|id| (id.public().sig_pk, id)).collect();
    let mut seen = 0;
    for (_, envelope) in relay.inbox(new, material.label(), 0).await.unwrap() {
        if envelope.header.kind != Kind::RepairSigma {
            continue;
        }
        seen += 1;
        let helper = by_pk[&envelope.header.from];
        let after_x25519 = envelope.open(new, helper.public()).unwrap();
        for (what, secret) in [
            ("vault secret", material.vault_secret),
            ("log key", material.log_key),
        ] {
            assert!(
                !after_x25519.windows(32).any(|w| w == secret.as_slice()),
                "with the new phone's X25519 secret a recorder still can't see the {what}"
            );
        }
    }
    assert!(seen >= 2, "every helper's sigma checked");
}

#[tokio::test]
async fn a_cosigner_without_a_post_quantum_key_gets_no_words() {
    use zafe_proto::relay::RecoveryValueKind;
    let relay = RelayClient::new(start_relay().await);
    let (ids, _m) = vault(&relay, 3).await;
    let new = Identity::generate(&mut StdRng::seed_from_u64(77));
    let subject = new.public().sig_pk;
    let older_app = ids[1].public().sig_pk;
    relay
        .recovery_put(
            &ids[1],
            subject,
            older_app,
            RecoveryValueKind::Announce,
            [0; 32],
        )
        .await
        .unwrap();

    let rows = recovery_checks(&relay, &new, &pins()).await.unwrap();
    let row = rows.iter().find(|r| r.cosigner == older_app).unwrap();
    assert!(row.update_needed && row.words.is_empty());
    let values = relay.recovery_read(&new, subject).await.unwrap();
    assert!(
        !values
            .iter()
            .any(|v| v.payload.cosigner == older_app && v.payload.kind == RecoveryValueKind::Commit),
        "no commit, so an older app never gets words to approve with"
    );

    assert_eq!(
        new_phone_words(&relay, &ids[1], new.public())
            .await
            .unwrap(),
        None
    );
    let rows = recovery_checks(&relay, &new, &pins()).await.unwrap();
    assert!(!rows
        .iter()
        .any(|r| r.cosigner == older_app && r.update_needed));
    let values = relay.recovery_read(&new, subject).await.unwrap();
    assert!(
        values.iter().any(|v| v.payload.cosigner == older_app
            && v.payload.kind == RecoveryValueKind::Commit),
        "once it runs an updated app and publishes its key, the check goes ahead"
    );
}
