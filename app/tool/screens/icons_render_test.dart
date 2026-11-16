import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/theme/app_theme.dart';
import 'package:zafe/src/core/theme/material_theme.dart';
import 'package:zafe/src/core/widgets/app_button.dart';
import 'package:zafe/src/core/widgets/app_icon.dart';

const _all = <String>[
  AppIcons.addNew,
  AppIcons.arrowDown,
  AppIcons.arrowDownCircle,
  AppIcons.book,
  AppIcons.cameraDenied,
  AppIcons.cancel,
  AppIcons.check,
  AppIcons.checkCircle,
  AppIcons.chevronBackward,
  AppIcons.chevronForward,
  AppIcons.cog,
  AppIcons.copy,
  AppIcons.cross,
  AppIcons.day,
  AppIcons.edit,
  AppIcons.editFilled,
  AppIcons.endpoint,
  AppIcons.eye,
  AppIcons.eyeClosed,
  AppIcons.globe,
  AppIcons.help,
  AppIcons.history,
  AppIcons.home,
  AppIcons.importWallet,
  AppIcons.key,
  AppIcons.link,
  AppIcons.loader,
  AppIcons.lock,
  AppIcons.monitor,
  AppIcons.night,
  AppIcons.plane,
  AppIcons.plus,
  AppIcons.qr,
  AppIcons.renew,
  AppIcons.share,
  AppIcons.shieldKeyhole,
  AppIcons.theme,
  AppIcons.time,
  AppIcons.tor,
  AppIcons.trash,
  AppIcons.unlock,
  AppIcons.user,
  AppIcons.users,
  AppIcons.wallet,
  AppIcons.warning,
  AppIcons.warningCircle,
  AppIcons.zcashCurrency,
];

Future<void> _loadFonts() async {
  final families = <String, List<String>>{
    'DM Sans': ['Regular', 'Medium', 'SemiBold'],
    'JetBrains Mono': ['Regular'],
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

Widget _sheet() => Builder(
  builder: (context) {
    final colors = context.colors;
    final label = AppTypography.labelSmall.copyWith(
      color: colors.text.secondary,
      fontFamily: 'JetBrains Mono',
      fontSize: 9,
    );
    Widget cell(String name) => SizedBox(
      width: 88,
      height: 84,
      child: Column(
        children: [
          Row(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              AppIcon(name, size: 28, animated: false),
              const SizedBox(width: 6),
              AppIcon(name, size: 20, animated: false),
              const SizedBox(width: 6),
              AppIcon(
                name,
                size: 20,
                active: true,
                color: colors.icon.brand,
                animated: false,
              ),
            ],
          ),
          const SizedBox(height: 6),
          Text(name, style: label, textAlign: TextAlign.center),
        ],
      ),
    );
    Widget tile(String name, Color bg, Color fg, {bool active = false}) =>
        Container(
          width: 40,
          height: 40,
          margin: const EdgeInsets.only(right: 8),
          decoration: BoxDecoration(color: bg, shape: BoxShape.circle),
          alignment: Alignment.center,
          child: AppIcon(name, size: 18, color: fg, active: active),
        );
    return Material(
      color: colors.background.window,
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Wrap(children: [for (final n in _all) cell(n)]),
            const SizedBox(height: 12),
            Row(
              children: [
                AppIcon(AppIcons.warning, size: 24, color: colors.icon.warning),
                const SizedBox(width: 8),
                AppIcon(
                  AppIcons.warningCircle,
                  size: 24,
                  color: colors.icon.destructive,
                ),
                const SizedBox(width: 8),
                AppIcon(
                  AppIcons.trash,
                  size: 24,
                  color: colors.icon.destructive,
                ),
                const SizedBox(width: 8),
                AppIcon(
                  AppIcons.checkCircle,
                  size: 24,
                  color: colors.icon.value,
                ),
                const SizedBox(width: 8),
                AppIcon(AppIcons.lock, size: 24, color: colors.icon.muted),
                const SizedBox(width: 16),
                tile(
                  AppIcons.editFilled,
                  colors.background.brandAlpha,
                  colors.icon.brand,
                  active: true,
                ),
                tile(
                  AppIcons.arrowDown,
                  colors.background.valueAlpha,
                  colors.text.value,
                ),
                tile(
                  AppIcons.time,
                  colors.background.neutralSubtleOpacity,
                  colors.icon.regular,
                ),
              ],
            ),
            const SizedBox(height: 12),
            Row(
              children: [
                Expanded(
                  child: AppButton(
                    expand: true,
                    onPressed: () {},
                    leading: const AppIcon(AppIcons.copy, size: 20),
                    child: const Text('Copy'),
                  ),
                ),
                const SizedBox(width: 8),
                Expanded(
                  child: AppButton(
                    expand: true,
                    variant: AppButtonVariant.secondary,
                    onPressed: () {},
                    leading: const AppIcon(AppIcons.qr, size: 20),
                    child: const Text('Scan'),
                  ),
                ),
              ],
            ),
            const SizedBox(height: 8),
            Row(
              children: [
                Expanded(
                  child: AppButton(
                    expand: true,
                    variant: AppButtonVariant.ghost,
                    onPressed: () {},
                    leading: const AppIcon(AppIcons.importWallet, size: 20),
                    child: const Text('Paste'),
                  ),
                ),
                const SizedBox(width: 8),
                Expanded(
                  child: AppButton(
                    expand: true,
                    variant: AppButtonVariant.destructive,
                    onPressed: () {},
                    leading: const AppIcon(AppIcons.trash, size: 20),
                    child: const Text('Remove'),
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  },
);

void main() {
  testWidgets('render icon contact sheet', (tester) async {
    await tester.runAsync(_loadFonts);
    final out = Directory(
      Platform.environment['SCREEN_PREVIEW_OUT'] ?? 'build/screen_preview',
    )..createSync(recursive: true);
    tester.view.devicePixelRatio = 3;
    tester.view.physicalSize = const Size(390 * 3, 1300 * 3);
    addTearDown(tester.view.reset);

    for (final (theme, data) in [
      ('dark', AppThemeData.dark),
      ('light', AppThemeData.light),
    ]) {
      final boundary = GlobalKey();
      await tester.pumpWidget(
        MaterialApp(
          debugShowCheckedModeBanner: false,
          theme: theme == 'dark'
              ? buildMaterialDarkTheme()
              : buildMaterialLightTheme(),
          home: RepaintBoundary(
            key: boundary,
            child: AppTheme(data: data, child: _sheet()),
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
          boundary.currentContext!.findRenderObject()! as RenderRepaintBoundary;
      final bytes = await tester.runAsync(() async {
        final image = await render.toImage(pixelRatio: 2);
        final data = await image.toByteData(format: ui.ImageByteFormat.png);
        return data!.buffer.asUint8List();
      });
      final path = '${out.path}/icons_$theme.png';
      File(path).writeAsBytesSync(bytes!);
      stdout.writeln(path);
    }
  });
}
