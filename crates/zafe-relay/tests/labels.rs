use std::sync::{Arc, Mutex};

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use http_body_util::BodyExt;
use rand::{rngs::StdRng, SeedableRng};
use tower::ServiceExt;
use zafe_proto::{
    relay::{
        decode_body, join_token_hash, AppendResult, CreateMailbox, ForgetPush, InboxAck,
        InboxAckResponse, InboxRead, InboxResponse, Join, LabelledAppend, LogRead, LogResponse,
        MailboxInfo, MembersRead, PostEnvelope, PushPlatform, RegisterPush, RelayInfo, Reseed,
        ReseedResponse, Seal, Signed, WaitRequest, WaitResponse, CREATE_TOKEN_HEADER,
        UNKNOWN_MAILBOX,
    },
    Envelope, Identity, Kind, LogEntry, LogKey, MailboxId,
};
use zafe_relay::{Notifier, Relay};

const VAULT: MailboxId = [1; 16];
const LABEL: MailboxId = [2; 16];
const OTHER_VAULT: MailboxId = [3; 16];
const NOW: u64 = 1_790_000_000;
const TOKEN: [u8; 32] = [4; 32];

async fn send(app: &Router, request: Request<Body>) -> (StatusCode, Vec<u8>) {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, body.to_vec())
}

async fn call(app: &Router, path: &str, body: Vec<u8>) -> (StatusCode, Vec<u8>) {
    send(app, Request::post(path).body(Body::from(body)).unwrap()).await
}

fn signed_body<T: serde::Serialize + serde::de::DeserializeOwned>(
    who: &Identity,
    payload: T,
) -> Vec<u8> {
    Signed::new(who, payload).unwrap().to_bytes().unwrap()
}

async fn signed<T: serde::Serialize + serde::de::DeserializeOwned>(
    app: &Router,
    path: &str,
    who: &Identity,
    payload: T,
) -> (StatusCode, Vec<u8>) {
    call(app, path, signed_body(who, payload)).await
}

fn relay() -> Relay {
    Relay::with_clock(Arc::new(|| NOW))
}

async fn post_env(app: &Router, who: &Identity, env: &Envelope) -> (StatusCode, Vec<u8>) {
    let envelope = env.to_bytes().unwrap();
    signed(app, "/v1/envelope/send", who, PostEnvelope { envelope }).await
}

fn ids(n: usize, seed: u64) -> Vec<Identity> {
    let mut rng = StdRng::seed_from_u64(seed);
    (0..n).map(|_| Identity::generate(&mut rng)).collect()
}

async fn open_mailbox(app: &Router, label: MailboxId, ids: &[Identity]) {
    let create = CreateMailbox {
        mailbox: label,
        join_token_hash: join_token_hash(&TOKEN),
        max_members: ids.len() as u16,
    };
    assert_eq!(
        signed(app, "/v1/mailbox/create", &ids[0], create).await.0,
        StatusCode::OK
    );
    for id in &ids[1..] {
        let join = Join {
            mailbox: label,
            join_token: TOKEN,
        };
        assert_eq!(
            signed(app, "/v1/mailbox/join", id, join).await.0,
            StatusCode::OK
        );
    }
    let seal = Seal {
        mailbox: label,
        members: ids.iter().map(|i| i.public().sig_pk).collect(),
    };
    assert_eq!(
        signed(app, "/v1/mailbox/seal", &ids[0], seal).await.0,
        StatusCode::OK
    );
}

fn entries(vault: MailboxId, ids: &[Identity], n: u64, rng: &mut StdRng) -> Vec<LogEntry> {
    let key = LogKey::generate(0, rng);
    let mut head = [0; 32];
    (0..n)
        .map(|i| {
            let author = &ids[i as usize % ids.len()];
            let e = LogEntry::create(author, &key, vault, i, head, b"event", rng).unwrap();
            head = e.hash().unwrap();
            e
        })
        .collect()
}

async fn append_to(
    app: &Router,
    who: &Identity,
    label: MailboxId,
    entry: &LogEntry,
) -> (StatusCode, Option<AppendResult>) {
    let req = LabelledAppend::new(label, entry, NOW).unwrap();
    let (status, body) = signed(app, "/v1/log/append-labelled", who, req).await;
    (status, decode_body::<AppendResult>(&body).ok())
}

async fn read_log(app: &Router, who: &Identity, label: MailboxId) -> Vec<LogEntry> {
    let read = LogRead {
        mailbox: label,
        from: 0,
        timestamp: NOW,
    };
    let (status, body) = signed(app, "/v1/log/read", who, read).await;
    assert_eq!(status, StatusCode::OK);
    decode_body::<LogResponse>(&body)
        .unwrap()
        .decoded()
        .unwrap()
}

fn reseed_req(
    label: MailboxId,
    ids: &[Identity],
    from: u64,
    entries: &[LogEntry],
    finish: bool,
) -> Reseed {
    Reseed {
        mailbox: label,
        members: ids.iter().map(|i| *i.public()).collect(),
        threshold: 2,
        from,
        entries: entries.iter().map(|e| e.to_bytes().unwrap()).collect(),
        finish,
        timestamp: NOW,
    }
}

