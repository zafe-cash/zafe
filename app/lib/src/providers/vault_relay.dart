import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../rust/api/vault.dart' as rust;
import 'endpoints_provider.dart';
import 'vault_provider.dart';

/// The active vault's relay (from its material: URL, mailbox label, whether a create
/// token is saved). A vault from before per-vault relays uses the app-wide relay
/// (`usesDefault`). Null without a vault.
final activeVaultRelayProvider = Provider<rust.VaultRelaySettings?>((ref) {
  final material = ref.watch(vaultProvider.select((v) => v.material));
  final fallback = ref.watch(endpointsProvider.select((e) => e.relayUrl));
  if (material == null) return null;
  try {
    return rust.vaultRelaySettings(
      material: material,
      defaultRelayUrl: fallback,
    );
  } catch (_) {
    return null;
  }
});

/// Vaults from before per-vault relays follow the app-wide relay setting, which is no
/// longer shown: each such vault gets the relay it uses saved into its material, once
/// (Home calls this after start). A vault that already has its own relay is left as is.
Future<void> pinLegacyRelays(WidgetRef ref) async {
  final url = ref.read(endpointsProvider).relayUrl;
  for (final v in ref.read(vaultProvider).vaults) {
    final material = v.material;
    if (material == null) continue;
    try {
      final pinned = rust.pinVaultRelay(material: material, relayUrl: url);
      if (pinned != null) {
        await ref.read(vaultProvider.notifier).replaceMaterial(v.id, pinned);
      }
    } catch (_) {
      // Unreadable material or a placeholder relay: leave it on the fallback.
    }
  }
}
