//! Moving a vault to another relay (`docs/relay-move.md` §4, §11): two in-process relays,
//! members on 2-of-3 and 2-of-5 vaults, each with its own copy of the log.

use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use rand::{rngs::StdRng, SeedableRng};
use zafe_core::{
    log_cache::LogCache,
    moves::{MoveLink, MoveMessage, MoveTarget},
    node::{
        self, create_vault, followed_relay_in, install_log_copy, join_vault, load_state,
        run_keygen, seal, set_name, Invite, KeygenOptions, MoveStatus, NodeError, VaultMaterial,
    },
    relay_client::RelayClient,
    wallet::regtest_network,
};
use zafe_proto::{Envelope, Identity, Kind};
use zafe_relay::Relay;

async fn serve(relay: Relay) -> String {
    serve_counting(relay).await.0
}

/// A relay and the number of requests it has answered.
async fn serve_counting(relay: Relay) -> (String, Arc<AtomicUsize>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = hits.clone();
    let app = relay.router().layer(axum::middleware::from_fn(
        move |req: axum::extract::Request, next: axum::middleware::Next| {
            let counter = counter.clone();
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                next.run(req).await
            }
        },
    ));
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{addr}"), hits)
}

struct World {
    dir: tempfile::TempDir,
    /// The relay the vault was made on.
    old: String,
    ids: Vec<Identity>,
    material: Vec<VaultMaterial>,
}

impl World {
    fn cache_dir(&self, i: usize) -> PathBuf {
        self.dir.path().join(format!("copy-{i}"))
    }

    /// Member `i`'s client for `url`, with its own copy of the log.
    fn client(&self, i: usize, url: &str) -> RelayClient {
        RelayClient::new(url).with_log_cache(Some(LogCache::in_dir(self.cache_dir(i))))
    }

    fn old(&self, i: usize) -> RelayClient {
        self.client(i, &self.old)
    }

    /// Member `i`'s material as its app would hold it: following the new relay once this
    /// phone verified the switch.
    fn material(&self, i: usize) -> VaultMaterial {
        let mut m = self.material[i].clone();
        if let Some(relay) = followed_relay_in(&self.cache_dir(i), &m) {
            m.relay = relay;
        }
        m
    }

    /// Member `i`'s client for wherever its vault lives now.
    fn here(&self, i: usize) -> RelayClient {
        let m = self.material(i);
        self.client(i, m.relay.url.as_deref().unwrap_or(&self.old))
    }

    async fn len(&self, i: usize, client: &RelayClient) -> u64 {
        load_state(client, &self.ids[i], &self.material(i))
            .await
            .unwrap()
            .0
            .len()
    }

    /// The old relay's log length, read as a new phone would (no copy, no move state).
    async fn old_len(&self) -> u64 {
        let bare = RelayClient::new(&self.old).with_log_cache(None);
        load_state(&bare, &self.ids[1], &self.material[1])
            .await
            .unwrap()
            .0
            .len()
    }

    async fn name(&self, i: usize, text: &str) -> Result<(), NodeError> {
        let mut rng = StdRng::seed_from_u64(200 + i as u64);
        set_name(
            &self.here(i),
            &self.ids[i],
            &self.material(i),
            text,
            &mut rng,
        )
        .await
    }

    /// Every member's app writes the current event version once (as each app does on its
    /// first refresh after an update).
    async fn announce_all(&self) {
        for i in 0..self.ids.len() {
            self.announce(i).await;
        }
    }

    async fn announce(&self, i: usize) {
        let client = self.old(i);
        let m = self.material(i);
        let (mut chain, mut state) = load_state(&client, &self.ids[i], &m).await.unwrap();
        node::announce_version(
            &client,
            &self.ids[i],
            &m,
            &mut chain,
            &mut state,
            &mut StdRng::seed_from_u64(300 + i as u64),
        )
        .await
        .unwrap();
    }

