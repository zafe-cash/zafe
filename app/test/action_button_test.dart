import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/theme/app_theme.dart';
import 'package:zafe/src/core/widgets/action_button.dart';

Future<void> _pump(WidgetTester tester, Widget button) => tester.pumpWidget(
  MaterialApp(
    home: AppTheme(
      data: AppThemeData.dark,
      child: Scaffold(
        body: Center(child: SizedBox(width: 320, child: button)),
      ),
    ),
  ),
);

void main() {
  testWidgets('works, shows done, then runs onDone', (tester) async {
    final gate = Completer<void>();
    var done = 0;
    await _pump(
      tester,
      ActionButton(
        label: 'Approve',
        doneLabel: 'Approved',
        onPressed: () => gate.future,
        onDone: () => done++,
      ),
    );
    await tester.tap(find.text('Approve'));
    await tester.pump();
    // Working: still labelled, a second tap does nothing.
    await tester.tap(find.text('Approve'));
    gate.complete();
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 200));
    expect(find.text('Approved'), findsOneWidget);
    expect(done, 0);
    await tester.pump(ActionButton.doneHold);
    expect(done, 1);
  });

  testWidgets('a failure shows its message under the button', (tester) async {
    await _pump(
      tester,
      ActionButton(
        label: 'Propose',
        errorFallback: 'Couldn\'t propose. Try again.',
        onPressed: () async => throw StateError('relay down'),
      ),
    );
    await tester.tap(find.text('Propose'));
    await tester.pumpAndSettle();
    expect(find.text('Couldn\'t propose. Try again.'), findsOneWidget);
    expect(find.text('Propose'), findsOneWidget, reason: 'back to idle');
  });

  testWidgets('backing out shows nothing', (tester) async {
    var done = 0;
    await _pump(
      tester,
      ActionButton(
        label: 'Approve',
        doneLabel: 'Approved',
        onPressed: () async => throw const ActionCancelled(),
        onDone: () => done++,
      ),
    );
    await tester.tap(find.text('Approve'));
    await tester.pumpAndSettle();
    expect(find.text('Approve'), findsOneWidget);
    expect(find.textContaining('Try again'), findsNothing);
    expect(done, 0);
  });
}
