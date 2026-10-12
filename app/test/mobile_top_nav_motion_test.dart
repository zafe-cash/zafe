import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/layout/mobile/mobile_top_nav.dart';
import 'package:zafe/src/core/theme/app_theme.dart';

Widget _nav({required bool animated}) => MaterialApp(
  builder: (context, child) => AppTheme(data: AppThemeData.dark, child: child!),
  home: Scaffold(
    body: MobileTopNav.account(
      accountName: 'Vault',
      syncLabel: 'Syncing...',
      syncAnimated: animated,
    ),
  ),
);

void main() {
  testWidgets('the sync shimmer runs for its budget, then the label is still', (
    tester,
  ) async {
    await tester.pumpWidget(_nav(animated: true));
    await tester.pump(const Duration(seconds: 1));
    expect(tester.hasRunningAnimations, isTrue);

    // Nearly out of budget: still moving.
    await tester.pump(const Duration(seconds: 8));
    expect(tester.hasRunningAnimations, isTrue);

    // Budget spent: no controller ticks any more, the text stays.
    await tester.pump(const Duration(seconds: 2));
    await tester.pump();
    expect(tester.hasRunningAnimations, isFalse);
    expect(find.text('Syncing...'), findsOneWidget);
  });

  testWidgets('a new syncing episode gets a fresh budget', (tester) async {
    await tester.pumpWidget(_nav(animated: true));
    await tester.pump(const Duration(seconds: 11));
    await tester.pump();
    expect(tester.hasRunningAnimations, isFalse);

    await tester.pumpWidget(_nav(animated: false));
    await tester.pumpWidget(_nav(animated: true));
    await tester.pump(const Duration(seconds: 1));
    expect(tester.hasRunningAnimations, isTrue);
    await tester.pump(const Duration(seconds: 11));
    await tester.pump();
    expect(tester.hasRunningAnimations, isFalse);
  });

  testWidgets('a synced label does not animate at all', (tester) async {
    await tester.pumpWidget(_nav(animated: false));
    await tester.pump(const Duration(seconds: 1));
    expect(tester.hasRunningAnimations, isFalse);
  });
}
