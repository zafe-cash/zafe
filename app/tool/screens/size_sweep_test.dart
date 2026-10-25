// Renders screens and sheets on small to large phones, tablets, foldables (inner and
// cover screens, a hinge, a half-opened fold), split-screen and landscape windows, at
// larger system text sizes, laid out as the app does (pages in `AppColumn`). Fails on any
// layout overflow (Flutter reports it as an error), a button that can't be scrolled into
// view, text or a button across a hinge, or a button on the hero art. Writes a few renders
// per size to look at (and every failing one). Not part of `flutter test`; run it
// explicitly (from app/):
//
//   flutter test tool/screens/size_sweep_test.dart
//
// Output (SCREEN_PREVIEW_OUT, default build/screen_preview/): sweep_<case>_<size>_x<scale>.png.
import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/config/endpoints.dart';
import 'package:zafe/src/core/layout/app_column.dart';
import 'package:zafe/src/core/layout/mobile/app_mobile_shell.dart';
import 'package:zafe/src/core/layout/mobile/app_mobile_tab_bar.dart';
import 'package:zafe/src/core/security/app_lock_gate.dart' show LockScreen;
import 'package:zafe/src/core/widgets/app_button.dart';
import 'package:zafe/src/core/widgets/app_icon.dart';
import 'package:zafe/src/core/widgets/mobile/mobile_transaction_progress_screen.dart';
import 'package:zafe/src/features/onboarding/onboarding_art.dart';
import 'package:zafe/src/features/onboarding/welcome_screen.dart';
import 'package:zafe/src/features/proposals/sent_art.dart';
import 'package:zafe/src/providers/endpoints_provider.dart';
import 'package:zafe/src/providers/vault_provider.dart';
import 'package:zafe/src/core/layout/mobile/zafe_screen.dart';
import 'package:zafe/src/core/theme/app_theme.dart';
import 'package:zafe/src/core/widgets/confirm_option.dart';
import 'package:zafe/src/core/widgets/mobile/mobile_list_row.dart';
import 'package:zafe/src/core/widgets/mobile/zafe_detail.dart';
import 'package:zafe/src/core/widgets/mobile_text_field.dart';
import 'package:zafe/src/features/home/needs_you.dart' show RequestCard;
import 'package:zafe/src/features/home/home_screen.dart'
    show BalanceCard, NoticeCard;
import 'package:zafe/src/features/backup/export_screen.dart';
import 'package:zafe/src/features/onboarding/setup_screen.dart';
import 'package:zafe/src/features/received/received_row.dart';
import 'package:zafe/src/features/recover/recover_screen.dart';
import 'package:zafe/src/features/signers/replace_signer_screen.dart';
import 'package:zafe/src/features/signers/signers_screen.dart';
import 'package:zafe/src/notifications/notification_primer.dart';
import 'package:zafe/src/rust/api/received.dart' show ReceivedInfo;
import 'package:zafe/src/rust/api/repair.dart' show CosignerWords;

import 'relay_move_render_test.dart' show relayMoveCases, relayMoveScreens;
import 'relay_render_test.dart' show relayCases;

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

const _key = '97c9ce4b0d2f6a1e8c3b5d7f9a1c3e5f7b9d1f3a5c7e9b1d3f5a7c9e1b3d123e';
const _key2 =
    '24bafbab373b21c99c7ea2e902f0034ec45a8c27a87f1880090043a3f458ec55';
const _code =
    'zafe-recover-v1:2c9f04d1a7e8b3c56d0f1e2a3b4c5d6e7f8091a2b3c4d5e6f708192a3b4c5d6e7f'
    '8a9b0c1d2e3f405162738495a6b7c8d9e0f1a2b3c4d5e6f708192a3b4c5d6e7f8091';

