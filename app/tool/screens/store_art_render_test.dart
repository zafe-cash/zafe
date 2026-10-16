// Renders single illustration layers (the key's shards, a coin) on a transparent
// background at high resolution, for the store listing's graphics. Not part of
// `flutter test`; run it explicitly (from app/):
//
//   flutter test tool/screens/store_art_render_test.dart
//
// Output (SCREEN_PREVIEW_OUT, default build/screen_preview/): art_<layer>.png, each on
// its scene's canvas (pieces stay where the scene draws them; crop by alpha).

import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_svg/flutter_svg.dart';
import 'package:flutter_test/flutter_test.dart';

const _layers = <(String, double, double)>[
  ('key_shards_piece0_dark', 1080, 1240),
  ('key_shards_piece1_dark', 1080, 1240),
  ('key_shards_piece2_dark', 1080, 1240),
  ('key_shards_piece3_dark', 1080, 1240),
  ('key_shards_piece4_dark', 1080, 1240),
  ('sent_slot_coin_dark', 100, 64),
];

void main() {
  testWidgets('render store art layers', (tester) async {
    final out = Directory(
      Platform.environment['SCREEN_PREVIEW_OUT'] ?? 'build/screen_preview',
    )..createSync(recursive: true);
    for (final (name, w, h) in _layers) {
      // Coins are tiny on their canvas: draw them larger.
      final scale = w < 200 ? 12.0 : 2.0;
      tester.view.devicePixelRatio = 1;
      tester.view.physicalSize = Size(w * scale, h * scale);
      final boundary = GlobalKey();
      final svg = File(
        'assets/illustrations/layers/$name.svg',
      ).readAsStringSync();
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: RepaintBoundary(
            key: boundary,
            child: SizedBox(
              width: w * scale,
              height: h * scale,
              child: SvgPicture.string(svg, fit: BoxFit.fill),
            ),
          ),
        ),
      );
      for (var i = 0; i < 4; i++) {
        await tester.runAsync(
          () => Future<void>.delayed(const Duration(milliseconds: 50)),
        );
        await tester.pump();
      }
      final render =
          boundary.currentContext!.findRenderObject()! as RenderRepaintBoundary;
      final bytes = await tester.runAsync(() async {
        final image = await render.toImage();
        final data = await image.toByteData(format: ui.ImageByteFormat.png);
        return data!.buffer.asUint8List();
      });
      File('${out.path}/art_$name.png').writeAsBytesSync(bytes!);
    }
    tester.view.reset();
  });
}
