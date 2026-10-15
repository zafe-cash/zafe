import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/theme/app_theme.dart';
import 'package:zafe/src/features/send/payment_link.dart';
import 'package:zafe/src/features/send/payment_request_screen.dart';
import 'package:zafe/src/rust/api/proposals.dart' as rust;

const _address =
    'uregtest1zkuzfv5m3yhv2j4fmvq5rjurkxenxyq8r7h4daun2zkznrjaa8ra8asgdm8wwgwjvlwwrxx7347r8w0ee6dqyw4rufw4wg9djwcr6frzkezmdw6dud3wsm99eany5r8wgsctlxquu009nzd6hsme2tcsk0v3sgjvxa70er7h27z5epr67p5q767s2z5gt88paru56mxpm6pwz0cu35m';

rust.ScannedPayment _payment({
  int zat = 125000000,
  String label = '',
  String message = '',
}) => rust.ScannedPayment(
  address: _address,
  amountZat: BigInt.from(zat),
  memo: '',
  label: label,
  message: message,
);

const _treasury = PayFromVault(
  id: 'aa11',
  name: 'Treasury',
  threshold: 2,
  members: 3,
);
const _grants = PayFromVault(
  id: 'bb22',
  name: 'Grants',
  threshold: 3,
  members: 5,
);

/// The view with its state held by the test.
class _Harness extends StatefulWidget {
  const _Harness({
    required this.vaults,
    required this.onContinue,
    this.payments,
    this.problem = '',
  });
  final List<PayFromVault> vaults;
  final List<rust.ScannedPayment>? payments;
  final String problem;
  final void Function(String vaultId) onContinue;

  @override
  State<_Harness> createState() => _HarnessState();
}

class _HarnessState extends State<_Harness> {
  late String? _selected = widget.vaults.firstOrNull?.id;
  bool _confirmed = false;

  @override
  Widget build(BuildContext context) => PaymentRequestView(
    payments: widget.payments ?? [_payment()],
    problem: widget.problem,
    vaults: widget.vaults,
    selectedId: _selected,
    onSelect: (id) => setState(() => _selected = id),
    confirmed: _confirmed,
    onConfirm: () => setState(() => _confirmed = !_confirmed),
    onContinue: () => widget.onContinue(_selected!),
    onDecline: () {},
  );
}

Future<void> _pump(WidgetTester tester, Widget child) async {
  // The test font (Ahem) is wider than the app's, so give it room.
  tester.view.physicalSize = const Size(900, 2400);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  await tester.pumpWidget(
    MaterialApp(
      home: AppTheme(data: AppThemeData.dark, child: child),
    ),
  );
}

Finder _text(String s) => find.textContaining(s, findRichText: true);

