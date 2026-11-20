use orchard::{
    keys::{FullViewingKey, Scope},
    note::Note,
    tree::MerklePath,
    Address, Anchor,
};
use pczt::Pczt;
use rand::{rngs::StdRng, SeedableRng};
use zafe_core::verify::{verify_pczt, Expectations, Payment, VerifyError};
use zcash_primitives::transaction::builder::BundlePadding;
use zcash_protocol::memo::{Memo, MemoBytes};

pub mod common;
use common::*;

const FUNDS: u64 = 1_000_000;
const FEE_2_ACTIONS: u64 = 10_000;
const FEE_3_ACTIONS: u64 = 15_000;

struct Vault {
    fvk: FullViewingKey,
}

impl Vault {
    fn new(seed: u64) -> Self {
        let mut rng = StdRng::seed_from_u64(seed);
        let members = run_keygen(params(2, 3), &mut rng);
        Self {
            fvk: members[0].1.vault_keys.fvk().clone(),
        }
    }

    fn change_address(&self) -> Address {
        self.fvk.address_at(0u32, Scope::Internal)
    }

    fn pczt(&self, outputs: Vec<Out>) -> Pczt {
        let note =
            receive_ironwood_note(&self.fvk, self.fvk.address_at(0u32, Scope::External), FUNDS);
        let (anchor, path) = witness(&note);
        build_pczt(&self.fvk, note, path, anchor, outputs)
    }

    fn pay(&self, recipient: Address, value: u64, memo: &MemoBytes) -> Out {
        Out {
            ovk: Some(self.fvk.to_ovk(Scope::External)),
            recipient,
            value,
            memo: memo.clone(),
        }
    }

    fn change(&self, value: u64) -> Out {
        Out {
            ovk: Some(self.fvk.to_ovk(Scope::Internal)),
            recipient: self.change_address(),
            value,
            memo: MemoBytes::empty(),
        }
    }
}

fn memo(text: &str) -> MemoBytes {
    Memo::from_bytes(text.as_bytes()).unwrap().encode()
}

fn payment(recipient: Address, amount_zat: u64, memo: &MemoBytes) -> Payment {
    Payment {
        recipient,
        amount_zat,
        memo: *memo.as_array(),
    }
}

fn expect(payments: Vec<Payment>) -> Expectations {
    Expectations {
        payments,
        consensus_branch_id: branch_id(),
        tip_height: TARGET_HEIGHT - 1,
        max_expiry_delta: 100,
    }
}

#[test]
fn honest_payment_with_change_verifies() {
    let v = Vault::new(30);
    let payee = outside_address(7);
    let m = memo("Invoice #123");
    let pczt = v.pczt(vec![
        v.pay(payee, 400_000, &m),
        v.change(FUNDS - 400_000 - FEE_2_ACTIONS),
    ]);

    let verified = verify_pczt(&pczt, &v.fvk, &expect(vec![payment(payee, 400_000, &m)])).unwrap();
    assert_eq!(verified.input_total_zat, FUNDS);
    assert_eq!(verified.change_total_zat, FUNDS - 400_000 - FEE_2_ACTIONS);
    assert_eq!(verified.fee_zat, FEE_2_ACTIONS);
    assert_eq!(verified.spends_to_sign.len(), 1);
    assert_eq!(
        verified.sighash,
        zafe_core::tx::shielded_sighash(&pczt).unwrap()
    );
}

#[test]
fn batch_payment_verifies() {
    let v = Vault::new(31);
    let (a, b) = (outside_address(7), outside_address(8));
    let (ma, mb) = (memo("grant A"), memo("grant B"));
    let pczt = v.pczt(vec![
        v.pay(a, 300_000, &ma),
        v.pay(b, 200_000, &mb),
        v.change(FUNDS - 500_000 - FEE_3_ACTIONS),
    ]);
    let ok = verify_pczt(
        &pczt,
        &v.fvk,
        &expect(vec![payment(b, 200_000, &mb), payment(a, 300_000, &ma)]),
    );
    assert!(ok.is_ok(), "{ok:?}");
}

