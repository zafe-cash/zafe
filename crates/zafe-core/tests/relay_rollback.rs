use std::{path::Path, time::Duration};

use rand::{rngs::StdRng, SeedableRng};
use zafe_core::{
    log_cache::LogCache,
    node::{
        create_vault, follow_relay, install_log_copy, join_vault, load_state, reseed_relay,
        run_keygen, seal, set_name, Invite, KeygenOptions, NodeError, Reseeded, VaultMaterial,
    },
    relay_client::RelayClient,
    wallet::regtest_network,
};
use zafe_proto::Identity;
use zafe_relay::Relay;

async fn serve(relay: Relay) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, relay.router()).await.unwrap();
    });
    format!("http://{addr}")
}

fn snapshot(db: &Path, to: &Path) {
    let conn = rusqlite::Connection::open(db).unwrap();
    conn.execute("VACUUM INTO ?1", [to.to_str().unwrap()])
        .unwrap();
}

struct Vault {
    ids: Vec<Identity>,
    material: Vec<VaultMaterial>,
}

async fn make_vault(relay: &RelayClient) -> Vault {
    let mut rng = StdRng::seed_from_u64(70);
    let ids: Vec<Identity> = (0..3).map(|_| Identity::generate(&mut rng)).collect();
    let invite = create_vault(relay, &ids[0], "Grants", 2, 3, &mut rng)
        .await
        .unwrap();
    let invite = Invite::decode(&invite.encode()).unwrap();
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
    let (mut r0, mut r1, mut r2) = (
        StdRng::seed_from_u64(1),
        StdRng::seed_from_u64(2),
        StdRng::seed_from_u64(3),
    );
    let (a, b, c) = tokio::join!(
        run_keygen(
            relay,
            &ids[0],
            &invite,
            &number,
            &net,
            "regtest",
            KeygenOptions {
                creator_birthday_height: Some(2),
                creator_expiry_blocks: None,
                max_birthday: None
            },
            member_dir(relay, &ids[0]),
            setup_key(&ids[0]),
            &mut r0,
            timeout
        ),
        run_keygen(
            relay,
            &ids[1],
            &invite,
            &number,
            &net,
            "regtest",
            KeygenOptions {
                creator_birthday_height: None,
                creator_expiry_blocks: None,
                max_birthday: None
            },
            member_dir(relay, &ids[1]),
            setup_key(&ids[1]),
            &mut r1,
            timeout
        ),
        run_keygen(
            relay,
            &ids[2],
            &invite,
            &number,
            &net,
            "regtest",
            KeygenOptions {
                creator_birthday_height: None,
                creator_expiry_blocks: None,
                max_birthday: None
            },
            member_dir(relay, &ids[2]),
            setup_key(&ids[2]),
            &mut r2,
            timeout
        ),
    );
    Vault {
        ids,
        material: vec![a.unwrap(), b.unwrap(), c.unwrap()],
    }
}

struct World {
    dir: tempfile::TempDir,
    url: String,
    vault: Vault,
}

impl World {
    fn db(&self) -> std::path::PathBuf {
        self.dir.path().join("relay.db")
    }

    fn client(&self, i: usize, url: &str) -> RelayClient {
        let cache = LogCache::in_dir(self.dir.path().join(format!("copy-{i}")));
        RelayClient::new(url).with_log_cache(Some(cache))
    }
}

async fn world() -> World {
    let dir = tempfile::tempdir().unwrap();
    let relay = Relay::open(&dir.path().join("relay.db")).unwrap();
    let url = serve(relay).await;
    let vault = make_vault(&RelayClient::new(&url).with_log_cache(None)).await;
    World { dir, url, vault }
}

async fn name(w: &World, i: usize, url: &str, text: &str) {
    let mut rng = StdRng::seed_from_u64(100 + i as u64);
    set_name(
        &w.client(i, url),
        &w.vault.ids[i],
        &w.vault.material[i],
        text,
        &mut rng,
    )
    .await
    .unwrap();
}

async fn len(w: &World, i: usize, url: &str) -> Result<u64, NodeError> {
    load_state(&w.client(i, url), &w.vault.ids[i], &w.vault.material[i])
        .await
        .map(|(chain, _)| chain.len())
}

async fn rewound(w: &World, tag: &str) -> String {
    let snap = w.dir.path().join(format!("{tag}.db"));
    snapshot(&w.db(), &snap);
    serve(Relay::open(&snap).unwrap()).await
}

