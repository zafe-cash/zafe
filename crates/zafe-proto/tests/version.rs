use rand::{rngs::StdRng, SeedableRng};
use zafe_proto::{
    relay::{decode_body, InboxResponse, LogResponse, MembersRead, Signed},
    version::{self, Format, UnsupportedVersion},
    Envelope, Identity, IdentitySeeds, Kind, LogEntry, LogKey, ProtoError,
};

const MAILBOX: [u8; 16] = [5; 16];

fn bump(mut bytes: Vec<u8>) -> Vec<u8> {
    let v = u16::from_le_bytes([bytes[0], bytes[1]]) + 1;
    bytes[..2].copy_from_slice(&v.to_le_bytes());
    bytes
}

fn newer(format: Format) -> ProtoError {
    ProtoError::UnsupportedVersion(UnsupportedVersion {
        format,
        found: format.current() + 1,
        supported: format.current(),
    })
}

#[test]
fn envelopes_are_versioned_and_sign_their_version() {
    let mut rng = StdRng::seed_from_u64(1);
    let ids: Vec<Identity> = (0..2).map(|_| Identity::generate(&mut rng)).collect();
    let env = Envelope::public(&ids[0], MAILBOX, 1, Kind::DkgEcho, b"echo").unwrap();
    assert_eq!(env.header.version, version::ENVELOPE);
    let bytes = env.to_bytes().unwrap();
    assert_eq!(&bytes[..2], &version::ENVELOPE.to_le_bytes());
    assert_eq!(Envelope::from_bytes(&bytes).unwrap(), env);
    assert_eq!(
        Envelope::from_bytes(&bump(bytes)).unwrap_err(),
        newer(Format::Envelope)
    );

    let mut header = env.header.clone();
    header.version += 1;
    let future = Envelope::signed_raw(&ids[0], header, env.body.clone()).unwrap();
    assert_eq!(
        future.verify(ids[0].public()).unwrap_err(),
        newer(Format::Envelope)
    );
    let mut downgraded = future.clone();
    downgraded.header.version = version::ENVELOPE;
    assert_eq!(
        downgraded.verify(ids[0].public()).unwrap_err(),
        ProtoError::BadSignature
    );
}

#[test]
fn log_entries_are_versioned_and_sign_their_version() {
    let mut rng = StdRng::seed_from_u64(2);
    let me = Identity::generate(&mut rng);
    let key = LogKey::generate(0, &mut rng);
    let entry = LogEntry::create(&me, &key, MAILBOX, 0, [0; 32], b"created", &mut rng).unwrap();
    assert_eq!(entry.header.version, version::LOG_ENTRY);
    let bytes = entry.to_bytes().unwrap();
    assert_eq!(LogEntry::from_bytes(&bytes).unwrap(), entry);
    assert_eq!(
        LogEntry::from_bytes(&bump(bytes)).unwrap_err(),
        newer(Format::LogEntry)
    );

    let mut tampered = entry.clone();
    tampered.header.version += 1;
    assert_eq!(
        tampered.verify_signature(me.public()).unwrap_err(),
        newer(Format::LogEntry)
    );
    assert_eq!(tampered.decrypt(&key).unwrap_err(), ProtoError::Crypto);
}

#[test]
fn relay_bodies_are_versioned_and_requests_sign_the_version() {
    let mut rng = StdRng::seed_from_u64(3);
    let me = Identity::generate(&mut rng);
    let req = Signed::new(
        &me,
        MembersRead {
            mailbox: MAILBOX,
            timestamp: 1,
        },
    )
    .unwrap();
    let bytes = req.to_bytes().unwrap();
    assert_eq!(&bytes[..2], &version::RELAY_API.to_le_bytes());
    let back = Signed::<MembersRead>::from_bytes(&bytes).unwrap();
    assert!(back.verify().is_ok());
    assert_eq!(
        Signed::<MembersRead>::from_bytes(&bump(bytes)).unwrap_err(),
        newer(Format::RelayApi)
    );
    assert_eq!(
        decode_body::<LogResponse>(&[0xFF, 0xFF]).unwrap_err(),
        ProtoError::UnsupportedVersion(UnsupportedVersion {
            format: Format::RelayApi,
            found: 0xFFFF,
            supported: version::RELAY_API
        })
    );
}

#[test]
fn responses_carry_raw_items_so_one_unknown_item_is_isolated() {
    let mut rng = StdRng::seed_from_u64(4);
    let me = Identity::generate(&mut rng);
    let good = Envelope::public(&me, MAILBOX, 1, Kind::DkgEcho, b"a").unwrap();
    let inbox = InboxResponse {
        envelopes: vec![
            (1, good.to_bytes().unwrap()),
            (2, bump(good.to_bytes().unwrap())),
        ],
    };
    assert_eq!(inbox.decoded(), vec![(1, good)]);

    let key = LogKey::generate(0, &mut rng);
    let entry = LogEntry::create(&me, &key, MAILBOX, 0, [0; 32], b"x", &mut rng).unwrap();
    let log = LogResponse {
        entries: vec![entry.to_bytes().unwrap(), bump(entry.to_bytes().unwrap())],
    };
    assert_eq!(log.decoded().unwrap_err(), newer(Format::LogEntry));
}

#[test]
fn identity_seeds_are_versioned() {
    let mut rng = StdRng::seed_from_u64(5);
    let id = Identity::generate(&mut rng);
    let bytes = id.seeds().to_bytes();
    assert_eq!(bytes.len(), 66);
    let back = Identity::from_seeds(IdentitySeeds::from_bytes(&bytes).unwrap());
    assert_eq!(back.public(), id.public());
    assert_eq!(
        IdentitySeeds::from_bytes(&bump(bytes)).err(),
        Some(newer(Format::IdentitySeeds))
    );
    assert_eq!(
        IdentitySeeds::from_bytes(&[1, 0, 1, 2]).err(),
        Some(ProtoError::Encoding)
    );
}
