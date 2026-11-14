//! Security review 2026-11, batch 4 (rule 7, "Relay server"): the client never follows a
//! relay's redirect to another host. reqwest's default policy follows up to 10 redirects
//! (`reqwest-0.12.28/src/redirect.rs:161-163`), resends the body on 307/308
//! (`tower-http-0.6.11/src/follow_redirect/mod.rs:291`) and strips only the standard
//! credential headers across hosts (`redirect.rs:239-250`), so a relay could send a phone's
//! signed requests, and the `zafe-create-token` header, to any address, plain `http` or the
//! phone's own network included. Through Tor no redirect is followed.

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use axum::{
    http::{header::LOCATION, StatusCode},
    response::IntoResponse,
    routing::any,
    Router,
};
use rand::SeedableRng;
use zafe_core::relay_client::RelayClient;

async fn serve(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

/// Another host (same address, another port: reqwest's own test for "cross host" is host
/// or port) that counts what reaches it.
async fn elsewhere() -> (String, Arc<AtomicUsize>) {
    let hits = Arc::new(AtomicUsize::new(0));
    let seen = hits.clone();
    let app = Router::new().fallback(any(move || {
        let seen = seen.clone();
        async move {
            seen.fetch_add(1, Ordering::SeqCst);
            StatusCode::OK
        }
    }));
    (serve(app).await, hits)
}

/// A relay that redirects every request to `to` with `status`.
async fn redirecting(status: StatusCode, to: String) -> String {
    let app = Router::new().fallback(any(move || {
        let to = to.clone();
        async move { (status, [(LOCATION, format!("{to}/v1/mailbox/members"))]).into_response() }
    }));
    serve(app).await
}

#[tokio::test]
async fn a_relay_cant_redirect_requests_to_another_host() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(1);
    let me = zafe_proto::Identity::generate(&mut rng);
    for status in [
        StatusCode::TEMPORARY_REDIRECT,
        StatusCode::PERMANENT_REDIRECT,
        StatusCode::FOUND,
        StatusCode::SEE_OTHER,
    ] {
        let (other, hits) = elsewhere().await;
        let relay = RelayClient::new(redirecting(status, other).await);
        let result = relay.members(&me, [1; 16]).await;
        assert_eq!(
            hits.load(Ordering::SeqCst),
            0,
            "{status}: the request was sent on to another host ({result:?})"
        );
        assert!(result.is_err(), "{status}: a redirect isn't an answer");
    }
}
