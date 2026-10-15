use rand::{rngs::StdRng, SeedableRng};
use zafe_proto::{Envelope, Identity, Kind, ProtoError, Recipient, ReplayGuard};

const MAILBOX: [u8; 16] = [9; 16];

fn identities(n: usize) -> (StdRng, Vec<Identity>) {
    let mut rng = StdRng::seed_from_u64(1);
    let ids = (0..n).map(|_| Identity::generate(&mut rng)).collect();
    (rng, ids)
}

#[test]
fn public_envelope_round_trip() {
    let (_, ids) = identities(2);
    let env = Envelope::public(&ids[0], MAILBOX, 1, Kind::DkgRound1, b"round-1 package").unwrap();
    let decoded = Envelope::from_bytes(&env.to_bytes().unwrap()).unwrap();
    assert_eq!(decoded, env);
    assert_eq!(
        decoded.open(&ids[1], ids[0].public()).unwrap(),
        b"round-1 package"
    );
}

#[test]
fn sealed_envelope_only_opens_for_recipient() {
    let (mut rng, ids) = identities(3);
    let env = Envelope::sealed(
        &ids[0],
        ids[1].public(),
        MAILBOX,
        1,
        Kind::DkgRound2,
        b"secret share",
        &mut rng,
    )
    .unwrap();
    assert_eq!(env.header.to, Recipient::One(ids[1].public().sig_pk));
    assert!(
        !env.body.windows(12).any(|w| w == b"secret share"),
        "payload must not appear in clear"
    );
    assert_eq!(env.open(&ids[1], ids[0].public()).unwrap(), b"secret share");
    assert_eq!(
        env.open(&ids[2], ids[0].public()).unwrap_err(),
        ProtoError::NotForMe
    );
}

#[test]
fn tampering_is_detected() {
    let (mut rng, ids) = identities(2);
    let env = Envelope::sealed(
        &ids[0],
        ids[1].public(),
        MAILBOX,
        1,
        Kind::DkgRound2,
        b"x",
        &mut rng,
    )
    .unwrap();

    let mut body = env.clone();
    *body.body.last_mut().unwrap() ^= 1;
    assert_eq!(
        body.open(&ids[1], ids[0].public()).unwrap_err(),
        ProtoError::BadSignature
    );

    let mut header = env.clone();
    header.header.seq = 2;
    assert_eq!(
        header.open(&ids[1], ids[0].public()).unwrap_err(),
        ProtoError::BadSignature
    );

    // Claiming a different sender than the signer.
    assert_eq!(
        env.verify(ids[1].public()).unwrap_err(),
        ProtoError::WrongSender
    );
}

#[test]
fn ciphertext_cannot_be_moved_to_another_header() {
    // Member 2 intercepts a payload sealed to member 1 and re-sends it, validly signed by
    // itself, under a different header. The header is the HPKE AAD, so opening fails.
    let (mut rng, ids) = identities(3);
    let original = Envelope::sealed(
        &ids[0],
        ids[1].public(),
        MAILBOX,
        1,
        Kind::SkContribution,
        b"r_i",
        &mut rng,
    )
    .unwrap();
    let mut header = original.header.clone();
    header.from = ids[2].public().sig_pk;
    header.seq = 7;
    let transplanted = Envelope::signed_raw(&ids[2], header, original.body.clone()).unwrap();

    transplanted
        .verify(ids[2].public())
        .expect("validly signed by the attacker");
    assert_eq!(
        transplanted.open(&ids[1], ids[2].public()).unwrap_err(),
        ProtoError::Crypto
    );
}

#[test]
fn replay_guard_rejects_old_sequence_numbers() {
    let (_, ids) = identities(1);
    let mut guard = ReplayGuard::default();
    let e1 = Envelope::public(&ids[0], MAILBOX, 1, Kind::Approval, b"a").unwrap();
    let e2 = Envelope::public(&ids[0], MAILBOX, 2, Kind::Approval, b"b").unwrap();
    guard.check_and_record(&e1.header).unwrap();
    guard.check_and_record(&e2.header).unwrap();
    assert!(matches!(
        guard.check_and_record(&e1.header),
        Err(ProtoError::Replay { .. })
    ));
    assert!(matches!(
        guard.check_and_record(&e2.header),
        Err(ProtoError::Replay { .. })
    ));
}

#[test]
fn identity_is_deterministic_from_seeds() {
    let (_, ids) = identities(1);
    let again = Identity::from_seeds(ids[0].seeds().clone());
    assert_eq!(again.public(), ids[0].public());
}