#[test]
fn rejects_hidden_output_to_attacker() {
    let v = Vault::new(32);
    let payee = outside_address(7);
    let m = memo("Invoice #123");
    let pczt = v.pczt(vec![
        v.pay(payee, 400_000, &m),
        v.pay(
            outside_address(66),
            FUNDS - 400_000 - FEE_2_ACTIONS,
            &MemoBytes::empty(),
        ),
    ]);
    let err = verify_pczt(&pczt, &v.fvk, &expect(vec![payment(payee, 400_000, &m)])).unwrap_err();
    assert!(matches!(err, VerifyError::UnexpectedOutput(_)), "{err:?}");
}

#[test]
fn rejects_wrong_amount_recipient_or_memo() {
    let v = Vault::new(33);
    let payee = outside_address(7);
    let m = memo("Invoice #123");
    let pczt = v.pczt(vec![
        v.pay(payee, 400_000, &m),
        v.change(FUNDS - 400_000 - FEE_2_ACTIONS),
    ]);

    for claimed in [
        payment(payee, 300_000, &m),
        payment(outside_address(9), 400_000, &m),
        payment(payee, 400_000, &memo("Invoice #999")),
    ] {
        let err = verify_pczt(&pczt, &v.fvk, &expect(vec![claimed])).unwrap_err();
        assert!(matches!(err, VerifyError::UnexpectedOutput(_)), "{err:?}");
    }
}

#[test]
fn rejects_missing_payment() {
    let v = Vault::new(34);
    let payee = outside_address(7);
    let m = memo("x");
    let pczt = v.pczt(vec![
        v.pay(payee, 400_000, &m),
        v.change(FUNDS - 400_000 - FEE_2_ACTIONS),
    ]);
    let err = verify_pczt(
        &pczt,
        &v.fvk,
        &expect(vec![
            payment(payee, 400_000, &m),
            payment(outside_address(8), 1, &m),
        ]),
    )
    .unwrap_err();
    assert_eq!(err, VerifyError::MissingPayments(1));
}

#[test]
fn rejects_excess_fee() {
    let v = Vault::new(35);
    let payee = outside_address(7);
    let m = memo("x");
    let note = receive_ironwood_note(&v.fvk, v.fvk.address_at(0u32, Scope::External), FUNDS);
    let (anchor, path) = witness(&note);
    let outputs = vec![
        v.pay(payee, 400_000, &m),
        v.change(FUNDS - 400_000 - 50_000),
    ];
    let pczt = build_pczt_with_fee(&v.fvk, note, path, anchor, outputs, Some(50_000));
    let err = verify_pczt(&pczt, &v.fvk, &expect(vec![payment(payee, 400_000, &m)])).unwrap_err();
    assert_eq!(
        err,
        VerifyError::WrongFee {
            expected: FEE_2_ACTIONS,
            actual: 50_000
        }
    );
}

#[test]
fn rejects_unrecoverable_payment_output() {
    let v = Vault::new(36);
    let payee = outside_address(7);
    let m = memo("x");
    let hidden = Out {
        ovk: None,
        recipient: payee,
        value: 400_000,
        memo: m.clone(),
    };
    let pczt = v.pczt(vec![hidden, v.change(FUNDS - 400_000 - FEE_2_ACTIONS)]);
    let err = verify_pczt(&pczt, &v.fvk, &expect(vec![payment(payee, 400_000, &m)])).unwrap_err();
    assert!(
        matches!(err, VerifyError::UnrecoverableOutput(_)),
        "{err:?}"
    );
}

#[test]
fn rejects_wrong_network_or_expiry() {
    let v = Vault::new(37);
    let payee = outside_address(7);
    let m = memo("x");
    let pczt = v.pczt(vec![
        v.pay(payee, 400_000, &m),
        v.change(FUNDS - 400_000 - FEE_2_ACTIONS),
    ]);
    let payments = vec![payment(payee, 400_000, &m)];

    let mut wrong_branch = expect(payments.clone());
    wrong_branch.consensus_branch_id ^= 1;
    assert!(matches!(
        verify_pczt(&pczt, &v.fvk, &wrong_branch).unwrap_err(),
        VerifyError::WrongBranchId { .. }
    ));

    let mut stale = expect(payments);
    stale.tip_height = TARGET_HEIGHT + 1_000;
    assert!(matches!(
        verify_pczt(&pczt, &v.fvk, &stale).unwrap_err(),
        VerifyError::BadExpiry { .. }
    ));
}

