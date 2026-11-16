import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../core/config/endpoints.dart';
import '../core/config/lightwalletd_presets.dart';
import '../core/config/network_config.dart';
import '../core/errors/sync_failure.dart';
import '../rust/api/endpoints.dart' as rust;
import 'endpoints_provider.dart';

enum ServerCheck { checking, ok, unavailable, wrongNetwork }

class ServerProbe {
  const ServerProbe(this.check, [this.latency]);
  static const checking = ServerProbe(ServerCheck.checking);

  final ServerCheck check;

  final Duration? latency;

  String get label => switch (check) {
    ServerCheck.checking => 'Checking...',
    ServerCheck.ok => '${latency!.inMilliseconds} ms',
    ServerCheck.unavailable => 'Unavailable',
    ServerCheck.wrongNetwork => 'Wrong network',
  };
}

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

class ServerSwitch {
  const ServerSwitch({required this.from, required this.to});
  final LightwalletdPreset from;
  final LightwalletdPreset to;

  String get message =>
      '${from.label} wasn\'t answering. Switched to ${to.label}.';
}

class ServerFailoverNotifier extends Notifier<ServerSwitch?> {
  static const _cooldown = Duration(minutes: 5);
  DateTime? _lastAttempt;

  @override
  ServerSwitch? build() => null;

  static bool serverProblem(SyncFailure failure) =>
      failure.endpoint == SyncEndpoint.lightwalletd &&
      switch (failure.kind) {
        SyncFailureKind.lightwalletdUnreachable ||
        SyncFailureKind.timeout ||
        SyncFailureKind.tls ||
        SyncFailureKind.serverBehind => true,
        _ => false,
      };

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
