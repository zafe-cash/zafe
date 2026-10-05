import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../core/config/endpoints.dart';
import '../core/config/lightwalletd_presets.dart';
import '../core/config/network_config.dart';
import '../core/errors/sync_failure.dart';
import '../rust/api/endpoints.dart' as rust;
import 'endpoints_provider.dart';

enum ServerCheck { checking, ok, unavailable, wrongNetwork }

/// What a lightwalletd server answered to an info call.
class ServerProbe {
  const ServerProbe(this.check, [this.latency]);
  static const checking = ServerProbe(ServerCheck.checking);

  final ServerCheck check;

  /// Round trip of the info call, when it answered.
  final Duration? latency;

  String get label => switch (check) {
    ServerCheck.checking => 'Checking...',
    ServerCheck.ok => '${latency!.inMilliseconds} ms',
    ServerCheck.unavailable => 'Unavailable',
    ServerCheck.wrongNetwork => 'Wrong network',
  };
}

/// Asks lightwalletd at `url` for its info (network and tip) and times it. Goes through
/// Tor when "Use Tor" is on, like every other request.
Future<ServerProbe> probeLightwalletd(String url) async {
  final watch = Stopwatch()..start();
  try {
    await rust.checkLightwalletd(
      lightwalletdUrl: url,
      networkName: kZafeNetwork,
    );
    return ServerProbe(ServerCheck.ok, watch.elapsed);
  } catch (e) {
    final failure = classifySyncFailure(e, fallback: SyncEndpoint.lightwalletd);
    return ServerProbe(
      failure.kind == SyncFailureKind.wrongNetwork
          ? ServerCheck.wrongNetwork
          : ServerCheck.unavailable,
    );
  }
}

/// An automatic move from one listed Zcash server to another.
class ServerSwitch {
  const ServerSwitch({required this.from, required this.to});
  final LightwalletdPreset from;
  final LightwalletdPreset to;

  String get message =>
      '${from.label} wasn\'t answering. Switched to ${to.label}.';
}

/// Moves sync to another listed lightwalletd server when the chosen one fails, and keeps
/// the last move for the app to announce. Only between presets: a custom server is the
/// user's choice and is never left. At most one attempt per [_cooldown], so a phone that
/// is merely offline doesn't walk the list on every poll.
class ServerFailoverNotifier extends Notifier<ServerSwitch?> {
  static const _cooldown = Duration(minutes: 5);
  DateTime? _lastAttempt;

  @override
  ServerSwitch? build() => null;

  /// The sync failures a different server can fix.
  static bool serverProblem(SyncFailure failure) =>
      failure.endpoint == SyncEndpoint.lightwalletd &&
      switch (failure.kind) {
        SyncFailureKind.lightwalletdUnreachable ||
        SyncFailureKind.timeout ||
        SyncFailureKind.tls ||
        SyncFailureKind.serverBehind => true,
        _ => false,
      };

  /// After a failed sync: tries the other presets in order and switches to the first that
  /// answers on this network. Returns true when it switched (sync should run again).
  Future<bool> afterSyncFailure(Object error) async {
    final failure = classifySyncFailure(error);
    if (!serverProblem(failure)) return false;
    final now = DateTime.now();
    final last = _lastAttempt;
    if (last != null && now.difference(last) < _cooldown) return false;
    _lastAttempt = now;
    final fromUrl = ref.read(endpointsProvider).lightwalletdUrl;
    final from = lightwalletdPresetFor(fromUrl);
    if (from == null) return false;
    for (final preset in lightwalletdFallbacks(fromUrl)) {
      final probe = await probeLightwalletd(preset.url);
      if (probe.check != ServerCheck.ok) continue;
      // The user may have picked another server meanwhile.
      if (ref.read(endpointsProvider).lightwalletdUrl != fromUrl) return false;
      await ref
          .read(endpointsProvider.notifier)
          .set(EndpointKind.lightwalletd, preset.url);
      state = ServerSwitch(from: from, to: preset);
      return true;
    }
    return false;
  }
}

final serverFailoverProvider =
    NotifierProvider<ServerFailoverNotifier, ServerSwitch?>(
      ServerFailoverNotifier.new,
    );