#[tokio::test]
async fn a_labelled_mailbox_holds_one_vault_whatever_its_label() {
    let app = relay().router();
    let ids = ids(4, 1);
    let members = &ids[..3];
    open_mailbox(&app, LABEL, members).await;
    let mut rng = StdRng::seed_from_u64(2);
    let log = entries(VAULT, members, 3, &mut rng);

    let (status, _) = call(&app, "/v1/log/append", log[0].to_bytes().unwrap()).await;
    assert_eq!(status, StatusCode::UPGRADE_REQUIRED);

    assert_eq!(
        append_to(&app, &ids[1], LABEL, &log[0]).await,
        (StatusCode::FORBIDDEN, None)
    );
    assert_eq!(
        append_to(&app, &ids[0], LABEL, &log[0]).await,
        (StatusCode::OK, Some(AppendResult::Appended { index: 0 }))
    );
    assert_eq!(
        append_to(&app, &ids[2], LABEL, &log[1]).await,
        (StatusCode::OK, Some(AppendResult::Appended { index: 1 }))
    );
    assert_eq!(
        append_to(&app, &ids[1], LABEL, &log[1]).await,
        (StatusCode::OK, Some(AppendResult::Conflict { len: 2 }))
    );
    let (status, result) = append_to(&app, &ids[3], LABEL, &log[2]).await;
    assert_eq!((status, result), (StatusCode::FORBIDDEN, None));
    let key = LogKey::generate(0, &mut rng);
    let head = log[1].hash().unwrap();
    let foreign =
        LogEntry::create(&ids[1], &key, OTHER_VAULT, 2, head, b"other", &mut rng).unwrap();
    let (status, result) = append_to(&app, &ids[1], LABEL, &foreign).await;
    assert_eq!((status, result), (StatusCode::BAD_REQUEST, None));
    let mut moved =
        Signed::new(&ids[1], LabelledAppend::new([9; 16], &log[2], NOW).unwrap()).unwrap();
    moved.payload.mailbox = LABEL;
    let (status, _) = call(&app, "/v1/log/append-labelled", moved.to_bytes().unwrap()).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let old = LabelledAppend::new(LABEL, &log[2], NOW - 3600).unwrap();
    assert_eq!(
        signed(&app, "/v1/log/append-labelled", &ids[1], old)
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    for e in &log[2..] {
        assert_eq!(append_to(&app, &ids[0], LABEL, e).await.0, StatusCode::OK);
    }
    assert_eq!(read_log(&app, &ids[2], LABEL).await, log);
}

#[tokio::test]
async fn a_mailbox_whose_entries_carry_two_vault_ids_is_refused() {
    let app = relay().router();
    let ids = ids(3, 3);
    open_mailbox(&app, VAULT, &ids).await;
    let mut rng = StdRng::seed_from_u64(4);
    let foreign = entries(OTHER_VAULT, &ids, 2, &mut rng);
    assert_eq!(
        append_to(&app, &ids[0], VAULT, &foreign[0]).await.0,
        StatusCode::OK
    );
    let own = entries(VAULT, &ids, 1, &mut rng);
    let next = entry_after(VAULT, &ids[1], 1, &foreign[0], &mut rng);
    for entry in [&next, &own[0]] {
        assert_eq!(
            append_to(&app, &ids[1], VAULT, entry).await.0,
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        append_to(&app, &ids[1], VAULT, &foreign[1]).await.0,
        StatusCode::OK
    );

    let app = relay().router();
    open_mailbox(&app, VAULT, &ids).await;
    let log = entries(VAULT, &ids, 2, &mut rng);
    for (i, e) in log.iter().enumerate() {
        assert_eq!(
            append_to(&app, &ids[0], VAULT, e).await,
            (
                StatusCode::OK,
                Some(AppendResult::Appended { index: i as u64 })
            )
        );
    }
}

fn entry_after(
    vault: MailboxId,
    author: &Identity,
    index: u64,
    prev: &LogEntry,
    rng: &mut StdRng,
) -> LogEntry {
    let key = LogKey::generate(0, rng);
    LogEntry::create(
        author,
        &key,
        vault,
        index,
        prev.hash().unwrap(),
        b"other",
        rng,
    )
    .unwrap()
}

#[tokio::test]
async fn a_reseed_takes_any_label_but_one_vault() {
    let app = relay().router();
    let ids = ids(3, 5);
    let mut rng = StdRng::seed_from_u64(6);
    let log = entries(VAULT, &ids, 4, &mut rng);
    let reseed = |from, entries: &[LogEntry], finish| {
        signed(
            &app,
            "/v1/mailbox/reseed",
            &ids[0],
            reseed_req(LABEL, &ids, from, entries, finish),
        )
    };

    let mixed = [
        log[0].clone(),
        entry_after(OTHER_VAULT, &ids[1], 1, &log[0], &mut rng),
    ];
    let (status, body) = reseed(0, &mixed, false).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, b"this mailbox holds another vault");

    let (status, body) = reseed(0, &log[..2], false).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(decode_body::<ReseedResponse>(&body).unwrap().len, 2);
    let other = entry_after(OTHER_VAULT, &ids[1], 2, &log[1], &mut rng);
    let (status, body) = reseed(2, &[other], true).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, b"this mailbox holds another vault");
    let (status, body) = reseed(2, &log[2..3], true).await;
    assert_eq!(status, StatusCode::OK);
    assert!(decode_body::<ReseedResponse>(&body).unwrap().open);

    let other = entry_after(OTHER_VAULT, &ids[1], 3, &log[2], &mut rng);
    assert_eq!(
        append_to(&app, &ids[1], LABEL, &other).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        append_to(&app, &ids[1], LABEL, &log[3]).await,
        (StatusCode::OK, Some(AppendResult::Appended { index: 3 }))
    );
    assert_eq!(read_log(&app, &ids[2], LABEL).await, log);
}

