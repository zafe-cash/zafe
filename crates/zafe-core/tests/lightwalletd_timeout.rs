use std::time::{Duration, Instant};

use tokio::net::TcpListener;
use zafe_core::wallet;

#[tokio::test]
async fn silent_lightwalletd_times_out() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((socket, _)) = listener.accept().await {
            held.push(socket);
        }
    });

    let started = Instant::now();
    let result = tokio::time::timeout(Duration::from_secs(90), async {
        let mut client = wallet::connect(&format!("http://{addr}")).await?;
        wallet::latest_height(&mut client).await
    })
    .await
    .expect("the wallet call hung instead of timing out");
    assert!(result.is_err(), "a silent server can't return a height");
    assert!(
        started.elapsed()
            <= wallet::CONNECT_TIMEOUT + wallet::REQUEST_TIMEOUT + Duration::from_secs(5),
        "took {:?}",
        started.elapsed()
    );
}
