//! The client side of labelled mailboxes, `/v1/push/forget`, `GET /v1/info` and private
//! relays' create tokens (`docs/relay-move.md` §4.6, §6), against a real relay and against
//! one older than these routes.

use rand::{rngs::StdRng, SeedableRng};
use zafe_core::relay_client::{RelayClient, RelayClientError};
use zafe_proto::{
    relay::{AppendResult, PushPlatform, Reseed},
    Identity, LogEntry, LogKey, MailboxId,
};
use zafe_relay::Relay;

const VAULT: MailboxId = [1; 16];
const LABEL: MailboxId = [2; 16];
const JOIN: [u8; 32] = [5; 32];

async fn serve(router: axum::Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    format!("http://{addr}")
}

/// A relay from before these routes: it answers 404 with no body for them (axum's
/// fallback, as the hosted relay did).
async fn older_relay() -> String {
    serve(axum::Router::new().route("/health", axum::routing::get(|| async { "ok" }))).await
}

fn ids(seed: u64) -> Vec<Identity> {
    let mut rng = StdRng::seed_from_u64(seed);
    (0..3).map(|_| Identity::generate(&mut rng)).collect()
}

async fn open_mailbox(client: &RelayClient, label: MailboxId, ids: &[Identity]) {
    client
        .create_mailbox(&ids[0], label, &JOIN, ids.len() as u16)
        .await
        .unwrap();
    for id in &ids[1..] {
        client.join(id, label, JOIN).await.unwrap();
    }
    let members = ids.iter().map(|i| i.public().sig_pk).collect();
    client.seal(&ids[0], label, members).await.unwrap();
}

#[tokio::test]
async fn labelled_appends_reads_and_push_forget() {
    let client = RelayClient::new(serve(Relay::new().router()).await);
    let ids = ids(1);
    open_mailbox(&client, LABEL, &ids).await;
    let mut rng = StdRng::seed_from_u64(2);
    let key = LogKey::generate(0, &mut rng);
    let first = LogEntry::create(&ids[0], &key, VAULT, 0, [0; 32], b"a", &mut rng).unwrap();
    let second = LogEntry::create(
        &ids[0],
        &key,
        VAULT,
        1,
        first.hash().unwrap(),
        b"b",
        &mut rng,
    )
    .unwrap();

    // Another member posts the author's entries (catching up).
    for (i, e) in [&first, &second].into_iter().enumerate() {
        assert_eq!(
            client.append_log_to(&ids[1], LABEL, e).await.unwrap(),
            AppendResult::Appended { index: i as u64 }
        );
    }
    assert_eq!(
        client.append_log_to(&ids[2], LABEL, &first).await.unwrap(),
        AppendResult::Conflict { len: 2 }
    );
    let log = client.read_log(&ids[2], LABEL, 0).await.unwrap();
    assert_eq!(log, vec![first.clone(), second]);
    // The old route files by vault id: it can't reach the label.
    assert!(matches!(
        client.append_log(&first).await,
        Err(RelayClientError::Status { status: 404, .. })
    ));
    // A label this relay doesn't have is a missing mailbox, not an old relay.
    match client.append_log_to(&ids[1], [9; 16], &first).await {
        Err(RelayClientError::Status { status: 404, body }) => {
            assert_eq!(body, "unknown mailbox")
        }
        other => panic!("{other:?}"),
    }

    client
        .register_push(&ids[1], LABEL, PushPlatform::Fcm, "t".into())
        .await
        .unwrap();
    client.forget_push(&ids[1], LABEL).await.unwrap();
    client.forget_push(&ids[1], [9; 16]).await.unwrap();
}

#[tokio::test]
async fn info_from_a_relay() {
    let client = RelayClient::new(serve(Relay::new().router()).await);
    let info = client.info().await.unwrap();
    assert_eq!(info.versions.relay_api, zafe_proto::version::RELAY_API);
    assert_eq!(info.limits.max_members, zafe_relay::MAX_MEMBERS);
    assert!(!info.push && !info.create_token);
    client.health().await.unwrap();
}

#[tokio::test]
async fn an_older_relay_is_reported_as_too_old() {
    let client = RelayClient::new(older_relay().await);
    let ids = ids(3);
    let mut rng = StdRng::seed_from_u64(4);
    let key = LogKey::generate(0, &mut rng);
    let entry = LogEntry::create(&ids[0], &key, VAULT, 0, [0; 32], b"a", &mut rng).unwrap();
    for result in [
        client.info().await.map(|_| ()),
        client.forget_push(&ids[0], LABEL).await,
        client
            .append_log_to(&ids[0], LABEL, &entry)
            .await
            .map(|_| ()),
    ] {
        assert!(
            matches!(result, Err(RelayClientError::RelayTooOld { .. })),
            "{result:?}"
        );
    }
    client.health().await.unwrap();
}

#[tokio::test]
async fn a_private_relay_takes_its_create_token() {
    let base = serve(Relay::new().with_create_token(Some("s3cret")).router()).await;
    let ids = ids(5);
    let without = RelayClient::new(base.clone());
    assert!(without.info().await.unwrap().create_token);
    assert!(matches!(
        without.create_mailbox(&ids[0], LABEL, &JOIN, 3).await,
        Err(RelayClientError::CreateTokenRefused)
    ));
    let wrong = RelayClient::new(base.clone()).with_create_token(Some("guess"));
    assert!(matches!(
        wrong.create_mailbox(&ids[0], LABEL, &JOIN, 3).await,
        Err(RelayClientError::CreateTokenRefused)
    ));

    let client = RelayClient::new(base).with_create_token(Some("s3cret"));
    open_mailbox(&client, LABEL, &ids).await;
    // A restore creates a mailbox too.
    let mut rng = StdRng::seed_from_u64(6);
    let key = LogKey::generate(0, &mut rng);
    let entry = LogEntry::create(&ids[0], &key, VAULT, 0, [0; 32], b"a", &mut rng).unwrap();
    let reseed = Reseed {
        mailbox: [7; 16],
        members: ids.iter().map(|i| *i.public()).collect(),
        threshold: 2,
        from: 0,
        entries: vec![entry.to_bytes().unwrap()],
        finish: true,
        timestamp: 0,
    };
    assert!(matches!(
        without.reseed(&ids[0], reseed.clone()).await,
        Err(RelayClientError::CreateTokenRefused)
    ));
    assert!(client.reseed(&ids[0], reseed).await.unwrap().open);
    // Without the token, everything but creating still works.
    assert_eq!(
        without.read_log(&ids[1], [7; 16], 0).await.unwrap(),
        vec![entry]
    );
}
