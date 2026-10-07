//! End-to-end on a local Ironwood regtest network (`ths`: Zakura + lightwalletd in Docker):
//! a 2-of-3 vault is funded from the `ths` faucet, every member syncs its own wallet,
//! one member proposes a payment, members approve and sign, and the transaction is
//! proven, broadcast, mined and seen by the recipient.
//!
//! Needs Docker and `ths` (see `infra/regtest/up.sh`). Run with:
//!   cargo test -p zafe-core --test regtest_e2e -- --ignored --nocapture

use std::{collections::BTreeMap, path::PathBuf, process::Command};

use orchard::{
    circuit::{OrchardCircuitVersion, ProvingKey, VerifyingKey},
    keys::{FullViewingKey, Scope, SpendingKey},
};
use pczt::roles::{prover::Prover, tx_extractor::TransactionExtractor};
use rand::{rngs::StdRng, SeedableRng};
use zafe_core::{
    session::{Leader, Member, MemoryNonceStore},
    tx,
    verify::{verify_pczt, Expectations, Payment},
    wallet::{connect, regtest_network, NoteHold, PaymentRequest, VaultWallet, WalletKey},
};
use zcash_client_backend::proto::service::RawTransaction;
use zcash_keys::{address::UnifiedAddress, keys::UnifiedFullViewingKey};
use zcash_protocol::{
    consensus::{BlockHeight, BranchId},
    memo::Memo,
};

mod common;
use common::{params, run_keygen};

const NAME: &str = "zafe-e2e";
// ths moves every port by this offset: Zakura RPC 48232, lightwalletd 39067.
const PORT_OFFSET: u16 = 30000;
const LWD_PORT: u16 = 9067 + PORT_OFFSET;

/// A `ths` (thus-spoke-zakura) environment started by `infra/regtest/up.sh`.
struct Regtest;

impl Regtest {
    fn script(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../infra/regtest")
            .join(name)
    }

    fn start() -> Self {
        let status = Command::new(Self::script("up.sh"))
            .env("ZAFE_REGTEST_NAME", NAME)
            .env("ZAFE_REGTEST_PORT_OFFSET", PORT_OFFSET.to_string())
            .status()
            .expect("run up.sh");
        assert!(status.success(), "regtest failed to start");
        Self
    }

    /// Several 5 ZEC faucet notes, confirmed.
    fn fund(&self, address: &str, notes: u32) {
        let status = Command::new(Self::script("fund.sh"))
            .args([address, &notes.to_string()])
            .env("ZAFE_REGTEST_NAME", NAME)
            .status()
            .expect("run fund.sh");
        assert!(status.success(), "funding failed");
    }

