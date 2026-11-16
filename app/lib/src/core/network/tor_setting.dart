library;

import '../../rust/api/tor.dart';

const kUseTorKey = 'zafe_use_tor';

const kTorBootstrapTimeoutSecs = 180;

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

  final bool enabled;
  final TorConnection connection;

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

  String get statusLabel => !enabled
      ? 'Off'
      : switch (connection) {
          TorConnection.connected => 'Connected',
          TorConnection.failed => 'Failed',
          TorConnection.connecting || TorConnection.off => 'Connecting...',
        };

  String get explanation => !enabled
      ? 'Off: the servers Zafe uses see your IP address.'
      : switch (connection) {
          TorConnection.connected =>
            'On: the servers Zafe uses can\'t see your IP address.',
          TorConnection.failed =>
            'Tor couldn\'t connect, so nothing is sent. Try again, or turn Tor off.',
          _ =>
            'Connecting to Tor. Nothing is sent until it connects; the first time can take a minute.',
        };

  String? get homeLabel {
    if (!enabled) return null;
    return switch (connection) {
      TorConnection.connected => null,
      TorConnection.failed => 'Tor couldn\'t connect',
      _ => 'Connecting to Tor...',
    };
  }
}

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
