//! `zafe-relay`: runs the relay.
//!
//! - `ZAFE_RELAY_LISTEN`: listen address (default `127.0.0.1:8787`; if unset and `PORT` is
//!   set, `0.0.0.0:$PORT`, as container platforms expect)
//! - `ZAFE_RELAY_DB`: SQLite file (default `zafe-relay.sqlite`; `:memory:` for none)
//! - `ZAFE_FCM_SERVICE_ACCOUNT`: path to a Firebase service-account JSON key; enables
//!   Android pushes (otherwise pushes are only logged)
//! - `ZAFE_FCM_SERVICE_ACCOUNT_JSON`: the same key inline (for secret env vars); wins
//! - `PORT`: see `ZAFE_RELAY_LISTEN`
//! - `ZAFE_RELAY_LIMITS`: `off` disables rate limits (default: on, [`Limits::hosted`])
//! - `ZAFE_RELAY_KEY_RATE` / `ZAFE_RELAY_IP_RATE`: requests per minute per signing key /
//!   client IP (burst: half of it); `0` turns that limit off
//! - `ZAFE_RELAY_CREATES_PER_DAY`: mailboxes (vaults) created per client IP per day
//!   (burst: half of it; default 20); `0` turns it off
//! - `ZAFE_RELAY_CLIENT_IP_HEADER`: behind a proxy, the header carrying the client address
//!   (`fly-client-ip` on Fly.io, `x-forwarded-for` behind Caddy). Unset: the TCP peer
//! - `ZAFE_RELAY_QUOTAS`: `off` disables storage quotas (default: on, [`Quotas::hosted`]:
//!   10,000 undelivered envelopes per recipient, 256 MiB of undelivered envelopes and
//!   512 MiB of vault log per mailbox). Over a quota the relay answers 507
//! - `ZAFE_RELAY_MAX_INBOX`: undelivered envelopes per recipient in a mailbox
//! - `ZAFE_RELAY_MAX_DELIVERY_MB` / `ZAFE_RELAY_MAX_LOG_MB`: MiB of undelivered envelopes /
//!   of vault log per mailbox
//! - `ZAFE_RELAY_MAX_VAULTS_PER_KEY`: mailboxes one signing key may create (default 8).
//!   `0` turns that quota off, as for the three before it
//!
//! [`Limits::hosted`]: zafe_relay::limits::Limits::hosted
//! [`Quotas::hosted`]: zafe_relay::quota::Quotas::hosted

use std::{net::SocketAddr, path::Path, time::Duration};

