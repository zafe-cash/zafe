//! Member verification of proposal PCZTs (spec §9.3): the honest case, and each way a
//! malicious proposer, leader or relay could try to get a bad transaction signed.

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

mod common;
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

    /// Builds a PCZT spending a freshly received vault note to `outputs`.
    fn pczt(&self, outputs: Vec<Out>) -> Pczt {
        let note =
            receive_ironwood_note(&self.fvk, self.fvk.address_at(0u32, Scope::External), FUNDS);
        let (anchor, path) = witness(&note);
        build_pczt(&self.fvk, note, path, anchor, outputs)
    }

    /// An output recoverable by the vault (how Zafe builds payments).
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
    // Pays the invoice, but "change" goes to an attacker instead of the vault.
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
    // The builder shuffles action order, so match the error kind, not the index.
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
        payment(payee, 300_000, &m),                    // amount differs
        payment(outside_address(9), 400_000, &m),       // recipient differs
        payment(payee, 400_000, &memo("Invoice #999")), // memo differs
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
    // Under-reported change silently burns value as fee.
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
    // Built without the vault OVK: the memo and recipient cannot be independently checked.
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
    stale.tip_height = TARGET_HEIGHT + 1_000; // the transaction's expiry has passed
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

// --- Action count: padding must not raise the fee (security report 2026-11-12) ---------

/// `count` vault notes of `value` each, in one tree.
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

/// A transaction to the vault's own address of `value` with no memo (zero-value change).
fn zero_change(v: &Vault) -> Out {
    v.change(0)
}

#[test]
fn rejects_padding_that_raises_the_fee() {
    // One spend, a payment and change need 2 actions. The builder pads to 5 and charges the
    // ZIP 317 fee for all 5: the payment is exact, the extra 15,000 zat come out of change.
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
    // Padding made of zero-value outputs to the vault's own address: they decrypt like
    // change, but no builder adds them next to real change.
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
    // More change notes than the split policy ever makes (4): the extra ones only add fee.
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
    // `wallet::change_split_policy`: up to 4 change notes while the vault has few notes,
    // each at least `CHANGE_NOTE_MIN_ZAT`. Laid out as zcash_client_backend does: the
    // quotient in every note, the remainder added to the first.
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
    // Five notes for one payment and change: 5 actions, each carrying a real spend.
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
    // A cancelled proposal's notes swept back to the vault: no payments, every note spent.
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
    // Two payments that use the note up exactly: the wallet still adds one zero-value
    // change output, so 3 actions are needed.
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
    // A builder may skip padding (pad_to_minimum = 1): one note swept in one action still
    // pays the 2-action ZIP 317 minimum, and fewer actions never raise the fee.
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
    // Zero-value notes to a vault address verify as vault spends (they are, and members
    // sign them), but they move no value: a builder could add any number of them and take
    // 5,000 zat each out of change. The notes need not exist on chain (the action circuit
    // skips the Merkle check for v_old = 0).
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
    // Real vault notes worth no more than the marginal fee: no honest wallet spends one in
    // an action of its own (each costs more fee than it brings), so they don't count.
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
    // A payment and change need 2 actions; a zero-value and a 5,000-zat vault note fit in
    // them for free (zcash_client_backend admits dust inputs exactly there), and every
    // vault spend is still signed.
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
    // One zat above the marginal fee, a note counts: five spends need five actions.
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
    // No change left: one zero-value vault output is what the wallet adds; a second one
    // is padding.
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

/// Change is recovered with the vault's internal outgoing viewing key as exactly what the
/// wallet makes: the change address, its value, an empty memo. Anything else lets a
/// proposer put data in change members can't see (a random `out_ciphertext`, a memo) or
/// park change at another vault address. On 4b5ae0e each of these verified: change was
/// only trial-decrypted with the vault's incoming viewing key.
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
    // No outgoing key (random out_ciphertext), or the external one.
    unexpected(check(Out {
        ovk: None,
        ..v.change(590_000)
    }));
    unexpected(check(Out {
        ovk: Some(v.fvk.to_ovk(Scope::External)),
        ..v.change(590_000)
    }));
    // A memo in change.
    unexpected(check(Out {
        memo: memo("hidden"),
        ..v.change(590_000)
    }));
    // Another vault address.
    unexpected(check(Out {
        recipient: v.fvk.address_at(0u32, Scope::External),
        ..v.change(590_000)
    }));
    unexpected(check(Out {
        recipient: v.fvk.address_at(1u32, Scope::Internal),
        ..v.change(590_000)
    }));
}

/// A payment two notes pay (2 spends, 2 outputs: 2 actions) plus a note of a cancelled
/// payment that may still be sent, which it must spend (`node::check_vote`): 3 vault
/// spends, 3 actions. Members accept it at the 3-action fee and refuse the 2-action one, so
/// the wallet must pay for the forced spend (selection recomputes the fee over all inputs).
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
