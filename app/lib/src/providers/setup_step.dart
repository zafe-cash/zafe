import 'package:shared_preferences/shared_preferences.dart';

import '../core/config/endpoints.dart';
import '../core/config/network_config.dart';
import '../core/storage/zafe_paths.dart';
import '../core/storage/zafe_secure_store.dart';
import '../rust/api/vault.dart' as rust;

/// The creator's chosen approval window, kept until its round-1 message carries it.
String expiryDaysKey(String vaultId) => 'zafe_vault_${vaultId}_expiryDays';

/// One step of a vault setup, from the setup screen or a background check: nobody has to
/// be online at the same time. With [confirmed] (the safety words the member compared),
/// this member's part of key generation starts. Once the vault exists its material is
/// saved here (then the key-generation state is deleted), whoever ran the step; the
/// result's `material` says so.
Future<rust.SetupProgress> stepSetup({
  required ZafeEndpoints endpoints,
  required String vaultId,
  required List<int> seeds,
  required String invite,
  String? confirmed,
}) async {
  // Another engine (a background check, the app) already saved the vault: stepping on
  // would find no state and offer the words again.
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
  // The creator's token for a private relay: needed on every step, background ones too.
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
