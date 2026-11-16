import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../core/security/app_lock.dart';
import '../core/security/device_auth.dart';
import 'vault_provider.dart';

final deviceAuthProvider = Provider<DeviceAuth>(
  (ref) => DeviceAuth(platformAuthenticator()),
);

final hasScreenLockProvider = FutureProvider.autoDispose<bool>(
  (ref) => ref.read(deviceAuthProvider).hasScreenLock(),
);

const kAppLockKey = 'zafe_app_lock';

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
