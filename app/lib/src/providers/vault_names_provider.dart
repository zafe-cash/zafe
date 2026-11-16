import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../core/storage/vault_name.dart';
import 'vault_provider.dart';

class VaultNamesNotifier extends Notifier<Map<String, String>> {
  @override
  Map<String, String> build() {
    final ids = ref.watch(
      vaultProvider.select((v) => v.vaults.map((s) => s.id).join(',')),
    );
    if (ids.isNotEmpty) _load(ids.split(','));
    return const {};
  }

  Future<void> _load(List<String> ids) async {
    final names = <String, String>{};
    for (final id in ids) {
      final name = await VaultName.read(id);
      if (name != null) names[id] = name;
    }
    state = names;
  }

  Future<void> rename(String vaultId, String name) async {
    await VaultName.write(vaultId, name);
    final cleaned = VaultName.clean(name);
    state = {...state}..remove(vaultId);
    if (cleaned.isNotEmpty) state = {...state, vaultId: cleaned};
  }
}

final vaultNamesProvider =
    NotifierProvider<VaultNamesNotifier, Map<String, String>>(
      VaultNamesNotifier.new,
    );

final activeVaultNameProvider = Provider<String?>((ref) {
  final vault = ref.watch(vaultProvider);
  final creator = vault.summary?.name;
  if (creator == null) return null;
  return VaultName.display(
    creator,
    ref.watch(vaultNamesProvider)[vault.activeId],
  );
});