#[tokio::test]
async fn a_relay_serving_an_older_log_is_refused() {
    let w = world().await;
    let before = len(&w, 0, &w.url).await.unwrap();
    let old = rewound(&w, "old").await;
    name(&w, 1, &w.url, "Bob").await;
    name(&w, 2, &w.url, "Carol").await;
    assert_eq!(len(&w, 0, &w.url).await.unwrap(), before + 2);

    let err = len(&w, 0, &old).await.unwrap_err();
    assert!(
        matches!(err, NodeError::RelayRolledBack { local_len } if local_len == before + 2),
        "{err:?}"
    );
    let fresh = RelayClient::new(&old).with_log_cache(None);
    let (chain, _) = load_state(&fresh, &w.vault.ids[2], &w.vault.material[2])
        .await
        .unwrap();
    assert_eq!(chain.len(), before);
}

#[tokio::test]
async fn a_relay_serving_another_history_is_refused() {
    let w = world().await;
    let old = rewound(&w, "fork").await;
    name(&w, 1, &w.url, "Bob").await;
    name(&w, 2, &w.url, "Carol").await;
    let total = len(&w, 0, &w.url).await.unwrap();
    let bare = RelayClient::new(&old).with_log_cache(None);
    let mut rng = StdRng::seed_from_u64(5);
    for (i, text) in [(0, "Zed"), (1, "Yan")] {
        set_name(&bare, &w.vault.ids[i], &w.vault.material[i], text, &mut rng)
            .await
            .unwrap();
    }
    let err = len(&w, 0, &old).await.unwrap_err();
    assert!(
        matches!(err, NodeError::RelayForked { index } if index == total - 1),
        "{err:?}"
    );
}

#[tokio::test]
async fn a_wiped_relay_is_reported_and_restored_from_a_members_copy() {
    let w = world().await;
    name(&w, 1, &w.url, "Bob").await;
    let total = len(&w, 0, &w.url).await.unwrap();
    assert_eq!(len(&w, 1, &w.url).await.unwrap(), total);

    let wiped = serve(Relay::new()).await;
    let err = len(&w, 0, &wiped).await.unwrap_err();
    assert!(
        matches!(err, NodeError::RelayLostVault { local_len } if local_len == total),
        "{err:?}"
    );
    assert!(len(&w, 2, &wiped).await.is_err());

    let relay = w.client(0, &wiped);
    let out = reseed_relay(
        &relay,
        &w.vault.ids[0],
        w.vault.material[0].ids(),
        &w.vault.material[0].log_key(),
    )
    .await
    .unwrap();
    assert_eq!(out, Reseeded::Restored { entries: total });

    for i in 0..3 {
        assert_eq!(len(&w, i, &wiped).await.unwrap(), total, "member {i}");
    }
    name(&w, 2, &wiped, "Carol").await;
    assert_eq!(len(&w, 0, &wiped).await.unwrap(), total + 1);
    let again = reseed_relay(
        &w.client(1, &wiped),
        &w.vault.ids[1],
        w.vault.material[1].ids(),
        &w.vault.material[1].log_key(),
    )
    .await
    .unwrap();
    assert_eq!(again, Reseeded::Current);
}

#[tokio::test]
async fn a_rewound_relay_is_caught_up_from_a_members_copy() {
    let w = world().await;
    let old = rewound(&w, "behind").await;
    name(&w, 1, &w.url, "Bob").await;
    name(&w, 2, &w.url, "Carol").await;
    let total = len(&w, 0, &w.url).await.unwrap();
    assert!(matches!(
        len(&w, 0, &old).await,
        Err(NodeError::RelayRolledBack { .. })
    ));

    let out = reseed_relay(
        &w.client(0, &old),
        &w.vault.ids[0],
        w.vault.material[0].ids(),
        &w.vault.material[0].log_key(),
    )
    .await
    .unwrap();
    assert_eq!(out, Reseeded::CaughtUp { appended: 2 });
    assert_eq!(len(&w, 0, &old).await.unwrap(), total);
    assert_eq!(len(&w, 2, &old).await.unwrap(), total);
}

