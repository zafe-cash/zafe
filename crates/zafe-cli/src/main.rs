//! `zafe`: a headless Zafe member (spec §15, milestone M0).
//!
//! State lives in `--home` as plain files. That is for development only: identity seeds,
//! the FROST share, the vault secret and nonces belong in platform secure storage on real
//! devices (spec §14).

use std::{fs, path::PathBuf, time::Duration};

use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, Subcommand};
use rand::rngs::OsRng;
use zafe_core::{
    node::{self, Invite, VaultMaterial},
    nonce_store::{FileNonceStore, FilePoolStore},
    relay_client::RelayClient,
    repair::{self, RecoveryRequest, RecoveryStatus},
    session::ProposalId,
    state_dir,
    wallet::{connect, latest_height, PaymentRequest, VaultWallet, WalletKey, ZafeNetwork},
};

// Files in a member's home (dev-only plain files). Signing state uses `state_dir` names.
const IDENTITY_FILE: &str = "identity.bin";
const INVITE_FILE: &str = "invite.txt";
const VAULT_FILE: &str = "vault.bin";
const WALLET_DB_FILE: &str = "wallet.sqlite";
const WALLET_KEY_FILE: &str = "wallet.key";
/// The leader's signing requests and used commitments.
const REQUESTS_DIR: &str = "requests";
use zafe_proto::{Identity, IdentitySeeds};
use zcash_protocol::memo::Memo;

#[derive(Parser)]
#[command(
    name = "zafe",
    about = "Headless Zafe multisig member (development only)"
)]
struct Cli {
    /// Directory holding this member's state.
    #[arg(long, env = "ZAFE_HOME", default_value = "./zafe-home")]
    home: PathBuf,
    #[arg(long, env = "ZAFE_RELAY", default_value = "http://127.0.0.1:8787")]
    relay: String,
    #[arg(
        long,
        env = "ZAFE_LIGHTWALLETD",
        default_value = "http://127.0.0.1:9067"
    )]
    lightwalletd: String,
    /// Zcash network: regtest (local, default), test or main. Read from `ZAFE_NETWORK`.
    #[arg(long, env = "ZAFE_NETWORK", default_value = "regtest")]
    network: String,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create this member's identity.
    Init,
    /// Vault setup.
    #[command(subcommand)]
    Vault(VaultCmd),
    /// Sync the vault wallet and print the balance.
    Sync,
    /// Propose a payment.
    Propose {
        #[arg(long)]
        to: String,
        /// Amount in zatoshis.
        #[arg(long)]
        amount: u64,
        #[arg(long)]
        memo: Option<String>,
        /// Send as soon as the approvals complete (one-tap vaults).
        #[arg(long)]
        auto_send: bool,
    },
    /// Publish fresh commitments so proposals can be signed at approval time (one tap).
    Pool,
    /// Send a proposal whose approvals already carry every signature (one tap).
    Send { proposal: String },
    /// Export this member's seat as an encrypted backup (text form, as the app's "Copy as
    /// text"). Never includes signing nonces.
    Backup {
        #[arg(long)]
        passphrase: String,
    },
    /// List proposals.
    Proposals,
    /// Set this member's display name for the other members (empty clears it).
    Name { name: String },
    /// Verify a proposal independently and approve it. Never sends (scripts decide when):
    /// once approvals complete, run `zafe send` (one tap) or `request`, `respond` and
    /// `finalize` (interactive). The app sends by itself.
    Approve { proposal: String },
    /// Reject a proposal.
    Reject { proposal: String },
    /// Leader: send signing requests for an approved proposal.
    Request { proposal: String },
    /// Answer pending signing requests.
    Respond,
    /// Leader: aggregate shares, prove, broadcast.
    Finalize { proposal: String },
    /// Moving a signer's seat to a new device (a lost phone; spec §10.1).
    #[command(subcommand)]
    Seat(SeatCmd),
    /// On a new device with no backup: print this device's recovery code for a co-signer,
    /// and (with `--wait`) wait until the seat has moved and the key is repaired.
    Recover {
        #[arg(long)]
        wait: bool,
        #[arg(long, default_value_t = 300)]
        timeout_secs: u64,
    },
}