#[test]
fn rejects_spend_from_another_wallet() {
    let v = Vault::new(38);
    let other = FullViewingKey::from(&orchard::keys::SpendingKey::from_bytes([5; 32]).unwrap());
    let note = receive_ironwood_note(&other, other.address_at(0u32, Scope::External), FUNDS);
    let (anchor, path) = witness(&note);
    let payee = outside_address(7);
    let m = memo("x");
    let out = Out {
        ovk: Some(v.fvk.to_ovk(Scope::External)),
        recipient: payee,
        value: 400_000,
        memo: m.clone(),
    };
    let other_change = Out {
        ovk: None,
        recipient: other.address_at(0u32, Scope::Internal),
        value: FUNDS - 400_000 - FEE_2_ACTIONS,
        memo: MemoBytes::empty(),
    };
    let pczt = build_pczt(&other, note, path, anchor, vec![out, other_change]);
    let err = verify_pczt(&pczt, &v.fvk, &expect(vec![payment(payee, 400_000, &m)])).unwrap_err();
    assert!(matches!(err, VerifyError::ForeignSpend(_)), "{err:?}");
}

fn vault_notes(v: &Vault, count: usize, value: u64) -> (Anchor, Vec<(Note, MerklePath)>) {
    let notes: Vec<Note> = (0..count)
        .map(|_| receive_ironwood_note(&v.fvk, v.fvk.address_at(0u32, Scope::External), value))
        .collect();
    let (anchor, paths) = witness_all(&notes);
    (anchor, notes.into_iter().zip(paths).collect())
}

fn padded_to(actions: u8) -> BundlePadding {
    BundlePadding {
        bundle_required: false,
        pad_to_minimum: Some(actions),
    }
}

fn zero_change(v: &Vault) -> Out {
    v.change(0)
}

#[test]
fn rejects_padding_that_raises_the_fee() {
    let v = Vault::new(40);
    let payee = outside_address(7);
    let m = memo("Invoice #123");
    let (anchor, spends) = vault_notes(&v, 1, FUNDS);
    let fee = 5 * 5_000;
    let pczt = build_pczt_custom(
        &v.fvk,
        spends,
        anchor,
        vec![v.pay(payee, 400_000, &m), v.change(FUNDS - 400_000 - fee)],
        None,
        padded_to(5),
    );
    let err = verify_pczt(&pczt, &v.fvk, &expect(vec![payment(payee, 400_000, &m)])).unwrap_err();
    assert_eq!(
        err,
        VerifyError::TooManyActions {
            actions: 5,
            needed: 2
        }
    );
}

#[test]
fn rejects_zero_value_vault_outputs_as_padding() {
    let v = Vault::new(41);
    let payee = outside_address(7);
    let m = memo("x");
    let (anchor, spends) = vault_notes(&v, 1, FUNDS);
    let fee = 4 * 5_000;
    let pczt = build_pczt_custom(
        &v.fvk,
        spends,
        anchor,
        vec![
            v.pay(payee, 400_000, &m),
            v.change(FUNDS - 400_000 - fee),
            zero_change(&v),
            zero_change(&v),
        ],
        None,
        BundlePadding::DEFAULT,
    );
    let err = verify_pczt(&pczt, &v.fvk, &expect(vec![payment(payee, 400_000, &m)])).unwrap_err();
    assert_eq!(
        err,
        VerifyError::TooManyActions {
            actions: 4,
            needed: 2
        }
    );
}

