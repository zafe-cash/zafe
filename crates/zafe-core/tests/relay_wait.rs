//! `RelayClient::wait_for_activity` over real HTTP: long polls against the relay, and
//! the fallback when a relay doesn't have them.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use rand::{rngs::StdRng, RngCore, SeedableRng};
use zafe_core::relay_client::RelayClient;
use zafe_proto::{relay::WaitResponse, Envelope, Identity, Kind, LogEntry, LogKey};
use zafe_relay::limits::Limits;

async fn serve(router: axum::Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    format!("http://{addr}")
}

/// A sealed three-member mailbox on a relay with the hosted limits.
async fn setup() -> (RelayClient, Arc<Vec<Identity>>, [u8; 16], StdRng) {
    let url = serve(
        zafe_relay::Relay::new()
            .with_limits(Limits::hosted())
            .router(),
    )
    .await;
    let relay = RelayClient::new(url);
    let mut rng = StdRng::seed_from_u64(21);
    let ids: Vec<Identity> = (0..3).map(|_| Identity::generate(&mut rng)).collect();
    let (mut mailbox, mut token) = ([0u8; 16], [0u8; 32]);
    rng.fill_bytes(&mut mailbox);
    rng.fill_bytes(&mut token);
    relay
        .create_mailbox(&ids[0], mailbox, &token, 3)
        .await
        .unwrap();
    for id in &ids[1..] {
        relay.join(id, mailbox, token).await.unwrap();
    }
    let members = ids.iter().map(|i| i.public().sig_pk).collect();
    relay.seal(&ids[0], mailbox, members).await.unwrap();
    (relay, Arc::new(ids), mailbox, rng)
}

#[tokio::test]
async fn long_polls_answer_on_activity_and_time_out_otherwise() {
    let (relay, ids, mailbox, mut rng) = setup().await;
    let long = Duration::from_secs(20);

    // Where things are now, without waiting.
    let start = relay
        .wait_for_activity(&ids[1], mailbox, 0, 0, Duration::ZERO)
        .await
        .unwrap()
        .expect("supported");
    assert_eq!(start, WaitResponse::default());

    // Another member appends to the log: the wait answers at once.
    let waiting = {
        let (relay, ids) = (relay.clone(), ids.clone());
        tokio::spawn(async move {
            let t = Instant::now();
            let r = relay.wait_for_activity(&ids[1], mailbox, 0, 0, long).await;
            (r, t.elapsed())
        })
    };
    tokio::time::sleep(Duration::from_millis(300)).await;
    let key = LogKey::generate(0, &mut rng);
    let entry = LogEntry::create(&ids[0], &key, mailbox, 0, [0; 32], b"created", &mut rng).unwrap();
    relay.append_log_to(&ids[0], mailbox, &entry).await.unwrap();
    let (answer, took) = waiting.await.unwrap();
    let answer = answer.unwrap().unwrap();
    assert_eq!(answer.log_len, 1);
    assert!(took < Duration::from_secs(5), "took {took:?}");

    // A signing request to this member: the wait answers with the new inbox cursor.
    let waiting = {
        let (relay, ids) = (relay.clone(), ids.clone());
        tokio::spawn(async move { relay.wait_for_activity(&ids[1], mailbox, 1, 0, long).await })
    };
    tokio::time::sleep(Duration::from_millis(300)).await;
    let envelope = Envelope::sealed(
        &ids[2],
        ids[1].public(),
        mailbox,
        1,
        Kind::SigningRequest,
        b"please sign",
        &mut rng,
    )
    .unwrap();
    relay.send(&ids[2], &envelope).await.unwrap();
    let answer = waiting.await.unwrap().unwrap().unwrap();
    assert_eq!(answer.log_len, 1);
    assert!(answer.inbox_cursor > 0);
    assert!(answer.is_news(1, 0));

    // Nothing new: the wait ends after its time with the same state.
    let t = Instant::now();
    let quiet = relay
        .wait_for_activity(
            &ids[1],
            mailbox,
            1,
            answer.inbox_cursor,
            Duration::from_secs(1),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(quiet, answer);
    assert!(t.elapsed() >= Duration::from_secs(1));
}

#[tokio::test]
async fn a_relay_without_long_polls_reads_as_unsupported() {
    let (_, ids, mailbox, _) = setup().await;
    // An older relay: every other route, no `/v1/wait` (axum answers 404).
    let old =
        serve(axum::Router::new().route("/health", axum::routing::get(|| async { "ok" }))).await;
    let answer = RelayClient::new(old)
        .wait_for_activity(&ids[0], mailbox, 0, 0, Duration::from_secs(1))
        .await
        .unwrap();
    assert_eq!(answer, None);
}

#[tokio::test]
async fn outsiders_are_refused_not_unsupported() {
    let (relay, _, mailbox, mut rng) = setup().await;
    let outsider = Identity::generate(&mut rng);
    let err = relay
        .wait_for_activity(&outsider, mailbox, 0, 0, Duration::from_secs(1))
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            zafe_core::relay_client::RelayClientError::Status { status: 403, .. }
        ),
        "{err}"
    );
}