#[tokio::test]
async fn restoring_never_overwrites_another_history() {
    let w = world().await;
    let other = rewound(&w, "other").await;
    name(&w, 1, &w.url, "Bob").await;
    assert!(len(&w, 0, &w.url).await.is_ok());
    let bare = RelayClient::new(&other).with_log_cache(None);
    let mut rng = StdRng::seed_from_u64(6);
    set_name(
        &bare,
        &w.vault.ids[2],
        &w.vault.material[2],
        "Carol",
        &mut rng,
    )
    .await
    .unwrap();
    let err = reseed_relay(
        &w.client(0, &other),
        &w.vault.ids[0],
        w.vault.material[0].ids(),
        &w.vault.material[0].log_key(),
    )
    .await;
    assert!(matches!(err, Err(NodeError::RelayForked { .. })), "{err:?}");
}

#[tokio::test]
async fn a_restore_in_progress_cannot_be_joined_or_continued_by_others() {
    use zafe_proto::relay::Reseed;
    let w = world().await;
    name(&w, 1, &w.url, "Bob").await;
    name(&w, 2, &w.url, "Carol").await;
    let total = len(&w, 0, &w.url).await.unwrap();
    assert!(total >= 3);
    let wiped = serve(Relay::new()).await;
    let relay = w.client(0, &wiped);
    let entries = w.client(0, &w.url);
    let (chain, state) = load_state(&entries, &w.vault.ids[0], &w.vault.material[0])
        .await
        .unwrap();
    assert_eq!(chain.len(), total);
    let raw: Vec<Vec<u8>> = chain
        .entries()
        .iter()
        .map(|e| e.to_bytes().unwrap())
        .collect();
    let request = |from: usize, finish: bool| Reseed {
        mailbox: w.vault.material[0].label(),
        members: state.member_identities(),
        threshold: 2,
        from: from as u64,
        entries: raw[from..from + 1].to_vec(),
        finish,
        timestamp: 0,
    };
    let a = relay
        .reseed(&w.vault.ids[0], request(0, false))
        .await
        .unwrap();
    assert_eq!((a.len, a.open, a.restored), (1, false, true));
    assert!(w
        .client(1, &wiped)
        .reseed(&w.vault.ids[1], request(1, false))
        .await
        .is_err());
    assert!(w
        .client(1, &wiped)
        .read_log(&w.vault.ids[1], request(0, false).mailbox, 0)
        .await
        .is_ok());
    let b = relay
        .reseed(&w.vault.ids[0], request(0, false))
        .await
        .unwrap();
    assert_eq!(b.len, 1);
    assert!(relay
        .reseed(
            &w.vault.ids[0],
            Reseed {
                entries: raw[2..3].to_vec(),
                ..request(1, false)
            }
        )
        .await
        .is_err());
    let mut rng = StdRng::seed_from_u64(9);
    assert!(set_name(
        &w.client(0, &wiped),
        &w.vault.ids[0],
        &w.vault.material[0],
        "X",
        &mut rng
    )
    .await
    .is_err());
}

#[tokio::test]
async fn a_mailbox_still_being_set_up_is_not_reseeded() {
    use zafe_proto::relay::Reseed;
    let url = serve(Relay::new()).await;
    let relay = RelayClient::new(&url).with_log_cache(None);
    let mut rng = StdRng::seed_from_u64(31);
    let ids: Vec<Identity> = (0..2).map(|_| Identity::generate(&mut rng)).collect();
    let invite = create_vault(&relay, &ids[0], "Setup", 2, 2, &mut rng)
        .await
        .unwrap();
    let request = Reseed {
        mailbox: invite.label,
        members: ids.iter().map(|i| *i.public()).collect(),
        threshold: 2,
        from: 0,
        entries: vec![],
        finish: true,
        timestamp: 0,
    };
    assert!(relay.reseed(&ids[0], request).await.is_err());
}