/// (name, page title or null for a sheet, content)
final _cases = <(String, String?, Widget)>[
  (
    'setup_words',
    'Grants',
    const Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SetupSafetyWordsCard(words: 'regular fancy about business', waiting: 0),
        SizedBox(height: 12),
        SetupSafetyWordsCard(words: null, waiting: 2),
        SizedBox(height: 12),
        ConfirmOption(
          selected: true,
          label: 'I compared these words with every member',
          onTap: _noop,
        ),
      ],
    ),
  ),
  ('setup_waiting', 'Grants', Text(waitingText([_key, _key2]))),
  (
    'recover',
    'Recover your seat',
    const RecoverBody(
      code: _code,
      checks: [
        CosignerWords(cosignerKeyHex: _key, words: 'guilt afford pear gasp'),
        CosignerWords(cosignerKeyHex: _key2, words: 'say green fiction stick'),
      ],
    ),
  ),
  (
    'replace',
    'Replace lost phone',
    const Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SeatMoveCard(
          who: 'Alexandra',
          approvals: 1,
          needed: 2,
          onApprove: _noop,
        ),
        SizedBox(height: 12),
        SafetyWordsCard(words: null, who: 'Alexandra'),
        SizedBox(height: 12),
        SafetyWordsCard(
          words: 'ancient cinnamon zebra legend',
          who: 'Alexandra',
          myKeyHex: _key,
          confirmed: true,
        ),
      ],
    ),
  ),
  (
    'home_notices',
    'Home',
    const Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        NoticeCard(
          title: 'Back up this vault',
          body: 'It holds money, and your key share lives only on this phone',
          onTap: _noop,
        ),
        SizedBox(height: 12),
        NoticeCard(
          title: 'Notifications are off',
          body: 'You won\'t hear when a payment needs your approval',
          onTap: _noop,
        ),
      ],
    ),
  ),
  (
    'settings_rows',
    'Settings',
    const Column(
      children: [
        MobileListRow(
          label: 'Require unlock to approve',
          value: 'On',
          showChevron: true,
        ),
        MobileListRow(
          label: 'Lock app',
          value: 'After 1 minute',
          showChevron: true,
        ),
        MobileListRow(label: 'Approval rule', value: '2 of 3 signers'),
      ],
    ),
  ),
  (
    'payment',
    'Review payment',
    const Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        PaymentCard(
          amountText: '2.00',
          address: 'uregtest10jal7369n7asxnqyrgq84c4cyl8s',
          onFullAddress: _noop,
        ),
        SizedBox(height: 12),
        DetailRow(label: 'Network fee', value: '0.0001 TAZ', onHelp: _noop),
        DetailRow(
          label: 'Checked on this phone',
          value: 'Matches',
          onHelp: _noop,
        ),
        DetailRow(label: 'Approvals needed', value: '2 of 3 signers'),
        TxIdRow(
          txid:
              '10616fc1aa0000000000000000000000000000000000000000000000c84a8447',
        ),
      ],
    ),
  ),
  (
    'fields',
    'Back up vault',
    Column(
      children: [
        MobileTextField(
          controller: TextEditingController(),
          focusNode: FocusNode(),
          hintText: 'Passphrase',
          trailing: const Text('Suggest'),
        ),
        const SizedBox(height: 12),
        MobileTextField(
          controller: TextEditingController(text: 'uregtest10jal7369n7asx'),
          focusNode: FocusNode(),
          leading: const Icon(Icons.send, size: 20),
          trailing: const Text('Clear'),
        ),
      ],
    ),
  ),
  (
    'vault_card',
    'Home',
    BalanceCard(
      totalZat: BigInt.from(2899990000),
      fiatText: null,
      confirmingText: '3.9999 TAZ confirming',
      hidden: false,
      onToggle: _noop,
      threshold: 2,
      members: 3,
    ),
  ),
  (
    'activity_row',
    'Activity',
    ReceivedRow(
      received: ReceivedInfo(
        txid: 'e88aa577',
        amountZat: BigInt.from(500000000),
        minedHeight: 114,
        blockTimeSecs: 1296822960,
        confirmations: 13,
        memo: '',
        isCoinbase: false,
      ),
      onTap: _noop,
    ),
  ),
  (
    'request_cards',
    'Home',
    const Column(
      children: [
        RequestCard(
          tileKey: _key,
          who: 'd05aee...bd79',
          amount: '0.8',
          detail: 'Printer ink',
          action: 'Approve',
          onTap: _noop,
          onDecline: _noop,
        ),
        SizedBox(height: 12),
        RequestCard(
          tileKey: _key2,
          who: 'Treasurer Alice',
          amount: '12,450.25',
          detail: 'Q4 grants round, second tranche',
          action: 'Send now',
          onTap: _noop,
        ),
      ],
    ),
  ),
  ('sheet_primer', null, const NotificationPrimerSheet()),
  // The relay picker, the vault's relay in Settings, Create > Relay, the join line,
  // the seat-recovery vault link and Home during an outage.
  ...relayCases(),
  // Moving the vault to another relay: the vault relay sheet, outage sheet, Home cards,
  // Activity row, picker.
  ...relayMoveCases(),
  (
    'backup_words',
    null,
    // In ZafeScreen's ListView, as in the app: tall with large text, so it scrolls.
    ListView(
      padding: const EdgeInsets.all(16),
      children: const [
        BackupWordGrid(
          words: [
            'abandon',
            'category',
            'disorder',
            'electric',
            'february',
            'general', //
            'happy', 'industry', 'journey', 'kangaroo', 'language', 'mushroom',
          ],
        ),
      ],
    ),
  ),
];

