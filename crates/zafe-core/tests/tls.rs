use std::{net::SocketAddr, sync::Arc};

use rand::{rngs::StdRng, SeedableRng};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc,
};
use tokio_rustls::{
    rustls::{
        self,
        pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer},
    },
    server::TlsStream,
    TlsAcceptor,
};
use zafe_core::{relay_client::RelayClient, wallet};
use zafe_proto::Identity;

struct TlsListener {
    tcp: TcpListener,
    acceptor: TlsAcceptor,
    handshakes: mpsc::UnboundedSender<Result<(), String>>,
}

impl axum::serve::Listener for TlsListener {
    type Io = TlsStream<TcpStream>;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            let Ok((tcp, addr)) = self.tcp.accept().await else {
                continue;
            };
            match self.acceptor.accept(tcp).await {
                Ok(stream) => {
                    let _ = self.handshakes.send(Ok(()));
                    return (stream, addr);
                }
                Err(e) => {
                    let _ = self.handshakes.send(Err(e.to_string()));
                }
            }
        }
    }

    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        self.tcp.local_addr()
    }
}

struct Server {
    port: u16,
    cert: CertificateDer<'static>,
    handshakes: mpsc::UnboundedReceiver<Result<(), String>>,
}

async fn start_tls_relay() -> Server {
    let certified = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).unwrap();
    let cert = certified.cert.der().clone();
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(certified.key_pair.serialize_der()));
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(vec![cert.clone()], key)
    .unwrap();

    let tcp = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = tcp.local_addr().unwrap().port();
    let (tx, handshakes) = mpsc::unbounded_channel();
    let listener = TlsListener {
        tcp,
        acceptor: TlsAcceptor::from(Arc::new(config)),
        handshakes: tx,
    };
    tokio::spawn(async move {
        axum::serve(listener, zafe_relay::Relay::new().router())
            .await
            .unwrap();
    });
    Server {
        port,
        cert,
        handshakes,
    }
}

async fn create(relay: &RelayClient) -> Result<(), zafe_core::relay_client::RelayClientError> {
    let mut rng = StdRng::seed_from_u64(7);
    let who = Identity::generate(&mut rng);
    relay.create_mailbox(&who, [1; 16], &[2; 32], 3).await
}

#[tokio::test]
async fn relay_client_speaks_https_to_a_trusted_certificate() {
    let mut server = start_tls_relay().await;
    let relay =
        RelayClient::with_extra_root(format!("https://localhost:{}", server.port), &server.cert)
            .unwrap();
    create(&relay).await.expect("request over TLS");
    assert_eq!(server.handshakes.recv().await, Some(Ok(())));
}

#[tokio::test]
async fn relay_client_rejects_an_untrusted_certificate() {
    let mut server = start_tls_relay().await;
    let relay = RelayClient::new(format!("https://localhost:{}", server.port));
    let err = create(&relay)
        .await
        .expect_err("self-signed must be refused");
    assert!(
        err.to_string().to_lowercase().contains("certificate"),
        "{err}"
    );
    assert!(
        matches!(
            err,
            zafe_core::relay_client::RelayClientError::Transport {
                failure: zafe_core::net::NetFailure::Tls,
                ..
            }
        ),
        "{err:?}"
    );
    assert!(matches!(server.handshakes.recv().await, Some(Err(_))));
}

#[tokio::test]
async fn relay_client_checks_the_hostname() {
    let mut server = start_tls_relay().await;
    let relay =
        RelayClient::with_extra_root(format!("https://127.0.0.1:{}", server.port), &server.cert)
            .unwrap();
    let err = create(&relay)
        .await
        .expect_err("wrong name must be refused");
    assert!(
        err.to_string().to_lowercase().contains("certificate"),
        "{err}"
    );
    assert!(
        matches!(
            err,
            zafe_core::relay_client::RelayClientError::Transport {
                failure: zafe_core::net::NetFailure::Tls,
                ..
            }
        ),
        "{err:?}"
    );
    assert!(matches!(server.handshakes.recv().await, Some(Err(_))));
}

#[tokio::test]
async fn lightwalletd_client_uses_tls_for_https() {
    let mut server = start_tls_relay().await;
    let err = wallet::connect(&format!("https://localhost:{}", server.port))
        .await
        .expect_err("self-signed must be refused");
    assert!(
        err.to_string().to_lowercase().contains("certificate"),
        "{err}"
    );
    assert!(
        matches!(
            err,
            wallet::WalletError::Remote {
                failure: zafe_core::net::NetFailure::Tls,
                ..
            }
        ),
        "{err:?}"
    );
    assert!(matches!(server.handshakes.recv().await, Some(Err(_))));
}

#[tokio::test]
#[ignore = "needs internet"]
async fn public_testnet_lightwalletd_over_tls() {
    let endpoint = std::env::var("ZAFE_TESTNET_LIGHTWALLETD")
        .unwrap_or_else(|_| "https://testnet.zec.rocks:443".into());
    let mut client = wallet::connect(&endpoint).await.expect("connect");
    let height = wallet::latest_height(&mut client).await.expect("tip");
    assert!(height > 3_000_000, "testnet tip {height}");
}
