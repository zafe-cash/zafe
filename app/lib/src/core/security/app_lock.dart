/// App lock (spec §14): the whole app asks for the phone's unlock at launch and when it
/// comes back after some time in the background. Pure rules; the overlay is
/// `app_lock_gate.dart`.
library;

import 'package:flutter/widgets.dart' show AppLifecycleState;

/// How long the app may stay in the background before it locks.
enum AppLockDelay {
  off('Off', null),
  immediately('Immediately', Duration.zero),
  oneMinute('After 1 minute', Duration(minutes: 1)),
  fiveMinutes('After 5 minutes', Duration(minutes: 5));

  const AppLockDelay(this.label, this.after);
  final String label;

  /// Null when the app never locks.
  final Duration? after;

  /// The default: a wallet locks, but switching apps to copy an address doesn't.
  static const standard = oneMinute;

  /// From the stored preference; unknown or missing values get [standard].
  static AppLockDelay fromName(String? name) => AppLockDelay.values.firstWhere(
    (d) => d.name == name,
    orElse: () => standard,
  );

  /// Whether moving from this setting to [next] weakens the lock (so it needs an
  /// unlock first): turning it off or waiting longer.
  bool weakenedBy(AppLockDelay next) {
    final now = after;
    final then = next.after;
    if (now == null) return false;
    return then == null || then > now;
  }
}

/// Whether the app starts locked: only with a vault on the device (onboarding has
/// nothing to protect yet).
bool lockOnLaunch({required AppLockDelay delay, required bool hasVaults}) =>
    delay.after != null && hasVaults;

/// Whether the app locks when it comes back to the foreground after [away] in the
/// background. Time spent behind the system's own unlock prompt ([duringPrompt])
/// doesn't count, or unlocking would lock the app again; but only up to
/// [kPromptGrace]: someone who left the phone with a prompt showing and came back
/// much later (or a thief who found it) gets the lock like any other return.
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

/// Longest time behind the system unlock prompt that doesn't count as being away.
const kPromptGrace = Duration(minutes: 2);

/// Whether the privacy cover (the lock tile over the app, hiding balances) is drawn.
enum CoverMode { none, now }

/// The privacy cover for a lifecycle [state]. Losing focus alone ([AppLifecycleState.inactive])
/// covers only when another activity is on top ([topResumed] false: the app switcher,
/// whose live tile shows the running app). The screenshot overlay and the notification
/// shade take focus but leave the app on top: covering then put the lock tile on screen
/// right after a screenshot, as if capture were blocked (it isn't, outside the secret
/// screens). [topResumed] null: the platform doesn't say (Android 9 and older), cover on
/// any focus loss. [prompting]: our own unlock prompt.
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
