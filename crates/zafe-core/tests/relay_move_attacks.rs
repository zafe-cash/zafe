use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use rand::{rngs::StdRng, SeedableRng};
use zafe_core::{
    log_cache::LogCache,
    moves::{MoveLink, MoveMessage},
    node::{
        self, create_vault, followed_relay_in, join_vault, load_state, run_keygen, seal, set_name,
        Invite, KeygenOptions, MoveStatus, NodeError, VaultMaterial,
    },
    relay_client::RelayClient,
};
use zafe_proto::{Envelope, Identity, Kind, MailboxId};
use zcash_protocol::memo::MemoBytes;

pub mod common;
use common::{
    build_pczt, nu6_3_network, receive_ironwood_note,
    relay_vault::{member_dir, safety_words, setup_key},
    witness, Out, TARGET_HEIGHT,
};
use zafe_relay::Relay;

async fn serve(relay: Relay) -> String {
    serve_counting(relay).await.0
}

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
    old: String,
    ids: Vec<Identity>,
    material: Vec<VaultMaterial>,
}

impl World {
    fn cache_dir(&self, i: usize) -> PathBuf {
        self.dir.path().join(format!("copy-{i}"))
    }

    fn client(&self, i: usize, url: &str) -> RelayClient {
        RelayClient::new(url).with_log_cache(Some(LogCache::in_dir(self.cache_dir(i))))
    }

    fn old(&self, i: usize) -> RelayClient {
        self.client(i, &self.old)
    }

    fn material(&self, i: usize) -> VaultMaterial {
        let mut m = self.material[i].clone();
        if let Some(relay) = followed_relay_in(&self.cache_dir(i), &m) {
            m.relay = relay;
        }
        m
    }

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

    async fn old_len(&self) -> u64 {
        let bare = RelayClient::new(&self.old).with_log_cache(None);
        load_state(&bare, &self.ids[1], &self.material[1])
            .await
            .unwrap()
            .0
            .len()
    }

