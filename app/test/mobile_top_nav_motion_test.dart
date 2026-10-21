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
  testWidgets('the shimmer steps on a timer, never one frame per vsync', (
    tester,
  ) async {
    await tester.pumpWidget(_nav(animated: true));
    expect(find.byType(ShaderMask), findsOneWidget);
    // An AnimationController would keep a frame scheduled all the time (a redraw of the
    // whole screen at the display's rate). Between timer ticks nothing is scheduled.
    expect(tester.hasRunningAnimations, isFalse);
    for (var i = 0; i < 40; i++) {
      await tester.pump(const Duration(milliseconds: 25));
      expect(
        tester.binding.hasScheduledFrame,
        isFalse,
        reason: 'frame scheduled after ${(i + 1) * 25} ms',
      );
    }
    expect(find.text('Syncing...'), findsOneWidget);
  });

  testWidgets('the shimmer keeps going for as long as the sync lasts', (
    tester,
  ) async {
    await tester.pumpWidget(_nav(animated: true));
    await tester.pump(const Duration(minutes: 3));
    expect(find.byType(ShaderMask), findsOneWidget);
  });

  testWidgets('it stops, and leaves no timer, when the sync ends', (
    tester,
  ) async {
    await tester.pumpWidget(_nav(animated: true));
    await tester.pump(const Duration(seconds: 1));
    await tester.pumpWidget(_nav(animated: false));
    expect(find.byType(ShaderMask), findsNothing);
    await tester.pump(const Duration(seconds: 5));
    expect(tester.binding.hasScheduledFrame, isFalse);
    // The framework fails a test that ends with a timer still pending.
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('a synced label does not animate at all', (tester) async {
    await tester.pumpWidget(_nav(animated: false));
    await tester.pump(const Duration(seconds: 1));
    expect(find.byType(ShaderMask), findsNothing);
    expect(tester.hasRunningAnimations, isFalse);
  });
}
