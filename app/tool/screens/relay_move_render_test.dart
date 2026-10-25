// Renders the relay-move screens without a device (docs/relay-move.md §4.2-4.5): Move
// (picking the relay, a member who must update), the "Moving to" list, the follow screen
// from a notice, from a link and after the switch, the mover's done page, the Home cards,
// the vault relay sheet's Move button, the outage sheet's Move button, the payment
// screen's "Payments resume" line and the Activity row, in both themes. Not part of
// `flutter test`; run it explicitly (from app/):
//
//   flutter test tool/screens/relay_move_render_test.dart
//
// Output (SCREEN_PREVIEW_OUT, default build/screen_preview/): move_<state>_<theme>.png.
import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/config/relay_presets.dart';
import 'package:zafe/src/core/layout/mobile/app_mobile_sheet.dart';
import 'package:zafe/src/core/layout/mobile/zafe_screen.dart';
import 'package:zafe/src/core/theme/app_theme.dart';
import 'package:zafe/src/core/widgets/action_button.dart';
import 'package:zafe/src/core/widgets/app_button.dart';
import 'package:zafe/src/core/widgets/mobile/mobile_surface_card.dart';
import 'package:zafe/src/core/widgets/mobile/zafe_detail.dart';
import 'package:zafe/src/features/home/home_screen.dart' show NoticeCard;
import 'package:zafe/src/features/proposals/activity_feed.dart';
import 'package:zafe/src/features/relay_move/relay_move_screen.dart';
import 'package:zafe/src/features/settings/relay_picker.dart';
import 'package:zafe/src/features/settings/vault_relay_sheet.dart';
import 'package:zafe/src/rust/api/endpoints.dart';
import 'package:zafe/src/rust/api/proposals.dart' show RelayMoveRecord;

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

const _host = 'relay.dao.org';

Future<void> _done() async {}

Widget _followButtons() => Column(
  mainAxisSize: MainAxisSize.min,
  crossAxisAlignment: CrossAxisAlignment.stretch,
  children: [
    const ActionButton(label: 'Move', onPressed: _done),
    const SizedBox(height: AppSpacing.xs),
    AppButton(
      expand: true,
      variant: AppButtonVariant.ghost,
      onPressed: _noop,
      child: const Text('Not now'),
    ),
  ],
);

/// Whole pages with their pinned buttons: (name, page).
List<(String, Widget)> relayMoveScreens() => [
  (
    'start',
    const ZafeScreen(
      title: kRelayMoveTitle,
      bottom: ActionButton(label: 'Move', onPressed: _done),
      children: [
        MoveHeading('Move Grants to another relay'),
        MoveStartBody(host: _host, onPick: _noop),
      ],
    ),
  ),
  (
    'start_blocked',
    ZafeScreen(
      title: kRelayMoveTitle,
      bottom: const ActionButton(
        label: 'Move',
        enabled: false,
        onPressed: _done,
      ),
      children: [
        const MoveHeading('Move Grants to another relay'),
        MoveStartBody(
          host: _host,
          onPick: _noop,
          mustUpdate: mustUpdateText(['Bob', 'Carol']),
        ),
      ],
    ),
  ),
  (
    'progress',
    const ZafeScreen(
      title: kRelayMoveTitle,
      children: [
        MoveHeading('Moving to $_host'),
        MoveProgressBody(
          members: [
            ('You', true),
            ('Bob', false),
            ('Carol', false),
            ('Dan', false),
          ],
          needed: 3,
          total: 4,
          onShare: _done,
          onCancel: _done,
        ),
      ],
    ),
  ),
  (
    'progress_relay_down',
    ZafeScreen(
      title: kRelayMoveTitle,
      children: [
        const MoveHeading('Moving to $_host'),
        MoveProgressBody(
          members: const [
            ('You', true),
            ('Alexandra Montgomery-Smith', true),
            ('Carol', false),
            ('Dan', false),
          ],
          needed: 3,
          total: 4,
          resume: paymentsResumeText(1, _host),
          onShare: _done,
          onCancel: _done,
        ),
      ],
    ),
  ),
  (
    'follow_notice',
    ZafeScreen(
      title: kRelayMoveTitle,
      bottom: _followButtons(),
      children: const [
        FollowMoveBody(
          vaultName: 'Grants',
          host: _host,
          mover: 'Alice',
          arrived: 1,
          members: 4,
          noPush: true,
          seesIp: true,
        ),
      ],
    ),
  ),
  (
    'follow_link',
    ZafeScreen(
      title: kRelayMoveTitle,
      bottom: _followButtons(),
      children: const [
        FollowMoveBody(vaultName: 'Grants', host: _host, noPush: true),
      ],
    ),
  ),
  (
    'follow_ours',
    ZafeScreen(
      title: kRelayMoveTitle,
      bottom: _followButtons(),
      children: const [
        FollowMoveBody(
          vaultName: 'Grants',
          host: 'relay.zafe.cash',
          mover: 'Alice',
          arrived: 2,
          members: 3,
        ),
      ],
    ),
  ),
  (
    'follow_moved',
    ZafeScreen(
      title: kRelayMoveTitle,
      bottom: _followButtons(),
      children: const [
        FollowMoveBody(
          vaultName: 'Grants',
          host: _host,
          moved: true,
          noPush: true,
        ),
      ],
    ),
  ),
  (
    'moved_done',
    ZafeScreen(
      title: kRelayMoveTitle,
      bottom: AppButton(
        expand: true,
        onPressed: _noop,
        child: const Text('Done'),
      ),
      children: const [MovedBody(vaultName: 'Grants', host: _host)],
    ),
  ),
  (
    'payment_resume',
    ZafeScreen(
      title: 'Send',
      children: [
        Builder(
          builder: (context) => Text(
            paymentsResumeText(1, _host),
            style: AppTypography.bodyMedium.copyWith(
              color: context.colors.text.warning,
            ),
          ),
        ),
      ],
    ),
  ),
];