#[tokio::test]
async fn a_relay_restored_with_other_members_or_threshold_is_refused() {
    use zafe_proto::relay::Reseed;

    let w = world().await;
    let total = len(&w, 2, &w.url).await.unwrap();
    let entries: Vec<Vec<u8>> = w
        .client(2, &w.url)
        .log_cache()
        .unwrap()
        .read(&w.vault.material[2].descriptor.vault_id)
        .unwrap()
        .iter()
        .map(|e| e.to_bytes().unwrap())
        .collect();
    assert_eq!(entries.len() as u64, total);
    let honest: Vec<_> = w.vault.material[2]
        .descriptor
        .members
        .iter()
        .map(|m| m.identity)
        .collect();
    let mut rng = StdRng::seed_from_u64(900);
    let intruder_id = Identity::generate(&mut rng);
    let intruder = intruder_id.public();

    let attacks: Vec<(&str, Vec<_>, u16)> = vec![
        ("threshold 1", honest.clone(), 1),
        ("extra key", [honest.clone(), vec![*intruder]].concat(), 2),
        ("member left out", honest[1..].to_vec(), 2),
    ];
    let mut checked = 0;
    for (name, members, threshold) in attacks {
        let url = serve(Relay::new()).await;
        let bare = RelayClient::new(&url).with_log_cache(None);
        let answer = bare
            .reseed(
                &w.vault.ids[2],
                Reseed {
                    mailbox: w.vault.material[2].label(),
                    members,
                    threshold,
                    from: 0,
                    entries: entries.clone(),
                    finish: true,
                    timestamp: 0,
                },
            )
            .await;
        if answer.is_err() {
            continue;
        }
        let err = len(&w, 0, &url).await.unwrap_err();
        assert!(
            matches!(err, NodeError::RelayMembership(_)),
            "{name}: {err:?}"
        );
        checked += 1;
    }
    assert!(checked >= 2, "the relay accepted too few of the attacks");

    let url = serve(Relay::new()).await;
    reseed_relay(
        &w.client(2, &url),
        &w.vault.ids[2],
        w.vault.material[2].ids(),
        &w.vault.material[2].log_key(),
    )
    .await
    .unwrap();
    assert_eq!(len(&w, 0, &url).await.unwrap(), total);
}

#[tokio::test]
async fn a_forked_device_can_discard_its_copy_and_follow_the_relay() {
    let w = world().await;
    let old = rewound(&w, "fork-follow").await;
    name(&w, 1, &w.url, "Bob").await;
    name(&w, 2, &w.url, "Carol").await;
    let total = len(&w, 0, &w.url).await.unwrap();
    let bare = RelayClient::new(&old).with_log_cache(None);
    let mut rng = StdRng::seed_from_u64(5);
    for (i, text) in [(0, "Zed"), (1, "Yan")] {
        set_name(&bare, &w.vault.ids[i], &w.vault.material[i], text, &mut rng)
            .await
            .unwrap();
    }
    let ids = w.vault.material[0].ids();
    let key = w.vault.material[0].log_key();
    assert!(matches!(
        len(&w, 0, &old).await,
        Err(NodeError::RelayForked { .. })
    ));

    let refused = follow_relay(&w.client(0, &w.url), &w.vault.ids[0], ids, &key).await;
    assert!(
        matches!(refused, Err(NodeError::Protocol(_))),
        "{refused:?}"
    );
    assert_eq!(len(&w, 0, &w.url).await.unwrap(), total);

    let out = follow_relay(&w.client(0, &old), &w.vault.ids[0], ids, &key)
        .await
        .unwrap();
    assert_eq!(
        out.dropped, 2,
        "Bob and Carol exist only in this device's copy"
    );
    assert_eq!(out.entries, total);
    assert_eq!(len(&w, 0, &old).await.unwrap(), total);
    assert!(matches!(
        len(&w, 0, &w.url).await,
        Err(NodeError::RelayForked { .. })
    ));
}

#[tokio::test]
async fn a_copy_that_does_not_verify_is_an_error_until_the_member_follows_the_relay() {
    let w = world().await;
    name(&w, 1, &w.url, "Bob").await;
    let total = len(&w, 0, &w.url).await.unwrap();
    let mailbox = w.vault.material[0].descriptor.vault_id;
    let ids = w.vault.material[0].ids();
    let key = w.vault.material[0].log_key();

    let cache = LogCache::in_dir(w.dir.path().join("copy-0"));
    let mut saved = cache.read(&mailbox).unwrap();
    saved.last_mut().unwrap().ciphertext[30] ^= 1;
    cache.remove(&mailbox);
    assert!(cache.write(&mailbox, &saved).unwrap());

    let err = len(&w, 0, &w.url).await.unwrap_err();
    assert!(matches!(err, NodeError::LogCopyInvalid(_)), "{err:?}");

    let out = follow_relay(&w.client(0, &w.url), &w.vault.ids[0], ids, &key)
        .await
        .unwrap();
    assert_eq!(out.entries, total);
    assert_eq!(out.dropped, 1, "only the damaged entry differs");
    assert_eq!(len(&w, 0, &w.url).await.unwrap(), total);
}

