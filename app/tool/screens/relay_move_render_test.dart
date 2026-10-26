// Renders the server and move screens without a device (docs/relay-move.md §4.2-4.5):
// the vault's Server screen (normal, a member must update, a private server), the
// server list from Move, the waiting room (and with the old server down), the mover's
// done page, the follow screen from a link, the Status screen's move suggestion, the
// Zcash server screen, the Needs you move cards, Home's mover card, the confirm dialog
// and the Activity row, in both themes. Not part of `flutter test`; run it explicitly
// (from app/):
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
import 'package:zafe/src/core/config/lightwalletd_presets.dart';
import 'package:zafe/src/core/config/relay_presets.dart';
import 'package:zafe/src/core/layout/mobile/zafe_screen.dart';
import 'package:zafe/src/core/theme/app_theme.dart';
import 'package:zafe/src/core/widgets/action_button.dart';
import 'package:zafe/src/core/widgets/app_button.dart';
import 'package:zafe/src/core/widgets/app_confirm.dart';
import 'package:zafe/src/core/widgets/mobile/mobile_surface_card.dart';
import 'package:zafe/src/core/widgets/mobile/zafe_detail.dart';
import 'package:zafe/src/core/widgets/mobile_text_field.dart';
import 'package:zafe/src/features/home/home_screen.dart' show NoticeCard;
import 'package:zafe/src/features/home/needs_you.dart' show RequestCard;
import 'package:zafe/src/features/home/sync_status_screen.dart'
    show MoveSuggestion;
import 'package:zafe/src/features/proposals/activity_feed.dart';
import 'package:zafe/src/features/relay_move/relay_move_screen.dart';
import 'package:zafe/src/features/settings/relay_picker.dart';
import 'package:zafe/src/features/settings/vault_server_screen.dart';
import 'package:zafe/src/features/settings/zcash_server_screen.dart';
import 'package:zafe/src/providers/server_failover_provider.dart';
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
const _ours = 'https://relay.zafe.cash';
const _key1 =
    'a3f19c2d7e4b8a10c5d6e7f8091a2b3c4d5e6f708192a3b4c5d6e7f8091a2b3c';

Future<void> _done() async {}

RelayCheck _info({bool push = true, bool token = false}) => RelayCheck(
  tooOld: false,
  relayApiVersion: 2,
  push: push,
  createTokenRequired: token,
  maxMembers: 64,
  requestsPerMinute: 300,
  deliveryRetentionDays: 30,
  idleDays: 365,
);

Widget _server({
  String url = _ours,
  RelayCheckResult? result,
  String? mustUpdate,
  bool hasToken = false,
}) => ZafeScreen(
  title: 'Server',
  bottom: ActionButton(
    label: 'Move to another server',
    variant: AppButtonVariant.secondary,
    enabled: mustUpdate == null,
    onPressed: _done,
  ),
  children: [
    VaultServerBody(
      vaultName: 'Grants',
      url: url,
      hasCreateToken: hasToken,
      result: result ?? RelayCheckResult.ok(_info()),
      token: TextEditingController(),
      tokenFocus: FocusNode(),
      mustUpdate: mustUpdate,
    ),
  ],
);

Widget _waiting({String? resume}) => ZafeScreen(
  title: 'Server',
  bottom: const ActionButton(
    label: 'Cancel move',
    variant: AppButtonVariant.ghost,
    onPressed: _done,
  ),
  children: [
    MoveWaitingRoom(
      vaultName: 'Grants',
      host: _host,
      members: const [
        ('You', true, null),
        ('Treasurer Alice', true, null),
        ('Bob', false, _noop),
        ('8c41e2...77d0', false, _noop),
      ],
      needed: 3,
      total: 4,
      resume: resume,
    ),
  ],
);

