//! Moving a lost member's seat to a new phone (spec §10.1): the new phone's recovery code,
//! co-signers' approvals, and the new phone collecting its repaired key.
//!
//! The helpers' side of the share repair runs inside `list_proposals` (every refresh).

use rand::rngs::OsRng;
use zafe_core::{
    relay_client::RelayClient,
    repair::{self, RecoveryRequest, RecoveryStatus, SeatLink},
};

use super::{
    error::{ZafeError, ZafeErrorKind},
    vault::{identity, material, runtime, vault_relay},
};

type Result<T, E = ZafeError> = std::result::Result<T, E>;

pub struct RecoveryCode {
    /// `zafe-recover-v1:...`: sent to a co-signer (text, link or QR code).
    pub code: String,
}

/// The recovery code of a new phone's identity (`seeds`, a fresh identity kept until the
/// seat has moved).
#[flutter_rust_bridge::frb(sync)]
pub fn recovery_code(seeds: Vec<u8>) -> Result<RecoveryCode> {
    let request = RecoveryRequest {
        identity: *identity(&seeds)?.public(),
    };
    Ok(RecoveryCode {
        code: request.encode(),
    })
}

pub struct RecoveryCodeInfo {
    /// The new phone's signing key (hex).
    pub key_hex: String,
}

/// Parses a recovery code a co-signer pasted or scanned (also inside a message or a
/// `zafe://recover?code=` link).
#[flutter_rust_bridge::frb(sync)]
pub fn parse_recovery_code(code: String) -> Result<RecoveryCodeInfo> {
    let request = decode(&code)?;
    Ok(RecoveryCodeInfo {
        key_hex: hex::encode(request.identity.sig_pk),
    })
}

/// Co-signer: one step of the safety check with the phone that showed `code` (poll it
/// while the screen is open). `None` until the new phone has taken part; then the four
/// words to compare with it on a call before approving. Runs on the vault's relay (from
/// `material`; `relay_url` for a vault from before per-vault relays), where the new phone
/// takes part after opening the co-signer's [`seat_link`].
pub fn new_phone_words(
    relay_url: String,
    seeds: Vec<u8>,
    material: Option<Vec<u8>>,
    code: String,
) -> Result<Option<String>> {
    let me = identity(&seeds)?;
    let request = decode(&code)?;
    let relay = match material {
        Some(bytes) => vault_relay(&relay_url, &self::material(&bytes)?),
        None => RelayClient::pooled(relay_url),
    };
    let mut watch = crate::diag::Stopwatch::new("seat-words");
    let words = runtime().block_on(repair::new_phone_words(&relay, &me, &request.identity));
    watch.lap("relay");
    watch.finish();
    Ok(words?)
}

fn decode(text: &str) -> Result<RecoveryRequest> {
    let code = text
        .split(|c: char| c.is_whitespace() || c == '=' || c == '&')
        .find(|w| w.starts_with("zafe-recover-v"))
        .unwrap_or(text);
    RecoveryRequest::decode(code).map_err(|e| match e {
        zafe_core::node::NodeError::UnsupportedVersion(_) => e.into(),
        _ => ZafeError::invalid("That isn't a Zafe recovery code"),
    })
}

/// Approves moving the seat of `old_key_hex` (a signer who lost their phone) to the phone
/// that showed `code`. Returns whether this was the last approval needed: the move is then
/// pending (the old phone is asked first; the helpers' parts complete it after their 72 h).
pub fn approve_seat_move(
    relay_url: String,
    seeds: Vec<u8>,
    material: Vec<u8>,
    old_key_hex: String,
    code: String,
    words: String,
) -> Result<bool> {
    let me = identity(&seeds)?;
    let m = self::material(&material)?;
    let request = decode(&code)?;
    let old: [u8; 32] = hex::decode(old_key_hex.trim())
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or_else(|| ZafeError::invalid("not a signer key"))?;
    // Whether `old` is a member is checked against the log (seats may have moved).
    if old == me.public().sig_pk {
        return Err(ZafeError::invalid("You can't approve moving your own seat"));
    }
    let mut watch = crate::diag::Stopwatch::new("seat-approve");
    let moved = runtime().block_on(repair::approve_replacement(
        &vault_relay(&relay_url, &m),
        &me,
        &m,
        old,
        &request,
        &words,
        &mut OsRng,
    ));
    watch.lap("relay");
    watch.finish();
    Ok(moved?)
}

/// The old phone: its owner still has it and keeps the seat, voiding the pending move of it
/// (only a first-round move; behind unlock, never automatic). Returns whether a move was
/// voided now.
pub fn keep_seat(relay_url: String, seeds: Vec<u8>, material: Vec<u8>) -> Result<bool> {
    let me = identity(&seeds)?;
    let m = self::material(&material)?;
    let mut watch = crate::diag::Stopwatch::new("seat-keep");
    let kept = runtime().block_on(repair::keep_seat(
        &vault_relay(&relay_url, &m),
        &me,
        &m,
        &mut OsRng,
    ));
    watch.lap("relay");
    watch.finish();
    Ok(kept?)
}

/// A helper stops helping the pending move at `replacement`, for good (its owner says they
/// didn't lose their phone). Behind a confirm and the unlock; never automatic.
pub fn stop_helping(
    relay_url: String,
    seeds: Vec<u8>,
    material: Vec<u8>,
    replacement: u64,
) -> Result<()> {
    let me = identity(&seeds)?;
    let m = self::material(&material)?;
    runtime().block_on(repair::stop_helping(
        &vault_relay(&relay_url, &m),
        &me,
        &m,
        replacement,
        &mut OsRng,
    ))?;
    Ok(())
}