/// Whole screens, laid out as the router does: in an `AppColumn`, except Welcome, which
/// handles wide windows itself. `art`: the screen has hero art no button may sit on.
final _screens = <(String, Widget, {bool column, bool art})>[
  ('welcome', const WelcomeScreen(), column: false, art: true),
  (
    'creating_keys',
    const CreatingKeysScreen(
      waiting: 'Waiting for 24bafb...ec55 and dccacc...b50b.',
    ),
    column: true,
    art: true,
  ),
  (
    'sending',
    MobileTransactionProgressScreen(
      phase: MobileTransactionProgressPhase.succeeded,
      title: 'Sent!',
      body: 'It will confirm on-chain shortly. Track it in Activity.',
      canPop: true,
      bodyMaxWidth: 260,
      background: const SentArt(sent: true),
      primaryActionLabel: 'Done',
      onPrimaryAction: _noop,
    ),
    column: true,
    art: false,
  ),
  // Over the router, in its own column.
  ('lock', const LockScreen(onUnlock: _noop), column: false, art: false),
  ('tabs', const _Tabs(), column: true, art: false),
  // The relay move pages with their pinned buttons.
  for (final (name, screen) in relayMoveScreens())
    ('move_$name', screen, column: true, art: false),
  // A pushed page with its pinned actions (they scroll with it in a short window).
  (
    'page_actions',
    const ZafeScreen(
      title: 'Review payment',
      showBack: false,
      bottom: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          AppButton(expand: true, onPressed: _noop, child: Text('Approve')),
          SizedBox(height: 12),
          AppButton(
            expand: true,
            variant: AppButtonVariant.ghost,
            onPressed: _noop,
            child: Text('Decline'),
          ),
        ],
      ),
      children: [
        PaymentCard(
          amountText: '2.00',
          address: 'uregtest10jal7369n7asxnqyrgq84c4cyl8s',
          onFullAddress: _noop,
        ),
        SizedBox(height: 12),
        DetailRow(label: 'Network fee', value: '0.0001 TAZ', onHelp: _noop),
        DetailRow(label: 'Approvals needed', value: '2 of 3 signers'),
      ],
    ),
    column: true,
    art: false,
  ),
];

/// Home under the floating tab bar, as `VaultShell` shows it.
class _Tabs extends StatelessWidget {
  const _Tabs();

