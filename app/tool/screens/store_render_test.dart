// Renders the screens for the Google Play listing: Home with a request waiting, the
// payment request a co-signer sees, and the same screens' parts as the app builds them,
// with fake data and the mainnet ticker. Not part of `flutter test`; run it explicitly
// (from app/):
//
//   flutter test --dart-define=ZAFE_NETWORK=main tool/screens/store_render_test.dart
//
// Output (SCREEN_PREVIEW_OUT, default build/screen_preview/):
// store_{home,request}_{dark,light}.png at 3x (1170 px wide).

import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/layout/mobile/app_mobile_tab_bar.dart';
import 'package:zafe/src/core/layout/mobile/mobile_top_nav.dart';
import 'package:zafe/src/core/layout/mobile/zafe_screen.dart';
import 'package:zafe/src/core/theme/app_theme.dart';
import 'package:zafe/src/core/widgets/action_button.dart';
import 'package:zafe/src/core/widgets/app_button.dart';
import 'package:zafe/src/core/widgets/app_icon.dart';
import 'package:zafe/src/core/widgets/mobile/mobile_surface_card.dart';
import 'package:zafe/src/core/widgets/mobile/zafe_detail.dart';
import 'package:zafe/src/features/home/home_screen.dart';
import 'package:zafe/src/features/home/needs_you.dart';
import 'package:zafe/src/features/onboarding/setup_screen.dart';
import 'package:zafe/src/features/proposals/proposal_parts.dart';
import 'package:zafe/src/features/proposals/proposal_status.dart';
import 'package:zafe/src/features/received/received_row.dart';
import 'package:zafe/src/features/signers/signers_screen.dart';
import 'package:zafe/src/features/settings/vault_protection_screen.dart';
import 'package:zafe/src/features/vaults/vault_emblem.dart';
import 'package:zafe/src/providers/proposals_provider.dart';
import 'package:zafe/src/rust/api/proposals.dart' as rust;
import 'package:zafe/src/rust/api/received.dart' as rust;

const _me = 'a3f09c41d2e87b5566c0de19f4a2b7c8e1d0937a6b5c4d3e2f1a0b9c8d7e6f50';
const _maya =
    '5e17b2c9a4d86f3310ab77e2c4d9f1086b3a2c5d7e9f0a1b2c3d4e5f6a7b8c9d';
const _ravi = 'c84d1e0f2a3b4c5d6e7f8091a2b3c4d5e6f708192a3b4c5d6e7f8091a2be8d7';
const _members = [_me, _maya, _ravi];
const _names = {_maya: 'Maya', _ravi: 'Ravi'};
const _vaultId =
    '7b2e9c41d0a8f3e6b5c4d3e2f1a0b9c8d7e6f5a4b3c2d1e0f9a8b7c6d5e4f3a2';
const _addr =
    'u1k3v8m2q9x7h5f4d6s8a0p2o4i6u8y0t2r4e6w8q0z9x7c5v3b1n2m4l6k8j0h2g4f6d8s0a';

rust.ProposalInfo _proposal(
  String id,
  int zat, {
  String memo = '',
  rust.ProposalStage stage = rust.ProposalStage.open,
  List<String> approvals = const [],
  rust.MyVote myVote = rust.MyVote.none,
  String author = _maya,
  int createdAt = 1790000000,
}) => rust.ProposalInfo(
  id: id,
  author: author,
  isMine: author == _me,
  payments: [
    rust.PaymentInfo(address: _addr, amountZat: BigInt.from(zat), memo: memo),
  ],
  totalZat: BigInt.from(zat),
  stage: stage,
  approvals: approvals,
  rejections: const [],
  myVote: myVote,
  threshold: 2,
  rejectionThreshold: 2,
  createdAt: BigInt.from(createdAt),
  txid: null,
  signingStarted: false,
  oneTap: true,
  ready: false,
  completedByMe: false,
  autoSend: true,
  expiryHeight: 0,
  needsReapproval: false,
  stillSendable: false,
  needsNewProposal: false,
);

