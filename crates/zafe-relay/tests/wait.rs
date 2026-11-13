//! Long polls (`/v1/wait`), driven in-process.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use http_body_util::BodyExt;
use rand::{rngs::StdRng, RngCore, SeedableRng};
use tower::ServiceExt;
use zafe_proto::{
    relay::{
        decode_body, join_token_hash, AppendResult, CreateMailbox, Join, LabelledAppend,
        PostEnvelope, Seal, Signed, WaitRequest, WaitResponse,
    },
    Envelope, Identity, Kind, LogEntry, LogKey,
};
use zafe_relay::Relay;

const MAILBOX: [u8; 16] = [9; 16];
const NOW: u64 = 1_790_000_000;

async fn call(app: &Router, path: &str, body: Vec<u8>) -> (StatusCode, Vec<u8>) {
    let response = app
        .clone()
        .oneshot(Request::post(path).body(Body::from(body)).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, body.to_vec())
}

async fn signed<T: serde::Serialize + serde::de::DeserializeOwned>(
    app: &Router,
    path: &str,
    who: &Identity,
    payload: T,
) -> (StatusCode, Vec<u8>) {
    let body = Signed::new(who, payload).unwrap().to_bytes().unwrap();
    call(app, path, body).await
}

fn wait_req(log_len: u64, inbox_after: u64, max_wait_secs: u32) -> WaitRequest {
    WaitRequest {
        mailbox: MAILBOX,
        log_len,
        inbox_after,
        max_wait_secs,
        timestamp: NOW,
    }
}

/// Starts a wait for `who` in the background; the handle yields status, answer and how
/// long it took.
fn spawn_wait(
    app: &Router,
    who: &Identity,
    req: WaitRequest,
) -> tokio::task::JoinHandle<(StatusCode, Option<WaitResponse>, Duration)> {
    let app = app.clone();
    let body = Signed::new(who, req).unwrap().to_bytes().unwrap();
    tokio::spawn(async move {
        let started = Instant::now();
        let (status, body) = call(&app, "/v1/wait", body).await;
        let answer = (status == StatusCode::OK).then(|| decode_body(&body).unwrap());
        (status, answer, started.elapsed())
    })
}

struct World {
    app: Router,
    rng: StdRng,
    /// ids[0..3] are members (ids[0] created the mailbox); ids[3] is an outsider.
    ids: Vec<Identity>,
    key: LogKey,
    head: [u8; 32],
    len: u64,
    seq: u64,
}

impl World {
    async fn new(relay: Relay) -> Self {
        let app = relay.router();
        let mut rng = StdRng::seed_from_u64(11);
        let ids: Vec<Identity> = (0..4).map(|_| Identity::generate(&mut rng)).collect();
        let mut token = [0u8; 32];
        rng.fill_bytes(&mut token);
        let create = CreateMailbox {
            mailbox: MAILBOX,
            join_token_hash: join_token_hash(&token),
            max_members: 3,
        };
        assert_eq!(
            signed(&app, "/v1/mailbox/create", &ids[0], create).await.0,
            StatusCode::OK
        );
        for id in &ids[1..3] {
            let join = Join {
                mailbox: MAILBOX,
                join_token: token,
            };
            assert_eq!(
                signed(&app, "/v1/mailbox/join", id, join).await.0,
                StatusCode::OK
            );
        }
        let members = ids[..3].iter().map(|i| i.public().sig_pk).collect();
        let seal = Seal {
            mailbox: MAILBOX,
            members,
        };
        assert_eq!(
            signed(&app, "/v1/mailbox/seal", &ids[0], seal).await.0,
            StatusCode::OK
        );
        let key = LogKey::generate(0, &mut rng);
        Self {
            app,
            rng,
            ids,
            key,
            head: [0; 32],
            len: 0,
            seq: 0,
        }
    }

    async fn append(&mut self, author: usize) {
        let entry = LogEntry::create(
            &self.ids[author],
            &self.key,
            MAILBOX,
            self.len,
            self.head,
            b"event",
            &mut self.rng,
        )
        .unwrap();
        let request = LabelledAppend::new(MAILBOX, &entry, NOW).unwrap();
        let (status, body) = signed(
            &self.app,
            "/v1/log/append-labelled",
            &self.ids[author],
            request,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            decode_body::<AppendResult>(&body).unwrap(),
            AppendResult::Appended { index: self.len }
        );
        self.head = entry.hash().unwrap();
        self.len += 1;
    }

    async fn send(&mut self, from: usize, to: usize) {
        self.seq += 1;
        let envelope = Envelope::sealed(
            &self.ids[from],
            self.ids[to].public(),
            MAILBOX,
            self.seq,
            Kind::SigningRequest,
            b"sign this",
            &mut self.rng,
        )
        .unwrap();
        let envelope = envelope.to_bytes().unwrap();
        let (status, _) = signed(
            &self.app,
            "/v1/envelope/send",
            &self.ids[from],
            PostEnvelope { envelope },
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }
}

fn relay() -> Relay {
    Relay::with_clock(Arc::new(|| NOW))
}

/// Gives a spawned wait time to reach the relay and start waiting.
async fn settle() {
    tokio::time::sleep(Duration::from_millis(200)).await;
}

#[tokio::test]
async fn a_log_append_wakes_a_waiting_member_at_once() {
    let mut w = World::new(relay()).await;
    w.append(0).await;
    let waiting = spawn_wait(&w.app, &w.ids[1], wait_req(1, 0, 20));
    settle().await;
    assert!(!waiting.is_finished(), "nothing new yet");
    w.append(2).await;
    let (status, answer, took) = waiting.await.unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        answer.unwrap(),
        WaitResponse {
            log_len: 2,
            inbox_cursor: 0
        }
    );
    assert!(took < Duration::from_secs(5), "answered in {took:?}");
}

