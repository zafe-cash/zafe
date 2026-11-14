//! Storage quotas for the hosted relay: how much one mailbox (vault) may keep.
//!
//! Rate limits ([`crate::limits`]) bound how fast a member can write; quotas bound how
//! much a vault, and the whole relay, can accumulate. Each cap is `None` = unlimited:
//!
//! - **Undelivered envelopes per (sender, recipient) pair** (count and bytes): fixed, so
//!   one member's envelopes, never acked (a phone acks only the kinds it handles), close
//!   only its own pair, never another member's inbox for everyone (security review 2026-11,
//!   NEW 6), and a victim never reads more than a pair's bytes of one sender's junk. A pair
//!   holds at least two envelopes of [`zafe_proto::relay::MAX_ENTRY_BYTES`], so a large
//!   vault's signing request always fits. Deliveries stay until acked or
//!   [`crate::Relay::prune`] drops them after [`crate::DELIVERY_RETENTION_SECS`].
//! - **Delivery bytes per mailbox** (an envelope sent to all members is stored, and
//!   counted, once per recipient), and per sender at most half of it (never below a
//!   pair's): one dishonest member can't fill it, and a 64-member vault's leader still
//!   sends a whole signing round (a 1/n share, 1 MiB, refused one of ~20 notes and t = 33:
//!   ~70 KB to each of 32 signers). Pruning and acks free it.
//! - **Log bytes per mailbox**: the vault log is kept forever, so this cap is generous and
//!   only stops a runaway or hostile member. Per author (the entry's signer) at most half
//!   of it, whatever the member count: one dishonest member freezes nobody, and a vault
//!   where one person proposes every payment still gets half (a 1/n share would stop a
//!   2-of-3 treasurer at a third). Enforced on appends only: a restore or relay move takes
//!   any signed history whole and counts it per author, so a dishonest member's junk keeps
//!   using its share on the new relay. The log is kept forever, so this is a lifetime
//!   limit on a relay, and a move doesn't reset it (security review 2026-11, Residuals).
//! - **Mailboxes per signing key**: creating a mailbox is free, so one key can't create
//!   more than a few (an indexed count on `mailboxes.creator`).
//! - **Database bytes** (`db_bytes`, the relay as a whole): `PRAGMA max_page_count`, so a
//!   write past it fails (507, [`Quota::Relay`]). From [`crate::NEARLY_FULL_PERCENT`],
//!   mailboxes stop being created and mailboxes younger than [`crate::YOUNG_MAILBOX_DAYS`]
//!   stop writing, so established vaults keep the rest. Without admission control (a
//!   create token or proof of work) a stranger can still fill a public relay with enough
//!   mailboxes prepared a month ahead: these caps only set how many (security review
//!   2026-11, Residuals).
//!
//! A write that would go over a cap is refused whole with
//! [`crate::RelayError::QuotaExceeded`] (HTTP 507 Insufficient Storage): nothing is
//! stored, the sender's sequence number isn't consumed. Only [`Quota::Recipient`] refuses
//! one recipient (body [`zafe_proto::relay::RECIPIENT_FULL`]); a leader skips that signer.
//!
//! Cost per request: the byte totals are running counters on the `mailboxes` row
//! (`delivery_bytes`, `log_bytes`), per (mailbox, recipient, sender) in `inbox_counts`
//! (`n`, `bytes`) and per (mailbox, author) in `log_counts`, each changed in the same
//! transaction as the rows it counts, so no request walks a table: a delivery reads one
//! counter row per recipient and its sender's rows (at most `MAX_MEMBERS`).
//!
//! Against honest traffic, per pair: a dozen small setup messages (key generation, safety
//! words, the log key), one signing request per round from the leader and one reply per
//! signer (a few KB, acked once handled), small repair parts and a padded move notice per
//! move. Per mailbox: a 64-member key generation leaves roughly 64 x 63 x 9 KB (~36 MB)
//! until it is done. Log: a payment adds about 20 KB (proposal, votes, the signed
//! transaction), about 73 MiB a year at 10 payments a day: the hosted 512 MiB lasts about
//! seven years, one author's half about three and a half.

/// Which cap a refused write hit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quota {
    /// The sender's share of one member's inbox (count or bytes): refuses that recipient
    /// only.
    Recipient,
    /// The vault's undelivered messages, or this sender's part of them.
    Deliveries,
    /// The vault log, or this author's part of it.
    Log,
    /// The signing key already created too many mailboxes.
    Mailboxes,
    /// The relay's database is at its cap ([`Quotas::db_bytes`]), or nearly and the
    /// mailbox is young.
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

/// Storage caps per mailbox. `None` is unlimited.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Quotas {
    /// Undelivered envelopes one sender may leave one recipient in one mailbox.
    pub envelopes_per_pair: Option<u64>,
    /// Bytes of undelivered envelopes one sender may leave one recipient in one mailbox.
    pub pair_bytes: Option<u64>,
    /// Total bytes of undelivered envelopes in one mailbox (a sender may use
    /// half of it, never less than `pair_bytes`).
    pub delivery_bytes: Option<u64>,
    /// Total bytes of one mailbox's vault log (an author may append up to
    /// [`author_log_cap`] of it).
    pub log_bytes: Option<u64>,
    /// Mailboxes one signing key may have created (counting ones since cancelled is fine:
    /// restarts). The app uses a fresh key per vault, so one is the norm; the per-IP
    /// creation limit ([`crate::limits::Limits::creates_per_ip`]) bounds new keys.
    pub mailboxes_per_key: Option<u64>,
    /// Bytes the relay's whole database may take (`ZAFE_RELAY_MAX_DB_MB`; no default: it
    /// depends on the host's disk).
    pub db_bytes: Option<u64>,
}

/// How much of a `cap`-byte log one author may write: half, so it takes two colluding
/// members to fill it.
pub fn author_log_cap(cap: u64) -> u64 {
    cap / 2
}

/// How much of a `cap`-byte delivery total one sender may use: half, as for the log
/// ([`author_log_cap`]), never below a pair's.
pub(crate) fn sender_delivery_cap(cap: u64, pair: Option<u64>) -> u64 {
    (cap / 2).max(pair.unwrap_or(0)).min(cap)
}

const MIB: u64 = 1024 * 1024;

impl Quotas {
    /// No quotas (tests, self-hosting on a trusted network).
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

    /// Defaults for a public relay (module docs): a pair holds 256 envelopes and 2 MiB, a
    /// vault 64 MiB of undelivered messages (they expire) and 512 MiB of log (kept for
    /// good, so sized for years of a busy vault rather than against strangers: a stranger
    /// with mailboxes prepared a month ahead fills a relay whatever this cap is).
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
