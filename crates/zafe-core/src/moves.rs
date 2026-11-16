use serde::{Deserialize, Serialize};
use zafe_proto::{
    version::{self, DecodeError, Format},
    Identity, IdentityPublic, MailboxId,
};

const ARRIVAL_DOMAIN: &[u8] = b"Zafe relay move arrival v1";
const STATE_KEY_DOMAIN: &str = "Zafe relay move state key v1";
const MOVE_ID_DOMAIN: &str = "Zafe relay move id v1";

pub const MOVE_LIFETIME_SECS: u64 = 14 * 24 * 3600;

pub const MOVE_LINK_BASE: &str = "https://zafe.cash/move#";

pub const MOVE_PAD_BYTES: usize = 1024;

pub fn switch_threshold(members: usize, threshold: usize) -> usize {
    threshold.max(members.saturating_sub(threshold) + 1)
}

pub fn move_id_for(mover: &[u8; 32], label: &MailboxId) -> [u8; 16] {
    let mut input = mover.to_vec();
    input.extend_from_slice(label);
    let key = blake3::derive_key(MOVE_ID_DOMAIN, &input);
    key[..16].try_into().expect("16 bytes")
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MoveTarget {
    pub vault_id: MailboxId,
    pub move_id: [u8; 16],
    pub relay: String,
    pub label: MailboxId,
    pub base_index: u64,
    pub base_hash: [u8; 32],
    pub expiry: u64,
}

impl MoveTarget {
    fn message(&self) -> Vec<u8> {
        let mut msg = ARRIVAL_DOMAIN.to_vec();
        msg.extend_from_slice(&postcard::to_allocvec(self).expect("encodable"));
        msg
    }

    pub fn sign(&self, me: &Identity) -> Vec<u8> {
        me.sign(&self.message()).to_vec()
    }

    pub fn verify(&self, who: &IdentityPublic, signature: &[u8]) -> bool {
        who.verify(&self.message(), signature).is_ok()
    }

    pub fn mover_among<'a>(
        &self,
        members: impl IntoIterator<Item = &'a [u8; 32]>,
    ) -> Option<[u8; 32]> {
        members
            .into_iter()
            .find(|pk| move_id_for(pk, &self.label) == self.move_id)
            .copied()
    }

    pub fn expired(&self, now: u64) -> bool {
        now >= self.expiry
    }

    pub fn host(&self) -> String {
        relay_host(&self.relay)
    }
}

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

pub fn relay_host(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    rest.split('/').next().unwrap_or(rest).to_owned()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MoveMessage {
    Arrival {
        target: MoveTarget,
        signature: Vec<u8>,
        create_token: Option<String>,
    },
    Notice {
        target: MoveTarget,
        mover: [u8; 32],
        signature: Vec<u8>,
        create_token: Option<String>,
        moved: Option<Vec<u8>>,
    },
    Cancelled {
        move_id: [u8; 16],
    },
}

impl MoveMessage {
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

    pub fn encode(&self) -> String {
        let bytes = version::encode(Format::MoveLink, self).expect("encodable");
        format!("{MOVE_LINK_BASE}{}", hex::encode(bytes))
    }

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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveState {
    pub target: MoveTarget,
    pub mover: [u8; 32],
    pub arrived: bool,
    pub create_token: Option<String>,
    pub notified: Vec<[u8; 32]>,
    pub cancelled: bool,
    pub moved_seen: bool,
    pub switched: bool,
    pub moved_notice_sent: bool,
    pub arrival_delivered: bool,
    pub from: crate::node::VaultRelay,
}

impl MoveState {
    pub fn open(&self, now: u64) -> bool {
        !self.switched && !self.cancelled && !self.moved_seen && !self.target.expired(now)
    }

    fn file_key(vault_secret: &[u8; 32]) -> crate::nonce_store::NonceKey {
        crate::nonce_store::NonceKey::from_raw(blake3::derive_key(STATE_KEY_DOMAIN, vault_secret))
    }

    fn file_name(vault: &MailboxId) -> String {
        format!("{}.move", hex::encode(vault))
    }

    pub fn load(dir: &std::path::Path, vault: &MailboxId, vault_secret: &[u8; 32]) -> Option<Self> {
        let name = Self::file_name(vault);
        let bytes = std::fs::read(dir.join(&name)).ok()?;
        let body = version::unframe(Format::MoveState, &bytes).ok()?;
        let plain = Self::file_key(vault_secret).open(&name, body)?;
        postcard::from_bytes(&plain).ok()
    }

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
