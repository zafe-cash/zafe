// Renders the welcome screen (with and without a relay), its Server settings sheet and
// the "already in a vault" sheet,
// in both themes, to PNGs for review without a device. Not part of `flutter test`; run it
// explicitly (from app/):
//
//   flutter test tool/screens/welcome_render_test.dart
//
// Output (SCREEN_PREVIEW_OUT, default build/screen_preview/): welcome_<state>_<theme>.png.
import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/config/endpoints.dart';
import 'package:zafe/src/core/config/network_config.dart';
import 'package:zafe/src/core/theme/app_theme.dart';
import 'package:zafe/src/features/onboarding/welcome_screen.dart';
import 'package:zafe/src/providers/endpoints_provider.dart';
import 'package:zafe/src/providers/vault_provider.dart';

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

class _Endpoints extends EndpointsNotifier {
  _Endpoints(this.endpoints);
  final ZafeEndpoints endpoints;

  @override
  ZafeEndpoints build() => endpoints;
}

class _Vault extends VaultNotifier {
  @override
  VaultState build() => const VaultState();
}

const _configured = ZafeEndpoints(
  relayUrl: 'https://relay.example.org',
  lightwalletdUrl: 'https://testnet.zec.rocks:443',
);
const _unset = ZafeEndpoints(
  relayUrl: kPlaceholderRelayUrl,
  lightwalletdUrl: 'https://testnet.zec.rocks:443',
);

void main() {
  testWidgets('render the welcome screen', (tester) async {
    await tester.runAsync(_loadFonts);
    final out = Directory(
      Platform.environment['SCREEN_PREVIEW_OUT'] ?? 'build/screen_preview',
    )..createSync(recursive: true);
    tester.view.devicePixelRatio = 3;
    tester.view.physicalSize = const Size(390 * 3, 844 * 3);
    addTearDown(tester.view.reset);

    for (final (name, endpoints, tap) in [
      ('relay', _configured, null),
      ('norelay', _unset, null),
      ('sheet', _unset, 'Server settings'),
      ('already', _configured, "I'm already in a vault"),
    ]) {
      for (final (theme, data) in [
        ('dark', AppThemeData.dark),
        ('light', AppThemeData.light),
      ]) {
        final boundary = GlobalKey();
        await tester.pumpWidget(
          ProviderScope(
            key: ValueKey('$name-$theme'),
            overrides: [
              endpointsProvider.overrideWith(() => _Endpoints(endpoints)),
              vaultProvider.overrideWith(_Vault.new),
            ],
            // The boundary wraps the whole app, so the sheet's route is captured too.
            child: RepaintBoundary(
              key: boundary,
              child: MaterialApp(
                debugShowCheckedModeBanner: false,
                builder: (context, child) =>
                    AppTheme(data: data, child: child!),
                home: const WelcomeScreen(),
              ),
            ),
          ),
        );
        if (tap != null) await tester.tap(find.bySemanticsLabel(tap));
        for (var i = 0; i < 8; i++) {
          await tester.runAsync(
            () => Future<void>.delayed(const Duration(milliseconds: 50)),
          );
          await tester.pump(const Duration(milliseconds: 100));
        }
        final target =
            boundary.currentContext!.findRenderObject()!
                as RenderRepaintBoundary;
        final bytes = await tester.runAsync(() async {
          final image = await target.toImage(pixelRatio: _scale);
          final data = await image.toByteData(format: ui.ImageByteFormat.png);
          return data!.buffer.asUint8List();
        });
        final path = '${out.path}/welcome_${name}_$theme.png';
        File(path).writeAsBytesSync(bytes!);
        // ignore: avoid_print
        print(path);
      }
    }
  });
}

// SCREEN_PREVIEW_SCALE renders larger (the store listing uses 3).
final _scale = double.parse(
  Platform.environment['SCREEN_PREVIEW_SCALE'] ?? '2',
);