/// Sheets and cards: (name, page title or null for a sheet, content).
List<(String, String?, Widget)> relayMoveCases() => [
  (
    'picker',
    null,
    MobileModalScaffold(
      title: 'Move to',
      onClose: _noop,
      child: RelayPickerBody(
        presets: kMainnetRelayPresets,
        selectedUrl: 'https://$_host',
        result: const RelayCheckResult.ok(
          RelayCheck(
            tooOld: false,
            relayApiVersion: 2,
            push: false,
            createTokenRequired: false,
            maxMembers: 64,
            requestsPerMinute: 300,
            deliveryRetentionDays: 30,
            idleDays: 365,
          ),
        ),
        otherOpen: true,
        other: TextEditingController(text: _host),
        otherFocus: FocusNode(),
        onSelect: (_) {},
        onOther: _noop,
        onConfirm: _noop,
        onRunYourOwn: _noop,
      ),
    ),
  ),
  (
    'vault_relay',
    null,
    MobileModalScaffold(
      title: vaultRelayTitle('Grants committee'),
      titleMaxLines: 2,
      onClose: _noop,
      child: VaultRelayBody(
        url: 'https://relay.zafe.cash',
        hasCreateToken: false,
        result: const RelayCheckResult.failed('Couldn\'t connect.'),
        token: TextEditingController(),
        tokenFocus: FocusNode(),
        onMove: _noop,
      ),
    ),
  ),
  (
    'vault_relay_moving',
    null,
    MobileModalScaffold(
      title: vaultRelayTitle('Grants'),
      titleMaxLines: 2,
      onClose: _noop,
      child: VaultRelayBody(
        url: 'https://relay.zafe.cash',
        hasCreateToken: false,
        result: const RelayCheckResult.failed('Couldn\'t connect.'),
        token: TextEditingController(),
        tokenFocus: FocusNode(),
        onMove: _noop,
        movingTo: _host,
      ),
    ),
  ),
  (
    'outage_sheet',
    null,
    Builder(
      builder: (context) => MobileModalScaffold(
        title: 'Can\'t reach the relay',
        titleMaxLines: 2,
        onClose: _noop,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text(
              'Payments can\'t be approved until the relay is back. Your funds '
              'are safe.',
              style: AppTypography.bodyMedium.copyWith(
                color: context.colors.text.secondary,
              ),
            ),
            const SizedBox(height: AppSpacing.sm),
            const DetailRow(label: 'Relay', value: 'relay.zafe.cash'),
            const DetailRow(label: 'Last synced', value: 'Today 09:14'),
            const SizedBox(height: AppSpacing.md),
            AppButton(
              expand: true,
              onPressed: _noop,
              child: const Text('Try again'),
            ),
            const SizedBox(height: AppSpacing.xs),
            AppButton(
              expand: true,
              variant: AppButtonVariant.secondary,
              onPressed: _noop,
              child: const Text('Move to another relay'),
            ),
          ],
        ),
      ),
    ),
  ),
  (
    'home_cards',
    'Home',
    const Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        NoticeCard(
          title: 'Move to $_host?',
          body: 'Alice is moving Grants. 1 of 4 have moved.',
          warning: false,
          onTap: _noop,
        ),
        SizedBox(height: AppSpacing.md),
        NoticeCard(
          title: 'Moved to $_host',
          body: 'Grants moved to $_host. Move to continue.',
          warning: false,
          onTap: _noop,
        ),
        SizedBox(height: AppSpacing.md),
        NoticeCard(
          title: 'Two moves found',
          body: 'Ask your group which relay to use.',
        ),
      ],
    ),
  ),
  (
    'activity_row',
    'Activity',
    MobileSurfaceCard(
      cornerRadius: AppRadii.large,
      child: ActivityRow(
        item: RelayMovedActivity(
          vaultName: 'Grants',
          record: RelayMoveRecord(
            relayUrl: 'https://$_host',
            host: _host,
            at: BigInt.from(1761400000),
            logIndex: BigInt.from(42),
          ),
        ),
      ),
    ),
  ),
];