#[tokio::test]
async fn other_routes_work_with_labels_unchanged() {
    let app = relay().router();
    let ids = ids(3, 7);
    open_mailbox(&app, LABEL, &ids).await;
    let mut rng = StdRng::seed_from_u64(8);
    let log = entries(VAULT, &ids, 1, &mut rng);
    assert_eq!(
        append_to(&app, &ids[0], LABEL, &log[0]).await.0,
        StatusCode::OK
    );

    let read = MembersRead {
        mailbox: LABEL,
        timestamp: NOW,
    };
    let (status, body) = signed(&app, "/v1/mailbox/info", &ids[1], read).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(decode_body::<MailboxInfo>(&body).unwrap().members.len(), 3);

    let push = RegisterPush {
        mailbox: LABEL,
        platform: PushPlatform::Fcm,
        token: "t".into(),
    };
    assert_eq!(
        signed(&app, "/v1/push/register", &ids[1], push).await.0,
        StatusCode::OK
    );

    let env = Envelope::sealed(
        &ids[0],
        ids[1].public(),
        LABEL,
        1,
        Kind::Approval,
        b"hi",
        &mut rng,
    )
    .unwrap();
    assert_eq!(post_env(&app, &ids[0], &env).await.0, StatusCode::OK);
    let read = InboxRead {
        mailbox: LABEL,
        after: 0,
        timestamp: NOW,
    };
    let (status, body) = signed(&app, "/v1/inbox", &ids[1], read).await;
    assert_eq!(status, StatusCode::OK);
    let inbox = decode_body::<InboxResponse>(&body).unwrap().decoded();
    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0].1, env);

    let wait = WaitRequest {
        mailbox: LABEL,
        log_len: 0,
        inbox_after: 0,
        max_wait_secs: 0,
        timestamp: NOW,
    };
    let (status, body) = signed(&app, "/v1/wait", &ids[1], wait).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        decode_body::<WaitResponse>(&body).unwrap(),
        WaitResponse {
            log_len: 1,
            inbox_cursor: inbox[0].0
        }
    );

    let ack = InboxAck {
        mailbox: LABEL,
        cursors: vec![inbox[0].0],
        timestamp: NOW,
    };
    let (status, body) = signed(&app, "/v1/inbox/ack", &ids[1], ack).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(decode_body::<InboxAckResponse>(&body).unwrap().deleted, 1);
    assert_eq!(read_log(&app, &ids[1], LABEL).await, log);
}

#[tokio::test]
async fn an_envelope_with_an_old_label_fails_on_the_new_mailbox() {
    const OLD: MailboxId = [5; 16];
    const NEW: MailboxId = [6; 16];
    let old_relay = relay().router();
    let new_relay = relay().router();
    let ids = ids(3, 9);
    open_mailbox(&old_relay, OLD, &ids).await;
    open_mailbox(&new_relay, NEW, &ids).await;
    let mut rng = StdRng::seed_from_u64(10);
    let env = Envelope::sealed(
        &ids[0],
        ids[1].public(),
        OLD,
        1,
        Kind::Approval,
        b"approve",
        &mut rng,
    )
    .unwrap();
    assert_eq!(post_env(&old_relay, &ids[0], &env).await.0, StatusCode::OK);
    let (status, body) = post_env(&new_relay, &ids[0], &env).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, UNKNOWN_MAILBOX.as_bytes());
    let mut relabelled = env.clone();
    relabelled.header.mailbox = NEW;
    assert_eq!(
        post_env(&new_relay, &ids[0], &relabelled).await.0,
        StatusCode::UNAUTHORIZED
    );
    let read = InboxRead {
        mailbox: NEW,
        after: 0,
        timestamp: NOW,
    };
    let (_, body) = signed(&new_relay, "/v1/inbox", &ids[1], read).await;
    assert!(decode_body::<InboxResponse>(&body)
        .unwrap()
        .envelopes
        .is_empty());
}

#[derive(Default)]
struct Recorder(Mutex<Vec<String>>);

impl Notifier for Recorder {
    fn notify(&self, _platform: PushPlatform, token: &str) {
        self.0.lock().unwrap().push(token.to_owned());
    }
}

#[tokio::test]
async fn a_member_forgets_its_own_push_token() {
    let recorder = Arc::new(Recorder::default());
    let app = relay().with_notifier(recorder.clone()).router();
    let ids = ids(3, 11);
    open_mailbox(&app, LABEL, &ids).await;
    for (i, token) in [(1, "token-B"), (2, "token-C")] {
        let push = RegisterPush {
            mailbox: LABEL,
            platform: PushPlatform::Fcm,
            token: token.into(),
        };
        assert_eq!(
            signed(&app, "/v1/push/register", &ids[i], push).await.0,
            StatusCode::OK
        );
    }
    let forget = |timestamp| ForgetPush {
        mailbox: LABEL,
        timestamp,
    };
    assert_eq!(
        signed(&app, "/v1/push/forget", &ids[1], forget(NOW - 3600))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        signed(&app, "/v1/push/forget", &ids[1], forget(NOW))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        signed(&app, "/v1/push/forget", &ids[1], forget(NOW))
            .await
            .0,
        StatusCode::OK
    );
    let outsider = Identity::generate(&mut StdRng::seed_from_u64(12));
    assert_eq!(
        signed(&app, "/v1/push/forget", &outsider, forget(NOW))
            .await
            .0,
        StatusCode::OK
    );
    let unknown = ForgetPush {
        mailbox: [8; 16],
        timestamp: NOW,
    };
    assert_eq!(
        signed(&app, "/v1/push/forget", &ids[1], unknown).await.0,
        StatusCode::OK
    );
    let env = Envelope::public(&ids[0], LABEL, 1, Kind::Approval, b"news").unwrap();
    assert_eq!(post_env(&app, &ids[0], &env).await.0, StatusCode::OK);
    assert_eq!(*recorder.0.lock().unwrap(), vec!["token-C".to_owned()]);
}