#[derive(Subcommand)]
enum SeatCmd {
    /// Approve moving a signer's seat to the device that printed `code`.
    Approve { old: String, code: String },
    /// Seat moves waiting for approvals.
    List,
    /// Do this member's part in repairing a moved seat's key (and save the new membership).
    Repair,
    /// Take over from a helper who stalled the repair of the seat moved at `replacement`.
    Retry { replacement: u64, stalled: String },
}

#[derive(Subcommand)]
enum VaultCmd {
    /// Create a vault and print the invite.
    Create {
        #[arg(long)]
        name: String,
        #[arg(long)]
        threshold: u16,
        #[arg(long)]
        members: u16,
    },
    /// Join a vault from an invite string.
    Join { invite: String },
    /// Show members, whether membership is sealed, and the safety number.
    Members,
    /// Creator: freeze membership once everyone has joined.
    Seal,
    /// Run key generation. Requires the safety number you compared with the others.
    Keygen {
        #[arg(long)]
        safety_number: String,
        #[arg(long, default_value_t = 300)]
        timeout_secs: u64,
        /// Creator only: vault birthday height (default: lightwalletd tip + 1).
        #[arg(long)]
        birthday: Option<u32>,
        /// Creator only: how many days a proposal stays approvable (1 to 30, default 7).
        #[arg(long)]
        expiry_days: Option<u32>,
    },
    /// Show the vault address and details.
    Show,
}

struct Home(PathBuf);

impl Home {
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn identity(&self) -> Result<Identity> {
        let bytes = fs::read(self.path(IDENTITY_FILE)).context("no identity; run `zafe init`")?;
        Ok(Identity::from_seeds(IdentitySeeds::from_bytes(&bytes)?))
    }

    fn invite(&self) -> Result<Invite> {
        let s = fs::read_to_string(self.path(INVITE_FILE))
            .context("no invite; create or join a vault")?;
        Ok(Invite::decode(&s)?)
    }

    fn material(&self) -> Result<VaultMaterial> {
        let bytes = fs::read(self.path(VAULT_FILE))
            .context("vault not created yet; run `zafe vault keygen`")?;
        Ok(VaultMaterial::from_bytes(&bytes)?)
    }
}

/// Nonces on disk, one file per (proposal, PCZT hash). `take` deletes before returning.
/// Set once from `--network` / `ZAFE_NETWORK` at startup.
static NETWORK: std::sync::OnceLock<ZafeNetwork> = std::sync::OnceLock::new();

/// The network this run uses (regtest unless `--network` says otherwise).
fn network() -> ZafeNetwork {
    NETWORK.get().cloned().expect("network is set at startup")
}

fn parse_proposal(s: &str) -> Result<ProposalId> {
    hex::decode(s)?
        .try_into()
        .map_err(|_| anyhow!("proposal id must be 16 bytes hex"))
}

async fn open_wallet(
    home: &Home,
    material: &VaultMaterial,
    lwd: &str,
) -> Result<VaultWallet<ZafeNetwork>> {
    let mut client = connect(lwd).await?;
    let path = home.path(WALLET_DB_FILE);
    let ufvk = material.vault_keys()?.ufvk()?;
    // Dev-only: the wallet key sits in a plain file next to the database.
    let key_path = home.path(WALLET_KEY_FILE);
    let key = match fs::read(&key_path) {
        Ok(bytes) => WalletKey::from_slice(&bytes)?,
        Err(_) => {
            let key = WalletKey::random();
            fs::write(&key_path, key.as_bytes())?;
            key
        }
    };
    let mut wallet = if VaultWallet::exists(&path, &key, network()) {
        VaultWallet::open(&path, &key, network())?
    } else {
        // Missing, or not readable with this key (e.g. a plain database from an older
        // build): the wallet is a cache of chain data, so start over from the birthday.
        let _ = fs::remove_file(&path);
        VaultWallet::create(
            &path,
            &key,
            network(),
            &material.descriptor.name,
            &ufvk,
            material.descriptor.birthday_height,
            &mut client,
        )
        .await?
    };
    wallet.sync(&mut client).await?;
    Ok(wallet)
}