/// Whole pages with their pinned buttons: (name, page).
List<(String, Widget)> relayMoveScreens() => [
  ('server', _server()),
  ('server_must_update', _server(mustUpdate: mustUpdateText(['Bob']))),
  (
    'server_private',
    _server(
      url: 'https://$_host',
      result: RelayCheckResult.ok(_info(push: false, token: true)),
    ),
  ),
  (
    'picker',
    ZafeScreen(
      title: 'Move to',
      bottom: const RelayPickerActions(
        confirmLabel: 'Use this server',
        onConfirm: _noop,
        onRunYourOwn: _noop,
      ),
      children: [
        RelayPickerBody(
          presets: kMainnetRelayPresets,
          selectedUrl: 'https://$_host',
          result: RelayCheckResult.ok(_info(push: false)),
          otherOpen: true,
          other: TextEditingController(text: _host),
          otherFocus: FocusNode(),
          onSelect: (_) {},
          onOther: _noop,
        ),
      ],
    ),
  ),
  ('waiting', _waiting()),
  ('waiting_down', _waiting(resume: paymentsResumeText(1, _host))),
  (
    'moved',
    ZafeScreen(
      title: 'Server',
      bottom: AppButton(
        expand: true,
        onPressed: _noop,
        child: const Text('Done'),
      ),
      children: const [MovedBody(vaultName: 'Grants', host: _host)],
    ),
  ),
  (
    'follow_link',
    ZafeScreen(
      title: kRelayMoveTitle,
      bottom: _followButtons(),
      children: const [
        FollowMoveBody(
          vaultName: 'Grants',
          host: _host,
          noPush: true,
          seesIp: true,
        ),
      ],
    ),
  ),
  (
    'status_suggest',
    ZafeScreen(
      title: 'Status',
      bottom: AppButton(
        expand: true,
        variant: AppButtonVariant.secondary,
        onPressed: _noop,
        child: const Text('Try again'),
      ),
      children: [
        Builder(
          builder: (context) => Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Text(
                'Can\'t reach the server',
                style: AppTypography.headlineSmall.copyWith(
                  color: context.colors.text.accent,
                ),
              ),
              const SizedBox(height: AppSpacing.xs),
              Text(
                'Payments can\'t be approved until the server is back. Your funds '
                'are safe.',
                style: AppTypography.bodyMedium.copyWith(
                  color: context.colors.text.secondary,
                ),
              ),
              const SizedBox(height: AppSpacing.sm),
              const MobileSurfaceCard(
                cornerRadius: AppRadii.large,
                child: Column(
                  children: [
                    DetailRow(label: 'Server', value: 'relay.zafe.cash'),
                    DetailRow(label: 'Down since', value: '09:14'),
                    DetailRow(label: 'Last synced', value: 'Today 09:12'),
                  ],
                ),
              ),
              const SizedBox(height: AppSpacing.md),
              const MoveSuggestion(
                vaultName: 'Grants',
                host: 'relay2.zafe.cash',
                onMove: _done,
                onPickAnother: _noop,
              ),
            ],
          ),
        ),
      ],
    ),
  ),
  (
    'zcash_server',
    ZafeScreen(
      title: 'Zcash server',
      bottom: const ActionButton(label: 'Test and save', onPressed: _done),
      children: [
        LightwalletdServerList(
          presets: kLightwalletdPresets,
          selectedUrl: 'https://zcash.dao.org:9067',
          probes: {
            for (final p in kLightwalletdPresets)
              p.url: const ServerProbe(
                ServerCheck.ok,
                Duration(milliseconds: 120),
              ),
          },
          customOpen: true,
          onSelect: (_) {},
          onCustom: _noop,
        ),
        const SizedBox(height: AppSpacing.xs),
        MobileTextField(
          controller: TextEditingController(text: 'https://zcash.dao.org:9067'),
          focusNode: FocusNode(),
          hintText: 'https://',
        ),
      ],
    ),
  ),
  (
    'confirm',
    Builder(
      builder: (context) => Scaffold(
        backgroundColor: context.colors.background.neutralScrim,
        body: const Center(
          child: AppConfirmCard(
            title: 'Remove "Grants"?',
            body:
                'This deletes your key share from this phone. If fewer than 2 '
                'members keep theirs, the funds are lost for good.',
            confirmLabel: 'Remove from this phone',
            cancelLabel: 'Keep it',
            onConfirm: _noop,
            onCancel: _noop,
          ),
        ),
      ),
    ),
  ),
];

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

/// Cards: (name, page title, content).
List<(String, String?, Widget)> relayMoveCases() => [
  (
    'needs_you',
    'Home',
    const Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        RequestCard(
          tileKey: _key1,
          who: 'Move Grants to $_host',
          detail:
              'Treasurer Alice asked · 1 of 4 moved · your funds don\'t move',
          action: 'Move',
          onTap: _noop,
          onDecline: _noop,
          declineLabel: 'Not now',
        ),
        SizedBox(height: AppSpacing.s),
        RequestCard(
          tileKey: _key1,
          who: 'Grants moved to $_host',
          detail: 'Move to continue · your funds don\'t move',
          action: 'Move',
          onTap: _noop,
        ),
      ],
    ),
  ),
  (
    'home_mover',
    'Home',
    const Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        NoticeCard(
          title: 'Moving Grants to $_host',
          body: '2 of 4 moved',
          warning: false,
          onTap: _noop,
        ),
        SizedBox(height: AppSpacing.md),
        NoticeCard(
          title: 'Two moves found',
          body: 'Ask your group which server to use.',
          onTap: _noop,
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
