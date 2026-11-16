import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'vault_provider.dart';

const kThemeModeKey = 'zafe_theme_mode';

ThemeMode themeModeFromName(String? name) => ThemeMode.values.firstWhere(
  (m) => m.name == name,
  orElse: () => ThemeMode.system,
);

class ThemeModeNotifier extends Notifier<ThemeMode> {
  @override
  ThemeMode build() => ref.watch(vaultBootstrapProvider).themeMode;

  Future<void> set(ThemeMode mode) async {
    state = mode;
    final prefs = await SharedPreferences.getInstance();
    await prefs.setString(kThemeModeKey, mode.name);
  }
}

final themeModeProvider = NotifierProvider<ThemeModeNotifier, ThemeMode>(
  ThemeModeNotifier.new,
);
