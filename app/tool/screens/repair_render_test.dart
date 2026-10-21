// Renders the lost-phone flow without a device: the new phone's recovery screen (waiting,
// repairing, checking the vault), a co-signer's safety-code check, and the Signers tab's
// pending move card, in both themes. Not part of `flutter test`; run it explicitly (from app/):
//
//   flutter test tool/screens/repair_render_test.dart
//
// Output (SCREEN_PREVIEW_OUT, default build/screen_preview/): repair_<state>_<theme>.png.
import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/layout/mobile/zafe_screen.dart';
import 'package:zafe/src/core/theme/app_theme.dart';
import 'package:zafe/src/features/recover/recover_screen.dart';
import 'package:zafe/src/features/signers/replace_signer_screen.dart';
import 'package:zafe/src/features/signers/signers_screen.dart';
import 'package:zafe/src/rust/api/repair.dart'
    show CosignerWords, RecoveryStage;

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

void main() {
  testWidgets('render the lost-phone flow', (tester) async {
    await tester.runAsync(_loadFonts);
    final out = Directory(
      Platform.environment['SCREEN_PREVIEW_OUT'] ?? 'build/screen_preview',
    )..createSync(recursive: true);
    tester.view.devicePixelRatio = 3;
    tester.view.physicalSize = const Size(390 * 3, 844 * 3);
    addTearDown(tester.view.reset);

    final pages = <(String, String, Widget)>[
      (
        'waiting',
        'Recover your seat',
        const RecoverBody(
          code: _code,
          checks: [
            CosignerWords(
              cosignerKeyHex:
                  '97c9ce4b0d2f6a1e8c3b5d7f9a1c3e5f7b9d1f3a5c7e9b1d3f5a7c9e1b3d123e',
              words: 'ancient cinnamon zebra legend',
            ),
          ],
        ),
      ),
      (
        'repairing',
        'Recover your seat',
        const RecoverBody(
          code: _code,
          stage: RecoveryStage.repairing,
          received: 1,
          needed: 2,
        ),
      ),
      (
        'check',
        'Check your vault',
        const VaultCheckBody(
          name: 'Grants',
          address:
              'utest1qz8m4k2v7d3w9x0c5n6p1r8s2t4u7y9a3b5e6f8g0h2j4k6l8m0n2p4q6r8s0t2u4w6',
          onReject: _noop,
        ),
      ),
      (
        'approve',
        'Replace lost phone',
        const Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            SeatMoveCard(
              who: 'Alice',
              approvals: 1,
              needed: 2,
              onApprove: _noop,
            ),
            SizedBox(height: AppSpacing.md),
            SafetyWordsCard(
              words: 'ancient cinnamon zebra legend',
              who: 'Alice',
              myKeyHex:
                  '97c9ce4b0d2f6a1e8c3b5d7f9a1c3e5f7b9d1f3a5c7e9b1d3f5a7c9e1b3d123e',
              confirmed: true,
            ),
            SizedBox(height: AppSpacing.md),
            RepairCard(
              who: 'Alice',
              helpers: [('b', 'Bob'), ('c', 'Cara')],
              meHelping: false,
              onTakeOver: _takeOver,
            ),
          ],
        ),
      ),
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
        final path = '${out.path}/repair_${name}_$theme.png';
        File(path).writeAsBytesSync(bytes!);
        // ignore: avoid_print
        print(path);
      }
    }
  });
}

void _noop() {}

Future<void> _takeOver(String key, String name) async {}
