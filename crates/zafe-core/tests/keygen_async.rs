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
        let m = membership(&relay, &self.id, invite).await?;
        let m = seal_when_full(&relay, &self.id, invite, m).await?;
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
    let m = membership(&relay, &members[1].id, &invite).await.unwrap();
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