async fn tip(home: &Home, material: &VaultMaterial, lwd: &str) -> Result<u32> {
    open_wallet(home, material, lwd)
        .await?
        .chain_height()?
        .ok_or_else(|| anyhow!("wallet not synced"))
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let net = ZafeNetwork::from_name(&cli.network)
        .ok_or_else(|| anyhow::anyhow!("--network must be regtest, test or main"))?;
    let _ = NETWORK.set(net);
    let home = Home(cli.home.clone());
    fs::create_dir_all(&home.0)?;
    let relay = RelayClient::new(&cli.relay);
    let mut rng = OsRng;

    match cli.command {
        Command::Init => {
            if home.path(IDENTITY_FILE).exists() {
                bail!("identity already exists in {}", home.0.display());
            }
            let id = Identity::generate(&mut rng);
            fs::write(home.path(IDENTITY_FILE), id.seeds().to_bytes())?;
            println!("identity {}", hex::encode(id.public().sig_pk));
        }
        Command::Vault(cmd) => vault(cmd, &home, &relay, &cli.lightwalletd, &mut rng).await?,
        Command::Sync => {
            let material = home.material()?;
            let wallet = open_wallet(&home, &material, &cli.lightwalletd).await?;
            let b = wallet.balance()?;
            println!(
                "height {} ironwood_spendable {} ironwood_total {} total {}",
                wallet.chain_height()?.unwrap_or(0),
                b.ironwood_spendable,
                b.ironwood_total,
                b.total
            );
        }
        Command::Propose {
            to,
            amount,
            memo,
            auto_send,
        } => {
            let material = home.material()?;
            let mut wallet = open_wallet(&home, &material, &cli.lightwalletd).await?;
            let memo = memo
                .map(|m| Memo::from_bytes(m.as_bytes()).map(|m| m.encode()))
                .transpose()?;
            let payments = [PaymentRequest {
                address: to,
                amount_zat: amount,
                memo,
            }];
            let id = node::propose(
                &relay,
                &home.identity()?,
                &material,
                &mut wallet,
                &mut connect(&cli.lightwalletd).await?,
                &node::SentTxs::in_dir(home.path(state_dir::SENT)),
                &payments,
                auto_send,
                &mut rng,
            )
            .await?;
            println!("proposal {}", hex::encode(id));
        }
        Command::Proposals => {
            let material = home.material()?;
            let (_, state) = node::load_state(&relay, &home.identity()?, &material).await?;
            for p in state.proposals.values() {
                let total: u64 = p.payments.iter().map(|x| x.amount_zat).sum();
                println!(
                    "{} {:?} {} zat to {} payee(s), approvals {}/{}, rejections {}{}",
                    hex::encode(p.id),
                    p.status,
                    total,
                    p.payments.len(),
                    p.approvals.len(),
                    material.descriptor.threshold,
                    p.rejections.len(),
                    p.txid
                        .map(|t| format!(", txid {}", hex_txid(&t)))
                        .unwrap_or_default()
                );
            }
        }
        Command::Approve { proposal } => {
            let material = home.material()?;
            let tip = tip(&home, &material, &cli.lightwalletd).await?;
            let mut store = FileNonceStore::new(home.path(state_dir::NONCES));
            let mut pool = FilePoolStore::new(home.path(state_dir::POOL));
            let approved = node::approve(
                &relay,
                &home.identity()?,
                &material,
                &network(),
                tip,
                parse_proposal(&proposal)?,
                &mut store,
                &mut pool,
                &mut rng,
            )
            .await?;
            let verified = &approved.verified;
            println!(
                "verified and approved{}: {} payment(s), fee {} zat, change {} zat, {} spend(s) to sign",
                if approved.signed { " and signed" } else { "" },
                verified.payments.len(),
                verified.fee_zat,
                verified.change_total_zat,
                verified.spends_to_sign.len()
            );
            if approved.completed {
                println!("signatures complete: run `zafe send {proposal}` to broadcast");
            }
        }
        Command::Backup { passphrase } => {
            let contents = zafe_core::backup::Contents {
                identity_seeds: home.identity()?.seeds().to_bytes(),
                material: fs::read(home.path(VAULT_FILE))
                    .context("vault not created yet; run `zafe vault keygen`")?,
                invite: fs::read_to_string(home.path(INVITE_FILE))?,
                created_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_secs(),
                names: Default::default(),
            };
            let bytes = zafe_core::backup::encrypt(
                &contents,
                &passphrase,
                zafe_core::backup::KdfParams::DEFAULT,
                &mut rng,
            )?;
            zafe_core::backup::decrypt(&bytes, &passphrase)?.validate()?;
            println!("{}", zafe_core::backup::to_text(&bytes));
            let at = contents.created_at;
            node::attest_backup(&relay, &home.identity()?, &home.material()?, at, &mut rng).await?;
            eprintln!("backup attested in the vault log");
        }
        Command::Name { name } => {
            let material = home.material()?;
            node::set_name(&relay, &home.identity()?, &material, &name, &mut rng).await?;
            println!("name set");
        }
        Command::Pool => {
            let material = home.material()?;
            let mut pool = FilePoolStore::new(home.path(state_dir::POOL));
            let n = node::top_up_pool(&relay, &home.identity()?, &material, &mut pool, &mut rng)
                .await?;
            println!("published {n} commitment(s)");
        }
        Command::Send { proposal } => {
            let material = home.material()?;
            let tip = tip(&home, &material, &cli.lightwalletd).await?;
            let mut client = connect(&cli.lightwalletd).await?;
            let sent = node::send_ready(
                &relay,
                &home.identity()?,
                &material,
                &network(),
                tip,
                parse_proposal(&proposal)?,
                &mut client,
                &mut rng,
            )
            .await?;
            node::SentTxs::in_dir(home.path(state_dir::SENT)).put(&sent);
            println!("broadcast txid {}", hex_txid(&sent.txid));
        }
        Command::Reject { proposal } => {
            node::reject(
                &relay,
                &home.identity()?,
                &home.material()?,
                parse_proposal(&proposal)?,
                &mut rng,
            )
            .await?;
            println!("rejected");
        }
        Command::Request { proposal } => {
            let material = home.material()?;
            let tip = tip(&home, &material, &cli.lightwalletd).await?;
            let id = parse_proposal(&proposal)?;
            // Commitment sets already put in a request must never be reused.
            let used_path = home.path(REQUESTS_DIR).join(state_dir::USED_COMMITMENTS);
            let mut used = match fs::read(&used_path) {
                Ok(bytes) => node::decode_used_commitments(&bytes)?,
                Err(_) => Default::default(),
            };
            let sent = node::request_signatures(
                &relay,
                &home.identity()?,
                &material,
                &network(),
                tip,
                id,
                &used,
                &mut rng,
            )
            .await?;
            used.extend(sent.used_commitments);
            fs::create_dir_all(home.path(REQUESTS_DIR))?;
            fs::write(&used_path, node::encode_used_commitments(&used)?)?;
            fs::write(
                home.path(&format!("requests/{proposal}.bin")),
                node::encode_request(&sent.request)?,
            )?;
            // If this member is also a signer, it signs its own part now (never via relay).
            let mut store = FileNonceStore::new(home.path(state_dir::NONCES));
            if let Some(own) = node::sign_own_shares(
                &relay,
                &home.identity()?,
                &material,
                &network(),
                tip,
                &sent.request,
                &mut store,
            )
            .await?
            {
                fs::write(
                    home.path(&format!("requests/{proposal}.own")),
                    node::encode_own_shares(&own)?,
                )?;
            }
            println!(
                "signing requests sent to {} member(s)",
                sent.request.signers.len()
            );
        }
        Command::Seat(cmd) => {
            let me = home.identity()?;
            let material = home.material()?;
            match cmd {
                SeatCmd::Approve { old, code } => {
                    let old: [u8; 32] = hex::decode(old.trim())?
                        .try_into()
                        .map_err(|_| anyhow!("old must be a 32-byte hex key"))?;
                    let request = RecoveryRequest::decode(&code)?;
                    let moved = repair::approve_replacement(
                        &relay, &me, &material, old, &request, &mut rng,
                    )
                    .await?;
                    println!(
                        "approved (safety code {}){}",
                        request.safety_code(),
                        if moved { "; the seat moved" } else { "" }
                    );
                }
                SeatCmd::List => {
                    let (_, state) = node::load_state(&relay, &me, &material).await?;
                    for p in state.pending_replacements.values() {
                        println!(
                            "{} -> {}  safety {}  approvals {}/{}",
                            hex::encode(p.old),
                            hex::encode(p.new.sig_pk),
                            RecoveryRequest { identity: p.new }.safety_code(),
                            p.approvals.len(),
                            state.descriptor.threshold
                        );
                    }
                    for r in state.replacements.iter().filter(|r| !r.done) {
                        let helpers: Vec<String> = r.helpers.iter().map(hex::encode).collect();
                        println!(
                            "repairing {} (move {}, attempt {}): helpers {}",
                            hex::encode(r.new.sig_pk),
                            r.index,
                            r.attempt,
                            helpers.join(", ")
                        );
                    }
                }
                SeatCmd::Retry {
                    replacement,
                    stalled,
                } => {
                    let stalled: [u8; 32] = hex::decode(stalled.trim())?
                        .try_into()
                        .map_err(|_| anyhow!("stalled must be a 32-byte hex key"))?;
                    repair::retry_repair(&relay, &me, &material, replacement, stalled, &mut rng)
                        .await?;
                    println!("retrying with you as a helper");
                }
                SeatCmd::Repair => {
                    let (_, state) = node::load_state(&relay, &me, &material).await?;
                    let report = repair::help_repairs(
                        &relay,
                        &me,
                        &material,
                        &state,
                        &home.path(state_dir::REPAIR),
                        &mut rng,
                    )
                    .await?;
                    if let Some(updated) = repair::current_material(&material, &state) {
                        fs::write(home.path(VAULT_FILE), updated.to_bytes()?)?;
                    }
                    println!("{report:?}");
                }
            }
        }
        Command::Recover { wait, timeout_secs } => {
            if home.path(VAULT_FILE).exists() {
                bail!("this home already holds a vault");
            }
            if !home.path(IDENTITY_FILE).exists() {
                let id = Identity::generate(&mut rng);
                fs::write(home.path(IDENTITY_FILE), id.seeds().to_bytes())?;
            }
            let me = home.identity()?;
            let request = RecoveryRequest {
                identity: *me.public(),
            };
            println!("{}", request.encode());
            eprintln!("safety code {}", request.safety_code());
            let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);
            loop {
                match repair::try_recover(&relay, &me).await? {
                    RecoveryStatus::Done { material, invite } => {
                        fs::write(home.path(VAULT_FILE), material.to_bytes()?)?;
                        fs::write(home.path(INVITE_FILE), invite.encode())?;
                        repair::mark_repair_done(&relay, &me, &material, &mut rng).await?;
                        println!("recovered {}", material.descriptor.name);
                        break;
                    }
                    status if !wait || std::time::Instant::now() > deadline => {
                        eprintln!("{status:?}");
                        if wait {
                            bail!("timed out");
                        }
                        break;
                    }
                    _ => tokio::time::sleep(Duration::from_secs(2)).await,
                }
            }
        }
        Command::Respond => {
            let material = home.material()?;
            let tip = tip(&home, &material, &cli.lightwalletd).await?;
            let mut store = FileNonceStore::new(home.path(state_dir::NONCES));
            let report = node::respond(
                &relay,
                &home.identity()?,
                &material,
                &network(),
                tip,
                &mut store,
                &mut rng,
            )
            .await?;
            for (proposal, reason) in &report.skipped {
                eprintln!("skipped request for {}: {reason}", hex::encode(proposal));
            }
            println!("answered {} signing request(s)", report.answered.len());
        }
        Command::Finalize { proposal } => {
            let material = home.material()?;
            let request = node::decode_request(
                &fs::read(home.path(&format!("requests/{proposal}.bin")))
                    .context("run `zafe request` first")?,
            )?;
            let own: Option<Vec<Vec<u8>>> =
                fs::read(home.path(&format!("requests/{proposal}.own")))
                    .ok()
                    .map(|b| node::decode_own_shares(&b))
                    .transpose()?;
            let mut client = connect(&cli.lightwalletd).await?;
            let sent = node::finalize(
                &relay,
                &home.identity()?,
                &material,
                &request,
                own.as_deref(),
                &mut client,
                Duration::from_secs(120),
                |p| eprintln!("shares {}/{}", p.received, p.needed),
                &mut rng,
            )
            .await?;
            node::SentTxs::in_dir(home.path(state_dir::SENT)).put(&sent);
            println!("broadcast txid {}", hex_txid(&sent.txid));
        }
    }
    Ok(())
}