#[test]
fn rejects_change_split_beyond_the_policy() {
    let v = Vault::new(42);
    let payee = outside_address(7);
    let m = memo("x");
    let (anchor, spends) = vault_notes(&v, 1, FUNDS);
    let fee = 7 * 5_000;
    let each = (FUNDS - 400_000 - fee) / 6;
    let mut outputs = vec![v.pay(payee, 400_000, &m)];
    outputs.extend((0..6).map(|i| {
        v.change(if i == 0 {
            FUNDS - 400_000 - fee - 5 * each
        } else {
            each
        })
    }));
    let pczt = build_pczt_custom(
        &v.fvk,
        spends,
        anchor,
        outputs,
        None,
        BundlePadding::DEFAULT,
    );
    let err = verify_pczt(&pczt, &v.fvk, &expect(vec![payment(payee, 400_000, &m)])).unwrap_err();
    assert_eq!(
        err,
        VerifyError::TooManyActions {
            actions: 7,
            needed: 5
        }
    );
}

#[test]
fn change_split_into_four_notes_verifies() {
    let v = Vault::new(43);
    let payee = outside_address(7);
    let m = memo("x");
    let funds = 5_000_003;
    let (anchor, spends) = vault_notes(&v, 1, funds);
    let fee = 5 * 5_000;
    let change = funds - 400_000 - fee;
    assert!(change / 4 >= zafe_core::wallet::CHANGE_NOTE_MIN_ZAT);
    let mut outputs = vec![v.pay(payee, 400_000, &m)];
    outputs.extend((0..4).map(|i| {
        v.change(if i == 0 {
            change / 4 + change % 4
        } else {
            change / 4
        })
    }));
    let pczt = build_pczt_custom(
        &v.fvk,
        spends,
        anchor,
        outputs,
        None,
        BundlePadding::DEFAULT,
    );
    let verified = verify_pczt(&pczt, &v.fvk, &expect(vec![payment(payee, 400_000, &m)])).unwrap();
    assert_eq!(verified.fee_zat, fee);
}

#[test]
fn more_spends_than_outputs_verifies() {
    let v = Vault::new(44);
    let payee = outside_address(7);
    let m = memo("x");
    let (anchor, spends) = vault_notes(&v, 5, 200_000);
    let fee = 5 * 5_000;
    let pczt = build_pczt_custom(
        &v.fvk,
        spends,
        anchor,
        vec![
            v.pay(payee, 700_000, &m),
            v.change(1_000_000 - 700_000 - fee),
        ],
        None,
        BundlePadding::DEFAULT,
    );
    let verified = verify_pczt(&pczt, &v.fvk, &expect(vec![payment(payee, 700_000, &m)])).unwrap();
    assert_eq!(verified.fee_zat, fee);
    assert_eq!(verified.spends_to_sign.len(), 5);
}

#[test]
fn sweep_without_payments_verifies() {
    let v = Vault::new(45);
    let (anchor, spends) = vault_notes(&v, 3, 200_000);
    let fee = 3 * 5_000;
    let pczt = build_pczt_custom(
        &v.fvk,
        spends,
        anchor,
        vec![v.change(600_000 - fee)],
        None,
        BundlePadding::DEFAULT,
    );
    let verified = verify_pczt(&pczt, &v.fvk, &expect(vec![])).unwrap();
    assert_eq!(verified.fee_zat, fee);
    assert_eq!(verified.payments.len(), 0);
}

#[test]
fn exact_spend_with_zero_change_verifies() {
    let v = Vault::new(46);
    let (a, b) = (outside_address(7), outside_address(8));
    let (ma, mb) = (memo("a"), memo("b"));
    let (anchor, spends) = vault_notes(&v, 1, FUNDS);
    let pczt = build_pczt_custom(
        &v.fvk,
        spends,
        anchor,
        vec![
            v.pay(a, 600_000, &ma),
            v.pay(b, FUNDS - 600_000 - FEE_3_ACTIONS, &mb),
            zero_change(&v),
        ],
        None,
        BundlePadding::DEFAULT,
    );
    let ok = verify_pczt(
        &pczt,
        &v.fvk,
        &expect(vec![
            payment(a, 600_000, &ma),
            payment(b, FUNDS - 600_000 - FEE_3_ACTIONS, &mb),
        ]),
    );
    assert!(ok.is_ok(), "{ok:?}");
}

