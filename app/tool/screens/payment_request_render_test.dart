import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/security/app_lock_gate.dart';
import 'package:zafe/src/core/theme/app_theme.dart';
import 'package:zafe/src/features/send/payment_request_screen.dart';
import 'package:zafe/src/rust/api/proposals.dart' as rust;

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

const _a =
    'utest1qzfl8ezprr0xhlwkuyq3yzw7pfgxrkcmqyd2kphkqcvz9l8rm8y4xktzu3j8kq7xz5hdlx6vpfnxe9mlnxdv6hjx0xy0u4ecpgr2l3lwe6whr8lkdmsugx4p5aj3zgcqs6yz9uqv8cc6uas9kr8vshz2lu3dhp6fnp9m7k3rmt0zq0dhq2c2p7gwy3ytwn0cvs50aqyq6k3f5';
const _b =
    'utest1n2prk8zqah8eyz9wsguyfdwf5ul5jx9qmeygd6dhcwn3uyc0rjtyqh0mydjd8a9x6r3wkfg6yjfn7xq52zs04l9d7xp6c08g5a5r9f7ywj4zvjxts7lws0v9q4j8yrh6x0dmrk73fsn6yhjttf2dd2mqvx0lnvg8u56qs2jvm7qszm0g4q0y5ys3fzd6tthk9psw3vxquakzx4h';

rust.ScannedPayment _p(
  String address,
  int zat, {
  String label = '',
  String message = '',
  String memo = '',
}) => rust.ScannedPayment(
  address: address,
  amountZat: BigInt.from(zat),
  memo: memo,
  label: label,
  message: message,
);

const _treasury = PayFromVault(
  id: '6f1a2b3c4d5e6f708192a3b4c5d6e7f8',
  name: 'Treasury',
  threshold: 2,
  members: 3,
  balanceZat: null,
);
final _grants = PayFromVault(
  id: 'c0ffee00112233445566778899aabbcc',
  name: 'Grants committee',
  threshold: 3,
  members: 5,
  balanceZat: BigInt.from(4825000000),
  spendableZat: BigInt.from(4825000000),
);
final _petty = PayFromVault(
  id: '0badc0de0badc0de0badc0de0badc0de',
  name: 'Petty cash',
  threshold: 1,
  members: 2,
  balanceZat: BigInt.from(60000000),
  spendableZat: BigInt.from(60000000),
);

void main() {
  testWidgets('render the payment request page', (tester) async {
    await tester.runAsync(_loadFonts);
    final out = Directory(
      Platform.environment['SCREEN_PREVIEW_OUT'] ?? 'build/screen_preview',
    )..createSync(recursive: true);
    tester.view.devicePixelRatio = 3;
    addTearDown(tester.view.reset);

    PaymentRequestView view({
      required List<rust.ScannedPayment> payments,
      List<PayFromVault>? vaults,
      String? selected,
      bool confirmed = false,
      String problem = '',
    }) => PaymentRequestView(
      payments: payments,
      problem: problem,
      vaults: vaults ?? [_treasury],
      selectedId: selected ?? _treasury.id,
      onSelect: (_) {},
      confirmed: confirmed,
      onConfirm: () {},
      onContinue: () {},
      onDecline: () {},
    );

    final pages = <(String, double, Widget)>[
      (
        'payment_request_single',
        1240,
        view(
          payments: [
            _p(_a, 125000000, label: 'Coffee shop', message: 'Order #42'),
          ],
        ),
      ),
      (
        'payment_request_vaults',
        1640,
        view(
          payments: [
            _p(_a, 250000000, memo: 'Invoice 2026-118'),
            _p(_b, 40000000),
          ],
          vaults: [_treasury, _grants, _petty],
          selected: _grants.id,
          confirmed: true,
        ),
      ),
      ('payment_request_no_amount', 1100, view(payments: [_p(_a, 0)])),
      (
        'payment_request_short',
        1240,
        view(
          payments: [_p(_a, 125000000)],
          vaults: [_petty],
          selected: _petty.id,
          confirmed: true,
        ),
      ),
      (
        'payment_request_problem',
        844,
        view(
          payments: const [],
          problem: 'This address is for another network',
        ),
      ),
      ('lock_idle', 844, LockScreen(onUnlock: () {})),
      (
        'lock_lockedout',
        844,
        LockScreen(
          onUnlock: () {},
          message: 'Too many attempts. Try again later',
        ),
      ),
    ];
    for (final (name, height, body) in pages) {
      tester.view.physicalSize = Size(390 * 3, height * 3);
      for (final (theme, data) in [
        ('dark', AppThemeData.dark),
        ('light', AppThemeData.light),
      ]) {
        final boundary = GlobalKey();
        await tester.pumpWidget(
          RepaintBoundary(
            key: ValueKey('$name-$theme'),
            child: RepaintBoundary(
              key: boundary,
              child: MaterialApp(
                debugShowCheckedModeBanner: false,
                builder: (context, child) =>
                    AppTheme(data: data, child: child!),
                home: body,
              ),
            ),
          ),
        );
        for (var i = 0; i < 6; i++) {
          await tester.runAsync(
            () => Future<void>.delayed(const Duration(milliseconds: 50)),
          );
          await tester.pump(const Duration(milliseconds: 100));
        }
        final target =
            boundary.currentContext!.findRenderObject()!
                as RenderRepaintBoundary;
        final bytes = await tester.runAsync(() async {
          final image = await target.toImage(pixelRatio: 2);
          final data = await image.toByteData(format: ui.ImageByteFormat.png);
          return data!.buffer.asUint8List();
        });
        final path = '${out.path}/${name}_$theme.png';
        File(path).writeAsBytesSync(bytes!);
        stdout.writeln(path);
      }
    }
  });
}