final _asked = _proposal(
  'p1',
  1250000000,
  memo: 'Audit milestone 2',
  approvals: const [_maya],
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

// Home as HomeScreen lays it out: top bar, vault card, buttons, the request waiting for
// this member, recent activity, tab bar.
Widget _home() => Builder(
  builder: (context) {
    final colors = context.colors;
    return Scaffold(
      backgroundColor: colors.background.window,
      body: SafeArea(
        child: Column(
          children: [
            const MobileTopNav.account(
              accountName: 'Grants',
              syncLabel: 'Synced',
              avatar: VaultEmblem(vaultId: _vaultId),
            ),
            Expanded(
              child: ListView(
                padding: const EdgeInsets.fromLTRB(16, 16, 16, 16),
                children: [
                  BalanceCard(
                    totalZat: BigInt.from(4237485000),
                    fiatText: null,
                    hidden: false,
                    onToggle: () {},
                    threshold: 2,
                    members: 3,
                  ),
                  const SizedBox(height: AppSpacing.s),
                  Row(
                    children: [
                      Expanded(
                        child: AppButton(
                          expand: true,
                          onPressed: () {},
                          leading: const AppIcon(AppIcons.plane, size: 20),
                          child: const Text('New payment'),
                        ),
                      ),
                      const SizedBox(width: AppSpacing.xs),
                      Expanded(
                        child: AppButton(
                          expand: true,
                          variant: AppButtonVariant.secondary,
                          onPressed: () {},
                          leading: const AppIcon(
                            AppIcons.arrowDownCircle,
                            size: 20,
                          ),
                          child: const Text('Receive'),
                        ),
                      ),
                    ],
                  ),
                  const SizedBox(height: AppSpacing.md),
                  RequestCard(
                    tileKey: _maya,
                    who: 'Maya',
                    amount: '12.5',
                    detail: 'Audit milestone 2',
                    action: 'Approve',
                    onTap: () {},
                    onAction: () {},
                    onDecline: () {},
                  ),
                  const SizedBox(height: AppSpacing.md),
                  Row(
                    children: [
                      Expanded(
                        child: Text(
                          'Recent activity',
                          style: AppTypography.labelLarge.copyWith(
                            color: colors.text.accent,
                            fontWeight: FontWeight.w600,
                          ),
                        ),
                      ),
                      Text(
                        'See all',
                        style: AppTypography.labelLarge.copyWith(
                          color: colors.button.ghost.label,
                        ),
                      ),
                      AppIcon(
                        AppIcons.chevronForward,
                        size: 16,
                        color: colors.button.ghost.label,
                      ),
                    ],
                  ),
                  const SizedBox(height: AppSpacing.md),
                  for (final row in <Widget>[
                    ReceivedRow(
                      received: rust.ReceivedInfo(
                        txid: 'aa',
                        amountZat: BigInt.from(2500000000),
                        minedHeight: 1200,
                        blockTimeSecs: 1789850000,
                        confirmations: 12,
                        memo: '',
                        isCoinbase: false,
                      ),
                      onTap: () {},
                    ),
                    ProposalRow(
                      proposal: _proposal(
                        'p3',
                        300000000,
                        stage: rust.ProposalStage.sent,
                        approvals: const [_ravi, _me],
                        author: _ravi,
                        createdAt: 1789600000,
                      ),
                      onTap: () {},
                    ),
                    ProposalRow(
                      proposal: _proposal(
                        'p4',
                        80000000,
                        stage: rust.ProposalStage.sent,
                        approvals: const [_maya, _me],
                        createdAt: 1789100000,
                      ),
                      onTap: () {},
                    ),
                  ]) ...[row, const SizedBox(height: AppSpacing.s)],
                ],
              ),
            ),
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 0, 16, 24),
              child: AppMobileTabBar(
                items: const [
                  AppMobileTabItem(iconName: AppIcons.home, label: 'Home'),
                  AppMobileTabItem(
                    iconName: AppIcons.history,
                    label: 'Activity',
                  ),
                  AppMobileTabItem(iconName: AppIcons.users, label: 'Signers'),
                  AppMobileTabItem(iconName: AppIcons.cog, label: 'Settings'),
                ],
                currentIndex: 0,
                onSelect: (_) {},
              ),
            ),
          ],
        ),
      ),
    );
  },
);

