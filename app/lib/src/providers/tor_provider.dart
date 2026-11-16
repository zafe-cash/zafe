import 'dart:async';

import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../core/errors/zafe_error_copy.dart';
import '../core/network/tor_setting.dart';
import '../core/storage/zafe_paths.dart';
import '../rust/api/error.dart';
import '../rust/api/tor.dart' as rust;
import 'vault_provider.dart';

abstract interface class TorRuntime {
  void request();
  Future<TorConnection> enable({required int timeoutSecs});
  void disable();
  TorConnection state();
  void setDormant(bool dormant);
}

class RustTorRuntime implements TorRuntime {
  const RustTorRuntime();

  @override
  void request() => rust.torRequest();

  @override
  Future<TorConnection> enable({required int timeoutSecs}) async {
    final dir = (await ZafePaths.get()).torDir;
    return torConnectionOf(
      await rust.torEnable(torDir: dir, timeoutSecs: timeoutSecs),
    );
  }

  @override
  void disable() => rust.torDisable();

  @override
  TorConnection state() => torConnectionOf(rust.torState());

  @override
  void setDormant(bool dormant) => rust.torSetDormant(dormant: dormant);
}

final torRuntimeProvider = Provider<TorRuntime>((_) => const RustTorRuntime());

class TorNotifier extends Notifier<TorSetting> {
  int _attempt = 0;

  TorRuntime get _runtime => ref.read(torRuntimeProvider);

  @override
  TorSetting build() {
    final enabled = ref.watch(vaultBootstrapProvider).useTor;
    return TorSetting(
      enabled: enabled,
      connection: enabled ? TorConnection.connecting : TorConnection.off,
    );
  }

  Future<void> start() async {
    if (state.enabled) await _connect();
  }

  Future<void> setEnabled(bool enabled) async {
    if (enabled == state.enabled) return;
    final prefs = await SharedPreferences.getInstance();
    if (enabled) {
      _runtime.request();
      state = const TorSetting(
        enabled: true,
        connection: TorConnection.connecting,
      );
      await prefs.setBool(kUseTorKey, true);
      await _connect();
    } else {
      await prefs.setBool(kUseTorKey, false);
      _attempt++;
      _runtime.disable();
      state = const TorSetting.off();
    }
  }

  Future<void> retry() async {
    if (!state.enabled || state.connection == TorConnection.connecting) return;
    await _connect();
  }

  void setForeground(bool foreground) {
    _runtime.setDormant(!foreground);
    if (!foreground || !state.enabled) return;
    final now = _runtime.state();
    if (now == TorConnection.connected &&
        state.connection != TorConnection.connected) {
      state = state.copyWith(connection: now, clearError: true);
    } else if (state.connection == TorConnection.failed) {
      unawaited(retry());
    }
  }

  Future<void> _connect() async {
    final attempt = ++_attempt;
    state = state.copyWith(
      connection: TorConnection.connecting,
      clearError: true,
    );
    try {
      final connection = await _runtime.enable(
        timeoutSecs: kTorBootstrapTimeoutSecs,
      );
      if (attempt != _attempt || !state.enabled) return;
      state = state.copyWith(connection: connection, clearError: true);
    } catch (e) {
      debugPrint('tor: ${describeError(e)}');
      if (attempt != _attempt || !state.enabled) return;
      state = state.copyWith(
        connection: TorConnection.failed,
        error: e is ZafeError ? e.message : '$e',
      );
    }
  }
}

final torProvider = NotifierProvider<TorNotifier, TorSetting>(TorNotifier.new);

final torLifecycleProvider = Provider<void>((ref) {
  final notifier = ref.read(torProvider.notifier);
  unawaited(notifier.start());
  final listener = AppLifecycleListener(
    onStateChange: (s) => switch (s) {
      AppLifecycleState.resumed => notifier.setForeground(true),
      AppLifecycleState.hidden ||
      AppLifecycleState.paused => notifier.setForeground(false),
      _ => null,
    },
  );
  ref.onDispose(listener.dispose);
});