    async fn move_to(&self, i: usize, url: &str) -> Result<node::MoveStarted, NodeError> {
        node::move_vault(
            &self.old(i),
            &self.ids[i],
            &self.material(i),
            url,
            None,
            &mut StdRng::seed_from_u64(400 + i as u64),
        )
        .await
    }

    async fn follow(&self, i: usize, link: &str) -> Result<MoveStatus, NodeError> {
        Box::pin(node::follow_move(
            &self.old(i),
            &self.ids[i],
            &self.material(i),
            &MoveLink::decode(link).unwrap(),
            &mut StdRng::seed_from_u64(500 + i as u64),
        ))
        .await
    }

    async fn progress(&self, i: usize) -> Option<MoveStatus> {
        node::move_progress(
            &self.old(i),
            &self.ids[i],
            &self.material(i),
            &mut StdRng::seed_from_u64(600 + i as u64),
        )
        .await
        .unwrap()
    }

    async fn notice(&self, i: usize) -> Result<Option<node::MoveNotice>, NodeError> {
        node::move_notice(&self.old(i), &self.ids[i], &self.material[i]).await
    }
}

fn opts(creator: bool) -> KeygenOptions {
    KeygenOptions {
        creator_birthday_height: creator.then_some(2),
        creator_expiry_blocks: None,
        max_birthday: None,
    }
}

/// `n` members make a `t`-of-`n` vault on a fresh relay; every member loads the log once
/// (so each phone holds a copy).
async fn world(t: u16, n: u16, seed: u64) -> World {
    let dir = tempfile::tempdir().unwrap();
    let old = serve(Relay::new()).await;
    let relay = RelayClient::new(&old).with_log_cache(None);
    let mut rng = StdRng::seed_from_u64(seed);
    let ids: Vec<Identity> = (0..n).map(|_| Identity::generate(&mut rng)).collect();
    let invite = create_vault(&relay, &ids[0], "Grants", t, n, &mut rng)
        .await
        .unwrap();
    let invite = Invite::decode(&invite.encode()).unwrap();
    for id in &ids[1..] {
        join_vault(&relay, id, &invite).await.unwrap();
    }
    seal(&relay, &ids[0], &invite).await.unwrap();
    let words = futures::future::join_all(
        ids.iter()
            .map(|id| node::wait_for_safety_words(&relay, id, &invite, Duration::from_secs(30))),
    )
    .await
    .into_iter()
    .map(Result::unwrap)
    .next()
    .unwrap();
    let net = regtest_network();
    let material = futures::future::join_all(ids.iter().enumerate().map(|(i, id)| {
        let (relay, invite, words, net, dir) = (&relay, &invite, &words, &net, dir.path());
        async move {
            run_keygen(
                relay,
                id,
                invite,
                words,
                net,
                "regtest",
                opts(i == 0),
                dir.join(format!("keygen-{i}")),
                zafe_core::nonce_store::NonceKey::derive(&[i as u8; 32], &[0; 16]),
                &mut StdRng::seed_from_u64(seed + 10 + i as u64),
                Duration::from_secs(120),
            )
            .await
            .unwrap()
        }
    }))
    .await;
    let w = World {
        dir,
        old,
        ids,
        material,
    };
    for i in 0..n as usize {
        w.len(i, &w.old(i)).await;
    }
    w
}

#[tokio::test]
async fn moving_needs_every_members_app_to_support_it() {
    let w = world(2, 3, 1).await;
    let new = serve(Relay::new()).await;
    // Member 0 updated (its move announces it); 1 and 2 haven't written anything yet.
    let err = w.move_to(0, &new).await.unwrap_err();
    match err {
        NodeError::MembersMustUpdate(mut behind) => {
            behind.sort();
            let mut want = vec![w.ids[1].public().sig_pk, w.ids[2].public().sig_pk];
            want.sort();
            assert_eq!(behind, want)
        }
        e => panic!("{e:?}"),
    }
    w.announce_all().await;
    w.move_to(0, &new).await.unwrap();
}

