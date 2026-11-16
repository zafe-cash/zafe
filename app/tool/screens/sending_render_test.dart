import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/theme/app_theme.dart';
import 'package:zafe/src/core/widgets/mobile/mobile_transaction_progress_screen.dart';
import 'package:zafe/src/features/proposals/sent_art.dart';

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

final _phases =
    <String, (MobileTransactionProgressPhase, String, String, String?)>{
      'sending_progress': (
        MobileTransactionProgressPhase.inProgress,
        'Sending...',
        'Building the transaction and submitting it...',
        'Back to Home',
      ),
      'sending_sent': (
        MobileTransactionProgressPhase.succeeded,
        'Sent!',
        'It confirms on-chain shortly.',
        null,
      ),
    };

void main() {
  testWidgets('render the sending screen', (tester) async {
    await tester.runAsync(_loadFonts);
    final out = Directory(
      Platform.environment['SCREEN_PREVIEW_OUT'] ?? 'build/screen_preview',
    )..createSync(recursive: true);
    tester.view.devicePixelRatio = 3;
    tester.view.physicalSize = const Size(390 * 3, 844 * 3);
    addTearDown(tester.view.reset);

    for (final MapEntry(key: name, value: (phase, title, body, action))
        in _phases.entries) {
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
                child: AppTheme(
                  data: data,
                  child: MobileTransactionProgressScreen(
                    phase: phase,
                    title: title,
                    body: body,
                    canPop: true,
                    bodyMaxWidth: 260,
                    background: SentArt(
                      sent: phase == MobileTransactionProgressPhase.succeeded,
                    ),
                    secondaryActionLabel: action,
                    onSecondaryAction: action == null ? null : () {},
                  ),
                ),
              ),
            ),
          ),
        );
        for (var i = 0; i < 8; i++) {
          await tester.runAsync(
            () => Future<void>.delayed(const Duration(milliseconds: 50)),
          );
          await tester.pump(const Duration(milliseconds: 400));
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
        stdout.writeln(path);
      }
    }
  });
}
