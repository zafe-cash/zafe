//! Batch 4 review: one member must not be able to shut another member's inbox for everyone.

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use http_body_util::BodyExt;
use rand::{rngs::StdRng, SeedableRng};
use tower::ServiceExt;
use zafe_proto::{
    relay::{join_token_hash, CreateMailbox, Join, PostEnvelope, Seal, Signed},
    Envelope, Identity, Kind, MailboxId,
};
use zafe_relay::{quota::Quotas, Relay};

const MAILBOX: MailboxId = [7; 16];
const TOKEN: [u8; 32] = [4; 32];

async fn signed<T: serde::Serialize + serde::de::DeserializeOwned>(
    app: &Router,
    path: &str,
    who: &Identity,
    payload: T,
) -> StatusCode {
    let body = Signed::new(who, payload).unwrap().to_bytes().unwrap();
    let response = app
        .clone()
        .oneshot(Request::post(path).body(Body::from(body)).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let _ = response.into_body().collect().await;
    status
}

async fn mailbox(app: &Router, ids: &[Identity]) {
    let create = CreateMailbox {
        mailbox: MAILBOX,
        join_token_hash: join_token_hash(&TOKEN),
        max_members: ids.len() as u16,
    };
    assert_eq!(
        signed(app, "/v1/mailbox/create", &ids[0], create).await,
        StatusCode::OK
    );
    for id in &ids[1..] {
        let join = Join {
            mailbox: MAILBOX,
            join_token: TOKEN,
        };
        assert_eq!(
            signed(app, "/v1/mailbox/join", id, join).await,
            StatusCode::OK
        );
    }
    let seal = Seal {
        mailbox: MAILBOX,
        members: ids.iter().map(|i| i.public().sig_pk).collect(),
    };
    assert_eq!(
        signed(app, "/v1/mailbox/seal", &ids[0], seal).await,
        StatusCode::OK
    );
}

async fn send(app: &Router, from: &Identity, to: &Identity, seq: u64, kind: Kind) -> StatusCode {
    let mut rng = StdRng::seed_from_u64(seq);
    let envelope = Envelope::sealed(from, to.public(), MAILBOX, seq, kind, b"x", &mut rng)
        .unwrap()
        .to_bytes()
        .unwrap();
    signed(app, "/v1/envelope/send", from, PostEnvelope { envelope }).await
}

/// Member 0 fills member 1's inbox with envelopes of a kind member 1's phone never acks
/// (it acks only what it handles: `node::respond` skips other kinds, and move notices are
/// never acked), so they stay until pruned, 30 days. Then member 2, the leader, can't send
/// member 1 a signing request: every send to member 1 gets 507 for a month. The same works
/// on a seat move's pending key before the helpers' repair parts arrive. The cap bounds
/// each (sender, recipient) pair, not the inbox for every sender.
#[tokio::test]
async fn one_member_cant_fill_another_members_inbox_for_everyone() {
    let cap = 8;
    let relay = Relay::new().with_quotas(Quotas {
        envelopes_per_pair: Some(cap),
        ..Quotas::none()
    });
    let app = relay.router();
    let mut rng = StdRng::seed_from_u64(1);
    let ids: Vec<Identity> = (0..3).map(|_| Identity::generate(&mut rng)).collect();
    mailbox(&app, &ids).await;

    let mut flooded = 0;
    for seq in 1..=cap {
        if send(&app, &ids[0], &ids[1], seq, Kind::DkgEcho).await == StatusCode::OK {
            flooded += 1;
        }
    }
    assert!(flooded > 0, "the flood was refused outright");
    assert_eq!(
        send(&app, &ids[2], &ids[1], 1, Kind::SigningRequest).await,
        StatusCode::OK,
        "member 1 must still receive the leader's signing request"
    );
}
