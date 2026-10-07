import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'src/app.dart';
import 'src/core/diagnostics/crash_log.dart';
import 'src/core/network/tor_setting.dart';
import 'src/core/storage/zafe_paths.dart';
import 'src/notifications/vault_watch.dart';
import 'src/providers/vault_provider.dart';
import 'src/services/invite_links.dart';
import 'src/rust/api/app.dart';
import 'src/rust/api/tor.dart';
import 'src/rust/frb_generated.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  // Local only (Settings > Diagnostic report): nothing is uploaded.
  CrashLog.instance = CrashLog.forDir(
    (await ZafePaths.get()).diagnosticsDir,
    isBackground: false,
  )..install();
  initInviteLinks(); // early, so the link that launched the app isn't missed
  // Shown on the licenses page (Settings > Open-source licenses): bundled fonts, icons
  // and the third-party code (NOTICE).
  LicenseRegistry.addLicense(() async* {
    for (final (packages, asset) in const [
      (['Space Grotesk'], 'assets/fonts/licenses/SpaceGrotesk-OFL.txt'),
      (['DM Sans'], 'assets/fonts/licenses/DMSans-OFL.txt'),
      (['JetBrains Mono'], 'assets/fonts/licenses/JetBrainsMono-OFL.txt'),
      (['Phosphor Icons'], 'assets/icons/licenses/Phosphor-MIT.txt'),
      (['Vizor (chainapsis/vizor-wallet)'], 'NOTICE'),
    ]) {
      yield LicenseEntryWithLineBreaks(
        packages,
        await rootBundle.loadString(asset),
      );
    }
  });
  await RustLib.init();
  // This phone's copy of each vault's log, before anything can reach the relay: a relay
  // that lost or rewound the log is refused against it.
  initLogCache(dir: (await ZafePaths.get()).logDir);
  // "Use Tor": switch the route before anything can connect (fail-closed). Bootstrapping
  // starts later (`torLifecycleProvider`); nothing waits for it here.
  if ((await SharedPreferences.getInstance()).getBool(kUseTorKey) ?? false) {
    torRequest();
  }
  await initVaultNotifications();
  final bootstrap = await VaultBootstrap.load();
  runApp(
    ProviderScope(
      overrides: [vaultBootstrapProvider.overrideWithValue(bootstrap)],
      child: const ZafeApp(),
    ),
  );
}
