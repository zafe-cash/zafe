//! Firebase Cloud Messaging (HTTP v1) sender for content-free "vault activity" pushes.
//!
//! The relay is blind, so a push carries no data about the vault: it only wakes the app,
//! which then reads the encrypted log itself. Messages are data-only, high priority, and
//! share one collapse key, so a burst of log entries wakes a device once.
//!
//! Auth: the service-account key (the JSON file from the Firebase console) signs a short
//! RS256 JWT, exchanged at Google's token endpoint for an OAuth access token, cached until
//! shortly before it expires.
//!
//! Sends go through a queue that holds each token once and at most [`MAX_IN_FLIGHT`]
//! workers, so a burst across vaults never opens unbounded connections and a queued push
//! is never dropped (the relay already sends a token at most one push per window,
//! [`crate::push`]).

use std::{
    collections::{HashSet, VecDeque},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use serde::Deserialize;
use tokio::sync::Mutex;
use zafe_proto::relay::PushPlatform;

use crate::Notifier;

const SCOPE: &str = "https://www.googleapis.com/auth/firebase.messaging";

/// Most pushes in flight to FCM at once.
pub const MAX_IN_FLIGHT: usize = 32;
const FCM_BASE: &str = "https://fcm.googleapis.com";

#[derive(Clone, Deserialize)]
pub struct ServiceAccount {
    pub project_id: String,
    pub client_email: String,
    pub private_key: String,
    #[serde(default = "default_token_uri")]
    pub token_uri: String,
}

fn default_token_uri() -> String {
    "https://oauth2.googleapis.com/token".into()
}

impl ServiceAccount {
    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| format!("service account JSON: {e}"))
    }
}

struct CachedToken {
    value: String,
    expires_at: u64,
}

/// Sends FCM pushes; APNs tokens are only logged until the APNs sender lands.
pub struct FcmNotifier {
    account: ServiceAccount,
    fcm_base: String,
    http: reqwest::Client,
    token: Arc<Mutex<Option<CachedToken>>>,
    runtime: tokio::runtime::Handle,
    on_unregistered: Option<OnUnregistered>,
    queue: Arc<Queue>,
}

/// Queued pushes in order, and the tokens among them.
type Waiting = (VecDeque<(PushPlatform, String)>, HashSet<String>);

/// Pushes waiting for a worker: each token once (a later push to a queued token is the
/// same push), and how many workers run.
#[derive(Default)]
struct Queue {
    waiting: std::sync::Mutex<Waiting>,
    workers: AtomicUsize,
}

/// Callback for device tokens FCM reports as gone.
type OnUnregistered = Arc<dyn Fn(&str) + Send + Sync>;

impl FcmNotifier {
    /// Must be created inside the relay's tokio runtime (pushes are sent on it).
    pub fn new(account: ServiceAccount) -> Self {
        Self::with_endpoint(account, FCM_BASE)
    }

    /// Custom FCM base URL (tests).
    pub fn with_endpoint(account: ServiceAccount, fcm_base: &str) -> Self {
        Self {
            account,
            fcm_base: fcm_base.trim_end_matches('/').to_owned(),
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .expect("http client"),
            token: Arc::new(Mutex::new(None)),
            runtime: tokio::runtime::Handle::current(),
            on_unregistered: None,
            queue: Arc::new(Queue::default()),
        }
    }

    /// Called with a device token FCM reports as no longer registered (app uninstalled,
    /// token rotated), so the relay stops pushing to it.
    pub fn on_unregistered(mut self, f: impl Fn(&str) + Send + Sync + 'static) -> Self {
        self.on_unregistered = Some(Arc::new(f));
        self
    }

