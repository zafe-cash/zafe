//! Setup helpers shared by the bridge tests.

use std::time::Duration;

use rust_lib_zafe::api::vault;
use zafe_core::{
    node::{self, Invite, KeygenOptions, KeygenProgress},
    nonce_store::NonceKey,
    wallet::regtest_network,
};
use zafe_proto::{Identity, IdentitySeeds};

/// A member taking part in a setup: through the bridge (as the app does: its one-tap pool
/// is published at the end) or through `zafe_core` directly (no pool).
pub struct Setup {
    pub seeds: Vec<u8>,
    pub state_dir: String,
    pub bridge: bool,
}

/// Steps every member once a round until all show the safety words (each step sends that
/// member's commitment, then its value; the creator's first step locks membership), and
/// returns them.
pub fn safety_words(relay: &str, lwd: &str, members: &[Setup], invite: &str) -> String {
    for _ in 0..300 {
        let words: Vec<Option<String>> = members
            .iter()
            .map(|m| step(relay, lwd, m, invite, None, None).safety_number)
            .collect();
        if words.iter().all(Option::is_some) {
            let words: Vec<String> = words.into_iter().flatten().collect();
            assert!(
                words.iter().all(|w| w == &words[0]),
                "same words everywhere"
            );
            return words[0].clone();
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("no safety words");
}

fn step(
    relay: &str,
    lwd: &str,
    m: &Setup,
    invite: &str,
    confirmed: Option<String>,
    birthday: Option<u32>,
) -> vault::SetupProgress {
    vault::setup_step(
        relay.into(),
        lwd.into(),
        "regtest".into(),
        m.seeds.clone(),
        invite.into(),
        m.state_dir.clone(),
        confirmed,
        birthday,
        None,
        None,
    )
    .unwrap()
}

/// Key generation the way the app runs it: one step per member per round, nobody waiting
/// for anyone (each step only does what the messages so far allow). Returns each
/// member's vault material, in order.
pub fn keygen(
    relay: &str,
    lwd: &str,
    members: &[Setup],
    invite: &str,
    words: &str,
    birthday: Option<u32>,
) -> Vec<Vec<u8>> {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let client = zafe_core::relay_client::RelayClient::new(relay.to_owned());
    let decoded = Invite::decode(invite).unwrap();
    let mut done: Vec<Option<Vec<u8>>> = vec![None; members.len()];
    for _ in 0..600 {
        for (i, m) in members.iter().enumerate() {
            if done[i].is_some() {
                continue;
            }
            done[i] = if m.bridge {
                step(relay, lwd, m, invite, Some(words.into()), birthday).material
            } else {
                let me = Identity::from_seeds(IdentitySeeds::from_bytes(&m.seeds).unwrap());
                rt.block_on(async {
                    let membership = node::membership(&client, &me, &decoded).await.unwrap();
                    let membership = node::seal_when_full(&client, &me, &decoded, membership)
                        .await
                        .unwrap();
                    match node::keygen_step(
                        &client,
                        &me,
                        &decoded,
                        &membership,
                        std::path::Path::new(&m.state_dir),
                        &NonceKey::derive(&m.seeds, &decoded.vault_id),
                        Some(words),
                        &regtest_network(),
                        "regtest",
                        KeygenOptions {
                            creator_birthday_height: birthday,
                            ..Default::default()
                        },
                        &mut rand::rngs::OsRng,
                    )
                    .await
                    .unwrap()
                    {
                        KeygenProgress::Done(material) => Some(material.to_bytes().unwrap()),
                        KeygenProgress::Waiting { .. } => None,
                    }
                })
            };
            if done[i].is_some() {
                vault::forget_keygen(m.state_dir.clone()).unwrap();
            }
        }
        if done.iter().all(Option::is_some) {
            return done.into_iter().flatten().collect();
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    panic!("key generation didn't finish");
}