// The payment request as ProposalScreen shows it to a member who hasn't voted.
Widget _request() => ZafeScreen(
  title: 'Payment request',
  bottom: Column(
    mainAxisSize: MainAxisSize.min,
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      ActionButton(
        label: 'Approve',
        icon: AppIcons.check,
        onPressed: () async {},
      ),
      const SizedBox(height: AppSpacing.xs),
      ActionButton(
        label: 'Decline',
        variant: AppButtonVariant.ghost,
        onPressed: () async {},
      ),
    ],
  ),
  children: [
    PaymentRequestView(
      proposal: _asked,
      members: _members,
      me: _me,
      names: _names,
    ),
  ],
);

// The Signers tab: backup health and each signer's phone.
Widget _signers() => ZafeScreen(
  title: 'Signers',
  showBack: false,
  children: [
    const BackupHealthCard(signers: 3, threshold: 2, backedUp: 3),
    const SizedBox(height: AppSpacing.md),
    MobileSurfaceCard(
      cornerRadius: AppRadii.large,
      child: Column(
        children: [
          for (final m in _members)
            SignerRow(
              keyHex: m,
              me: _me,
              name: _names[m],
              trailing: const BackupLabel(backedUp: true),
            ),
        ],
      ),
    ),
  ],
);

// Key generation: each member's shard drifts on its own (the whole key never exists).
Widget _keys() => CreatingKeysScreen(onAbandon: () {});

Widget _protection() => const ZafeScreen(
  title: 'How it\'s protected',
  children: [VaultProtectionBody(threshold: 2, signers: 3)],
);

void main() {
  testWidgets('render store screens', (tester) async {
    await tester.runAsync(_loadFonts);
    final out = Directory(
      Platform.environment['SCREEN_PREVIEW_OUT'] ?? 'build/screen_preview',
    )..createSync(recursive: true);
    tester.view.devicePixelRatio = 3;
    tester.view.physicalSize = const Size(390 * 3, 844 * 3);
    addTearDown(tester.view.reset);

    for (final (name, screen) in [
      ('home', _home),
      ('request', _request),
      ('signers', _signers),
      ('protection', _protection),
      ('keys', _keys),
    ]) {
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
            child: RepaintBoundary(
              key: boundary,
              child: MaterialApp(
                debugShowCheckedModeBanner: false,
                // No desktop scrollbars in a phone screenshot.
                scrollBehavior: const MaterialScrollBehavior().copyWith(
                  scrollbars: false,
                ),
                builder: (context, child) =>
                    AppTheme(data: data, child: child!),
                home: screen(),
              ),
            ),
          ),
        );
        // Enough frames for the key art to load and close.
        for (var i = 0; i < 12; i++) {
          await tester.runAsync(
            () => Future<void>.delayed(const Duration(milliseconds: 50)),
          );
          await tester.pump(const Duration(milliseconds: 300));
        }
        final render =
            boundary.currentContext!.findRenderObject()!
                as RenderRepaintBoundary;
        final bytes = await tester.runAsync(() async {
          final image = await render.toImage(pixelRatio: 3);
          final data = await image.toByteData(format: ui.ImageByteFormat.png);
          return data!.buffer.asUint8List();
        });
        final path = '${out.path}/store_${name}_$theme.png';
        File(path).writeAsBytesSync(bytes!);
        // ignore: avoid_print
        print(path);
      }
    }
  });
}
