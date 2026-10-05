// Renders the Zcash server list (mainnet presets, with each kind of check result), in both
// themes, to PNGs for review without a device. Not part of `flutter test`; run it
// explicitly (from app/):
//
//   flutter test tool/screens/servers_render_test.dart
//
// Output (SCREEN_PREVIEW_OUT, default build/screen_preview/): servers_<theme>.png.
import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/config/lightwalletd_presets.dart';
import 'package:zafe/src/core/layout/mobile/app_mobile_sheet.dart';
import 'package:zafe/src/core/theme/app_theme.dart';
import 'package:zafe/src/features/settings/endpoint_sheet.dart';
import 'package:zafe/src/providers/server_failover_provider.dart';

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

ServerProbe _ms(int ms) =>
    ServerProbe(ServerCheck.ok, Duration(milliseconds: ms));

final _probes = <String, ServerProbe>{
  'https://zec.rocks:443': _ms(84),
  'https://na.zec.rocks:443': _ms(142),
  'https://eu.zec.rocks:443': _ms(61),
  'https://ap.zec.rocks:443': _ms(233),
  'https://sa.zec.rocks:443': const ServerProbe(ServerCheck.unavailable),
  'https://us.zec.stardust.rest:443': _ms(158),
  // eu.zec.stardust.rest still checking.
  'https://lwd.zcashexplorer.app:9067': _ms(190),
};

void main() {
  testWidgets('render the Zcash server list', (tester) async {
    await tester.runAsync(_loadFonts);
    final out = Directory(
      Platform.environment['SCREEN_PREVIEW_OUT'] ?? 'build/screen_preview',
    )..createSync(recursive: true);
    tester.view.devicePixelRatio = 3;
    tester.view.physicalSize = const Size(390 * 3, 760 * 3);
    addTearDown(tester.view.reset);

    for (final (theme, data) in [
      ('dark', AppThemeData.dark),
      ('light', AppThemeData.light),
    ]) {
      final boundary = GlobalKey();
      await tester.pumpWidget(
        MaterialApp(
          key: ValueKey(theme),
          debugShowCheckedModeBanner: false,
          home: RepaintBoundary(
            key: boundary,
            child: AppTheme(
              data: data,
              child: Builder(
                builder: (context) => ColoredBox(
                  color: context.colors.background.window,
                  child: Align(
                    alignment: Alignment.bottomCenter,
                    child: Padding(
                      padding: const EdgeInsets.all(16),
                      child: Material(
                        type: MaterialType.transparency,
                        child: MobileModalScaffold(
                          title: 'Zcash server',
                          onClose: () {},
                          child: LightwalletdServerList(
                            presets: kMainnetLightwalletdPresets,
                            selectedUrl: 'https://eu.zec.rocks:443',
                            probes: _probes,
                            onSelect: (_) {},
                            onCustom: () {},
                          ),
                        ),
                      ),
                    ),
                  ),
                ),
              ),
            ),
          ),
        ),
      );
      await tester.pump(const Duration(milliseconds: 100));
      final render =
          boundary.currentContext!.findRenderObject()! as RenderRepaintBoundary;
      final bytes = await tester.runAsync(() async {
        final image = await render.toImage(pixelRatio: 2);
        final data = await image.toByteData(format: ui.ImageByteFormat.png);
        return data!.buffer.asUint8List();
      });
      final path = '${out.path}/servers_$theme.png';
      File(path).writeAsBytesSync(bytes!);
      // ignore: avoid_print
      print(path);
    }
  });
}
