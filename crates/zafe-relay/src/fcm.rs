use std::{
    collections::{HashSet, VecDeque},
    sync::Arc,
    time::Duration,
};

use serde::Deserialize;
use tokio::sync::Mutex;
use zafe_proto::relay::PushPlatform;

use crate::Notifier;

const SCOPE: &str = "https://www.googleapis.com/auth/firebase.messaging";

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

pub struct FcmNotifier {
    account: ServiceAccount,
    fcm_base: String,
    http: reqwest::Client,
    token: Arc<Mutex<Option<CachedToken>>>,
    runtime: tokio::runtime::Handle,
    on_unregistered: Option<OnUnregistered>,
    queue: Arc<Queue>,
}

#[derive(Default)]
struct Waiting {
    pushes: VecDeque<(PushPlatform, String)>,
    tokens: HashSet<String>,
    workers: usize,
}

#[derive(Default)]
struct Queue {
    waiting: std::sync::Mutex<Waiting>,
}

type OnUnregistered = Arc<dyn Fn(&str) + Send + Sync>;

impl FcmNotifier {
    pub fn new(account: ServiceAccount) -> Self {
        Self::with_endpoint(account, FCM_BASE)
    }

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

    pub fn on_unregistered(mut self, f: impl Fn(&str) + Send + Sync + 'static) -> Self {
        self.on_unregistered = Some(Arc::new(f));
        self
    }

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

    fn spawn_worker(&self) {
        let queue = self.queue.clone();
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
                    let next = waiting.pushes.pop_front();
                    if let Some((_, token)) = &next {
                        waiting.tokens.remove(token);
                    } else {
                        waiting.workers -= 1;
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
        let start = {
            let mut waiting = self.queue.waiting.lock().expect("lock");
            if !waiting.tokens.insert(token.to_owned()) {
                return;
            }
            waiting.pushes.push_back((platform, token.to_owned()));
            let start = waiting.workers < MAX_IN_FLIGHT;
            if start {
                waiting.workers += 1;
            }
            start
        };
        if start {
            self.spawn_worker();
        }
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
        assert_eq!(fid["message"]["data"], serde_json::json!({ "t": "vault" }));
    }
}
