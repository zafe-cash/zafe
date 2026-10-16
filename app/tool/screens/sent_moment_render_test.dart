// Renders the sending screen through the "Sent!" moment, frame by frame, the whole
// screen (art, badge, title and the coin fountain together), for review without a
// device. Not part of `flutter test`; run it from app/:
//
//   flutter test tool/screens/sent_moment_render_test.dart
//
// Output (SCREEN_PREVIEW_OUT, default build/screen_preview/): sent_moment_<theme>_<ms>.png
// (ms after "Sent!"; negative = while sending).
import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/theme/app_theme.dart';
import 'package:zafe/src/core/widgets/mobile/mobile_transaction_progress_screen.dart';
import 'package:zafe/src/features/proposals/sent_art.dart';

Future<void> _loadFonts() async {
  final families = <String, List<String>>{
    'DM Sans': ['Regular', 'Medium', 'SemiBold'],
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

void main() {
  testWidgets('render the sent moment', (tester) async {
    await tester.runAsync(_loadFonts);
    final out = Directory(
      Platform.environment['SCREEN_PREVIEW_OUT'] ?? 'build/screen_preview',
    )..createSync(recursive: true);
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = const Size(390, 844);
    addTearDown(tester.view.reset);

    for (final (theme, data) in [
      ('dark', AppThemeData.dark),
      ('light', AppThemeData.light),
    ]) {
      final key = GlobalKey();
      Widget app(bool sent) => RepaintBoundary(
        key: key,
        child: MaterialApp(
          debugShowCheckedModeBanner: false,
          builder: (context, child) => AppTheme(data: data, child: child!),
          home: MobileTransactionProgressScreen(
            phase: sent
                ? MobileTransactionProgressPhase.succeeded
                : MobileTransactionProgressPhase.inProgress,
            title: sent ? 'Sent!' : 'Sending...',
            body: sent
                ? 'It confirms on-chain shortly.'
                : 'Building the transaction and submitting it...',
            canPop: true,
            bodyMaxWidth: 260,
            background: SentArt(sent: sent, sending: !sent),
            successEffect: const CoinFountain(),
          ),
        ),
      );
      Future<void> shot(String name) => tester.runAsync(() async {
        final boundary =
            key.currentContext!.findRenderObject()! as RenderRepaintBoundary;
        final image = await boundary.toImage(pixelRatio: 2);
        final bytes = await image.toByteData(format: ui.ImageByteFormat.png);
        File(
          '${out.path}/sent_moment_${theme}_$name.png',
        ).writeAsBytesSync(bytes!.buffer.asUint8List());
      });

      await tester.pumpWidget(app(false));
      for (var i = 0; i < 8; i++) {
        await tester.runAsync(
          () => Future<void>.delayed(const Duration(milliseconds: 150)),
        );
        await tester.pump(const Duration(milliseconds: 16));
      }
      await tester.pump(const Duration(milliseconds: 1800));
      await shot('-0500');
      await tester.pump(const Duration(milliseconds: 500));
      await shot('-0000');
      await tester.pumpWidget(app(true));
      // The fountain's coin pictures load off the test clock.
      await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 200)),
      );
      var t = 0;
      for (final ms in [40, 80, 140, 220, 320, 450, 600, 800, 1000, 1300]) {
        await tester.pump(Duration(milliseconds: ms - t));
        t = ms;
        await shot('+${ms.toString().padLeft(4, '0')}');
      }
      await tester.pump(const Duration(seconds: 4));
      await tester.pumpWidget(const SizedBox());
    }
  });
}
