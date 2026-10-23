// Renders the app components the website's section loops are made of (infra/site/film),
// from the app's own widgets with fake data, frame by frame at 30 fps: each moment as a card
// on its own (the request through "Approved", sending through "Sent!", the keys being made,
// the vault with a request waiting).
// Not part of `flutter test`; run it from app/:
//
//   flutter test --dart-define=ZAFE_NETWORK=main tool/screens/site_motion_render_test.dart
//
// Output (SCREEN_PREVIEW_OUT, default build/screen_preview/): motion_<card>/f_<n>.png, the
// card alone (transparent around it), 360 wide at 3x. infra/site/film composes them.
import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/widgets/app_icon.dart';
import 'package:zafe/src/features/home/home_screen.dart';
import 'package:zafe/src/features/home/needs_you.dart';
import 'package:zafe/src/features/onboarding/key_shards_art.dart';
import 'package:zafe/src/core/theme/app_theme.dart';
import 'package:zafe/src/core/widgets/action_button.dart';
import 'package:zafe/src/core/widgets/app_button.dart';
import 'package:zafe/src/core/widgets/mobile/mobile_transaction_progress_screen.dart';
import 'package:zafe/src/features/proposals/proposal_parts.dart';
import 'package:zafe/src/features/proposals/sent_art.dart';
import 'package:zafe/src/providers/proposals_provider.dart';
import 'package:zafe/src/rust/api/proposals.dart' as rust;

const _me = 'a3f09c41d2e87b5566c0de19f4a2b7c8e1d0937a6b5c4d3e2f1a0b9c8d7e6f50';
const _maya =
    '5e17b2c9a4d86f3310ab77e2c4d9f1086b3a2c5d7e9f0a1b2c3d4e5f6a7b8c9d';
const _ravi = 'c84d1e0f2a3b4c5d6e7f8091a2b3c4d5e6f708192a3b4c5d6e7f8091a2be8d7';
const _members = [_me, _maya, _ravi];
const _names = {_maya: 'Maya', _ravi: 'Ravi'};
const _addr =
    'u1k3v8m2q9x7h5f4d6s8a0p2o4i6u8y0t2r4e6w8q0z9x7c5v3b1n2m4l6k8j0h2g4f6d8s0a';

const _frame = Duration(microseconds: 33333); // 30 fps

final _asked = rust.ProposalInfo(
  id: 'p1',
  author: _maya,
  isMine: false,
  payments: [
    rust.PaymentInfo(
      address: _addr,
      amountZat: BigInt.from(1250000000),
      memo: 'Audit milestone 2',
    ),
  ],
  totalZat: BigInt.from(1250000000),
  stage: rust.ProposalStage.open,
  approvals: const [_maya],
  rejections: const [],
  myVote: rust.MyVote.none,
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

// One moment as a card for the page's loops: the app's own widgets on the app's surface
// colour, nothing around them.
Widget _card({
  required Widget child,
  EdgeInsets padding = const EdgeInsets.fromLTRB(20, 28, 20, 20),
}) => Builder(
  builder: (context) => SizedBox(
    width: 360,
    child: ClipRRect(
      borderRadius: BorderRadius.circular(28),
      child: ColoredBox(
        color: context.colors.background.window,
        child: Padding(padding: padding, child: child),
      ),
    ),
  ),
);

// The request: the app's request view and Approve button.
Widget _requestCard() => _card(
  child: Column(
    mainAxisSize: MainAxisSize.min,
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      PaymentRequestView(
        proposal: _asked,
        members: _members,
        me: _me,
        names: _names,
      ),
      const SizedBox(height: 20),
      ActionButton(
        label: 'Approve',
        icon: AppIcons.check,
        doneLabel: 'Approved',
        onPressed: () =>
            Future<void>.delayed(const Duration(milliseconds: 900)),
      ),
    ],
  ),
);

