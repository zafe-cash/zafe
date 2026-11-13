import 'package:flutter/widgets.dart' show AppLifecycleState;
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

  test('a screenshot never draws the privacy cover; the app switcher does', () {
    CoverMode cover(
      AppLifecycleState state, {
      bool prompting = false,
      bool? topResumed,
    }) => privacyCoverFor(state, prompting: prompting, topResumed: topResumed);

    expect(cover(AppLifecycleState.resumed), CoverMode.none);
    expect(cover(AppLifecycleState.resumed, topResumed: false), CoverMode.none);
    // Lost focus but still the top app: the screenshot overlay, the notification
    // shade. The cover would land on screen after the image was taken.
    expect(cover(AppLifecycleState.inactive, topResumed: true), CoverMode.none);
    // Another activity on top: the app switcher (its live tile shows the app).
    expect(cover(AppLifecycleState.inactive, topResumed: false), CoverMode.now);
    // Android 9 and older don't say: cover on any focus loss.
    expect(cover(AppLifecycleState.inactive), CoverMode.now);
    // Really in the background: always.
    for (final away in [
      AppLifecycleState.hidden,
      AppLifecycleState.paused,
      AppLifecycleState.detached,
    ]) {
      expect(cover(away, topResumed: true), CoverMode.now);
      expect(cover(away, topResumed: false), CoverMode.now);
    }
    // Our own unlock prompt never draws it.
    expect(
      cover(AppLifecycleState.inactive, prompting: true, topResumed: false),
      CoverMode.none,
    );
    expect(cover(AppLifecycleState.hidden, prompting: true), CoverMode.none);
  });
}
