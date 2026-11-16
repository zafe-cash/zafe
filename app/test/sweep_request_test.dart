import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/storage/member_names.dart';
import 'package:zafe/src/core/theme/app_theme.dart';
import 'package:zafe/src/features/proposals/proposal_parts.dart';
import 'package:zafe/src/providers/privacy_mode_provider.dart';
import 'package:zafe/src/providers/proposals_provider.dart';
import 'package:zafe/src/rust/api/proposals.dart' as rust;

import 'vault_updates_test.dart' show proposal;

class _Privacy extends PrivacyModeNotifier {
  @override
  bool build() => false;
}

rust.ReviewInfo _review({required bool verified}) => rust.ReviewInfo(
  verified: verified,
  problem: verified ? '' : 'Change goes to an address that is not the vault',
  feeZat: BigInt.from(10000),
  changeZat: verified ? BigInt.from(149990000) : BigInt.zero,
  inputZat: verified ? BigInt.from(150000000) : BigInt.zero,
  spends: 1,
  expiryHeight: 12000,
  tipHeight: 2000,
);

Future<String> _shown(WidgetTester tester, rust.ReviewInfo review) async {
  tester.view.physicalSize = const Size(900, 2400);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  final p = proposal('p', author: 'a1' * 32);
  final sweep = rust.ProposalInfo(
    id: p.id,
    author: p.author,
    isMine: p.isMine,
    payments: const [],
    totalZat: BigInt.zero,
    stage: p.stage,
    approvals: p.approvals,
    rejections: p.rejections,
    myVote: p.myVote,
    threshold: p.threshold,
    rejectionThreshold: p.rejectionThreshold,
    createdAt: p.createdAt,
    txid: p.txid,
    signingStarted: p.signingStarted,
    oneTap: p.oneTap,
    ready: p.ready,
    completedByMe: p.completedByMe,
    autoSend: p.autoSend,
    expiryHeight: p.expiryHeight,
    needsReapproval: p.needsReapproval,
    stillSendable: p.stillSendable,
    sentAnyway: p.sentAnyway,
    maybeSigned: p.maybeSigned,
    invalidatedBy: p.invalidatedBy,
    needsNewProposal: p.needsNewProposal,
  );
  await tester.pumpWidget(
    ProviderScope(
      overrides: [
        privacyModeProvider.overrideWith(_Privacy.new),
        proposalReviewProvider.overrideWith((ref, id) async => review),
      ],
      child: MaterialApp(
        home: AppTheme(
          data: AppThemeData.dark,
          child: Scaffold(
            body: SingleChildScrollView(
              child: PaymentRequestView(
                proposal: sweep,
                members: ['a1' * 32, 'b0' * 32],
                me: 'b0' * 32,
                names: SignerNames.none,
              ),
            ),
          ),
        ),
      ),
    ),
  );
  await tester.pump();
  return [
    for (final t in tester.widgetList<Text>(find.byType(Text)))
      t.data ?? t.textSpan?.toPlainText() ?? '',
  ].join('\n');
}

void main() {
  testWidgets('a sweep request shows what it moves, not 0', (tester) async {
    final shown = await _shown(tester, _review(verified: true));
    expect(shown, contains('1.4999 TAZ'));
    expect(shown, contains('Back to the vault'));
  });

  testWidgets('an unchecked sweep shows no amount', (tester) async {
    final shown = await _shown(tester, _review(verified: false));
    expect(shown, isNot(contains('TAZ')));
  });
}
