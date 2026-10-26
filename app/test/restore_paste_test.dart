import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/theme/app_theme.dart';
import 'package:zafe/src/core/widgets/clipboard_countdown.dart';
import 'package:zafe/src/features/backup/restore_screen.dart';

/// The system clipboard as `Clipboard.getData` sees it.
void _clipboard(WidgetTester tester, String? text) {
  tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
    SystemChannels.platform,
    (call) async => call.method == 'Clipboard.getData'
        ? (text == null ? null : {'text': text})
        : null,
  );
  addTearDown(
    () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.platform,
      null,
    ),
  );
}

Widget _app(Widget home) => ProviderScope(
  child: MaterialApp(
    builder: (context, child) =>
        AppTheme(data: AppThemeData.dark, child: child!),
    home: home,
  ),
);

void main() {
  testWidgets('Paste with an empty clipboard says so (it used to do nothing)', (
    tester,
  ) async {
    tester.view.devicePixelRatio = 3;
    tester.view.physicalSize = const Size(400 * 3, 900 * 3);
    addTearDown(tester.view.reset);
    _clipboard(tester, null);
    await tester.pumpWidget(_app(const RestoreScreen()));
    await tester.tap(find.text('Paste'));
    await tester.pump();
    expect(
      find.text(
        'Nothing on the clipboard. Copy the backup text again, or choose the file.',
      ),
      findsOneWidget,
    );
  });

  testWidgets('Paste keeps text that is not a backup out of the field', (
    tester,
  ) async {
    _clipboard(tester, 'my bank password');
    final pasted = await pastedBackupText();
    expect(pasted.text, isNull);
    expect(pasted.problem, startsWith("That isn't backup text"));
  });

  testWidgets('Paste takes backup text, trimmed', (tester) async {
    _clipboard(tester, '  zafe-backup-v1:WkFGRUJBSw  \n');
    final pasted = await pastedBackupText();
    expect(pasted.text, 'zafe-backup-v1:WkFGRUJBSw');
    expect(pasted.problem, isNull);
  });

  testWidgets('the countdown shows the seconds left, then that it is gone', (
    tester,
  ) async {
    await tester.pumpWidget(
      _app(
        ClipboardCountdown(
          copiedAt: DateTime.now().subtract(const Duration(seconds: 18)),
        ),
      ),
    );
    expect(find.text('On the clipboard for 42 s'), findsOneWidget);
    await tester.pumpWidget(
      _app(
        ClipboardCountdown(
          copiedAt: DateTime.now().subtract(const Duration(seconds: 61)),
        ),
      ),
    );
    expect(find.text('No longer on the clipboard'), findsOneWidget);
  });
}
