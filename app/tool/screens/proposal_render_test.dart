// Renders the proposal screen's body (payment card, approvals, details) and the
// signer rows with fake data, in both themes, to PNGs for review without a device.
// Not part of `flutter test`; run it explicitly (from app/):
//
//   flutter test tool/screens/proposal_render_test.dart
//
// Output (SCREEN_PREVIEW_OUT, default build/screen_preview/): one PNG per scenario.
// The action buttons are stand-ins with the screen's copy (the real ones live in
// ProposalScreen, which needs the Rust bridge).
import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/layout/mobile/zafe_screen.dart';
import 'package:zafe/src/core/theme/app_theme.dart';
import 'package:zafe/src/core/widgets/app_button.dart';
import 'package:zafe/src/core/widgets/app_icon.dart';
import 'package:zafe/src/core/widgets/mobile/mobile_surface_card.dart';
import 'package:zafe/src/core/widgets/mobile/zafe_detail.dart';
import 'package:zafe/src/features/proposals/proposal_parts.dart';
import 'package:zafe/src/providers/proposals_provider.dart';
import 'package:zafe/src/rust/api/proposals.dart' as rust;

const _me = 'a3f09c41d2e87b5566c0de19f4a2b7c8e1d0937a6b5c4d3e2f1a0b9c8d7e6f50';
const _bob = '5e17b2c9a4d86f3310ab77e2c4d9f1086b3a2c5d7e9f0a1b2c3d4e5f6a7b8c9d';
const _cara =
    'c84d1e6fa2b937054e8d6c1b0a9f8e7d6c5b4a39281706f5e4d3c2b1a0f9e8d7';
const _dev = '0b92e4f7c1a6d3588d2e1f0c9b8a7f6e5d4c3b2a19087f6e5d4c3b2a1f0e9d8c';
const _addr =
    'u1k3v8m2q9x7h5f4d6s8a0p2o4i6u8y0t2r4e6w8q0z9x7c5v3b1n2m4l6k8j0h2g4f6d8s0a';

rust.ProposalInfo _proposal({
  rust.ProposalStage stage = rust.ProposalStage.open,
  List<String> approvals = const [],
  List<String> rejections = const [],
  rust.MyVote myVote = rust.MyVote.none,
  bool ready = false,
  bool mine = false,
  String? txid,
}) => rust.ProposalInfo(
  id: 'p1',
  author: _bob,
  isMine: mine,
  payments: [
    rust.PaymentInfo(
      address: _addr,
      amountZat: BigInt.from(1250000000),
      memo: 'Grant: Q4 audit milestone',
    ),
  ],
  totalZat: BigInt.from(1250000000),
  stage: stage,
  approvals: approvals,
  rejections: rejections,
  myVote: myVote,
  threshold: 2,
  rejectionThreshold: 2,
  createdAt: BigInt.from(1790000000),
  txid: txid,
  signingStarted: false,
  oneTap: true,
  ready: ready,
  completedByMe: false,
  autoSend: true,
  expiryHeight: 0,
  needsReapproval: false,
  stillSendable: false,
  needsNewProposal: false,
);

final _review = rust.ReviewInfo(
  verified: true,
  problem: '',
  feeZat: BigInt.from(15000),
  changeZat: BigInt.from(237485000),
  inputZat: BigInt.from(1487500000),
  spends: 2,
  expiryHeight: 12000,
  tipHeight: 2000,
);

Future<void> _loadFonts() async {
  final families = <String, List<String>>{
    'DM Sans': ['Regular', 'Medium', 'SemiBold'],
    'JetBrains Mono': ['Regular', 'Medium'],
    'Space Grotesk': ['Medium', 'SemiBold'],
  };
  for (final MapEntry(key: family, value: weights) in families.entries) {
    final loader = FontLoader(family);
    final file = family.replaceAll(' ', '');
    for (final w in weights) {
      final bytes = File('assets/fonts/$file-$w.ttf').readAsBytesSync();
      loader.addFont(Future.value(ByteData.sublistView(bytes)));
    }
    await loader.load();
  }
}

Widget _actions(
  String primary, {
  String? note,
  String? secondary,
  String icon = AppIcons.check,
}) => Column(
  crossAxisAlignment: CrossAxisAlignment.stretch,
  children: [
    const SizedBox(height: AppSpacing.lg),
    AppButton(
      expand: true,
      leading: AppIcon(icon, size: 20),
      onPressed: () {},
      child: Text(primary),
    ),
    if (note != null) ...[
      const SizedBox(height: AppSpacing.xs),
      Builder(
        builder: (context) => Text(
          note,
          textAlign: TextAlign.center,
          style: AppTypography.bodySmall.copyWith(
            color: context.colors.text.secondary,
          ),
        ),
      ),
    ],
    if (secondary != null) ...[
      const SizedBox(height: AppSpacing.s),
      AppButton(
        expand: true,
        variant: AppButtonVariant.ghost,
        onPressed: () {},
        child: Text(secondary),
      ),
    ],
  ],
);

final _members = [_me, _bob, _cara];

