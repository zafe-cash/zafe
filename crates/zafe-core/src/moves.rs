//! Moving a vault to another relay: the formats (`docs/relay-move.md` §4.2-4.7).
//!
//! A move has a target ([`MoveTarget`]): the vault, a random move id, the new relay's URL,
//! the vault's new mailbox label there, the point of the log the mover started from
//! (`base`) and an expiry. Each member who moves signs the target with its identity key:
//! its **arrival**, sent sealed to every other member on the new relay. The first phone
//! holding m = max(t, n − t + 1) arrivals appends `VaultEvent::VaultMoved` there, and from
//! that entry on the vault lives on the new relay. On the old relay the mover leaves a
//! sealed **move notice** for each member (never acknowledged, so a phone that was off
//! finds it later); the switching phone adds the `VaultMoved` entry to it.
//!
//! The flows are in `node` (`move_vault`, `follow_move`, `move_progress`,
//! `cancel_move`); this phone's part in a move is a [`MoveState`] file next to its log copy.

use serde::{Deserialize, Serialize};
use zafe_proto::{
    version::{self, DecodeError, Format},
    Identity, IdentityPublic, MailboxId,
};

const ARRIVAL_DOMAIN: &[u8] = b"Zafe relay move arrival v1";
const STATE_KEY_DOMAIN: &str = "Zafe relay move state key v1";

/// How long a move stays open: 14 days from when the mover started it.
pub const MOVE_LIFETIME_SECS: u64 = 14 * 24 * 3600;

/// The site the move link points at (only its fragment matters, and the site never sees it).
pub const MOVE_LINK_BASE: &str = "https://zafe.cash/move#";

/// Move messages are padded to a multiple of this many bytes, so the old relay can't tell
/// a move notice from the sealed signing messages it relays every day by its size (a 2-of-3
/// signing request for one spend is about 0.7 KB, its shares about 0.1 KB).
pub const MOVE_PAD_BYTES: usize = 1024;

/// Members who must have moved before the vault switches: at least t (so payments work on
/// the new relay) and more than could be left behind (so they can never reach t on the old
/// one): `max(t, n − t + 1)`. 2-of-3 and 3-of-5: t; 2-of-4: 3; 2-of-5: 4.
pub fn switch_threshold(members: usize, threshold: usize) -> usize {
    threshold.max(members.saturating_sub(threshold) + 1)
}

/// What every arrival of a move signs.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MoveTarget {
    pub vault_id: MailboxId,
    pub move_id: [u8; 16],
    /// The new relay ([`normalize_relay_url`]).
    pub relay: String,
    /// The vault's mailbox label on the new relay.
    pub label: MailboxId,
    /// Index and hash of the last log entry the mover's copy held when it started.
    pub base_index: u64,
    pub base_hash: [u8; 32],
    /// Unix seconds (mover's clock): [`MOVE_LIFETIME_SECS`] after it started.
    pub expiry: u64,
}

impl MoveTarget {
    fn message(&self) -> Vec<u8> {
        let mut msg = ARRIVAL_DOMAIN.to_vec();
        msg.extend_from_slice(&postcard::to_allocvec(self).expect("encodable"));
        msg
    }

    /// `me`'s arrival: its signature over the target.
    pub fn sign(&self, me: &Identity) -> Vec<u8> {
        me.sign(&self.message()).to_vec()
    }

    /// Whether `signature` is `who`'s arrival for this target.
    pub fn verify(&self, who: &IdentityPublic, signature: &[u8]) -> bool {
        who.verify(&self.message(), signature).is_ok()
    }

    pub fn expired(&self, now: u64) -> bool {
        now >= self.expiry
    }

    /// The relay's host (and port) for display: `relay.dao.org`.
    pub fn host(&self) -> String {
        relay_host(&self.relay)
    }
}

/// `scheme://host[:port][/path]` with the scheme and host lower-cased and no trailing
/// slash, so every member signs the same string for the same relay. `None` for anything
/// that isn't an http(s) URL.
pub fn normalize_relay_url(url: &str) -> Option<String> {
    let url = url.trim().trim_end_matches('/');
    let (scheme, rest) = url.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "https" && scheme != "http" {
        return None;
    }
    let (host, path) = match rest.find('/') {
        Some(i) => rest.split_at(i),
        None => (rest, ""),
    };
    if host.is_empty()
        || host.contains('@')
        || path.contains(['?', '#'])
        || host.contains(|c: char| c.is_whitespace())
    {
        return None;
    }
    Some(format!("{scheme}://{}{path}", host.to_ascii_lowercase()))
}