#[tokio::test]
async fn a_move_with_the_old_relay_alive() {
    let w = world(2, 3, 2).await;
    w.announce_all().await;
    let (new, new_hits) = serve_counting(Relay::new()).await;
    let started = w.move_to(0, &new).await.unwrap();
    assert_eq!(started.notified.len(), 2, "notices for both other members");
    assert!(started.link.starts_with("https://zafe.cash/move#"));

    // Payments (here: names) go on on the old relay until the switch.
    w.name(2, "Carol").await.unwrap();
    let old_len = w.len(2, &w.old(2)).await;

    // Bob sees the card from the old relay; nothing has connected to the new one.
    let before = new_hits.load(Ordering::SeqCst);
    let notice = w.notice(1).await.unwrap().expect("a move notice");
    assert_eq!(notice.target, started.target);
    assert!(!notice.moved);
    MoveLink::decode(&started.link).unwrap();
    assert_eq!(
        new_hits.load(Ordering::SeqCst),
        before,
        "no request before the tap"
    );

    // Bob taps Move: 2 of 3 have moved, which is m for 2-of-3, so the vault switches.
    let status = w.follow(1, &started.link).await.unwrap();
    assert_eq!(status.needed, 2);
    assert!(status.switched, "{status:?}");
    assert!(new_hits.load(Ordering::SeqCst) > before);

    // The new relay holds everything the old one had (Carol's name included), then the
    // switch.
    let bob = w.here(1);
    assert_eq!(bob.base_url(), new);
    let (chain, state) = load_state(&bob, &w.ids[1], &w.material(1)).await.unwrap();
    assert_eq!(chain.len(), old_len + 1);
    let moved = state.moved().expect("switched");
    assert_eq!(moved.index, old_len);
    assert_eq!(moved.target, started.target);
    assert_eq!(state.names[&w.ids[2].public().sig_pk], "Carol");

    // Carol was off: she finds the switch in the notice and can't write to the old relay.
    let notice = w.notice(2).await.unwrap().unwrap();
    assert!(
        notice.moved,
        "the switching phone left VaultMoved in the notices"
    );
    let err = w.name(2, "Carol B").await.unwrap_err();
    assert!(
        matches!(&err, NodeError::VaultMoved { url } if *url == started.target.relay),
        "{err:?}"
    );
    // She follows later and writes to the new relay.
    let status = w.follow(2, &started.link).await.unwrap();
    assert!(status.switched);
    w.name(2, "Carol B").await.unwrap();
    assert_eq!(w.len(2, &w.here(2)).await, old_len + 2);
    // Alice, the mover, sees the switch on her next refresh.
    assert!(w.progress(0).await.unwrap().switched);
    assert_eq!(w.here(0).base_url(), new);
    assert_eq!(w.len(0, &w.here(0)).await, old_len + 2);
    // The old relay got nothing after the switch.
    assert_eq!(w.old_len().await, old_len);
}

#[tokio::test]
async fn a_move_with_the_old_relay_dead() {
    let w = world(2, 3, 3).await;
    w.announce_all().await;
    // The phones' copies are current from their last refresh before the relay died.
    for i in 0..3 {
        w.len(i, &w.old(i)).await;
    }
    let dead = "http://127.0.0.1:9";
    let new = serve(Relay::new()).await;
    let client = |i: usize| w.client(i, dead);
    let mut rng = StdRng::seed_from_u64(9);
    let started = node::move_vault(&client(0), &w.ids[0], &w.material(0), &new, None, &mut rng)
        .await
        .unwrap();
    assert!(started.notified.is_empty(), "the old relay took no notices");
    let status = node::follow_move(
        &client(1),
        &w.ids[1],
        &w.material(1),
        &MoveLink::decode(&started.link).unwrap(),
        &mut rng,
    )
    .await
    .unwrap();
    assert!(status.switched);
    // Both write on the new relay now.
    w.name(0, "Alice").await.unwrap();
    w.name(1, "Bob").await.unwrap();
    let (_, state) = load_state(&w.here(1), &w.ids[1], &w.material(1))
        .await
        .unwrap();
    assert_eq!(state.names.len(), 2);
}