use zafe_relay::{
    limits::{DailyRate, Limits, Rate},
    quota::Quotas,
};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        // Plain text for Docker / systemd / Fly log collectors.
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stdout()))
        .init();
    let addr = listen_addr(
        std::env::var("ZAFE_RELAY_LISTEN").ok(),
        std::env::var("PORT").ok(),
    );
    let db = std::env::var("ZAFE_RELAY_DB").unwrap_or_else(|_| "zafe-relay.sqlite".into());
    let relay = if db == ":memory:" {
        zafe_relay::Relay::new()
    } else {
        zafe_relay::Relay::open(Path::new(&db)).map_err(std::io::Error::other)?
    };

    // The key comes from a secret file, or inline from a secret env var (platforms such as
    // Fly.io expose secrets as env vars). Never bake it into an image.
    let fcm_json = match (
        std::env::var("ZAFE_FCM_SERVICE_ACCOUNT_JSON"),
        std::env::var("ZAFE_FCM_SERVICE_ACCOUNT"),
    ) {
        (Ok(json), _) if !json.trim().is_empty() => Ok(json),
        (_, Ok(path)) if !path.is_empty() => Ok(std::fs::read_to_string(&path)?),
        _ => Err(()),
    };
    let relay = match fcm_json {
        Ok(json) => {
            let account =
                zafe_relay::fcm::ServiceAccount::from_json(&json).map_err(std::io::Error::other)?;
            tracing::info!("FCM pushes enabled for project {}", account.project_id);
            let store = relay.clone();
            let fcm = zafe_relay::fcm::FcmNotifier::new(account).on_unregistered(move |token| {
                if let Err(e) = store.forget_push_token(token) {
                    tracing::warn!("forgetting a push token failed: {e}");
                }
            });
            relay.with_notifier(std::sync::Arc::new(fcm))
        }
        Err(_) => relay,
    };

    let limits = limits_from_env(|name| std::env::var(name).ok());
    tracing::info!(
        "limits: per key {:?}, per IP {:?}, creations per IP {:?}, client IP from {}",
        limits.per_key,
        limits.per_ip,
        limits.creates_per_ip,
        limits.client_ip_header.as_deref().unwrap_or("the TCP peer")
    );
    let quotas = quotas_from_env(|name| std::env::var(name).ok());
    tracing::info!(
        "quotas: envelopes per recipient {:?}, delivery bytes {:?}, log bytes {:?}, vaults per key {:?}",
        quotas.envelopes_per_recipient,
        quotas.delivery_bytes,
        quotas.log_bytes,
        quotas.mailboxes_per_key
    );
    let relay = relay.with_limits(limits).with_quotas(quotas);

    // Hourly retention pruning of old undelivered envelopes.
    let pruner = relay.clone();
    tokio::spawn(async move {
        loop {
            match pruner.prune() {
                Ok(n) if n > 0 => tracing::info!("pruned {n} expired deliveries"),
                Ok(_) => {}
                Err(e) => tracing::warn!("prune failed: {e}"),
            }
            tokio::time::sleep(Duration::from_secs(3600)).await;
        }
    });

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("zafe-relay listening on {addr}, db {db}");
    axum::serve(
        listener,
        relay
            .router()
            .into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
}

/// Rate limits from the environment (see the module docs).
fn limits_from_env(var: impl Fn(&str) -> Option<String>) -> Limits {
    if var("ZAFE_RELAY_LIMITS").is_some_and(|v| v.eq_ignore_ascii_case("off")) {
        return Limits::none();
    }
    let mut limits = Limits::hosted();
    let rate = |name: &str, default: Option<Rate>| match var(name).map(|v| v.parse::<u32>()) {
        Some(Ok(0)) => None,
        Some(Ok(n)) => Some(Rate::new(n, (n / 2).max(1))),
        _ => default,
    };
    limits.per_key = rate("ZAFE_RELAY_KEY_RATE", limits.per_key);
    limits.per_ip = rate("ZAFE_RELAY_IP_RATE", limits.per_ip);
    limits.creates_per_ip = match var("ZAFE_RELAY_CREATES_PER_DAY").map(|v| v.trim().parse::<u32>())
    {
        Some(Ok(0)) => None,
        Some(Ok(n)) => Some(DailyRate::new(n, (n / 2).max(1))),
        _ => limits.creates_per_ip,
    };
    limits.client_ip_header = var("ZAFE_RELAY_CLIENT_IP_HEADER")
        .map(|h| h.trim().to_ascii_lowercase())
        .filter(|h| !h.is_empty());
    limits
}

/// Storage quotas from the environment (see the module docs).
fn quotas_from_env(var: impl Fn(&str) -> Option<String>) -> Quotas {
    if var("ZAFE_RELAY_QUOTAS").is_some_and(|v| v.eq_ignore_ascii_case("off")) {
        return Quotas::none();
    }
    let mut quotas = Quotas::hosted();
    let value = |name: &str, scale: u64, default: Option<u64>| match var(name)
        .map(|v| v.trim().parse::<u64>())
    {
        Some(Ok(0)) => None,
        Some(Ok(n)) => Some(n.saturating_mul(scale)),
        _ => default,
    };
    quotas.envelopes_per_recipient =
        value("ZAFE_RELAY_MAX_INBOX", 1, quotas.envelopes_per_recipient);
    quotas.delivery_bytes = value("ZAFE_RELAY_MAX_DELIVERY_MB", 1 << 20, quotas.delivery_bytes);
    quotas.log_bytes = value("ZAFE_RELAY_MAX_LOG_MB", 1 << 20, quotas.log_bytes);
    quotas.mailboxes_per_key = value("ZAFE_RELAY_MAX_VAULTS_PER_KEY", 1, quotas.mailboxes_per_key);
    quotas
}