#[test]
fn unpadded_single_action_verifies() {
    let v = Vault::new(47);
    let (anchor, spends) = vault_notes(&v, 1, FUNDS);
    let pczt = build_pczt_custom(
        &v.fvk,
        spends,
        anchor,
        vec![v.change(FUNDS - FEE_2_ACTIONS)],
        None,
        padded_to(1),
    );
    let verified = verify_pczt(&pczt, &v.fvk, &expect(vec![])).unwrap();
    assert_eq!(verified.fee_zat, FEE_2_ACTIONS);
}

#[test]
fn rejects_zero_value_vault_spends_as_padding() {
    let v = Vault::new(48);
    let payee = outside_address(7);
    let m = memo("x");
    let ext = v.fvk.address_at(0u32, Scope::External);
    let mut notes = vec![receive_ironwood_note(&v.fvk, ext, FUNDS)];
    notes.extend((0..4).map(|_| receive_ironwood_note(&v.fvk, ext, 0)));
    let (anchor, paths) = witness_all(&notes);
    let fee = 5 * 5_000;
    let pczt = build_pczt_custom(
        &v.fvk,
        notes.into_iter().zip(paths).collect(),
        anchor,
        vec![v.pay(payee, 400_000, &m), v.change(FUNDS - 400_000 - fee)],
        None,
        BundlePadding::DEFAULT,
    );
    let err = verify_pczt(&pczt, &v.fvk, &expect(vec![payment(payee, 400_000, &m)])).unwrap_err();
    assert_eq!(
        err,
        VerifyError::TooManyActions {
            actions: 5,
            needed: 2
        }
    );
}

#[test]
fn rejects_dust_vault_spends_as_padding() {
    let v = Vault::new(49);
    let payee = outside_address(7);
    let m = memo("x");
    let ext = v.fvk.address_at(0u32, Scope::External);
    let mut notes = vec![receive_ironwood_note(&v.fvk, ext, FUNDS)];
    notes.extend((0..4).map(|_| receive_ironwood_note(&v.fvk, ext, 5_000)));
    let (anchor, paths) = witness_all(&notes);
    let fee = 5 * 5_000;
    let pczt = build_pczt_custom(
        &v.fvk,
        notes.into_iter().zip(paths).collect(),
        anchor,
        vec![
            v.pay(payee, 400_000, &m),
            v.change(FUNDS + 4 * 5_000 - 400_000 - fee),
        ],
        None,
        BundlePadding::DEFAULT,
    );
    let err = verify_pczt(&pczt, &v.fvk, &expect(vec![payment(payee, 400_000, &m)])).unwrap_err();
    assert_eq!(
        err,
        VerifyError::TooManyActions {
            actions: 5,
            needed: 2
        }
    );
}

#[test]
fn small_spends_in_free_actions_verify() {
    let v = Vault::new(50);
    let payee = outside_address(7);
    let m = memo("x");
    let ext = v.fvk.address_at(0u32, Scope::External);
    for small in [0, 5_000] {
        let notes = vec![
            receive_ironwood_note(&v.fvk, ext, FUNDS),
            receive_ironwood_note(&v.fvk, ext, small),
        ];
        let (anchor, paths) = witness_all(&notes);
        let pczt = build_pczt_custom(
            &v.fvk,
            notes.into_iter().zip(paths).collect(),
            anchor,
            vec![
                v.pay(payee, 400_000, &m),
                v.change(FUNDS + small - 400_000 - FEE_2_ACTIONS),
            ],
            None,
            BundlePadding::DEFAULT,
        );
        let verified =
            verify_pczt(&pczt, &v.fvk, &expect(vec![payment(payee, 400_000, &m)])).unwrap();
        assert_eq!(verified.fee_zat, FEE_2_ACTIONS);
        assert_eq!(verified.spends_to_sign.len(), 2);
    }
}

