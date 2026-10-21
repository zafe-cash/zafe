// Renders the screens and sheets added for the security fixes and onboarding (2026-10-15)
// on small to large phones and at larger system text sizes. Fails on any layout overflow
// (Flutter reports it as an error), and writes the smallest and largest-text renders to
// look at. Not part of `flutter test`; run it explicitly (from app/):
//
//   flutter test tool/screens/size_sweep_test.dart
//
// Output (SCREEN_PREVIEW_OUT, default build/screen_preview/): sweep_<case>_<size>.png.
import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/layout/mobile/zafe_screen.dart';
import 'package:zafe/src/core/theme/app_theme.dart';
import 'package:zafe/src/core/widgets/confirm_option.dart';
import 'package:zafe/src/core/widgets/mobile/mobile_list_row.dart';
import 'package:zafe/src/core/widgets/mobile/zafe_detail.dart';
import 'package:zafe/src/core/widgets/mobile_text_field.dart';
import 'package:zafe/src/features/home/needs_you.dart' show RequestCard;
import 'package:zafe/src/features/home/home_screen.dart'
    show BalanceCard, NoticeCard;
import 'package:zafe/src/features/backup/export_screen.dart';
import 'package:zafe/src/features/onboarding/setup_screen.dart';
import 'package:zafe/src/features/received/received_row.dart';
import 'package:zafe/src/features/recover/recover_screen.dart';
import 'package:zafe/src/features/signers/replace_signer_screen.dart';
import 'package:zafe/src/features/signers/signers_screen.dart';
import 'package:zafe/src/notifications/notification_primer.dart';
import 'package:zafe/src/rust/api/received.dart' show ReceivedInfo;
import 'package:zafe/src/rust/api/repair.dart' show CosignerWords;

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

const _key = '97c9ce4b0d2f6a1e8c3b5d7f9a1c3e5f7b9d1f3a5c7e9b1d3f5a7c9e1b3d123e';
const _key2 =
    '24bafbab373b21c99c7ea2e902f0034ec45a8c27a87f1880090043a3f458ec55';
const _code =
    'zafe-recover-v1:2c9f04d1a7e8b3c56d0f1e2a3b4c5d6e7f8091a2b3c4d5e6f708192a3b4c5d6e7f'
    '8a9b0c1d2e3f405162738495a6b7c8d9e0f1a2b3c4d5e6f708192a3b4c5d6e7f8091';

/// (name, page title or null for a sheet, content)
final _cases = <(String, String?, Widget)>[
  (
    'setup_words',
    'Grants',
    const Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SetupSafetyWordsCard(words: 'regular fancy about business', waiting: 0),
        SizedBox(height: 12),
        SetupSafetyWordsCard(words: null, waiting: 2),
        SizedBox(height: 12),
        ConfirmOption(
          selected: true,
          label: 'I compared these words with every member',
          onTap: _noop,
        ),
      ],
    ),
  ),
  ('setup_waiting', 'Grants', Text(waitingText([_key, _key2]))),
  (
    'recover',
    'Recover your seat',
    const RecoverBody(
      code: _code,
      checks: [
        CosignerWords(cosignerKeyHex: _key, words: 'guilt afford pear gasp'),
        CosignerWords(cosignerKeyHex: _key2, words: 'say green fiction stick'),
      ],
    ),
  ),
  (
    'replace',
    'Replace lost phone',
    const Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SeatMoveCard(
          who: 'Alexandra',
          approvals: 1,
          needed: 2,
          onApprove: _noop,
        ),
        SizedBox(height: 12),
        SafetyWordsCard(words: null, who: 'Alexandra'),
        SizedBox(height: 12),
        SafetyWordsCard(
          words: 'ancient cinnamon zebra legend',
          who: 'Alexandra',
          myKeyHex: _key,
          confirmed: true,
        ),
      ],
    ),
  ),
  (
    'home_notices',
    'Home',
    const Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        NoticeCard(
          title: 'Back up this vault',
          body: 'It holds money, and your key share lives only on this phone',
          onTap: _noop,
        ),
        SizedBox(height: 12),
        NoticeCard(
          title: 'Notifications are off',
          body: 'You won\'t hear when a payment needs your approval',
          onTap: _noop,
        ),
      ],
    ),
  ),
  (
    'settings_rows',
    'Settings',
    const Column(
      children: [
        MobileListRow(
          label: 'Require unlock to approve',
          value: 'On',
          showChevron: true,
        ),
        MobileListRow(
          label: 'Lock app',
          value: 'After 1 minute',
          showChevron: true,
        ),
        MobileListRow(label: 'Approval rule', value: '2 of 3 signers'),
      ],
    ),
  ),
  (
    'payment',
    'Review payment',
    const Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        PaymentCard(
          amountText: '2.00',
          address: 'uregtest10jal7369n7asxnqyrgq84c4cyl8s',
          onFullAddress: _noop,
        ),
        SizedBox(height: 12),
        DetailRow(label: 'Network fee', value: '0.0001 TAZ', onHelp: _noop),
        DetailRow(
          label: 'Checked on this phone',
          value: 'Matches',
          onHelp: _noop,
        ),
        DetailRow(label: 'Approvals needed', value: '2 of 3 signers'),
        TxIdRow(
          txid:
              '10616fc1aa0000000000000000000000000000000000000000000000c84a8447',
        ),
      ],
    ),
  ),
  (
    'fields',
    'Back up vault',
    Column(
      children: [
        MobileTextField(
          controller: TextEditingController(),
          focusNode: FocusNode(),
          hintText: 'Passphrase',
          trailing: const Text('Suggest'),
        ),
        const SizedBox(height: 12),
        MobileTextField(
          controller: TextEditingController(text: 'uregtest10jal7369n7asx'),
          focusNode: FocusNode(),
          leading: const Icon(Icons.send, size: 20),
          trailing: const Text('Clear'),
        ),
      ],
    ),
  ),
  (
    'vault_card',
    'Home',
    BalanceCard(
      totalZat: BigInt.from(2899990000),
      fiatText: null,
      confirmingText: '3.9999 TAZ confirming',
      hidden: false,
      onToggle: _noop,
      threshold: 2,
      members: 3,
    ),
  ),
  (
    'activity_row',
    'Activity',
    ReceivedRow(
      received: ReceivedInfo(
        txid: 'e88aa577',
        amountZat: BigInt.from(500000000),
        minedHeight: 114,
        blockTimeSecs: 1296822960,
        confirmations: 13,
        memo: '',
        isCoinbase: false,
      ),
      onTap: _noop,
    ),
  ),
  (
    'request_cards',
    'Home',
    const Column(
      children: [
        RequestCard(
          tileKey: _key,
          who: 'd05aee...bd79',
          amount: '0.8',
          detail: 'Printer ink',
          action: 'Approve',
          onTap: _noop,
          onDecline: _noop,
        ),
        SizedBox(height: 12),
        RequestCard(
          tileKey: _key2,
          who: 'Treasurer Alice',
          amount: '12,450.25',
          detail: 'Q4 grants round, second tranche',
          action: 'Send now',
          onTap: _noop,
        ),
      ],
    ),
  ),
  (
    'creating_keys',
    null,
    const CreatingKeysScreen(
      waiting: 'Waiting for 24bafb...ec55 and dccacc...b50b.',
    ),
  ),
  ('sheet_primer', null, const NotificationPrimerSheet()),
  (
    'backup_words',
    null,
    // In ZafeScreen's ListView, as in the app: tall with large text, so it scrolls.
    ListView(
      padding: const EdgeInsets.all(16),
      children: const [
        BackupWordGrid(
          words: [
            'abandon',
            'category',
            'disorder',
            'electric',
            'february',
            'general', //
            'happy', 'industry', 'journey', 'kangaroo', 'language', 'mushroom',
          ],
        ),
      ],
    ),
  ),
];

