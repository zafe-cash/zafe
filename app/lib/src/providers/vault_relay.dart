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