#[test]
fn spends_above_the_marginal_fee_count() {
    let v = Vault::new(51);
    let payee = outside_address(7);
    let m = memo("x");
    let ext = v.fvk.address_at(0u32, Scope::External);
    let mut notes = vec![receive_ironwood_note(&v.fvk, ext, FUNDS)];
    notes.extend((0..4).map(|_| receive_ironwood_note(&v.fvk, ext, 5_001)));
    let (anchor, paths) = witness_all(&notes);
    let fee = 5 * 5_000;
    let pczt = build_pczt_custom(
        &v.fvk,
        notes.into_iter().zip(paths).collect(),
        anchor,
        vec![
            v.pay(payee, 400_000, &m),
            v.change(FUNDS + 4 * 5_001 - 400_000 - fee),
        ],
        None,
        BundlePadding::DEFAULT,
    );
    let verified = verify_pczt(&pczt, &v.fvk, &expect(vec![payment(payee, 400_000, &m)])).unwrap();
    assert_eq!(verified.fee_zat, fee);
    assert_eq!(verified.spends_to_sign.len(), 5);
}

#[test]
fn rejects_two_zero_value_change_outputs() {
    let v = Vault::new(52);
    let payee = outside_address(7);
    let m = memo("x");
    let (anchor, spends) = vault_notes(&v, 1, FUNDS);
    let pczt = build_pczt_custom(
        &v.fvk,
        spends,
        anchor,
        vec![
            v.pay(payee, FUNDS - FEE_3_ACTIONS, &m),
            zero_change(&v),
            zero_change(&v),
        ],
        None,
        BundlePadding::DEFAULT,
    );
    let err = verify_pczt(
        &pczt,
        &v.fvk,
        &expect(vec![payment(payee, FUNDS - FEE_3_ACTIONS, &m)]),
    )
    .unwrap_err();
    assert_eq!(
        err,
        VerifyError::TooManyActions {
            actions: 3,
            needed: 2
        }
    );
}

#[test]
fn rejects_one_action_over_the_bound() {
    let v = Vault::new(53);
    let payee = outside_address(7);
    let m = memo("x");
    let (anchor, spends) = vault_notes(&v, 1, FUNDS);
    let pczt = build_pczt_custom(
        &v.fvk,
        spends,
        anchor,
        vec![
            v.pay(payee, 400_000, &m),
            v.change(FUNDS - 400_000 - FEE_3_ACTIONS),
        ],
        None,
        padded_to(3),
    );
    let err = verify_pczt(&pczt, &v.fvk, &expect(vec![payment(payee, 400_000, &m)])).unwrap_err();
    assert_eq!(
        err,
        VerifyError::TooManyActions {
            actions: 3,
            needed: 2
        }
    );
}

#[test]
fn change_is_exactly_the_wallets() {
    let v = Vault::new(31);
    let payee = outside_address(7);
    let m = memo("Invoice");
    let check = |change: Out| {
        let pczt = v.pczt(vec![v.pay(payee, 400_000, &m), change]);
        verify_pczt(&pczt, &v.fvk, &expect(vec![payment(payee, 400_000, &m)]))
    };
    check(v.change(590_000)).unwrap();
    let unexpected = |r: Result<_, VerifyError>| {
        assert!(matches!(r, Err(VerifyError::UnexpectedChange(_))), "{r:?}")
    };
    unexpected(check(Out {
        ovk: None,
        ..v.change(590_000)
    }));
    unexpected(check(Out {
        ovk: Some(v.fvk.to_ovk(Scope::External)),
        ..v.change(590_000)
    }));
    unexpected(check(Out {
        memo: memo("hidden"),
        ..v.change(590_000)
    }));
    unexpected(check(Out {
        recipient: v.fvk.address_at(0u32, Scope::External),
        ..v.change(590_000)
    }));
    unexpected(check(Out {
        recipient: v.fvk.address_at(1u32, Scope::Internal),
        ..v.change(590_000)
    }));
}

