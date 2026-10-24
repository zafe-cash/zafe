//! Encrypted vault backups (spec §12.2): round trip, wrong passphrase, tampering, bounds
//! on the key-derivation settings, membership check, passphrase rules.

use rand::{rngs::StdRng, SeedableRng};
use zafe_core::{
    backup::{self, BackupError, Contents, KdfParams},
    node::VaultMaterial,
    vault::{MemberInfo, VaultDescriptor},
};
use zafe_proto::{
    version::{self, Format, UnsupportedVersion},
    Identity,
};

const PASS: &str =
    "correct horse battery staple orbit lantern violet echo marble quiet river north";

fn material_for(ids: &[&Identity]) -> Vec<u8> {
    let descriptor = VaultDescriptor {
        vault_id: [7; 16],
        version: version::DESCRIPTOR,
        name: "Grants".into(),
        network: "regtest".into(),
        threshold: 2,
        members: ids
            .iter()
            .enumerate()
            .map(|(i, id)| MemberInfo {
                identity: *id.public(),
                frost_id: vec![i as u8],
                name: format!("m{i}"),
            })
            .collect(),
        group_public_key: [1; 32],
        ufvk: "uviewregtest1...".into(),
        address: "uregtest1...".into(),
        use_qsk: true,
        birthday_height: 2,
        proposal_expiry_blocks: 8064,
        epoch: 0,
        transcript_hash: [2; 32],
    };
    VaultMaterial {
        descriptor,
        key_package: vec![3; 40],
        public_key_package: vec![4; 40],
        vault_secret: [5; 32],
        log_key_epoch: 0,
        log_key: [6; 32],
        relay: zafe_core::node::VaultRelay::legacy([7; 16]),
    }
    .to_bytes()
    .unwrap()
}

fn seeds(id: &Identity) -> Vec<u8> {
    id.seeds().to_bytes()
}

fn contents(rng: &mut StdRng) -> (Contents, Identity) {
    let me = Identity::generate(rng);
    let other = Identity::generate(rng);
    (
        Contents {
            identity_seeds: seeds(&me),
            material: material_for(&[&me, &other]),
            invite: "zafe-invite-v1:abc".into(),
            created_at: 1_700_000_000,
            names: [(hex::encode([1u8; 32]), "Alice".to_string())].into(),
            log: vec![vec![1, 2, 3]],
        },
        me,
    )
}

#[test]
fn round_trip_as_bytes_and_text() {
    let mut rng = StdRng::seed_from_u64(1);
    let (c, _) = contents(&mut rng);
    let bytes = backup::encrypt(&c, PASS, KdfParams::DEFAULT, &mut rng).unwrap();
    assert!(backup::is_backup(&bytes));
    let back = backup::decrypt(&bytes, PASS).unwrap();
    assert_eq!(back.identity_seeds, c.identity_seeds);
    assert_eq!(back.material, c.material);
    assert_eq!(back.invite, c.invite);
    assert_eq!(back.names, c.names);

    // The text form survives line wrapping (e.g. pasted from a note).
    let text = backup::to_text(&bytes);
    assert!(text.starts_with("zafe-backup-v1:"));
    let wrapped: String = text
        .as_bytes()
        .chunks(40)
        .map(|c| std::str::from_utf8(c).unwrap())
        .collect::<Vec<_>>()
        .join("\n  ");
    assert_eq!(backup::from_text(&wrapped).unwrap(), bytes);
}

#[test]
fn wrong_passphrase_and_tampering_fail() {
    let mut rng = StdRng::seed_from_u64(2);
    let (c, _) = contents(&mut rng);
    let bytes = backup::encrypt(&c, PASS, KdfParams::DEFAULT, &mut rng).unwrap();
    assert_eq!(
        backup::decrypt(
            &bytes,
            "correct horse battery staple orbit lantern violet echo marble quiet river south"
        )
        .unwrap_err(),
        BackupError::WrongPassphraseOrDamaged
    );
    // Any flipped bit in the header (authenticated) or ciphertext fails.
    for i in [22, 40, bytes.len() - 1] {
        let mut t = bytes.clone();
        t[i] ^= 1;
        assert_eq!(
            backup::decrypt(&t, PASS).unwrap_err(),
            BackupError::WrongPassphraseOrDamaged
        );
    }
    // A file asking for weak (or absurd) key derivation is refused before any work.
    let mut weak = bytes.clone();
    weak[8..12].copy_from_slice(&1024u32.to_le_bytes()); // 1 MiB
    assert_eq!(
        backup::decrypt(&weak, PASS).unwrap_err(),
        BackupError::BadParams
    );
    let mut huge = bytes.clone();
    huge[12..16].copy_from_slice(&1000u32.to_le_bytes());
    assert_eq!(
        backup::decrypt(&huge, PASS).unwrap_err(),
        BackupError::BadParams
    );
    // Not a backup at all.
    assert_eq!(
        backup::decrypt(b"hello", PASS).unwrap_err(),
        BackupError::NotABackup
    );
    let mut newer = bytes.clone();
    newer[7] = zafe_proto::version::BACKUP as u8 + 1;
    assert_eq!(
        backup::decrypt(&newer, PASS).unwrap_err(),
        BackupError::UnsupportedVersion(UnsupportedVersion {
            format: Format::Backup,
            found: zafe_proto::version::BACKUP + 1,
            supported: zafe_proto::version::BACKUP
        })
    );
}

#[test]
fn material_and_identity_inside_are_versioned() {
    let mut rng = StdRng::seed_from_u64(4);
    let (c, _) = contents(&mut rng);
    assert!(c.validate().is_ok());
    let mut newer = c.clone();
    newer.material[0] = 9;
    assert!(matches!(
        newer.validate(),
        Err(BackupError::UnsupportedVersion(v)) if v.format == Format::VaultMaterial && v.is_newer()
    ));
    let mut newer = c.clone();
    newer.identity_seeds[0] = 9;
    assert!(matches!(
        newer.validate(),
        Err(BackupError::UnsupportedVersion(v)) if v.format == Format::IdentitySeeds
    ));
}

#[test]
fn identity_must_be_a_member() {
    let mut rng = StdRng::seed_from_u64(3);
    let (mut c, _) = contents(&mut rng);
    c.identity_seeds = seeds(&Identity::generate(&mut rng)); // someone else
    assert_eq!(
        backup::encrypt(&c, PASS, KdfParams::DEFAULT, &mut rng).unwrap_err(),
        BackupError::NotAMember
    );
}

#[test]
fn passphrase_rules() {
    assert!(matches!(
        backup::check_passphrase("hunter2"),
        Err(BackupError::WeakPassphrase(_))
    ));
    assert!(matches!(
        backup::check_passphrase("Password2026!"),
        Err(BackupError::WeakPassphrase(_))
    ));
    assert!(backup::check_passphrase("Tr0ub4dor&3-quantum-Walrus-Kettle!").is_ok());
    assert!(backup::check_passphrase(PASS).is_ok());

    let mut rng = StdRng::seed_from_u64(4);
    let suggested = backup::suggest_passphrase(&mut rng);
    assert_eq!(suggested.split_whitespace().count(), 12);
    assert!(backup::check_passphrase(&suggested).is_ok());
    assert_ne!(suggested, backup::suggest_passphrase(&mut rng));
}
