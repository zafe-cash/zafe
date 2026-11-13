//! Request limits for the hosted relay: token buckets per signing key and per client IP.
//!
//! Keys are only charged after their signature verified, so nobody can drain another
//! member's bucket. The IP limit catches floods of unauthenticated requests; behind a proxy
//! it uses the client address the proxy reports (see [`Limits::client_ip_header`]).
//! Creating a mailbox is free and keys cost nothing to make, so creations (and what
//! co-signers leave for a recovering phone) have their own, much slower per-IP bucket
//! ([`Limits::creates_per_ip`]).

use std::{collections::HashMap, hash::Hash, net::IpAddr, sync::Mutex};

/// Requests per minute and burst size of one bucket.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rate {
    pub per_minute: u32,
    pub burst: u32,
}

impl Rate {
    pub const fn new(per_minute: u32, burst: u32) -> Self {
        Self { per_minute, burst }
    }
}

/// Events per day and burst size of one bucket (for rare actions such as creating a mailbox).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DailyRate {
    pub per_day: u32,
    pub burst: u32,
}

impl DailyRate {
    pub const fn new(per_day: u32, burst: u32) -> Self {
        Self { per_day, burst }
    }
}

/// Relay limits. `None` rates are unlimited (tests, self-hosting on a trusted network).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Per signing key. The app polls every 15 s (a few requests); a leader collecting
    /// signatures reads its inbox twice a second for up to 90 s.
    pub per_key: Option<Rate>,
    /// Per client IP (many members can share one address behind NAT).
    pub per_ip: Option<Rate>,
    /// Mailboxes (vaults) created per client IP. Each vault is created once, by one member.
    /// A co-signer's seat-recovery values (announce, challenge) take from the same bucket.
    pub creates_per_ip: Option<DailyRate>,
    /// Header holding the client address when the relay runs behind a proxy (e.g.
    /// `fly-client-ip`, or `x-forwarded-for` behind Caddy, whose rightmost entry is the
    /// one the proxy added). `None`: the TCP peer address. Never trust a header a client
    /// can set directly.
    pub client_ip_header: Option<String>,
}

impl Limits {
    /// No limits.
    pub fn none() -> Self {
        Self {
            per_key: None,
            per_ip: None,
            creates_per_ip: None,
            client_ip_header: None,
        }
    }

    /// Defaults for a public relay.
    pub fn hosted() -> Self {
        Self {
            per_key: Some(Rate::new(300, 150)),
            per_ip: Some(Rate::new(1200, 600)),
            creates_per_ip: Some(DailyRate::new(20, 10)),
            client_ip_header: None,
        }
    }
}

/// Token buckets keyed by `K`, refilled from a clock in seconds.
pub(crate) struct Buckets<K> {
    per_sec: f64,
    burst: f64,
    state: Mutex<HashMap<K, (f64, u64)>>,
}

impl<K: Eq + Hash> Buckets<K> {
    pub(crate) fn new(rate: Rate) -> Self {
        Self::with(f64::from(rate.per_minute) / 60.0, rate.burst)
    }

    pub(crate) fn daily(rate: DailyRate) -> Self {
        Self::with(f64::from(rate.per_day) / 86_400.0, rate.burst)
    }

    fn with(per_sec: f64, burst: u32) -> Self {
        Self {
            per_sec,
            burst: f64::from(burst.max(1)),
            state: Mutex::new(HashMap::new()),
        }
    }

    /// Takes one token for `key` at time `now` (seconds). On `Err`, the seconds until one
    /// token is available again.
    pub(crate) fn take(&self, key: K, now: u64) -> Result<(), u64> {
        let (per_sec, burst) = (self.per_sec, self.burst);
        let mut state = self.state.lock().expect("lock");
        let (tokens, last) = state.entry(key).or_insert((burst, now));
        *tokens = (*tokens + now.saturating_sub(*last) as f64 * per_sec).min(burst);
        *last = now;
        if *tokens >= 1.0 {
            *tokens -= 1.0;
            Ok(())
        } else if per_sec > 0.0 {
            Err(((1.0 - *tokens) / per_sec).ceil().max(1.0) as u64)
        } else {
            Err(60)
        }
    }

    /// Drops buckets that have refilled completely (idle keys), so memory stays bounded.
    pub(crate) fn prune(&self, now: u64) {
        let (per_sec, burst) = (self.per_sec, self.burst);
        self.state
            .lock()
            .expect("lock")
            .retain(|_, (tokens, last)| {
                *tokens + now.saturating_sub(*last) as f64 * per_sec < burst
            });
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.state.lock().expect("lock").len()
    }
}

/// The client address from the configured proxy header (rightmost entry) or the peer.
pub(crate) fn client_ip(
    headers: &axum::http::HeaderMap,
    header: Option<&str>,
    peer: Option<IpAddr>,
) -> Option<IpAddr> {
    match header {
        Some(name) => headers
            .get(name)?
            .to_str()
            .ok()?
            .rsplit(',')
            .next()?
            .trim()
            .parse()
            .ok(),
        None => peer,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bucket_allows_a_burst_then_refills() {
        let b = Buckets::new(Rate::new(60, 3));
        assert!(b.take("k", 100).is_ok());
        assert!(b.take("k", 100).is_ok());
        assert!(b.take("k", 100).is_ok());
        assert_eq!(b.take("k", 100), Err(1));
        // One token per second at 60/min.
        assert!(b.take("k", 101).is_ok());
        assert!(b.take("k", 101).is_err());
        // Other keys have their own bucket.
        assert!(b.take("other", 101).is_ok());
        // Never more than the burst after a long pause.
        for _ in 0..3 {
            assert!(b.take("k", 10_000).is_ok());
        }
        assert!(b.take("k", 10_000).is_err());
    }

    #[test]
    fn daily_bucket_refills_slowly() {
        let b = Buckets::daily(DailyRate::new(24, 2));
        assert!(b.take("ip", 0).is_ok());
        assert!(b.take("ip", 0).is_ok());
        // 24 a day is one an hour.
        assert!(matches!(b.take("ip", 0), Err(3600..=3601)));
        assert!(matches!(b.take("ip", 1800), Err(1800..=1801)));
        assert!(b.take("ip", 3600).is_ok());
        assert!(b.take("ip", 3600).is_err());
    }

    #[test]
    fn idle_buckets_are_pruned() {
        let b = Buckets::new(Rate::new(60, 3));
        b.take("busy", 100).unwrap();
        b.take("idle", 0).unwrap();
        b.prune(100);
        assert_eq!(b.len(), 1);
    }

    #[test]
    fn client_ip_comes_from_the_configured_header_only() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-forwarded-for", "6.6.6.6, 203.0.113.9".parse().unwrap());
        let peer = Some("10.0.0.1".parse().unwrap());
        assert_eq!(
            client_ip(&headers, Some("x-forwarded-for"), peer),
            Some("203.0.113.9".parse().unwrap()),
            "the rightmost entry is the one the proxy added"
        );
        assert_eq!(client_ip(&headers, None, peer), peer, "headers ignored");
        assert_eq!(client_ip(&headers, Some("fly-client-ip"), peer), None);
    }
}