#[tokio::test]
async fn an_envelope_wakes_its_recipient_only() {
    let mut w = World::new(relay()).await;
    let to_one = spawn_wait(&w.app, &w.ids[1], wait_req(0, 0, 20));
    let to_two = spawn_wait(&w.app, &w.ids[2], wait_req(0, 0, 2));
    settle().await;
    w.send(0, 1).await;

    let (status, answer, took) = to_one.await.unwrap();
    assert_eq!(status, StatusCode::OK);
    let answer = answer.unwrap();
    assert_eq!(answer.log_len, 0);
    assert!(answer.inbox_cursor > 0);
    assert!(took < Duration::from_secs(5), "answered in {took:?}");

    // Member 2 was woken by the same signal, found nothing for itself and kept waiting.
    let (status, answer, took) = to_two.await.unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(answer.unwrap(), WaitResponse::default());
    assert!(
        took >= Duration::from_secs(2),
        "waited its full time: {took:?}"
    );

    // Behind already: answered without waiting.
    let (_, answer, took) = spawn_wait(&w.app, &w.ids[1], wait_req(0, 0, 20))
        .await
        .unwrap();
    assert!(answer.unwrap().inbox_cursor > 0);
    assert!(took < Duration::from_secs(2));
}

#[tokio::test]
async fn a_quiet_wait_times_out_with_the_current_state() {
    let mut w = World::new(relay()).await;
    w.append(0).await;
    let (status, answer, took) = spawn_wait(&w.app, &w.ids[0], wait_req(1, 0, 1))
        .await
        .unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        answer.unwrap(),
        WaitResponse {
            log_len: 1,
            inbox_cursor: 0
        }
    );
    assert!(took >= Duration::from_secs(1) && took < Duration::from_secs(4));

    // max_wait 0 is a plain read of where the mailbox is.
    let (_, answer, took) = spawn_wait(&w.app, &w.ids[0], wait_req(u64::MAX, u64::MAX, 0))
        .await
        .unwrap();
    assert_eq!(answer.unwrap().log_len, 1);
    assert!(took < Duration::from_secs(1));
}

#[tokio::test]
async fn only_members_with_valid_signatures_may_wait() {
    let w = World::new(relay()).await;
    // An outsider, and an unknown mailbox, are both 403 (404 means "no such endpoint").
    let (status, _) = signed(&w.app, "/v1/wait", &w.ids[3], wait_req(0, 0, 1)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let mut unknown = wait_req(0, 0, 1);
    unknown.mailbox = [1; 16];
    let (status, _) = signed(&w.app, "/v1/wait", &w.ids[0], unknown).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    // A forged signature, and a stale timestamp.
    let mut forged = Signed::new(&w.ids[3], wait_req(0, 0, 1)).unwrap();
    forged.signer = *w.ids[1].public();
    let (status, _) = call(&w.app, "/v1/wait", forged.to_bytes().unwrap()).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let mut stale = wait_req(0, 0, 1);
    stale.timestamp = NOW - 3600;
    let (status, _) = signed(&w.app, "/v1/wait", &w.ids[1], stale).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn concurrent_waits_are_capped_per_key() {
    let mut w = World::new(relay()).await;
    let first = spawn_wait(&w.app, &w.ids[1], wait_req(0, 0, 20));
    let second = spawn_wait(&w.app, &w.ids[1], wait_req(0, 0, 20));
    settle().await;

    let response = w
        .app
        .clone()
        .oneshot(
            Request::post("/v1/wait")
                .body(Body::from(
                    Signed::new(&w.ids[1], wait_req(0, 0, 20))
                        .unwrap()
                        .to_bytes()
                        .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(response.headers().contains_key("retry-after"));

    // Other members have their own slots; answers needing no wait take none.
    let other = spawn_wait(&w.app, &w.ids[2], wait_req(0, 0, 20));
    settle().await;
    let (status, _, _) = spawn_wait(&w.app, &w.ids[1], wait_req(0, 0, 0))
        .await
        .unwrap();
    assert_eq!(status, StatusCode::OK);

    // A client that gives up frees its slot.
    first.abort();
    let _ = first.await;
    settle().await;
    let third = spawn_wait(&w.app, &w.ids[1], wait_req(0, 0, 20));
    settle().await;
    assert!(!third.is_finished(), "admitted and waiting");

    w.append(0).await;
    for handle in [second, other, third] {
        let (status, answer, _) = handle.await.unwrap();
        assert_eq!(status, StatusCode::OK);
        assert_eq!(answer.unwrap().log_len, 1);
    }
}

#[tokio::test]
async fn waits_are_capped_in_total() {
    let mut w = World::new(relay().with_wait_caps(2, 1)).await;
    let first = spawn_wait(&w.app, &w.ids[1], wait_req(0, 0, 20));
    settle().await;
    let (status, _) = signed(&w.app, "/v1/wait", &w.ids[2], wait_req(0, 0, 20)).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    w.append(0).await;
    assert_eq!(first.await.unwrap().0, StatusCode::OK);
}
