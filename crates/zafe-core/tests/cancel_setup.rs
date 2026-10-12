//! A creator can abandon a vault setup before membership is locked: the invite stops
//! working, members waiting in it see the setup is gone, and nobody else can cancel it.

use rand::{rngs::StdRng, SeedableRng};
use zafe_core::{
    node::{cancel_setup, create_vault, join_vault, membership, seal, NodeError},
    relay_client::{RelayClient, RelayClientError},
};
use zafe_proto::Identity;
use zafe_relay::Relay;

async fn serve(relay: Relay) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, relay.router()).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn cancelling_a_setup_ends_the_invite() {
    let relay = RelayClient::new(serve(Relay::new()).await);
    let mut rng = StdRng::seed_from_u64(9);
    let (creator, joiner, stranger) = (
        Identity::generate(&mut rng),
        Identity::generate(&mut rng),
        Identity::generate(&mut rng),
    );
    let invite = create_vault(&relay, &creator, "Grants", 2, 3, &mut rng)
        .await
        .unwrap();
    join_vault(&relay, &joiner, &invite).await.unwrap();

    // Only the creator can cancel.
    assert!(matches!(
        cancel_setup(&relay, &joiner, &invite).await,
        Err(NodeError::Verification(_))
    ));
    assert!(matches!(
        cancel_setup(&relay, &stranger, &invite).await,
        Err(NodeError::Verification(_))
    ));
    membership(&relay, &joiner, &invite).await.unwrap();

    // The creator cancels: the joiner's next read says the mailbox is gone, the invite
    // can't be used again and a second cancel finds nothing.
    assert!(cancel_setup(&relay, &creator, &invite).await.unwrap());
    assert!(matches!(
        membership(&relay, &joiner, &invite).await,
        Err(NodeError::Relay(RelayClientError::Status {
            status: 404,
            ..
        }))
    ));
    assert!(join_vault(&relay, &stranger, &invite).await.is_err());
    assert!(!cancel_setup(&relay, &creator, &invite).await.unwrap());

    // A new setup with the corrected details works.
    let again = create_vault(&relay, &creator, "Grants committee", 2, 2, &mut rng)
        .await
        .unwrap();
    join_vault(&relay, &joiner, &again).await.unwrap();
    seal(&relay, &creator, &again).await.unwrap();
}

#[tokio::test]
async fn a_locked_membership_cannot_be_cancelled() {
    let relay = RelayClient::new(serve(Relay::new()).await);
    let mut rng = StdRng::seed_from_u64(10);
    let (creator, joiner) = (Identity::generate(&mut rng), Identity::generate(&mut rng));
    let invite = create_vault(&relay, &creator, "Grants", 2, 2, &mut rng)
        .await
        .unwrap();
    join_vault(&relay, &joiner, &invite).await.unwrap();
    seal(&relay, &creator, &invite).await.unwrap();
    assert!(cancel_setup(&relay, &creator, &invite).await.is_err());
    membership(&relay, &joiner, &invite).await.unwrap();
}
