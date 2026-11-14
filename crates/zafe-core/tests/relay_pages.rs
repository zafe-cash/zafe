//! Security review 2026-11, batch 4 (rule 7): a relay page stops at a byte budget
//! (`zafe_proto::relay::READ_BUDGET_BYTES`), so no member can make the log too big to read
//! in one response. A page the budget cut is shorter than `READ_PAGE`; the client must not
//! read it as the end of the log (`zafe_proto::relay::page_may_continue`).

use rand::{rngs::StdRng, SeedableRng};
use zafe_core::node::load_state;
use zafe_proto::LogEntry;

mod common;
use common::relay_vault::vault;

#[tokio::test]
async fn a_member_reads_a_padded_log_whole_in_one_load() {
    let (relay, ids, ms, _) = vault(2, 3, false).await;
    let (chain, _) = load_state(&relay, &ids[1], &ms[1]).await.unwrap();
    let mut head = chain.entries().last().unwrap().hash().unwrap();
    let mut index = chain.len();
    // A dishonest member pads the log: 24 entries of ~0.9 MB, more than one page's budget.
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
    // Member 2 has no copy of the log: one load reads it all.
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
