import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/painting.dart';
import 'package:flutter_svg/flutter_svg.dart';
import 'package:flutter_test/flutter_test.dart';

const _res = 'android/app/src/main/res';
const _ios = 'ios/Runner/Assets.xcassets';
const _densities = {
  'mdpi': 1.0,
  'hdpi': 1.5,
  'xhdpi': 2.0,
  'xxhdpi': 3.0,
  'xxxhdpi': 4.0,
};

final _pictures = <String, PictureInfo>{};

const _tile = Color(0xFF004A46);

Future<PictureInfo> _load(String name) async =>
    _pictures[name] ??= await vg.loadPicture(
      SvgStringLoader(File('tool/brand/svg/$name.svg').readAsStringSync()),
      null,
    );

Future<ui.Image> _render(String name, int px) async {
  final info = await _load(name);
  final recorder = ui.PictureRecorder();
  final canvas = Canvas(recorder)..scale(px / info.size.width);
  canvas.drawPicture(info.picture);
  return recorder.endRecording().toImageSync(px, px);
}

Future<void> _write(ui.Image image, String path) async {
  final data = await image.toByteData(format: ui.ImageByteFormat.png);
  File(path)
    ..parent.createSync(recursive: true)
    ..writeAsBytesSync(data!.buffer.asUint8List());
}

Future<void> _emit(String svg, int px, String path) async =>
    _write(await _render(svg, px), path);

void main() {
  test('render app icon and splash', () async {
    for (final MapEntry(key: dpi, value: scale) in _densities.entries) {
      final dir = '$_res/mipmap-$dpi';
      await _emit('icon_legacy', (48 * scale).round(), '$dir/ic_launcher.png');
      final adaptive = (108 * scale).round();
      await _emit(
        'adaptive_foreground',
        adaptive,
        '$dir/ic_launcher_foreground.png',
      );
      await _emit(
        'adaptive_monochrome',
        adaptive,
        '$dir/ic_launcher_monochrome.png',
      );
      for (final (qualifier, theme) in [('', 'light'), ('-night', 'dark')]) {
        final drawables = '$_res/drawable$qualifier-$dpi';
        await _emit(
          'splash_icon_$theme',
          (288 * scale).round(),
          '$drawables/splash_icon.png',
        );
        await _emit(
          'splash_mark_$theme',
          (160 * scale).round(),
          '$drawables/splash_mark.png',
        );
      }
    }

    const iosIcons = {
      '20x20@1x': 20,
      '20x20@2x': 40,
      '20x20@3x': 60,
      '29x29@1x': 29,
      '29x29@2x': 58,
      '29x29@3x': 87,
      '40x40@1x': 40,
      '40x40@2x': 80,
      '40x40@3x': 120,
      '60x60@2x': 120,
      '60x60@3x': 180,
      '76x76@1x': 76,
      '76x76@2x': 152,
      '83.5x83.5@2x': 167,
      '1024x1024@1x': 1024,
    };
    for (final MapEntry(key: name, value: px) in iosIcons.entries) {
      await _emit('icon', px, '$_ios/AppIcon.appiconset/Icon-App-$name.png');
    }
    for (final (suffix, scale) in [('', 1), ('@2x', 2), ('@3x', 3)]) {
      await _emit(
        'splash_mark_light',
        160 * scale,
        '$_ios/LaunchImage.imageset/LaunchImage$suffix.png',
      );
      await _emit(
        'splash_mark_dark',
        160 * scale,
        '$_ios/LaunchImage.imageset/LaunchImageDark$suffix.png',
      );
    }

    await _write(await _sheet(), 'build/brand_preview/brand_sheet.png');
    stdout.writeln('Preview: build/brand_preview/brand_sheet.png');
  });
}

Future<ui.Image> _sheet() async {
  const w = 1800.0, h = 1500.0;
  final recorder = ui.PictureRecorder();
  final c = Canvas(recorder);
  c.drawRect(
    const Rect.fromLTWH(0, 0, w, h),
    Paint()..color = const Color(0xFF6B6B68),
  );

  Future<void> adaptive(Offset o, double size, Path Function(Rect) mask) async {
    final rect = o & Size(size, size);
    final full = size * 108 / 72;
    final layers = rect.inflate((full - size) / 2);
    c.save();
    c.clipPath(mask(rect));
    c.drawRect(layers, Paint()..color = _tile);
    final img = await _render('adaptive_foreground', full.round());
    c.drawImageRect(
      img,
      Rect.fromLTWH(0, 0, img.width.toDouble(), img.height.toDouble()),
      layers,
      Paint()..filterQuality = FilterQuality.high,
    );
    c.restore();
  }

  Path circle(Rect r) => Path()..addOval(r);
  Path squircle(Rect r) =>
      Path()
        ..addRRect(RRect.fromRectAndRadius(r, Radius.circular(r.width * 0.3)));
  Path square(Rect r) =>
      Path()
        ..addRRect(RRect.fromRectAndRadius(r, Radius.circular(r.width * 0.1)));

  await adaptive(const Offset(40, 40), 320, circle);
  await adaptive(const Offset(400, 40), 320, squircle);
  await adaptive(const Offset(760, 40), 320, square);

  final themed = const Offset(1120, 40) & const Size(320, 320);
  c.drawOval(themed, Paint()..color = const Color(0xFFCFE3E1));
  final mono = await _render('adaptive_monochrome', 480);
  c.drawImageRect(
    mono,
    const Rect.fromLTWH(0, 0, 480, 480),
    themed.inflate(80),
    Paint()
      ..colorFilter = const ColorFilter.mode(
        Color(0xFF003C38),
        BlendMode.srcIn,
      ),
  );

  final ios = await _render('icon', 320);
  c.save();
  c.clipRRect(
    RRect.fromRectAndRadius(
      const Offset(1480, 40) & const Size(280, 280),
      const Radius.circular(62),
    ),
  );
  c.drawImageRect(
    ios,
    const Rect.fromLTWH(0, 0, 320, 320),
    const Offset(1480, 40) & const Size(280, 280),
    Paint()..filterQuality = FilterQuality.high,
  );
  c.restore();
  var x = 40.0;
  for (final s in [24, 32, 48, 72, 96, 144, 192]) {
    final img = await _render('icon_legacy', s);
    c.drawImage(img, Offset(x, 400), Paint());
    x += s + 30;
  }

  for (final (i, theme, bg) in [
    (0, 'dark', const Color(0xFF080B0B)),
    (1, 'light', const Color(0xFFF1F5F5)),
  ]) {
    final frame = Offset(40 + i * 480.0, 640) & const Size(432, 840);
    c.drawRect(frame, Paint()..color = bg);
    final icon = await _render('splash_icon_$theme', (288 * 1.2).round());
    c.drawImage(
      icon,
      frame.center - Offset(icon.width / 2, icon.height / 2),
      Paint(),
    );
  }
  return recorder.endRecording().toImageSync(w.round(), h.round());
}