#[tokio::test]
async fn two_of_five_switches_at_four_not_at_two_or_three() {
    let w = world(2, 5, 4).await;
    w.announce_all().await;
    let new = serve(Relay::new()).await;
    let started = w.move_to(0, &new).await.unwrap();
    for (i, expect) in [(1, 2), (2, 3)] {
        let s = w.follow(i, &started.link).await.unwrap();
        assert_eq!((s.arrived.len(), s.needed, s.switched), (expect, 4, false));
    }
    // Payments still go to the old relay meanwhile, also from members who moved.
    w.name(1, "Bob").await.unwrap();
    assert_eq!(w.here(1).base_url(), w.old);
    let s = w.follow(3, &started.link).await.unwrap();
    assert!(s.switched, "4 of 5: {s:?}");
    let (_, state) = load_state(&w.here(3), &w.ids[3], &w.material(3))
        .await
        .unwrap();
    assert_eq!(state.moved().unwrap().arrived.len(), 4);
    assert_eq!(state.names[&w.ids[1].public().sig_pk], "Bob");
}

#[tokio::test]
async fn a_new_entry_on_the_new_relay_before_the_switch_is_refused() {
    let w = world(2, 3, 5).await;
    w.announce_all().await;
    let new = serve(Relay::new()).await;
    let started = w.move_to(0, &new).await.unwrap();
    // Member 0's material pointed at the new relay before `VaultMoved`.
    let mut m = w.material(0);
    m.relay.url = Some(started.target.relay.clone());
    m.relay.label = started.target.label;
    let mut rng = StdRng::seed_from_u64(1);
    let err = set_name(&w.client(0, &new), &w.ids[0], &m, "Alice", &mut rng)
        .await
        .unwrap_err();
    assert!(
        matches!(err, NodeError::Protocol(ref e) if e.contains("before the vault switches")),
        "{err:?}"
    );
}

#[tokio::test]
async fn an_arrived_phones_write_after_the_switch_goes_to_the_new_relay() {
    let w = world(2, 3, 6).await;
    w.announce_all().await;
    let new = serve(Relay::new()).await;
    let started = w.move_to(0, &new).await.unwrap();
    assert!(w.follow(1, &started.link).await.unwrap().switched);
    let old_len = w.old_len().await;
    // Alice hasn't refreshed: her phone still writes through the old relay's client, but it
    // checks the new relay first and writes there.
    let mut rng = StdRng::seed_from_u64(2);
    set_name(&w.old(0), &w.ids[0], &w.material[0], "Alice", &mut rng)
        .await
        .unwrap();
    assert_eq!(w.old_len().await, old_len, "nothing new on the old relay");
    let (_, state) = load_state(&w.here(1), &w.ids[1], &w.material(1))
        .await
        .unwrap();
    assert_eq!(state.names[&w.ids[0].public().sig_pk], "Alice");
    // And from now on her phone follows the new relay.
    assert_eq!(w.here(0).base_url(), new);
}