#[test]
fn a_forced_extra_spend_adds_an_action_and_its_fee() {
    let v = Vault::new(45);
    let payee = outside_address(7);
    let m = memo("x");
    let exp = || expect(vec![payment(payee, 400_000, &m)]);
    let build = |notes: usize, fee: u64| {
        let (anchor, spends) = vault_notes(&v, notes, 300_000);
        build_pczt_custom(
            &v.fvk,
            spends,
            anchor,
            vec![
                v.pay(payee, 400_000, &m),
                v.change(notes as u64 * 300_000 - 400_000 - fee),
            ],
            Some(fee),
            BundlePadding::DEFAULT,
        )
    };
    let base = build(2, FEE_2_ACTIONS);
    assert_eq!(base.ironwood().actions().len(), 2);
    verify_pczt(&base, &v.fvk, &exp()).unwrap();
    let forced = build(3, FEE_3_ACTIONS);
    assert_eq!(forced.ironwood().actions().len(), 3);
    assert_eq!(
        verify_pczt(&forced, &v.fvk, &exp()).unwrap().fee_zat,
        FEE_3_ACTIONS
    );
    assert!(matches!(
        verify_pczt(&build(3, FEE_2_ACTIONS), &v.fvk, &exp()),
        Err(VerifyError::WrongFee { .. })
    ));
}

fn two_spend_pczt(v: &Vault) -> (Pczt, Expectations) {
    let payee = outside_address(7);
    let m = memo("x");
    let (anchor, spends) = vault_notes(v, 2, 500_000);
    let pczt = build_pczt_custom(
        &v.fvk,
        spends,
        anchor,
        vec![
            v.pay(payee, 700_000, &m),
            v.change(1_000_000 - 700_000 - FEE_2_ACTIONS),
        ],
        None,
        BundlePadding::DEFAULT,
    );
    (pczt, expect(vec![payment(payee, 700_000, &m)]))
}

fn replace_once(bytes: &mut [u8], from: &[u8; 32], to: &[u8; 32]) {
    let at: Vec<usize> = bytes
        .windows(32)
        .enumerate()
        .filter(|(_, w)| *w == from)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(at.len(), 1, "field occurs exactly once");
    bytes[at[0]..at[0] + 32].copy_from_slice(to);
}

fn alpha_bytes(alpha: &pasta_curves::pallas::Scalar) -> [u8; 32] {
    ff::PrimeField::to_repr(alpha)
}

#[test]
fn rejects_a_zero_alpha_whose_rk_is_the_vault_key() {
    let v = Vault::new(61);
    let (pczt, expected) = two_spend_pczt(&v);
    let spends = verify_pczt(&pczt, &v.fvk, &expected)
        .unwrap()
        .spends_to_sign;
    let ak: [u8; 32] = v.fvk.to_bytes()[..32].try_into().unwrap();

    let mut bytes = pczt.serialize().unwrap();
    replace_once(&mut bytes, &alpha_bytes(&spends[0].alpha), &[0; 32]);
    replace_once(&mut bytes, &spends[0].rk, &ak);
    let tampered = Pczt::parse(&bytes).unwrap();

    let err = verify_pczt(&tampered, &v.fvk, &expected).unwrap_err();
    assert!(
        matches!(err, VerifyError::DegenerateAlpha(i) if i == spends[0].action_index),
        "{err:?}"
    );
}

#[test]
fn rejects_two_spends_sharing_one_alpha() {
    let v = Vault::new(62);
    let (pczt, expected) = two_spend_pczt(&v);
    let spends = verify_pczt(&pczt, &v.fvk, &expected)
        .unwrap()
        .spends_to_sign;
    assert_eq!(spends.len(), 2);

    let mut bytes = pczt.serialize().unwrap();
    replace_once(
        &mut bytes,
        &alpha_bytes(&spends[1].alpha),
        &alpha_bytes(&spends[0].alpha),
    );
    replace_once(&mut bytes, &spends[1].rk, &spends[0].rk);
    let tampered = Pczt::parse(&bytes).unwrap();

    let err = verify_pczt(&tampered, &v.fvk, &expected).unwrap_err();
    assert!(
        matches!(err, VerifyError::DegenerateAlpha(i) if i == spends[1].action_index),
        "{err:?}"
    );
}

