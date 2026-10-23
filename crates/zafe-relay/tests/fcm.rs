//! FCM sender against a mock of Google's token and FCM endpoints: the OAuth JWT is signed
//! correctly, the access token is used, and the message carries no vault data.

use std::sync::{Arc, Mutex};

use axum::{extract::State, http::HeaderMap, routing::post, Form, Json, Router};
use zafe_proto::relay::PushPlatform;
use zafe_relay::{
    fcm::{FcmNotifier, ServiceAccount},
    Notifier,
};

// A key generated for this test only (tests/fixtures), never used anywhere else.
const TEST_KEY: &str = include_str!("fixtures/test-only-rsa.pem");
const TEST_PUB: &str = include_str!("fixtures/test-only-rsa.pub.pem");

#[derive(Default)]
struct Seen {
    assertion: Option<String>,
    bearer: Option<String>,
    message: Option<serde_json::Value>,
}

#[tokio::test]
async fn sends_content_free_high_priority_pushes() {
    let seen = Arc::new(Mutex::new(Seen::default()));
    let app = Router::new()
        .route(
            "/token",
            post(
                |State(seen): State<Arc<Mutex<Seen>>>,
                 Form(form): Form<std::collections::HashMap<String, String>>| async move {
                    assert_eq!(
                        form["grant_type"],
                        "urn:ietf:params:oauth:grant-type:jwt-bearer"
                    );
                    seen.lock().unwrap().assertion = Some(form["assertion"].clone());
                    Json(serde_json::json!({"access_token": "tok-123", "expires_in": 3600}))
                },
            ),
        )
        .route(
            "/v1/projects/zafe-test/messages:send",
            post(
                |State(seen): State<Arc<Mutex<Seen>>>,
                 headers: HeaderMap,
                 Json(body): Json<serde_json::Value>| async move {
                    let mut s = seen.lock().unwrap();
                    s.bearer = headers
                        .get("authorization")
                        .map(|v| v.to_str().unwrap().to_owned());
                    s.message = Some(body);
                    Json(serde_json::json!({"name": "projects/zafe-test/messages/1"}))
                },
            ),
        )
        .with_state(seen.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let account = ServiceAccount {
        project_id: "zafe-test".into(),
        client_email: "relay@zafe-test.iam.gserviceaccount.com".into(),
        private_key: TEST_KEY.into(),
        token_uri: format!("{base}/token"),
    };
    let notifier = FcmNotifier::with_endpoint(account, &base);
    notifier.notify(PushPlatform::Fcm, "device-token-1");
    notifier.notify(PushPlatform::Apns, "ignored-for-now");

    for _ in 0..100 {
        if seen.lock().unwrap().message.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let s = seen.lock().unwrap();
    assert_eq!(s.bearer.as_deref(), Some("Bearer tok-123"));
    assert_eq!(
        s.message.as_ref().unwrap(),
        &serde_json::json!({
            "message": {
                "token": "device-token-1",
                "data": { "t": "vault" },
                "android": { "priority": "high", "collapse_key": "zafe" },
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
    );

    // The assertion is an RS256 JWT for the service account, scoped to FCM.
    #[derive(serde::Deserialize)]
    struct Claims {
        iss: String,
        scope: String,
        aud: String,
    }
    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256);
    validation.set_audience(&[format!("{base}/token")]);
    let claims = jsonwebtoken::decode::<Claims>(
        s.assertion.as_ref().unwrap(),
        &jsonwebtoken::DecodingKey::from_rsa_pem(TEST_PUB.as_bytes()).unwrap(),
        &validation,
    )
    .unwrap()
    .claims;
    assert_eq!(claims.iss, "relay@zafe-test.iam.gserviceaccount.com");
    assert_eq!(
        claims.scope,
        "https://www.googleapis.com/auth/firebase.messaging"
    );
    assert_eq!(claims.aud, format!("{base}/token"));
}

#[tokio::test]
async fn unregistered_tokens_are_reported_once_fcm_says_so() {
    let app = Router::new()
        .route(
            "/token",
            post(|| async { Json(serde_json::json!({"access_token": "tok", "expires_in": 3600})) }),
        )
        .route(
            "/v1/projects/zafe-test/messages:send",
            post(|| async {
                (
                    axum::http::StatusCode::NOT_FOUND,
                    Json(serde_json::json!({"error": {
                        "code": 404,
                        "status": "NOT_FOUND",
                        "details": [{
                            "@type": "type.googleapis.com/google.firebase.fcm.v1.FcmError",
                            "errorCode": "UNREGISTERED"
                        }]
                    }})),
                )
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let account = ServiceAccount {
        project_id: "zafe-test".into(),
        client_email: "relay@zafe-test.iam.gserviceaccount.com".into(),
        private_key: TEST_KEY.into(),
        token_uri: format!("{base}/token"),
    };
    let gone = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = gone.clone();
    let notifier = FcmNotifier::with_endpoint(account, &base)
        .on_unregistered(move |t| sink.lock().unwrap().push(t.to_owned()));
    notifier.notify(PushPlatform::Fcm, "stale-device");
    for _ in 0..100 {
        if !gone.lock().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(*gone.lock().unwrap(), vec!["stale-device".to_owned()]);

    // Other failures never forget a token.
    assert!(!FcmNotifier::is_unregistered(404, "not json"));
    assert!(!FcmNotifier::is_unregistered(
        400,
        r#"{"error":{"details":[{"errorCode":"UNREGISTERED"}]}}"#
    ));
    assert!(!FcmNotifier::is_unregistered(
        404,
        r#"{"error":{"details":[{"errorCode":"SENDER_ID_MISMATCH"}]}}"#
    ));
}
