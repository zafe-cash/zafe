import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../rust/api/vault.dart' as rust;
import 'endpoints_provider.dart';
import 'vault_provider.dart';

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
    } catch (_) {}
  }
}