  @override
  Widget build(BuildContext context) => AppMobileShell(
    body: SafeArea(
      bottom: false,
      child: ListView(
        padding: const EdgeInsets.fromLTRB(16, 16, 16, 64 + 48),
        children: [
          BalanceCard(
            totalZat: BigInt.from(2899990000),
            fiatText: null,
            hidden: false,
            onToggle: _noop,
            threshold: 2,
            members: 3,
          ),
          const SizedBox(height: 12),
          AppButton(
            expand: true,
            onPressed: _noop,
            leading: const AppIcon(AppIcons.plane, size: 20),
            child: const Text('New payment'),
          ),
          const SizedBox(height: 12),
          const RequestCard(
            tileKey: _key,
            who: 'Treasurer Alice',
            amount: '12,450.25',
            detail: 'Q4 grants round, second tranche',
            action: 'Approve or decline',
            onTap: _noop,
          ),
        ],
      ),
    ),
    tabBar: AppMobileTabBar(
      items: const [
        AppMobileTabItem(iconName: AppIcons.home, label: 'Home'),
        AppMobileTabItem(iconName: AppIcons.history, label: 'Activity'),
        AppMobileTabItem(iconName: AppIcons.users, label: 'Signers'),
        AppMobileTabItem(iconName: AppIcons.cog, label: 'Settings'),
      ],
      currentIndex: 0,
      onSelect: (_) {},
    ),
  );
}

/// A hinge (Surface Duo: two 540 wide screens, 34 between them) and a half-opened fold.
const _duoHinge = ui.DisplayFeature(
  bounds: Rect.fromLTWH(540, 0, 34, 720),
  type: ui.DisplayFeatureType.hinge,
  state: ui.DisplayFeatureState.unknown,
);
const _bookFold = ui.DisplayFeature(
  bounds: Rect.fromLTWH(420, 0, 0, 900),
  type: ui.DisplayFeatureType.fold,
  state: ui.DisplayFeatureState.postureHalfOpened,
);
const _tabletopFold = ui.DisplayFeature(
  bounds: Rect.fromLTWH(0, 450, 840, 0),
  type: ui.DisplayFeatureType.fold,
  state: ui.DisplayFeatureState.postureHalfOpened,
);

/// Logical sizes (width x height), the text scales tried on each, and display features.
/// Device sizes are approximate classes, not exact models.
const _sizes = <(String, Size, List<double>, List<ui.DisplayFeature>)>[
  // Phones (and a foldable's cover screen / a flip phone's main screen, portrait).
  ('320x568', Size(320, 568), [1.0, 1.3, 2.0], []),
  ('360x640', Size(360, 640), [1.0, 1.3, 2.0], []),
  ('390x844', Size(390, 844), [1.0, 1.3], []),
  ('412x915', Size(412, 915), [1.0, 1.3], []),
  ('fold_cover_344x882', Size(344, 882), [1.0, 2.0], []),
  // Flip phones' cover screens: tiny, nearly square.
  ('flip_cover_360x380', Size(360, 380), [1.0, 1.3, 2.0], []),
  ('flip_cover_300x330', Size(300, 330), [1.0, 1.3], []),
  // Tablets, both ways, and split-screen halves.
  ('tablet_800x1280', Size(800, 1280), [1.0, 2.0], []),
  ('tablet_1280x800', Size(1280, 800), [1.0, 2.0], []),
  ('split_640x800', Size(640, 800), [1.0, 2.0], []),
  ('split_800x620', Size(800, 620), [1.0, 2.0], []),
  // Book-style foldables' inner screens, both ways, and half of one in landscape.
  ('fold_inner_840x900', Size(840, 900), [1.0, 2.0], []),
  ('fold_inner_626x890', Size(626, 890), [1.0, 2.0], []),
  ('fold_inner_890x626', Size(890, 626), [1.0, 2.0], []),
  ('fold_split_445x626', Size(445, 626), [1.0, 2.0], []),
  // A short, wide outer screen (rotates only if the display is large).
  ('outer_466x678', Size(466, 678), [1.0, 1.3, 2.0], []),
  ('outer_678x466', Size(678, 466), [1.0, 2.0], []),
  // Landscape phone-sized windows (split screen, freeform, a large-screen phone).
  ('landscape_640x360', Size(640, 360), [1.0, 2.0], []),
  ('landscape_915x412', Size(915, 412), [1.0, 1.3], []),
  // Hinged and half-opened.
  ('duo_hinge_1114x720', Size(1114, 720), [1.0, 2.0], [_duoHinge]),
  ('fold_book_840x900', Size(840, 900), [1.0], [_bookFold]),
  ('fold_tabletop_840x900', Size(840, 900), [1.0], [_tabletopFold]),
];

