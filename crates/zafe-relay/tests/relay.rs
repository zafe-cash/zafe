//! Relay behavior over its HTTP API, driven in-process.

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
        InboxResponse, Join, LogRead, LogResponse, MembersRead, MembersResponse, Remove,
        RenewInvite, Seal, Signed, INVITE_TTL_SECS,
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

struct World {
    app: Router,
    rng: StdRng,
    ids: Vec<Identity>,
    token: [u8; 32],
}

impl World {
    /// Creator (ids[0]) opens the mailbox; ids[1], ids[2] join; ids[3] is an outsider.
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

    // Wrong token is refused.
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

    // Only the creator can seal, and only with exactly the current member set.
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

    // After sealing, even the right token cannot add members.
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

    // Outsiders cannot list members; stale requests are refused.
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
        assert_eq!(
            call(&w.app, "/v1/envelope", env.to_bytes().unwrap())
                .await
                .0,
            StatusCode::OK
        );
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

    // Replay of an already-delivered envelope is refused.
    assert_eq!(
        call(&w.app, "/v1/envelope", sealed.to_bytes().unwrap())
            .await
            .0,
        StatusCode::CONFLICT
    );
    // Outsiders cannot post or read.
    let intruder = Envelope::public(&w.ids[3], MAILBOX, 1, Kind::Approval, b"x").unwrap();
    assert_eq!(
        call(&w.app, "/v1/envelope", intruder.to_bytes().unwrap())
            .await
            .0,
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
    // A forged envelope claiming a member as sender fails verification.
    let mut forged = Envelope::public(&w.ids[3], MAILBOX, 9, Kind::Approval, b"x").unwrap();
    forged.header.from = w.ids[1].public().sig_pk;
    assert_eq!(
        call(&w.app, "/v1/envelope", forged.to_bytes().unwrap())
            .await
            .0,
        StatusCode::UNAUTHORIZED
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
    // No log before membership is sealed.
    assert_eq!(
        call(&w.app, "/v1/log/append", first.to_bytes().unwrap())
            .await
            .0,
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

    let (_, body) = call(&w.app, "/v1/log/append", first.to_bytes().unwrap()).await;
    assert_eq!(
        decode_body::<AppendResult>(&body).unwrap(),
        AppendResult::Appended { index: 0 }
    );

    // Two members race to append index 1: the second gets a conflict.
    let head = first.hash().unwrap();
    let a = LogEntry::create(&w.ids[1], &key, MAILBOX, 1, head, b"proposal A", &mut w.rng).unwrap();
    let b = LogEntry::create(&w.ids[2], &key, MAILBOX, 1, head, b"proposal B", &mut w.rng).unwrap();
    let (_, ra) = call(&w.app, "/v1/log/append", a.to_bytes().unwrap()).await;
    let (_, rb) = call(&w.app, "/v1/log/append", b.to_bytes().unwrap()).await;
    assert_eq!(
        decode_body::<AppendResult>(&ra).unwrap(),
        AppendResult::Appended { index: 1 }
    );
    assert_eq!(
        decode_body::<AppendResult>(&rb).unwrap(),
        AppendResult::Conflict { len: 2 }
    );

    // A non-member who knows a member's key can't probe the log length: an entry signed
    // by someone else, at a wrong index, is refused before any conflict is reported.
    let mut probe =
        LogEntry::create(&w.ids[3], &key, MAILBOX, 9, head, b"probe", &mut w.rng).unwrap();
    probe.header.author = w.ids[1].public().sig_pk;
    let (status, body) = call(&w.app, "/v1/log/append", probe.to_bytes().unwrap()).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(decode_body::<AppendResult>(&body).is_err());

    // A member reads the log back and verifies the whole chain.
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
    // Mailbox is full (3 of 3): a fourth identity with the right token is refused.
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

    // Only the creator can remove, and only before sealing; the creator cannot be removed.
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

    // A slot is free again.
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

    // Just past 72 hours: a new member is turned away, one already in can rejoin.
    clock.store(NOW + INVITE_TTL_SECS + 1, Ordering::SeqCst);
    assert_eq!(
        signed(&app, "/v1/mailbox/join", &ids[2], join()).await.0,
        StatusCode::GONE
    );
    assert_eq!(
        signed(&app, "/v1/mailbox/join", &ids[1], join()).await.0,
        StatusCode::OK
    );

    // Only the creator renews; then the invite works again for 72 hours.
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

    // A locked setup's invite can't be renewed (nobody else can join anyway).
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

    // Not a member, not the creator: refused, and nothing is deleted.
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

    // The creator cancels: the mailbox, its members and the invite are gone.
    let (status, _) = signed(&w.app, "/v1/mailbox/cancel", &w.ids[0], cancel()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(members(1).await, StatusCode::NOT_FOUND);
    let join = Join {
        mailbox: MAILBOX,
        join_token: w.token,
    };
    let (status, _) = signed(&w.app, "/v1/mailbox/join", &w.ids[3], join).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "the invite no longer works");
    // A second cancel finds nothing.
    let (status, _) = signed(&w.app, "/v1/mailbox/cancel", &w.ids[0], cancel()).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_sealed_setup_can_be_cancelled_until_the_vault_exists() {
    // A locked setup whose keys were never made (a member never came back): the creator
    // can still cancel it, nobody else can.
    let mut w = World::new(true).await;
    let cancel = || CancelSetup { mailbox: MAILBOX };
    let (status, _) = signed(&w.app, "/v1/mailbox/cancel", &w.ids[1], cancel()).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    // Once the vault's first log entry exists, it can't be cancelled any more.
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
    let (status, _) = call(&w.app, "/v1/log/append", first.to_bytes().unwrap()).await;
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
    // GET only: the API itself is POST.
    let response = app
        .oneshot(Request::post("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

/// Bodies in a version the relay doesn't speak get 426 with the supported version, not a
/// generic "malformed request".
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
    assert_eq!(
        call_full(&w.app, "/v1/envelope", bump(envelope)).await,
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
    assert_eq!(
        call_full(&w.app, "/v1/log/append", bump(entry)).await,
        (
            StatusCode::UPGRADE_REQUIRED,
            Some(version::LOG_ENTRY.to_string())
        )
    );
}

// --- Limits -----------------------------------------------------------------------------

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

    // Forged requests claiming Alice's key are refused before they cost her anything.
    let mut forged = Signed::new(&bob, members_read()).unwrap();
    forged.signer = *alice.public();
    for _ in 0..5 {
        let (status, _) = call(&app, "/v1/mailbox/members", forged.to_bytes().unwrap()).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    // Alice's burst of two goes through (unknown mailbox: 404), the third is limited.
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
    // Bob has his own bucket.
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
        // A fresh key each time, as the app makes per vault.
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
    // 24 a day: the next one is about an hour away.
    let retry: u64 = refused.headers()["retry-after"]
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!((3600..=3601).contains(&retry), "retry after {retry}");
    // Another address has its own allowance; other requests aren't affected.
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
    // Retrying an existing mailbox still says it exists, not over quota.
    let (status, _) = call(&app, "/v1/mailbox/create", create_request(&alice, [1; 16])).await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, body) = call(&app, "/v1/mailbox/create", create_request(&alice, [3; 16])).await;
    assert_eq!(status, StatusCode::INSUFFICIENT_STORAGE);
    assert!(String::from_utf8_lossy(&body).contains("too many vaults"));
    // The refused mailbox wasn't kept: another key can create it.
    let bob = Identity::generate(&mut StdRng::seed_from_u64(13));
    let (status, _) = call(&app, "/v1/mailbox/create", create_request(&bob, [3; 16])).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn oversized_bodies_are_refused() {
    let app = Relay::with_clock(Arc::new(|| NOW)).router();
    let (status, _) = call(
        &app,
        "/v1/log/append",
        vec![0u8; zafe_relay::MAX_BODY_BYTES + 1],
    )
    .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
}

/// A seat recovery's safety values: the new phone (subject) leaves its commitment and
/// secret, others one challenge each; the first of each stays; the relay hands back the
/// signed values.
#[tokio::test]
async fn recovery_values_keep_the_first_of_each_and_who_may_post() {
    use zafe_proto::relay::{RecoveryPut, RecoveryRead, RecoveryValueKind as K, RecoveryValues};
    let w = World::new(true).await;
    let (new, cosigner) = (&w.ids[3], &w.ids[1]);
    let subject = new.public().sig_pk;
    let put = |kind, value: u8| RecoveryPut {
        subject,
        kind,
        value: [value; 32],
        timestamp: NOW,
    };
    let path = "/v1/recovery/put";
    // Only the new phone commits and reveals; only others challenge.
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
    // A second commitment doesn't replace the first.
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
        "/v1/recovery/read",
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
    assert_eq!(values, vec![put(K::Commit, 1), put(K::Challenge, 3)]);

    // A stranger can't bury the real challenges under many of its own.
    let mut rng = StdRng::seed_from_u64(9);
    let mut refused = false;
    for _ in 0..20 {
        let stranger = Identity::generate(&mut rng);
        if signed(&w.app, path, &stranger, put(K::Challenge, 4))
            .await
            .0
            == StatusCode::FORBIDDEN
        {
            refused = true;
            break;
        }
    }
    assert!(refused, "challenges per recovering phone are capped");
}