void main() {
  group('payment links', () {
    test('only zcash: links count, in any case', () {
      expect(isPaymentLink('zcash:u1abc?amount=1'), isTrue);
      expect(isPaymentLink('  ZCASH:u1abc'), isTrue);
      expect(isPaymentLink('zafe://join?invite=x'), isFalse);
      expect(isPaymentLink('https://example.com/zcash:'), isFalse);
    });

    test('a link that waited too long expires', () {
      final at = DateTime(2026, 10, 4, 12);
      final link = PendingPaymentLink('zcash:x', at);
      expect(paymentLinkExpired(link, at.add(kPaymentLinkTtl)), isFalse);
      expect(
        paymentLinkExpired(
          link,
          at.add(kPaymentLinkTtl + const Duration(seconds: 1)),
        ),
        isTrue,
      );
    });
  });

  testWidgets('Continue needs the tick, then pays from the chosen vault', (
    tester,
  ) async {
    String? paidFrom;
    await _pump(
      tester,
      _Harness(
        vaults: const [_treasury, _grants],
        onContinue: (id) => paidFrom = id,
      ),
    );
    expect(_text('Check who sent this'), findsOneWidget);

    await tester.tap(find.text('Continue'));
    await tester.pump();
    expect(paidFrom, isNull, reason: 'not before the owner confirms');

    // Only the chosen vault is on the page; the others are behind "Change".
    expect(find.text('Grants'), findsNothing);
    await tester.tap(find.text('Change'));
    await tester.pumpAndSettle();
    await tester.tap(find.bySemanticsLabel(RegExp('^Grants')));
    await tester.pumpAndSettle();
    expect(find.text('Treasury'), findsNothing);
    expect(_text('Grants'), findsWidgets);
    expect(_text('3 of 5'), findsWidgets);

    await tester.tap(find.bySemanticsLabel(RegExp('^I know who sent this')));
    await tester.pump();
    await tester.tap(find.text('Continue'));
    await tester.pump();
    expect(paidFrom, 'bb22');
  });

  testWidgets('the link\'s own name and message are marked unverified', (
    tester,
  ) async {
    await _pump(
      tester,
      _Harness(
        vaults: const [_treasury],
        payments: [_payment(label: 'Coffee shop', message: 'Order #42')],
        onContinue: (_) {},
      ),
    );
    expect(_text('Name in the link (not verified)'), findsOneWidget);
    expect(_text('"Coffee shop"'), findsOneWidget);
    expect(_text('Message in the link (not verified)'), findsOneWidget);
    // One vault: shown, nothing to change.
    expect(find.text('Treasury'), findsOneWidget);
    expect(find.bySemanticsLabel('Change vault'), findsNothing);
  });

  group('funds check', () {
    BigInt z(int v) => BigInt.from(v);

    test('the fee floor is 5,000 zats per action, at least two', () {
      expect(minimumFeeZat(1), z(10000));
      expect(minimumFeeZat(2), z(10000));
      expect(minimumFeeZat(5), z(25000));
    });

    test('spendable funds decide when known', () {
      final need = z(125000000) + minimumFeeZat(1);
      expect(
        checkFunds(
          requestedZat: z(125000000),
          recipients: 1,
          spendableZat: need,
        ).check,
        FundsCheck.enough,
      );
      final short = checkFunds(
        requestedZat: z(125000000),
        recipients: 1,
        spendableZat: need - z(1),
        // A larger total doesn't help: part of it is held or unconfirmed.
        totalZat: need * z(2),
      );
      expect(short.check, FundsCheck.short);
      expect(short.shortByZat, z(1));
    });

    test('a saved total can only prove a shortfall', () {
      expect(
        checkFunds(requestedZat: z(100), recipients: 1, totalZat: z(50)).check,
        FundsCheck.short,
      );
      expect(
        checkFunds(
          requestedZat: z(100),
          recipients: 1,
          totalZat: z(1000000),
        ).check,
        FundsCheck.unknown,
      );
    });

    test('no amount or no balance: unknown', () {
      expect(
        checkFunds(
          requestedZat: BigInt.zero,
          recipients: 1,
          spendableZat: BigInt.zero,
        ).check,
        FundsCheck.unknown,
      );
      expect(
        checkFunds(requestedZat: z(100), recipients: 1).check,
        FundsCheck.unknown,
      );
    });
  });

  testWidgets('a vault that can\'t pay is marked and can\'t be picked', (
    tester,
  ) async {
    final poor = PayFromVault(
      id: 'cc33',
      name: 'Petty cash',
      threshold: 1,
      members: 2,
      spendableZat: BigInt.from(1000),
    );
    String? paidFrom;
    await _pump(
      tester,
      _Harness(vaults: [_treasury, poor], onContinue: (id) => paidFrom = id),
    );
    // The page shows the chosen vault only; the sheet marks the one that's short.
    expect(_text('Not enough funds'), findsNothing);
    await tester.tap(find.text('Change'));
    await tester.pumpAndSettle();
    expect(_text('Not enough funds: short by'), findsOneWidget);
    await tester.tap(find.text('Petty cash'));
    await tester.pumpAndSettle();
    expect(find.text('Pay from'), findsWidgets, reason: 'sheet stays open');
    await tester.tap(find.bySemanticsLabel('Close'));
    await tester.pumpAndSettle();
    await tester.tap(find.bySemanticsLabel(RegExp('^I know who sent this')));
    await tester.pump();
    await tester.tap(find.text('Continue'));
    await tester.pump();
    expect(paidFrom, 'aa11');
  });

  testWidgets('Continue stays off when the only vault can\'t pay', (
    tester,
  ) async {
    var paid = false;
    await _pump(
      tester,
      _Harness(
        vaults: [
          PayFromVault(
            id: 'cc33',
            name: 'Petty cash',
            threshold: 1,
            members: 2,
            spendableZat: BigInt.from(1000),
          ),
        ],
        onContinue: (_) => paid = true,
      ),
    );
    expect(_text('doesn\'t have enough'), findsOneWidget);
    await tester.tap(find.bySemanticsLabel(RegExp('^I know who sent this')));
    await tester.pump();
    await tester.tap(find.text('Continue'));
    await tester.pump();
    expect(paid, isFalse);
  });

  testWidgets('an unpayable link says why and offers nothing to pay', (
    tester,
  ) async {
    await _pump(
      tester,
      _Harness(
        vaults: const [_treasury],
        problem: 'This address is for another network',
        onContinue: (_) {},
      ),
    );
    expect(_text('can\'t be paid'), findsOneWidget);
    expect(_text('another network'), findsOneWidget);
    expect(find.text('Continue'), findsNothing);
  });
}
