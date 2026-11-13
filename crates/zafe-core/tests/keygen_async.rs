//! Key generation that nobody has to wait for: each member's part is a step it repeats
//! (`node::keygen_step`) with its state on disk, so members take part at different times,
//! a phone can stop mid-ceremony and resume, and two processes of one member can't split
//! it.

use std::{path::PathBuf, time::Duration};

use rand::{rngs::StdRng, SeedableRng};
use zafe_core::{
    node::{
        create_vault, join_vault, keygen_started, keygen_step, membership, seal_when_full, Invite,
        KeygenOptions, KeygenProgress, NodeError, VaultMaterial,
    },
    nonce_store::NonceKey,
    relay_client::{RelayClient, RelayClientError},
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

/// One member of the setup: its identity and where its key-generation state lives.
struct Member {
    id: Identity,
    dir: PathBuf,
}

impl Member {
    fn key(&self, invite: &Invite) -> NonceKey {
        NonceKey::derive(&self.id.public().sig_pk, &invite.vault_id)
    }

    /// One step, from a fresh relay client (as a restarted app or a background check
    /// would): seals when full (creator), the safety check, then key generation.
    async fn step(
        &self,
        relay_url: &str,
        invite: &Invite,
        words: Option<&str>,
    ) -> Result<(Option<String>, KeygenProgress), NodeError> {
        let relay = RelayClient::new(relay_url.to_owned());
        let key = self.key(invite);
        let m = membership(&relay, &self.id, invite, &self.dir, &key).await?;
        let m = seal_when_full(&relay, &self.id, invite, m, &self.dir, &key).await?;
        let progress = keygen_step(
            &relay,
            &self.id,
            invite,
            &m,
            &self.dir,
            &self.key(invite),
            words,
            &regtest_network(),
            "regtest",
            KeygenOptions {
                creator_birthday_height: Some(2),
                ..Default::default()
            },
            &mut rand::rngs::OsRng,
        )
        .await?;
        Ok((m.safety_words, progress))
    }
}

/// A 2-of-3 setup everyone has joined; nobody locked it (the creator's first step does).
async fn setup() -> (String, Vec<Member>, Invite) {
    let url = start_relay().await;
    let relay = RelayClient::new(url.clone());
    let mut rng = StdRng::seed_from_u64(70);
    let ids: Vec<Identity> = (0..3).map(|_| Identity::generate(&mut rng)).collect();
    let invite = create_vault(&relay, &ids[0], "Grants", 2, 3, &mut rng)
        .await
        .unwrap();
    let invite = Invite::decode(&invite.encode()).unwrap();
    for id in &ids[1..] {
        join_vault(&relay, id, &invite).await.unwrap();
    }
    let members = ids
        .into_iter()
        .map(|id| Member {
            id,
            dir: tempfile::tempdir().unwrap().keep(),
        })
        .collect();
    (url, members, invite)
}

/// Steps every member until each shows the safety words.
async fn words(url: &str, members: &[Member], invite: &Invite) -> String {
    for _ in 0..50 {
        let mut all = Vec::new();
        for m in members {
            all.push(m.step(url, invite, None).await.unwrap().0);
        }
        if all.iter().all(Option::is_some) {
            return all[0].clone().unwrap();
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("no safety words");
}

fn done(p: KeygenProgress) -> Option<VaultMaterial> {
    match p {
        KeygenProgress::Done(m) => Some(*m),
        KeygenProgress::Waiting { .. } => None,
    }
}

#[tokio::test]
async fn members_take_part_at_different_times() {
    let (url, members, invite) = setup().await;
    let words = words(&url, &members, &invite).await;

    // A and B confirm and step; C hasn't looked yet. They get as far as they can, then
    // wait for C, by name.
    for m in &members[..2] {
        assert!(done(m.step(&url, &invite, Some(&words)).await.unwrap().1).is_none());
    }
    // Before confirming, C's step does nothing (no state, nothing sent).
    let (_, p) = members[2].step(&url, &invite, None).await.unwrap();
    assert!(
        matches!(
            p,
            KeygenProgress::Waiting {
                stage: "your confirmation",
                ..
            }
        ),
        "{p:?}"
    );
    assert!(!keygen_started(&members[2].dir));
    let (_, p) = members[0].step(&url, &invite, None).await.unwrap();
    match p {
        KeygenProgress::Waiting { missing, .. } => {
            assert_eq!(missing, vec![members[2].id.public().sig_pk]);
        }
        other => panic!("{other:?}"),
    }

    // C confirms later. From then on every step (no words needed: the state remembers
    // them) moves the ceremony on, in any order, until each member holds the vault.
    members[2].step(&url, &invite, Some(&words)).await.unwrap();
    let mut out: Vec<Option<VaultMaterial>> = vec![None, None, None];
    for _ in 0..40 {
        for (i, m) in members.iter().enumerate().rev() {
            if out[i].is_none() {
                out[i] = done(m.step(&url, &invite, None).await.unwrap().1);
            }
        }
        if out.iter().all(Option::is_some) {
            break;
        }
    }
    let out: Vec<VaultMaterial> = out.into_iter().map(Option::unwrap).collect();
    assert!(out.iter().all(|m| m.descriptor == out[0].descriptor));
    // Once done, a step returns the same vault again until the state is forgotten.
    let again = done(members[1].step(&url, &invite, None).await.unwrap().1).unwrap();
    assert_eq!(again.key_package, out[1].key_package);
}

#[tokio::test]
async fn two_processes_of_one_member_make_one_round_1() {
    let (url, members, invite) = setup().await;
    let words = words(&url, &members, &invite).await;
    // The app and a background check of member A start at the same moment.
    let (x, y) = tokio::join!(
        members[0].step(&url, &invite, Some(&words)),
        members[0].step(&url, &invite, Some(&words)),
    );
    x.unwrap();
    y.unwrap();
    let mut out: Vec<Option<VaultMaterial>> = vec![None, None, None];
    for _ in 0..40 {
        for (i, m) in members.iter().enumerate() {
            if out[i].is_none() {
                out[i] = done(m.step(&url, &invite, Some(&words)).await.unwrap().1);
            }
        }
        if out.iter().all(Option::is_some) {
            break;
        }
    }
    // Had they made two round 1s, the others would hold a package A's state can't
    // finish with, and the descriptors (transcript hash) wouldn't agree.
    let out: Vec<VaultMaterial> = out.into_iter().map(Option::unwrap).collect();
    assert!(out.iter().all(|m| m.descriptor == out[0].descriptor));
}

#[tokio::test]
async fn a_state_file_that_does_not_decrypt_is_an_error_not_a_fresh_start() {
    let (url, members, invite) = setup().await;
    let words = words(&url, &members, &invite).await;
    members[1].step(&url, &invite, Some(&words)).await.unwrap();
    // Same member, another key (say, a broken restore): it must not start over.
    let relay = RelayClient::new(url.clone());
    let m = membership(
        &relay,
        &members[1].id,
        &invite,
        &members[1].dir,
        &members[1].key(&invite),
    )
    .await
    .unwrap();
    let err = keygen_step(
        &relay,
        &members[1].id,
        &invite,
        &m,
        &members[1].dir,
        &NonceKey::derive(b"another key", &invite.vault_id),
        Some(&words),
        &regtest_network(),
        "regtest",
        KeygenOptions::default(),
        &mut rand::rngs::OsRng,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, NodeError::Protocol(_)), "{err:?}");
}

/// The setup screen's poll and the "They match" tap can both send for one member, and
/// over Tor the later seq can reach the relay first: the earlier send then gets 409. The
/// keygen and safety sends seal again with a fresh seq instead of failing the tap.
#[tokio::test]
async fn a_send_overtaken_by_a_later_seq_is_sealed_again() {
    use zafe_core::node::{send_fresh, SeqCounter};
    use zafe_proto::envelope::{Envelope, Kind};

    let (url, members, invite) = setup().await;
    let relay = RelayClient::new(url);
    let me = &members[1].id;
    let public = |seq| Envelope::public(me, invite.label, seq, Kind::DkgEcho, &[0; 32]).unwrap();
    let mut seq = SeqCounter::default();
    let (earlier, later) = (seq.next_seq(), seq.next_seq());
    relay.send(&public(later)).await.unwrap();
    assert!(matches!(
        relay.send(&public(earlier)).await,
        Err(RelayClientError::Status { status: 409, .. })
    ));

    let mut first = true;
    send_fresh(&relay, &mut SeqCounter::default(), |n| {
        let n = if std::mem::take(&mut first) {
            earlier
        } else {
            n
        };
        Ok(public(n))
    })
    .await
    .unwrap();
}

/// A sealed setup on `relay` for `vault_id` under `label`: what one member list looks like
/// to its members (the first creates and seals it).
async fn view(
    relay: &RelayClient,
    ids: &[&Identity],
    vault_id: [u8; 16],
    label: [u8; 16],
) -> Invite {
    let token = [label[0]; 32];
    relay
        .create_mailbox(ids[0], label, &token, ids.len() as u16)
        .await
        .unwrap();
    for id in &ids[1..] {
        relay.join(id, label, token).await.unwrap();
    }
    let members = ids.iter().map(|id| id.public().sig_pk).collect();
    relay.seal(ids[0], label, members).await.unwrap();
    Invite {
        vault_id,
        label,
        relay_url: Some(relay.base_url().to_owned()),
        join_token: token,
        creator: ids[0].public().sig_pk,
        threshold: 2,
        members: ids.len() as u16,
        name: "Grants".into(),
    }
}

/// Every member's safety words for `invite`, polled together (A from its own state dir);
/// they must agree.
async fn all_words(
    relay: &RelayClient,
    a: &Member,
    others: &[&Identity],
    invite: &Invite,
) -> String {
    let key = a.key(invite);
    let (mine, theirs) = tokio::join!(
        zafe_core::node::wait_for_safety_words(
            relay,
            &a.id,
            invite,
            &a.dir,
            &key,
            Duration::from_secs(30),
        ),
        futures::future::join_all(others.iter().map(|id| safety_words(relay, id, invite)))
    );
    let mine = mine.unwrap();
    assert!(theirs.into_iter().all(|w| w.unwrap() == mine));
    mine
}

/// Rule 1: a member pins the member list it revealed its safety value against. Here A
/// sees {A, B, C} and reveals; a relay that then shows A {A, B, M} (where it could pick M's
/// value knowing A's) gets a hard stop, and so does A's key generation.
#[tokio::test]
async fn a_member_list_switched_after_the_reveal_stops_the_setup() {
    let url = start_relay().await;
    let relay = RelayClient::new(url.clone());
    let mut rng = StdRng::seed_from_u64(74);
    let ids: Vec<Identity> = (0..4).map(|_| Identity::generate(&mut rng)).collect();
    let (b, c, m) = (&ids[1], &ids[2], &ids[3]);
    let a = Member {
        id: Identity::from_seeds(ids[0].seeds().clone()),
        dir: tempfile::tempdir().unwrap().keep(),
    };
    let vault_id = [9; 16];
    let honest = view(&relay, &[&ids[0], b, c], vault_id, [1; 16]).await;
    let first = all_words(&relay, &a, &[b, c], &honest).await;
    a.step(&url, &honest, Some(&first)).await.unwrap();

    let switched = view(&relay, &[&ids[0], b, m], vault_id, [2; 16]).await;
    assert!(matches!(
        membership(&relay, &a.id, &switched, &a.dir, &a.key(&switched)).await,
        Err(NodeError::Verification(_))
    ));
    assert!(matches!(
        a.step(&url, &switched, None).await,
        Err(NodeError::Verification(_))
    ));
}

/// Rule 1: a member pins the commitments it revealed against. After A revealed, the relay
/// drops M's commitment and value from A's inbox and serves new ones, chosen knowing A's
/// value: A refuses instead of showing words over them. Finishing the setup deletes the pin.
#[tokio::test]
async fn a_commitment_served_again_after_the_reveal_stops_the_setup() {
    use zafe_core::node::SeqCounter;
    use zafe_proto::envelope::{Envelope, Kind};
    let url = start_relay().await;
    let relay = RelayClient::new(url.clone());
    let mut rng = StdRng::seed_from_u64(77);
    let ids: Vec<Identity> = (0..3).map(|_| Identity::generate(&mut rng)).collect();
    let (b, m) = (&ids[1], &ids[2]);
    let a = Member {
        id: Identity::from_seeds(ids[0].seeds().clone()),
        dir: tempfile::tempdir().unwrap().keep(),
    };
    let invite = view(&relay, &[&ids[0], b, m], [9; 16], [3; 16]).await;
    let words = all_words(&relay, &a, &[b, m], &invite).await;

    // Gone from A's inbox: M's commitment and value (the relay can drop anything).
    let theirs: Vec<u64> = relay
        .inbox(&a.id, invite.label, 0)
        .await
        .unwrap()
        .into_iter()
        .filter(|(_, e)| e.header.from == m.public().sig_pk)
        .map(|(cursor, _)| cursor)
        .collect();
    relay.ack_inbox(&a.id, invite.label, &theirs).await.unwrap();
    let value = [5; 32];
    let commitment = zafe_core::sas::commitment(&invite.vault_id, &m.public().sig_pk, &value);
    let mut seq = SeqCounter::default();
    for (kind, payload) in [
        (Kind::SafetyCommit, commitment),
        (Kind::SafetyReveal, value),
    ] {
        let envelope = Envelope::public(m, invite.label, seq.next_seq(), kind, &payload).unwrap();
        relay.send(&envelope).await.unwrap();
    }
    let again = membership(&relay, &a.id, &invite, &a.dir, &a.key(&invite)).await;
    assert!(
        matches!(again, Err(NodeError::Verification(_))),
        "words {:?} instead of {words}",
        again.map(|m| m.safety_words)
    );

    zafe_core::node::forget_keygen(&a.dir).unwrap();
    assert!(!a.dir.join(zafe_core::state_dir::SAFETY_PIN).exists());
}

/// Rule 1: a revealed safety value never comes back. A member who leaves a setup (its state
/// dir is deleted) and joins the same invite again commits to a new value and never sends
/// the old one again; the others, whose pins hold the old commitment, stop.
#[tokio::test]
async fn rejoining_a_setup_never_reveals_the_old_value_again() {
    use zafe_proto::envelope::Kind;
    let url = start_relay().await;
    let relay = RelayClient::new(url.clone());
    let mut rng = StdRng::seed_from_u64(79);
    let ids: Vec<Identity> = (0..3).map(|_| Identity::generate(&mut rng)).collect();
    let (b, c) = (&ids[1], &ids[2]);
    let a = Member {
        id: Identity::from_seeds(ids[0].seeds().clone()),
        dir: tempfile::tempdir().unwrap().keep(),
    };
    let b_side = Member {
        id: Identity::from_seeds(b.seeds().clone()),
        dir: tempfile::tempdir().unwrap().keep(),
    };
    let invite = view(&relay, &[&ids[0], b, c], [9; 16], [4; 16]).await;
    let key = b_side.key(&invite);
    let (b_words, c_words, words) = tokio::join!(
        zafe_core::node::wait_for_safety_words(
            &relay,
            &b_side.id,
            &invite,
            &b_side.dir,
            &key,
            Duration::from_secs(30),
        ),
        safety_words(&relay, c, &invite),
        all_words(&relay, &a, &[], &invite),
    );
    assert_eq!((b_words.unwrap(), c_words.unwrap()), (words.clone(), words));

    // "Leave": the setup's state dir is deleted; the same invite is opened again.
    std::fs::remove_dir_all(&a.dir).unwrap();
    let _ = membership(&relay, &a.id, &invite, &a.dir, &a.key(&invite)).await;

    let (label, a_pub) = (invite.label, *a.id.public());
    let from_a = |kind| {
        let relay = &relay;
        async move {
            relay
                .inbox(c, label, 0)
                .await
                .unwrap()
                .into_iter()
                .filter(|(_, e)| e.header.kind == kind && e.header.from == a_pub.sig_pk)
                .map(|(_, e)| e.open(c, &a_pub).unwrap())
                .collect::<Vec<_>>()
        }
    };
    let commits = from_a(Kind::SafetyCommit).await;
    assert_eq!(commits.len(), 2, "a new commitment after rejoining");
    assert_ne!(commits[0], commits[1]);
    let reveals = from_a(Kind::SafetyReveal).await;
    assert!(
        reveals.iter().skip(1).all(|r| *r != reveals[0]),
        "the old value is never sent again"
    );
    // B pinned A's first commitment: it stops.
    assert!(matches!(
        membership(&relay, &b_side.id, &invite, &b_side.dir, &key).await,
        Err(NodeError::Verification(_))
    ));
}

/// One member's safety words for `invite`, polled with its own state dir for the pin.
async fn safety_words(
    relay: &zafe_core::relay_client::RelayClient,
    id: &zafe_proto::Identity,
    invite: &zafe_core::node::Invite,
) -> Result<String, zafe_core::node::NodeError> {
    let dir = tempfile::tempdir().unwrap().keep();
    let key = zafe_core::nonce_store::NonceKey::derive(&id.public().sig_pk, &invite.vault_id);
    zafe_core::node::wait_for_safety_words(
        relay,
        id,
        invite,
        &dir,
        &key,
        std::time::Duration::from_secs(30),
    )
    .await
}