async fn get(app: &Router, path: &str) -> (StatusCode, Vec<u8>) {
    send(app, Request::get(path).body(Body::empty()).unwrap()).await
}

#[tokio::test]
async fn info_says_what_the_relay_allows_and_health_stays_plain() {
    use zafe_relay::{expiry::Expiry, limits::Limits, quota::Quotas};
    let (status, body) = get(&relay().router(), "/v1/info").await;
    assert_eq!(status, StatusCode::OK);
    let info: RelayInfo = serde_json::from_slice(&body).unwrap();
    assert_eq!(info.versions.relay_api, zafe_proto::version::RELAY_API);
    assert_eq!(info.versions.envelope, zafe_proto::version::ENVELOPE);
    assert_eq!(info.versions.log_entry, zafe_proto::version::LOG_ENTRY);
    assert_eq!(
        info.limits.max_body_bytes,
        zafe_relay::MAX_BODY_BYTES as u64
    );
    assert_eq!(info.limits.max_members, zafe_relay::MAX_MEMBERS);
    assert_eq!(info.limits.log_bytes, None);
    assert!(!info.push, "pushes are only logged");
    assert!(!info.create_token);
    assert_eq!(info.expiry.idle_days, None);

    let app = relay()
        .with_limits(Limits::hosted())
        .with_quotas(Quotas::hosted())
        .with_expiry(Expiry::hosted())
        .with_create_token(Some("secret"))
        .with_notifier(Arc::new(Recorder::default()))
        .router();
    let (_, body) = get(&app, "/v1/info").await;
    let text = String::from_utf8(body.clone()).unwrap();
    assert!(!text.contains("secret"), "{text}");
    let info: RelayInfo = serde_json::from_slice(&body).unwrap();
    assert!(info.push && info.create_token);
    assert_eq!(info.limits.log_bytes, Quotas::hosted().log_bytes);
    assert_eq!(info.limits.mailboxes_per_key, Some(8));
    assert_eq!(info.limits.requests_per_minute, Some(300));
    assert_eq!(info.limits.delivery_retention_days, 30);
    assert_eq!(info.expiry.idle_days, Some(365));
    assert_eq!(info.expiry.unused_days, Some(30));
    let partial: RelayInfo = serde_json::from_str(r#"{"push":true,"later":1}"#).unwrap();
    assert!(partial.push && !partial.create_token);

    assert_eq!(get(&app, "/health").await, (StatusCode::OK, b"ok".to_vec()));
}

fn create_body(who: &Identity, mailbox: MailboxId) -> Vec<u8> {
    signed_body(
        who,
        CreateMailbox {
            mailbox,
            join_token_hash: join_token_hash(&TOKEN),
            max_members: 3,
        },
    )
}

async fn post_with_token(
    app: &Router,
    path: &str,
    body: Vec<u8>,
    token: Option<&str>,
) -> axum::response::Response {
    let mut request = Request::post(path).header("fly-client-ip", "203.0.113.7");
    if let Some(token) = token {
        request = request.header(CREATE_TOKEN_HEADER, token);
    }
    app.clone()
        .oneshot(request.body(Body::from(body)).unwrap())
        .await
        .unwrap()
}

#[tokio::test]
async fn a_private_relay_needs_its_create_token() {
    let app = relay().with_create_token(Some(" s3cret ")).router();
    let ids = ids(4, 13);
    for token in [None, Some("wrong"), Some("s3cre"), Some("")] {
        let refused = post_with_token(
            &app,
            "/v1/mailbox/create",
            create_body(&ids[0], LABEL),
            token,
        )
        .await;
        assert_eq!(refused.status(), StatusCode::UNAUTHORIZED, "{token:?}");
        assert_eq!(
            refused.headers()["www-authenticate"],
            CREATE_TOKEN_HEADER,
            "{token:?}"
        );
    }
    let created = post_with_token(
        &app,
        "/v1/mailbox/create",
        create_body(&ids[0], LABEL),
        Some("s3cret"),
    )
    .await;
    assert_eq!(created.status(), StatusCode::OK);
    let join = Join {
        mailbox: LABEL,
        join_token: TOKEN,
    };
    assert_eq!(
        signed(&app, "/v1/mailbox/join", &ids[1], join).await.0,
        StatusCode::OK
    );

    let mut rng = StdRng::seed_from_u64(14);
    let log = entries(VAULT, &ids[..3], 2, &mut rng);
    let reseed = |from, entries: &[LogEntry], finish| {
        signed_body(
            &ids[0],
            reseed_req([7; 16], &ids[..3], from, entries, finish),
        )
    };
    let refused = post_with_token(
        &app,
        "/v1/mailbox/reseed",
        reseed(0, &log[..1], false),
        None,
    )
    .await;
    assert_eq!(refused.status(), StatusCode::UNAUTHORIZED);
    let started = post_with_token(
        &app,
        "/v1/mailbox/reseed",
        reseed(0, &log[..1], false),
        Some("s3cret"),
    )
    .await;
    assert_eq!(started.status(), StatusCode::OK);
    let finished =
        post_with_token(&app, "/v1/mailbox/reseed", reseed(1, &log[1..], true), None).await;
    assert_eq!(finished.status(), StatusCode::OK);
    let lookup = signed_body(&ids[1], reseed_req([7; 16], &ids[..3], 0, &[], true));
    assert_eq!(
        post_with_token(&app, "/v1/mailbox/reseed", lookup, None)
            .await
            .status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn wrong_create_tokens_count_against_the_creation_limit() {
    use zafe_relay::limits::{DailyRate, Limits};
    let app = relay()
        .with_limits(Limits {
            creates_per_ip: Some(DailyRate::new(24, 2)),
            client_ip_header: Some("fly-client-ip".into()),
            ..Limits::none()
        })
        .with_create_token(Some("s3cret"))
        .router();
    let ids = ids(3, 15);
    for (i, token) in ["guess1", "guess2"].into_iter().enumerate() {
        let refused = post_with_token(
            &app,
            "/v1/mailbox/create",
            create_body(&ids[i], [i as u8 + 20; 16]),
            Some(token),
        )
        .await;
        assert_eq!(refused.status(), StatusCode::UNAUTHORIZED);
    }
    let limited = post_with_token(
        &app,
        "/v1/mailbox/create",
        create_body(&ids[2], [30; 16]),
        Some("s3cret"),
    )
    .await;
    assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn a_relay_without_a_token_ignores_one() {
    let app = relay().with_create_token(Some("  ")).router();
    let ids = ids(1, 16);
    for (n, token) in [None, Some("anything")].into_iter().enumerate() {
        let created = post_with_token(
            &app,
            "/v1/mailbox/create",
            create_body(&ids[0], [n as u8 + 40; 16]),
            token,
        )
        .await;
        assert_eq!(created.status(), StatusCode::OK);
    }
    let (status, body) = get(&app, "/v1/info").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        !serde_json::from_slice::<RelayInfo>(&body)
            .unwrap()
            .create_token
    );
}

#[tokio::test]
async fn mailboxes_have_at_most_64_members() {
    let app = relay().router();
    let ids = ids(1, 17);
    let create = |max_members| CreateMailbox {
        mailbox: LABEL,
        join_token_hash: join_token_hash(&TOKEN),
        max_members,
    };
    assert_eq!(
        signed(&app, "/v1/mailbox/create", &ids[0], create(65))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        signed(&app, "/v1/mailbox/create", &ids[0], create(64))
            .await
            .0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn a_seat_move_on_a_labelled_mailbox_is_pending_until_finished() {
    use zafe_proto::relay::{
        LabelledReplace, MembersResponse, SeatCancel, SeatCancelPost, SeatFinish, SeatFinishPost,
        SeatMoveApproval, SetThreshold,
    };
    let app = relay().router();
    let ids = ids(3, 90);
    let fresh = &self::ids(1, 91)[0];
    let create = CreateMailbox {
        mailbox: LABEL,
        join_token_hash: join_token_hash(&TOKEN),
        max_members: 3,
    };
    assert_eq!(
        signed(&app, "/v1/mailbox/create", &ids[0], create).await.0,
        StatusCode::OK
    );
    let threshold = SetThreshold {
        mailbox: LABEL,
        threshold: 2,
    };
    assert_eq!(
        signed(&app, "/v1/mailbox/threshold", &ids[0], threshold)
            .await
            .0,
        StatusCode::OK
    );
    for id in &ids[1..] {
        let join = Join {
            mailbox: LABEL,
            join_token: TOKEN,
        };
        assert_eq!(
            signed(&app, "/v1/mailbox/join", id, join).await.0,
            StatusCode::OK
        );
    }
    let seal = Seal {
        mailbox: LABEL,
        members: ids.iter().map(|i| i.public().sig_pk).collect(),
    };
    assert_eq!(
        signed(&app, "/v1/mailbox/seal", &ids[0], seal).await.0,
        StatusCode::OK
    );
    let mut rng = StdRng::seed_from_u64(92);
    let log = entries(VAULT, &ids, 2, &mut rng);
    assert_eq!(
        append_to(&app, &ids[0], LABEL, &log[0]).await.0,
        StatusCode::OK
    );
    let old = ids[0].public().sig_pk;
    let approval = SeatMoveApproval {
        vault_id: VAULT,
        old,
        new: *fresh.public(),
        round: 0,
    };
    let approvals = |a: &SeatMoveApproval| {
        ids[1..]
            .iter()
            .map(|i| (i.public().sig_pk, a.sign(i).unwrap()))
            .collect::<Vec<_>>()
    };
    let replace = |a: SeatMoveApproval| LabelledReplace {
        mailbox: LABEL,
        approval: a,
        approvals: approvals(&a),
    };
    let other = SeatMoveApproval {
        vault_id: OTHER_VAULT,
        ..approval
    };
    assert_eq!(
        signed(
            &app,
            "/v1/mailbox/replace-labelled",
            &ids[1],
            replace(other)
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let short = LabelledReplace {
        mailbox: LABEL,
        approval,
        approvals: approvals(&approval)[..1].to_vec(),
    };
    assert_eq!(
        signed(&app, "/v1/mailbox/replace-labelled", &ids[1], short)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        signed(
            &app,
            "/v1/mailbox/replace-labelled",
            &ids[1],
            replace(approval)
        )
        .await
        .0,
        StatusCode::OK
    );
    let info = || async {
        let read = MembersRead {
            mailbox: LABEL,
            timestamp: NOW,
        };
        let (status, body) = signed(&app, "/v1/mailbox/info", &ids[1], read).await;
        assert_eq!(status, StatusCode::OK);
        decode_body::<MailboxInfo>(&body).unwrap()
    };
    let now = info().await;
    assert!(now.members.iter().any(|m| m.sig_pk == old));
    assert!(!now.members.contains(fresh.public()));
    assert_eq!(now.pending.len(), 1);
    assert_eq!(now.pending[0].new, *fresh.public());
    assert_eq!(now.pending[0].move_id, approval.move_id());
    assert_eq!(
        signed(
            &app,
            "/v1/mailbox/replace-labelled",
            &ids[2],
            replace(approval)
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(info().await.pending.len(), 1);

    let to_new = Envelope::sealed(
        &ids[1],
        fresh.public(),
        LABEL,
        1,
        Kind::RepairSigma,
        b"s",
        &mut rng,
    )
    .unwrap();
    assert_eq!(post_env(&app, &ids[1], &to_new).await.0, StatusCode::OK);
    let read = InboxRead {
        mailbox: LABEL,
        after: 0,
        timestamp: NOW,
    };
    let (status, body) = signed(&app, "/v1/inbox", fresh, read.clone()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        decode_body::<InboxResponse>(&body).unwrap().decoded().len(),
        1
    );
    assert_eq!(
        append_to(&app, fresh, LABEL, &log[1]).await.0,
        StatusCode::FORBIDDEN,
        "a pending key can't append"
    );
    let from_new = Envelope::public(fresh, LABEL, 1, Kind::Approval, b"x").unwrap();
    assert_eq!(
        post_env(&app, fresh, &from_new).await.0,
        StatusCode::FORBIDDEN,
        "nor send"
    );
    assert_eq!(
        signed(
            &app,
            "/v1/mailbox/replace-labelled",
            fresh,
            replace(approval)
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let push = RegisterPush {
        mailbox: LABEL,
        platform: PushPlatform::Fcm,
        token: "t".into(),
    };
    assert_eq!(
        signed(&app, "/v1/push/register", fresh, push).await.0,
        StatusCode::FORBIDDEN
    );
    let members_read = MembersRead {
        mailbox: LABEL,
        timestamp: NOW,
    };
    assert_eq!(
        signed(&app, "/v1/mailbox/members", fresh, members_read.clone())
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(read_log(&app, fresh, LABEL).await.len(), 1);
    let (status, body) = signed(
        &app,
        "/v1/mailboxes",
        fresh,
        zafe_proto::relay::MailboxesRead { timestamp: NOW },
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        decode_body::<zafe_proto::relay::MailboxesResponse>(&body)
            .unwrap()
            .mailboxes,
        vec![LABEL]
    );

    let finish = SeatFinish {
        mailbox: LABEL,
        vault_id: VAULT,
        move_id: approval.move_id(),
        old,
        new: fresh.public().sig_pk,
    };
    let post_finish = |who: &Identity, f: SeatFinish| {
        let post = SeatFinishPost {
            finish: f,
            by: *who.public(),
            signature: f.sign(who).unwrap(),
        };
        zafe_proto::relay::encode_body(&post).unwrap()
    };
    assert_eq!(
        call(&app, "/v1/seat/finish", post_finish(&ids[0], finish))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    let elsewhere = SeatFinish {
        mailbox: [9; 16],
        ..finish
    };
    let mut relabelled = SeatFinishPost {
        finish: elsewhere,
        by: *ids[1].public(),
        signature: elsewhere.sign(&ids[1]).unwrap(),
    };
    relabelled.finish.mailbox = LABEL;
    assert_eq!(
        call(
            &app,
            "/v1/seat/finish",
            zafe_proto::relay::encode_body(&relabelled).unwrap()
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(&app, "/v1/seat/finish", post_finish(&ids[1], finish))
            .await
            .0,
        StatusCode::OK
    );
    assert!(info().await.members.iter().any(|m| m.sig_pk == old));
    assert_eq!(
        call(&app, "/v1/seat/finish", post_finish(&ids[2], finish))
            .await
            .0,
        StatusCode::OK
    );
    let (status, body) = signed(&app, "/v1/mailbox/members", &ids[1], members_read).await;
    assert_eq!(status, StatusCode::OK);
    let members = decode_body::<MembersResponse>(&body).unwrap().members;
    assert!(members.contains(fresh.public()));
    assert!(!members.iter().any(|m| m.sig_pk == old));
    assert!(info().await.pending.is_empty());

    let app = relay().router();
    open_mailbox_with_threshold(&app, &ids).await;
    assert_eq!(
        append_to(&app, &ids[0], LABEL, &log[0]).await.0,
        StatusCode::OK
    );
    let seat = ids[1].public().sig_pk;
    let move_of = |round: u32| SeatMoveApproval {
        vault_id: VAULT,
        old: seat,
        new: *fresh.public(),
        round,
    };
    let by_others = |a: SeatMoveApproval| LabelledReplace {
        mailbox: LABEL,
        approval: a,
        approvals: [0, 2]
            .iter()
            .map(|&i| (ids[i].public().sig_pk, a.sign(&ids[i]).unwrap()))
            .collect(),
    };
    assert_eq!(
        signed(
            &app,
            "/v1/mailbox/replace-labelled",
            &ids[0],
            by_others(move_of(0))
        )
        .await
        .0,
        StatusCode::OK
    );
    let cancel = |who: &Identity, c: SeatCancel| {
        let post = SeatCancelPost {
            cancel: c,
            signature: c.sign(who).unwrap(),
        };
        zafe_proto::relay::encode_body(&post).unwrap()
    };
    let first = SeatCancel {
        mailbox: LABEL,
        vault_id: VAULT,
        move_id: move_of(0).move_id(),
        old: seat,
        new: fresh.public().sig_pk,
        at: NOW,
    };
    assert_eq!(
        call(&app, "/v1/seat/cancel", cancel(&ids[0], first))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(&app, "/v1/seat/cancel", cancel(&ids[1], first))
            .await
            .0,
        StatusCode::OK
    );
    assert!(
        pending_on(&app, &ids[0]).await.is_empty(),
        "the pending key is gone"
    );
    let read = MembersRead {
        mailbox: LABEL,
        timestamp: NOW,
    };
    let (_, body) = signed(&app, "/v1/mailbox/info", &ids[2], read).await;
    let shown = decode_body::<MailboxInfo>(&body).unwrap().cancelled;
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].cancel, first);
    assert!(first.verify(ids[1].public(), &shown[0].signature).is_ok());
    assert_eq!(
        signed(
            &app,
            "/v1/mailbox/replace-labelled",
            &ids[0],
            by_others(move_of(0))
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let to_other = SeatMoveApproval {
        new: *self::ids(1, 93)[0].public(),
        ..move_of(0)
    };
    assert_eq!(
        signed(
            &app,
            "/v1/mailbox/replace-labelled",
            &ids[0],
            by_others(to_other)
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        signed(
            &app,
            "/v1/mailbox/replace-labelled",
            &ids[0],
            by_others(move_of(1))
        )
        .await
        .0,
        StatusCode::OK
    );
    let second = SeatCancel {
        move_id: move_of(1).move_id(),
        ..first
    };
    assert_eq!(
        call(&app, "/v1/seat/cancel", cancel(&ids[1], second))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(pending_on(&app, &ids[0]).await.len(), 2);
}

async fn seat_world(seed: u64) -> (Router, Vec<Identity>) {
    let app = relay().router();
    let ids = ids(3, seed);
    open_mailbox_with_threshold(&app, &ids).await;
    let mut rng = StdRng::seed_from_u64(seed);
    let log = entries(VAULT, &ids, 1, &mut rng);
    assert_eq!(
        append_to(&app, &ids[0], LABEL, &log[0]).await.0,
        StatusCode::OK
    );
    (app, ids)
}

async fn approve_on(
    app: &Router,
    ids: &[Identity],
    by: [usize; 2],
    a: zafe_proto::relay::SeatMoveApproval,
) -> (StatusCode, Vec<u8>) {
    let request = zafe_proto::relay::LabelledReplace {
        mailbox: LABEL,
        approval: a,
        approvals: by
            .iter()
            .map(|&i| (ids[i].public().sig_pk, a.sign(&ids[i]).unwrap()))
            .collect(),
    };
    signed(app, "/v1/mailbox/replace-labelled", &ids[by[0]], request).await
}

async fn finish_on(
    app: &Router,
    who: &Identity,
    a: &zafe_proto::relay::SeatMoveApproval,
) -> StatusCode {
    let finish = zafe_proto::relay::SeatFinish {
        mailbox: LABEL,
        vault_id: VAULT,
        move_id: a.move_id(),
        old: a.old,
        new: a.new.sig_pk,
    };
    let post = zafe_proto::relay::SeatFinishPost {
        finish,
        by: *who.public(),
        signature: finish.sign(who).unwrap(),
    };
    call(
        app,
        "/v1/seat/finish",
        zafe_proto::relay::encode_body(&post).unwrap(),
    )
    .await
    .0
}

async fn cancel_on(
    app: &Router,
    old: &Identity,
    a: &zafe_proto::relay::SeatMoveApproval,
) -> StatusCode {
    let cancel = zafe_proto::relay::SeatCancel {
        mailbox: LABEL,
        vault_id: VAULT,
        move_id: a.move_id(),
        old: a.old,
        new: a.new.sig_pk,
        at: NOW,
    };
    let post = zafe_proto::relay::SeatCancelPost {
        cancel,
        signature: cancel.sign(old).unwrap(),
    };
    call(
        app,
        "/v1/seat/cancel",
        zafe_proto::relay::encode_body(&post).unwrap(),
    )
    .await
    .0
}

async fn members_now(app: &Router, who: &Identity) -> Vec<[u8; 32]> {
    let read = MembersRead {
        mailbox: LABEL,
        timestamp: NOW,
    };
    let (_, body) = signed(app, "/v1/mailbox/info", who, read).await;
    decode_body::<MailboxInfo>(&body)
        .unwrap()
        .members
        .iter()
        .map(|m| m.sig_pk)
        .collect()
}

#[tokio::test]
async fn a_cancel_touches_only_the_move_it_names() {
    let (app, ids) = seat_world(94).await;
    let (z, x) = (self::ids(1, 95).remove(0), self::ids(1, 96).remove(0));
    let seat = ids[1].public().sig_pk;
    let move_to = |new: &Identity| zafe_proto::relay::SeatMoveApproval {
        vault_id: VAULT,
        old: seat,
        new: *new.public(),
        round: 0,
    };
    assert_eq!(
        approve_on(&app, &ids, [0, 2], move_to(&z)).await.0,
        StatusCode::OK
    );
    assert_eq!(
        approve_on(&app, &ids, [0, 2], move_to(&x)).await.0,
        StatusCode::OK
    );
    assert_eq!(cancel_on(&app, &ids[1], &move_to(&z)).await, StatusCode::OK);
    let pending = pending_on(&app, &ids[0]).await;
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].new, *x.public(), "X's row is untouched");
    for i in [0, 2] {
        assert_eq!(finish_on(&app, &ids[i], &move_to(&x)).await, StatusCode::OK);
    }
    let members = members_now(&app, &ids[0]).await;
    assert!(members.contains(&x.public().sig_pk) && !members.contains(&seat));
}

#[tokio::test]
async fn finishes_switch_a_cancelled_row_at_t() {
    let (app, ids) = seat_world(97).await;
    let x = self::ids(1, 98).remove(0);
    let seat = ids[1].public().sig_pk;
    let a = zafe_proto::relay::SeatMoveApproval {
        vault_id: VAULT,
        old: seat,
        new: *x.public(),
        round: 0,
    };
    assert_eq!(approve_on(&app, &ids, [0, 2], a).await.0, StatusCode::OK);
    assert_eq!(finish_on(&app, &ids[0], &a).await, StatusCode::OK);
    assert_eq!(cancel_on(&app, &ids[1], &a).await, StatusCode::OK);
    assert!(
        pending_on(&app, &ids[0]).await.is_empty(),
        "no pending access"
    );
    assert_eq!(finish_on(&app, &ids[1], &a).await, StatusCode::FORBIDDEN);
    assert_eq!(finish_on(&app, &x, &a).await, StatusCode::FORBIDDEN);
    assert!(members_now(&app, &ids[0]).await.contains(&seat));
    assert_eq!(finish_on(&app, &ids[2], &a).await, StatusCode::OK);
    let members = members_now(&app, &ids[0]).await;
    assert!(members.contains(&x.public().sig_pk) && !members.contains(&seat));
}

#[tokio::test]
async fn one_seats_junk_rows_dont_block_another_seat() {
    let (app, ids) = seat_world(99).await;
    let move_of = |seat: usize, new: &Identity| zafe_proto::relay::SeatMoveApproval {
        vault_id: VAULT,
        old: ids[seat].public().sig_pk,
        new: *new.public(),
        round: 0,
    };
    let junk = self::ids(zafe_proto::relay::MAX_PENDING_SEAT_MOVES + 1, 100);
    for z in &junk[..zafe_proto::relay::MAX_PENDING_SEAT_MOVES] {
        assert_eq!(
            approve_on(&app, &ids, [0, 2], move_of(1, z)).await.0,
            StatusCode::OK
        );
    }
    let (status, body) = approve_on(&app, &ids, [0, 2], move_of(1, junk.last().unwrap())).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body, zafe_proto::relay::TOO_MANY_SEAT_MOVES.as_bytes());
    let real = self::ids(1, 101).remove(0);
    assert_eq!(
        approve_on(&app, &ids, [0, 1], move_of(2, &real)).await.0,
        StatusCode::OK
    );
}

async fn pending_on(app: &Router, who: &Identity) -> Vec<zafe_proto::relay::PendingSeat> {
    let read = MembersRead {
        mailbox: LABEL,
        timestamp: NOW,
    };
    let (_, body) = signed(app, "/v1/mailbox/info", who, read).await;
    decode_body::<MailboxInfo>(&body).unwrap().pending
}

async fn open_mailbox_with_threshold(app: &Router, ids: &[Identity]) {
    use zafe_proto::relay::SetThreshold;
    let create = CreateMailbox {
        mailbox: LABEL,
        join_token_hash: join_token_hash(&TOKEN),
        max_members: ids.len() as u16,
    };
    assert_eq!(
        signed(app, "/v1/mailbox/create", &ids[0], create).await.0,
        StatusCode::OK
    );
    let threshold = SetThreshold {
        mailbox: LABEL,
        threshold: 2,
    };
    assert_eq!(
        signed(app, "/v1/mailbox/threshold", &ids[0], threshold)
            .await
            .0,
        StatusCode::OK
    );
    for id in &ids[1..] {
        let join = Join {
            mailbox: LABEL,
            join_token: TOKEN,
        };
        assert_eq!(
            signed(app, "/v1/mailbox/join", id, join).await.0,
            StatusCode::OK
        );
    }
    let seal = Seal {
        mailbox: LABEL,
        members: ids.iter().map(|i| i.public().sig_pk).collect(),
    };
    assert_eq!(
        signed(app, "/v1/mailbox/seal", &ids[0], seal).await.0,
        StatusCode::OK
    );
}