async fn vault(
    cmd: VaultCmd,
    home: &Home,
    relay: &RelayClient,
    lwd: &str,
    rng: &mut OsRng,
) -> Result<()> {
    match cmd {
        VaultCmd::Create {
            name,
            threshold,
            members,
        } => {
            let invite =
                node::create_vault(relay, &home.identity()?, &name, threshold, members, rng)
                    .await?;
            fs::write(home.path(INVITE_FILE), invite.encode())?;
            println!("{}", invite.encode());
        }
        VaultCmd::Join { invite } => {
            let parsed = Invite::decode(&invite)?;
            node::join_vault(relay, &home.identity()?, &parsed).await?;
            fs::write(home.path(INVITE_FILE), parsed.encode())?;
            println!("joined vault {}", parsed.name);
        }
        VaultCmd::Members => {
            let (members, sealed, number) =
                node::membership(relay, &home.identity()?, &home.invite()?).await?;
            for m in &members {
                println!("member {}", hex::encode(m.sig_pk));
            }
            println!("sealed {sealed}");
            println!("safety number: {number}");
        }
        VaultCmd::Seal => {
            node::seal(relay, &home.identity()?, &home.invite()?).await?;
            println!("membership sealed");
        }
        VaultCmd::Keygen {
            safety_number,
            timeout_secs,
            birthday,
            expiry_days,
        } => {
            let invite = home.invite()?;
            let me = home.identity()?;
            let birthday = match (me.public().sig_pk == invite.creator, birthday) {
                (true, Some(height)) => Some(height.max(2)),
                (true, None) => Some((latest_height(&mut connect(lwd).await?).await? + 1).max(2)),
                (false, _) => None,
            };
            let material = node::run_keygen(
                relay,
                &me,
                &invite,
                &safety_number,
                &network(),
                network().name(),
                birthday,
                expiry_days.map(|d| d * 1152),
                rng,
                Duration::from_secs(timeout_secs),
            )
            .await?;
            fs::write(home.path(VAULT_FILE), material.to_bytes()?)?;
            println!("vault created: {}", material.descriptor.address);
            // Like the app: publish the one-tap commitment pool while every member is
            // present, so the first payment can be one tap.
            let mut pool = FilePoolStore::new(home.path(state_dir::POOL));
            let n = node::top_up_pool(relay, &me, &material, &mut pool, rng).await?;
            println!("published {n} commitment(s)");
        }
        VaultCmd::Show => {
            let m = home.material()?;
            println!("name {}", m.descriptor.name);
            println!(
                "threshold {} of {}",
                m.descriptor.threshold,
                m.descriptor.members.len()
            );
            println!("address {}", m.descriptor.address);
            println!("ufvk {}", m.descriptor.ufvk);
            println!("birthday {}", m.descriptor.birthday_height);
        }
    }
    Ok(())
}

fn hex_txid(txid: &[u8; 32]) -> String {
    let mut display = *txid;
    display.reverse(); // txids display in reversed byte order
    hex::encode(display)
}
