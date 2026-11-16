use rand::rngs::OsRng;
use zafe_core::backup::{self, Contents, KdfParams};

use super::{
    error::{ZafeError, ZafeErrorKind},
    names::{from_map, to_map, SignerName},
};

impl From<backup::BackupError> for ZafeError {
    fn from(e: backup::BackupError) -> Self {
        use backup::BackupError::*;
        let kind = match &e {
            UnsupportedVersion(v) if v.is_newer() => ZafeErrorKind::UpdateRequired,
            WeakPassphrase(_) | NotABackup | WrongPassphraseOrDamaged | UnsupportedVersion(_) => {
                ZafeErrorKind::InvalidInput
            }
            NotAMember | WrongAddress | BadParams => ZafeErrorKind::Verification,
            Encoding(_) => ZafeErrorKind::Other,
        };
        let message = match e {
            WeakPassphrase(hint) => format!("Passphrase too weak: {hint}"),
            NotABackup => "This isn't a Zafe vault backup".into(),
            WrongPassphraseOrDamaged => {
                "These words don't open this backup, or it is damaged".into()
            }
            UnsupportedVersion(v) if v.is_newer() => {
                "This backup needs a newer version of Zafe".into()
            }
            UnsupportedVersion(_) => {
                "This backup was made by an older version of Zafe that this one can't read".into()
            }
            other => other.to_string(),
        };
        ZafeError::new(kind, message)
    }
}

pub struct PassphraseCheck {
    pub ok: bool,
    pub hint: String,
}

#[flutter_rust_bridge::frb(sync)]
pub fn check_backup_passphrase(passphrase: String) -> PassphraseCheck {
    match backup::check_passphrase(&passphrase) {
        Ok(()) => PassphraseCheck {
            ok: true,
            hint: String::new(),
        },
        Err(backup::BackupError::WeakPassphrase(hint)) => PassphraseCheck { ok: false, hint },
        Err(e) => PassphraseCheck {
            ok: false,
            hint: e.to_string(),
        },
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn suggest_backup_passphrase() -> String {
    backup::suggest_passphrase(&mut OsRng)
}

pub struct ExportedBackup {
    pub bytes: Vec<u8>,
    pub text: String,
}

pub fn export_vault_backup(
    relay_url: String,
    seeds: Vec<u8>,
    material: Vec<u8>,
    invite: String,
    names: Vec<SignerName>,
    passphrase: String,
) -> Result<ExportedBackup, ZafeError> {
    let mut m = super::vault::material(&material)?;
    if m.relay.url.is_none() {
        m.relay.url = Some(relay_url.trim_end_matches('/').to_owned());
    }
    let cache = zafe_core::log_cache::configured().ok_or_else(|| {
        ZafeError::new(
            ZafeErrorKind::Other,
            "the log copy is not set up (init_log_cache was not called)",
        )
    })?;
    let log = cache
        .read(&m.descriptor.vault_id)
        .map_err(|e| ZafeError::new(ZafeErrorKind::Other, e.to_string()))?
        .iter()
        .map(|e| e.to_bytes())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| ZafeError::new(ZafeErrorKind::Other, format!("log copy: {e:?}")))?;
    let contents = Contents {
        identity_seeds: seeds,
        material: m.to_bytes()?,
        invite,
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs()),
        names: to_map(names),
        log,
    };
    let bytes = backup::encrypt(&contents, &passphrase, KdfParams::DEFAULT, &mut OsRng)?;
    backup::decrypt(&bytes, &passphrase)?.validate()?;
    let text = backup::to_text(&bytes);
    Ok(ExportedBackup { bytes, text })
}

pub struct ImportedVault {
    pub vault_id: String,
    pub name: String,
    pub identity_seeds: Vec<u8>,
    pub material: Vec<u8>,
    pub invite: String,
    pub names: Vec<SignerName>,
}

pub fn import_vault_backup(data: Vec<u8>, passphrase: String) -> Result<ImportedVault, ZafeError> {
    let bytes = match std::str::from_utf8(&data) {
        Ok(text) if text.trim_start().starts_with("zafe-backup-v1:") => backup::from_text(text)?,
        _ => data,
    };
    let contents = backup::decrypt(&bytes, &passphrase)?;
    let material = contents.validate()?;
    let cache = zafe_core::log_cache::configured().ok_or_else(|| {
        ZafeError::new(
            ZafeErrorKind::Other,
            "the log copy is not set up (init_log_cache was not called)",
        )
    })?;
    zafe_core::node::install_log_copy(&cache, &material, &contents.log)?;
    Ok(ImportedVault {
        vault_id: hex::encode(material.descriptor.vault_id),
        name: material.descriptor.name.clone(),
        identity_seeds: contents.identity_seeds.clone(),
        material: contents.material.clone(),
        invite: contents.invite.clone(),
        names: from_map(&contents.names),
    })
}
