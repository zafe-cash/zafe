import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/storage/member_names.dart';
import 'package:zafe/src/core/formatting/member_label.dart';
import 'package:zafe/src/core/theme/app_theme.dart';
import 'package:zafe/src/features/home/needs_you.dart';
import 'package:zafe/src/features/proposals/proposal_parts.dart';
import 'package:zafe/src/features/proposals/proposal_timeline.dart';
import 'package:zafe/src/providers/privacy_mode_provider.dart';

import 'vault_updates_test.dart' show proposal;

class _Privacy extends PrivacyModeNotifier {
  @override
  bool build() => false;
}

// 2-of-3, Bob's phone. Bob named Alice's key "Alice" here; Mallory calls herself "Alice"
// in the vault log, or "Alicе" with a Cyrillic е next to Alice's own shared "Alice".
void main() {
  final alice = 'a1' * 32, bob = 'b0' * 32, mallory = 'c3' * 32;
  final members = [alice, bob, mallory];

  for (final (what, names) in [
    (
      'a claim of a label',
      SignerNames(local: {alice: 'Alice'}, shared: {mallory: 'Alice'}),
    ),
    (
      'a lookalike claim',
      SignerNames(shared: {alice: 'Alice', mallory: 'Alicе'}),
    ),
  ]) {
    testWidgets('Mallory\'s payment never reads as Alice\'s ($what)', (
      tester,
    ) async {
      tester.view.physicalSize = const Size(900, 2400);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      final p = proposal('p', author: mallory);
      await tester.pumpWidget(
        ProviderScope(
          overrides: [privacyModeProvider.overrideWith(_Privacy.new)],
          child: MaterialApp(
            home: AppTheme(
              data: AppThemeData.dark,
              child: Scaffold(
                body: SingleChildScrollView(
                  child: PaymentRequestView(
                    proposal: p,
                    members: members,
                    me: bob,
                    names: names,
                  ),
                ),
              ),
            ),
          ),
        ),
      );
      await tester.pump();
      final shown = [
        for (final t in tester.widgetList<Text>(find.byType(Text)))
          t.data ?? t.textSpan?.toPlainText() ?? '',
      ].join('\n');
      expect(shown, contains('c3c3c3...c3c3'));
      expect(
        RegExp(r'\bAlice\b(?! \()').hasMatch(shown),
        isFalse,
        reason: shown,
      );
      final steps = timelineSteps(p, members: members, me: bob, names: names);
      expect(steps.first.title, contains('c3c3c3...c3c3'));
    });
  }

  // The key after a name must stay in view: a long claim at 360 dp and 2.0 text size
  // used to cut it off (one line for a seat move, two for "who · detail").
  for (final (what, amount, by) in [
    ('a payment', '100', null),
    ('a seat move', null, null),
    ('a relay move', null, 'mover'),
  ]) {
    testWidgets('the key after a name is never cut ($what)', (tester) async {
      tester.view.physicalSize = const Size(360, 1600);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      final names = SignerNames(
        shared: {mallory: 'Alice Wonderland Liddell Smith'},
      );
      final who = memberLabel(mallory, names: names);
      await tester.pumpWidget(
        MaterialApp(
          home: MediaQuery(
            data: const MediaQueryData(
              size: Size(360, 1600),
              textScaler: TextScaler.linear(2),
            ),
            child: AppTheme(
              data: AppThemeData.dark,
              child: Scaffold(
                body: SingleChildScrollView(
                  child: RequestCard(
                    tileKey: mallory,
                    who: by == null ? who : 'Move Grants to relay.example.org',
                    by: by == null ? null : '$who asked',
                    amount: amount,
                    detail: 'To u1abcde .... vwxyz12 · expires in 3 hours',
                    action: 'Review',
                    onTap: () {},
                  ),
                ),
              ),
            ),
          ),
        ),
      );
      final line = find.textContaining('c3c3c3...c3c3');
      expect(line, findsOneWidget);
      expect(
        tester.renderObject<RenderParagraph>(line).didExceedMaxLines,
        isFalse,
      );
    });
  }
}
