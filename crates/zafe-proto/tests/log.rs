use rand::{rngs::StdRng, SeedableRng};
use zafe_proto::{
    log::GENESIS_PREV_HASH, Chain, ChainError, Identity, IdentityPublic, LogEntry, LogKey,
    ProtoError,
};

const MAILBOX: [u8; 16] = [3; 16];

fn setup() -> (StdRng, Vec<Identity>, Vec<IdentityPublic>, LogKey) {
    let mut rng = StdRng::seed_from_u64(2);
    let ids: Vec<Identity> = (0..3).map(|_| Identity::generate(&mut rng)).collect();
    let publics = ids.iter().map(|i| *i.public()).collect();
    let key = LogKey::generate(0, &mut rng);
    (rng, ids, publics, key)
}

fn entry(
    rng: &mut StdRng,
    author: &Identity,
    key: &LogKey,
    chain: &Chain,
    event: &[u8],
) -> LogEntry {
    LogEntry::create(author, key, MAILBOX, chain.len(), chain.head(), event, rng).unwrap()
}

#[test]
fn members_build_and_replay_the_same_chain() {
    let (mut rng, ids, members, key) = setup();
    let mut relay = Chain::new(MAILBOX);
    for (i, event) in [b"vault created".as_slice(), b"proposal", b"approve"]
        .iter()
        .enumerate()
    {
        let e = entry(&mut rng, &ids[i], &key, &relay, event);
        relay.append(e, &members).unwrap();
    }
    let mut member = Chain::new(MAILBOX);
    for e in relay.entries() {
        member.append(e.clone(), &members).unwrap();
    }
    assert_eq!(member.head(), relay.head());
    assert_eq!(member.entries()[1].decrypt(&key).unwrap(), b"proposal");
}

#[test]
fn detects_fork_gap_and_outsiders() {
    let (mut rng, ids, members, key) = setup();
    let mut chain = Chain::new(MAILBOX);
    chain
        .append(entry(&mut rng, &ids[0], &key, &chain, b"a"), &members)
        .unwrap();

    let fork =
        LogEntry::create(&ids[1], &key, MAILBOX, 1, GENESIS_PREV_HASH, b"b", &mut rng).unwrap();
    assert_eq!(
        chain.clone().append(fork, &members).unwrap_err(),
        ChainError::Fork(1)
    );

    let skipped =
        LogEntry::create(&ids[1], &key, MAILBOX, 2, chain.head(), b"c", &mut rng).unwrap();
    assert!(matches!(
        chain.clone().append(skipped, &members),
        Err(ChainError::Gap {
            expected: 1,
            got: 2
        })
    ));

    let outsider = Identity::generate(&mut rng);
    let e = entry(&mut rng, &outsider, &key, &chain, b"d");
    assert_eq!(
        chain.clone().append(e, &members).unwrap_err(),
        ChainError::UnknownAuthor(1)
    );
}

#[test]
fn tampered_entries_fail() {
    let (mut rng, ids, members, key) = setup();
    let chain = Chain::new(MAILBOX);
    let good = entry(&mut rng, &ids[0], &key, &chain, b"payload");

    let mut ct = good.clone();
    *ct.ciphertext.last_mut().unwrap() ^= 1;
    assert!(matches!(
        chain.clone().append(ct.clone(), &members),
        Err(ChainError::Invalid { .. })
    ));
    assert_eq!(ct.decrypt(&key).unwrap_err(), ProtoError::Crypto);

    let mut author = good.clone();
    author.header.author = members[1].sig_pk;
    assert!(matches!(
        chain.clone().append(author, &members),
        Err(ChainError::Invalid { .. })
    ));
}

#[test]
fn old_or_foreign_keys_cannot_decrypt() {
    let (mut rng, ids, _, key) = setup();
    let e = entry(&mut rng, &ids[0], &key, &Chain::new(MAILBOX), b"secret");
    let rotated = LogKey::generate(1, &mut rng);
    assert_eq!(e.decrypt(&rotated).unwrap_err(), ProtoError::Crypto);
    let same_epoch_other_key = LogKey::generate(0, &mut rng);
    assert_eq!(
        e.decrypt(&same_epoch_other_key).unwrap_err(),
        ProtoError::Crypto
    );
}
