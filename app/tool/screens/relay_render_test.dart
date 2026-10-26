// Renders the relay screens without a device: the relay picker (our relay, another
// address that needs a create token and sends no notifications, a relay too old for new
// vaults), the vault's relay in Settings, Create > More options > Relay, the join
// screen's fact line, the seat-recovery vault link on both phones and Home during a
// relay outage, in both themes. Not part of `flutter test`; run it explicitly (from app/):
//
//   flutter test tool/screens/relay_render_test.dart
//
// Output (SCREEN_PREVIEW_OUT, default build/screen_preview/): relay_<state>_<theme>.png.
import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/config/relay_presets.dart';
import 'package:zafe/src/core/layout/mobile/mobile_top_nav.dart';
import 'package:zafe/src/core/layout/mobile/zafe_screen.dart';
import 'package:zafe/src/core/theme/app_theme.dart';
import 'package:zafe/src/core/widgets/app_icon.dart';
import 'package:zafe/src/core/widgets/mobile/mobile_list_row.dart';
import 'package:zafe/src/core/widgets/mobile/mobile_surface_card.dart';
import 'package:zafe/src/features/home/home_screen.dart' show NoticeCard;
import 'package:zafe/src/features/onboarding/create_vault_screen.dart';
import 'package:zafe/src/features/onboarding/join_vault_screen.dart';
import 'package:zafe/src/features/recover/recover_screen.dart';
import 'package:zafe/src/features/settings/relay_picker.dart';
import 'package:zafe/src/features/signers/replace_signer_screen.dart';
import 'package:zafe/src/notifications/vault_updates.dart';
import 'package:zafe/src/rust/api/endpoints.dart';

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

const _code =
    'zafe-recover-v1:2c9f04d1a7e8b3c56d0f1e2a3b4c5d6e7f8091a2b3c4d5e6f708192a3b4c5d6e7f'
    '8a9b0c1d2e3f405162738495a6b7c8d9e0f1a2b3c4d5e6f708192a3b4c5d6e7f8091';

RelayCheck relayInfo({
  bool push = true,
  bool token = false,
  bool tooOld = false,
}) => RelayCheck(
  tooOld: tooOld,
  relayApiVersion: tooOld ? 0 : 2,
  push: push,
  createTokenRequired: token,
  maxMembers: 64,
  requestsPerMinute: 300,
  deliveryRetentionDays: 30,
  idleDays: 365,
);

Widget picker(
  String selectedUrl,
  RelayCheckResult result, {
  bool other = false,
  bool token = false,
}) => Column(
  crossAxisAlignment: CrossAxisAlignment.stretch,
  children: [
    RelayPickerBody(
      presets: kMainnetRelayPresets,
      selectedUrl: selectedUrl,
      result: result,
      otherOpen: other,
      other: TextEditingController(text: other ? relayHost(selectedUrl) : ''),
      otherFocus: FocusNode(),
      token: token ? TextEditingController() : null,
      tokenFocus: FocusNode(),
      onSelect: (_) {},
      onOther: _noop,
    ),
  ],
);

/// (name, page title or null for a sheet, content)
List<(String, String?, Widget)> relayCases() => [
  (
    'picker_ours',
    'Server',
    picker(kMainnetRelayUrlForPreview, RelayCheckResult.ok(relayInfo())),
  ),
  (
    'picker_other_token',
    'Server',
    picker(
      'https://relay.dao.org',
      RelayCheckResult.ok(relayInfo(push: false, token: true)),
      other: true,
      token: true,
    ),
  ),
  (
    'picker_too_old',
    'Server',
    picker(
      'https://old.relay.example',
      RelayCheckResult.ok(relayInfo(tooOld: true)),
      other: true,
    ),
  ),
  (
    'picker_unreachable',
    'Server',
    picker(
      kMainnetRelayUrlForPreview,
      const RelayCheckResult.failed('Couldn\'t connect.'),
    ),
  ),
  (
    'settings_servers',
    'Settings',
    MobileSurfaceCard(
      cornerRadius: AppRadii.large,
      child: Column(
        children: [
          const MobileListRow(
            leading: AppIcon(AppIcons.server, size: 20),
            label: 'Server',
            value: 'relay.dao.org',
            showChevron: true,
            minRowHeight: 44,
          ),
          const MobileListRow(
            leading: AppIcon(AppIcons.globe, size: 20),
            label: 'Zcash server',
            value: 'zec.rocks',
            showChevron: true,
            minRowHeight: 44,
          ),
        ],
      ),
    ),
  ),
  (
    'create_more',
    'New vault',
    const CreateRelayRow(host: 'relay.dao.org', onTap: _noop),
  ),
  (
    'join_fact',
    'Join a vault',
    Builder(
      builder: (context) => Text(
        inviteFact(
          name: 'Grants',
          threshold: 2,
          members: 3,
          relayHost: 'relay.dao.org',
        ),
        style: AppTypography.bodyMedium.copyWith(
          color: context.colors.text.accent,
        ),
      ),
    ),
  ),
  (
    'recover_link',
    'Recover your seat',
    const RecoverBody(code: _code, linkHost: 'relay.dao.org'),
  ),
  (
    'replace_link',
    'Replace lost phone',
    const Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SeatLinkCard(host: 'relay.dao.org', onShare: _noop, onShowQr: _noop),
        SizedBox(height: 16),
        SafetyWordsCard(words: null, who: 'Alexandra'),
      ],
    ),
  ),
  (
    'home_outage',
    null,
    Builder(
      builder: (context) => ColoredBox(
        color: context.colors.background.window,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            MobileTopNav.account(
              accountName: 'Grants',
              syncLabel: 'Can\'t reach server',
              syncLabelColor: context.colors.sync.textError,
            ),
            Padding(
              padding: const EdgeInsets.all(16),
              child: NoticeCard(
                title: 'Money left without approval',
                body: unapprovedSpendText(
                  fromCopyAt: DateTime(2026, 10, 25, 9, 14),
                  now: DateTime(2026, 10, 25, 12),
                ),
                onTap: _noop,
              ),
            ),
          ],
        ),
      ),
    ),
  ),
];

const kMainnetRelayUrlForPreview = 'https://relay.zafe.cash';

void main() {
  testWidgets('render the relay screens', (tester) async {
    await tester.runAsync(_loadFonts);
    final out = Directory(
      Platform.environment['SCREEN_PREVIEW_OUT'] ?? 'build/screen_preview',
    )..createSync(recursive: true);
    tester.view.devicePixelRatio = 3;
    tester.view.physicalSize = const Size(390 * 3, 844 * 3);
    addTearDown(tester.view.reset);

    for (final (name, title, body) in relayCases()) {
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
                home: title == null
                    ? Builder(
                        builder: (context) => Scaffold(
                          backgroundColor: context.colors.background.window,
                          body: Align(
                            alignment: Alignment.bottomCenter,
                            child: Padding(
                              padding: const EdgeInsets.all(16),
                              child: DecoratedBox(
                                decoration: BoxDecoration(
                                  color: context.colors.background.base,
                                  borderRadius: BorderRadius.circular(
                                    AppRadii.xLarge,
                                  ),
                                ),
                                child: body,
                              ),
                            ),
                          ),
                        ),
                      )
                    : ZafeScreen(
                        title: title,
                        showBack: false,
                        children: [body],
                      ),
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
        final path = '${out.path}/relay_${name}_$theme.png';
        File(path).writeAsBytesSync(bytes!);
        // ignore: avoid_print
        print(path);
      }
    }
  });
}

void _noop() {}
