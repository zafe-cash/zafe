// Renders the "Use Tor" sheet in each state, in both themes, to PNGs for review without
// a device. Not part of `flutter test`; run it explicitly (from app/):
//
//   flutter test tool/screens/tor_render_test.dart
//
// Output (SCREEN_PREVIEW_OUT, default build/screen_preview/): tor_<state>_<theme>.png.
import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/network/tor_setting.dart';
import 'package:zafe/src/core/theme/app_theme.dart';
import 'package:zafe/src/features/settings/tor_setting_row.dart';
import 'package:zafe/src/providers/tor_provider.dart';

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

class _Fixed extends TorNotifier {
  _Fixed(this.setting);
  final TorSetting setting;

  @override
  TorSetting build() => setting;
}

const _states = {
  'off': TorSetting.off(),
  'connecting': TorSetting(enabled: true, connection: TorConnection.connecting),
  'connected': TorSetting(enabled: true, connection: TorConnection.connected),
  'failed': TorSetting(
    enabled: true,
    connection: TorConnection.failed,
    error: 'Tor did not connect in time',
  ),
};

void main() {
  testWidgets('render the Tor sheet', (tester) async {
    await tester.runAsync(_loadFonts);
    final out = Directory(
      Platform.environment['SCREEN_PREVIEW_OUT'] ?? 'build/screen_preview',
    )..createSync(recursive: true);
    tester.view.devicePixelRatio = 3;
    tester.view.physicalSize = const Size(390 * 3, 160 * 3);
    addTearDown(tester.view.reset);

    for (final MapEntry(key: name, value: setting) in _states.entries) {
      for (final (theme, data) in [
        ('dark', AppThemeData.dark),
        ('light', AppThemeData.light),
      ]) {
        final boundary = GlobalKey();
        await tester.pumpWidget(
          ProviderScope(
            // A new scope per state: overrides can't change on a live one.
            key: ValueKey('$name-$theme'),
            overrides: [torProvider.overrideWith(() => _Fixed(setting))],
            child: MaterialApp(
              debugShowCheckedModeBanner: false,
              home: RepaintBoundary(
                key: boundary,
                child: AppTheme(
                  data: data,
                  child: Builder(
                    builder: (context) => ColoredBox(
                      color: context.colors.background.window,
                      child: Align(
                        alignment: Alignment.center,
                        child: Padding(
                          padding: const EdgeInsets.all(16),
                          child: Material(
                            type: MaterialType.transparency,
                            child: TorSettingRow(
                              rowStyle: AppTypography.labelLarge.copyWith(
                                color: context.colors.text.accent,
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
          ),
        );
        for (var i = 0; i < 5; i++) {
          await tester.runAsync(
            () => Future<void>.delayed(const Duration(milliseconds: 50)),
          );
          await tester.pump(const Duration(milliseconds: 100));
        }
        final render =
            boundary.currentContext!.findRenderObject()!
                as RenderRepaintBoundary;
        final bytes = await tester.runAsync(() async {
          final image = await render.toImage(pixelRatio: 2);
          final data = await image.toByteData(format: ui.ImageByteFormat.png);
          return data!.buffer.asUint8List();
        });
        final path = '${out.path}/tor_${name}_$theme.png';
        File(path).writeAsBytesSync(bytes!);
        // ignore: avoid_print
        print(path);
      }
    }
  });
}