#[tokio::test]
async fn a_restore_from_a_pre_move_backup_follows() {
    let w = world(2, 3, 7).await;
    w.announce_all().await;
    // The backup: member 2's material and copy of the log, before the move.
    let backup: Vec<Vec<u8>> = LogCache::in_dir(w.cache_dir(2))
        .read(&w.material[2].descriptor.vault_id)
        .unwrap()
        .iter()
        .map(|e| e.to_bytes().unwrap())
        .collect();
    let new = serve(Relay::new()).await;
    let started = w.move_to(0, &new).await.unwrap();
    assert!(w.follow(1, &started.link).await.unwrap().switched);
    w.name(1, "Bob").await.unwrap();

    // Member 2's phone is lost; a new one restores the backup (another log copy dir).
    let restored = w.dir.path().join("restored");
    let cache = LogCache::in_dir(&restored);
    install_log_copy(&cache, &w.material[2], &backup).unwrap();
    let old = RelayClient::new(&w.old).with_log_cache(Some(cache.clone()));
    let notice = node::move_notice(&old, &w.ids[2], &w.material[2])
        .await
        .unwrap()
        .unwrap();
    assert!(notice.moved);
    let mut rng = StdRng::seed_from_u64(3);
    let status = node::follow_move(
        &old,
        &w.ids[2],
        &w.material[2],
        &MoveLink::decode(&started.link).unwrap(),
        &mut rng,
    )
    .await
    .unwrap();
    assert!(status.switched);
    let relay = followed_relay_in(&restored, &w.material[2]).unwrap();
    let mut m = w.material[2].clone();
    m.relay = relay;
    let client = RelayClient::new(&new).with_log_cache(Some(cache));
    let (_, state) = load_state(&client, &w.ids[2], &m).await.unwrap();
    assert_eq!(state.names[&w.ids[1].public().sig_pk], "Bob");
}

#[tokio::test]
async fn a_cancelled_move_is_not_followed() {
    let w = world(2, 3, 8).await;
    w.announce_all().await;
    let new = serve(Relay::new()).await;
    let started = w.move_to(0, &new).await.unwrap();
    node::cancel_move(
        &w.old(0),
        &w.ids[0],
        &w.material(0),
        &mut StdRng::seed_from_u64(4),
    )
    .await
    .unwrap();
    // The card goes away, and the link no longer works.
    assert_eq!(w.notice(1).await.unwrap(), None);
    let err = w.follow(1, &started.link).await.unwrap_err();
    assert!(matches!(err, NodeError::MoveNotFound), "{err:?}");
    // Alice can start another move.
    let again = serve(Relay::new()).await;
    w.move_to(0, &again).await.unwrap();
}

#[tokio::test]
async fn a_cancel_withheld_from_one_phone_still_leaves_one_history() {
    let w = world(2, 3, 9).await;
    w.announce_all().await;
    let new = serve(Relay::new()).await;
    let started = w.move_to(0, &new).await.unwrap();
    node::cancel_move(
        &w.old(0),
        &w.ids[0],
        &w.material(0),
        &mut StdRng::seed_from_u64(4),
    )
    .await
    .unwrap();
    // The new relay withholds the cancel from Carol (simulated: it's gone from her inbox).
    let new_client = RelayClient::new(&new).with_log_cache(None);
    let inbox = new_client
        .inbox(&w.ids[2], started.target.label, 0)
        .await
        .unwrap();
    let cursors: Vec<u64> = inbox
        .iter()
        .filter(|(_, e)| e.header.kind == Kind::MoveCancelled)
        .map(|(c, _)| *c)
        .collect();
    assert_eq!(cursors.len(), 1);
    new_client
        .ack_inbox(&w.ids[2], started.target.label, &cursors)
        .await
        .unwrap();
    // Carol follows with Alice's stale arrival: 2 of 3, the vault switches.
    assert!(w.follow(2, &started.link).await.unwrap().switched);
    // Everyone acts on that one entry. Bob finds it in the old relay's notices and stops
    // writing there until he follows; Alice, who had arrived, writes to the new relay.
    assert!(w.notice(1).await.unwrap().unwrap().moved);
    let err = w.name(1, "x").await.unwrap_err();
    assert!(matches!(err, NodeError::VaultMoved { .. }), "{err:?}");
    assert!(w.notice(0).await.unwrap().unwrap().moved);
    let old_len = w.old_len().await;
    w.name(0, "Alice").await.unwrap();
    assert_eq!(w.here(0).base_url(), new);
    assert_eq!(w.old_len().await, old_len);
    assert!(w.follow(1, &started.link).await.unwrap().switched);
    w.name(1, "Bob").await.unwrap();
    let (_, state) = load_state(&w.here(1), &w.ids[1], &w.material(1))
        .await
        .unwrap();
    assert_eq!(state.names.len(), 2);
}

