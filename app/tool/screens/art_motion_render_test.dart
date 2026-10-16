// Renders frames of the animated art (key shards drifting and joining; the Sent coins
// leaving the slot) in both themes for review without a device. Not part of `flutter test`; run it from app/:
//
//   flutter test tool/screens/art_motion_render_test.dart
//
// Output (SCREEN_PREVIEW_OUT, default build/screen_preview/): key_shards_<theme>_<ms>.png,
// sent_coins_<theme>_<ms>.png.
import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/theme/app_theme.dart';
import 'package:zafe/src/features/onboarding/key_shards_art.dart';
import 'package:zafe/src/features/onboarding/welcome_art.dart';
import 'package:zafe/src/features/proposals/sent_art.dart';

void main() {
  testWidgets('render key shards frames', (tester) async {
    final out = Directory(
      Platform.environment['SCREEN_PREVIEW_OUT'] ?? 'build/screen_preview',
    )..createSync(recursive: true);
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = const Size(540, 620);
    addTearDown(tester.view.reset);
    final key = GlobalKey();

    for (final (theme, data) in [
      ('dark', AppThemeData.dark),
      ('light', AppThemeData.light),
    ]) {
      Widget app(bool assembled) => MaterialApp(
        debugShowCheckedModeBanner: false,
        builder: (context, child) => AppTheme(data: data, child: child!),
        home: RepaintBoundary(
          key: key,
          child: KeyShardsArt(assembled: assembled),
        ),
      );
      Future<void> shot(String name) async {
        await tester.runAsync(() async {
          final boundary =
              key.currentContext!.findRenderObject()! as RenderRepaintBoundary;
          final image = await boundary.toImage();
          final bytes = await image.toByteData(format: ui.ImageByteFormat.png);
          File(
            '${out.path}/key_shards_${theme}_$name.png',
          ).writeAsBytesSync(bytes!.buffer.asUint8List());
        });
      }

      await tester.pumpWidget(app(false));
      // flutter_svg decodes off the test clock.
      for (var i = 0; i < 6; i++) {
        await tester.runAsync(
          () => Future<void>.delayed(const Duration(milliseconds: 200)),
        );
        await tester.pump(const Duration(milliseconds: 16));
      }
      await shot('apart');
      await tester.pumpWidget(app(true));
      var t = 0;
      for (final ms in [150, 300, 450, 600, 900, 1110, 1700]) {
        await tester.pump(Duration(milliseconds: ms - t));
        t = ms;
        await shot(ms.toString().padLeft(4, '0'));
      }
      await tester.pumpAndSettle();
      await tester.pump(const Duration(seconds: 1));
      await tester.pumpWidget(const SizedBox());
    }
  });

  testWidgets('render sent coin frames', (tester) async {
    final out = Directory(
      Platform.environment['SCREEN_PREVIEW_OUT'] ?? 'build/screen_preview',
    )..createSync(recursive: true);
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = const Size(360, 780);
    addTearDown(tester.view.reset);
    final key = GlobalKey();
    for (final (theme, data) in [
      ('dark', AppThemeData.dark),
      ('light', AppThemeData.light),
    ]) {
      Widget app(bool sent) => MaterialApp(
        debugShowCheckedModeBanner: false,
        builder: (context, child) => AppTheme(data: data, child: child!),
        home: RepaintBoundary(
          key: key,
          child: SentArt(sent: sent, sending: !sent),
        ),
      );
      Future<void> shot(String name) => tester.runAsync(() async {
        final boundary =
            key.currentContext!.findRenderObject()! as RenderRepaintBoundary;
        final image = await boundary.toImage();
        final bytes = await image.toByteData(format: ui.ImageByteFormat.png);
        File(
          '${out.path}/sent_coins_${theme}_$name.png',
        ).writeAsBytesSync(bytes!.buffer.asUint8List());
      });
      await tester.pumpWidget(app(false));
      for (var i = 0; i < 6; i++) {
        await tester.runAsync(
          () => Future<void>.delayed(const Duration(milliseconds: 200)),
        );
        await tester.pump(const Duration(milliseconds: 16));
      }
      // The stream while sending.
      for (final ms in [600, 1400, 2600]) {
        await tester.pump(const Duration(milliseconds: 400));
        await tester.pump(Duration(milliseconds: ms - 400));
        await shot('stream_$ms');
      }
      await shot('0000');
      await tester.pumpWidget(app(true));
      var t = 0;
      for (final ms in [80, 160, 240, 320, 400, 500, 650, 800, 1000]) {
        await tester.pump(Duration(milliseconds: ms - t));
        t = ms;
        await shot(ms.toString().padLeft(4, '0'));
      }
      await tester.pump(const Duration(seconds: 3));
      await tester.pumpWidget(const SizedBox());
    }
  });

  testWidgets('render welcome wheel frames', (tester) async {
    final out = Directory(
      Platform.environment['SCREEN_PREVIEW_OUT'] ?? 'build/screen_preview',
    )..createSync(recursive: true);
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = const Size(540, 620);
    addTearDown(tester.view.reset);
    final key = GlobalKey();
    await tester.pumpWidget(
      MaterialApp(
        debugShowCheckedModeBanner: false,
        builder: (context, child) =>
            AppTheme(data: AppThemeData.dark, child: child!),
        home: RepaintBoundary(key: key, child: const WelcomeArt()),
      ),
    );
    for (var i = 0; i < 6; i++) {
      await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 200)),
      );
      await tester.pump(const Duration(milliseconds: 16));
    }
    // The unlock starts 0.6 s in.
    await tester.pump(const Duration(milliseconds: 600));
    var t = 0;
    for (final ms in [
      0,
      600,
      1200,
      1800,
      2400,
      3000,
      3200,
      3500,
      3800,
      4100,
      4500,
      4900,
      5500,
    ]) {
      await tester.pump(Duration(milliseconds: ms - t));
      t = ms;
      await tester.runAsync(() async {
        final boundary =
            key.currentContext!.findRenderObject()! as RenderRepaintBoundary;
        final image = await boundary.toImage();
        final bytes = await image.toByteData(format: ui.ImageByteFormat.png);
        File(
          '${out.path}/welcome_wheel_${ms.toString().padLeft(4, '0')}.png',
        ).writeAsBytesSync(bytes!.buffer.asUint8List());
      });
    }
    await tester.pumpWidget(const SizedBox());
    await tester.pump(const Duration(seconds: 10));
  });
}
