import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/theme/app_theme.dart';
import 'package:zafe/src/core/widgets/app_icon.dart';
import 'package:zafe/src/core/widgets/app_menu.dart';

const _margin = 16.0;

/// Opens an [AppMenu] whose 40x40 anchor sits at [anchorAt] on a [screen]-sized phone.
Future<List<String>> _open(
  WidgetTester tester, {
  Size screen = const Size(360, 640),
  Offset anchorAt = const Offset(288, 80),
  double textScale = 1,
  List<String> labels = const ['Rename', 'Remove'],
}) async {
  tester.view.devicePixelRatio = 3;
  tester.view.physicalSize = screen * 3;
  addTearDown(tester.view.reset);
  final tapped = <String>[];
  await tester.pumpWidget(
    MaterialApp(
      builder: (context, child) => MediaQuery(
        data: MediaQuery.of(
          context,
        ).copyWith(textScaler: TextScaler.linear(textScale)),
        child: AppTheme(data: AppThemeData.light, child: child!),
      ),
      home: Scaffold(
        body: Stack(
          children: [
            Positioned(
              left: anchorAt.dx,
              top: anchorAt.dy,
              child: AppMenu(
                items: [
                  for (final l in labels)
                    AppMenuItem(
                      label: l,
                      icon: AppIcons.edit,
                      onTap: () => tapped.add(l),
                    ),
                ],
                builder: (context, open) => GestureDetector(
                  behavior: HitTestBehavior.opaque,
                  onTap: open,
                  child: const SizedBox(
                    key: Key('anchor'),
                    width: 40,
                    height: 40,
                  ),
                ),
              ),
            ),
          ],
        ),
      ),
    ),
  );
  await tester.tap(find.byKey(const Key('anchor')));
  await tester.pumpAndSettle();
  return tapped;
}

Rect _anchor(WidgetTester tester) =>
    tester.getRect(find.byKey(const Key('anchor')));

/// The menu's items, top to bottom: their union is the menu less its padding.
Rect _items(WidgetTester tester) => tester
    .widgetList<MenuItemButton>(find.byType(MenuItemButton))
    .map((w) => tester.getRect(find.byWidget(w)))
    .reduce((a, b) => a.expandToInclude(b));

void main() {
  testWidgets('hangs from the anchor\'s right edge, just below it', (
    tester,
  ) async {
    await _open(tester);
    final anchor = _anchor(tester);
    final menu = _items(tester);
    // MenuAnchor started it at the anchor and pushed it against the screen edge.
    // The items sit inside the panel's 1 px border.
    expect(menu.right, moreOrLessEquals(anchor.right - 1));
    expect(menu.top, greaterThan(anchor.bottom));
    expect(menu.top - anchor.bottom, lessThan(16));
  });

  for (final screen in const [
    Size(320, 568),
    Size(360, 640),
    Size(800, 1280),
  ]) {
    testWidgets('stays inside the margins on $screen at 2x text', (
      tester,
    ) async {
      await _open(
        tester,
        screen: screen,
        anchorAt: Offset(screen.width - 72, 80),
        textScale: 2,
        // Wider than a small phone in the test font: it wraps instead.
        labels: const ['Rename', 'Remove from this phone'],
      );
      final menu = _items(tester);
      expect(menu.left, greaterThanOrEqualTo(_margin));
      expect(menu.right, lessThanOrEqualTo(screen.width - _margin));
      expect(tester.takeException(), isNull); // no overflow
    });
  }

  testWidgets('an anchor at the screen edge keeps the menu inside the margin', (
    tester,
  ) async {
    await _open(tester, anchorAt: const Offset(320, 80));
    expect(_items(tester).right, lessThanOrEqualTo(360 - _margin));
  });

  testWidgets('opens upward when there is no room below', (tester) async {
    await _open(tester, anchorAt: const Offset(288, 560));
    expect(_items(tester).bottom, lessThan(_anchor(tester).top));
  });

  testWidgets(
    'picking an item closes the menu and acts; outside taps close it',
    (tester) async {
      final tapped = await _open(tester);
      await tester.tap(find.text('Remove'));
      await tester.pumpAndSettle();
      expect(tapped, ['Remove']);
      expect(find.byType(MenuItemButton), findsNothing);

      await tester.tap(find.byKey(const Key('anchor')));
      await tester.pumpAndSettle();
      expect(find.byType(MenuItemButton), findsNWidgets(2));
      await tester.tapAt(const Offset(20, 600));
      await tester.pumpAndSettle();
      expect(find.byType(MenuItemButton), findsNothing);
    },
  );

  testWidgets('Back closes an open menu and stays on the screen', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        builder: (context, child) =>
            AppTheme(data: AppThemeData.light, child: child!),
        home: Builder(
          builder: (context) => GestureDetector(
            onTap: () => Navigator.of(context).push(
              MaterialPageRoute<void>(
                builder: (_) => Scaffold(
                  body: Center(
                    child: AppMenu(
                      items: [AppMenuItem(label: 'Rename', onTap: () {})],
                      builder: (context, open) => GestureDetector(
                        behavior: HitTestBehavior.opaque,
                        onTap: open,
                        child: const SizedBox(
                          key: Key('anchor'),
                          width: 40,
                          height: 40,
                        ),
                      ),
                    ),
                  ),
                ),
              ),
            ),
            child: const ColoredBox(color: Color(0xFFFFFFFF)),
          ),
        ),
      ),
    );
    await tester.tapAt(const Offset(10, 10));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const Key('anchor')));
    await tester.pumpAndSettle();
    expect(find.text('Rename'), findsOneWidget);

    await tester.binding.handlePopRoute(); // Android Back
    await tester.pumpAndSettle();
    expect(find.text('Rename'), findsNothing);
    expect(
      find.byKey(const Key('anchor')),
      findsOneWidget,
    ); // still on the page

    await tester.binding.handlePopRoute(); // now Back leaves
    await tester.pumpAndSettle();
    expect(find.byKey(const Key('anchor')), findsNothing);
  });
}
