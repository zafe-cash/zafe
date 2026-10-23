import 'dart:async';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/notifications/check_lock.dart';

void main() {
  late Directory dir;
  setUp(() => dir = Directory.systemTemp.createTempSync('check_lock'));
  tearDown(() => dir.deleteSync(recursive: true));

  test('a check that arrives mid-check makes the running one repeat', () async {
    final runs = <String?>[];
    final first = Completer<void>();
    final running = CheckLock(dir).run('a', (skip) async {
      runs.add(skip);
      if (runs.length == 1) await first.future;
    });
    await Future<void>.delayed(const Duration(milliseconds: 50));
    // Two pushes while the first check runs: one repeat, not two, and no lost news.
    await CheckLock(dir).run('a', (_) async => runs.add('second'));
    await CheckLock(dir).run('a', (_) async => runs.add('third'));
    first.complete();
    await running;
    expect(runs, ['a', 'a']);
    expect(File('${dir.path}/check.lock').existsSync(), isFalse);
    expect(File('${dir.path}/check.again').existsSync(), isFalse);
  });

  test('requests with different skips repeat without skipping', () async {
    final runs = <String?>[];
    final first = Completer<void>();
    final running = CheckLock(dir).run('a', (skip) async {
      runs.add(skip);
      if (runs.length == 1) await first.future;
    });
    await Future<void>.delayed(const Duration(milliseconds: 50));
    await CheckLock(dir).run('a', (_) async {});
    await CheckLock(dir).run(null, (_) async {});
    first.complete();
    await running;
    expect(runs, ['a', null]);
  });

  test(
    'without news the check runs once, and a stale lock is ignored',
    () async {
      File('${dir.path}/check.lock')
        ..writeAsStringSync('old')
        ..setLastModifiedSync(
          DateTime.now().subtract(const Duration(minutes: 6)),
        );
      var runs = 0;
      await CheckLock(dir).run(null, (_) async => runs++);
      expect(runs, 1);
    },
  );

  test('a failing check still releases the lock', () async {
    await CheckLock(dir).run(null, (_) async => throw StateError('x'));
    expect(File('${dir.path}/check.lock').existsSync(), isFalse);
  });

  test('repeats are bounded', () async {
    var runs = 0;
    await CheckLock(dir).run(null, (_) async {
      runs++;
      await CheckLock(dir).run(null, (_) async {});
    });
    expect(runs, CheckLock.maxRuns);
  });
}
