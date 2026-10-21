//! Storage quotas for the hosted relay: how much one mailbox (vault) may keep.
//!
//! Rate limits ([`crate::limits`]) bound how fast a member can write; quotas bound how
//! much a vault can accumulate. Four caps, each `None` = unlimited:
//!
//! - **Undelivered envelopes per recipient** (count). Deliveries stay until
//!   [`crate::Relay::prune`] drops them after [`crate::DELIVERY_RETENTION_SECS`], so this
//!   is "envelopes addressed to one member in the last 30 days".
//! - **Delivery bytes per mailbox**: the stored size of every delivery (an envelope sent to
//!   all members is stored, and counted, once per recipient). Pruning frees it.
//! - **Log bytes per mailbox**: the vault log is kept forever, so this cap is generous and
//!   only stops a runaway or hostile member; each entry is already at most
//!   [`crate::MAX_BODY_BYTES`].
//! - **Mailboxes per signing key**: creating a mailbox is free, so one key can't create
//!   more than a few (an indexed count on `mailboxes.creator`).
//!
//! A write that would go over a cap is refused whole with
//! [`crate::RelayError::QuotaExceeded`] (HTTP 507 Insufficient Storage): nothing is
//! stored, the sender's sequence number isn't consumed.
//!
//! Cost per request: the byte totals are running counters on the `mailboxes` row
//! (`delivery_bytes`, `log_bytes`), updated in the same transaction as each insert and by
//! pruning, so no request sums a table. The per-recipient count is an indexed query on
//! `deliveries_by_recipient (mailbox, recipient, cursor)`: it only walks that recipient's
//! index entries, at most the cap itself, and only runs when the cap is set.

/// Which cap a refused write hit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quota {
    /// A recipient has too many undelivered envelopes.
    Inbox,
    /// The mailbox's undelivered envelopes take too many bytes.
    Deliveries,
    /// The mailbox's vault log takes too many bytes.
    Log,
    /// The signing key already created too many mailboxes.
    Mailboxes,
}

impl Quota {
    pub fn as_str(self) -> &'static str {
        match self {
            Quota::Inbox => "a member's inbox is full",
            Quota::Deliveries => "the vault's undelivered messages are over the size limit",
            Quota::Log => "the vault log is over the size limit",
            Quota::Mailboxes => "this key has created too many vaults on this relay",
        }
    }
}

impl core::fmt::Display for Quota {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Storage caps per mailbox. `None` is unlimited.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Quotas {
    /// Undelivered envelopes one recipient may have in one mailbox.
    pub envelopes_per_recipient: Option<u64>,
    /// Total bytes of undelivered envelopes in one mailbox.
    pub delivery_bytes: Option<u64>,
    /// Total bytes of one mailbox's vault log.
    pub log_bytes: Option<u64>,
    /// Mailboxes one signing key may create (counted from the table, so it holds across
    /// restarts). The app uses a fresh key per vault, so one is the norm; the per-IP
    /// creation limit ([`crate::limits::Limits::creates_per_ip`]) bounds new keys.
    pub mailboxes_per_key: Option<u64>,
}

const MIB: u64 = 1024 * 1024;

impl Quotas {
    /// No quotas (tests, self-hosting on a trusted network).
    pub const fn none() -> Self {
        Self {
            envelopes_per_recipient: None,
            delivery_bytes: None,
            log_bytes: None,
            mailboxes_per_key: None,
        }
    }

    /// Defaults for a public relay. Real vaults stay far below them: key generation sends
    /// a few envelopes per member, an interactive signing round one request (a PCZT, tens
    /// of KB) and one answer per signer, and one-tap signing goes through the log. A
    /// proposal with its votes adds well under 1 MB to the log, so 512 MiB is thousands of
    /// payments.
    pub const fn hosted() -> Self {
        Self {
            envelopes_per_recipient: Some(10_000),
            delivery_bytes: Some(256 * MIB),
            log_bytes: Some(512 * MIB),
            mailboxes_per_key: Some(8),
        }
    }
}

impl Default for Quotas {
    fn default() -> Self {
        Self::none()
    }
}

/// What one mailbox currently stores (for operators and tests).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    /// Undelivered envelopes (all recipients).
    pub deliveries: u64,
    /// Bytes of undelivered envelopes (the running counter).
    pub delivery_bytes: u64,
    /// Bytes of the vault log (the running counter).
    pub log_bytes: u64,
}