    fn mine(&self, blocks: u32) {
        let ths = std::env::var("THS").unwrap_or_else(|_| "ths".into());
        let out = Command::new(ths)
            .args(["--name", NAME, "mine", &blocks.to_string()])
            .output()
            .expect("run ths mine");
        assert!(
            out.status.success(),
            "ths mine failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

impl Drop for Regtest {
    fn drop(&mut self) {
        let _ = Command::new(Self::script("down.sh"))
            .env("ZAFE_REGTEST_NAME", NAME)
            .status();
    }
}

async fn chain_tip(client: &mut zafe_core::wallet::Client) -> u64 {
    client
        .get_latest_block(zcash_client_backend::proto::service::ChainSpec::default())
        .await
        .expect("chain tip")
        .into_inner()
        .height
}

async fn wait_for_lightwalletd_height(client: &mut zafe_core::wallet::Client, height: u64) {
    for _ in 0..120 {
        let tip = client
            .get_latest_block(zcash_client_backend::proto::service::ChainSpec::default())
            .await
            .map(|r| r.into_inner().height)
            .unwrap_or(0);
        if tip >= height {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    panic!("lightwalletd did not reach height {height}");
}

#[tokio::test]
#[ignore = "needs Docker; see module docs"]
async fn vault_pays_on_regtest() {
    let network = regtest_network();
    let mut rng = StdRng::seed_from_u64(99);

    // 1. Create a 2-of-3 vault.
    let members = run_keygen(params(2, 3), &mut rng);
    let vault_keys = &members[0].1.vault_keys;
    let vault_fvk = vault_keys.fvk().clone();
    let vault_ufvk = vault_keys.ufvk().unwrap();
    let vault_ua = UnifiedAddress::from_receivers(
        Some(vault_fvk.address_at(0u32, Scope::External)),
        None,
        None,
    )
    .unwrap()
    .encode(&network);
    println!("vault address: {vault_ua}");

    // 2. Start regtest and fund the vault from the ths faucet: several notes, so the
    // reservation checks below have notes to spare.
    let chain = Regtest::start();
    chain.fund(&vault_ua, 4);
    let lwd = format!("http://127.0.0.1:{LWD_PORT}");
    let mut client = connect(&lwd).await.expect("lightwalletd");
    let funded = chain_tip(&mut client).await;
    wait_for_lightwalletd_height(&mut client, funded).await;

    // 3. Each member keeps its own wallet database and syncs independently.
    let dir = tempdir();
    let mut wallets = Vec::new();
    for (i, _) in members.iter().enumerate() {
        let path = dir.join(format!("member{i}.sqlite"));
        let mut w = VaultWallet::create(
            &path,
            &WalletKey::random(),
            network,
            "vault",
            &vault_ufvk,
            2,
            &mut client,
        )
        .await
        .unwrap();
        w.sync(&mut client).await.unwrap();
        wallets.push(w);
    }
    let before = wallets[0].balance().unwrap();
    println!("vault balance after funding: {before:?}");
    assert!(
        before.ironwood_spendable > 0,
        "vault has spendable Ironwood funds"
    );
    for w in &wallets[1..] {
        assert_eq!(
            w.balance().unwrap(),
            before,
            "every member sees the same balance"
        );
    }

    // 4. Member 0 proposes paying 1 ZEC with a memo to an outside recipient.
    let recipient_fvk = FullViewingKey::from(&SpendingKey::from_bytes([77; 32]).unwrap());
    let recipient = recipient_fvk.address_at(0u32, Scope::External);
    let recipient_ua = UnifiedAddress::from_receivers(Some(recipient), None, None)
        .unwrap()
        .encode(&network);
    let memo = Memo::from_bytes(b"Grant milestone 1").unwrap().encode();
    let amount = 100_000_000;
    let pczt = wallets[0]
        .propose(
            &[PaymentRequest {
                address: recipient_ua.clone(),
                amount_zat: amount,
                memo: Some(memo.clone()),
            }],
            8064,
        )
        .expect("proposal");

    // 4b. Note reservation: member 1's wallet holds back the notes member 0's open
    // proposal spends, so its own proposal picks other notes, and asking for more than
    // what's left fails as "reserved", not "insufficient".
    let hold = NoteHold {
        owner: [1; 32],
        nullifiers: tx::spent_nullifiers(&pczt).unwrap(),
        expiry_height: *pczt.global().expiry_height(),
    };
    let other = PaymentRequest {
        address: recipient_ua.clone(),
        amount_zat: amount,
        memo: None,
    };
    let unreserved = wallets[1]
        .propose(std::slice::from_ref(&other), 8064)
        .unwrap();
    let overlap = |a: &pczt::Pczt, b: &pczt::Pczt| {
        let a = tx::spent_nullifiers(a).unwrap();
        tx::spent_nullifiers(b)
            .unwrap()
            .iter()
            .any(|nf| a.contains(nf))
    };
    assert!(
        overlap(&pczt, &unreserved),
        "without reservation both members pick the same notes"
    );
    let held = wallets[1].reserve(std::slice::from_ref(&hold)).unwrap();
    assert!(held > 0, "the proposal's notes are held");
    assert_eq!(
        wallets[1].reserve(std::slice::from_ref(&hold)).unwrap(),
        held,
        "idempotent"
    );
    let reserved = wallets[1]
        .propose(std::slice::from_ref(&other), 8064)
        .unwrap();
    assert!(!overlap(&pczt, &reserved), "reserved notes are skipped");
    let spendable = wallets[1].balance().unwrap().ironwood_spendable;
    assert!(
        spendable < before.ironwood_spendable,
        "held notes aren't spendable"
    );
    let too_much = PaymentRequest {
        amount_zat: spendable,
        ..other.clone()
    };
    assert!(matches!(
        wallets[1].propose(std::slice::from_ref(&too_much), 8064),
        Err(zafe_core::wallet::WalletError::FundsReserved)
    ));
    // Releasing (the proposal closed) makes the notes spendable again.
    assert_eq!(wallets[1].reserve(&[]).unwrap(), 0);
    assert_eq!(
        wallets[1].balance().unwrap().ironwood_spendable,
        before.ironwood_spendable
    );
    wallets[1].propose(&[too_much], 8064).unwrap();

    // 5. Members verify independently, approve, and sign (2 of 3).
    let tip = wallets[0].chain_height().unwrap().unwrap();
    let expected = Expectations {
        payments: vec![Payment {
            recipient,
            amount_zat: amount,
            memo: *memo.as_array(),
        }],
        consensus_branch_id: BranchId::for_height(&network, BlockHeight::from_u32(tip + 1)).into(),
        tip_height: tip,
        max_expiry_delta: 8064 + 1 + 96 + zafe_core::vault::EXPIRY_ROUNDING_BLOCKS,
    };
    let verified = verify_pczt(&pczt, &vault_fvk, &expected).expect("member verification");
    println!(
        "verified: fee {} zat, change {} zat",
        verified.fee_zat, verified.change_total_zat
    );

    let proposal_id = [1u8; 16];
    let mut leader = Leader::new(proposal_id, &pczt, &verified, 2).unwrap();
    let mut stores: Vec<MemoryNonceStore> = (0..3).map(|_| MemoryNonceStore::default()).collect();
    for i in [1, 2] {
        let (id, out) = &members[i];
        let m = Member {
            identifier: *id,
            key_package: &out.key_package,
            vault_fvk: out.vault_keys.fvk(),
        };
        let (approval, _) = m
            .approve(proposal_id, &pczt, &expected, &mut stores[i], &mut rng)
            .unwrap();
        assert!(leader.add_approval(approval));
    }
    let request = leader.request(&[members[1].0, members[2].0]).unwrap();
    let mut shares = BTreeMap::new();
    for i in [1, 2] {
        let (id, out) = &members[i];
        let m = Member {
            identifier: *id,
            key_package: &out.key_package,
            vault_fvk: out.vault_keys.fvk(),
        };
        shares.insert(
            *id,
            m.sign(&request, &pczt, &expected, &mut stores[i]).unwrap(),
        );
    }
    let signatures = leader
        .aggregate(&request, &shares, &members[0].1.public_key_package)
        .unwrap();

    // 6. Inject, prove, extract (fully verified), broadcast, mine.
    let signed = tx::apply_signatures(pczt, &signatures).unwrap();
    let pk = ProvingKey::build(OrchardCircuitVersion::PostNu6_3);
    let vk = VerifyingKey::build(OrchardCircuitVersion::PostNu6_3);
    let proved = Prover::new(signed)
        .create_ironwood_proof(&pk)
        .unwrap()
        .finish();
    let transaction = TransactionExtractor::new(proved)
        .with_orchard(&vk)
        .extract()
        .expect("valid transaction");
    let mut raw = Vec::new();
    transaction.write(&mut raw).unwrap();
    let reply = client
        .send_transaction(RawTransaction {
            data: raw,
            height: 0,
        })
        .await
        .expect("send")
        .into_inner();
    println!(
        "broadcast: code {} {}",
        reply.error_code, reply.error_message
    );
    assert_eq!(
        reply.error_code, 0,
        "node rejected transaction: {}",
        reply.error_message
    );
    println!("txid {}", transaction.txid());
    // Dropped-broadcast detection reads the whole mempool (txids in protocol order).
    let txid: [u8; 32] = *transaction.txid().as_ref();
    let mut in_mempool = false;
    for _ in 0..20 {
        if zafe_core::wallet::mempool_txids(&mut client)
            .await
            .unwrap()
            .contains(&txid)
        {
            in_mempool = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    assert!(in_mempool, "broadcast transaction not seen in the mempool");
    assert!(!wallets[0].tx_mined(&txid).unwrap());

    chain.mine(2);
    wait_for_lightwalletd_height(&mut client, u64::from(tip) + 2).await;

    // 7. The vault sees the spend; the recipient sees the payment.
    wallets[1].sync(&mut client).await.unwrap();
    let after = wallets[1].balance().unwrap();
    println!("vault balance after payment: {after:?}");

    // The unapproved-spend query sees exactly this transaction, mined, with the notes it
    // spent (it is what the app's alert compares against the log).
    let spends = wallets[1].vault_spends().unwrap();
    let spend = spends
        .iter()
        .find(|s| s.txid == txid)
        .expect("the wallet lists the vault's spend");
    assert!(spend.mined_height.is_some(), "mined spend has a height");
    assert!(!spend.nullifiers.is_empty(), "spent notes are listed");
    assert_eq!(spends.len(), 1, "no other spends: {spends:?}");

    let recipient_ufvk = UnifiedFullViewingKey::from_orchard_fvk(recipient_fvk).unwrap();
    let mut recipient_wallet = VaultWallet::create(
        &dir.join("recipient.sqlite"),
        &WalletKey::random(),
        network,
        "recipient",
        &recipient_ufvk,
        2,
        &mut client,
    )
    .await
    .unwrap();
    recipient_wallet.sync(&mut client).await.unwrap();
    let received = recipient_wallet.balance().unwrap();
    println!("recipient balance: {received:?}");
    assert_eq!(
        received.ironwood_total, amount,
        "recipient received the payment in Ironwood"
    );
}

fn tempdir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zafe-e2e-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
