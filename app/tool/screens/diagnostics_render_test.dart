// Renders the Settings > "Diagnostic report" sheet (empty log, and a log with app, background
// and Rust panic entries) in both themes to PNGs for review without a device.
// Not part of `flutter test`; run it explicitly (from app/):
//
//   flutter test tool/screens/diagnostics_render_test.dart
//
// Output (SCREEN_PREVIEW_OUT, default build/screen_preview/): diagnostics_<state>_<theme>.png.
import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/diagnostics/crash_log.dart';
import 'package:zafe/src/core/theme/app_theme.dart';
import 'package:zafe/src/features/settings/diagnostics_sheet.dart';

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

void main() {
  testWidgets('render the diagnostic report sheet', (tester) async {
    await tester.runAsync(_loadFonts);
    final out = Directory(
      Platform.environment['SCREEN_PREVIEW_OUT'] ?? 'build/screen_preview',
    )..createSync(recursive: true);
    tester.view.devicePixelRatio = 3;
    tester.view.physicalSize = const Size(390 * 3, 844 * 3);
    addTearDown(tester.view.reset);

    final dir = Directory.systemTemp.createTempSync('diag-render');
    addTearDown(() => dir.deleteSync(recursive: true));
    final empty = CrashLog.forDir('${dir.path}/empty', isBackground: false);
    final full = CrashLog.forDir('${dir.path}/full', isBackground: false);
    await tester.runAsync(() async {
      await full.record(
        'flutter',
        StateError(
          'Bad state: no vault for utest1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq',
        ),
        StackTrace.fromString(
          '#0      ProposalsNotifier.refresh (package:zafe/src/providers/proposals_provider.dart:212:7)\n'
          '#1      _rootRunUnary (dart:async/zone.dart:1407:47)',
        ),
      );
      await CrashLog.forDir('${dir.path}/full', isBackground: true).record(
        'background-vault',
        StateError('relay answered 500'),
        StackTrace.fromString(
          '#0      _checkVault (package:zafe/src/notifications/vault_watch.dart:301:9)',
        ),
      );
      File('${dir.path}/full/rust-panics.log').writeAsStringSync(
        '2026-10-07T10:00:00Z panic at crates/zafe-core/src/node.rs:42:9\n',
      );
    });

    for (final (name, log) in [('empty', empty), ('entries', full)]) {
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
                home: Builder(
                  builder: (context) => Scaffold(
                    backgroundColor: context.colors.background.window,
                    body: Align(
                      alignment: Alignment.bottomCenter,
                      child: DiagnosticsSheet(log: log),
                    ),
                  ),
                ),
              ),
            ),
          ),
        );
        for (var i = 0; i < 12; i++) {
          await tester.runAsync(
            () => Future<void>.delayed(const Duration(milliseconds: 100)),
          );
          await tester.pump(const Duration(milliseconds: 100));
        }
        await tester.pump(const Duration(seconds: 1));
        final target =
            boundary.currentContext!.findRenderObject()!
                as RenderRepaintBoundary;
        final bytes = await tester.runAsync(() async {
          final image = await target.toImage(pixelRatio: 2);
          final data = await image.toByteData(format: ui.ImageByteFormat.png);
          return data!.buffer.asUint8List();
        });
        final path = '${out.path}/diagnostics_${name}_$theme.png';
        File(path).writeAsBytesSync(bytes!);
        // ignore: avoid_print
        print(path);
      }
    }
  });
}