/// Logical sizes (width x height) and the text scales tried on each.
const _sizes = <(String, Size, List<double>)>[
  ('320x568', Size(320, 568), [1.0, 1.3, 2.0]),
  ('360x640', Size(360, 640), [1.0, 1.3, 2.0]),
  ('390x844', Size(390, 844), [1.0, 1.3]),
  ('412x915', Size(412, 915), [1.0, 1.3]),
];

void main() {
  testWidgets('new screens fit small to large phones and large text', (
    tester,
  ) async {
    await tester.runAsync(_loadFonts);
    final out = Directory(
      Platform.environment['SCREEN_PREVIEW_OUT'] ?? 'build/screen_preview',
    )..createSync(recursive: true);
    tester.view.devicePixelRatio = 2;
    addTearDown(tester.view.reset);
    final problems = <String>[];

    for (final (sizeName, size, scales) in _sizes) {
      tester.view.physicalSize = size * 2;
      for (final scale in scales) {
        for (final (name, title, body) in _cases) {
          final boundary = GlobalKey();
          final errors = <FlutterErrorDetails>[];
          final previous = FlutterError.onError;
          FlutterError.onError = errors.add;
          await tester.pumpWidget(
            RepaintBoundary(
              key: boundary,
              child: MaterialApp(
                debugShowCheckedModeBanner: false,
                builder: (context, child) => MediaQuery(
                  data: MediaQuery.of(
                    context,
                  ).copyWith(textScaler: TextScaler.linear(scale)),
                  child: AppTheme(data: AppThemeData.light, child: child!),
                ),
                home: title == null
                    ? Scaffold(
                        backgroundColor: Colors.black54,
                        // As the sheet sits on a phone: the screen's height and no
                        // scrolling but its own.
                        body: Align(
                          alignment: Alignment.bottomCenter,
                          child: body,
                        ),
                      )
                    : ZafeScreen(
                        title: title,
                        showBack: false,
                        children: [body],
                      ),
              ),
            ),
          );
          await tester.pump(const Duration(milliseconds: 100));
          FlutterError.onError = previous;
          for (final e in errors) {
            problems.add('$name $sizeName x$scale: ${e.exceptionAsString()}');
          }
          final keep =
              (sizeName == '320x568' && scale == 2.0) ||
              (sizeName == '360x640' && scale == 1.3);
          if (keep || errors.isNotEmpty) {
            final target =
                boundary.currentContext!.findRenderObject()!
                    as RenderRepaintBoundary;
            final bytes = await tester.runAsync(() async {
              final image = await target.toImage(pixelRatio: 1.5);
              final data = await image.toByteData(
                format: ui.ImageByteFormat.png,
              );
              return data!.buffer.asUint8List();
            });
            File(
              '${out.path}/sweep_${name}_${sizeName}_x$scale.png',
            ).writeAsBytesSync(bytes!);
          }
        }
      }
    }
    expect(problems, isEmpty, reason: problems.join('\n'));
  });
}

void _noop() {}
