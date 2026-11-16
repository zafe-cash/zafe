use rand::{rngs::StdRng, SeedableRng};
use zafe_core::node::load_state;
use zafe_proto::LogEntry;

pub mod common;
use common::relay_vault::vault;

#[tokio::test]
async fn a_member_reads_a_padded_log_whole_in_one_load() {
    let (relay, ids, ms, _) = vault(2, 3, false).await;
    let (chain, _) = load_state(&relay, &ids[1], &ms[1]).await.unwrap();
    let mut head = chain.entries().last().unwrap().hash().unwrap();
    let mut index = chain.len();
    let key = ms[1].log_key();
    let mut rng = StdRng::seed_from_u64(3);
    let junk = vec![0u8; 900_000];
    for _ in 0..24 {
        let e = LogEntry::create(
            &ids[1],
            &key,
            ms[1].descriptor.vault_id,
            index,
            head,
            &junk,
            &mut rng,
        )
        .unwrap();
        relay
            .append_log_to(&ids[1], ms[1].label(), &e)
            .await
            .unwrap();
        head = e.hash().unwrap();
        index += 1;
    }
    let (chain, state) = load_state(&relay, &ids[2], &ms[2]).await.unwrap();
    assert_eq!(
        chain.len(),
        index,
        "the load stopped at a page the budget cut"
    );
    assert_eq!(
        state.ignored.len(),
        24,
        "the padding is ignored, not trusted"
    );
}
