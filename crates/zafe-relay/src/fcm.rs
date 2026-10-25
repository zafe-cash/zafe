//! Firebase Cloud Messaging (HTTP v1) sender for content-free "vault activity" pushes.
//!
//! The relay is blind, so a push carries no data about the vault: it only wakes the app,
//! which then reads the encrypted log itself. Messages are data-only, high priority, and
//! share one collapse key, so a burst of log entries wakes a device once.
//!
//! Auth: the service-account key (the JSON file from the Firebase console) signs a short
//! RS256 JWT, exchanged at Google's token endpoint for an OAuth access token, cached until
//! shortly before it expires.

use std::{sync::Arc, time::Duration};

use serde::Deserialize;
use tokio::sync::Mutex;
use zafe_proto::relay::PushPlatform;

use crate::Notifier;

const SCOPE: &str = "https://www.googleapis.com/auth/firebase.messaging";
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
        }
    }

    /// Called with a device token FCM reports as no longer registered (app uninstalled,
    /// token rotated), so the relay stops pushing to it.
    pub fn on_unregistered(mut self, f: impl Fn(&str) + Send + Sync + 'static) -> Self {
        self.on_unregistered = Some(Arc::new(f));
        self
    }

    /// Whether an FCM v1 error response says the device token is gone for good.
    pub fn is_unregistered(status: u16, body: &str) -> bool {
        status == 404
            && serde_json::from_str::<serde_json::Value>(body).is_ok_and(|v| {
                v["error"]["details"].as_array().is_some_and(|d| {
                    d.iter()
                        .any(|e| e["errorCode"].as_str() == Some("UNREGISTERED"))
                })
            })
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

    /// The FCM message: data only, no vault information, high priority (Android) or a silent
    /// background push (iOS), one collapse key.
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
                "android": { "priority": "high", "collapse_key": "zafe" },
                // iOS: FCM hands this to APNs as a silent (background) push, which wakes
                // the app to read the log itself. Priority 5 is what Apple allows for it
                // (10 is for alerts); the system may still delay or drop silent pushes.
                "apns": {
                    "headers": {
                        "apns-push-type": "background",
                        "apns-priority": "5",
                        "apns-collapse-id": "zafe"
                    },
                    "payload": { "aps": { "content-available": 1 } }
                }
            }
        })
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
        let (http, account, cache) = (self.http.clone(), self.account.clone(), self.token.clone());
        let url = format!(
            "{}/v1/projects/{}/messages:send",
            self.fcm_base, account.project_id
        );
        let body = Self::message(platform, token);
        let (device_token, on_unregistered) = (token.to_owned(), self.on_unregistered.clone());
        self.runtime.spawn(async move {
            let result = async {
                let bearer = Self::access_token(&http, &account, &cache).await?;
                let resp = http
                    .post(&url)
                    .bearer_auth(bearer)
                    .json(&body)
                    .send()
                    .await
                    .map_err(|e| format!("send: {e}"))?;
                let status = resp.status();
                if !status.is_success() {
                    let text = resp.text().await.unwrap_or_default();
                    if Self::is_unregistered(status.as_u16(), &text) {
                        tracing::info!("push: token unregistered, forgetting it");
                        if let Some(f) = &on_unregistered {
                            f(&device_token);
                        }
                        return Ok(());
                    }
                    return Err(format!("FCM {status}: {text}"));
                }
                tracing::debug!("push: sent");
                Ok(())
            }
            .await;
            if let Err(e) = result {
                tracing::warn!("push failed: {e}");
            }
        });
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

    #[test]
    fn the_ios_push_is_silent_and_carries_nothing_visible() {
        let m = FcmNotifier::message(PushPlatform::FcmFid, "f1");
        let apns = &m["message"]["apns"];
        assert_eq!(apns["headers"]["apns-push-type"], "background");
        assert_eq!(
            apns["payload"]["aps"],
            serde_json::json!({ "content-available": 1 })
        );
        // No alert, sound or badge: the app decides what to show after reading the log.
        assert!(apns["payload"]["aps"].get("alert").is_none());
    }
}
