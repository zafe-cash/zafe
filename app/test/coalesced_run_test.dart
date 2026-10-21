import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/async/coalesced_run.dart';

void main() {
  test('calls while a run is going make exactly one more run', () async {
    final run = CoalescedRun();
    final runs = <bool>[];
    final gate = Completer<void>();
    final first = run.run(false, (f) async {
      runs.add(f);
      if (runs.length == 1) await gate.future;
    });
    await run.run(false, (_) async => fail('must not run concurrently'));
    await run.run(true, (_) async => fail('must not run concurrently'));
    gate.complete();
    await first;
    // The second pass is forced because one of the requests asked for it.
    expect(runs, [false, true]);
    expect(run.busy, isFalse);
  });

  test('no request meanwhile: one run', () async {
    final run = CoalescedRun();
    var n = 0;
    await run.run(true, (_) async => n++);
    expect(n, 1);
  });

  test('the task can ask for another pass, bounded', () async {
    final run = CoalescedRun(maxRuns: 3);
    var n = 0;
    await run.run(true, (_) async {
      n++;
      run.again();
    });
    expect(n, 3);
  });

  test('a failing pass frees the runner', () async {
    final run = CoalescedRun();
    await expectLater(
      run.run(true, (_) async => throw StateError('x')),
      throwsStateError,
    );
    expect(run.busy, isFalse);
  });
}
