//! Compares the consensus branch id lightwalletd reports for its tip with the one this build
//! uses at that height (dev tool): cargo run -p zafe-core --example branch_check
use zafe_core::wallet::{connect, ZafeNetwork};
use zcash_client_backend::proto::service;
use zcash_protocol::consensus::{BlockHeight, BranchId};

#[tokio::main]
async fn main() {
    for (name, url, net) in [
        ("testnet", "https://testnet.zec.rocks:443", ZafeNetwork::Test),
        ("mainnet", "https://zec.rocks:443", ZafeNetwork::Main),
    ] {
        let mut c = match connect(url).await {
            Ok(c) => c,
            Err(e) => {
                println!("{name}: connect failed: {e}");
                continue;
            }
        };
        let info = c.get_lightd_info(service::Empty {}).await.unwrap().into_inner();
        let h = info.block_height as u32;
        let ours = BranchId::for_height(&net, BlockHeight::from_u32(h + 1));
        println!(
            "{name}: tip {h}  server branch {}  ours at tip+1 {:08x} ({:?})  chain {} {}",
            info.consensus_branch_id,
            u32::from(ours),
            ours,
            info.chain_name,
            info.vendor
        );
    }
}
