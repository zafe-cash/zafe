#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quota {
    Recipient,
    Deliveries,
    Log,
    Mailboxes,
    Relay,
}

impl Quota {
    pub fn as_str(self) -> &'static str {
        match self {
            Quota::Recipient => zafe_proto::relay::RECIPIENT_FULL,
            Quota::Deliveries => "the vault's undelivered messages are over the size limit",
            Quota::Log => zafe_proto::relay::LOG_FULL,
            Quota::Mailboxes => "this key has created too many vaults on this relay",
            Quota::Relay => zafe_proto::relay::RELAY_FULL,
        }
    }
}

impl core::fmt::Display for Quota {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Quotas {
    pub envelopes_per_pair: Option<u64>,
    pub pair_bytes: Option<u64>,
    pub delivery_bytes: Option<u64>,
    pub log_bytes: Option<u64>,
    pub mailboxes_per_key: Option<u64>,
    pub db_bytes: Option<u64>,
}

pub fn author_log_cap(cap: u64) -> u64 {
    cap / 2
}

pub(crate) fn sender_delivery_cap(cap: u64, pair: Option<u64>) -> u64 {
    (cap / 2).max(pair.unwrap_or(0)).min(cap)
}

const MIB: u64 = 1024 * 1024;

impl Quotas {
    pub const fn none() -> Self {
        Self {
            envelopes_per_pair: None,
            pair_bytes: None,
            delivery_bytes: None,
            log_bytes: None,
            mailboxes_per_key: None,
            db_bytes: None,
        }
    }

    pub const fn hosted() -> Self {
        Self {
            envelopes_per_pair: Some(256),
            pair_bytes: Some(2 * MIB),
            delivery_bytes: Some(64 * MIB),
            log_bytes: Some(512 * MIB),
            mailboxes_per_key: Some(8),
            db_bytes: None,
        }
    }
}

impl Default for Quotas {
    fn default() -> Self {
        Self::none()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    pub deliveries: u64,
    pub delivery_bytes: u64,
    pub log_bytes: u64,
}
