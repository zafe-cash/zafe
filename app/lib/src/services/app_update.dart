import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:in_app_update/in_app_update.dart';

import '../providers/device_lock_provider.dart' show appLockedProvider;
import '../providers/proposals_provider.dart';

const kUpdateCheckInterval = Duration(hours: 6);

const kCriticalUpdatePriority = 4;

enum UpdateAction { none, immediate, flexible, readyToInstall }

UpdateAction decideUpdate({
  required bool available,
  required bool downloaded,
  required int priority,
  required bool immediateAllowed,
  required bool flexibleAllowed,
  required bool sending,
}) {
  if (downloaded) return UpdateAction.readyToInstall;
  if (!available) return UpdateAction.none;
  if (priority >= kCriticalUpdatePriority && immediateAllowed && !sending) {
    return UpdateAction.immediate;
  }
  if (flexibleAllowed) return UpdateAction.flexible;
  return UpdateAction.none;
}

enum AppUpdateState { idle, downloading, ready }

class AppUpdateNotifier extends Notifier<AppUpdateState> {
  DateTime? _checkedAt;
  bool _checking = false;

  @override
  AppUpdateState build() {
    if (defaultTargetPlatform != TargetPlatform.android || kIsWeb) {
      return AppUpdateState.idle;
    }
    final lifecycle = AppLifecycleListener(onResume: () => unawaited(check()));
    ref.onDispose(lifecycle.dispose);
    ref.listen(appLockedProvider, (_, locked) {
      if (!locked) unawaited(check());
    });
    Future.microtask(check);
    return AppUpdateState.idle;
  }

  bool get _sending =>
      ref.read(proposalsProvider).sends.values.any((s) => s.running);

  Future<void> check() async {
    if (_checking || ref.read(appLockedProvider)) return;
    if (state != AppUpdateState.idle) return;
    final now = DateTime.now();
    if (_checkedAt case final at?
        when now.difference(at) < kUpdateCheckInterval) {
      return;
    }
    _checking = true;
    _checkedAt = now;
    try {
      final info = await InAppUpdate.checkForUpdate();
      final action = decideUpdate(
        available:
            info.updateAvailability == UpdateAvailability.updateAvailable,
        downloaded: info.installStatus == InstallStatus.downloaded,
        priority: info.updatePriority,
        immediateAllowed: info.immediateUpdateAllowed,
        flexibleAllowed: info.flexibleUpdateAllowed,
        sending: _sending,
      );
      switch (action) {
        case UpdateAction.none:
          break;
        case UpdateAction.readyToInstall:
          state = AppUpdateState.ready;
        case UpdateAction.immediate:
          await InAppUpdate.performImmediateUpdate();
        case UpdateAction.flexible:
          state = AppUpdateState.downloading;
          final result = await InAppUpdate.startFlexibleUpdate();
          state = result == AppUpdateResult.success
              ? AppUpdateState.ready
              : AppUpdateState.idle;
      }
    } catch (e) {
      debugPrint('update check: $e');
      if (state == AppUpdateState.downloading) state = AppUpdateState.idle;
    } finally {
      _checking = false;
    }
  }

  Future<void> restart() => InAppUpdate.completeFlexibleUpdate();
}

final appUpdateProvider = NotifierProvider<AppUpdateNotifier, AppUpdateState>(
  AppUpdateNotifier.new,
);
