/// "Use Tor" (Settings > Privacy): the saved choice and the connection's state, plus the
/// copy for Settings and Home. Pure: unit-tested in `test/tor_setting_test.dart`.
///
/// The route itself lives in Rust (`zafe_core::tor`, process-wide and fail-closed): with
/// Tor on, every relay request and lightwalletd connection goes through Tor or fails; it
/// never falls back to a direct connection. Push notifications (FCM) are Google's and stay
/// as they are; they carry no content.
library;

import '../../rust/api/tor.dart';

/// Shared preference holding the choice (per device, default off). Background checks
/// read it after `prefs.reload()`.
const kUseTorKey = 'zafe_use_tor';

/// How long the app waits for Tor to connect in the foreground. A first bootstrap
/// downloads Tor's directory (tens of seconds on a slow link); later ones reuse it.
const kTorBootstrapTimeoutSecs = 180;

/// How long a background check waits for Tor before giving up on this run (the next
/// periodic check tries again). Push-triggered checks get little time from the OS.
const kBackgroundTorTimeoutSecs = 60;

enum TorConnection { off, connecting, connected, failed }

TorConnection torConnectionOf(TorState state) => switch (state) {
  TorState.off => TorConnection.off,
  TorState.connecting => TorConnection.connecting,
  TorState.connected => TorConnection.connected,
  TorState.failed => TorConnection.failed,
};

class TorSetting {
  const TorSetting({
    required this.enabled,
    required this.connection,
    this.error,
  });

  const TorSetting.off()
    : enabled = false,
      connection = TorConnection.off,
      error = null;

  /// The user's choice.
  final bool enabled;
  final TorConnection connection;

  /// Why the last attempt to connect failed (technical, shown small).
  final String? error;

  bool get connecting => enabled && connection == TorConnection.connecting;
  bool get failed => enabled && connection == TorConnection.failed;

  TorSetting copyWith({
    bool? enabled,
    TorConnection? connection,
    String? error,
    bool clearError = false,
  }) => TorSetting(
    enabled: enabled ?? this.enabled,
    connection: connection ?? this.connection,
    error: clearError ? null : (error ?? this.error),
  );

  /// The value next to "Use Tor" in Settings.
  String get statusLabel => !enabled
      ? 'Off'
      : switch (connection) {
          TorConnection.connected => 'Connected',
          TorConnection.failed => 'Failed',
          // Enabled but the runtime hasn't started yet counts as connecting.
          TorConnection.connecting || TorConnection.off => 'Connecting…',
        };

  /// What the setting does in its current state, for the Tor sheet.
  String get explanation => !enabled
      ? 'Off: the relay and Zcash server see your IP address.'
      : switch (connection) {
          TorConnection.connected =>
            'On: the relay and Zcash server can\'t see your IP address.',
          TorConnection.failed =>
            'Tor couldn\'t connect, so nothing is sent. Try again, or turn Tor off.',
          _ =>
            'Connecting to Tor. Nothing is sent until it connects; the first time can take a minute.',
        };

  /// Home's status line while Tor is in the way, or `null` to show the usual one.
  /// Only a failure is shown as an error.
  String? get homeLabel {
    if (!enabled) return null;
    return switch (connection) {
      TorConnection.connected => null,
      TorConnection.failed => 'Tor couldn\'t connect',
      _ => 'Connecting to Tor…',
    };
  }
}

/// For background checks (WorkManager, push), which run without the app's providers:
/// with Tor on, switches the route (before any network call) and connects, within the
/// budget `enable` applies. Returns whether the check may go on: `false` when Tor didn't
/// connect, so the check does nothing rather than fail every request one by one.
Future<bool> ensureTorForBackground({
  required bool useTor,
  required void Function() request,
  required Future<TorConnection> Function() enable,
  void Function(Object error)? onError,
}) async {
  if (!useTor) return true;
  request();
  try {
    return await enable() == TorConnection.connected;
  } catch (e) {
    onError?.call(e);
    return false;
  }
}