    async fn load_old(&self, i: usize) -> Result<(), NodeError> {
        load_state(&self.old(i), &self.ids[i], &self.material(i))
            .await
            .map(|_| ())
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

async fn world(t: u16, n: u16, seed: u64) -> World {
    world_on(t, n, seed, serve(Relay::new()).await).await
}

async fn world_on(t: u16, n: u16, seed: u64, old: String) -> World {
    let dir = tempfile::tempdir().unwrap();
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
    let words = futures::future::join_all(ids.iter().map(|id| safety_words(&relay, id, &invite)))
        .await
        .into_iter()
        .map(Result::unwrap)
        .next()
        .unwrap();
    let net = nu6_3_network();
    let material = futures::future::join_all(ids.iter().enumerate().map(|(i, id)| {
        let (relay, invite, words, net) = (&relay, &invite, &words, &net);
        async move {
            run_keygen(
                relay,
                id,
                invite,
                words,
                net,
                "regtest",
                opts(i == 0),
                member_dir(relay, id),
                setup_key(id),
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
async fn a_withdrawn_arrival_never_moves_its_phone() {
    let w = world(2, 4, 21).await;
    w.announce_all().await;
    let new = serve(Relay::new()).await;
    let started = w.move_to(0, &new).await.unwrap();
    assert!(!w.follow(1, &started.link).await.unwrap().switched);
    node::cancel_move(
        &w.old(1),
        &w.ids[1],
        &w.material(1),
        &mut StdRng::seed_from_u64(4),
    )
    .await
    .unwrap();

    withhold(
        &new,
        &w.ids[2],
        started.target.label,
        Kind::MoveCancelled,
        None,
    )
    .await;
    let carol = w.follow(2, &started.link).await;
    assert!(
        !matches!(carol, Ok(ref s) if s.switched),
        "Carol must not count Bob's withdrawn arrival (his cancel is on the old relay): {carol:?}"
    );

    let _ = w.notice(1).await;
    let wrote = w.name(1, "Bob").await;
    assert_eq!(
        w.here(1).base_url(),
        w.old,
        "Bob withdrew: his phone must not follow the switch on its own ({wrote:?})"
    );
    assert_ne!(
        followed_relay_in(&w.cache_dir(1), &w.material[1]).and_then(|r| r.url),
        Some(zafe_core::moves::normalize_relay_url(&new).unwrap())
    );
}

#[tokio::test]
async fn a_withdrawn_phone_stops_and_follows_only_on_a_tap() {
    let w = world(2, 4, 22).await;
    w.announce_all().await;
    let new = serve(Relay::new()).await;
    let started = w.move_to(0, &new).await.unwrap();
    assert!(!w.follow(1, &started.link).await.unwrap().switched);
    node::cancel_move(
        &w.old(1),
        &w.ids[1],
        &w.material(1),
        &mut StdRng::seed_from_u64(4),
    )
    .await
    .unwrap();
    let bob = Some(w.ids[1].public().sig_pk);
    withhold(
        &new,
        &w.ids[2],
        started.target.label,
        Kind::MoveCancelled,
        bob,
    )
    .await;
    withhold(
        &w.old,
        &w.ids[2],
        w.material[2].label(),
        Kind::MoveCancelled,
        bob,
    )
    .await;
    assert!(w.follow(2, &started.link).await.unwrap().switched);

    let old_len = w.old_len().await;
    assert!(w.notice(1).await.unwrap().unwrap().moved);
    let wrote = w.name(1, "Bob").await;
    assert!(
        matches!(wrote, Err(NodeError::VaultMoved { .. })),
        "Bob withdrew: his write stops until he taps ({wrote:?})"
    );
    assert_eq!(w.here(1).base_url(), w.old, "not followed on its own");
    assert_eq!(
        w.old_len().await,
        old_len,
        "nothing written to the old relay"
    );
    assert!(w.follow(1, &started.link).await.unwrap().switched);
    w.name(1, "Bob").await.unwrap();
    assert_eq!(w.here(1).base_url(), new);
}

#[tokio::test]
async fn a_movers_cancel_on_the_old_relay_stops_the_switch() {
    let w = world(2, 3, 23).await;
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
    withhold(
        &new,
        &w.ids[2],
        started.target.label,
        Kind::MoveCancelled,
        None,
    )
    .await;
    let carol = w.follow(2, &started.link).await;
    assert!(
        !matches!(carol, Ok(ref s) if s.switched),
        "the mover's cancel is on the old relay: {carol:?}"
    );
}

#[tokio::test]
async fn a_mover_that_cancelled_follows_only_on_a_tap() {
    let w = world(2, 3, 24).await;
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
    withhold(
        &new,
        &w.ids[2],
        started.target.label,
        Kind::MoveCancelled,
        None,
    )
    .await;
    withhold(
        &w.old,
        &w.ids[2],
        w.material[2].label(),
        Kind::MoveCancelled,
        None,
    )
    .await;
    assert!(w.follow(2, &started.link).await.unwrap().switched);

    let old_len = w.old_len().await;
    assert!(w.notice(0).await.unwrap().unwrap().moved);
    let wrote = w.name(0, "Alice").await;
    assert!(
        matches!(wrote, Err(NodeError::VaultMoved { .. })),
        "Alice cancelled: her write stops until she taps ({wrote:?})"
    );
    assert_eq!(w.here(0).base_url(), w.old);
    assert_eq!(w.old_len().await, old_len);
    assert!(w.follow(0, &started.link).await.unwrap().switched);
    w.name(0, "Alice").await.unwrap();
    assert_eq!(w.here(0).base_url(), new);
}

#[tokio::test]
async fn the_movers_cancel_ends_the_move_whatever_the_arrival_order() {
    let w = world(2, 4, 25).await;
    w.announce_all().await;
    let new = serve(Relay::new()).await;
    let started = w.move_to(0, &new).await.unwrap();
    assert!(!w.follow(1, &started.link).await.unwrap().switched);
    let label = started.target.label;
    let alice = Some(w.ids[0].public().sig_pk);
    let arrival = MoveMessage::Arrival {
        target: started.target.clone(),
        signature: started.target.sign(&w.ids[0]),
        create_token: None,
    };
    for i in [2, 3] {
        assert_eq!(
            withhold(&new, &w.ids[i], label, Kind::Arrival, alice).await,
            1
        );
        send_move(&new, &w.ids[0], &w.ids[i], label, Kind::Arrival, &arrival).await;
    }
    node::cancel_move(
        &w.old(0),
        &w.ids[0],
        &w.material(0),
        &mut StdRng::seed_from_u64(4),
    )
    .await
    .unwrap();

    let dave = w.follow(3, &started.link).await;
    let carol = w.follow(2, &started.link).await;
    assert!(
        ![&dave, &carol]
            .iter()
            .any(|s| matches!(s, Ok(s) if s.switched)),
        "the mover called the move off: Dave {dave:?}, Carol {carol:?}"
    );
}

#[tokio::test]
async fn a_member_cant_claim_another_members_move() {
    let w = world(2, 3, 26).await;
    w.announce_all().await;
    let new = serve(Relay::new()).await;
    let started = w.move_to(0, &new).await.unwrap();
    let label = w.material[0].label();
    let alice = Some(w.ids[0].public().sig_pk);
    assert_eq!(
        withhold(&w.old, &w.ids[1], label, Kind::MoveNotice, alice).await,
        1
    );
    let forged = MoveMessage::Notice {
        target: started.target.clone(),
        mover: w.ids[2].public().sig_pk,
        signature: started.target.sign(&w.ids[2]),
        create_token: None,
        moved: None,
    };
    send_move(
        &w.old,
        &w.ids[2],
        &w.ids[1],
        label,
        Kind::MoveNotice,
        &forged,
    )
    .await;
    let cancel = MoveMessage::Cancelled {
        move_id: started.target.move_id,
    };
    send_move(
        &w.old,
        &w.ids[2],
        &w.ids[1],
        label,
        Kind::MoveCancelled,
        &cancel,
    )
    .await;
    let genuine = MoveMessage::Notice {
        target: started.target.clone(),
        mover: w.ids[0].public().sig_pk,
        signature: started.target.sign(&w.ids[0]),
        create_token: None,
        moved: None,
    };
    send_move(
        &w.old,
        &w.ids[0],
        &w.ids[1],
        label,
        Kind::MoveNotice,
        &genuine,
    )
    .await;

    let notice = w.notice(1).await.unwrap();
    assert_eq!(
        notice.as_ref().map(|n| n.mover),
        Some(w.ids[0].public().sig_pk),
        "Bob still sees Alice's move, with Alice as the mover: {notice:?}"
    );
}

#[tokio::test]
async fn a_cancelled_phone_cant_arrive_again() {
    let w = world(2, 4, 27).await;
    w.announce_all().await;
    let new = serve(Relay::new()).await;
    let started = w.move_to(0, &new).await.unwrap();
    assert!(!w.follow(1, &started.link).await.unwrap().switched);
    node::cancel_move(
        &w.old(1),
        &w.ids[1],
        &w.material(1),
        &mut StdRng::seed_from_u64(4),
    )
    .await
    .unwrap();
    let again = w.follow(1, &started.link).await;
    assert!(
        matches!(&again, Err(NodeError::MoveCancelled)),
        "Bob left this move; he needs a new one: {again:?}"
    );
}

#[tokio::test]
async fn an_arrival_served_after_its_cancel_doesnt_count() {
    let w = world(2, 4, 28).await;
    w.announce_all().await;
    let new = serve(Relay::new()).await;
    let started = w.move_to(0, &new).await.unwrap();
    assert!(!w.follow(1, &started.link).await.unwrap().switched);
    node::cancel_move(
        &w.old(1),
        &w.ids[1],
        &w.material(1),
        &mut StdRng::seed_from_u64(4),
    )
    .await
    .unwrap();
    let label = started.target.label;
    let bob = Some(w.ids[1].public().sig_pk);
    assert_eq!(
        withhold(&new, &w.ids[2], label, Kind::Arrival, bob).await,
        1
    );
    let arrival = MoveMessage::Arrival {
        target: started.target.clone(),
        signature: started.target.sign(&w.ids[1]),
        create_token: None,
    };
    send_move(&new, &w.ids[1], &w.ids[2], label, Kind::Arrival, &arrival).await;

    let carol = w.follow(2, &started.link).await;
    assert!(
        !matches!(carol, Ok(ref s) if s.switched),
        "Bob's cancelled arrival must not count: {carol:?}"
    );
}

#[tokio::test]
async fn an_arrived_phone_doesnt_write_while_its_new_relay_is_down() {
    let w = world(2, 5, 29).await;
    w.announce_all().await;
    let (new, down) = serve_breakable(Relay::new(), "/").await;
    let started = w.move_to(0, &new).await.unwrap();
    assert!(!w.follow(1, &started.link).await.unwrap().switched);
    down.store(true, Ordering::SeqCst);

    let old_len = w.old_len().await;
    let wrote = w.name(1, "Bob").await;
    assert!(wrote.is_err(), "unknown means stop: {wrote:?}");
    assert_eq!(w.old_len().await, old_len, "nothing on the old relay");

    node::cancel_move(
        &w.old(1),
        &w.ids[1],
        &w.material(1),
        &mut StdRng::seed_from_u64(4),
    )
    .await
    .unwrap();
    w.name(1, "Bob").await.unwrap();
    assert_eq!(
        w.old_len().await,
        old_len + 1,
        "after cancelling he writes here"
    );
}

#[tokio::test]
async fn a_write_waits_until_the_move_notices_can_be_read() {
    let (old, inbox_down) = serve_breakable(Relay::new(), "/v1/inbox").await;
    let w = world_on(2, 3, 30, old).await;
    w.announce_all().await;
    inbox_down.store(true, Ordering::SeqCst);

    let old_len = w.old_len().await;
    let wrote = w.name(1, "Bob").await;
    assert!(wrote.is_err(), "unknown means stop: {wrote:?}");
    assert_eq!(w.old_len().await, old_len);
}

#[tokio::test]
async fn a_phone_that_wrote_after_the_switch_can_still_follow() {
    let w = world(2, 3, 33).await;
    w.announce_all().await;
    let new = serve(Relay::new()).await;
    let started = w.move_to(0, &new).await.unwrap();
    assert!(w.follow(1, &started.link).await.unwrap().switched);
    withhold(
        &w.old,
        &w.ids[2],
        w.material[2].label(),
        Kind::MoveNotice,
        None,
    )
    .await;
    w.name(2, "C-old")
        .await
        .expect("Carol never saw the switch: she writes to the old relay");

    let status = w.follow(2, &started.link).await;
    assert!(
        matches!(status, Ok(ref s) if s.switched),
        "Carol follows; her post-switch entry is void: {status:?}"
    );
    let (chain, state) = load_state(&w.here(2), &w.ids[2], &w.material(2))
        .await
        .unwrap();
    assert_eq!(w.here(2).base_url(), new);
    assert!(!state.names.values().any(|n| n == "C-old"));
    let copy = LogCache::in_dir(w.cache_dir(2))
        .read(&w.material[2].descriptor.vault_id)
        .unwrap();
    assert_eq!(
        copy.len() as u64,
        chain.len(),
        "the copy is the new relay's history"
    );
}

#[tokio::test]
async fn a_dropped_part_with_a_possibly_signed_payment_is_never_dropped() {
    let w = world(2, 3, 34).await;
    w.announce_all().await;
    let new = serve(Relay::new()).await;
    let started = w.move_to(0, &new).await.unwrap();
    assert!(w.follow(1, &started.link).await.unwrap().switched);
    withhold(
        &w.old,
        &w.ids[2],
        w.material[2].label(),
        Kind::MoveNotice,
        None,
    )
    .await;

    let id = propose(&w.old(2), &w.ids[2], &w.material[2], 7).await;
    approve_on(&w.old(2), &w.ids[2], &w.material[2], id, 2).await;
    let bob_old = RelayClient::new(&w.old)
        .with_log_cache(Some(LogCache::in_dir(w.dir.path().join("bob-old"))));
    approve_on(&bob_old, &w.ids[1], &w.material[1], id, 1).await;
    let (_, state) = load_state(&w.old(2), &w.ids[2], &w.material(2))
        .await
        .unwrap();
    assert!(state.proposals[&id].possibly_signed, "precondition");
    let before = LogCache::in_dir(w.cache_dir(2))
        .read(&w.material[2].descriptor.vault_id)
        .unwrap()
        .len();

    let status = w.follow(2, &started.link).await;
    assert!(
        matches!(status, Err(NodeError::RelayForked { .. })),
        "{status:?}"
    );
    let after = LogCache::in_dir(w.cache_dir(2))
        .read(&w.material[2].descriptor.vault_id)
        .unwrap()
        .len();
    assert_eq!(after, before, "Carol's copy is untouched");
}

#[tokio::test]
async fn a_members_garbage_move_messages_dont_stop_writes() {
    let w = world(2, 3, 35).await;
    w.announce_all().await;
    let label = w.material[0].label();
    for kind in [Kind::MoveNotice, Kind::MoveCancelled, Kind::Arrival] {
        let env = Envelope::sealed(
            &w.ids[2],
            w.ids[1].public(),
            label,
            micros(),
            kind,
            &[0xff; 1024],
            &mut StdRng::seed_from_u64(11),
        )
        .unwrap();
        w.old(2).send(&w.ids[2], &env).await.unwrap();
    }
    w.name(1, "Bob").await.unwrap();
}

#[tokio::test]
async fn an_early_write_to_the_new_relay_is_a_move_conflict() {
    let w = world(2, 3, 31).await;
    w.announce_all().await;
    let new = serve(Relay::new()).await;
    let started = w.move_to(0, &new).await.unwrap();

    let mut m = w.material(2);
    m.relay.url = Some(started.target.relay.clone());
    m.relay.label = started.target.label;
    let to_new = RelayClient::new(&new)
        .with_log_cache(Some(LogCache::in_dir(w.dir.path().join("mallory-new"))));
    set_name(
        &to_new,
        &w.ids[2],
        &m,
        "M-new",
        &mut StdRng::seed_from_u64(7),
    )
    .await
    .expect("nothing stops a member without a move from writing to the new relay");
    w.name(2, "M-old").await.unwrap();

    let status = w.follow(1, &started.link).await;
    assert!(
        matches!(&status, Err(NodeError::MoveConflict)),
        "Bob is told to start the move again: {status:?}"
    );
    for i in [0, 1] {
        let loaded = load_state(&w.old(i), &w.ids[i], &w.material(i)).await;
        assert!(
            loaded.is_ok(),
            "member {i} keeps working on the old relay: {:?}",
            loaded.err()
        );
    }
    w.name(1, "Bob")
        .await
        .expect("Bob can still write on the old relay");
    assert_eq!(w.here(1).base_url(), w.old);
}

#[tokio::test]
async fn a_tap_on_move_doesnt_take_a_new_relay_entry_into_the_copy() {
    let w = world(2, 5, 32).await;
    w.announce_all().await;
    let new = serve(Relay::new()).await;
    let started = w.move_to(0, &new).await.unwrap();

    let mut m = w.material(4);
    m.relay.url = Some(started.target.relay.clone());
    m.relay.label = started.target.label;
    let to_new = RelayClient::new(&new)
        .with_log_cache(Some(LogCache::in_dir(w.dir.path().join("mallory-new"))));
    set_name(
        &to_new,
        &w.ids[4],
        &m,
        "M-new",
        &mut StdRng::seed_from_u64(8),
    )
    .await
    .unwrap();

    let status = w.follow(1, &started.link).await;
    assert!(
        !matches!(status, Ok(ref s) if s.switched),
        "two of four can't switch: {status:?}"
    );
    for i in [0, 1] {
        let loaded = load_state(&w.old(i), &w.ids[i], &w.material(i)).await;
        assert!(
            loaded.is_ok(),
            "member {i} still works on the old relay before the switch: {:?}",
            loaded.err()
        );
    }
    w.name(1, "Bob")
        .await
        .expect("Bob can still write on the old relay");
}

#[tokio::test]
async fn an_arrived_phone_writes_here_again_once_its_move_expires() {
    let w = world(2, 5, 36).await;
    w.announce_all().await;
    let (new, down) = serve_breakable(Relay::new(), "/").await;
    let started = w.move_to(0, &new).await.unwrap();
    let mut target = started.target.clone();
    target.expiry = micros() / 1_000_000 + 3;
    let label = target.label;
    let alice = Some(w.ids[0].public().sig_pk);
    assert_eq!(
        withhold(&new, &w.ids[1], label, Kind::Arrival, alice).await,
        1
    );
    let arrival = MoveMessage::Arrival {
        target: target.clone(),
        signature: target.sign(&w.ids[0]),
        create_token: None,
    };
    send_move(&new, &w.ids[0], &w.ids[1], label, Kind::Arrival, &arrival).await;
    let status = w.follow(1, &started.link).await.unwrap();
    assert_eq!(status.target.expiry, target.expiry, "precondition");
    down.store(true, Ordering::SeqCst);

    tokio::time::sleep(Duration::from_secs(5)).await;
    let old_len = w.old_len().await;
    w.name(1, "Bob")
        .await
        .expect("the move expired: Bob writes to the old relay");
    assert_eq!(w.old_len().await, old_len + 1);
}

fn micros() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64
}

async fn send_move(
    url: &str,
    from: &Identity,
    to: &Identity,
    label: MailboxId,
    kind: Kind,
    msg: &MoveMessage,
) {
    for _ in 0..20 {
        let env = Envelope::sealed(
            from,
            to.public(),
            label,
            micros(),
            kind,
            &msg.to_padded(),
            &mut StdRng::seed_from_u64(9),
        )
        .unwrap();
        match RelayClient::new(url)
            .with_log_cache(None)
            .send(from, &env)
            .await
        {
            Ok(_) => return,
            Err(zafe_core::relay_client::RelayClientError::Status { status: 409, .. }) => {
                tokio::time::sleep(std::time::Duration::from_millis(200)).await
            }
            Err(e) => panic!("{e:?}"),
        }
    }
    panic!("the relay kept refusing the seq");
}

async fn withhold(
    url: &str,
    who: &Identity,
    label: MailboxId,
    kind: Kind,
    sender: Option<[u8; 32]>,
) -> usize {
    let client = RelayClient::new(url).with_log_cache(None);
    let inbox = client.inbox(who, label, 0).await.unwrap();
    let cursors: Vec<u64> = inbox
        .iter()
        .filter(|(_, e)| e.header.kind == kind && sender.is_none_or(|s| e.header.from == s))
        .map(|(c, _)| *c)
        .collect();
    if !cursors.is_empty() {
        client.ack_inbox(who, label, &cursors).await.unwrap();
    }
    cursors.len()
}

async fn serve_breakable(relay: Relay, path: &'static str) -> (String, Arc<AtomicBool>) {
    use axum::response::IntoResponse;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let down = Arc::new(AtomicBool::new(false));
    let flag = down.clone();
    let app = relay.router().layer(axum::middleware::from_fn(
        move |req: axum::extract::Request, next: axum::middleware::Next| {
            let flag = flag.clone();
            async move {
                if flag.load(Ordering::SeqCst) && req.uri().path().starts_with(path) {
                    return axum::http::StatusCode::SERVICE_UNAVAILABLE.into_response();
                }
                next.run(req).await
            }
        },
    ));
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{addr}"), down)
}

const TIP: u32 = TARGET_HEIGHT - 1;

async fn propose(relay: &RelayClient, author: &Identity, m: &VaultMaterial, id: u8) -> [u8; 16] {
    use orchard::keys::Scope;
    let fvk = m.vault_keys().unwrap().fvk().clone();
    let note = receive_ironwood_note(&fvk, fvk.address_at(1u32, Scope::External), 1_000_000);
    let (anchor, path) = witness(&note);
    let pczt = build_pczt(
        &fvk,
        note,
        path,
        anchor,
        vec![Out {
            ovk: Some(fvk.to_ovk(Scope::Internal)),
            recipient: fvk.address_at(0u32, Scope::Internal),
            value: 990_000,
            memo: MemoBytes::empty(),
        }],
    );
    let event = zafe_core::vault::VaultEvent::Proposal {
        id: [id; 16],
        payments: vec![],
        pczt_hash: zafe_core::session::pczt_hash(&pczt).unwrap(),
        pczt: pczt.serialize().unwrap(),
        tip_height: TIP,
        created_at: 0,
        signing_spends: 1,
        auto_send: false,
    };
    let (mut chain, mut state) = load_state(relay, author, m).await.unwrap();
    node::append_event(
        relay,
        author,
        m,
        &mut chain,
        &mut state,
        &event,
        &mut StdRng::seed_from_u64(u64::from(id)),
    )
    .await
    .unwrap();
    [id; 16]
}

async fn approve_on(
    relay: &RelayClient,
    me: &Identity,
    m: &VaultMaterial,
    proposal: [u8; 16],
    seed: u64,
) {
    node::approve(
        relay,
        me,
        m,
        &nu6_3_network(),
        &node::ChainView {
            tip: TIP,
            spends: Vec::new(),
            dropped: Vec::new(),
        },
        proposal,
        &mut zafe_core::session::MemoryNonceStore::default(),
        &mut zafe_core::session::MemoryPoolStore::default(),
        &mut StdRng::seed_from_u64(seed),
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn a_move_conflict_doesnt_stop_an_arrived_phones_writes() {
    let w = world(2, 3, 37).await;
    w.announce_all().await;
    let new = serve(Relay::new()).await;
    let started = w.move_to(0, &new).await.unwrap();
    let mut m = w.material(2);
    m.relay.url = Some(started.target.relay.clone());
    m.relay.label = started.target.label;
    let to_new = RelayClient::new(&new)
        .with_log_cache(Some(LogCache::in_dir(w.dir.path().join("mallory-new"))));
    set_name(
        &to_new,
        &w.ids[2],
        &m,
        "M-new",
        &mut StdRng::seed_from_u64(7),
    )
    .await
    .unwrap();
    w.name(2, "M-old").await.unwrap();
    let _ = w.load_old(0).await;

    let old_len = w.old_len().await;
    let wrote = w.name(0, "Alice").await;
    assert!(
        wrote.is_ok(),
        "the mover keeps working where the vault is: {wrote:?}"
    );
    assert_eq!(w.old_len().await, old_len + 1);
}

#[tokio::test]
async fn a_dropped_part_that_isnt_intent_only_is_never_dropped() {
    let w = world(2, 3, 38).await;
    w.announce_all().await;
    let new = serve(Relay::new()).await;
    let started = w.move_to(0, &new).await.unwrap();
    assert!(w.follow(1, &started.link).await.unwrap().switched);
    withhold(
        &w.old,
        &w.ids[2],
        w.material[2].label(),
        Kind::MoveNotice,
        None,
    )
    .await;
    let old = w.old(2);
    let (mut chain, mut state) = load_state(&old, &w.ids[2], &w.material(2)).await.unwrap();
    node::append_event(
        &old,
        &w.ids[2],
        &w.material(2),
        &mut chain,
        &mut state,
        &zafe_core::vault::VaultEvent::PoolReset,
        &mut StdRng::seed_from_u64(9),
    )
    .await
    .expect("Carol never saw the switch: she writes to the old relay");
    let before = LogCache::in_dir(w.cache_dir(2))
        .read(&w.material[2].descriptor.vault_id)
        .unwrap();

    let status = w.follow(2, &started.link).await;
    assert!(
        matches!(status, Err(NodeError::RelayForked { .. })),
        "a pool reset is evidence, not intent: {status:?}"
    );
    let after = LogCache::in_dir(w.cache_dir(2))
        .read(&w.material[2].descriptor.vault_id)
        .unwrap();
    assert_eq!(after.len(), before.len(), "the copy is untouched");
}

#[tokio::test]
async fn a_cancel_counts_before_an_arrival_served_after_it() {
    let w = world(2, 4, 40).await;
    w.announce_all().await;
    let new = serve(Relay::new()).await;
    let started = w.move_to(0, &new).await.unwrap();
    assert!(!w.follow(1, &started.link).await.unwrap().switched);
    node::cancel_move(
        &w.old(1),
        &w.ids[1],
        &w.material(1),
        &mut StdRng::seed_from_u64(4),
    )
    .await
    .unwrap();
    let label = started.target.label;
    let bob = Some(w.ids[1].public().sig_pk);
    withhold(
        &w.old,
        &w.ids[2],
        w.material[2].label(),
        Kind::MoveCancelled,
        bob,
    )
    .await;
    assert_eq!(
        withhold(&new, &w.ids[2], label, Kind::Arrival, bob).await,
        1
    );
    let arrival = MoveMessage::Arrival {
        target: started.target.clone(),
        signature: started.target.sign(&w.ids[1]),
        create_token: None,
    };
    send_move(&new, &w.ids[1], &w.ids[2], label, Kind::Arrival, &arrival).await;

    let carol = w.follow(2, &started.link).await;
    assert!(
        !matches!(carol, Ok(ref s) if s.switched),
        "Bob's cancel comes first in Carol's inbox: {carol:?}"
    );
}

#[tokio::test]
async fn a_switch_checks_the_old_relay_one_last_time() {
    let w = world(2, 4, 39).await;
    w.announce_all().await;
    let new = serve(Relay::new()).await;
    let started = w.move_to(0, &new).await.unwrap();
    assert!(!w.follow(1, &started.link).await.unwrap().switched);
    w.name(2, "C-old").await.unwrap();
    let mut m = w.material(3);
    m.relay.url = Some(started.target.relay.clone());
    m.relay.label = started.target.label;
    let to_new = RelayClient::new(&new)
        .with_log_cache(Some(LogCache::in_dir(w.dir.path().join("mallory-new"))));
    set_name(
        &to_new,
        &w.ids[3],
        &m,
        "M-new",
        &mut StdRng::seed_from_u64(7),
    )
    .await
    .unwrap();
    let arrival = MoveMessage::Arrival {
        target: started.target.clone(),
        signature: started.target.sign(&w.ids[3]),
        create_token: None,
    };
    send_move(
        &new,
        &w.ids[3],
        &w.ids[1],
        started.target.label,
        Kind::Arrival,
        &arrival,
    )
    .await;

    let progress = node::move_progress(
        &w.old(1),
        &w.ids[1],
        &w.material(1),
        &mut StdRng::seed_from_u64(601),
    )
    .await;
    assert!(
        !matches!(progress, Ok(Some(ref s)) if s.switched),
        "the old relay holds another entry where the new one has Mallory's: {progress:?}"
    );
}