/// `relay.dao.org` from `https://relay.dao.org/`.
pub fn relay_host(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    rest.split('/').next().unwrap_or(rest).to_owned()
}

/// What travels in a move's envelopes (`Kind::Arrival`, `Kind::MoveNotice`,
/// `Kind::MoveCancelled`), version-tagged and padded ([`MoveMessage::to_padded`]).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MoveMessage {
    /// On the new relay: the sender's arrival. The mover's also carries the relay's create
    /// token (sealed), so every member can restore the vault on a private relay.
    Arrival {
        target: MoveTarget,
        signature: Vec<u8>,
        create_token: Option<String>,
    },
    /// On the old relay, from the mover: where the vault is moving and the mover's arrival;
    /// from the phone that switched, also the `VaultMoved` log entry (`LogEntry::to_bytes`).
    Notice {
        target: MoveTarget,
        mover: [u8; 32],
        signature: Vec<u8>,
        create_token: Option<String>,
        moved: Option<Vec<u8>>,
    },
    /// The sender withdraws its arrival for `move_id` (from the mover: the move is off).
    Cancelled { move_id: [u8; 16] },
}

impl MoveMessage {
    /// `version || u32 length || postcard || zeros`, a multiple of [`MOVE_PAD_BYTES`].
    pub fn to_padded(&self) -> Vec<u8> {
        let body = postcard::to_allocvec(self).expect("encodable");
        let mut inner = (body.len() as u32).to_le_bytes().to_vec();
        inner.extend_from_slice(&body);
        let framed = version::frame(Format::MoveMessage, &inner);
        let size = framed.len().div_ceil(MOVE_PAD_BYTES).max(1) * MOVE_PAD_BYTES;
        let mut out = framed;
        out.resize(size, 0);
        out
    }

    pub fn from_padded(bytes: &[u8]) -> Result<Self, DecodeError> {
        let inner = version::unframe(Format::MoveMessage, bytes)?;
        let bad = DecodeError::Malformed(Format::MoveMessage);
        let (len, rest) = inner.split_at_checked(4).ok_or(bad)?;
        let len = u32::from_le_bytes(len.try_into().expect("4 bytes")) as usize;
        let body = rest.get(..len).ok_or(bad)?;
        postcard::from_bytes(body).map_err(|_| bad)
    }
}

/// What the move link carries in its fragment: enough to find the move, nothing secret
/// (the create token never goes in a link) and nothing to trust: following needs a current
/// member's arrival on that relay.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveLink {
    pub vault_id: MailboxId,
    pub relay: String,
    pub label: MailboxId,
    pub move_id: [u8; 16],
}

impl MoveLink {
    pub fn of(target: &MoveTarget) -> Self {
        Self {
            vault_id: target.vault_id,
            relay: target.relay.clone(),
            label: target.label,
            move_id: target.move_id,
        }
    }

    /// `https://zafe.cash/move#<hex>`.
    pub fn encode(&self) -> String {
        let bytes = version::encode(Format::MoveLink, self).expect("encodable");
        format!("{MOVE_LINK_BASE}{}", hex::encode(bytes))
    }

    /// Reads the link, its fragment, or a message that contains the link (the share text).
    pub fn decode(text: &str) -> Result<Self, DecodeError> {
        let bad = DecodeError::Malformed(Format::MoveLink);
        let text = text.trim();
        let fragment = match text.find("/move#") {
            Some(i) => &text[i + "/move#".len()..],
            None => text,
        };
        let hex_part: String = fragment
            .chars()
            .take_while(|c| c.is_ascii_hexdigit())
            .collect();
        let bytes = hex::decode(hex_part).map_err(|_| bad)?;
        let link: Self = version::decode(Format::MoveLink, &bytes)?;
        let relay = normalize_relay_url(&link.relay).ok_or(bad)?;
        Ok(Self { relay, ..link })
    }
}

/// This phone's part in a move of one vault (`<vault id>.move` in the log copy's
/// directory, encrypted with a key derived from the vault secret). One move at a time per
/// phone: a new one replaces it only once it switched, expired or was cancelled.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveState {
    pub target: MoveTarget,
    /// Who started the move.
    pub mover: [u8; 32],
    /// This phone sent its arrival (from then on it checks the new relay for `VaultMoved`
    /// before every write to the old one, §4.5 rule 2).
    pub arrived: bool,
    /// The new relay's create token (the mover's own, or from the mover's sealed arrival or
    /// notice).
    pub create_token: Option<String>,
    /// Members (signing keys) whose move notice reached the old relay.
    pub notified: Vec<[u8; 32]>,
    pub cancelled: bool,
    /// This phone saw `VaultMoved` for the move and verified it: the vault is on the new
    /// relay ([`MoveState::switched`] decides whether this phone already follows it).
    pub moved_seen: bool,
    /// This phone verified `VaultMoved` on the new relay itself: its vault material
    /// follows the new relay from now on.
    pub switched: bool,
    /// This phone left the `VaultMoved` entry in the old relay's move notices.
    pub moved_notice_sent: bool,
    /// This phone's arrival reached every other member on the new relay.
    pub arrival_delivered: bool,
}

