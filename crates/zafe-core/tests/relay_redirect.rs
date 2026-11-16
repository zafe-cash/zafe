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
