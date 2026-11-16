import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'vault_provider.dart';

const kPrivacyModeKey = 'zafe_privacy_mode_enabled';

class PrivacyModeNotifier extends Notifier<bool> {
  @override
  bool build() => ref.watch(vaultBootstrapProvider).privacyMode;

  Future<void> toggle() async {
    state = !state;
    final prefs = await SharedPreferences.getInstance();
    await prefs.setBool(kPrivacyModeKey, state);
  }
}

final privacyModeProvider = NotifierProvider<PrivacyModeNotifier, bool>(
  PrivacyModeNotifier.new,
);