class _Endpoints extends EndpointsNotifier {
  @override
  ZafeEndpoints build() => const ZafeEndpoints(
    relayUrl: 'https://relay.example.org',
    lightwalletdUrl: 'https://testnet.zec.rocks:443',
  );
}

class _Vault extends VaultNotifier {
  @override
  VaultState build() => const VaultState();
}

void main() {
  testWidgets('screens fit phones, tablets, foldables and large text', (
    tester,
  ) async {
    await tester.runAsync(_loadFonts);
    final out = Directory(
      Platform.environment['SCREEN_PREVIEW_OUT'] ?? 'build/screen_preview',
    )..createSync(recursive: true);
    tester.view.devicePixelRatio = 2;
    addTearDown(tester.view.reset);
    final problems = <String>[];

    for (final (sizeName, size, scales, features) in _sizes) {
      tester.view.physicalSize = size * 2;
      // Logical pixels (the engine divides by the pixel ratio before the view has them).
      tester.view.displayFeatures = features;
      final phone = !sizeName.contains('_');
      for (final scale in scales) {
        final all = <(String, Widget, bool)>[
          for (final (name, title, body) in _cases)
            (
              name,
              AppColumn(
                child: title == null
                    ? Scaffold(
                        backgroundColor: Colors.black54,
                        // As the sheet sits on a phone: the screen's height and no
                        // scrolling but its own.
                        body: Align(
                          alignment: Alignment.bottomCenter,
                          child: body,
                        ),
                      )
                    : ZafeScreen(
                        title: title,
                        showBack: false,
                        children: [body],
                      ),
              ),
              false,
            ),
          for (final (name, screen, :column, :art) in _screens)
            (name, column ? AppColumn(child: screen) : screen, art),
        ];
        for (final (name, page, art) in all) {
          final boundary = GlobalKey();
          final errors = <FlutterErrorDetails>[];
          final previous = FlutterError.onError;
          FlutterError.onError = errors.add;
          await tester.pumpWidget(
            ProviderScope(
              key: ValueKey('$sizeName $scale $name'),
              overrides: [
                endpointsProvider.overrideWith(_Endpoints.new),
                vaultProvider.overrideWith(_Vault.new),
              ],
              child: RepaintBoundary(
                key: boundary,
                child: MaterialApp(
                  debugShowCheckedModeBanner: false,
                  builder: (context, child) => MediaQuery(
                    data: MediaQuery.of(
                      context,
                    ).copyWith(textScaler: TextScaler.linear(scale)),
                    child: AppTheme(data: AppThemeData.light, child: child!),
                  ),
                  home: page,
                ),
              ),
            ),
          );
          await tester.pump(const Duration(milliseconds: 100));
          if (art) {
            // The art's SVG layers load asynchronously.
            await tester.runAsync(
              () => Future<void>.delayed(const Duration(milliseconds: 50)),
            );
            await tester.pump(const Duration(milliseconds: 100));
          }
          final where = '$name $sizeName x$scale';
          final checks = <String>[];
          final window = Offset.zero & size;
          // Every button can be scrolled fully into view, never sits on the hero art
          // (scrolled to the top and to the end), and nothing crosses a hinge.
          final seen = <Element>{};
          for (final end in [false, true]) {
            if (end) {
              for (final s in tester.stateList<ScrollableState>(
                find.byType(Scrollable),
              )) {
                s.position.jumpTo(s.position.maxScrollExtent);
              }
              await tester.pump();
            }
            final heroes = find.byKey(kHeroArtKey);
            final hero = heroes.evaluate().isEmpty
                ? null
                : tester.getRect(heroes.first);
            for (final button in find.byType(AppButton).evaluate()) {
              final r = tester.getRect(find.byWidget(button.widget)).deflate(1);
              if (window.contains(r.topLeft) &&
                  window.contains(r.bottomRight)) {
                seen.add(button);
              }
              // The art fades into the window colour from 74% of its height.
              if (art &&
                  hero != null &&
                  r.overlaps(
                    Rect.fromLTRB(
                      hero.left,
                      hero.top,
                      hero.right,
                      hero.top + hero.height * 0.74,
                    ),
                  ) &&
                  r.overlaps(window)) {
                final label = find
                    .descendant(
                      of: find.byWidget(button.widget),
                      matching: find.byType(Text),
                    )
                    .evaluate()
                    .map((e) => (e.widget as Text).data)
                    .join();
                checks.add('button "$label" on the art (end: $end)');
              }
            }
            for (final f in features) {
              final hinge = f.bounds.inflate(0.5);
              for (final p in find.byType(RichText).evaluate()) {
                final box = p.renderObject! as RenderBox;
                if (!box.hasSize || box.size.isEmpty) continue;
                var r = box.localToGlobal(Offset.zero) & box.size;
                // What a scroll view clips off isn't on screen.
                for (RenderObject? o = box.parent; o != null; o = o.parent) {
                  if (o is RenderBox && o is RenderAbstractViewport) {
                    r = r.intersect(o.localToGlobal(Offset.zero) & o.size);
                    break;
                  }
                }
                if (r.width > 0 &&
                    r.height > 0 &&
                    r.overlaps(hinge) &&
                    r.overlaps(window)) {
                  checks.add(
                    'text "${(p.widget as RichText).text.toPlainText()}" '
                    'across the ${f.type.name}',
                  );
                }
              }
            }
          }
          for (final button in find.byType(AppButton).evaluate().toList()) {
            if (seen.contains(button)) continue;
            final which = find.byWidget(button.widget);
            await tester.ensureVisible(which);
            await tester.pump();
            final r = tester.getRect(which).deflate(1);
            if (!window.contains(r.topLeft) ||
                !window.contains(r.bottomRight)) {
              checks.add('a button can\'t be scrolled into view');
            }
          }
          FlutterError.onError = previous;
          for (final e in errors) {
            problems.add('$where: ${e.exceptionAsString()}');
          }
          for (final c in checks.toSet()) {
            problems.add('$where: $c');
          }
          final keep =
              (sizeName == '320x568' && scale == 2.0) ||
              (sizeName == '360x640' && scale == 1.3) ||
              (!phone && scale == 1.0);
          if (keep || errors.isNotEmpty || checks.isNotEmpty) {
            // Back to the top for the picture.
            for (final s in tester.stateList<ScrollableState>(
              find.byType(Scrollable),
            )) {
              s.position.jumpTo(0);
            }
            await tester.pump();
            final target =
                boundary.currentContext!.findRenderObject()!
                    as RenderRepaintBoundary;
            final bytes = await tester.runAsync(() async {
              final image = await target.toImage(
                pixelRatio: size.longestSide > 1000 ? 1 : 1.5,
              );
              final data = await image.toByteData(
                format: ui.ImageByteFormat.png,
              );
              return data!.buffer.asUint8List();
            });
            File(
              '${out.path}/sweep_${name}_${sizeName}_x$scale.png',
            ).writeAsBytesSync(bytes!);
          }
        }
      }
    }
    await tester.pumpWidget(const SizedBox());
    // The whole list (the test's failure message elides long ones).
    File(
      '${out.path}/sweep_problems.txt',
    ).writeAsStringSync(problems.join('\n'));
    expect(problems, isEmpty, reason: problems.join('\n'));
  });
}

void _noop() {}
