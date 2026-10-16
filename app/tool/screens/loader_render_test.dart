// Renders the loading arc at a few sizes, light and dark, at four moments of a turn.
//   flutter test tool/screens/loader_render_test.dart
// Output: build/screen_preview/loader.png
import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/theme/app_theme.dart';
import 'package:zafe/src/core/widgets/app_loading_icon.dart';

void main() {
  testWidgets('loader', (tester) async {
    tester.view.devicePixelRatio = 3;
    tester.view.physicalSize = const Size(360 * 3, 200 * 3);
    addTearDown(tester.view.reset);
    final boundary = GlobalKey();
    Widget row(AppThemeData data) => AppTheme(
      data: data,
      child: Builder(
        builder: (context) => ColoredBox(
          color: context.colors.background.window,
          child: Padding(
            padding: const EdgeInsets.all(16),
            child: Row(
              mainAxisAlignment: MainAxisAlignment.spaceEvenly,
              children: [
                for (final s in [16.0, 20.0, 24.0, 48.0])
                  AppLoadingIcon(size: s, color: context.colors.icon.accent),
                AppLoadingIcon(size: 24, color: context.colors.icon.brand),
              ],
            ),
          ),
        ),
      ),
    );
    await tester.pumpWidget(
      MaterialApp(
        debugShowCheckedModeBanner: false,
        home: RepaintBoundary(
          key: boundary,
          child: Column(
            children: [row(AppThemeData.dark), row(AppThemeData.light)],
          ),
        ),
      ),
    );
    final out = Directory('build/screen_preview')..createSync(recursive: true);
    for (var i = 0; i < 4; i++) {
      await tester.pump(const Duration(milliseconds: 250));
      final render =
          boundary.currentContext!.findRenderObject()! as RenderRepaintBoundary;
      final bytes = await tester.runAsync(() async {
        final image = await render.toImage(pixelRatio: 3);
        final data = await image.toByteData(format: ui.ImageByteFormat.png);
        return data!.buffer.asUint8List();
      });
      File('${out.path}/loader_$i.png').writeAsBytesSync(bytes!);
    }
  });
}
