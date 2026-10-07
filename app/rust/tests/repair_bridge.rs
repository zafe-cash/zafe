//! A lost phone's seat moves to a new phone through the Flutter bridge API (what the Dart
//! side calls): recovery code, co-signers' approvals, the helpers' repair inside their
//! regular refresh, and the new phone collecting its key. No chain needed.

use std::thread;

use rust_lib_zafe::api::{
    proposals::{self, ProposalList},
    repair::{self, RecoveryStage},
    vault,
};

fn start_relay() -> String {
    let (tx, rx) = std::sync::mpsc::channel();
    thread::spawn(move || {
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async move {
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                tx.send(listener.local_addr().unwrap()).unwrap();
                axum::serve(
                    listener,
                    zafe_relay::Relay::new()
                        .with_limits(zafe_relay::limits::Limits::hosted())
                        .router(),
                )
                .await
                .unwrap();
            })
    });
    format!("http://{}", rx.recv().unwrap())
}

#[test]
fn a_lost_phone_is_replaced_through_the_bridge() {
    let relay = start_relay();
    let tmp = std::env::temp_dir().join(format!("zafe-repair-bridge-{}", std::process::id()));
    rust_lib_zafe::api::app::init_log_cache(tmp.join("log").to_string_lossy().into_owned());
    let seeds: Vec<Vec<u8>> = (0..3).map(|_| vault::generate_identity().seeds).collect();
    let invite =
        vault::create_vault(relay.clone(), seeds[0].clone(), "Grants".into(), 2, 3).unwrap();
    for s in &seeds[1..] {
        vault::join_vault(relay.clone(), s.clone(), invite.clone()).unwrap();
    }
    vault::seal_vault(relay.clone(), seeds[0].clone(), invite.clone()).unwrap();
    let safety = vault::vault_membership(relay.clone(), seeds[0].clone(), invite.clone())
        .unwrap()
        .safety_number;
    let materials: Vec<Vec<u8>> = seeds
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let (relay, s, invite, sn) = (relay.clone(), s.clone(), invite.clone(), safety.clone());
            // The same dir `state_dir(i)` returns below.
            let state = tmp.join(format!("m{i}")).to_string_lossy().into_owned();
            thread::spawn(move || {
                vault::run_keygen(
                    relay,
                    "http://127.0.0.1:1".into(),
                    "regtest".into(),
                    s,
                    invite,
                    sn,
                    60,
                    Some(2),
                    None,
                    Some(state),
                )
                .unwrap()
            })
        })
        .collect::<Vec<_>>()
        .into_iter()
        .map(|h| h.join().unwrap())
        .collect();
    let state_dir = |i: usize| {
        let d = tmp.join(format!("m{i}"));
        std::fs::create_dir_all(&d).unwrap();
        d.to_string_lossy().into_owned()
    };
    let list = |i: usize, seeds: &[u8], material: &[u8]| -> ProposalList {
        proposals::list_proposals(
            relay.clone(),
            state_dir(i),
            seeds.to_vec(),
            material.to_vec(),
            None,
        )
        .unwrap()
    };

    // Member 0 loses their phone; the new phone shows its codes.
    let fresh = vault::generate_identity();
    let code = repair::recovery_code(fresh.seeds.clone()).unwrap();
    let parsed =
        repair::parse_recovery_code(format!("Move my seat please: {}", code.code)).unwrap();
    assert_eq!(parsed.safety_code, code.safety_code);
    assert_eq!(parsed.key_hex, fresh.public_key_hex);
    assert!(matches!(
        repair::check_recovery(relay.clone(), fresh.seeds.clone())
            .unwrap()
            .stage,
        RecoveryStage::Waiting
    ));

    // Member 1 approves; the others see the pending move with the same safety code.
    let old = vault::identity_public_key(seeds[0].clone()).unwrap();
    assert!(!repair::approve_seat_move(
        relay.clone(),
        seeds[1].clone(),
        materials[1].clone(),
        old.clone(),
        code.code.clone(),
    )
    .unwrap());
    let pending = list(2, &seeds[2], &materials[2]).seat_moves;
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].old_key_hex, old);
    assert_eq!(pending[0].safety_code, code.safety_code);
    assert_eq!((pending[0].approvals.len(), pending[0].needed), (1, 2));

    // Member 2 approves from the pending move's code: the seat moves.
    assert!(repair::approve_seat_move(
        relay.clone(),
        seeds[2].clone(),
        materials[2].clone(),
        old.clone(),
        pending[0].code.clone(),
    )
    .unwrap());

    // The helpers' regular refreshes repair the key; their material picks up the move.
    let mut updated = [materials[1].clone(), materials[2].clone()];
    let mut done = None;
    for _ in 0..4 {
        for (k, i) in [1usize, 2].into_iter().enumerate() {
            let l = list(i, &seeds[i], &updated[k]);
            assert!(l.seat_moves.is_empty());
            if let Some(m) = l.updated_material {
                updated[k] = m;
            }
        }
        let p = repair::check_recovery(relay.clone(), fresh.seeds.clone()).unwrap();
        if matches!(p.stage, RecoveryStage::Done) {
            done = Some(p);
            break;
        }
    }
    let done = done.expect("recovered");
    assert_eq!(done.name, "Grants");
    let summary = vault::vault_summary(done.material.clone()).unwrap();
    assert!(summary.members.contains(&fresh.public_key_hex));
    assert!(!summary.members.contains(&old));
    for m in &updated {
        let s = vault::vault_summary(m.clone()).unwrap();
        assert!(s.members.contains(&fresh.public_key_hex));
        assert_eq!(s.address, summary.address);
    }

    // Until the new phone checks in, members see the repair in progress.
    let repairs = list(1, &seeds[1], &updated[0]).repairs;
    assert_eq!(repairs.len(), 1);
    assert_eq!(repairs[0].new_key_hex, fresh.public_key_hex);
    assert_eq!((repairs[0].helpers.len(), repairs[0].attempt), (2, 0));

    // The new phone works like any member; its first refresh marks the repair done.
    let mine = list(3, &fresh.seeds, &done.material);
    assert!(mine.updated_material.is_none());
    assert!(mine.repairs.is_empty());
    assert!(list(1, &seeds[1], &updated[0]).repairs.is_empty());
    let _ = std::fs::remove_dir_all(&tmp);
}