// Sending: the progress badge (spinner, then the check and the coin fountain) and the
// words, as the progress screen shows them. The words keep one height so nothing moves.
Widget _sendCard(bool sent) => Builder(
  builder: (context) {
    final colors = context.colors;
    return _card(
      padding: const EdgeInsets.fromLTRB(24, 44, 24, 40),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          MobileTransactionProgressBadge(
            phase: sent
                ? MobileTransactionProgressPhase.succeeded
                : MobileTransactionProgressPhase.inProgress,
            successEffect: const CoinFountain(),
          ),
          const SizedBox(height: AppSpacing.xl),
          SizedBox(
            height: 124,
            // As the progress screen: the old words leave in the first half, the new ones
            // arrive in the second, rising a little; never both at once.
            child: AnimatedSwitcher(
              duration: const Duration(milliseconds: 420),
              layoutBuilder: (current, previous) => Stack(
                alignment: Alignment.topCenter,
                children: [...previous, ?current],
              ),
              transitionBuilder: (child, animation) {
                final a = CurvedAnimation(
                  parent: animation,
                  curve: const Interval(0.5, 1, curve: Curves.easeOutCubic),
                );
                return FadeTransition(
                  opacity: a,
                  child: SlideTransition(
                    position: Tween(
                      begin: const Offset(0, 0.12),
                      end: Offset.zero,
                    ).animate(a),
                    child: child,
                  ),
                );
              },
              child: Column(
                key: ValueKey(sent),
                children: [
                  Text(
                    sent ? 'Sent!' : 'Sending...',
                    textAlign: TextAlign.center,
                    style: AppTypography.displayLarge.copyWith(
                      color: colors.text.accent,
                    ),
                  ),
                  const SizedBox(height: AppSpacing.s),
                  ConstrainedBox(
                    constraints: const BoxConstraints(maxWidth: 260),
                    child: Text(
                      sent
                          ? 'It confirms on-chain shortly.'
                          : 'Building the transaction and submitting it...',
                      textAlign: TextAlign.center,
                      style: AppTypography.bodyMediumStrong.copyWith(
                        color: colors.text.primary,
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  },
);

// The keys being made: the key shards art over the status, as the setup screen shows them.
Widget _keysCard(bool ready) => Builder(
  builder: (context) {
    final colors = context.colors;
    return _card(
      padding: EdgeInsets.zero,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          // The art fills its box and fades into the card at the bottom.
          SizedBox(
            height: 240,
            child: ClipRect(
              child: ShaderMask(
                blendMode: BlendMode.dstIn,
                shaderCallback: (rect) => const LinearGradient(
                  begin: Alignment.topCenter,
                  end: Alignment.bottomCenter,
                  colors: [Colors.black, Colors.black, Colors.transparent],
                  stops: [0, 0.7, 1],
                ).createShader(rect),
                child: KeyShardsArt(assembled: ready),
              ),
            ),
          ),
          Padding(
            padding: const EdgeInsets.fromLTRB(24, 4, 24, 28),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                AppIcon(
                  ready ? AppIcons.check : AppIcons.loader,
                  size: 24,
                  color: ready ? colors.icon.value : colors.icon.accent,
                ),
                const SizedBox(height: AppSpacing.sm),
                Text(
                  ready ? 'Vault keys ready' : 'Creating your vault keys...',
                  style: TextStyle(
                    fontFamily: 'Space Grotesk',
                    fontSize: 24,
                    height: 28 / 24,
                    color: colors.text.accent,
                  ),
                ),
                const SizedBox(height: AppSpacing.xs),
                SizedBox(
                  height: 48,
                  child: Text(
                    ready
                        ? 'Your share is on this phone.'
                        : 'You can close Zafe. It tells you when the vault is ready.',
                    style: AppTypography.bodyMedium.copyWith(
                      color: colors.text.secondary,
                    ),
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  },
);

// The vault as Home shows it: the balance, the two actions, and Maya's request waiting.
Widget _vaultCard() => _card(
  padding: const EdgeInsets.all(16),
  child: Column(
    mainAxisSize: MainAxisSize.min,
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      BalanceCard(
        totalZat: BigInt.from(4237485000),
        fiatText: null,
        hidden: false,
        onToggle: () {},
        threshold: 2,
        members: 3,
      ),
      const SizedBox(height: 8),
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
          const SizedBox(width: 8),
          Expanded(
            child: AppButton(
              expand: true,
              variant: AppButtonVariant.secondary,
              onPressed: () {},
              leading: const AppIcon(AppIcons.arrowDownCircle, size: 20),
              child: const Text('Receive'),
            ),
          ),
        ],
      ),
      const SizedBox(height: 16),
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
    ],
  ),
);

void main() {
  testWidgets('render the site motion clips', (tester) async {
    await tester.runAsync(_loadFonts);
    final root =
        Platform.environment['SCREEN_PREVIEW_OUT'] ?? 'build/screen_preview';
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = const Size(390, 844);
    addTearDown(tester.view.reset);

    late Directory dir;
    var n = 0;
    Future<void> clip(String name) async {
      dir = Directory('$root/motion_$name')..createSync(recursive: true);
      for (final f in dir.listSync()) {
        f.deleteSync();
      }
      n = 0;
    }

    Future<void> settle() async {
      for (var i = 0; i < 12; i++) {
        await tester.runAsync(
          () => Future<void>.delayed(const Duration(milliseconds: 50)),
        );
        await tester.pump(const Duration(milliseconds: 300));
      }
    }

    // Cards (transparent around them) at 3x, for the page's loops.
    final cardKey = GlobalKey();
    Future<void> mountCard(Widget card, {AppThemeData? theme}) =>
        tester.pumpWidget(
          ProviderScope(
            overrides: [
              proposalReviewProvider.overrideWith((ref, id) async => _review),
            ],
            child: MaterialApp(
              debugShowCheckedModeBanner: false,
              builder: (context, child) =>
                  AppTheme(data: theme ?? AppThemeData.light, child: child!),
              home: Material(
                type: MaterialType.transparency,
                child: Center(
                  child: RepaintBoundary(key: cardKey, child: card),
                ),
              ),
            ),
          ),
        );
    Future<void> cardShot() => tester.runAsync(() async {
      final b =
          cardKey.currentContext!.findRenderObject()! as RenderRepaintBoundary;
      final image = await b.toImage(pixelRatio: 3);
      final bytes = await image.toByteData(format: ui.ImageByteFormat.png);
      n++;
      File(
        '${dir.path}/f_${n.toString().padLeft(4, '0')}.png',
      ).writeAsBytesSync(bytes!.buffer.asUint8List());
    });
    Future<void> cardRoll(double seconds) async {
      for (var i = 0; i < (seconds * 30).round(); i++) {
        await tester.pump(_frame);
        await cardShot();
      }
    }

    // The request: Approve through "Approved".
    await clip('request_card');
    await mountCard(_requestCard());
    await settle();
    await cardRoll(1.0);
    final approve = await tester.startGesture(
      tester.getCenter(find.text('Approve')),
    );
    await cardRoll(0.2);
    await approve.up();
    await cardRoll(2.0);

    // Sending through "Sent!" with the coins (the progress screen's timing: Sent at frame 47).
    await clip('send_card');
    await mountCard(_sendCard(false));
    for (var i = 0; i < 8; i++) {
      await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 150)),
      );
      await tester.pump(const Duration(milliseconds: 16));
    }
    await cardRoll(1.4);
    await mountCard(_sendCard(true));
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 200)),
    );
    await cardRoll(3.0);

    // The keys: shards drift, then close into the key ("ready" at frame 62).
    await clip('keys_card');
    await mountCard(_keysCard(false));
    await settle();
    await cardRoll(2.0);
    await mountCard(_keysCard(true));
    await cardRoll(3.0);

    // The vault with Maya's request waiting, dark (one still).
    await clip('vault_card_dark');
    await mountCard(_vaultCard(), theme: AppThemeData.dark);
    await settle();
    await cardShot();

    await tester.pumpWidget(const SizedBox());
    await tester.pump(const Duration(seconds: 5));
  });
}
