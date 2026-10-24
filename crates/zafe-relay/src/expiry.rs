//! Relays own their data (`docs/relay-move.md` §6): every phone keeps the vault's history,
//! so a relay's copy is a cache, and mailboxes nobody uses are deleted by
//! [`crate::Relay::expire`]. Two rules, each `None` = off:
//!
//! - **Idle**: no member request for [`Expiry::idle_days`] (hosted: a year). The app's
//!   background checks keep a vault in use alive.
//! - **Unused**: no member other than the mailbox's creator made a request within
//!   [`Expiry::unused_days`] of its creation (hosted: 30 days). That is a restore or move
//!   that nobody followed, or a setup nobody joined. A restored vault that members only read
//!   stays: reads count, appends aren't needed.
//!
//! A member request is a signed request by a member of the mailbox (for `/v1/log/append`
//! and `/v1/envelope`, the entry's author or the envelope's sender). `/v1/push/forget`
//! doesn't count: a phone leaving the relay isn't use. Requests are kept per day only:
//! `mailboxes.last_seen_day` (days since the Unix epoch) is written at most once per mailbox
//! per day, and `shared` once, so reads add no write under the database mutex the rest of
//! the day, and the relay keeps no finer record of when members are active.
//!
//! Expiry deletes the mailbox and every row stored for it; the storage counters live on
//! the mailbox row and go with it. Off in [`crate::Relay::new`], on in the `zafe-relay`
//! binary ([`Expiry::hosted`]).

/// Seconds in a day (last-seen and creation are kept as day numbers).
pub const DAY_SECS: u64 = 86_400;

/// When [`crate::Relay::expire`] deletes a mailbox. `None` is never.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Expiry {
    /// Days without any member request.
    pub idle_days: Option<u32>,
    /// Days after creation within which a member other than the creator must make a request.
    pub unused_days: Option<u32>,
}

impl Expiry {
    /// No expiry (tests, development).
    pub const fn none() -> Self {
        Self {
            idle_days: None,
            unused_days: None,
        }
    }

    /// Defaults for a public relay.
    pub const fn hosted() -> Self {
        Self {
            idle_days: Some(365),
            unused_days: Some(30),
        }
    }

    pub fn is_off(&self) -> bool {
        self.idle_days.is_none() && self.unused_days.is_none()
    }
}

/// The day number of `secs` since the Unix epoch.
pub(crate) fn day(secs: u64) -> i64 {
    i64::try_from(secs / DAY_SECS).unwrap_or(i64::MAX)
}
