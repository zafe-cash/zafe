import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/security/app_lock.dart';

void main() {
  test('the stored setting reads back, unknown values get the default', () {
    for (final d in AppLockDelay.values) {
      expect(AppLockDelay.fromName(d.name), d);
    }
    expect(AppLockDelay.fromName(null), AppLockDelay.oneMinute);
    expect(AppLockDelay.fromName('sometimes'), AppLockDelay.oneMinute);
  });

  test('turning the lock off or waiting longer weakens it', () {
    expect(AppLockDelay.oneMinute.weakenedBy(AppLockDelay.off), isTrue);
    expect(AppLockDelay.oneMinute.weakenedBy(AppLockDelay.fiveMinutes), isTrue);
    expect(AppLockDelay.immediately.weakenedBy(AppLockDelay.oneMinute), isTrue);
    expect(
      AppLockDelay.oneMinute.weakenedBy(AppLockDelay.immediately),
      isFalse,
    );
    expect(AppLockDelay.off.weakenedBy(AppLockDelay.fiveMinutes), isFalse);
    expect(AppLockDelay.oneMinute.weakenedBy(AppLockDelay.oneMinute), isFalse);
  });

  test('locks at launch only when on and a vault exists', () {
    expect(
      lockOnLaunch(delay: AppLockDelay.oneMinute, hasVaults: true),
      isTrue,
    );
    expect(lockOnLaunch(delay: AppLockDelay.off, hasVaults: true), isFalse);
    expect(
      lockOnLaunch(delay: AppLockDelay.immediately, hasVaults: false),
      isFalse,
    );
  });

  test('locks on resume after the delay, never for its own prompt', () {
    bool resume(
      AppLockDelay delay,
      Duration away, {
      bool hasVaults = true,
      bool duringPrompt = false,
    }) => lockOnResume(
      delay: delay,
      hasVaults: hasVaults,
      away: away,
      duringPrompt: duringPrompt,
    );

    expect(resume(AppLockDelay.immediately, Duration.zero), isTrue);
    expect(
      resume(AppLockDelay.oneMinute, const Duration(seconds: 59)),
      isFalse,
    );
    expect(resume(AppLockDelay.oneMinute, const Duration(minutes: 1)), isTrue);
    expect(
      resume(AppLockDelay.fiveMinutes, const Duration(minutes: 4)),
      isFalse,
    );
    expect(resume(AppLockDelay.off, const Duration(days: 1)), isFalse);
    expect(
      resume(AppLockDelay.immediately, Duration.zero, hasVaults: false),
      isFalse,
    );
    // Behind the system PIN screen: the app was "away", but because of us.
    expect(
      resume(
        AppLockDelay.immediately,
        const Duration(seconds: 20),
        duringPrompt: true,
      ),
      isFalse,
    );
    // Left with a prompt showing and back much later: that is being away.
    expect(
      resume(
        AppLockDelay.oneMinute,
        const Duration(hours: 3),
        duringPrompt: true,
      ),
      isTrue,
    );
  });
}
