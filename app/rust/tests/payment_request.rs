//! Scanned payment targets: plain addresses and ZIP 321 URIs.

use orchard::keys::{FullViewingKey, Scope, SpendingKey};
use rust_lib_zafe::api::proposals::parse_payment_request;
use zcash_keys::address::UnifiedAddress;

fn regtest_address(seed: u8) -> String {
    let fvk = FullViewingKey::from(&SpendingKey::from_bytes([seed; 32]).unwrap());
    UnifiedAddress::from_receivers(
        Some(fvk.address_at(0u32, Scope::External)),
        None,
        None,
        None,
        None,
    )
    .unwrap()
    .encode(&zafe_core::wallet::regtest_network())
}

#[test]
fn plain_addresses_and_payment_links() {
    let a = regtest_address(1);
    let b = regtest_address(2);

    let plain = parse_payment_request("regtest".into(), format!("  {a}\n"));
    assert!(plain.problem.is_empty());
    assert_eq!(plain.payments.len(), 1);
    assert_eq!(plain.payments[0].address, a);
    assert_eq!(plain.payments[0].amount_zat, 0);

    // ZIP 321: amount and a base64url memo ("Grant #7").
    let uri = format!("zcash:{a}?amount=1.25&memo=R3JhbnQgIzc");
    let one = parse_payment_request("regtest".into(), uri);
    assert!(one.problem.is_empty(), "{}", one.problem);
    assert_eq!(one.payments[0].amount_zat, 125_000_000);
    assert_eq!(one.payments[0].memo, "Grant #7");
    assert!(one.payments[0].label.is_empty() && one.payments[0].message.is_empty());

    // The link's own label and message come through (percent-decoded) for display.
    let uri = format!("zcash:{a}?amount=2&label=Coffee%20shop&message=Order%20%2342");
    let labelled = parse_payment_request("regtest".into(), uri);
    assert!(labelled.problem.is_empty(), "{}", labelled.problem);
    assert_eq!(labelled.payments[0].label, "Coffee shop");
    assert_eq!(labelled.payments[0].message, "Order #42");

    // Several recipients become a batch.
    let uri = format!("zcash:?address={a}&amount=1&address.1={b}&amount.1=0.5");
    let batch = parse_payment_request("regtest".into(), uri);
    assert!(batch.problem.is_empty(), "{}", batch.problem);
    let got: Vec<(&str, u64)> = batch
        .payments
        .iter()
        .map(|p| (p.address.as_str(), p.amount_zat))
        .collect();
    assert_eq!(
        got,
        vec![(a.as_str(), 100_000_000), (b.as_str(), 50_000_000)]
    );
}

#[test]
fn unpayable_targets_say_why() {
    let a = regtest_address(1);
    assert_eq!(
        parse_payment_request("test".into(), a).problem,
        "This address is for a different Zcash network"
    );
    assert_eq!(
        parse_payment_request("regtest".into(), "hello".into()).problem,
        "Invalid address"
    );
    assert_eq!(
        parse_payment_request("regtest".into(), "zcash:?amount=nope".into()).problem,
        "This payment link can't be read"
    );
}
