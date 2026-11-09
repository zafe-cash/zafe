//! `zafe interop` end to end through its files: two Zafe members (always), and a Zafe
//! member with frost-zec (`FROST_ZEC_BIN`, ignored by default): key generation, then a
//! spend signed with each side coordinating, and a refusal.
//!
//! ```text
//! git clone https://github.com/OpenPGP-Wallet/frost-zec && cd frost-zec && cargo build
//! FROST_ZEC_BIN=$PWD/target/debug/frost-zec \
//!   cargo test -p zafe-cli --test interop -- --ignored --nocapture
//! ```
//!
//! The PCZT spends an invented note of the new vault (a one-leaf tree), so it is checked
//! and signed for real but never broadcast.

use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Output},
    time::{Duration, Instant},
};

use orchard::{
    builder::BundleType,
    bundle::BundleVersion,
    keys::{FullViewingKey, Scope, SpendingKey},
    note::Note,
    note_encryption::IronwoodDomain,
    tree::{MerkleHashOrchard, MerklePath},
    Address, Anchor,
};
use pczt::{
    roles::{creator::Creator, io_finalizer::IoFinalizer},
    Pczt,
};
use serde_json::Value;
use shardtree::{store::memory::MemoryShardStore, ShardTree};
use zafe_core::{
    keys::{VaultKeys, VaultSecret},
    tx,
    wallet::ZafeNetwork,
};
use zcash_keys::address::UnifiedAddress;
use zcash_note_encryption::try_note_decryption;
use zcash_primitives::transaction::{
    builder::{BuildConfig, Builder, BundlePadding, PcztResult},
    fees::zip317,
};
use zcash_protocol::{memo::MemoBytes, value::Zatoshis};

/// A testnet height after NU7 (the PCZT is v6 with the NU7 branch id).
const TIP: u32 = 5_000_000;
const FUNDS: u64 = 1_000_000;
const PAY: u64 = 400_000;
/// ZIP 317 for the two actions a payment plus change takes.
const FEE: u64 = 10_000;

fn root(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zafe-interop-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// One `zafe` member (its own home), on testnet, offline.
struct Zafe(PathBuf);

impl Zafe {
    fn new(root: &Path, name: &str) -> Self {
        let z = Zafe(root.join(name));
        let out = z.zafe(&["init"]);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        z
    }

    fn zafe(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_zafe"))
            .arg("--home")
            .arg(&self.0)
            .args(["--network", "test"])
            .args(args)
            .output()
            .unwrap()
    }

    /// `zafe interop <args>`.
    fn run(&self, args: &[&str]) -> Output {
        self.zafe(&[&["interop"], args].concat())
    }

    /// `zafe interop <args>`, which must succeed.
    fn ok(&self, args: &[&str]) -> String {
        let out = self.run(args);
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        assert!(
            out.status.success(),
            "zafe {args:?} failed: {text}{}",
            String::from_utf8_lossy(&out.stderr)
        );
        text
    }

    fn seat(&self) -> Value {
        seat(&self.0.join("interop/vault.json"))
    }
}

fn seat(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn field<'a>(v: &'a Value, name: &str) -> &'a str {
    v[name].as_str().unwrap()
}

/// Runs `step` until `done` holds (each run is one step of the other side's pace).
fn until(what: &str, mut step: impl FnMut(), done: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        step();
        if done() {
            return;
        }
        assert!(Instant::now() < deadline, "timed out: {what}");
        std::thread::sleep(Duration::from_millis(300));
    }
}

fn wait(child: Child, what: &str) -> String {
    let out = child.wait_with_output().unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.status.success(), "{what} failed:\n{text}");
    text
}

fn vault_fvk(seat: &Value) -> FullViewingKey {
    let sk: [u8; 32] = hex::decode(field(seat, "sk")).unwrap().try_into().unwrap();
    let ak: [u8; 32] = hex::decode(field(seat, "ak")).unwrap().try_into().unwrap();
    VaultKeys::derive(&VaultSecret::from_bytes(sk), &ak)
        .unwrap()
        .fvk()
        .clone()
}

fn payee() -> Address {
    FullViewingKey::from(&SpendingKey::from_bytes([7; 32]).unwrap())
        .address_at(0u32, Scope::External)
}