#[tokio::test]
async fn a_new_vault_lives_under_a_label() {
    let w = world().await;
    let m = &w.vault.material[0];
    assert_ne!(m.label(), m.descriptor.vault_id);
    assert_eq!(m.relay.url.as_deref(), Some(w.url.as_str()));
    let relay = w.client(0, &w.url);
    let (chain, _) = load_state(&relay, &w.vault.ids[0], m).await.unwrap();
    assert!(chain
        .entries()
        .iter()
        .all(|e| e.header.mailbox == m.descriptor.vault_id));
    let by_id = relay
        .read_log(&w.vault.ids[0], m.descriptor.vault_id, 0)
        .await;
    assert!(
        matches!(
            by_id,
            Err(zafe_core::relay_client::RelayClientError::Status { status: 404, .. })
        ),
        "{by_id:?}"
    );
    for other in &w.vault.material[1..] {
        assert_eq!(other.ids(), m.ids());
    }
}

#[tokio::test]
async fn a_restored_backup_refuses_a_relay_with_a_shorter_history() {
    use zafe_core::backup::{self, Contents, KdfParams};
    let w = world().await;
    let old = rewound(&w, "pre-backup").await;
    name(&w, 1, &w.url, "Bob").await;
    let total = len(&w, 2, &w.url).await.unwrap();

    let log: Vec<Vec<u8>> = w
        .client(2, &w.url)
        .log_cache()
        .unwrap()
        .read(&w.vault.material[2].descriptor.vault_id)
        .unwrap()
        .iter()
        .map(|e| e.to_bytes().unwrap())
        .collect();
    assert_eq!(log.len() as u64, total);
    let pass = "correct horse battery staple orbit lantern violet harbor cactus";
    let mut rng = StdRng::seed_from_u64(44);
    let bytes = backup::encrypt(
        &Contents {
            identity_seeds: w.vault.ids[2].seeds().to_bytes(),
            material: w.vault.material[2].to_bytes().unwrap(),
            invite: String::new(),
            created_at: 1,
            names: Default::default(),
            log,
        },
        pass,
        KdfParams::DEFAULT,
        &mut rng,
    )
    .unwrap();

    let contents = backup::decrypt(&bytes, pass).unwrap();
    let material = contents.validate().unwrap();
    assert_eq!(material.ids(), w.vault.material[2].ids());
    let fresh = LogCache::in_dir(w.dir.path().join("restored"));
    assert_eq!(
        install_log_copy(&fresh, &material, &contents.log).unwrap(),
        total
    );
    let restored = |url: &str| RelayClient::new(url).with_log_cache(Some(fresh.clone()));
    let err = load_state(&restored(&old), &w.vault.ids[2], &material)
        .await
        .unwrap_err();
    assert!(
        matches!(err, NodeError::RelayRolledBack { local_len } if local_len == total),
        "{err:?}"
    );
    let (chain, _) = load_state(&restored(&w.url), &w.vault.ids[2], &material)
        .await
        .unwrap();
    assert_eq!(chain.len(), total);

    let other = LogCache::in_dir(w.dir.path().join("restored-bad"));
    let mut bad = contents.log.clone();
    let last = bad.last_mut().unwrap();
    let n = last.len();
    last[n - 1] ^= 1;
    assert!(install_log_copy(&other, &material, &bad).is_err());
    assert!(other
        .read(&material.descriptor.vault_id)
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn the_spend_alert_falls_back_to_the_log_copy_when_the_relay_is_down() {
    let w = world().await;
    name(&w, 1, &w.url, "Bob").await;
    let live = w.client(0, &w.url);
    let (_, expected) = load_state(&live, &w.vault.ids[0], &w.vault.material[0])
        .await
        .unwrap();
    let cache = live.log_cache().unwrap().clone();
    let vault = w.vault.material[0].descriptor.vault_id;
    let reached = cache.relay_status(&vault);
    assert!(reached.last_reached.is_some());
    assert!(reached.unreachable_since.is_none());

    let mut down = w.vault.material[0].clone();
    down.relay.url = Some("http://127.0.0.1:9".into());
    let relay =
        RelayClient::new(down.relay.url.clone().unwrap()).with_log_cache(Some(cache.clone()));
    let err = load_state(&relay, &w.vault.ids[0], &down)
        .await
        .unwrap_err();
    assert!(zafe_core::node::relay_unreachable(&err), "{err:?}");
    let status = cache.relay_status(&vault);
    assert_eq!(status.last_reached, reached.last_reached);
    assert!(status.unreachable_since.is_some());

    let (state, as_of) = zafe_core::node::state_from_copy(&relay, &down)
        .unwrap()
        .expect("this phone has a copy");
    assert_eq!(Some(as_of), reached.last_reached);
    assert_eq!(state.names, expected.names);
    assert_eq!(state.proposals.len(), expected.proposals.len());
    assert!(zafe_core::spend_watch::unapproved_spends(&state, &[]).is_empty());

    load_state(&live, &w.vault.ids[0], &w.vault.material[0])
        .await
        .unwrap();
    assert!(cache.relay_status(&vault).unreachable_since.is_none());
}

#[tokio::test]
async fn a_recovered_seat_refuses_a_relay_with_a_shorter_history() {
    use zafe_core::repair::{
        approve_replacement, help_repairs, new_phone_words, recovery_checks, try_recover,
        RecoveryRequest, RecoveryStatus,
    };
    let w = world().await;
    let mut rng = StdRng::seed_from_u64(81);
    let fresh = Identity::generate(&mut rng);
    let request = RecoveryRequest {
        identity: *fresh.public(),
    };
    let new_phone = |url: &str, tag: &str| {
        RelayClient::new(url).with_log_cache(Some(LogCache::in_dir(w.dir.path().join(tag))))
    };
    let phone = new_phone(&w.url, "new-phone-probe");
    let old = w.vault.ids[0].public().sig_pk;
    for i in [1, 2] {
        let relay = w.client(i, &w.url);
        let mut words = None;
        for _ in 0..4 {
            recovery_checks(&phone, &fresh, &w.dir.path().join("recovery"))
                .await
                .unwrap();
            words = new_phone_words(&relay, &w.vault.ids[i], fresh.public())
                .await
                .unwrap();
            if words.is_some() {
                break;
            }
        }
        approve_replacement(
            &relay,
            &w.vault.ids[i],
            &w.vault.material[i],
            old,
            &request,
            &words.unwrap(),
            &mut rng,
        )
        .await
        .unwrap();
    }
    for i in [1, 2, 1] {
        let relay = w.client(i, &w.url);
        let (chain, state) = load_state(&relay, &w.vault.ids[i], &w.vault.material[i])
            .await
            .unwrap();
        let dir = w.dir.path().join(format!("repair-{i}"));
        help_repairs(
            &relay,
            &w.vault.ids[i],
            &w.vault.material[i],
            &chain,
            &state,
            &dir,
            std::time::Duration::ZERO,
            &mut rng,
        )
        .await
        .unwrap();
    }
    let total = len(&w, 1, &w.url).await.unwrap();

    let snap = w.dir.path().join("lost-entry.db");
    snapshot(&w.db(), &snap);
    {
        let db = rusqlite::Connection::open(&snap).unwrap();
        let n = db
            .execute(
                "DELETE FROM log_entries WHERE mailbox = ?1 AND idx = ?2",
                rusqlite::params![&w.vault.material[1].label()[..], (total - 1) as i64],
            )
            .unwrap();
        assert_eq!(n, 1);
    }
    let lossy = serve(Relay::open(&snap).unwrap()).await;
    let label = Some(w.vault.material[1].label());
    let confirmed = [1, 2].map(|i| w.vault.ids[i].public().sig_pk);
    let err = try_recover(&new_phone(&lossy, "new-phone-a"), &fresh, label, &confirmed)
        .await
        .unwrap_err();
    assert!(
        matches!(err, NodeError::RelayRolledBack { local_len } if local_len == total),
        "{err:?}"
    );
    assert!(LogCache::in_dir(w.dir.path().join("new-phone-a"))
        .read(&w.vault.material[1].descriptor.vault_id)
        .unwrap()
        .is_empty());

    let honest = new_phone(&w.url, "new-phone-b");
    let RecoveryStatus::Done { material, invite } = try_recover(&honest, &fresh, label, &confirmed)
        .await
        .unwrap()
    else {
        panic!("not recovered");
    };
    assert_eq!(material.ids(), w.vault.material[1].ids());
    assert_eq!(invite.ids(), material.ids());
    assert_eq!(
        honest
            .log_cache()
            .unwrap()
            .read(&material.descriptor.vault_id)
            .unwrap()
            .len() as u64,
        total
    );
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
