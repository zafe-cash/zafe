//! A relay can't make the client read an unbounded response (audit L6).

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use zafe_core::relay_client::{RelayClient, RelayClientError};

/// Answers every connection with a chunked body that never ends (no length to refuse up
/// front), so only the client's streaming cap stops it.
async fn endless_relay() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut request = [0u8; 4096];
                let _ = socket.read(&mut request).await;
                let head = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n";
                if socket.write_all(head.as_bytes()).await.is_err() {
                    return;
                }
                let data = vec![b'x'; 1 << 20];
                let mut chunk = format!("{:x}\r\n", data.len()).into_bytes();
                chunk.extend_from_slice(&data);
                chunk.extend_from_slice(b"\r\n");
                while socket.write_all(&chunk).await.is_ok() {}
            });
        }
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn an_endless_response_is_cut_off() {
    let relay = RelayClient::new(endless_relay().await);
    match relay.health().await {
        Err(RelayClientError::ResponseTooLarge) => {}
        other => panic!("{other:?}"),
    }
}