#[tokio::test]
async fn two_moves_honest_phones_reach_m_only_once() {
    let w = world(2, 3, 10).await;
    w.announce_all().await;
    let (r1, r2) = (serve(Relay::new()).await, serve(Relay::new()).await);
    let one = w.move_to(0, &r1).await.unwrap();
    let two = w.move_to(1, &r2).await.unwrap();
    // Carol follows the first: it switches. Bob is in his own move: one move per phone.
    assert!(w.follow(2, &one.link).await.unwrap().switched);
    let err = w.follow(1, &one.link).await.unwrap_err();
    assert!(matches!(err, NodeError::MoveInProgress), "{err:?}");
    // Bob's move never reaches m: Carol, switched, refuses it as older than the switch.
    let err = w.follow(2, &two.link).await.unwrap_err();
    assert!(
        matches!(err, NodeError::MoveStale | NodeError::MoveInProgress),
        "{err:?}"
    );
    let s = w.progress(1).await.unwrap();
    assert!(!s.switched && s.arrived.len() == 1, "{s:?}");
    // Bob cancels his and follows the one that switched.
    node::cancel_move(
        &w.old(1),
        &w.ids[1],
        &w.material(1),
        &mut StdRng::seed_from_u64(1),
    )
    .await
    .unwrap();
    assert!(w.follow(1, &one.link).await.unwrap().switched);
}

#[tokio::test]
async fn two_switches_from_the_same_point_are_both_refused() {
    let w = world(2, 3, 11).await;
    w.announce_all().await;
    let (r1, r2) = (serve(Relay::new()).await, serve(Relay::new()).await);
    let one = w.move_to(0, &r1).await.unwrap();
    let two = w.move_to(1, &r2).await.unwrap();
    // Carol follows the first: it switches.
    assert!(w.follow(2, &one.link).await.unwrap().switched);
    // Her own app refuses the second: it is older than the switch it made.
    let status = node::follow_move(
        &w.old(2),
        &w.ids[2],
        &w.material[2],
        &MoveLink::decode(&two.link).unwrap(),
        &mut StdRng::seed_from_u64(5),
    )
    .await;
    assert!(matches!(status, Err(NodeError::MoveStale)), "{status:?}");
    // A dishonest app signs the second too: Bob then holds 2 arrivals and switches, from
    // the same point of the history as the first switch.
    let mut rng = StdRng::seed_from_u64(6);
    let carol_sig = two.target.sign(&w.ids[2]);
    let msg = MoveMessage::Arrival {
        target: two.target.clone(),
        signature: carol_sig,
        create_token: None,
    };
    let env = Envelope::sealed(
        &w.ids[2],
        w.ids[1].public(),
        two.target.label,
        1,
        Kind::Arrival,
        &msg.to_padded(),
        &mut rng,
    )
    .unwrap();
    RelayClient::new(&r2).send(&env).await.unwrap();
    let s = w.progress(1).await.unwrap();
    assert!(s.switched, "{s:?}");
    // Alice, on the first relay, reads the old relay's notices: two switches from one point.
    let err = node::move_notice(&w.old(0), &w.ids[0], &w.material[0])
        .await
        .unwrap_err();
    assert!(matches!(err, NodeError::TwoMoves), "{err:?}");
}

#[tokio::test]
async fn a_link_without_a_members_arrival_is_refused() {
    let w = world(2, 3, 12).await;
    w.announce_all().await;
    let new = serve(Relay::new()).await;
    let started = w.move_to(0, &new).await.unwrap();
    // A forged link: the right relay, another label (nobody's mailbox there).
    let mut forged = MoveLink::decode(&started.link).unwrap();
    forged.label = [9; 16];
    let err = w.follow(1, &forged.encode()).await.unwrap_err();
    assert!(matches!(err, NodeError::MoveNotFound), "{err:?}");
    // The right mailbox, another move id: no arrival for it.
    let mut forged = MoveLink::decode(&started.link).unwrap();
    forged.move_id = [9; 16];
    let err = w.follow(1, &forged.encode()).await.unwrap_err();
    assert!(matches!(err, NodeError::MoveNotFound), "{err:?}");
}

