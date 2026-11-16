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
    pub code: String,
}

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
    pub key_hex: String,
}

#[flutter_rust_bridge::frb(sync)]
pub fn parse_recovery_code(code: String) -> Result<RecoveryCodeInfo> {
    let request = decode(&code)?;
    Ok(RecoveryCodeInfo {
        key_hex: hex::encode(request.identity.sig_pk),
    })
}

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
    Waiting,
    Pending,
    Repairing,
    Done,
}

pub struct RecoveryProgress {
    pub stage: RecoveryStage,
    pub received: u32,
    pub needed: u32,
    pub vault_id: String,
    pub name: String,
    pub material: Vec<u8>,
    pub invite: String,
    pub checks: Vec<CosignerWords>,
}

pub struct CosignerWords {
    pub cosigner_key_hex: String,
    pub words: String,
}

#[flutter_rust_bridge::frb(sync)]
pub fn seat_link(material: Vec<u8>, default_relay_url: String) -> Result<String> {
    let m = self::material(&material)?;
    Ok(SeatLink::for_vault(&m, &default_relay_url).encode())
}

pub struct SeatLinkInfo {
    pub relay_url: String,
    pub label: String,
}

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