final _scenarios = <String, (String, List<Widget>)>{
  'proposal_review': (
    'Review payment',
    [
      ProposalBody(
        proposal: _proposal(approvals: [_bob]),
        members: _members,
        me: _me,
      ),
      _actions(
        'Approve and sign',
        note:
            'Approving signs the payment on this device. It can\'t be withdrawn afterwards.',
        secondary: 'Reject',
      ),
    ],
  ),
  'proposal_ready': (
    'Ready to send',
    [
      ProposalBody(
        proposal: _proposal(
          stage: rust.ProposalStage.approved,
          approvals: [_bob, _me],
          myVote: rust.MyVote.approved,
          ready: true,
        ),
        members: _members,
        me: _me,
      ),
      _actions(
        'Send now',
        note:
            'Every signature is in. It is sent automatically by the signer who approved last; you can also send it yourself.',
      ),
    ],
  ),
  'proposal_rejected': (
    'Rejected',
    [
      ProposalBody(
        proposal: _proposal(
          stage: rust.ProposalStage.rejected,
          rejections: [_cara, _me],
          myVote: rust.MyVote.rejected,
        ),
        members: _members,
        me: _me,
      ),
    ],
  ),
  'proposal_sent': (
    'Sent',
    [
      ProposalBody(
        proposal: _proposal(
          stage: rust.ProposalStage.sent,
          approvals: [_bob, _cara],
          txid:
              '9f3c2a1b0e8d7c6b5a4f3e2d1c0b9a8f7e6d5c4b3a291807f6e5d4c3b2a1f0e9',
        ),
        members: [..._members, _dev],
        me: _me,
      ),
    ],
  ),
  'signers_and_new_payment': (
    'Signer rows',
    [
      PaymentCard(
        label: 'NEW PAYMENT',
        amountText: '12.5',
        address: _addr,
        onFullAddress: () {},
      ),
      const SizedBox(height: AppSpacing.md),
      MobileSurfaceCard(
        cornerRadius: AppRadii.large,
        child: Column(
          children: [
            for (final m in [_me, _bob, _cara, _dev])
              SignerRow(keyHex: m, me: _me),
          ],
        ),
      ),
    ],
  ),
  // The website's story (infra/site): Bob proposes and approves on his phone (shown
  // in dark), you approve on yours (shown in light) and it's sent.
  'story_bob_propose': (
    'Review',
    [
      PaymentCard(
        label: 'NEW PAYMENT',
        amountText: '12.50',
        address: _addr,
        onFullAddress: () {},
      ),
      const SizedBox(height: AppSpacing.lg),
      const MobileSurfaceCard(
        cornerRadius: AppRadii.large,
        child: Column(
          children: [
            DetailRow(label: 'Message', value: 'Grant: Q4 audit milestone'),
            DetailDivider(),
            DetailRow(label: 'Approvals needed', value: '2 of 3 signers'),
            DetailDivider(),
            DetailRow(label: 'Network fee', value: 'At least 0.0001 TAZ'),
          ],
        ),
      ),
      const SizedBox(height: AppSpacing.sm),
      Builder(
        builder: (context) => Text(
          'Nothing is sent yet. Each signer checks this payment on their own device and '
          'approves with one tap; 2 approvals complete it.',
          style: AppTypography.bodySmall.copyWith(
            color: context.colors.text.secondary,
          ),
        ),
      ),
      _actions('Propose payment', icon: AppIcons.plane, secondary: 'Cancel'),
    ],
  ),
  'story_bob_approve': (
    'Your payment',
    [
      ProposalBody(
        proposal: _proposal(mine: true),
        members: _members,
        me: _bob,
      ),
      _actions(
        'Approve and sign',
        note:
            'Approving signs the payment on this device. It can\'t be withdrawn afterwards.',
      ),
    ],
  ),
  'story_bob_approved': (
    'Your payment',
    [
      ProposalBody(
        proposal: _proposal(
          mine: true,
          approvals: [_bob],
          myVote: rust.MyVote.approved,
        ),
        members: _members,
        me: _bob,
      ),
    ],
  ),
  'story_me_sent': (
    'Sent',
    [
      ProposalBody(
        proposal: _proposal(
          stage: rust.ProposalStage.sent,
          approvals: [_bob, _me],
          myVote: rust.MyVote.approved,
          txid:
              '9f3c2a1b0e8d7c6b5a4f3e2d1c0b9a8f7e6d5c4b3a291807f6e5d4c3b2a1f0e9',
        ),
        members: _members,
        me: _me,
      ),
    ],
  ),
};

void main() {
  testWidgets('render proposal screens', (tester) async {
    await tester.runAsync(_loadFonts);
    final out = Directory(
      Platform.environment['SCREEN_PREVIEW_OUT'] ?? 'build/screen_preview',
    )..createSync(recursive: true);
    tester.view.devicePixelRatio = 3;
    tester.view.physicalSize = const Size(390 * 3, 1320 * 3);
    addTearDown(tester.view.reset);

    for (final MapEntry(key: name, value: (title, children))
        in _scenarios.entries) {
      for (final (theme, data) in [
        ('dark', AppThemeData.dark),
        ('light', AppThemeData.light),
      ]) {
        final boundary = GlobalKey();
        await tester.pumpWidget(
          ProviderScope(
            overrides: [
              proposalReviewProvider.overrideWith((ref, id) async => _review),
            ],
            child: MaterialApp(
              debugShowCheckedModeBanner: false,
              home: RepaintBoundary(
                key: boundary,
                child: AppTheme(
                  data: data,
                  child: ZafeScreen(title: title, children: children),
                ),
              ),
            ),
          ),
        );
        // Let the SVG icons and the review future load.
        for (var i = 0; i < 5; i++) {
          await tester.runAsync(
            () => Future<void>.delayed(const Duration(milliseconds: 50)),
          );
          await tester.pump(const Duration(milliseconds: 100));
        }
        final render =
            boundary.currentContext!.findRenderObject()!
                as RenderRepaintBoundary;
        final bytes = await tester.runAsync(() async {
          final image = await render.toImage(pixelRatio: 2);
          final data = await image.toByteData(format: ui.ImageByteFormat.png);
          return data!.buffer.asUint8List();
        });
        final path = '${out.path}/${name}_$theme.png';
        File(path).writeAsBytesSync(bytes!);
        // ignore: avoid_print
        print(path);
      }
    }
  });
}
