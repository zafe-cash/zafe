import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../core/security/app_lock.dart';
import '../core/security/device_auth.dart';
import 'vault_provider.dart';

const kRequireUnlockKey = 'zafe_require_unlock';
const kNoScreenLockWarnedKey = 'zafe_no_screen_lock_warned';

final deviceAuthProvider = Provider<DeviceAuth>(
  (ref) => DeviceAuth(platformAuthenticator()),
);

/// "Require unlock to approve" (default on): approving, sending, proposing,
/// exporting a backup and removing a vault ask for biometrics or the screen lock.
/// Plain prefs, read in the bootstrap.
class RequireUnlockNotifier extends Notifier<bool> {
  @override
  bool build() => ref.watch(vaultBootstrapProvider).requireUnlock;

  Future<void> set(bool value) async {
    state = value;
    final prefs = await SharedPreferences.getInstance();
    await prefs.setBool(kRequireUnlockKey, value);
  }
}

final requireUnlockProvider = NotifierProvider<RequireUnlockNotifier, bool>(
  RequireUnlockNotifier.new,
);

/// Whether the phone has a screen lock or biometrics. Re-checked each time
/// Settings opens (the owner may have just set one up).
final hasScreenLockProvider = FutureProvider.autoDispose<bool>(
  (ref) => ref.read(deviceAuthProvider).hasScreenLock(),
);

/// True the first time it's called on this install, so the "no screen lock"
/// toast shows once; Settings keeps the permanent warning.
Future<bool> takeNoScreenLockWarning() async {
  final prefs = await SharedPreferences.getInstance();
  if (prefs.getBool(kNoScreenLockWarnedKey) ?? false) return false;
  await prefs.setBool(kNoScreenLockWarnedKey, true);
  return true;
}

const kAppLockKey = 'zafe_app_lock';

/// "Lock app" (default: after 1 minute in the background): how long the app may be
/// away before it asks for the phone's unlock again. Plain prefs, read in the
/// bootstrap.
class AppLockDelayNotifier extends Notifier<AppLockDelay> {
  @override
  AppLockDelay build() => ref.watch(vaultBootstrapProvider).appLock;

  Future<void> set(AppLockDelay value) async {
    state = value;
    final prefs = await SharedPreferences.getInstance();
    await prefs.setString(kAppLockKey, value.name);
  }
}

final appLockDelayProvider =
    NotifierProvider<AppLockDelayNotifier, AppLockDelay>(
      AppLockDelayNotifier.new,
    );

/// Whether the app is locked right now (the lock screen covers everything). Starts
/// locked when there's a vault to protect; `AppLockGate` locks and unlocks it.
class AppLockedNotifier extends Notifier<bool> {
  @override
  bool build() {
    final boot = ref.read(vaultBootstrapProvider);
    return lockOnLaunch(delay: boot.appLock, hasVaults: boot.vaults.isNotEmpty);
  }

  void lock() => state = true;
  void unlock() => state = false;
}

final appLockedProvider = NotifierProvider<AppLockedNotifier, bool>(
  AppLockedNotifier.new,
);
