import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/painting.dart';
import 'package:flutter_svg/flutter_svg.dart';
import 'package:flutter_test/flutter_test.dart';

const _windowDark = Color(0xFF080B0B);
const _windowLight = Color(0xFFF1F5F5);
const _width = 1080.0;
const _phone = Size(1080, 2400);

Future<void> _writePng(ui.Image image, String path) async {
  final data = await image.toByteData(format: ui.ImageByteFormat.png);
  File(path).writeAsBytesSync(data!.buffer.asUint8List());
}

ui.Image _record(Size size, void Function(Canvas) paint) {
  final recorder = ui.PictureRecorder();
  paint(Canvas(recorder));
  return recorder.endRecording().toImageSync(
    size.width.round(),
    size.height.round(),
  );
}

void main() {
  test('render illustrations', () async {
    final out = Directory(
      Platform.environment['ILLUSTRATION_PREVIEW_OUT'] ??
          'build/illustration_preview',
    )..createSync(recursive: true);
    final files =
        Directory('assets/illustrations')
            .listSync()
            .whereType<File>()
            .where((f) => f.path.endsWith('.svg'))
            .toList()
          ..sort((a, b) => a.path.compareTo(b.path));
    expect(files, isNotEmpty);

    final thumbs = <ui.Image>[];
    for (final file in files) {
      final name = file.uri.pathSegments.last.replaceAll('.svg', '');
      final dark = name.endsWith('_dark');
      final window = dark ? _windowDark : _windowLight;
      final info = await vg.loadPicture(
        SvgStringLoader(file.readAsStringSync()),
        null,
      );
      final scale = _width / info.size.width;
      final artSize = Size(_width, info.size.height * scale);

      void drawArt(Canvas c) {
        c.save();
        c.scale(scale);
        c.drawPicture(info.picture);
        c.restore();
      }

      final raw = _record(artSize, drawArt);
      await _writePng(raw, '${out.path}/$name.png');

      thumbs.add(raw);
      if (artSize.height > 2 * artSize.width) {
        final bg = _record(_phone, (c) {
          c.drawRect(Offset.zero & _phone, Paint()..color = window);
          final s = _phone.height / artSize.height;
          c.save();
          c.translate((_phone.width - artSize.width * s) / 2, 0);
          c.scale(s);
          drawArt(c);
          c.restore();
          c.drawCircle(
            _phone.center(Offset.zero),
            84,
            Paint()..color = const Color(0xFF00736C),
          );
        });
        await _writePng(bg, '${out.path}/${name}_screen.png');
      } else if (artSize.height > artSize.width) {
        final hero = _record(_phone, (c) {
          c.drawRect(Offset.zero & _phone, Paint()..color = window);
          drawArt(c);
          c.drawRect(
            Offset.zero & artSize,
            Paint()
              ..shader = ui.Gradient.linear(
                Offset.zero,
                Offset(0, artSize.height),
                [
                  window.withValues(alpha: 0.85),
                  window.withValues(alpha: 0),
                  window.withValues(alpha: 0),
                  window,
                ],
                [0, 0.1, 0.4, 0.74],
              ),
          );
        });
        await _writePng(hero, '${out.path}/${name}_hero.png');
      }
      info.picture.dispose();
      stdout.writeln('${out.path}/$name.png');
    }

    const thumbW = 360.0, gap = 12.0, cols = 4;
    double h(ui.Image t) => thumbW * t.height / t.width;
    final rowHeights = <double>[
      for (var r = 0; r * cols < thumbs.length; r++)
        thumbs.skip(r * cols).take(cols).map(h).reduce((a, b) => a > b ? a : b),
    ];
    final sheet = _record(
      Size(
        cols * (thumbW + gap) + gap,
        rowHeights.fold(gap, (s, rh) => s + rh + gap),
      ),
      (c) {
        c.drawPaint(Paint()..color = const Color(0xFF888888));
        var y = gap;
        for (var i = 0; i < thumbs.length; i++) {
          if (i > 0 && i % cols == 0) y += rowHeights[i ~/ cols - 1] + gap;
          c.drawImageRect(
            thumbs[i],
            Offset.zero &
                Size(thumbs[i].width.toDouble(), thumbs[i].height.toDouble()),
            Rect.fromLTWH(
              gap + (i % cols) * (thumbW + gap),
              y,
              thumbW,
              h(thumbs[i]),
            ),
            Paint()..filterQuality = FilterQuality.medium,
          );
        }
      },
    );
    await _writePng(sheet, '${out.path}/contact_sheet.png');
  });
}
