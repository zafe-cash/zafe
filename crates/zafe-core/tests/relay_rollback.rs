//! A relay that lost or rewound the vault log is refused, and a member can restore it from
//! the copy of the log kept on the device (spec §6.3, §14).
//!
//! The "old relay" is a second relay started from a snapshot of the first one's database
//! taken earlier (a restored backup); "wiped" is an empty relay.

use std::{path::Path, time::Duration};

use rand::{rngs::StdRng, SeedableRng};
use zafe_core::{
    log_cache::LogCache,
    node::{
        create_vault, follow_relay, join_vault, load_state, reseed_relay, run_keygen, seal,
        set_name, Invite, NodeError, Reseeded, VaultMaterial,
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

/// A copy of the live database as of now (what restoring last night's backup gives).
fn snapshot(db: &Path, to: &Path) {
    let conn = rusqlite::Connection::open(db).unwrap();
    conn.execute("VACUUM INTO ?1", [to.to_str().unwrap()])
        .unwrap();
}

struct Vault {
    ids: Vec<Identity>,
    material: Vec<VaultMaterial>,
}

/// Three members make a 2-of-3 vault on `relay`.
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
    let (_, _, number) = zafe_core::node::membership(relay, &ids[0], &invite)
        .await
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
            Some(2),
            None,
            &mut r0,
            timeout
        ),
        run_keygen(relay, &ids[1], &invite, &number, &net, "regtest", None, None, &mut r1, timeout),
        run_keygen(relay, &ids[2], &invite, &number, &net, "regtest", None, None, &mut r2, timeout),
    );
    Vault {
        ids,
        material: vec![a.unwrap(), b.unwrap(), c.unwrap()],
    }
}

struct World {
    dir: tempfile::TempDir,
    /// The relay members use, on a database file.
    url: String,
    vault: Vault,
}

impl World {
    fn db(&self) -> std::path::PathBuf {
        self.dir.path().join("relay.db")
    }

    /// Member `i`'s view of the relay at `url`, with its own saved copy of the log.
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

/// A relay started from a snapshot of the live one, as the member at `url` left it.
async fn rewound(w: &World, tag: &str) -> String {
    let snap = w.dir.path().join(format!("{tag}.db"));
    snapshot(&w.db(), &snap);
    serve(Relay::open(&snap).unwrap()).await
}

#[tokio::test]
async fn a_relay_serving_an_older_log_is_refused() {
    let w = world().await;
    // Member 0 has seen the log as it is now.
    let before = len(&w, 0, &w.url).await.unwrap();
    // A backup of the relay is taken, then the vault moves on.
    let old = rewound(&w, "old").await;
    name(&w, 1, &w.url, "Bob").await;
    name(&w, 2, &w.url, "Carol").await;
    assert_eq!(len(&w, 0, &w.url).await.unwrap(), before + 2);

    // The relay comes back from the backup: two entries are gone.
    let err = len(&w, 0, &old).await.unwrap_err();
    assert!(
        matches!(err, NodeError::RelayRolledBack { local_len } if local_len == before + 2),
        "{err:?}"
    );
    // A device with no copy of the log (a new install) can't tell either: it trusts what
    // it is served the first time.
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
    // On the restored relay the vault went on differently: two other entries.
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
    // Members 0 and 1 hold the log; member 2 has never loaded it on this device.
    assert_eq!(len(&w, 1, &w.url).await.unwrap(), total);

    let wiped = serve(Relay::new()).await;
    let err = len(&w, 0, &wiped).await.unwrap_err();
    assert!(
        matches!(err, NodeError::RelayLostVault { local_len } if local_len == total),
        "{err:?}"
    );
    // A device with no copy gets the plain "unknown vault" error from the relay.
    assert!(len(&w, 2, &wiped).await.is_err());

    // Member 0 restores the relay from its copy.
    let relay = w.client(0, &wiped);
    let out = reseed_relay(
        &relay,
        &w.vault.ids[0],
        w.vault.material[0].descriptor.vault_id,
        &w.vault.material[0].log_key(),
    )
    .await
    .unwrap();
    assert_eq!(out, Reseeded::Restored { entries: total });

    // Everyone reads the same log from it, and keeps appending.
    for i in 0..3 {
        assert_eq!(len(&w, i, &wiped).await.unwrap(), total, "member {i}");
    }
    name(&w, 2, &wiped, "Carol").await;
    assert_eq!(len(&w, 0, &wiped).await.unwrap(), total + 1);
    // Another member's attempt finds it already restored.
    let again = reseed_relay(
        &w.client(1, &wiped),
        &w.vault.ids[1],
        w.vault.material[1].descriptor.vault_id,
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
        w.vault.material[0].descriptor.vault_id,
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
    // The other relay's next entry differs from the live one's.
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
        w.vault.material[0].descriptor.vault_id,
        &w.vault.material[0].log_key(),
    )
    .await;
    // Same length as the copy but another entry at the end: a fork, not a restore.
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
        mailbox: w.vault.material[0].descriptor.vault_id,
        members: state.member_identities(),
        threshold: 2,
        from: from as u64,
        entries: raw[from..from + 1].to_vec(),
        finish,
        timestamp: 0,
    };
    // The first entry only: not open yet.
    let a = relay
        .reseed(&w.vault.ids[0], request(0, false))
        .await
        .unwrap();
    assert_eq!((a.len, a.open, a.restored), (1, false, true));
    // Another member can't push entries into it (it isn't theirs), nor can anyone read it.
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
    // A repeated request reports where the log is instead of failing or duplicating.
    let b = relay
        .reseed(&w.vault.ids[0], request(0, false))
        .await
        .unwrap();
    assert_eq!(b.len, 1);
    // Entries that skip ahead or don't chain are refused.
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
    // Not open: ordinary appends are refused until it is finished.
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

/// A mailbox the creator is still setting up (invite out, members joining) is not a
/// restore target: reseeding can't be used to push a log into it or to seal it.
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
        mailbox: invite.mailbox,
        members: ids.iter().map(|i| *i.public()).collect(),
        threshold: 2,
        from: 0,
        entries: vec![],
        finish: true,
        timestamp: 0,
    };
    assert!(relay.reseed(&ids[0], request).await.is_err());
}

