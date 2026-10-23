//! Vault export and import (spec §12.2): passphrase-encrypted backups of one vault.
//! See `zafe_core::backup` for the format. Backups never contain signing nonces.

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
            NotAMember | BadParams => ZafeErrorKind::Verification,
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
    /// Why it's too weak (empty when ok).
    pub hint: String,
}

/// Live feedback while typing a backup passphrase.
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

/// 12 random words (128 bits): strong and writable on paper.
#[flutter_rust_bridge::frb(sync)]
pub fn suggest_backup_passphrase() -> String {
    backup::suggest_passphrase(&mut OsRng)
}

pub struct ExportedBackup {
    /// The file contents.
    pub bytes: Vec<u8>,
    /// The same backup as text (`zafe-backup-v1:...`), for password managers.
    pub text: String,
}

/// Encrypts this device's copy of a vault (Argon2id 64 MiB; takes a second or two), with
/// the local names this device gave the signers.
pub fn export_vault_backup(
    seeds: Vec<u8>,
    material: Vec<u8>,
    invite: String,
    names: Vec<SignerName>,
    passphrase: String,
) -> Result<ExportedBackup, ZafeError> {
    let contents = Contents {
        identity_seeds: seeds,
        material,
        invite,
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs()),
        names: to_map(names),
    };
    let bytes = backup::encrypt(&contents, &passphrase, KdfParams::DEFAULT, &mut OsRng)?;
    // Open it again before handing it out: the member attests this backup in the log.
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
    /// Local signer names saved with the backup (empty for backups made before names were
    /// included).
    pub names: Vec<SignerName>,
}

/// Decrypts a backup (file bytes, or the pasted text form) and checks it: the identity must
/// be a member of the vault it restores.
pub fn import_vault_backup(data: Vec<u8>, passphrase: String) -> Result<ImportedVault, ZafeError> {
    let bytes = match std::str::from_utf8(&data) {
        Ok(text) if text.trim_start().starts_with("zafe-backup-v1:") => backup::from_text(text)?,
        _ => data,
    };
    let contents = backup::decrypt(&bytes, &passphrase)?;
    let material = contents.validate()?;
    Ok(ImportedVault {
        vault_id: hex::encode(material.descriptor.vault_id),
        name: material.descriptor.name.clone(),
        identity_seeds: contents.identity_seeds.clone(),
        material: contents.material.clone(),
        invite: contents.invite.clone(),
        names: from_map(&contents.names),
    })
}
