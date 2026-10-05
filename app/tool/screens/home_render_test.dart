// Renders Home's parts (vault card, buttons, notice card, activity rows, tab bar) with
// fake data, in both themes, to PNGs for review without a device. HomeScreen itself
// needs the Rust bridge, so this pumps its public pieces. Not part of `flutter test`;
// run it explicitly (from app/):
//
//   flutter test tool/screens/home_render_test.dart
//
// Output (SCREEN_PREVIEW_OUT, default build/screen_preview/): home_{dark,light}.png.
// HOME_NOTICE=0 leaves out the backup notice (the website's showcase uses that).
import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/layout/mobile/app_mobile_tab_bar.dart';
import 'package:zafe/src/core/theme/app_theme.dart';
import 'package:zafe/src/core/widgets/app_button.dart';
import 'package:zafe/src/core/widgets/app_icon.dart';
import 'package:zafe/src/features/home/home_screen.dart';
import 'package:zafe/src/features/proposals/proposal_status.dart';
import 'package:zafe/src/features/received/received_row.dart';
import 'package:zafe/src/rust/api/proposals.dart' as rust;
import 'package:zafe/src/rust/api/received.dart' as rust;

const _me = 'a3f09c41d2e87b5566c0de19f4a2b7c8e1d0937a6b5c4d3e2f1a0b9c8d7e6f50';
const _bob = '5e17b2c9a4d86f3310ab77e2c4d9f1086b3a2c5d7e9f0a1b2c3d4e5f6a7b8c9d';
const _addr =
    'utest1k3v8m2q9x7h5f4d6s8a0p2o4i6u8y0t2r4e6w8q0z9x7c5v3b1n2m4l6k8j0h2g4f6d8s0a';

rust.ProposalInfo _proposal(
  String id,
  int zat, {
  rust.ProposalStage stage = rust.ProposalStage.open,
  List<String> approvals = const [],
  rust.MyVote myVote = rust.MyVote.none,
}) => rust.ProposalInfo(
  id: id,
  author: _bob,
  isMine: false,
  payments: [
    rust.PaymentInfo(address: _addr, amountZat: BigInt.from(zat), memo: ''),
  ],
  totalZat: BigInt.from(zat),
  stage: stage,
  approvals: approvals,
  rejections: const [],
  myVote: myVote,
  threshold: 2,
  rejectionThreshold: 2,
  createdAt: BigInt.from(1790000000),
  txid: null,
  signingStarted: false,
  oneTap: true,
  ready: false,
  completedByMe: false,
  autoSend: true,
  expiryHeight: 0,
  needsReapproval: false,
  stillSendable: false,
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

Widget _home() => Builder(
  builder: (context) {
    final colors = context.colors;
    return Material(
      color: colors.background.window,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Expanded(
            child: ListView(
              padding: const EdgeInsets.fromLTRB(16, 56, 16, 16),
              children: [
                BalanceCard(
                  totalZat: BigInt.from(4237485000),
                  fiatText: r'$5,668.30',
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
                if (_showNotice) ...[
                  const SizedBox(height: AppSpacing.md),
                  NoticeCard(
                    title: 'Back up this vault',
                    body: 'Save an encrypted backup so you can restore it.',
                    onTap: () {},
                  ),
                ],
                const SizedBox(height: AppSpacing.md),
                Text(
                  'Recent activity',
                  style: AppTypography.labelLarge.copyWith(
                    color: colors.text.accent,
                    fontWeight: FontWeight.w600,
                  ),
                ),
                const SizedBox(height: AppSpacing.md),
                for (final row in <Widget>[
                  ProposalRow(
                    proposal: _proposal('p1', 125000000, approvals: [_bob]),
                    onTap: () {},
                  ),
                  ReceivedRow(
                    received: rust.ReceivedInfo(
                      txid: 'aa',
                      amountZat: BigInt.from(50000000),
                      minedHeight: 1200,
                      blockTimeSecs: 1790000000,
                      confirmations: 12,
                      memo: '',
                      isCoinbase: false,
                    ),
                    onTap: () {},
                  ),
                  ProposalRow(
                    proposal: _proposal(
                      'p2',
                      300000000,
                      approvals: [_bob, _me],
                      myVote: rust.MyVote.approved,
                    ),
                    onTap: () {},
                  ),
                  ProposalRow(
                    proposal: _proposal(
                      'p3',
                      80000000,
                      stage: rust.ProposalStage.sent,
                      approvals: [_bob, _me],
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
                AppMobileTabItem(iconName: AppIcons.history, label: 'Activity'),
                AppMobileTabItem(iconName: AppIcons.users, label: 'Signers'),
                AppMobileTabItem(iconName: AppIcons.cog, label: 'Settings'),
              ],
              currentIndex: 0,
              onSelect: (_) {},
            ),
          ),
        ],
      ),
    );
  },
);

final _showNotice = Platform.environment['HOME_NOTICE'] != '0';

void main() {
  testWidgets('render home parts', (tester) async {
    await tester.runAsync(_loadFonts);
    final out = Directory(
      Platform.environment['SCREEN_PREVIEW_OUT'] ?? 'build/screen_preview',
    )..createSync(recursive: true);
    tester.view.devicePixelRatio = 3;
    tester.view.physicalSize = const Size(390 * 3, 844 * 3);
    addTearDown(tester.view.reset);

    for (final (theme, data) in [
      ('dark', AppThemeData.dark),
      ('light', AppThemeData.light),
    ]) {
      final boundary = GlobalKey();
      await tester.pumpWidget(
        ProviderScope(
          child: MaterialApp(
            debugShowCheckedModeBanner: false,
            home: RepaintBoundary(
              key: boundary,
              child: AppTheme(data: data, child: _home()),
            ),
          ),
        ),
      );
      for (var i = 0; i < 5; i++) {
        await tester.runAsync(
          () => Future<void>.delayed(const Duration(milliseconds: 50)),
        );
        await tester.pump(const Duration(milliseconds: 300));
      }
      final render =
          boundary.currentContext!.findRenderObject()! as RenderRepaintBoundary;
      final bytes = await tester.runAsync(() async {
        final image = await render.toImage(pixelRatio: 2);
        final data = await image.toByteData(format: ui.ImageByteFormat.png);
        return data!.buffer.asUint8List();
      });
      final path = '${out.path}/home_$theme.png';
      File(path).writeAsBytesSync(bytes!);
      // ignore: avoid_print
      print(path);
    }
  });
}
