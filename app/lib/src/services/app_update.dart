import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:in_app_update/in_app_update.dart';

import '../providers/device_lock_provider.dart' show appLockedProvider;
import '../providers/proposals_provider.dart';

/// Minimum time between two update checks (launch and every resume ask again).
const kUpdateCheckInterval = Duration(hours: 6);

/// Play's in-app update priority (0-5, set per release when publishing) from which an
/// update takes over the screen. Below it the update downloads in the background.
const kCriticalUpdatePriority = 4;

enum UpdateAction { none, immediate, flexible, readyToInstall }

/// What to do with what Play reports. Pure, so the rules are unit-tested.
///
/// A critical release (priority ≥ [kCriticalUpdatePriority]) takes over the screen,
/// unless a payment is being sent: Play would restart the app mid-send. Anything else
/// downloads in the background and waits for the user to restart.
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

/// Google Play in-app updates (Android, Play installs only: a debug or sideloaded build
/// gets an error from Play and nothing happens). Checks at launch and on resume, at most
/// every [kUpdateCheckInterval], only while the app is unlocked. A downloaded update
/// waits for "Restart" on Home ([restart]); it never restarts the app by itself.
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
          // Play asks the user first; the future ends once the download is done.
          state = AppUpdateState.downloading;
          final result = await InAppUpdate.startFlexibleUpdate();
          state = result == AppUpdateResult.success
              ? AppUpdateState.ready
              : AppUpdateState.idle;
      }
    } catch (e) {
      // Not installed from Play (debug, sideloaded, emulator), or Play unavailable.
      debugPrint('update check: $e');
      if (state == AppUpdateState.downloading) state = AppUpdateState.idle;
    } finally {
      _checking = false;
    }
  }

  /// Installs the downloaded update; Play restarts the app.
  Future<void> restart() => InAppUpdate.completeFlexibleUpdate();
}

final appUpdateProvider = NotifierProvider<AppUpdateNotifier, AppUpdateState>(
  AppUpdateNotifier.new,
);