#[tokio::test]
async fn an_expired_or_replayed_move_is_refused() {
    let w = world(2, 3, 13).await;
    w.announce_all().await;
    // A move whose 14 days are over: Alice's arrival is valid but expired.
    let new = serve(Relay::new()).await;
    let (chain, _) = load_state(&w.old(0), &w.ids[0], &w.material(0))
        .await
        .unwrap();
    let target = MoveTarget {
        vault_id: w.material[0].descriptor.vault_id,
        move_id: [7; 16],
        relay: zafe_core::moves::normalize_relay_url(&new).unwrap(),
        label: [8; 16],
        base_index: chain.len() - 1,
        base_hash: chain.head(),
        expiry: 1,
    };
    let ids = node::VaultIds {
        vault: target.vault_id,
        label: target.label,
    };
    node::reseed_relay(&w.client(0, &new), &w.ids[0], ids, &w.material[0].log_key())
        .await
        .unwrap();
    let msg = MoveMessage::Arrival {
        target: target.clone(),
        signature: target.sign(&w.ids[0]),
        create_token: None,
    };
    let env = Envelope::sealed(
        &w.ids[0],
        w.ids[1].public(),
        target.label,
        1,
        Kind::Arrival,
        &msg.to_padded(),
        &mut StdRng::seed_from_u64(1),
    )
    .unwrap();
    RelayClient::new(&new).send(&env).await.unwrap();
    let err = w
        .follow(1, &MoveLink::of(&target).encode())
        .await
        .unwrap_err();
    assert!(matches!(err, NodeError::MoveExpired), "{err:?}");

    // A replayed move: the vault moved to r1, then on to r2; the first link again is
    // older than the last switch.
    let r1 = serve(Relay::new()).await;
    let one = w.move_to(0, &r1).await.unwrap();
    assert!(w.follow(1, &one.link).await.unwrap().switched);
    assert!(w.progress(0).await.unwrap().switched);
    let r2 = serve(Relay::new()).await;
    let two = node::move_vault(
        &w.here(0),
        &w.ids[0],
        &w.material(0),
        &r2,
        None,
        &mut StdRng::seed_from_u64(2),
    )
    .await
    .unwrap();
    let s = node::follow_move(
        &w.here(1),
        &w.ids[1],
        &w.material(1),
        &MoveLink::decode(&two.link).unwrap(),
        &mut StdRng::seed_from_u64(3),
    )
    .await
    .unwrap();
    assert!(s.switched);
    let err = node::follow_move(
        &w.here(1),
        &w.ids[1],
        &w.material(1),
        &MoveLink::decode(&one.link).unwrap(),
        &mut StdRng::seed_from_u64(4),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, NodeError::MoveStale), "{err:?}");
}

#[tokio::test]
async fn an_envelope_from_the_old_relay_is_dropped_by_the_new_one() {
    let w = world(2, 3, 14).await;
    w.announce_all().await;
    let new = serve(Relay::new()).await;
    let started = w.move_to(0, &new).await.unwrap();
    // A signed envelope from the old relay names the old mailbox label: the new relay has
    // no such mailbox.
    let env = Envelope::sealed(
        &w.ids[0],
        w.ids[1].public(),
        w.material[0].label(),
        u64::MAX / 2,
        Kind::Arrival,
        b"replayed",
        &mut StdRng::seed_from_u64(1),
    )
    .unwrap();
    assert!(RelayClient::new(&new).send(&env).await.is_err());
    // And a client reading the new mailbox drops envelopes naming another one.
    assert_ne!(w.material[0].label(), started.target.label);
}

