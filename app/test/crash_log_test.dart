import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/diagnostics/crash_log.dart';
import 'package:zafe/src/core/diagnostics/scrub.dart';

void main() {
  group('scrubDiagnostics', () {
    test('removes addresses, keys, links, amounts and ids', () {
      const hex =
          '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef';
      final out = scrubDiagnostics(
        'failed for utest1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq '
        'key $hex at https://relay.example/v1/x?t=secret '
        'paying 1.5 TAZ vault 123e4567-e89b-12d3-a456-426614174000 '
        'zafe-invite-v1:abcd /data/user/0/xyz.zafe.zafe/files/vaults/abc/signing '
        'zcash:utest1abc?amount=1',
      );
      expect(out, isNot(contains('utest1')));
      expect(out, isNot(contains(hex)));
      expect(out, isNot(contains('relay.example')));
      expect(out, isNot(contains('1.5')));
      expect(out, isNot(contains('123e4567')));
      expect(out, isNot(contains('abcd')));
      expect(out, isNot(contains('/vaults/abc')));
      expect(out, contains('<address>'));
      expect(out, contains('<hex>'));
      expect(out, contains('<url>'));
      expect(out, contains('<amount>'));
    });

    test('keeps ordinary text', () {
      expect(
        scrubDiagnostics('RangeError: index out of range at line 12'),
        'RangeError: index out of range at line 12',
      );
    });
  });

  group('CrashLog', () {
    late Directory dir;
    setUp(() => dir = Directory.systemTemp.createTempSync('crashlog'));
    tearDown(() => dir.deleteSync(recursive: true));

    test('records scrubbed entries and keeps only the latest', () async {
      final log = CrashLog(File('${dir.path}/sub/crashes.log'));
      await log.record('flutter', StateError('paid 2 ZEC'), StackTrace.current);
      var entries = await log.read();
      expect(entries, hasLength(1));
      expect(entries.single, contains('<amount>'));
      expect(entries.single, isNot(contains('2 ZEC')));

      for (var i = 0; i < CrashLog.maxEntries + 5; i++) {
        await log.record('uncaught', StateError('e$i'));
      }
      entries = await log.read();
      expect(entries, hasLength(CrashLog.maxEntries));
      expect(entries.last, contains('e${CrashLog.maxEntries + 4}'));
    });

    test('report has no identifiers and clear empties it', () async {
      final log = CrashLog(File('${dir.path}/crashes.log'));
      await log.record('uncaught', StateError('boom'));
      final report = await log.report(network: 'test');
      expect(report, contains('Zafe diagnostic report'));
      expect(report, contains('boom'));
      await log.clear();
      expect(await log.read(), isEmpty);
      expect(await log.report(network: 'test'), contains('No errors recorded'));
    });

    test('an unwritable path never throws', () async {
      final log = CrashLog(File('/proc/nope/crashes.log'));
      await log.record('uncaught', StateError('x'));
    });
  });
}
