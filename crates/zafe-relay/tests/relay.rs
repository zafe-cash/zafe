use std::sync::Arc;

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
        decode_body, join_token_hash, AppendResult, CancelSetup, CreateMailbox, InboxRead,
        InboxResponse, Join, LabelledAppend, LogRead, LogResponse, MembersRead, MembersResponse,
        PostEnvelope, Remove, RenewInvite, Seal, Signed, INVITE_TTL_SECS,
    },
    Chain, Envelope, Identity, Kind, LogEntry, LogKey,
};
use zafe_relay::Relay;

const MAILBOX: [u8; 16] = [7; 16];
const NOW: u64 = 1_790_000_000;

async fn call(app: &Router, path: &str, body: Vec<u8>) -> (StatusCode, Vec<u8>) {
    let response = app
        .clone()
        .oneshot(Request::post(path).body(Body::from(body)).unwrap())
        .await
        .unwrap();
    let status = response.status();
    (
        status,
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
}

async fn signed<T: serde::Serialize + serde::de::DeserializeOwned>(
    app: &Router,
    path: &str,
    who: &Identity,
    payload: T,
) -> (StatusCode, Vec<u8>) {
    call(
        app,
        path,
        Signed::new(who, payload).unwrap().to_bytes().unwrap(),
    )
    .await
}

async fn post_env(app: &Router, who: &Identity, env: &Envelope) -> (StatusCode, Vec<u8>) {
    let envelope = env.to_bytes().unwrap();
    signed(app, "/v1/envelope/send", who, PostEnvelope { envelope }).await
}

async fn append(app: &Router, who: &Identity, entry: &LogEntry) -> (StatusCode, Vec<u8>) {
    let request = LabelledAppend::new(entry.header.mailbox, entry, NOW).unwrap();
    signed(app, "/v1/log/append-labelled", who, request).await
}

struct World {
    app: Router,
    rng: StdRng,
    ids: Vec<Identity>,
    token: [u8; 32],
}

impl World {
    async fn new(seal: bool) -> Self {
        let app = Relay::with_clock(Arc::new(|| NOW)).router();
        let mut rng = StdRng::seed_from_u64(5);
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
        let w = Self {
            app,
            rng,
            ids,
            token,
        };
        if seal {
            let members = w.ids[..3].iter().map(|i| i.public().sig_pk).collect();
            assert_eq!(
                signed(
                    &w.app,
                    "/v1/mailbox/seal",
                    &w.ids[0],
                    Seal {
                        mailbox: MAILBOX,
                        members
                    }
                )
                .await
                .0,
                StatusCode::OK
            );
        }
        w
    }

    async fn inbox(&self, who: usize) -> Vec<(u64, Envelope)> {
        let (status, body) = signed(
            &self.app,
            "/v1/inbox",
            &self.ids[who],
            InboxRead {
                mailbox: MAILBOX,
                after: 0,
                timestamp: NOW,
            },
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        decode_body::<InboxResponse>(&body).unwrap().decoded()
    }
}

#[tokio::test]
async fn membership_lifecycle() {
    let w = World::new(false).await;

    let mut bad = w.token;
    bad[0] ^= 1;
    let (status, _) = signed(
        &w.app,
        "/v1/mailbox/join",
        &w.ids[3],
        Join {
            mailbox: MAILBOX,
            join_token: bad,
        },
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let three: Vec<_> = w.ids[..3].iter().map(|i| i.public().sig_pk).collect();
    let (status, _) = signed(
        &w.app,
        "/v1/mailbox/seal",
        &w.ids[1],
        Seal {
            mailbox: MAILBOX,
            members: three.clone(),
        },
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = signed(
        &w.app,
        "/v1/mailbox/seal",
        &w.ids[0],
        Seal {
            mailbox: MAILBOX,
            members: three[..2].to_vec(),
        },
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = signed(
        &w.app,
        "/v1/mailbox/seal",
        &w.ids[0],
        Seal {
            mailbox: MAILBOX,
            members: three,
        },
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = signed(
        &w.app,
        "/v1/mailbox/join",
        &w.ids[3],
        Join {
            mailbox: MAILBOX,
            join_token: w.token,
        },
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, body) = signed(
        &w.app,
        "/v1/mailbox/members",
        &w.ids[2],
        MembersRead {
            mailbox: MAILBOX,
            timestamp: NOW,
        },
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let members = decode_body::<MembersResponse>(&body).unwrap();
    assert!(members.sealed);
    assert_eq!(members.members.len(), 3);

    let outsider = signed(
        &w.app,
        "/v1/mailbox/members",
        &w.ids[3],
        MembersRead {
            mailbox: MAILBOX,
            timestamp: NOW,
        },
    )
    .await;
    assert_eq!(outsider.0, StatusCode::FORBIDDEN);
    let stale = signed(
        &w.app,
        "/v1/mailbox/members",
        &w.ids[1],
        MembersRead {
            mailbox: MAILBOX,
            timestamp: NOW - 3600,
        },
    )
    .await;
    assert_eq!(stale.0, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn envelopes_are_routed_to_the_right_inboxes() {
    let mut w = World::new(true).await;
    let sealed = Envelope::sealed(
        &w.ids[0],
        w.ids[1].public(),
        MAILBOX,
        1,
        Kind::DkgRound2,
        b"for 1",
        &mut w.rng,
    )
    .unwrap();
    let public = Envelope::public(&w.ids[0], MAILBOX, 2, Kind::DkgRound1, b"for all").unwrap();
    for env in [&sealed, &public] {
        assert_eq!(post_env(&w.app, &w.ids[0], env).await.0, StatusCode::OK);
    }

    let inbox1 = w.inbox(1).await;
    let inbox2 = w.inbox(2).await;
    assert_eq!(inbox1.len(), 2);
    assert_eq!(
        inbox2.len(),
        1,
        "sealed envelope only goes to its recipient"
    );
    assert!(
        w.inbox(0).await.is_empty(),
        "sender does not receive its own broadcast"
    );
    assert_eq!(
        inbox1[0].1.open(&w.ids[1], w.ids[0].public()).unwrap(),
        b"for 1"
    );

    assert_eq!(
        post_env(&w.app, &w.ids[0], &sealed).await.0,
        StatusCode::CONFLICT
    );
    let intruder = Envelope::public(&w.ids[3], MAILBOX, 1, Kind::Approval, b"x").unwrap();
    assert_eq!(
        post_env(&w.app, &w.ids[3], &intruder).await.0,
        StatusCode::FORBIDDEN
    );
    let read = signed(
        &w.app,
        "/v1/inbox",
        &w.ids[3],
        InboxRead {
            mailbox: MAILBOX,
            after: 0,
            timestamp: NOW,
        },
    )
    .await;
    assert_eq!(read.0, StatusCode::FORBIDDEN);
    let mut forged = Envelope::public(&w.ids[3], MAILBOX, 9, Kind::Approval, b"x").unwrap();
    forged.header.from = w.ids[1].public().sig_pk;
    assert_eq!(
        post_env(&w.app, &w.ids[1], &forged).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        post_env(&w.app, &w.ids[3], &forged).await.0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn log_appends_must_extend_the_head() {
    let mut w = World::new(false).await;
    let key = LogKey::generate(0, &mut w.rng);
    let first = LogEntry::create(
        &w.ids[0],
        &key,
        MAILBOX,
        0,
        [0; 32],
        b"vault created",
        &mut w.rng,
    )
    .unwrap();
    assert_eq!(
        append(&w.app, &w.ids[0], &first).await.0,
        StatusCode::FORBIDDEN
    );

    let members: Vec<_> = w.ids[..3].iter().map(|i| i.public().sig_pk).collect();
    signed(
        &w.app,
        "/v1/mailbox/seal",
        &w.ids[0],
        Seal {
            mailbox: MAILBOX,
            members,
        },
    )
    .await;

    let (_, body) = append(&w.app, &w.ids[0], &first).await;
    assert_eq!(
        decode_body::<AppendResult>(&body).unwrap(),
        AppendResult::Appended { index: 0 }
    );

    let head = first.hash().unwrap();
    let a = LogEntry::create(&w.ids[1], &key, MAILBOX, 1, head, b"proposal A", &mut w.rng).unwrap();
    let b = LogEntry::create(&w.ids[2], &key, MAILBOX, 1, head, b"proposal B", &mut w.rng).unwrap();
    let (_, ra) = append(&w.app, &w.ids[1], &a).await;
    let (_, rb) = append(&w.app, &w.ids[2], &b).await;
    assert_eq!(
        decode_body::<AppendResult>(&ra).unwrap(),
        AppendResult::Appended { index: 1 }
    );
    assert_eq!(
        decode_body::<AppendResult>(&rb).unwrap(),
        AppendResult::Conflict { len: 2 }
    );

    let mut probe =
        LogEntry::create(&w.ids[3], &key, MAILBOX, 9, head, b"probe", &mut w.rng).unwrap();
    probe.header.author = w.ids[1].public().sig_pk;
    let (status, body) = append(&w.app, &w.ids[3], &probe).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(decode_body::<AppendResult>(&body).is_err());
    let (status, body) = append(&w.app, &w.ids[2], &probe).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(decode_body::<AppendResult>(&body).is_err());

    let (_, body) = signed(
        &w.app,
        "/v1/log/read",
        &w.ids[2],
        LogRead {
            mailbox: MAILBOX,
            from: 0,
            timestamp: NOW,
        },
    )
    .await;
    let entries = decode_body::<LogResponse>(&body)
        .unwrap()
        .decoded()
        .unwrap();
    let publics: Vec<_> = w.ids[..3].iter().map(|i| *i.public()).collect();
    let mut chain = Chain::new(MAILBOX);
    for e in entries {
        chain.append(e, &publics).unwrap();
    }
    assert_eq!(chain.len(), 2);
    assert_eq!(chain.entries()[1].decrypt(&key).unwrap(), b"proposal A");
}

#[tokio::test]
async fn member_cap_and_creator_removal() {
    let w = World::new(false).await;
    let full = signed(
        &w.app,
        "/v1/mailbox/join",
        &w.ids[3],
        Join {
            mailbox: MAILBOX,
            join_token: w.token,
        },
    )
    .await;
    assert_eq!(full.0, StatusCode::FORBIDDEN);

    let by_member = signed(
        &w.app,
        "/v1/mailbox/remove",
        &w.ids[1],
        Remove {
            mailbox: MAILBOX,
            member: w.ids[2].public().sig_pk,
        },
    )
    .await;
    assert_eq!(by_member.0, StatusCode::FORBIDDEN);
    let creator_self = signed(
        &w.app,
        "/v1/mailbox/remove",
        &w.ids[0],
        Remove {
            mailbox: MAILBOX,
            member: w.ids[0].public().sig_pk,
        },
    )
    .await;
    assert_eq!(creator_self.0, StatusCode::FORBIDDEN);
    let ok = signed(
        &w.app,
        "/v1/mailbox/remove",
        &w.ids[0],
        Remove {
            mailbox: MAILBOX,
            member: w.ids[2].public().sig_pk,
        },
    )
    .await;
    assert_eq!(ok.0, StatusCode::OK);

    let joined = signed(
        &w.app,
        "/v1/mailbox/join",
        &w.ids[3],
        Join {
            mailbox: MAILBOX,
            join_token: w.token,
        },
    )
    .await;
    assert_eq!(joined.0, StatusCode::OK);
}

#[tokio::test]
async fn an_invite_expires_after_72_hours_unless_the_creator_renews_it() {
    use std::sync::atomic::{AtomicU64, Ordering};
    let clock = Arc::new(AtomicU64::new(NOW));
    let app = Relay::with_clock({
        let clock = clock.clone();
        Arc::new(move || clock.load(Ordering::SeqCst))
    })
    .router();
    let mut rng = StdRng::seed_from_u64(9);
    let ids: Vec<Identity> = (0..4).map(|_| Identity::generate(&mut rng)).collect();
    let mut token = [0u8; 32];
    rng.fill_bytes(&mut token);
    let create = CreateMailbox {
        mailbox: MAILBOX,
        join_token_hash: join_token_hash(&token),
        max_members: 4,
    };
    assert_eq!(
        signed(&app, "/v1/mailbox/create", &ids[0], create).await.0,
        StatusCode::OK
    );
    let join = || Join {
        mailbox: MAILBOX,
        join_token: token,
    };
    let renew = || RenewInvite { mailbox: MAILBOX };
    assert_eq!(
        signed(&app, "/v1/mailbox/join", &ids[1], join()).await.0,
        StatusCode::OK
    );

    clock.store(NOW + INVITE_TTL_SECS + 1, Ordering::SeqCst);
    assert_eq!(
        signed(&app, "/v1/mailbox/join", &ids[2], join()).await.0,
        StatusCode::GONE
    );
    assert_eq!(
        signed(&app, "/v1/mailbox/join", &ids[1], join()).await.0,
        StatusCode::OK
    );

    assert_eq!(
        signed(&app, "/v1/mailbox/renew", &ids[1], renew()).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        signed(&app, "/v1/mailbox/renew", &ids[0], renew()).await.0,
        StatusCode::OK
    );
    assert_eq!(
        signed(&app, "/v1/mailbox/join", &ids[2], join()).await.0,
        StatusCode::OK
    );

    let members = ids[..3].iter().map(|i| i.public().sig_pk).collect();
    let seal = Seal {
        mailbox: MAILBOX,
        members,
    };
    assert_eq!(
        signed(&app, "/v1/mailbox/seal", &ids[0], seal).await.0,
        StatusCode::OK
    );
    assert_eq!(
        signed(&app, "/v1/mailbox/renew", &ids[0], renew()).await.0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn the_creator_can_cancel_a_setup_before_it_is_sealed() {
    let w = World::new(false).await;
    let cancel = || CancelSetup { mailbox: MAILBOX };

    for who in [1, 3] {
        let (status, _) = signed(&w.app, "/v1/mailbox/cancel", &w.ids[who], cancel()).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "identity {who}");
    }
    let members = |who: usize| {
        let w = &w;
        async move {
            signed(
                &w.app,
                "/v1/mailbox/members",
                &w.ids[who],
                MembersRead {
                    mailbox: MAILBOX,
                    timestamp: NOW,
                },
            )
            .await
            .0
        }
    };
    assert_eq!(members(1).await, StatusCode::OK);

    let (status, _) = signed(&w.app, "/v1/mailbox/cancel", &w.ids[0], cancel()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(members(1).await, StatusCode::NOT_FOUND);
    let join = Join {
        mailbox: MAILBOX,
        join_token: w.token,
    };
    let (status, _) = signed(&w.app, "/v1/mailbox/join", &w.ids[3], join).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "the invite no longer works");
    let (status, _) = signed(&w.app, "/v1/mailbox/cancel", &w.ids[0], cancel()).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_sealed_setup_can_be_cancelled_until_the_vault_exists() {
    let mut w = World::new(true).await;
    let cancel = || CancelSetup { mailbox: MAILBOX };
    let (status, _) = signed(&w.app, "/v1/mailbox/cancel", &w.ids[1], cancel()).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let key = LogKey::generate(0, &mut w.rng);
    let first = LogEntry::create(
        &w.ids[0],
        &key,
        MAILBOX,
        0,
        [0; 32],
        b"vault created",
        &mut w.rng,
    )
    .unwrap();
    let (status, _) = append(&w.app, &w.ids[0], &first).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = signed(&w.app, "/v1/mailbox/cancel", &w.ids[0], cancel()).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = signed(
        &w.app,
        "/v1/mailbox/members",
        &w.ids[1],
        MembersRead {
            mailbox: MAILBOX,
            timestamp: NOW,
        },
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the vault is untouched");
}

#[tokio::test]
async fn the_creator_cancels_a_sealed_setup_with_no_keys() {
    let w = World::new(true).await;
    let (status, _) = signed(
        &w.app,
        "/v1/mailbox/cancel",
        &w.ids[0],
        CancelSetup { mailbox: MAILBOX },
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = signed(
        &w.app,
        "/v1/mailbox/members",
        &w.ids[1],
        MembersRead {
            mailbox: MAILBOX,
            timestamp: NOW,
        },
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "the setup is gone");
}

#[tokio::test]
async fn health_answers_get() {
    let app = Relay::new().router();
    let response = app
        .clone()
        .oneshot(Request::get("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"ok");
    let response = app
        .oneshot(Request::post("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn too_new_versions_get_upgrade_required() {
    use zafe_proto::{relay::UNSUPPORTED_VERSION_HEADER, version};

    async fn call_full(app: &Router, path: &str, body: Vec<u8>) -> (StatusCode, Option<String>) {
        let response = app
            .clone()
            .oneshot(Request::post(path).body(Body::from(body)).unwrap())
            .await
            .unwrap();
        let header = response
            .headers()
            .get(UNSUPPORTED_VERSION_HEADER)
            .map(|v| v.to_str().unwrap().to_owned());
        (response.status(), header)
    }
    fn bump(mut bytes: Vec<u8>) -> Vec<u8> {
        let v = u16::from_le_bytes([bytes[0], bytes[1]]) + 1;
        bytes[..2].copy_from_slice(&v.to_le_bytes());
        bytes
    }

    let mut w = World::new(true).await;
    let request = Signed::new(
        &w.ids[1],
        MembersRead {
            mailbox: MAILBOX,
            timestamp: NOW,
        },
    )
    .unwrap()
    .to_bytes()
    .unwrap();
    assert_eq!(
        call_full(&w.app, "/v1/mailbox/members", request.clone()).await,
        (StatusCode::OK, None)
    );
    assert_eq!(
        call_full(&w.app, "/v1/mailbox/members", bump(request)).await,
        (
            StatusCode::UPGRADE_REQUIRED,
            Some(version::RELAY_API.to_string())
        )
    );

    let envelope = Envelope::public(&w.ids[1], MAILBOX, 1, Kind::DkgEcho, b"hi")
        .unwrap()
        .to_bytes()
        .unwrap();
    let post = |envelope| {
        Signed::new(&w.ids[1], PostEnvelope { envelope })
            .unwrap()
            .to_bytes()
            .unwrap()
    };
    assert_eq!(
        call_full(&w.app, "/v1/envelope/send", post(bump(envelope.clone()))).await,
        (
            StatusCode::UPGRADE_REQUIRED,
            Some(version::ENVELOPE.to_string())
        )
    );

    let key = LogKey::generate(0, &mut w.rng);
    let entry = LogEntry::create(&w.ids[0], &key, MAILBOX, 0, [0; 32], b"x", &mut w.rng)
        .unwrap()
        .to_bytes()
        .unwrap();
    for (path, body) in [
        ("/v1/envelope", envelope),
        ("/v1/log/append", entry),
        ("/v1/mailbox/replace", Vec::new()),
    ] {
        assert_eq!(
            call_full(&w.app, path, body).await,
            (
                StatusCode::UPGRADE_REQUIRED,
                Some(version::RELAY_API.to_string())
            ),
            "{path}"
        );
    }
}

fn limited(limits: zafe_relay::limits::Limits) -> Router {
    Relay::with_clock(Arc::new(|| NOW))
        .with_limits(limits)
        .router()
}

fn members_read() -> MembersRead {
    MembersRead {
        mailbox: MAILBOX,
        timestamp: NOW,
    }
}

#[tokio::test]
async fn keys_are_rate_limited_after_their_signature_verifies() {
    use zafe_relay::limits::{Limits, Rate};
    let app = limited(Limits {
        per_key: Some(Rate::new(60, 2)),
        ..Limits::none()
    });
    let mut rng = StdRng::seed_from_u64(9);
    let (alice, bob) = (Identity::generate(&mut rng), Identity::generate(&mut rng));

    let mut forged = Signed::new(&bob, members_read()).unwrap();
    forged.signer = *alice.public();
    for _ in 0..5 {
        let (status, _) = call(&app, "/v1/mailbox/members", forged.to_bytes().unwrap()).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    for _ in 0..2 {
        let (status, _) = signed(&app, "/v1/mailbox/members", &alice, members_read()).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
    let response = app
        .clone()
        .oneshot(
            Request::post("/v1/mailbox/members")
                .body(Body::from(
                    Signed::new(&alice, members_read())
                        .unwrap()
                        .to_bytes()
                        .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(response.headers()["retry-after"], "1");
    let (status, _) = signed(&app, "/v1/mailbox/members", &bob, members_read()).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn client_addresses_are_rate_limited() {
    use zafe_relay::limits::{Limits, Rate};
    let app = limited(Limits {
        per_ip: Some(Rate::new(60, 1)),
        client_ip_header: Some("fly-client-ip".into()),
        ..Limits::none()
    });
    let health = |ip: &'static str| {
        app.clone().oneshot(
            Request::get("/health")
                .header("fly-client-ip", ip)
                .body(Body::empty())
                .unwrap(),
        )
    };
    assert_eq!(
        health("203.0.113.1").await.unwrap().status(),
        StatusCode::OK
    );
    assert_eq!(
        health("203.0.113.1").await.unwrap().status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        health("203.0.113.2").await.unwrap().status(),
        StatusCode::OK
    );
}

fn create_request(who: &Identity, mailbox: [u8; 16]) -> Vec<u8> {
    Signed::new(
        who,
        CreateMailbox {
            mailbox,
            join_token_hash: join_token_hash(&[3; 32]),
            max_members: 3,
        },
    )
    .unwrap()
    .to_bytes()
    .unwrap()
}

#[tokio::test]
async fn mailbox_creation_is_limited_per_address() {
    use zafe_relay::limits::{DailyRate, Limits};
    let app = limited(Limits {
        creates_per_ip: Some(DailyRate::new(24, 2)),
        client_ip_header: Some("fly-client-ip".into()),
        ..Limits::none()
    });
    let mut rng = StdRng::seed_from_u64(11);
    let mut create = |ip: &'static str, n: u8| {
        let who = Identity::generate(&mut rng);
        app.clone().oneshot(
            Request::post("/v1/mailbox/create")
                .header("fly-client-ip", ip)
                .body(Body::from(create_request(&who, [n; 16])))
                .unwrap(),
        )
    };
    assert_eq!(
        create("203.0.113.1", 1).await.unwrap().status(),
        StatusCode::OK
    );
    assert_eq!(
        create("203.0.113.1", 2).await.unwrap().status(),
        StatusCode::OK
    );
    let refused = create("203.0.113.1", 3).await.unwrap();
    assert_eq!(refused.status(), StatusCode::TOO_MANY_REQUESTS);
    let retry: u64 = refused.headers()["retry-after"]
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!((3600..=3601).contains(&retry), "retry after {retry}");
    assert_eq!(
        create("203.0.113.2", 4).await.unwrap().status(),
        StatusCode::OK
    );
    let health = app
        .clone()
        .oneshot(
            Request::get("/health")
                .header("fly-client-ip", "203.0.113.1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(health.status(), StatusCode::OK);
}

#[tokio::test]
async fn one_key_can_create_only_a_few_mailboxes() {
    use zafe_relay::quota::Quotas;
    let app = Relay::with_clock(Arc::new(|| NOW))
        .with_quotas(Quotas {
            mailboxes_per_key: Some(2),
            ..Quotas::none()
        })
        .router();
    let alice = Identity::generate(&mut StdRng::seed_from_u64(12));
    for n in 1..=2 {
        let (status, _) = call(&app, "/v1/mailbox/create", create_request(&alice, [n; 16])).await;
        assert_eq!(status, StatusCode::OK);
    }
    let (status, _) = call(&app, "/v1/mailbox/create", create_request(&alice, [1; 16])).await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, body) = call(&app, "/v1/mailbox/create", create_request(&alice, [3; 16])).await;
    assert_eq!(status, StatusCode::INSUFFICIENT_STORAGE);
    assert!(String::from_utf8_lossy(&body).contains("too many vaults"));
    let bob = Identity::generate(&mut StdRng::seed_from_u64(13));
    let (status, _) = call(&app, "/v1/mailbox/create", create_request(&bob, [3; 16])).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn oversized_bodies_are_refused() {
    let app = Relay::with_clock(Arc::new(|| NOW)).router();
    let (status, _) = call(
        &app,
        "/v1/log/append-labelled",
        vec![0u8; zafe_relay::MAX_BODY_BYTES + 1],
    )
    .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn recovery_values_keep_the_first_of_each_and_who_may_post() {
    use zafe_proto::relay::{RecoveryPut, RecoveryRead, RecoveryValueKind as K, RecoveryValues};
    let w = World::new(true).await;
    let (new, cosigner) = (&w.ids[3], &w.ids[1]);
    let subject = new.public().sig_pk;
    let put = |kind, value: u8| RecoveryPut {
        subject,
        cosigner: cosigner.public().sig_pk,
        kind,
        value: [value; 32],
        timestamp: NOW,
    };
    let path = "/v1/seat-check/put";
    assert_eq!(
        signed(&w.app, path, new, put(K::Commit, 1)).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        signed(&w.app, path, new, put(K::Announce, 0)).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        signed(&w.app, path, &w.ids[2], put(K::Announce, 0)).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        signed(&w.app, path, cosigner, put(K::Announce, 0)).await.0,
        StatusCode::OK
    );
    assert_eq!(
        signed(&w.app, path, cosigner, put(K::Commit, 1)).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        signed(&w.app, path, new, put(K::Challenge, 1)).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        signed(&w.app, path, new, put(K::Commit, 1)).await.0,
        StatusCode::OK
    );
    assert_eq!(
        signed(&w.app, path, new, put(K::Commit, 2)).await.0,
        StatusCode::OK
    );
    assert_eq!(
        signed(&w.app, path, cosigner, put(K::Challenge, 3)).await.0,
        StatusCode::OK
    );

    let (status, body) = signed(
        &w.app,
        "/v1/seat-check/read",
        cosigner,
        RecoveryRead {
            subject,
            timestamp: NOW,
        },
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let values: Vec<RecoveryPut> = decode_body::<RecoveryValues>(&body)
        .unwrap()
        .values
        .iter()
        .map(|b| {
            Signed::<RecoveryPut>::from_bytes(b)
                .unwrap()
                .verify()
                .unwrap()
                .clone()
        })
        .collect();
    assert_eq!(
        values,
        vec![put(K::Announce, 0), put(K::Commit, 1), put(K::Challenge, 3)]
    );
}

#[tokio::test]
async fn recovery_pq_keys_are_posted_only_by_their_owner_at_full_size() {
    use zafe_proto::relay::{PqKeyPut, RecoveryRead, RecoveryValues, PQ_KEY_LEN};
    let w = World::new(true).await;
    let (new, cosigner) = (&w.ids[3], &w.ids[1]);
    let subject = new.public().sig_pk;
    let put = |owner: &Identity, byte: u8, len: usize| PqKeyPut {
        subject,
        owner: owner.public().sig_pk,
        key: vec![byte; len],
        timestamp: NOW,
    };
    let path = "/v1/seat-check/pq";
    assert_eq!(
        signed(&w.app, path, cosigner, put(new, 1, PQ_KEY_LEN))
            .await
            .0,
        StatusCode::FORBIDDEN,
        "nobody posts another identity's key"
    );
    assert_eq!(
        signed(&w.app, path, new, put(new, 1, PQ_KEY_LEN - 1))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        signed(&w.app, path, new, put(new, 1, PQ_KEY_LEN)).await.0,
        StatusCode::OK
    );
    assert_eq!(
        signed(&w.app, path, new, put(new, 2, PQ_KEY_LEN)).await.0,
        StatusCode::OK
    );
    assert_eq!(
        signed(&w.app, path, cosigner, put(cosigner, 3, PQ_KEY_LEN))
            .await
            .0,
        StatusCode::OK
    );

    let (status, body) = signed(
        &w.app,
        "/v1/seat-check/pq-read",
        cosigner,
        RecoveryRead {
            subject,
            timestamp: NOW,
        },
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let mut keys: Vec<PqKeyPut> = decode_body::<RecoveryValues>(&body)
        .unwrap()
        .values
        .iter()
        .map(|b| {
            Signed::<PqKeyPut>::from_bytes(b)
                .unwrap()
                .verify()
                .unwrap()
                .clone()
        })
        .collect();
    keys.sort_by_key(|k| k.owner);
    let mut expected = vec![put(new, 1, PQ_KEY_LEN), put(cosigner, 3, PQ_KEY_LEN)];
    expected.sort_by_key(|k| k.owner);
    assert_eq!(keys, expected, "the first key per owner stays");

    for i in 0..zafe_relay::MAX_PQ_KEYS_PER_SUBJECT - 2 {
        let filler = Identity::generate(&mut StdRng::seed_from_u64(1000 + i as u64));
        assert_eq!(
            signed(&w.app, path, &filler, put(&filler, 4, PQ_KEY_LEN))
                .await
                .0,
            StatusCode::OK
        );
    }
    let late = Identity::generate(&mut StdRng::seed_from_u64(999));
    assert_eq!(
        signed(&w.app, path, &late, put(&late, 5, PQ_KEY_LEN))
            .await
            .0,
        StatusCode::FORBIDDEN,
        "a recovery holds at most MAX_PQ_KEYS_PER_SUBJECT keys"
    );
    assert_eq!(
        signed(&w.app, path, cosigner, put(cosigner, 3, PQ_KEY_LEN))
            .await
            .0,
        StatusCode::OK,
        "an owner already there can post again"
    );
}

#[tokio::test]
async fn recovery_checks_are_charged_per_address_and_junk_cant_lock_out_a_cosigner() {
    use zafe_proto::relay::{RecoveryPut, RecoveryValueKind as K};
    use zafe_relay::limits::{DailyRate, Limits};
    let app = limited(Limits {
        creates_per_ip: Some(DailyRate::new(24, 2)),
        client_ip_header: Some("fly-client-ip".into()),
        ..Limits::none()
    });
    let mut rng = StdRng::seed_from_u64(14);
    let subject = Identity::generate(&mut rng).public().sig_pk;
    let put = |who: &Identity, ip: String| {
        let body = Signed::new(
            who,
            RecoveryPut {
                subject,
                cosigner: who.public().sig_pk,
                kind: K::Announce,
                value: [0; 32],
                timestamp: NOW,
            },
        )
        .unwrap()
        .to_bytes()
        .unwrap();
        app.clone().oneshot(
            Request::post("/v1/seat-check/put")
                .header("fly-client-ip", ip)
                .body(Body::from(body))
                .unwrap(),
        )
    };
    let mut fresh = || Identity::generate(&mut rng);
    for expected in [
        StatusCode::OK,
        StatusCode::OK,
        StatusCode::TOO_MANY_REQUESTS,
    ] {
        let status = put(&fresh(), "203.0.113.1".into()).await.unwrap().status();
        assert_eq!(status, expected);
    }
    for i in 0..20 {
        put(&fresh(), format!("198.51.100.{i}")).await.unwrap();
    }
    let cosigner = fresh();
    assert_eq!(
        put(&cosigner, "203.0.113.9".into()).await.unwrap().status(),
        StatusCode::OK
    );
}