fn encode(address: Address) -> String {
    UnifiedAddress::from_receivers(Some(address), None, None, None, None)
        .unwrap()
        .encode(&ZafeNetwork::Test)
}

/// A PCZT paying `PAY` to [`payee`] from an invented note of the vault, with change.
fn pczt(fvk: &FullViewingKey) -> Vec<u8> {
    let version = BundleVersion::ironwood_v3();
    let mut receive = orchard::builder::Builder::new(
        BundleType::DEFAULT,
        version,
        version.default_flags(),
        Anchor::empty_tree(),
    )
    .unwrap();
    receive
        .add_output(
            None,
            fvk.address_at(0u32, Scope::External),
            orchard::value::NoteValue::from_raw(FUNDS),
            MemoBytes::empty().into_bytes(),
        )
        .unwrap();
    let (bundle, meta) = receive
        .build::<i64>(&mut zafe_core::zrng::zrng())
        .unwrap()
        .unwrap();
    let action = &bundle.actions()[meta.output_action_index(0).unwrap()];
    let ivk = fvk.to_ivk(Scope::External).prepare();
    let (note, _, _): (Note, _, _) =
        try_note_decryption(&IronwoodDomain::for_action(action), &ivk, action).unwrap();

    let cmx: orchard::note::ExtractedNoteCommitment = note.commitment().into();
    let leaf = MerkleHashOrchard::from_cmx(&cmx);
    let mut tree =
        ShardTree::<_, 32, 16>::new(MemoryShardStore::<MerkleHashOrchard, u32>::empty(), 100);
    tree.append(leaf, incrementalmerkletree::Retention::Marked)
        .unwrap();
    tree.checkpoint(TIP).unwrap();
    let path = tree
        .witness_at_checkpoint_depth(0.into(), 0)
        .unwrap()
        .unwrap();
    let anchor: Anchor = path.root(leaf).into();
    let path: MerklePath = path.into();

    let mut builder = Builder::new(
        ZafeNetwork::Test,
        (TIP + 1).into(),
        BuildConfig::Standard {
            sapling_anchor: None,
            orchard_anchor: None,
            ironwood_anchor: Some(anchor),
            orchard_padding: BundlePadding::DEFAULT,
            ironwood_padding: BundlePadding::DEFAULT,
        },
    );
    builder
        .add_ironwood_spend::<zip317::FeeRule>(fvk.clone(), note, path)
        .unwrap();
    builder
        .add_ironwood_output::<zip317::FeeRule>(
            Some(fvk.to_ovk(Scope::External)),
            payee(),
            Zatoshis::const_from_u64(PAY),
            MemoBytes::empty(),
        )
        .unwrap();
    builder
        .add_ironwood_output::<zip317::FeeRule>(
            Some(fvk.to_ovk(Scope::Internal)),
            fvk.address_at(0u32, Scope::Internal),
            Zatoshis::const_from_u64(FUNDS - PAY - FEE),
            MemoBytes::empty(),
        )
        .unwrap();
    let PcztResult { pczt_parts, .. } = builder
        .build_for_pczt(zafe_core::zrng::zrng(), &zip317::FeeRule::standard())
        .unwrap();
    IoFinalizer::new(Creator::build_from_parts(pczt_parts).unwrap())
        .finalize_io(zafe_core::zrng::zrng())
        .unwrap()
        .serialize()
        .unwrap()
}

/// Every vault spend in the PCZT carries a signature (checked against its rk on insertion).
fn assert_signed(path: &Path, fvk: &FullViewingKey) {
    let signed = Pczt::parse(&std::fs::read(path).unwrap()).unwrap();
    assert!(tx::spends_to_sign(&signed, fvk).unwrap().is_empty());
}

fn expect_args(amount: u64) -> Vec<String> {
    vec![
        "--to".into(),
        encode(payee()),
        "--amount".into(),
        amount.to_string(),
        "--tip-height".into(),
        TIP.to_string(),
    ]
}

fn with<'a>(base: &[&'a str], extra: &'a [String]) -> Vec<&'a str> {
    base.iter()
        .copied()
        .chain(extra.iter().map(String::as_str))
        .collect()
}