impl MoveState {
    /// Still in progress: neither switched, cancelled nor expired.
    pub fn open(&self, now: u64) -> bool {
        !self.switched && !self.cancelled && !self.moved_seen && !self.target.expired(now)
    }

    fn file_key(vault_secret: &[u8; 32]) -> crate::nonce_store::NonceKey {
        crate::nonce_store::NonceKey::from_raw(blake3::derive_key(STATE_KEY_DOMAIN, vault_secret))
    }

    fn file_name(vault: &MailboxId) -> String {
        format!("{}.move", hex::encode(vault))
    }

    /// The vault's move file in `dir`, if there is a readable one.
    pub fn load(dir: &std::path::Path, vault: &MailboxId, vault_secret: &[u8; 32]) -> Option<Self> {
        let name = Self::file_name(vault);
        let bytes = std::fs::read(dir.join(&name)).ok()?;
        let body = version::unframe(Format::MoveState, &bytes).ok()?;
        let plain = Self::file_key(vault_secret).open(&name, body)?;
        postcard::from_bytes(&plain).ok()
    }

    /// Saves it durably (temp + fsync + rename).
    pub fn save(&self, dir: &std::path::Path, vault_secret: &[u8; 32]) -> std::io::Result<()> {
        let name = Self::file_name(&self.target.vault_id);
        let plain = postcard::to_allocvec(self).map_err(std::io::Error::other)?;
        let sealed = Self::file_key(vault_secret).seal(&name, &plain);
        std::fs::create_dir_all(dir)?;
        crate::durable::write(&dir.join(name), &version::frame(Format::MoveState, &sealed))
    }

    pub fn remove(dir: &std::path::Path, vault: &MailboxId) {
        let _ = crate::durable::remove(&dir.join(Self::file_name(vault)));
    }
}

pub(crate) fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn m_is_more_than_who_could_stay() {
        assert_eq!(switch_threshold(3, 2), 2);
        assert_eq!(switch_threshold(5, 3), 3);
        assert_eq!(switch_threshold(4, 2), 3);
        assert_eq!(switch_threshold(5, 2), 4);
        assert_eq!(switch_threshold(2, 2), 2);
        assert_eq!(switch_threshold(7, 4), 4);
    }

    #[test]
    fn urls_normalize() {
        assert_eq!(
            normalize_relay_url(" HTTPS://Relay.DAO.org/ ").as_deref(),
            Some("https://relay.dao.org")
        );
        assert_eq!(
            normalize_relay_url("http://10.0.2.2:18900").as_deref(),
            Some("http://10.0.2.2:18900")
        );
        assert_eq!(normalize_relay_url("ftp://x"), None);
        assert_eq!(normalize_relay_url("https://u@x"), None);
        assert_eq!(relay_host("https://relay.dao.org/x"), "relay.dao.org");
    }

    #[test]
    fn messages_pad_and_links_round_trip() {
        let target = MoveTarget {
            vault_id: [1; 16],
            move_id: [2; 16],
            relay: "https://relay.dao.org".into(),
            label: [3; 16],
            base_index: 7,
            base_hash: [4; 32],
            expiry: 99,
        };
        let msg = MoveMessage::Arrival {
            target: target.clone(),
            signature: vec![5; 64],
            create_token: None,
        };
        let bytes = msg.to_padded();
        assert_eq!(bytes.len(), MOVE_PAD_BYTES);
        assert_eq!(MoveMessage::from_padded(&bytes).unwrap(), msg);
        let cancel = MoveMessage::Cancelled { move_id: [2; 16] }.to_padded();
        assert_eq!(cancel.len(), MOVE_PAD_BYTES, "every kind looks the same");

        let link = MoveLink::of(&target);
        let text = format!(
            "I'm moving our Zafe vault Grants to relay.dao.org. Open this to move too: {}",
            link.encode()
        );
        assert_eq!(MoveLink::decode(&text).unwrap(), link);
        assert!(link.encode().starts_with(MOVE_LINK_BASE));
        assert!(MoveLink::decode("https://zafe.cash/move#zz").is_err());
    }
}
