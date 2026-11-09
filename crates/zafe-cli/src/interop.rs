//! `zafe interop`: one member of a vault shared with frost-zec
//! (OpenPGP-Wallet/frost-zec), through the JSON files of its interop format v0
//! (`frost-zec-interop-v0`, frost-zec `docs/interop.md`).
//!
//! A thin wrapper: the key generation, signing and verification are zafe-core's own
//! (`keygen`, `signing`, `tx`, `verify`), so a test runs real Zafe code. Each command is a
//! step function: run it again when the other member's file arrives. Files are exchanged
//! through `--dir`; round 2 files and `request.json` hold secrets and must travel only
//! end to end encrypted.
//!
//! Dev-only like the rest of this CLI: the seat (FROST share, `sk`) and the nonces sit in
//! plain files under `<home>/interop`. Each nonce file is deleted before the share it
//! covers is written.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, bail, ensure, Context, Result};
use clap::{Args, Subcommand};
use orchard::keys::Scope;
use pczt::Pczt;
use rand::{rngs::OsRng, RngCore};
use reddsa::frost::redpallas::{
    keys::{dkg, KeyPackage, PublicKeyPackage},
    round1::{SigningCommitments, SigningNonces},
    round2::SignatureShare,
    Identifier,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use zafe_core::{
    durable,
    keygen::{
        check_contribution, echo_with_commitments, group_key_bytes, member_identifier,
        KeygenParams, Round1, SkContribution,
    },
    keys::{VaultKeys, VaultSecret},
    node::orchard_receiver,
    signing, tx,
    verify::{verify_pczt, Expectations, Payment, VerifiedTx},
    wallet::{connect, latest_height, ZafeNetwork},
};
use zcash_keys::address::UnifiedAddress;
use zcash_protocol::{
    consensus::{BlockHeight, BranchId},
    memo::{Memo, MemoBytes},
};

/// Written in every message; a reader refuses any other format or kind.
const FORMAT: &str = "frost-zec-interop-v0";
const KIND_ROUND1: &str = "dkg-round1";
const KIND_ROUND2: &str = "dkg-round2";
const KIND_COMMITMENTS: &str = "sign-commitments";
const KIND_REQUEST: &str = "sign-request";
const KIND_REPLY: &str = "sign-reply";

// This member's state, in `<home>/interop`.
const KEYGEN_FILE: &str = "keygen.json";
const SEAT_FILE: &str = "vault.json";
const COSIGN_FILE: &str = "cosign.json";
const COORDINATE_FILE: &str = "coordinate.json";
/// The vault's wallet (`interop propose`): a cache of chain data, rebuilt from the
/// birthday if lost. Dev-only: its key sits in a plain file next to it.
const WALLET_DB_FILE: &str = "wallet.sqlite";
const WALLET_KEY_FILE: &str = "wallet.key";
// Exchanged messages, in `--dir` (round 1 and 2 names carry identifiers).
const COMMITMENTS_MSG: &str = "commitments.json";
const REQUEST_MSG: &str = "request.json";
const REPLY_MSG: &str = "reply.json";

/// Commitments a co-signer offers: one per spend it can sign (frost-zec offers 4).
const OFFERED_COMMITMENTS: usize = 4;

#[derive(Subcommand)]
pub enum InteropCmd {
    /// Key generation, one step per run: writes `round1.<id8>.json`; once the other
    /// members' round 1 files are in `--dir`, writes `round2.<me>-to-<them>.json`; once
    /// theirs to this member are there, finishes and prints the vault address.
    Keygen {
        /// The 16-byte vault id (hex) the members agreed on. Needed on the first run.
        #[arg(long)]
        vault_id: Option<String>,
        #[arg(long, default_value_t = 2)]
        threshold: u16,
        #[arg(long, default_value_t = 2)]
        members: u16,
        #[arg(long, default_value = "exchange")]
        dir: PathBuf,
    },
    /// Show this member's seat (vault id, members, viewing key, address).
    Show,
    /// Co-sign, one step per run: writes `commitments.json`; once `request.json` is in
    /// `--dir`, verifies the PCZT against `--to`/`--amount`/`--memo` and writes
    /// `reply.json` (shares, or the reason it refused).
    Cosign {
        #[arg(long, default_value = "session")]
        dir: PathBuf,
        /// Only show what `request.json` pays (decoded with this vault's keys); signs
        /// nothing and keeps the session. Then sign with the values you agreed on.
        #[arg(long)]
        inspect: bool,
        #[command(flatten)]
        expect: ExpectArgs,
    },
    /// Build and prove a PCZT from this vault's own wallet (synced through
    /// `--lightwalletd`): pays `--to`/`--amount`/`--memo`, change back to the vault. The
    /// input for `coordinate --pczt`.
    Propose {
        /// Where to write the proven PCZT.
        #[arg(long)]
        out: PathBuf,
        /// A height before the vault was first funded (needed the first time only).
        #[arg(long)]
        birthday: Option<u32>,
        #[command(flatten)]
        expect: ExpectArgs,
    },
    /// Coordinate, one step per run: once the co-signer's `commitments.json` is in
    /// `--dir`, verifies `--pczt` and writes `request.json`; once `reply.json` is there,
    /// aggregates and writes the signed PCZT to `--out`.
    Coordinate {
        #[arg(long, default_value = "session")]
        dir: PathBuf,
        /// The proven PCZT to sign (e.g. from `zcash-devtool pczt prove`).
        #[arg(long)]
        pczt: Option<PathBuf>,
        /// Where to write the signed PCZT.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Once signed, extract the transaction (checking proofs and signatures) and send
        /// it through `--lightwalletd`.
        #[arg(long)]
        broadcast: bool,
        #[command(flatten)]
        expect: ExpectArgs,
    },
}

/// What this member expects the transaction to do, given independently of the PCZT.
#[derive(Args)]
pub struct ExpectArgs {
    /// The recipient's unified address.
    #[arg(long)]
    to: Option<String>,
    /// The amount paid, in zatoshis.
    #[arg(long)]
    amount: Option<u64>,
    /// The memo text (none: an empty memo).
    #[arg(long)]
    memo: Option<String>,
    /// The chain tip (default: asked from `--lightwalletd`).
    #[arg(long)]
    tip_height: Option<u32>,
    /// Largest accepted `expiry - tip`: the builder's default is 40 blocks; `interop
    /// propose` rounds expiry up to a multiple of 432 blocks after NU7 (up to ~435 ahead).
    #[arg(long, default_value_t = 450)]
    max_expiry_delta: u32,
}

pub async fn run(
    cmd: InteropCmd,
    state: &Path,
    identity: Result<[u8; 32]>,
    network: &ZafeNetwork,
    lightwalletd: &str,
) -> Result<()> {
    fs::create_dir_all(state)?;
    match cmd {
        InteropCmd::Keygen {
            vault_id,
            threshold,
            members,
            dir,
        } => keygen(
            state, &identity?, network, vault_id, threshold, members, &dir,
        ),
        InteropCmd::Show => {
            print_seat(&Seat::load(state)?);
            Ok(())
        }
        InteropCmd::Cosign {
            dir, inspect: true, ..
        } => inspect(state, network, &dir),
        InteropCmd::Cosign { dir, expect, .. } => {
            cosign(state, network, &dir, &expect, lightwalletd).await
        }
        InteropCmd::Propose {
            out,
            birthday,
            expect,
        } => propose(state, network, &out, birthday, &expect, lightwalletd).await,
        InteropCmd::Coordinate {
            dir,
            pczt,
            out,
            broadcast,
            expect,
        } => {
            coordinate(
                state,
                network,
                &dir,
                pczt,
                out,
                broadcast,
                &expect,
                lightwalletd,
            )
            .await
        }
    }
}

// ---- Messages (field names as frost-zec writes them) ----

#[derive(Clone, Serialize, Deserialize)]
struct Round1Message {
    format: String,
    kind: String,
    network: String,
    vault_id: String,
    min_signers: u16,
    max_signers: u16,
    identity: String,
    identifier: String,
    package: String,
    sk_commitment: String,
}

#[derive(Serialize, Deserialize)]
struct Round2Message {
    format: String,
    kind: String,
    vault_id: String,
    from: String,
    to: String,
    package: String,
    echo: String,
    sk_contribution: String,
}

#[derive(Serialize, Deserialize)]
struct CommitmentsMessage {
    format: String,
    kind: String,
    session: String,
    identifier: String,
    commitments: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct RequestMessage {
    format: String,
    kind: String,
    session: String,
    identifier: String,
    pczt: String,
    commitments: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct ReplyMessage {
    format: String,
    kind: String,
    session: String,
    identifier: String,
    shares: Option<Vec<String>>,
    refused: Option<String>,
}

fn check_kind(format: &str, kind: &str, expected: &str) -> Result<()> {
    ensure!(
        format == FORMAT && kind == expected,
        "expected a `{expected}` message ({FORMAT}), got `{kind}` ({format})"
    );
    Ok(())
}

// ---- Key generation ----

/// A key generation in progress. Written before anything it covers is sent.
#[derive(Serialize, Deserialize)]
struct KeygenState {
    network: String,
    vault_id: String,
    min_signers: u16,
    max_signers: u16,
    identity: String,
    identifier: String,
    round1_secret: String,
    round1_package: String,
    sk_contribution: String,
    /// The other members' round 1, once all of it arrived (it is never replaced).
    others: Option<Vec<Round1Message>>,
}

/// One member's seat in a vault made with `interop keygen` (frost-zec's `vault.json`
/// fields, so the two sides print the same facts).
#[derive(Serialize, Deserialize)]
struct Seat {
    network: String,
    vault_id: String,
    min_signers: u16,
    max_signers: u16,
    identity: String,
    identifier: String,
    members: Vec<String>,
    key_package: String,
    public_key_package: String,
    sk: String,
    ak: String,
    ufvk: String,
    address: String,
}

impl Seat {
    fn load(state: &Path) -> Result<Seat> {
        read_json(&state.join(SEAT_FILE))?
            .ok_or_else(|| anyhow!("no interop vault yet: run `zafe interop keygen`"))
    }

    fn key_package(&self) -> Result<KeyPackage> {
        Ok(KeyPackage::deserialize(&hex::decode(&self.key_package)?)?)
    }

    fn public_key_package(&self) -> Result<PublicKeyPackage> {
        Ok(PublicKeyPackage::deserialize(&hex::decode(
            &self.public_key_package,
        )?)?)
    }

    fn vault_keys(&self) -> Result<VaultKeys> {
        Ok(VaultKeys::derive(
            &VaultSecret::from_bytes(bytes32(&self.sk)?),
            &bytes32(&self.ak)?,
        )?)
    }

    fn id(&self) -> Result<Identifier> {
        identifier(&self.identifier)
    }

    /// The other signer named in a message: another member of this vault.
    fn other_member(&self, hex_id: &str) -> Result<Identifier> {
        let id = identifier(hex_id)?;
        ensure!(
            id != self.id()? && self.members.iter().any(|m| m == hex_id),
            "{hex_id} is not another member of this vault"
        );
        Ok(id)
    }
}

fn keygen(
    state: &Path,
    identity: &[u8; 32],
    network: &ZafeNetwork,
    vault_id: Option<String>,
    threshold: u16,
    members: u16,
    dir: &Path,
) -> Result<()> {
    fs::create_dir_all(dir)?;
    if let Some(seat) = read_json::<Seat>(&state.join(SEAT_FILE))? {
        print_seat(&seat);
        println!("key generation is done");
        return Ok(());
    }
    let mut ks = match read_json::<KeygenState>(&state.join(KEYGEN_FILE))? {
        Some(ks) => {
            if let Some(v) = &vault_id {
                ensure!(
                    v.to_lowercase() == ks.vault_id,
                    "a key generation for vault {} is in progress here",
                    ks.vault_id
                );
            }
            ensure!(
                (ks.min_signers, ks.max_signers) == (threshold, members),
                "the key generation in progress is {}-of-{}",
                ks.min_signers,
                ks.max_signers
            );
            ensure!(
                ks.network == interop_network(network)?,
                "the key generation in progress is on {}",
                ks.network
            );
            ks
        }
        None => start_keygen(state, identity, network, vault_id, threshold, members)?,
    };

    let vault_id: [u8; 16] = hex::decode(&ks.vault_id)?
        .try_into()
        .map_err(|_| anyhow!("bad vault id"))?;
    let params = KeygenParams::new(vault_id, ks.min_signers, ks.max_signers)?;
    let my_id = identifier(&ks.identifier)?;
    let me = short(&my_id);
    let my_identity = bytes32(&ks.identity)?;
    let my_contribution = SkContribution::from_bytes(bytes32(&ks.sk_contribution)?);
    let others_needed = usize::from(ks.max_signers) - 1;

    // Round 1 is public: write it on every run, in case the first copy got lost.
    let round1_name = format!("round1.{me}.json");
    write_json(
        &dir.join(&round1_name),
        &Round1Message {
            format: FORMAT.into(),
            kind: KIND_ROUND1.into(),
            network: ks.network.clone(),
            vault_id: ks.vault_id.clone(),
            min_signers: ks.min_signers,
            max_signers: ks.max_signers,
            identity: ks.identity.clone(),
            identifier: ks.identifier.clone(),
            package: ks.round1_package.clone(),
            sk_commitment: hex::encode(my_contribution.commitment(&vault_id, &my_identity)),
        },
    )?;

    let others = match ks.others.clone() {
        Some(others) => others,
        None => {
            let found = files(dir, |n| n.starts_with("round1.") && n != round1_name)?;
            if found.len() < others_needed {
                println!(
                    "[{me}] round 1: send {} to every other member, then run this again when \
                     theirs ({}/{others_needed} in {}) are there",
                    dir.join(&round1_name).display(),
                    found.len(),
                    dir.display()
                );
                return Ok(());
            }
            ensure!(
                found.len() == others_needed,
                "more round 1 files than members in {}",
                dir.display()
            );
            let mut others = Vec::new();
            for path in &found {
                let m: Round1Message = read_json(path)?.context("file vanished")?;
                check_round1(&m, &ks)?;
                others.push(m);
            }
            ks.others = Some(others.clone());
            write_json(&state.join(KEYGEN_FILE), &ks)?;
            others
        }
    };

    // Part 2 is deterministic, so each run rebuilds the same round 2 from the saved round 1.
    let mut received1 = BTreeMap::new();
    let mut identities = BTreeMap::new();
    let mut commitments =
        BTreeMap::from([(my_id, my_contribution.commitment(&vault_id, &my_identity))]);
    for m in &others {
        let id = identifier(&m.identifier)?;
        ensure!(id != my_id, "a round 1 file claims to be this member's");
        received1.insert(
            id,
            dkg::round1::Package::deserialize(&hex::decode(&m.package)?)?,
        );
        identities.insert(id, bytes32(&m.identity)?);
        commitments.insert(id, bytes32(&m.sk_commitment)?);
    }
    ensure!(
        received1.len() == others_needed,
        "two round 1 files share an identifier"
    );
    let round1 = Round1::restore(
        params,
        my_id,
        &hex::decode(&ks.round1_secret)?,
        &hex::decode(&ks.round1_package)?,
    )?;
    let (round2, outgoing) = round1.advance(received1)?;
    let echo = echo_with_commitments(&round2.echo(), &commitments);

    let mut sent = Vec::new();
    for (to, package) in &outgoing {
        let path = dir.join(format!("round2.{me}-to-{}.json", short(to)));
        write_json(
            &path,
            &Round2Message {
                format: FORMAT.into(),
                kind: KIND_ROUND2.into(),
                vault_id: ks.vault_id.clone(),
                from: ks.identifier.clone(),
                to: hex::encode(to.serialize()),
                package: hex::encode(package.serialize()?),
                echo: hex::encode(echo),
                sk_contribution: ks.sk_contribution.clone(),
            },
        )?;
        sent.push(path);
    }

    let suffix = format!("-to-{me}.json");
    let incoming = files(dir, |n| n.starts_with("round2.") && n.ends_with(&suffix))?;
    if incoming.len() < others_needed {
        for path in &sent {
            println!(
                "[{me}] round 2: send {} only to its recipient, end to end encrypted (it \
                 holds secrets)",
                path.display()
            );
        }
        println!(
            "[{me}] then run this again when the round 2 files to this member ({}/{others_needed}) \
             are in {}",
            incoming.len(),
            dir.display()
        );
        return Ok(());
    }
    ensure!(
        incoming.len() == others_needed,
        "more round 2 files to this member than members in {}",
        dir.display()
    );

    let mut received2 = BTreeMap::new();
    let mut contributions = BTreeMap::from([(my_id, my_contribution.clone())]);
    for path in &incoming {
        let m: Round2Message = read_json(path)?.context("file vanished")?;
        check_kind(&m.format, &m.kind, KIND_ROUND2)?;
        ensure!(
            m.vault_id == ks.vault_id && m.to == ks.identifier,
            "{} is for another vault or member",
            path.display()
        );
        let from = identifier(&m.from)?;
        let their_identity = identities
            .get(&from)
            .ok_or_else(|| anyhow!("round 2 from a member that sent no round 1"))?;
        ensure!(
            m.echo == hex::encode(echo),
            "member {} saw a different round 1: abort",
            short(&from)
        );
        let revealed = SkContribution::from_bytes(bytes32(&m.sk_contribution)?);
        check_contribution(&vault_id, their_identity, &commitments[&from], &revealed)?;
        contributions.insert(from, revealed);
        ensure!(
            received2
                .insert(
                    from,
                    dkg::round2::Package::deserialize(&hex::decode(&m.package)?)?
                )
                .is_none(),
            "two round 2 files from one member"
        );
    }
    let output = round2.finish(received2)?.finish(&contributions)?;

    let ak = group_key_bytes(&output.public_key_package)?;
    let fvk = output.vault_keys.fvk();
    let seat = Seat {
        network: ks.network.clone(),
        vault_id: ks.vault_id.clone(),
        min_signers: ks.min_signers,
        max_signers: ks.max_signers,
        identity: ks.identity.clone(),
        identifier: ks.identifier.clone(),
        members: commitments
            .keys()
            .map(|id| hex::encode(id.serialize()))
            .collect(),
        key_package: hex::encode(output.key_package.serialize()?),
        public_key_package: hex::encode(output.public_key_package.serialize()?),
        sk: hex::encode(output.vault_secret.as_bytes()),
        ak: hex::encode(ak),
        ufvk: output
            .vault_keys
            .ufvk()?
            .encode(network)
            .map_err(|e| anyhow!("UFVK encoding: {e:?}"))?,
        address: UnifiedAddress::from_receivers(
            Some(fvk.address_at(0u32, Scope::External)),
            None,
            None,
            None,
            None,
        )
        .ok_or_else(|| anyhow!("cannot build the unified address"))?
        .encode(network),
    };
    write_json(&state.join(SEAT_FILE), &seat)?;
    durable::remove(&state.join(KEYGEN_FILE))?;
    // The round 2 files to this member are consumed (they hold its share's inputs).
    for path in &incoming {
        let _ = fs::remove_file(path);
    }
    print_seat(&seat);
    println!(
        "Compare the address with every other member before funding it. Delete the round 2 \
         files you sent once they were received."
    );
    Ok(())
}

fn start_keygen(
    state: &Path,
    identity: &[u8; 32],
    network: &ZafeNetwork,
    vault_id: Option<String>,
    threshold: u16,
    members: u16,
) -> Result<KeygenState> {
    let vault_id: [u8; 16] = match vault_id {
        Some(v) => hex::decode(&v)?
            .try_into()
            .map_err(|_| anyhow!("the vault id is 16 bytes (32 hex characters)"))?,
        None => {
            let mut id = [0u8; 16];
            OsRng.fill_bytes(&mut id);
            println!(
                "new vault id {}: give it to the other members",
                hex::encode(id)
            );
            id
        }
    };
    let params = KeygenParams::new(vault_id, threshold, members)?;
    let id = member_identifier(identity, &vault_id)?;
    let round1 = Round1::start(params, id, &mut OsRng)?;
    let contribution = SkContribution::generate(&mut OsRng);
    let ks = KeygenState {
        network: interop_network(network)?.into(),
        vault_id: hex::encode(vault_id),
        min_signers: threshold,
        max_signers: members,
        identity: hex::encode(identity),
        identifier: hex::encode(id.serialize()),
        round1_secret: hex::encode(round1.secret_bytes()?),
        round1_package: hex::encode(round1.package().serialize()?),
        sk_contribution: hex::encode(contribution.as_bytes()),
        others: None,
    };
    write_json(&state.join(KEYGEN_FILE), &ks)?;
    Ok(ks)
}

/// The parameters must be ours, and the identifier must follow from the identity key.
fn check_round1(m: &Round1Message, ks: &KeygenState) -> Result<()> {
    check_kind(&m.format, &m.kind, KIND_ROUND1)?;
    ensure!(
        m.vault_id == ks.vault_id
            && m.network == ks.network
            && m.min_signers == ks.min_signers
            && m.max_signers == ks.max_signers,
        "member {} uses another vault id, network or threshold",
        m.identifier
    );
    let vault_id: [u8; 16] = hex::decode(&ks.vault_id)?
        .try_into()
        .map_err(|_| anyhow!("bad vault id"))?;
    let id = member_identifier(&bytes32(&m.identity)?, &vault_id)?;
    ensure!(
        hex::encode(id.serialize()) == m.identifier,
        "member {}'s identifier does not follow from its identity key",
        m.identifier
    );
    Ok(())
}

fn print_seat(seat: &Seat) {
    println!("network      {}", seat.network);
    println!("vault id     {}", seat.vault_id);
    println!("threshold    {}-of-{}", seat.min_signers, seat.max_signers);
    for m in &seat.members {
        let mine = if *m == seat.identifier {
            "  (this member)"
        } else {
            ""
        };
        println!("member       {}{mine}", &m[..8]);
    }
    println!("ak           {}", seat.ak);
    println!("ufvk         {}", seat.ufvk);
    println!("address      {}", seat.address);
}

// ---- Signing ----

/// A co-signer's open session: its nonces, deleted before any share is written.
#[derive(Serialize, Deserialize)]
struct CosignState {
    session: String,
    nonces: Vec<String>,
    commitments: Vec<String>,
}

/// A coordinator's open session.
#[derive(Serialize, Deserialize)]
struct CoordinateState {
    session: String,
    cosigner: String,
    /// The co-signer's commitment for each spend, in spend order.
    their_commitments: Vec<String>,
    nonces: Vec<String>,
    commitments: Vec<String>,
    pczt: String,
}

/// What a request's PCZT does, read with this vault's keys: spends from the vault,
/// each payment (recovered with the vault's outgoing viewing key), change and fee. Only
/// a display: signing still checks the transaction against the values given on the
/// command line.
fn inspect(state: &Path, network: &ZafeNetwork, dir: &Path) -> Result<()> {
    use orchard::note_encryption::IronwoodDomain;
    use pczt::roles::verifier::Verifier;
    use zcash_note_encryption::{try_note_decryption, try_output_recovery_with_ovk};

    let seat = Seat::load(state)?;
    let path = dir.join(REQUEST_MSG);
    let request: RequestMessage =
        read_json(&path)?.ok_or_else(|| anyhow!("no {} in {}", REQUEST_MSG, dir.display()))?;
    check_kind(&request.format, &request.kind, KIND_REQUEST)?;
    let open: Option<CosignState> = read_json(&state.join(COSIGN_FILE))?;
    let pczt = parse_pczt(&request.pczt)?;
    let keys = seat.vault_keys()?;
    let fvk = keys.fvk();
    let ovk = fvk.to_ovk(Scope::External);
    let global = pczt.global();
    println!(
        "request from member {}, session {} ({})",
        &request.identifier[..8.min(request.identifier.len())],
        request.session,
        match &open {
            Some(o) if o.session == request.session => "this session",
            Some(_) => "NOT this session: it will be refused",
            None => "no session open here",
        }
    );
    println!(
        "  tx version {}, branch id {:#010x}, expiry height {}",
        global.tx_version(),
        global.consensus_branch_id(),
        global.expiry_height()
    );
    if !pczt.transparent().inputs().is_empty()
        || !pczt.transparent().outputs().is_empty()
        || !pczt.sapling().spends().is_empty()
        || !pczt.sapling().outputs().is_empty()
        || !pczt.orchard().actions().is_empty()
    {
        println!("  WARNING: has transparent, Sapling or Orchard-pool parts (signing refuses)");
    }

    let mut lines = Vec::new();
    let (mut spent, mut paid, mut change) = (0u64, 0u64, 0u64);
    Verifier::new(pczt.clone())
        .with_ironwood::<anyhow::Error, _>(|bundle| {
            for action in bundle.actions() {
                let spend = action.spend();
                if spend.fvk().as_ref() == Some(fvk) {
                    spent += spend.value().map(|v| v.inner()).unwrap_or(0);
                }
                let output = action.output();
                let (Some(value), Some(recipient)) = (output.value(), output.recipient()) else {
                    continue;
                };
                let value = value.inner();
                if value == 0 {
                    continue;
                }
                let domain = IronwoodDomain::for_pczt_action(action);
                if let Some(scope) = fvk.scope_for_address(recipient) {
                    let ivk = fvk.to_ivk(scope).prepare();
                    let ok = try_note_decryption(&domain, &ivk, action).is_some();
                    change += value;
                    lines.push(format!(
                        "  change   {} to this vault{}",
                        zec(value),
                        if ok {
                            ""
                        } else {
                            " (WARNING: the vault can't decrypt it)"
                        }
                    ));
                    continue;
                }
                paid += value;
                match try_output_recovery_with_ovk(
                    &domain,
                    &ovk,
                    action,
                    action.cv_net(),
                    &output.encrypted_note().out_ciphertext,
                ) {
                    Some((_, address, memo)) => {
                        let memo = MemoBytes::from_bytes(&memo)
                            .ok()
                            .and_then(|m| Memo::try_from(m).ok());
                        let ua =
                            UnifiedAddress::from_receivers(Some(address), None, None, None, None)
                                .map(|a| a.encode(network))
                                .unwrap_or_else(|| "(unencodable)".into());
                        lines.push(format!("  pay      {} to {ua}", zec(value)));
                        match memo {
                            Some(Memo::Empty) | None => {}
                            Some(Memo::Text(t)) => {
                                lines.push(format!("           memo \"{}\"", &*t))
                            }
                            Some(_) => lines.push("           memo (not text)".into()),
                        }
                    }
                    None => lines.push(format!(
                        "  pay      {} to an address this vault can't recover (signing refuses)",
                        zec(value)
                    )),
                }
            }
            Ok(())
        })
        .map_err(|e| anyhow!("PCZT: {e:?}"))?;
    println!("  spend    {} from this vault", zec(spent));
    for line in lines {
        println!("{line}");
    }
    match spent.checked_sub(paid + change) {
        Some(fee) => println!("  fee      {}", zec(fee)),
        None => println!("  WARNING: outputs exceed the vault's inputs"),
    }
    println!(
        "Nothing signed. If this is what you agreed, sign with: zafe interop cosign --dir {} \
         --to <address> --amount <zatoshis> [--memo <text>]",
        dir.display()
    );
    Ok(())
}

fn zec(zat: u64) -> String {
    format!(
        "{}.{:08} ZEC ({zat} zat)",
        zat / 100_000_000,
        zat % 100_000_000
    )
}

async fn cosign(
    state: &Path,
    network: &ZafeNetwork,
    dir: &Path,
    expect: &ExpectArgs,
    lightwalletd: &str,
) -> Result<()> {
    let seat = Seat::load(state)?;
    ensure!(
        seat.min_signers == 2,
        "interop signing supports a threshold of 2 only"
    );
    fs::create_dir_all(dir)?;
    let key_package = seat.key_package()?;
    let my_id = seat.id()?;
    let state_file = state.join(COSIGN_FILE);

    let request_path = dir.join(REQUEST_MSG);
    let open = match read_json::<CosignState>(&state_file)? {
        Some(open) => open,
        None => {
            // A request left over from an earlier session would be answered with nonces
            // it never saw: start clean.
            for name in [COMMITMENTS_MSG, REQUEST_MSG, REPLY_MSG] {
                let _ = fs::remove_file(dir.join(name));
            }
            let mut session = [0u8; 8];
            OsRng.fill_bytes(&mut session);
            let mut nonces = Vec::new();
            let mut commitments = Vec::new();
            for _ in 0..OFFERED_COMMITMENTS {
                let (n, c) = signing::commit(&key_package, &mut OsRng);
                nonces.push(hex::encode(n.serialize()?));
                commitments.push(hex::encode(c.serialize()?));
            }
            let open = CosignState {
                session: hex::encode(session),
                nonces,
                commitments,
            };
            write_json(&state_file, &open)?;
            open
        }
    };
    if !request_path.exists() {
        write_json(
            &dir.join(COMMITMENTS_MSG),
            &CommitmentsMessage {
                format: FORMAT.into(),
                kind: KIND_COMMITMENTS.into(),
                session: open.session.clone(),
                identifier: seat.identifier.clone(),
                commitments: open.commitments.clone(),
            },
        )?;
        println!(
            "commitments written to {}: send it to the coordinator, then run this again with \
             --to and --amount when its request.json is in {}",
            dir.join(COMMITMENTS_MSG).display(),
            dir.display()
        );
        return Ok(());
    }

    // Check what this member expects before anything is consumed.
    let payment = expected_payment(network, expect)?;
    let request: RequestMessage = read_json(&request_path)?.context("request vanished")?;
    // The request holds the PCZT and its alphas; the nonces are taken once: both go now.
    let _ = fs::remove_file(&request_path);
    durable::remove(&state_file)?;
    let nonces = open
        .nonces
        .iter()
        .map(|n| Ok(SigningNonces::deserialize(&hex::decode(n)?)?))
        .collect::<Result<Vec<_>>>()?;

    let refuse = |reason: String| -> Result<()> {
        write_json(
            &dir.join(REPLY_MSG),
            &ReplyMessage {
                format: FORMAT.into(),
                kind: KIND_REPLY.into(),
                session: open.session.clone(),
                identifier: seat.identifier.clone(),
                shares: None,
                refused: Some(reason.clone()),
            },
        )?;
        bail!("refused: {reason} (reply.json says so)")
    };
    check_kind(&request.format, &request.kind, KIND_REQUEST)?;
    if request.session != open.session {
        return refuse("request from another session".into());
    }
    let coordinator = match seat.other_member(&request.identifier) {
        Ok(id) => id,
        Err(e) => return refuse(e.to_string()),
    };
    let tip = tip_height(expect, lightwalletd).await;
    let verified = match parse_pczt(&request.pczt)
        .and_then(|pczt| check_pczt(&pczt, &seat, network, payment, tip, expect.max_expiry_delta))
    {
        Ok(v) => v,
        Err(e) => return refuse(format!("{e:#}")),
    };
    let spends = &verified.spends_to_sign;
    if spends.is_empty() || spends.len() > nonces.len() || request.commitments.len() != spends.len()
    {
        return refuse(format!(
            "{} spend(s), {} coordinator commitment(s), {} offered",
            spends.len(),
            request.commitments.len(),
            nonces.len()
        ));
    }
    print_verified(&verified);

    let mut shares = Vec::new();
    // Spend i uses commitment i on both sides; the nonces left over are dropped unused.
    for ((i, spend), nonce) in spends.iter().enumerate().zip(nonces) {
        let commitments = BTreeMap::from([
            (coordinator, commitment(&request.commitments[i])?),
            (my_id, commitment(&open.commitments[i])?),
        ]);
        let package = signing::signing_package(commitments, &verified.sighash);
        let share = signing::sign(&package, nonce, &key_package, spend.alpha)?;
        shares.push(hex::encode(share.serialize()));
    }
    write_json(
        &dir.join(REPLY_MSG),
        &ReplyMessage {
            format: FORMAT.into(),
            kind: KIND_REPLY.into(),
            session: open.session,
            identifier: seat.identifier.clone(),
            shares: Some(shares),
            refused: None,
        },
    )?;
    println!(
        "signed {} spend(s) over the sighash computed here; nonces deleted. Send {} to the \
         coordinator.",
        spends.len(),
        dir.join(REPLY_MSG).display()
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn coordinate(
    state: &Path,
    network: &ZafeNetwork,
    dir: &Path,
    pczt_path: Option<PathBuf>,
    out: Option<PathBuf>,
    broadcast: bool,
    expect: &ExpectArgs,
    lightwalletd: &str,
) -> Result<()> {
    let seat = Seat::load(state)?;
    ensure!(
        seat.min_signers == 2,
        "interop signing supports a threshold of 2 only"
    );
    fs::create_dir_all(dir)?;
    let key_package = seat.key_package()?;
    let my_id = seat.id()?;
    let state_file = state.join(COORDINATE_FILE);

    let Some(open) = read_json::<CoordinateState>(&state_file)? else {
        let Some(theirs) = read_json::<CommitmentsMessage>(&dir.join(COMMITMENTS_MSG))? else {
            println!(
                "waiting for the co-signer's {} in {}: run this again when it is there",
                COMMITMENTS_MSG,
                dir.display()
            );
            return Ok(());
        };
        check_kind(&theirs.format, &theirs.kind, KIND_COMMITMENTS)?;
        let cosigner = seat.other_member(&theirs.identifier)?;
        let pczt_path = pczt_path.ok_or_else(|| anyhow!("--pczt is required"))?;
        ensure!(
            out.is_some(),
            "--out is required (where the signed PCZT goes)"
        );
        let payment = expected_payment(network, expect)?;
        let pczt_bytes =
            fs::read(&pczt_path).with_context(|| format!("reading {}", pczt_path.display()))?;
        let pczt = Pczt::parse(&pczt_bytes).map_err(|e| anyhow!("PCZT: {e:?}"))?;
        // The coordinator checks too: whatever built the PCZT is not trusted.
        let verified = check_pczt(
            &pczt,
            &seat,
            network,
            payment,
            tip_height(expect, lightwalletd).await,
            expect.max_expiry_delta,
        )?;
        let spends = verified.spends_to_sign.len();
        ensure!(spends > 0, "no spend of this vault waits for a signature");
        ensure!(
            theirs.commitments.len() >= spends,
            "{spends} spend(s) but only {} commitment(s) from the co-signer",
            theirs.commitments.len()
        );
        print_verified(&verified);
        let mut nonces = Vec::new();
        let mut commitments = Vec::new();
        for _ in 0..spends {
            let (n, c) = signing::commit(&key_package, &mut OsRng);
            nonces.push(hex::encode(n.serialize()?));
            commitments.push(hex::encode(c.serialize()?));
        }
        let open = CoordinateState {
            session: theirs.session,
            cosigner: hex::encode(cosigner.serialize()),
            their_commitments: theirs.commitments[..spends].to_vec(),
            nonces,
            commitments,
            pczt: hex::encode(&pczt_bytes),
        };
        write_json(&state_file, &open)?;
        write_request(dir, &seat, &open)?;
        println!(
            "request written to {}: send it to the co-signer end to end encrypted (it holds \
             the PCZT), then run this again with --out when its reply.json is in {}",
            dir.join(REQUEST_MSG).display(),
            dir.display()
        );
        return Ok(());
    };

    let Some(reply) = read_json::<ReplyMessage>(&dir.join(REPLY_MSG))? else {
        write_request(dir, &seat, &open)?;
        println!(
            "waiting for the co-signer's {} in {}: run this again when it is there",
            REPLY_MSG,
            dir.display()
        );
        return Ok(());
    };
    let out = out.ok_or_else(|| anyhow!("--out is required (where the signed PCZT goes)"))?;
    // The nonces are used once, whatever the reply says.
    durable::remove(&state_file)?;
    for name in [COMMITMENTS_MSG, REQUEST_MSG, REPLY_MSG] {
        let _ = fs::remove_file(dir.join(name));
    }
    check_kind(&reply.format, &reply.kind, KIND_REPLY)?;
    let cosigner = identifier(&open.cosigner)?;
    ensure!(
        reply.session == open.session && seat.other_member(&reply.identifier)? == cosigner,
        "reply from another session or member"
    );
    if let Some(reason) = reply.refused {
        bail!("the co-signer refused: {reason}");
    }
    let their_shares = reply
        .shares
        .ok_or_else(|| anyhow!("the reply has neither shares nor a reason"))?;

    let pczt = parse_pczt(&open.pczt)?;
    let spends = tx::spends_to_sign(&pczt, seat.vault_keys()?.fvk())?;
    let sighash = tx::shielded_sighash(&pczt)?;
    ensure!(
        their_shares.len() == spends.len() && open.nonces.len() == spends.len(),
        "{} share(s) for {} spend(s)",
        their_shares.len(),
        spends.len()
    );
    let public_key_package = seat.public_key_package()?;
    let mut signatures = Vec::new();
    for (i, spend) in spends.iter().enumerate() {
        let commitments = BTreeMap::from([
            (my_id, commitment(&open.commitments[i])?),
            (cosigner, commitment(&open.their_commitments[i])?),
        ]);
        let package = signing::signing_package(commitments, &sighash);
        let nonces = SigningNonces::deserialize(&hex::decode(&open.nonces[i])?)?;
        let mine = signing::sign(&package, nonces, &key_package, spend.alpha)?;
        let shares = BTreeMap::from([
            (my_id, mine),
            (
                cosigner,
                SignatureShare::deserialize(&hex::decode(&their_shares[i])?)?,
            ),
        ]);
        let signature = signing::aggregate(&package, &shares, &public_key_package, spend.alpha)?;
        signatures.push((spend.action_index, signature));
    }
    // Checks each signature against its action's rk and the sighash.
    let signed = tx::apply_signatures(pczt, &signatures)?;
    let bytes = signed
        .clone()
        .serialize()
        .map_err(|e| anyhow!("PCZT encoding: {e:?}"))?;
    fs::write(&out, bytes)?;
    if broadcast {
        let mut client = connect(lightwalletd).await?;
        let sent = zafe_core::node::broadcast(signed, &mut client).await?;
        let mut txid = sent.txid;
        txid.reverse(); // txids display in reversed byte order
        println!("broadcast: txid {}", hex::encode(txid));
    }
    println!(
        "{} spend(s) signed with member {}; each signature checked under its rk. Written to \
         {}.",
        signatures.len(),
        short(&cosigner),
        out.display()
    );
    Ok(())
}

/// Syncs the vault's own wallet, builds the PCZT (ZIP 317 fee, payment outputs
/// recoverable with the vault OVK, change back to the vault) and proves it.
async fn propose(
    state: &Path,
    network: &ZafeNetwork,
    out: &Path,
    birthday: Option<u32>,
    expect: &ExpectArgs,
    lightwalletd: &str,
) -> Result<()> {
    use pczt::roles::prover::Prover;
    use zafe_core::wallet::{PaymentRequest, VaultWallet, WalletKey};

    let seat = Seat::load(state)?;
    let to = expect
        .to
        .clone()
        .ok_or_else(|| anyhow!("--to is required"))?;
    let amount_zat = expect
        .amount
        .ok_or_else(|| anyhow!("--amount is required"))?;
    let memo = expect
        .memo
        .as_deref()
        .map(|m| Ok::<_, anyhow::Error>(Memo::from_bytes(m.as_bytes())?.encode()))
        .transpose()?;
    let mut client = connect(lightwalletd).await?;
    let key_path = state.join(WALLET_KEY_FILE);
    let key = match fs::read(&key_path) {
        Ok(bytes) => WalletKey::from_slice(&bytes)?,
        Err(_) => {
            let key = WalletKey::random();
            durable::write(&key_path, key.as_bytes())?;
            key
        }
    };
    let path = state.join(WALLET_DB_FILE);
    let mut wallet = if VaultWallet::exists(&path, &key, *network) {
        VaultWallet::open(&path, &key, *network)?
    } else {
        let birthday = birthday.ok_or_else(|| {
            anyhow!("--birthday is required the first time: a height before the vault was funded")
        })?;
        VaultWallet::create(
            &path,
            &key,
            *network,
            "interop",
            &seat.vault_keys()?.ufvk()?,
            birthday,
            &mut client,
        )
        .await?
    };
    wallet.sync(&mut client).await?;
    let b = wallet.balance()?;
    println!(
        "vault balance: {} spendable, {} change waiting for confirmations, {} other pending \
         (zat, tip {})",
        b.ironwood_spendable,
        b.ironwood_change_pending,
        b.ironwood_pending,
        wallet.chain_height()?.unwrap_or(0)
    );
    let pczt = wallet.propose(
        &[PaymentRequest {
            address: to,
            amount_zat,
            memo,
        }],
        1,
    )?;
    println!("proving...");
    let proved = tokio::task::spawn_blocking(move || {
        Prover::new(pczt)
            .create_ironwood_proof(zafe_core::zrng::zrng(), zafe_core::node::proving_key())
            .map(|p| p.finish())
            .map_err(|e| anyhow!("proving: {e:?}"))
    })
    .await??;
    let bytes = proved
        .serialize()
        .map_err(|e| anyhow!("PCZT encoding: {e:?}"))?;
    fs::write(out, bytes)?;
    println!(
        "proven PCZT written to {}: pass it to `zafe interop coordinate --pczt`",
        out.display()
    );
    Ok(())
}

fn write_request(dir: &Path, seat: &Seat, open: &CoordinateState) -> Result<()> {
    write_json(
        &dir.join(REQUEST_MSG),
        &RequestMessage {
            format: FORMAT.into(),
            kind: KIND_REQUEST.into(),
            session: open.session.clone(),
            identifier: seat.identifier.clone(),
            pczt: open.pczt.clone(),
            commitments: open.commitments.clone(),
        },
    )
}

/// The one payment this member expects, from the command line (never from the PCZT).
fn expected_payment(network: &ZafeNetwork, expect: &ExpectArgs) -> Result<Payment> {
    let to = expect
        .to
        .as_deref()
        .ok_or_else(|| anyhow!("--to is required: the recipient you expect"))?;
    let amount_zat = expect
        .amount
        .ok_or_else(|| anyhow!("--amount is required: the zatoshis you expect"))?;
    let memo = match &expect.memo {
        Some(text) => Memo::from_bytes(text.as_bytes())?.encode(),
        None => MemoBytes::empty(),
    };
    Ok(Payment {
        recipient: orchard_receiver(network, to)?,
        amount_zat,
        memo: *memo.as_array(),
    })
}

/// The chain tip: `--tip-height`, or the lightwalletd's.
async fn tip_height(expect: &ExpectArgs, lightwalletd: &str) -> Result<u32> {
    if let Some(tip) = expect.tip_height {
        return Ok(tip);
    }
    let mut client = connect(lightwalletd).await?;
    Ok(latest_height(&mut client).await?)
}

/// zafe-core's member verification (spec §9.3): v6, branch id, expiry, Ironwood only,
/// every spend a vault note, the payment matched exactly, change the vault's, ZIP 317 fee,
/// and the sighash computed here.
fn check_pczt(
    pczt: &Pczt,
    seat: &Seat,
    network: &ZafeNetwork,
    payment: Payment,
    tip: Result<u32>,
    max_expiry_delta: u32,
) -> Result<VerifiedTx> {
    let tip = tip?;
    let expectations = Expectations {
        payments: vec![payment],
        consensus_branch_id: BranchId::for_height(network, BlockHeight::from_u32(tip + 1)).into(),
        tip_height: tip,
        max_expiry_delta,
    };
    Ok(verify_pczt(pczt, seat.vault_keys()?.fvk(), &expectations)?)
}

fn print_verified(v: &VerifiedTx) {
    println!("PCZT verified by zafe-core:");
    println!("  spends from the vault  {} zat", v.input_total_zat);
    for p in &v.payments {
        println!("  payment                {} zat", p.amount_zat);
    }
    println!("  change to the vault    {} zat", v.change_total_zat);
    println!("  fee (ZIP 317)          {} zat", v.fee_zat);
    println!("  expiry height          {}", v.expiry_height);
    println!("  sighash                {}", hex::encode(v.sighash));
    println!("  spends to sign         {}", v.spends_to_sign.len());
}

// ---- Helpers ----

/// `main` or `test`: the interop format has no regtest.
fn interop_network(network: &ZafeNetwork) -> Result<&'static str> {
    match network.name() {
        n @ ("main" | "test") => Ok(n),
        other => bail!("interop runs on main or test, not {other} (pass --network)"),
    }
}

fn parse_pczt(hex_pczt: &str) -> Result<Pczt> {
    Pczt::parse(&hex::decode(hex_pczt)?).map_err(|e| anyhow!("PCZT: {e:?}"))
}

fn commitment(hex_c: &str) -> Result<SigningCommitments> {
    Ok(SigningCommitments::deserialize(&hex::decode(hex_c)?)?)
}

fn identifier(hex_id: &str) -> Result<Identifier> {
    Ok(Identifier::deserialize(&hex::decode(hex_id)?)?)
}

/// The first 4 bytes of an identifier, in hex (file names).
fn short(id: &Identifier) -> String {
    hex::encode(&id.serialize()[..4])
}

fn bytes32(text: &str) -> Result<[u8; 32]> {
    hex::decode(text)?
        .try_into()
        .map_err(|_| anyhow!("expected 32 bytes"))
}

/// `*.json` files in `dir` whose names `keep` accepts, sorted.
fn files(dir: &Path, keep: impl Fn(&str) -> bool) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    for entry in fs::read_dir(dir)? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if name.ends_with(".json") && keep(&name) {
            found.push(dir.join(name));
        }
    }
    found.sort();
    Ok(found)
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes).with_context(|| {
            format!("{} is not a valid message", path.display())
        })?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

/// Atomic (temp + fsync + rename), so the other side never reads half a file.
fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    durable::write(path, &serde_json::to_vec_pretty(value)?)
        .with_context(|| format!("writing {}", path.display()))
}