/// `ZAFE_RELAY_LISTEN` wins; otherwise `PORT` (Fly.io, Cloud Run, ...) on all interfaces;
/// otherwise the local development default.
fn listen_addr(listen: Option<String>, port: Option<String>) -> String {
    match (listen, port) {
        (Some(listen), _) if !listen.is_empty() => listen,
        (_, Some(port)) if !port.is_empty() => format!("0.0.0.0:{port}"),
        _ => "127.0.0.1:8787".into(),
    }
}

/// Ctrl-C, or SIGTERM from Docker / systemd / Fly when stopping the service.
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
    tracing::info!("shutting down");
}

#[cfg(test)]
mod tests {
    use super::{limits_from_env, listen_addr, quotas_from_env};
    use zafe_relay::{
        limits::{DailyRate, Limits, Rate},
        quota::Quotas,
    };

    fn env(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |name: &str| {
            pairs
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn quotas_from_the_environment() {
        assert_eq!(quotas_from_env(env(&[])), Quotas::hosted());
        assert_eq!(
            quotas_from_env(env(&[("ZAFE_RELAY_QUOTAS", "off")])),
            Quotas::none()
        );
        let q = quotas_from_env(env(&[
            ("ZAFE_RELAY_MAX_INBOX", "50"),
            ("ZAFE_RELAY_MAX_DELIVERY_MB", "0"),
            ("ZAFE_RELAY_MAX_LOG_MB", "2"),
            ("ZAFE_RELAY_MAX_VAULTS_PER_KEY", "3"),
        ]));
        assert_eq!(q.envelopes_per_recipient, Some(50));
        assert_eq!(q.delivery_bytes, None);
        assert_eq!(q.log_bytes, Some(2 * 1024 * 1024));
        assert_eq!(q.mailboxes_per_key, Some(3));
        let q = quotas_from_env(env(&[("ZAFE_RELAY_MAX_INBOX", "lots")]));
        assert_eq!(q, Quotas::hosted(), "unparsable values keep the default");
    }

    #[test]
    fn limits_from_the_environment() {
        assert_eq!(limits_from_env(env(&[])), Limits::hosted());
        assert_eq!(
            limits_from_env(env(&[("ZAFE_RELAY_LIMITS", "OFF")])),
            Limits::none()
        );
        let l = limits_from_env(env(&[
            ("ZAFE_RELAY_KEY_RATE", "100"),
            ("ZAFE_RELAY_IP_RATE", "0"),
            ("ZAFE_RELAY_CLIENT_IP_HEADER", "Fly-Client-IP"),
            ("ZAFE_RELAY_CREATES_PER_DAY", "6"),
        ]));
        assert_eq!(l.per_key, Some(Rate::new(100, 50)));
        assert_eq!(l.per_ip, None);
        assert_eq!(l.creates_per_ip, Some(DailyRate::new(6, 3)));
        assert_eq!(l.client_ip_header.as_deref(), Some("fly-client-ip"));
    }

    #[test]
    fn listen_address_precedence() {
        assert_eq!(listen_addr(None, None), "127.0.0.1:8787");
        assert_eq!(listen_addr(None, Some("8080".into())), "0.0.0.0:8080");
        assert_eq!(
            listen_addr(Some("127.0.0.1:9000".into()), Some("8080".into())),
            "127.0.0.1:9000"
        );
        assert_eq!(listen_addr(Some(String::new()), None), "127.0.0.1:8787");
    }
}
