import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/theme/app_theme_host.dart';

void main() {
  testWidgets('the transparent nav bar style wins over MaterialApp\'s', (
    tester,
  ) async {
    final sent = <Map<Object?, Object?>>[];
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.platform,
      (call) async {
        if (call.method == 'SystemChrome.setSystemUIOverlayStyle') {
          sent.add(call.arguments as Map<Object?, Object?>);
        }
        return null;
      },
    );
    addTearDown(
      () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        null,
      ),
    );

    for (final mode in [ThemeMode.light, ThemeMode.dark]) {
      await tester.pumpWidget(
        AppThemeHost(
          themeMode: mode,
          child: MaterialApp(
            themeMode: mode,
            darkTheme: ThemeData.dark(),
            home: const Scaffold(),
          ),
        ),
      );
      await tester.pump();
      final last = sent.last;
      expect(last['systemNavigationBarColor'], 0x00000000, reason: '$mode');
      expect(
        last['systemNavigationBarIconBrightness'],
        mode == ThemeMode.dark ? 'Brightness.light' : 'Brightness.dark',
        reason: '$mode',
      );
    }
  });
}