/// A member restoring a wiped relay chooses its members and threshold; the relay can't read
/// the log to check them, so the other members do (spec §6.3).
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
                    mailbox: w.vault.material[2].descriptor.vault_id,
                    members,
                    threshold,
                    from: 0,
                    entries: entries.clone(),
                    finish: true,
                    timestamp: 0,
                },
            )
            .await;
        // A relay may refuse a list that omits its own signer; the others get through.
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

    // An honest restore still passes the check.
    let url = serve(Relay::new()).await;
    reseed_relay(
        &w.client(2, &url),
        &w.vault.ids[2],
        w.vault.material[2].descriptor.vault_id,
        &w.vault.material[2].log_key(),
    )
    .await
    .unwrap();
    assert_eq!(len(&w, 0, &url).await.unwrap(), total);
}

/// After `RelayForked` the minority device can choose to follow the relay.
#[tokio::test]
async fn a_forked_device_can_discard_its_copy_and_follow_the_relay() {
    let w = world().await;
    let old = rewound(&w, "fork-follow").await;
    name(&w, 1, &w.url, "Bob").await;
    name(&w, 2, &w.url, "Carol").await;
    let total = len(&w, 0, &w.url).await.unwrap();
    // Two other entries on the relay the device is about to meet.
    let bare = RelayClient::new(&old).with_log_cache(None);
    let mut rng = StdRng::seed_from_u64(5);
    for (i, text) in [(0, "Zed"), (1, "Yan")] {
        set_name(&bare, &w.vault.ids[i], &w.vault.material[i], text, &mut rng)
            .await
            .unwrap();
    }
    let mailbox = w.vault.material[0].descriptor.vault_id;
    let key = w.vault.material[0].log_key();
    assert!(matches!(
        len(&w, 0, &old).await,
        Err(NodeError::RelayForked { .. })
    ));

    // Not a fork: nothing is discarded.
    let refused = follow_relay(&w.client(0, &w.url), &w.vault.ids[0], mailbox, &key).await;
    assert!(
        matches!(refused, Err(NodeError::Protocol(_))),
        "{refused:?}"
    );
    assert_eq!(len(&w, 0, &w.url).await.unwrap(), total);

    let out = follow_relay(&w.client(0, &old), &w.vault.ids[0], mailbox, &key)
        .await
        .unwrap();
    assert_eq!(
        out.dropped, 2,
        "Bob and Carol exist only in this device's copy"
    );
    assert_eq!(out.entries, total);
    assert_eq!(len(&w, 0, &old).await.unwrap(), total);
    // The copy now follows that relay: the first relay is the one that looks forked.
    assert!(matches!(
        len(&w, 0, &w.url).await,
        Err(NodeError::RelayForked { .. })
    ));
}
