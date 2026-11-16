use rand::{rngs::StdRng, SeedableRng};
use zafe_core::log_cache::{LogCache, LogCacheError};
use zafe_proto::{log::GENESIS_PREV_HASH, Identity, LogEntry, LogKey};

const MAILBOX: [u8; 16] = [7; 16];

fn entries(n: u64) -> Vec<LogEntry> {
    let mut rng = StdRng::seed_from_u64(1);
    let id = Identity::generate(&mut rng);
    let key = LogKey::generate(0, &mut rng);
    let mut prev = GENESIS_PREV_HASH;
    (0..n)
        .map(|i| {
            let e = LogEntry::create(&id, &key, MAILBOX, i, prev, b"event", &mut rng).unwrap();
            prev = e.hash().unwrap();
            e
        })
        .collect()
}

#[test]
fn a_copy_round_trips_and_never_shrinks() {
    let dir = tempfile::tempdir().unwrap();
    let cache = LogCache::in_dir(dir.path().join("log"));
    assert!(
        cache.read(&MAILBOX).unwrap().is_empty(),
        "no file: no anchor"
    );
    let all = entries(5);
    assert!(cache.write(&MAILBOX, &all[..3]).unwrap());
    assert_eq!(cache.read(&MAILBOX).unwrap(), all[..3]);
    assert!(cache.write(&MAILBOX, &all).unwrap());
    assert!(!cache.write(&MAILBOX, &all[..4]).unwrap());
    assert!(!cache.write(&MAILBOX, &all).unwrap());
    assert_eq!(cache.read(&MAILBOX).unwrap(), all);
    assert!(cache.read(&[8; 16]).unwrap().is_empty());
    let names: Vec<_> = std::fs::read_dir(cache.dir())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(names.len(), 1, "{names:?}");
    cache.remove(&MAILBOX);
    assert!(cache.read(&MAILBOX).unwrap().is_empty());
}

#[test]
fn damaged_files_are_no_anchor_and_newer_ones_are_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let cache = LogCache::in_dir(dir.path());
    let all = entries(2);
    cache.write(&MAILBOX, &all).unwrap();
    let file = dir.path().join(format!("{}.log", hex::encode(MAILBOX)));

    let mut bytes = std::fs::read(&file).unwrap();
    bytes.truncate(5);
    std::fs::write(&file, &bytes).unwrap();
    assert!(cache.read(&MAILBOX).unwrap().is_empty());
    assert!(cache.write(&MAILBOX, &all).unwrap());
    assert_eq!(cache.read(&MAILBOX).unwrap(), all);

    let mut bytes = std::fs::read(&file).unwrap();
    bytes[..2].copy_from_slice(&99u16.to_le_bytes());
    std::fs::write(&file, &bytes).unwrap();
    let err = cache.read(&MAILBOX).unwrap_err();
    assert!(
        matches!(&err, LogCacheError::UnsupportedVersion(v) if v.is_newer()),
        "{err:?}"
    );
    assert!(cache.write(&MAILBOX, &entries(3)).is_err());
}

#[test]
fn concurrent_writers_never_leave_a_shorter_copy() {
    let dir = tempfile::tempdir().unwrap();
    let cache = LogCache::in_dir(dir.path());
    let all = entries(12);
    std::thread::scope(|s| {
        for n in 1..=12usize {
            let (cache, all) = (&cache, &all);
            s.spawn(move || {
                cache.write(&MAILBOX, &all[..n]).unwrap();
            });
        }
    });
    assert_eq!(cache.read(&MAILBOX).unwrap().len(), 12);
}