void main() {
  testWidgets('render the relay move screens', (tester) async {
    await tester.runAsync(_loadFonts);
    final out = Directory(
      Platform.environment['SCREEN_PREVIEW_OUT'] ?? 'build/screen_preview',
    )..createSync(recursive: true);
    tester.view.devicePixelRatio = 3;
    tester.view.physicalSize = const Size(390 * 3, 844 * 3);
    addTearDown(tester.view.reset);

    final pages = <(String, Widget Function(BuildContext))>[
      for (final (name, screen) in relayMoveScreens()) (name, (_) => screen),
      for (final (name, title, body) in relayMoveCases())
        (
          name,
          (context) => title == null
              ? Scaffold(
                  backgroundColor: context.colors.background.window,
                  body: Align(
                    alignment: Alignment.bottomCenter,
                    child: Padding(
                      padding: const EdgeInsets.all(16),
                      child: DecoratedBox(
                        decoration: BoxDecoration(
                          color: context.colors.background.base,
                          borderRadius: BorderRadius.circular(AppRadii.xLarge),
                        ),
                        child: body,
                      ),
                    ),
                  ),
                )
              : ZafeScreen(title: title, showBack: false, children: [body]),
        ),
    ];
    for (final (name, build) in pages) {
      for (final (theme, data) in [
        ('dark', AppThemeData.dark),
        ('light', AppThemeData.light),
      ]) {
        final boundary = GlobalKey();
        await tester.pumpWidget(
          RepaintBoundary(
            key: ValueKey('$name-$theme'),
            child: RepaintBoundary(
              key: boundary,
              child: MaterialApp(
                debugShowCheckedModeBanner: false,
                builder: (context, child) =>
                    AppTheme(data: data, child: child!),
                home: Builder(builder: build),
              ),
            ),
          ),
        );
        for (var i = 0; i < 4; i++) {
          await tester.runAsync(
            () => Future<void>.delayed(const Duration(milliseconds: 50)),
          );
          await tester.pump(const Duration(milliseconds: 100));
        }
        final target =
            boundary.currentContext!.findRenderObject()!
                as RenderRepaintBoundary;
        final bytes = await tester.runAsync(() async {
          final image = await target.toImage(pixelRatio: 2);
          final data = await image.toByteData(format: ui.ImageByteFormat.png);
          return data!.buffer.asUint8List();
        });
        final path = '${out.path}/move_${name}_$theme.png';
        File(path).writeAsBytesSync(bytes!);
        // ignore: avoid_print
        print(path);
      }
    }
  });
}

void _noop() {}
