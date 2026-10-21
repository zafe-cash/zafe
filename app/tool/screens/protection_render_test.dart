// Renders the "How it's protected" page (2 of 3, and 3 of 3) and the Signers tab's backup
// health (all, some, too few backed up) in both themes to PNGs for review without a device.
// Not part of `flutter test`; run it explicitly (from app/):
//
//   flutter test tool/screens/protection_render_test.dart
//
// Output (SCREEN_PREVIEW_OUT, default build/screen_preview/): protection_<rule>_<theme>.png,
// backups_<state>_<theme>.png.
import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/layout/mobile/zafe_screen.dart';
import 'package:zafe/src/core/theme/app_theme.dart';
import 'package:zafe/src/core/widgets/mobile/mobile_surface_card.dart';
import 'package:zafe/src/core/widgets/mobile/zafe_detail.dart';
import 'package:zafe/src/features/signers/signers_screen.dart';
import 'package:zafe/src/features/settings/vault_protection_screen.dart';

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

const _me = 'a3f09c41d2e87b5566c0de19f4a2b7c8e1d0937a6b5c4d3e2f1a0b9c8d7e6f50';
const _bob = '5e17b2c9a4d86f3310ab77e2c4d9f1086b3a2c5d7e9f0a1b2c3d4e5f6a7b8c9d';
const _cara =
    'c2d4e6f8a0b1c3d5e7f9a1b3c5d7e9f1a2b4c6d8e0f2a4b6c8d0e2f4a6b8c0d2';
const _dev = '9b8a7c6d5e4f3a2b1c0d9e8f7a6b5c4d3e2f1a0b9c8d7e6f5a4b3c2d1e0f9a8b';

/// The Signers tab's cards for a 2-of-4 vault where `backedUp` signers have a backup.
Widget _backups(Set<String> backedUp) {
  const members = [_me, _bob, _cara, _dev];
  return Column(
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      BackupHealthCard(
        signers: members.length,
        threshold: 2,
        backedUp: members.where(backedUp.contains).length,
      ),
      const SizedBox(height: AppSpacing.md),
      MobileSurfaceCard(
        cornerRadius: AppRadii.large,
        child: Column(
          children: [
            for (final m in members)
              SignerRow(
                keyHex: m,
                me: _me,
                name: {_bob: 'Bob', _cara: 'Cara'}[m],
                trailing: BackupLabel(backedUp: backedUp.contains(m)),
              ),
          ],
        ),
      ),
    ],
  );
}

void main() {
  testWidgets('render the protection page', (tester) async {
    await tester.runAsync(_loadFonts);
    final out = Directory(
      Platform.environment['SCREEN_PREVIEW_OUT'] ?? 'build/screen_preview',
    )..createSync(recursive: true);
    tester.view.devicePixelRatio = 3;
    tester.view.physicalSize = const Size(390 * 3, 844 * 3);
    addTearDown(tester.view.reset);

    final pages = <(String, String, Widget)>[
      for (final (rule, t, n) in [('2of3', 2, 3), ('3of3', 3, 3)])
        (
          'protection_$rule',
          'How it\'s protected',
          VaultProtectionBody(threshold: t, signers: n),
        ),
      ('backups_all', 'Signers', _backups({_me, _bob, _cara, _dev})),
      ('backups_some', 'Signers', _backups({_me, _cara})),
      ('backups_few', 'Signers', _backups({_cara})),
    ];
    for (final (name, title, body) in pages) {
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
                home: ZafeScreen(
                  title: title,
                  showBack: false,
                  children: [body],
                ),
              ),
            ),
          ),
        );
        for (var i = 0; i < 6; i++) {
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
        final path = '${out.path}/${name}_$theme.png';
        File(path).writeAsBytesSync(bytes!);
        // ignore: avoid_print
        print(path);
      }
    }
  });
}