#[tokio::test]
async fn a_second_move_starts_from_where_the_first_one_went() {
    let w = world(2, 3, 15).await;
    w.announce_all().await;
    let second = serve(Relay::new()).await;
    let started = w.move_to(0, &second).await.unwrap();
    assert!(w.follow(1, &started.link).await.unwrap().switched);
    assert!(w.progress(0).await.unwrap().switched);
    assert_eq!(w.here(0).base_url(), second);

    // Alice moves again before her app saved the second relay into its material (World
    // keeps the material each member was made with): until this move switches, her
    // vault stays on the second relay, never back on the first.
    let third = serve(Relay::new()).await;
    let again = node::move_vault(
        &w.here(0),
        &w.ids[0],
        &w.material(0),
        &third,
        None,
        &mut StdRng::seed_from_u64(700),
    )
    .await
    .unwrap();
    assert_eq!(
        w.here(0).base_url(),
        second,
        "until the second move switches"
    );
    w.name(0, "Alice").await.unwrap();

    let status = Box::pin(node::follow_move(
        &w.here(1),
        &w.ids[1],
        &w.material(1),
        &MoveLink::decode(&again.link).unwrap(),
        &mut StdRng::seed_from_u64(701),
    ))
    .await
    .unwrap();
    assert!(status.switched, "{status:?}");
    let (_, state) = load_state(&w.here(1), &w.ids[1], &w.material(1))
        .await
        .unwrap();
    assert_eq!(state.names[&w.ids[0].public().sig_pk], "Alice");
    assert_eq!(state.moves.len(), 2);
    let progress = node::move_progress(
        &w.here(0),
        &w.ids[0],
        &w.material(0),
        &mut StdRng::seed_from_u64(702),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(progress.switched);
    assert_eq!(w.here(0).base_url(), third);
}

#[tokio::test]
async fn a_straggler_that_never_read_the_notice_cannot_fork_the_old_relay() {
    let w = world(2, 3, 16).await;
    w.announce_all().await;
    let second = serve(Relay::new()).await;
    let started = w.move_to(0, &second).await.unwrap();
    assert!(w.follow(1, &started.link).await.unwrap().switched);
    let old_len = w.old_len().await;

    // Carol was off the whole time and writes before anything showed her the notice
    // (her app's first refresh): refused, so the old relay's history stays the one the
    // switch was made from.
    let err = w.name(2, "Carol").await.unwrap_err();
    assert!(
        matches!(&err, NodeError::VaultMoved { url } if *url == started.target.relay),
        "{err:?}"
    );
    assert_eq!(w.old_len().await, old_len);
    assert!(w.follow(2, &started.link).await.unwrap().switched);
    w.name(2, "Carol").await.unwrap();

    // The same after a second move, for a phone whose last move record is the switch
    // that brought it to the relay it is on.
    assert!(w.progress(0).await.unwrap().switched);
    let third = serve(Relay::new()).await;
    let again = node::move_vault(
        &w.here(0),
        &w.ids[0],
        &w.material(0),
        &third,
        None,
        &mut StdRng::seed_from_u64(800),
    )
    .await
    .unwrap();
    let status = Box::pin(node::follow_move(
        &w.here(1),
        &w.ids[1],
        &w.material(1),
        &MoveLink::decode(&again.link).unwrap(),
        &mut StdRng::seed_from_u64(801),
    ))
    .await
    .unwrap();
    assert!(status.switched, "{status:?}");
    let err = w.name(2, "Carol B").await.unwrap_err();
    assert!(
        matches!(&err, NodeError::VaultMoved { url } if *url == again.target.relay),
        "{err:?}"
    );
    let status = Box::pin(node::follow_move(
        &w.here(2),
        &w.ids[2],
        &w.material(2),
        &MoveLink::decode(&again.link).unwrap(),
        &mut StdRng::seed_from_u64(802),
    ))
    .await
    .unwrap();
    assert!(status.switched, "{status:?}");
    w.name(2, "Carol B").await.unwrap();
    assert_eq!(w.here(2).base_url(), third);
}
