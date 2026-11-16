import 'package:shared_preferences/shared_preferences.dart';

import '../core/config/endpoints.dart';
import '../core/config/network_config.dart';
import '../core/storage/zafe_paths.dart';
import '../core/storage/zafe_secure_store.dart';
import '../rust/api/vault.dart' as rust;

String expiryDaysKey(String vaultId) => 'zafe_vault_${vaultId}_expiryDays';

Future<rust.SetupProgress> stepSetup({
  required ZafeEndpoints endpoints,
  required String vaultId,
  required List<int> seeds,
  required String invite,
  String? confirmed,
}) async {
  final saved = (await ZafeSecureStore.instance.read(vaultId)).material;
  if (saved != null) {
    return rust.SetupProgress(
      members: const [],
      sealed: true,
      isCreator: false,
      waitingFor: const [],
      started: true,
      keygenWaitingFor: const [],
      material: saved,
    );
  }
  final stateDir = await (await ZafePaths.get()).stateDir(vaultId);
  final createToken = await ZafeSecureStore.instance.setupCreateToken(vaultId);
  final progress = await rust.setupStep(
    relayUrl: endpoints.relayUrl,
    lightwalletdUrl: endpoints.lightwalletdUrl,
    networkName: kZafeNetwork,
    seeds: seeds,
    invite: invite,
    stateDir: stateDir,
    confirmedSafetyNumber: confirmed,
    birthdayHeight: null,
    expiryDays: (await SharedPreferences.getInstance()).getInt(
      expiryDaysKey(vaultId),
    ),
    createToken: createToken,
  );
  final material = progress.material;
  if (material != null) {
    await ZafeSecureStore.instance.writeMaterial(vaultId, material);
    await ZafeSecureStore.instance.forgetSetupCreateToken(vaultId);
    await rust.forgetKeygen(stateDir: stateDir);
  }
  return progress;
}