pub enum RecoveryStage {
    /// No vault has moved a seat to this phone yet: waiting for co-signers to approve.
    Waiting,
    /// Co-signers approved; their phones wait up to 3 days before finishing, in case the old
    /// phone still works.
    Pending,
    /// The seat moved; the co-signers' phones are repairing the key.
    Repairing,
    /// Done: `material` and `invite` hold the vault (save them like a restored backup).
    Done,
}

pub struct RecoveryProgress {
    pub stage: RecoveryStage,
    /// Repair messages received and needed so far (`Repairing`; needed is 0 until known).
    pub received: u32,
    pub needed: u32,
    pub vault_id: String,
    pub name: String,
    /// `VaultMaterial::to_bytes` (secret; `Done` only).
    pub material: Vec<u8>,
    pub invite: String,
    /// Until done: the words each co-signer that started the safety check shows.
    pub checks: Vec<CosignerWords>,
}

/// The words a co-signer (`cosigner_key_hex`) sees for this new phone.
pub struct CosignerWords {
    pub cosigner_key_hex: String,
    pub words: String,
}

/// The co-signer's link for a phone recovering a seat in this vault: the vault's relay
/// and mailbox (`zafe-seat-v1:...`; `default_relay_url` stands in for a vault on the
/// app-wide relay). The new phone opens it before the safety check.
#[flutter_rust_bridge::frb(sync)]
pub fn seat_link(material: Vec<u8>, default_relay_url: String) -> Result<String> {
    let m = self::material(&material)?;
    Ok(SeatLink::for_vault(&m, &default_relay_url).encode())
}

/// Where a co-signer's seat link points.
pub struct SeatLinkInfo {
    pub relay_url: String,
    /// The vault's mailbox on that relay (hex), for [`check_recovery`].
    pub label: String,
}

/// Parses a seat link a co-signer sent (also inside a message or a `zafe://seat?link=`
/// URL).
#[flutter_rust_bridge::frb(sync)]
pub fn parse_seat_link(text: String) -> Result<SeatLinkInfo> {
    let link = SeatLink::decode(&text).map_err(|e| match e {
        zafe_core::node::NodeError::UnsupportedVersion(_) => e.into(),
        _ => ZafeError::invalid("That isn't a Zafe seat link"),
    })?;
    Ok(SeatLinkInfo {
        relay_url: link.relay_url,
        label: hex::encode(link.label),
    })
}

/// The new phone: checks whether its seat moved and the repaired key has arrived. On
/// `Done` the key has been checked against the vault (spec §10.4.2), and its relay history
/// against what every helper vouched for. `relay_url` and `label`: from the co-signer's
/// seat link ([`parse_seat_link`]); without a label (no link: the app-wide relay), every
/// mailbox the relay lists for this phone is checked. `confirmed`: the co-signers (hex
/// keys) whose words the person confirmed (kept with the recovery identity); a vault is
/// taken only through one of them. `recovery_dir`: where this phone pins what it reveals
/// to each co-signer (`ZafePaths.recoveryDir`; deleted with the recovery identity).
pub fn check_recovery(
    relay_url: String,
    seeds: Vec<u8>,
    label: Option<String>,
    confirmed: Vec<String>,
    recovery_dir: String,
) -> Result<RecoveryProgress> {
    let me = identity(&seeds)?;
    let confirmed = confirmed
        .iter()
        .map(|k| {
            hex::decode(k.trim())
                .ok()
                .and_then(|b| <[u8; 32]>::try_from(b).ok())
                .ok_or_else(|| ZafeError::invalid("not a signer key"))
        })
        .collect::<Result<Vec<_>>>()?;
    let label = label
        .map(|l| {
            hex::decode(l.trim())
                .ok()
                .and_then(|b| <[u8; 16]>::try_from(b).ok())
                .ok_or_else(|| ZafeError::invalid("bad mailbox label"))
        })
        .transpose()?;
    let relay = RelayClient::pooled(relay_url);
    let mut watch = crate::diag::Stopwatch::new("recover");
    // Both read the relay (~1 s each over Tor): at once. The words stay shown until done:
    // the person may confirm a co-signer's row after the seat moved.
    let (status, checks) = runtime().block_on(async {
        tokio::join!(
            repair::try_recover(&relay, &me, label, &confirmed),
            repair::recovery_checks(&relay, &me, std::path::Path::new(&recovery_dir))
        )
    });
    watch.lap("relay");
    watch.finish();
    let status = status?;
    let checks: Vec<CosignerWords> = checks?
        .into_iter()
        .map(|c| CosignerWords {
            cosigner_key_hex: hex::encode(c.cosigner),
            words: c.words,
        })
        .collect();
    let waiting = |stage, received, needed| RecoveryProgress {
        stage,
        received,
        needed,
        vault_id: String::new(),
        name: String::new(),
        material: Vec::new(),
        invite: String::new(),
        checks,
    };
    Ok(match status {
        RecoveryStatus::Waiting => waiting(RecoveryStage::Waiting, 0, 0),
        RecoveryStatus::Pending => waiting(RecoveryStage::Pending, 0, 0),
        RecoveryStatus::SeatMoved { received, needed } => {
            waiting(RecoveryStage::Repairing, received as u32, needed as u32)
        }
        RecoveryStatus::Done { material, invite } => RecoveryProgress {
            stage: RecoveryStage::Done,
            received: 0,
            needed: 0,
            vault_id: hex::encode(material.descriptor.vault_id),
            name: material.descriptor.name.clone(),
            material: material
                .to_bytes()
                .map_err(|e| ZafeError::new(ZafeErrorKind::Other, e.to_string()))?,
            invite: invite.encode(),
            checks: Vec::new(),
        },
    })
}
