library;

import 'package:flutter/widgets.dart' show AppLifecycleState;

enum AppLockDelay {
  off('Off', null),
  immediately('Immediately', Duration.zero),
  oneMinute('After 1 minute', Duration(minutes: 1)),
  fiveMinutes('After 5 minutes', Duration(minutes: 5));

  const AppLockDelay(this.label, this.after);
  final String label;

  final Duration? after;

  static const standard = oneMinute;

  static AppLockDelay fromName(String? name) => AppLockDelay.values.firstWhere(
    (d) => d.name == name,
    orElse: () => standard,
  );

  bool weakenedBy(AppLockDelay next) {
    final now = after;
    final then = next.after;
    if (now == null) return false;
    return then == null || then > now;
  }
}

bool lockOnLaunch({required AppLockDelay delay, required bool hasVaults}) =>
    delay.after != null && hasVaults;

bool lockOnResume({
  required AppLockDelay delay,
  required bool hasVaults,
  required Duration away,
  required bool duringPrompt,
}) {
  final after = delay.after;
  if (after == null || !hasVaults) return false;
  if (duringPrompt && away < kPromptGrace) return false;
  return away >= after;
}

const kPromptGrace = Duration(minutes: 2);

enum CoverMode { none, now }

CoverMode privacyCoverFor(
  AppLifecycleState state, {
  required bool prompting,
  required bool? topResumed,
}) {
  if (prompting) return CoverMode.none;
  return switch (state) {
    AppLifecycleState.resumed => CoverMode.none,
    AppLifecycleState.inactive =>
      topResumed ?? false ? CoverMode.none : CoverMode.now,
    AppLifecycleState.hidden ||
    AppLifecycleState.paused ||
    AppLifecycleState.detached => CoverMode.now,
  };
}