#[test]
fn rejects_a_pczt_that_still_carries_a_dummy_spending_key() {
    let v = Vault::new(63);
    let payee = outside_address(7);
    let m = memo("x");
    let (anchor, spends) = vault_notes(&v, 1, FUNDS);
    let unfinalized = build_pczt_unfinalized(
        &v.fvk,
        spends,
        anchor,
        vec![
            v.pay(payee, 400_000, &m),
            v.change(FUNDS - 400_000 - FEE_2_ACTIONS),
        ],
        None,
        BundlePadding::DEFAULT,
    );
    assert!(unfinalized
        .ironwood()
        .actions()
        .iter()
        .any(|a| a.spend().dummy_sk().is_some()));

    assert!(matches!(
        zafe_core::tx::spends_to_sign(&unfinalized, &v.fvk),
        Err(zafe_core::tx::TxError::DummyKey(_))
    ));

    let finalized = pczt::roles::io_finalizer::IoFinalizer::new(unfinalized.clone())
        .finalize_io(zafe_core::zrng::zrng())
        .unwrap();
    let signed_with_key =
        pczt::roles::combiner::Combiner::new(vec![finalized, unfinalized.clone()])
            .combine()
            .unwrap();
    let dummy = signed_with_key
        .ironwood()
        .actions()
        .iter()
        .position(|a| a.spend().dummy_sk().is_some())
        .unwrap();
    assert!(signed_with_key.ironwood().actions()[dummy]
        .spend()
        .spend_auth_sig()
        .is_some());
    let err = verify_pczt(
        &signed_with_key,
        &v.fvk,
        &expect(vec![payment(payee, 400_000, &m)]),
    )
    .unwrap_err();
    assert!(
        matches!(err, VerifyError::DummyKey(i) if i == dummy),
        "{err:?}"
    );
    assert!(verify_pczt(
        &unfinalized,
        &v.fvk,
        &expect(vec![payment(payee, 400_000, &m)])
    )
    .is_err());
}

fn with_user_address(pczt: Pczt, output_action: usize, user_address: String) -> Pczt {
    pczt::roles::updater::Updater::new(pczt)
        .update_ironwood_with(|mut updater| {
            updater.update_action_with(output_action, |mut action| {
                action.set_output_user_address(user_address);
                Ok(())
            })
        })
        .unwrap()
        .finish()
}

fn unified_with_orchard(receiver: &Address) -> String {
    use zcash_address::unified::{self, Encoding};
    let ua = unified::Address::try_from_items(
        unified::Revision::R0,
        vec![unified::Uitem::Data(unified::Receiver::Orchard(
            receiver.to_raw_address_bytes(),
        ))],
    )
    .unwrap();
    ua.encode(&zcash_protocol::consensus::NetworkType::Test)
}

fn payment_action(pczt: &Pczt, payee: &Address) -> usize {
    pczt.ironwood()
        .actions()
        .iter()
        .position(|a| {
            a.output()
                .recipient()
                .map(|r| r.as_slice() == payee.to_raw_address_bytes().as_slice())
                .unwrap_or(false)
        })
        .expect("payment output")
}

#[test]
fn user_address_must_contain_the_output_recipient() {
    let v = Vault::new(64);
    let payee = outside_address(7);
    let m = memo("x");
    let pay = || {
        v.pczt(vec![
            v.pay(payee, 400_000, &m),
            v.change(FUNDS - 400_000 - FEE_2_ACTIONS),
        ])
    };
    let expected = expect(vec![payment(payee, 400_000, &m)]);

    let honest = pay();
    let index = payment_action(&honest, &payee);
    let honest = with_user_address(honest, index, unified_with_orchard(&payee));
    verify_pczt(&honest, &v.fvk, &expected).expect("the payee's own address verifies");

    let lying = pay();
    let index = payment_action(&lying, &payee);
    let lying = with_user_address(lying, index, unified_with_orchard(&outside_address(8)));
    let err = verify_pczt(&lying, &v.fvk, &expected).unwrap_err();
    assert!(
        matches!(err, VerifyError::UserAddressMismatch(i) if i == index),
        "{err:?}"
    );

    let garbage = pay();
    let index = payment_action(&garbage, &payee);
    let garbage = with_user_address(garbage, index, "not an address".into());
    let err = verify_pczt(&garbage, &v.fvk, &expected).unwrap_err();
    assert!(
        matches!(err, VerifyError::UserAddressMismatch(_)),
        "{err:?}"
    );
}