#[test]
fn two_zafe_members_keygen_and_sign_in_both_roles() {
    let root = root("zafe");
    let ex = root.join("exchange");
    let ex = ex.to_str().unwrap();
    let (a, b) = (Zafe::new(&root, "a"), Zafe::new(&root, "b"));
    let vault_id = "00112233445566778899aabbccddeeff";

    // Round 1, round 2, finish: each member steps when the other's files are in.
    for z in [&a, &b] {
        z.ok(&["keygen", "--vault-id", vault_id, "--dir", ex]);
    }
    for z in [&a, &b] {
        z.ok(&["keygen", "--dir", ex]);
    }
    for z in [&a, &b] {
        z.ok(&["keygen", "--dir", ex]);
    }
    let (sa, sb) = (a.seat(), b.seat());
    for f in ["address", "ufvk", "ak", "sk"] {
        assert_eq!(field(&sa, f), field(&sb, f), "{f}");
    }
    assert!(field(&sa, "address").starts_with("utest"));
    let fvk = vault_fvk(&sa);
    let pczt_path = root.join("tx.pczt");
    std::fs::write(&pczt_path, pczt(&fvk)).unwrap();
    let pczt_arg = pczt_path.to_str().unwrap();

    // A coordinates, B co-signs, then the other way round.
    for (i, (coord, cosig)) in [(&a, &b), (&b, &a)].into_iter().enumerate() {
        let dir = root.join(format!("session{i}"));
        let dir = dir.to_str().unwrap();
        let out = root.join(format!("signed{i}.pczt"));
        let out = out.to_str().unwrap();
        let expect = expect_args(PAY);
        cosig.ok(&["cosign", "--dir", dir]);
        coord.ok(&with(
            &["coordinate", "--dir", dir, "--pczt", pczt_arg, "--out", out],
            &expect,
        ));
        cosig.ok(&with(&["cosign", "--dir", dir], &expect));
        coord.ok(&with(&["coordinate", "--dir", dir, "--out", out], &expect));
        assert_signed(Path::new(out), &fvk);
    }

    // The co-signer expects another amount: it refuses, and the coordinator learns why.
    let dir = root.join("refused");
    let dir = dir.to_str().unwrap();
    let out = root.join("never.pczt");
    let out = out.to_str().unwrap();
    b.ok(&["cosign", "--dir", dir]);
    a.ok(&with(
        &["coordinate", "--dir", dir, "--pczt", pczt_arg, "--out", out],
        &expect_args(PAY),
    ));
    let refused = b.run(&with(&["cosign", "--dir", dir], &expect_args(PAY + 1)));
    assert!(!refused.status.success());
    let failed = a.run(&with(
        &["coordinate", "--dir", dir, "--out", out],
        &expect_args(PAY),
    ));
    assert!(!failed.status.success());
    assert!(String::from_utf8_lossy(&failed.stderr).contains("refused"));
    assert!(!Path::new(out).exists());

    // A message of another format is refused.
    let other = root.join("other");
    std::fs::create_dir_all(&other).unwrap();
    let c = Zafe::new(&root, "c");
    c.ok(&[
        "keygen",
        "--vault-id",
        vault_id,
        "--dir",
        other.to_str().unwrap(),
    ]);
    std::fs::write(
        other.join("round1.ffffffff.json"),
        br#"{"format":"frost-zec-interop-v1","kind":"dkg-round1","network":"test","vault_id":"00112233445566778899aabbccddeeff","min_signers":2,"max_signers":2,"identity":"00","identifier":"00","package":"00","sk_commitment":"00"}"#,
    )
    .unwrap();
    let err = c.run(&["keygen", "--dir", other.to_str().unwrap()]);
    assert!(String::from_utf8_lossy(&err.stderr).contains("frost-zec-interop-v1"));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
#[ignore = "needs FROST_ZEC_BIN (a frost-zec build)"]
fn zafe_and_frost_zec_share_a_vault_and_sign_in_both_roles() {
    let bin = std::env::var("FROST_ZEC_BIN").expect("FROST_ZEC_BIN");
    let root = root("frost-zec");
    let ex = root.join("exchange");
    let fz_vault = root.join("frost-zec-vault");
    let z = Zafe::new(&root, "zafe");
    let frost = |args: &[&str], dir: &Path| {
        Command::new(&bin)
            .args(args)
            .arg("--dir")
            .arg(dir)
            .current_dir(&root)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap()
    };
    let vault_id = hex::encode(rand::random::<[u8; 16]>());

    // Key generation: frost-zec waits for files; Zafe steps until its seat exists.
    let child = frost(
        &[
            "interop",
            "dkg",
            "--network",
            "test",
            "--vault-id",
            &vault_id,
            "--out",
            fz_vault.to_str().unwrap(),
        ],
        &ex,
    );
    let ex_arg = ex.to_str().unwrap().to_owned();
    until(
        "Zafe's key generation",
        || {
            z.ok(&["keygen", "--vault-id", &vault_id, "--dir", &ex_arg]);
        },
        || z.0.join("interop/vault.json").exists(),
    );
    println!("{}", wait(child, "frost-zec interop dkg"));
    let (ours, theirs) = (z.seat(), seat(&fz_vault.join("vault.json")));
    for f in ["address", "ak", "sk", "vault_id"] {
        assert_eq!(field(&ours, f), field(&theirs, f), "{f}");
    }
    println!("same vault on both sides: {}", field(&ours, "address"));
    let fvk = vault_fvk(&ours);
    let pczt_path = root.join("tx.pczt");
    std::fs::write(&pczt_path, pczt(&fvk)).unwrap();
    let expect = expect_args(PAY);

    // frost-zec coordinates, Zafe co-signs.
    let s1 = root.join("session1");
    let s1_arg = s1.to_str().unwrap().to_owned();
    let signed1 = root.join("signed-by-frost-zec.pczt");
    z.ok(&["cosign", "--dir", &s1_arg]);
    let child = frost(
        &[
            "coordinate",
            "--vault",
            fz_vault.to_str().unwrap(),
            "--pczt",
            pczt_path.to_str().unwrap(),
            "--out",
            signed1.to_str().unwrap(),
        ],
        &s1,
    );
    wait_for(&s1.join("request.json"));
    z.ok(&with(&["cosign", "--dir", &s1_arg], &expect));
    println!("{}", wait(child, "frost-zec coordinate"));
    assert_signed(&signed1, &fvk);

    // Zafe coordinates, frost-zec co-signs.
    let s2 = root.join("session2");
    let s2_arg = s2.to_str().unwrap().to_owned();
    let signed2 = root.join("signed-by-zafe.pczt");
    let signed2_arg = signed2.to_str().unwrap().to_owned();
    let child = frost(
        &["cosign", "--vault", fz_vault.to_str().unwrap(), "--yes"],
        &s2,
    );
    let pczt_arg = pczt_path.to_str().unwrap().to_owned();
    until(
        "Zafe coordinating",
        || {
            z.ok(&with(
                &[
                    "coordinate",
                    "--dir",
                    &s2_arg,
                    "--pczt",
                    &pczt_arg,
                    "--out",
                    &signed2_arg,
                ],
                &expect,
            ));
        },
        || signed2.exists(),
    );
    println!("{}", wait(child, "frost-zec cosign"));
    assert_signed(&signed2, &fvk);

    // Zafe expects another amount: it refuses, and frost-zec's coordinator stops.
    let s3 = root.join("session3");
    let s3_arg = s3.to_str().unwrap().to_owned();
    z.ok(&["cosign", "--dir", &s3_arg]);
    let child = frost(
        &[
            "coordinate",
            "--vault",
            fz_vault.to_str().unwrap(),
            "--pczt",
            pczt_path.to_str().unwrap(),
            "--out",
            root.join("never.pczt").to_str().unwrap(),
        ],
        &s3,
    );
    let wrong = expect_args(PAY + 1);
    wait_for(&s3.join("request.json"));
    assert!(!z
        .run(&with(&["cosign", "--dir", &s3_arg], &wrong))
        .status
        .success());
    let out = child.wait_with_output().unwrap();
    assert!(!out.status.success());
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(text.contains("refused"), "{text}");
    println!("frost-zec on Zafe's refusal: {}", text.trim());

    let _ = std::fs::remove_dir_all(root);
}

/// Waits for the other side's file (frost-zec writes it atomically).
fn wait_for(file: &Path) {
    let deadline = Instant::now() + Duration::from_secs(120);
    while !file.exists() {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {}",
            file.display()
        );
        std::thread::sleep(Duration::from_millis(300));
    }
}