    /// Whether an FCM v1 error response says the device token will never work for this
    /// relay (FCM v1 `ErrorCode` reference):
    /// - 404 `UNREGISTERED`: "App instance was unregistered from FCM";
    /// - 403 `SENDER_ID_MISMATCH`: "The authenticated sender ID is different from the
    ///   sender ID for the registration token";
    /// - 400 `INVALID_ARGUMENT` only when its `google.rpc.BadRequest` names the token field
    ///   (`message.token` / `message.fid`): "An extension of type `google.rpc.BadRequest`
    ///   is returned to specify which field was invalid", and "INVALID_ARGUMENT can also be
    ///   returned in cases of issues in the message payload" (FCM "Manage tokens").
    pub fn is_unregistered(status: u16, body: &str) -> bool {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(body) else {
            return false;
        };
        let details = v["error"]["details"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let code = |want: &str| {
            details
                .iter()
                .any(|e| e["errorCode"].as_str() == Some(want))
        };
        match status {
            404 => code("UNREGISTERED"),
            403 => code("SENDER_ID_MISMATCH"),
            400 => details.iter().any(|e| {
                e["@type"].as_str() == Some("type.googleapis.com/google.rpc.BadRequest")
                    && e["fieldViolations"].as_array().is_some_and(|f| {
                        f.iter().any(|f| {
                            matches!(f["field"].as_str(), Some("message.token" | "message.fid"))
                        })
                    })
            }),
            _ => false,
        }
    }

    async fn access_token(
        http: &reqwest::Client,
        account: &ServiceAccount,
        cache: &Mutex<Option<CachedToken>>,
    ) -> Result<String, String> {
        let now = unix_now();
        let mut cache = cache.lock().await;
        if let Some(t) = cache.as_ref().filter(|t| t.expires_at > now + 60) {
            return Ok(t.value.clone());
        }
        #[derive(serde::Serialize)]
        struct Claims<'a> {
            iss: &'a str,
            scope: &'a str,
            aud: &'a str,
            iat: u64,
            exp: u64,
        }
        let key = jsonwebtoken::EncodingKey::from_rsa_pem(account.private_key.as_bytes())
            .map_err(|e| format!("service account key: {e}"))?;
        let assertion = jsonwebtoken::encode(
            &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256),
            &Claims {
                iss: &account.client_email,
                scope: SCOPE,
                aud: &account.token_uri,
                iat: now,
                exp: now + 3600,
            },
            &key,
        )
        .map_err(|e| format!("sign JWT: {e}"))?;
        #[derive(Deserialize)]
        struct TokenResponse {
            access_token: String,
            expires_in: u64,
        }
        let resp: TokenResponse = http
            .post(&account.token_uri)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                ("assertion", assertion.as_str()),
            ])
            .send()
            .await
            .map_err(|e| format!("token request: {e}"))?
            .error_for_status()
            .map_err(|e| format!("token request: {e}"))?
            .json()
            .await
            .map_err(|e| format!("token response: {e}"))?;
        *cache = Some(CachedToken {
            value: resp.access_token.clone(),
            expires_at: now + resp.expires_in,
        });
        Ok(resp.access_token)
    }

    /// The FCM message: data only, no vault information, high priority, one collapse key.
    /// An installation ID goes in `fid`, an older registration token in `token`.
    pub fn message(platform: PushPlatform, target: &str) -> serde_json::Value {
        let field = if platform == PushPlatform::FcmFid {
            "fid"
        } else {
            "token"
        };
        serde_json::json!({
            "message": {
                field: target,
                "data": { "t": "vault" },
                "android": { "priority": "high", "collapse_key": "zafe" }
            }
        })
    }
}

impl FcmNotifier {
    /// Sends one push and reports a gone token.
    async fn send(
        http: &reqwest::Client,
        account: &ServiceAccount,
        cache: &Mutex<Option<CachedToken>>,
        url: &str,
        platform: PushPlatform,
        token: &str,
        on_unregistered: Option<&OnUnregistered>,
    ) -> Result<(), String> {
        let bearer = Self::access_token(http, account, cache).await?;
        let resp = http
            .post(url)
            .bearer_auth(bearer)
            .json(&Self::message(platform, token))
            .send()
            .await
            .map_err(|e| format!("send: {e}"))?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            if Self::is_unregistered(status.as_u16(), &text) {
                tracing::info!("push: token gone, forgetting it");
                if let Some(f) = on_unregistered {
                    f(token);
                }
                return Ok(());
            }
            return Err(format!("FCM {status}: {text}"));
        }
        tracing::debug!("push: sent");
        Ok(())
    }

    /// Starts a worker when fewer than [`MAX_IN_FLIGHT`] run; each sends queued pushes
    /// until the queue is empty.
    fn spawn_worker(&self) {
        let queue = self.queue.clone();
        if queue
            .workers
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
                (n < MAX_IN_FLIGHT).then_some(n + 1)
            })
            .is_err()
        {
            return; // enough workers; one of them takes it
        }
        let (http, account, cache) = (self.http.clone(), self.account.clone(), self.token.clone());
        let url = format!(
            "{}/v1/projects/{}/messages:send",
            self.fcm_base, account.project_id
        );
        let on_unregistered = self.on_unregistered.clone();
        self.runtime.spawn(async move {
            loop {
                let next = {
                    let mut waiting = queue.waiting.lock().expect("lock");
                    let next = waiting.0.pop_front();
                    if let Some((_, token)) = &next {
                        waiting.1.remove(token);
                    } else {
                        // Under the queue's lock: a push queued after this sees one fewer
                        // worker and starts its own.
                        queue.workers.fetch_sub(1, Ordering::SeqCst);
                    }
                    next
                };
                let Some((platform, token)) = next else {
                    return;
                };
                let sent = Self::send(
                    &http,
                    &account,
                    &cache,
                    &url,
                    platform,
                    &token,
                    on_unregistered.as_ref(),
                )
                .await;
                if let Err(e) = sent {
                    tracing::warn!("push failed: {e}");
                }
            }
        });
    }
}

impl Notifier for FcmNotifier {
    fn notify(&self, platform: PushPlatform, token: &str) {
        if !matches!(platform, PushPlatform::Fcm | PushPlatform::FcmFid) {
            tracing::debug!(
                platform = platform.as_str(),
                "push: no sender for platform yet"
            );
            return;
        }
        {
            let mut waiting = self.queue.waiting.lock().expect("lock");
            if !waiting.1.insert(token.to_owned()) {
                return; // already queued: that push covers this one
            }
            waiting.0.push_back((platform, token.to_owned()));
        }
        self.spawn_worker();
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_installation_id_goes_in_fid_and_a_token_in_token() {
        let fid = FcmNotifier::message(PushPlatform::FcmFid, "f1");
        assert_eq!(fid["message"]["fid"], "f1");
        assert!(fid["message"].get("token").is_none());
        let token = FcmNotifier::message(PushPlatform::Fcm, "t1");
        assert_eq!(token["message"]["token"], "t1");
        assert!(token["message"].get("fid").is_none());
        // No vault information either way.
        assert_eq!(fid["message"]["data"], serde_json::json!({ "t": "vault" }));
    }
}
