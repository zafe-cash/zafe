import 'dart:async';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../core/config/endpoints.dart';
import '../core/config/network_config.dart';
import '../core/errors/zafe_error_copy.dart';
import '../core/network/tor_setting.dart' show kUseTorKey;
import '../core/storage/vault_summaries.dart';
import '../core/storage/zafe_paths.dart';
import '../core/storage/zafe_secure_store.dart';
import '../rust/api/vault.dart' as rust;
import '../core/security/app_lock.dart';
import 'device_lock_provider.dart' show kAppLockKey, kRequireUnlockKey;
import 'endpoints_provider.dart';
import 'payment_sounds_provider.dart' show kPaymentSoundsKey;
import 'privacy_mode_provider.dart' show kPrivacyModeKey;
import 'theme_mode_provider.dart' show kThemeModeKey, themeModeFromName;

const kActiveVaultKey = 'zafe_active_vault';

/// Snapshot read before the first frame, so the router can
/// start on the right screen without a flash.
class VaultBootstrap {
  const VaultBootstrap({
    this.vaults = const [],
    this.activeId,
    this.privacyMode = false,
    this.themeMode = ThemeMode.system,
    this.requireUnlock = true,
    this.appLock = AppLockDelay.standard,
    this.endpoints = ZafeEndpoints.defaults,
    this.useTor = false,
    this.paymentSounds = true,
  });
  final List<StoredVault> vaults;
  final String? activeId;
  final bool privacyMode;
  final ThemeMode themeMode;
  final bool requireUnlock;
  final AppLockDelay appLock;
  final ZafeEndpoints endpoints;

  /// "Use Tor" (the route was already switched in `main()`; see `torProvider`).
  final bool useTor;

  /// "Payment sounds" (`paymentSoundsProvider`).
  final bool paymentSounds;

  /// Needs Rust initialized (parses the legacy invite when migrating).
  static Future<VaultBootstrap> load() async {
    final store = ZafeSecureStore.instance;
    final prefs = await SharedPreferences.getInstance();
    final migrated = await store.migrateLegacy(
      (invite) => rust.parseInvite(invite: invite).vaultId,
    );
    if (migrated != null) {
      await (await ZafePaths.get()).migrateLegacy(migrated.id);
      await prefs.setString(kActiveVaultKey, migrated.id);
    }
    final vaults = await store.readAll();
    final saved = prefs.getString(kActiveVaultKey);
    final activeId = vaults.any((v) => v.id == saved)
        ? saved
        : (vaults.where((v) => v.ready).firstOrNull ?? vaults.firstOrNull)?.id;
    return VaultBootstrap(
      vaults: vaults,
      activeId: activeId,
      privacyMode: prefs.getBool(kPrivacyModeKey) ?? false,
      themeMode: themeModeFromName(prefs.getString(kThemeModeKey)),
      requireUnlock: prefs.getBool(kRequireUnlockKey) ?? true,
      appLock: AppLockDelay.fromName(prefs.getString(kAppLockKey)),
      endpoints: ZafeEndpoints.fromPrefs(prefs),
      useTor: prefs.getBool(kUseTorKey) ?? false,
      paymentSounds: prefs.getBool(kPaymentSoundsKey) ?? true,
    );
  }
}

final vaultBootstrapProvider = Provider<VaultBootstrap>(
  (ref) => const VaultBootstrap(),
);

class VaultState {
  const VaultState({
    this.vaults = const [],
    this.activeId,
    this.returnTo,
    this.membership,
    this.balances = const {},
    this.syncing = false,
    this.syncError,
    this.syncedAt = const {},
  });

  /// Every vault on this device, in the order added (ready and still setting up).
  final List<StoredVault> vaults;

  /// The vault the screens show; null on the welcome screen (first run, or adding one).
  final String? activeId;

  /// While adding another vault: the vault to go back to if the user cancels.
  final String? returnTo;

  final rust.MembershipInfo? membership;

  /// Last known balance per vault id (this session).
  final Map<String, rust.Balance> balances;
  final bool syncing;

  /// Last sync failure (a `ZafeError` from Rust), cleared by the next successful sync.
  final Object? syncError;

  /// When each vault last synced successfully in this session (older ones: the vault's
  /// `summary.json`).
  final Map<String, DateTime> syncedAt;

  StoredVault? get active {
    for (final v in vaults) {
      if (v.id == activeId) return v;
    }
    return null;
  }

  Uint8List? get identity => active?.identity;
  String? get invite => active?.invite;
  Uint8List? get material => active?.material;
  rust.Balance? get balance => activeId == null ? null : balances[activeId];

  bool get hasVault => material != null;
  bool get isSettingUp => !hasVault && invite != null;
  bool get isAdding => returnTo != null;

  rust.InviteInfo? get inviteInfo =>
      invite == null ? null : rust.parseInvite(invite: invite!);
  rust.VaultSummary? get summary =>
      material == null ? null : rust.vaultSummary(material: material!);
  String? get myKeyHex =>
      identity == null ? null : rust.identityPublicKey(seeds: identity!);

  VaultState copyWith({
    List<StoredVault>? vaults,
    String? activeId,
    bool clearActive = false,
    String? returnTo,
    bool clearReturnTo = false,
    rust.MembershipInfo? membership,
    bool clearMembership = false,
    Map<String, rust.Balance>? balances,
    bool? syncing,
    Object? syncError,
    bool clearSyncError = false,
    Map<String, DateTime>? syncedAt,
  }) => VaultState(
    vaults: vaults ?? this.vaults,
    activeId: clearActive ? null : (activeId ?? this.activeId),
    returnTo: clearReturnTo ? null : (returnTo ?? this.returnTo),
    membership: clearMembership ? null : (membership ?? this.membership),
    balances: balances ?? this.balances,
    syncing: syncing ?? this.syncing,
    syncError: clearSyncError ? null : (syncError ?? this.syncError),
    syncedAt: syncedAt ?? this.syncedAt,
  );
}

/// Owns the device's vaults: which one is active, adding (create/join, membership, key
/// generation), removing, and wallet sync of the active vault.
/// Longest a sync may keep the home screen on "Syncing..." (Rust caps a pass at 5 min).
const _syncTimeout = Duration(minutes: 6);

class VaultNotifier extends Notifier<VaultState> {
  final _store = ZafeSecureStore.instance;

  ZafeEndpoints get _endpoints => ref.read(endpointsProvider);

  @override
  VaultState build() {
    final boot = ref.watch(vaultBootstrapProvider);
    return VaultState(vaults: boot.vaults, activeId: boot.activeId);
  }

  Future<void> _reload({String? activeId, bool clearActive = false}) async {
    final vaults = await _store.readAll();
    state = state.copyWith(
      vaults: vaults,
      activeId: activeId,
      clearActive: clearActive,
      clearMembership: true,
      clearSyncError: true,
    );
    final prefs = await SharedPreferences.getInstance();
    final id = state.activeId;
    id == null
        ? await prefs.remove(kActiveVaultKey)
        : await prefs.setString(kActiveVaultKey, id);
  }

  /// Saves vault `id`'s material with the current membership (a signer's seat moved to a
  /// new phone, spec §10.1), so summaries and checks show the new key.
  Future<void> replaceMaterial(String id, List<int> material) async {
    await _store.writeMaterial(id, material);
    final vaults = await _store.readAll();
    state = state.copyWith(vaults: vaults);
  }

  /// Shows another vault.
  Future<void> switchTo(String id) async {
    if (id == state.activeId && !state.isAdding) return;
    state = state.copyWith(clearReturnTo: true);
    await _reload(activeId: id);
    unawaited(sync());
  }

  /// Leaves the current vault on screen to create or join another (onboarding shows).
  void beginAddVault() {
    final current = state.activeId;
    if (current == null) return;
    state = state.copyWith(
      returnTo: current,
      clearActive: true,
      clearMembership: true,
    );
  }

  /// Back to the vault that was on screen before "Add vault".
  Future<void> cancelAddVault() async {
    final back = state.returnTo;
    if (back == null) return;
    state = state.copyWith(clearReturnTo: true);
    await _reload(activeId: back);
  }

  Future<void> createVault({
    required String name,
    required int threshold,
    required int members,
    int expiryDays = 7,
  }) async {
    // A fresh member identity per vault, so the relay can't link memberships.
    final id = rust.generateIdentity();
    final invite = await rust.createVault(
      relayUrl: _endpoints.relayUrl,
      seeds: id.seeds,
      name: name,
      threshold: threshold,
      members: members,
    );
    final vaultId = rust.parseInvite(invite: invite).vaultId;
    await _store.add(id: vaultId, identity: id.seeds, invite: invite);
    // Used by `createKeys` (the creator's round-1 message carries it to the others).
    final prefs = await SharedPreferences.getInstance();
    await prefs.setInt(_expiryDaysKey(vaultId), expiryDays);
    state = state.copyWith(clearReturnTo: true);
    await _reload(activeId: vaultId);
  }

  Future<void> joinVault(String invite) async {
    final trimmed = invite.trim();
    final vaultId = rust
        .parseInvite(invite: trimmed)
        .vaultId; // validates first
    if (state.vaults.any((v) => v.id == vaultId)) {
      await switchTo(vaultId); // already on this device
      return;
    }
    final id = rust.generateIdentity();
    await rust.joinVault(
      relayUrl: _endpoints.relayUrl,
      seeds: id.seeds,
      invite: trimmed,
    );
    await _store.add(id: vaultId, identity: id.seeds, invite: trimmed);
    state = state.copyWith(clearReturnTo: true);
    await _reload(activeId: vaultId);
  }

  /// Adds a vault restored from a backup and shows it. This device starts with no signing
  /// nonces (backups never carry them) and publishes a fresh pool on its first refresh.
  Future<void> addRestoredVault({
    required String vaultId,
    required List<int> identity,
    required List<int> material,
    required String invite,
  }) async {
    if (state.vaults.any((v) => v.id == vaultId)) {
      throw StateError('already on this device');
    }
    await _store.add(id: vaultId, identity: identity, invite: invite);
    await _store.writeMaterial(vaultId, material);
    state = state.copyWith(clearReturnTo: true);
    await _reload(activeId: vaultId);
    unawaited(sync());
  }

  Future<rust.MembershipInfo> refreshMembership() async {
    final membership = await rust.vaultMembership(
      relayUrl: _endpoints.relayUrl,
      seeds: state.identity!,
      invite: state.invite!,
    );
    state = state.copyWith(membership: membership);
    return membership;
  }

  Future<void> seal() async {
    await rust.sealVault(
      relayUrl: _endpoints.relayUrl,
      seeds: state.identity!,
      invite: state.invite!,
    );
    await refreshMembership();
  }

  /// Runs key generation with every member; blocks until done. `safetyNumber` is the
  /// number the user confirmed out of band.
  Future<void> createKeys(String safetyNumber) async {
    final vaultId = state.activeId!;
    final material = await rust.runKeygen(
      relayUrl: _endpoints.relayUrl,
      lightwalletdUrl: _endpoints.lightwalletdUrl,
      networkName: kZafeNetwork,
      seeds: state.identity!,
      invite: state.invite!,
      confirmedSafetyNumber: safetyNumber,
      timeoutSecs: 600,
      birthdayHeight: null,
      expiryDays: (await SharedPreferences.getInstance()).getInt(
        _expiryDaysKey(vaultId),
      ),
      stateDir: await (await ZafePaths.get()).stateDir(vaultId),
    );
    await _store.writeMaterial(vaultId, material);
    await _reload(activeId: vaultId);
    unawaited(sync());
  }

  /// Removes a vault from this device: its secrets, signing state, notification state and
  /// wallet database. The vault and this member's seat stay on the relay.
  Future<void> removeVault(String id) async {
    await _store.remove(id);
    await (await ZafePaths.get()).deleteVault(id);
    final rest = state.vaults.where((v) => v.id != id).toList();
    final next = state.activeId == id
        ? (rest.where((v) => v.ready).firstOrNull ?? rest.firstOrNull)?.id
        : state.activeId;
    state = state.copyWith(
      balances: {...state.balances}..remove(id),
      clearReturnTo: true,
    );
    await _reload(activeId: next, clearActive: next == null);
    if (next != null) unawaited(sync());
  }

  /// Set when something besides the chain changed what a sync computes (proposals hold
  /// notes): the next `sync(force: false)` runs in full even at the same tip.
  bool _dirty = true;

  /// Proposals changed (new ones, votes, sends): note holds and the spendable balance
  /// need a full sync even if no block arrived.
  void markDirty() => _dirty = true;

  /// Syncs the active vault's wallet. With `force: false` (the Home poll), it first asks
  /// lightwalletd for the tip (one cheap call) and skips the full sync when nothing moved
  /// since the last successful one.
  Future<void> sync({bool force = true}) async {
    final material = state.material;
    final seeds = state.identity;
    final vaultId = state.activeId;
    if (material == null || seeds == null || vaultId == null || state.syncing) {
      return;
    }
    final last = state.balances[vaultId];
    if (!force && !_dirty && last != null && state.syncError == null) {
      try {
        final tip = await rust.chainTip(
          lightwalletdUrl: _endpoints.lightwalletdUrl,
        );
        if (tip == last.height) return;
      } catch (_) {
        // Fall through: the full sync reports the failure properly.
      }
    }
    _dirty = false;
    // Keep the last error on screen while retrying, so the status doesn't flicker
    // between "Syncing..." and the failure every poll.
    state = state.copyWith(syncing: true);
    // Breadcrumbs: the one unexplained hang (first sync after keygen) left nothing in
    // flight in Rust or on the platform side; if it recurs, the log names the last step.
    var step = 'paths';
    try {
      final balance = await () async {
        final paths = await ZafePaths.get();
        step = 'wallet key';
        final dbKey = await ZafeSecureStore.instance.walletKey(vaultId);
        step = 'rust sync';
        return rust.syncVault(
          dbDir: paths.dbDir,
          dbKey: dbKey,
          lightwalletdUrl: _endpoints.lightwalletdUrl,
          relayUrl: _endpoints.relayUrl,
          seeds: seeds,
          material: material,
        );
        // A pass is capped at 5 minutes in Rust; this guard frees `syncing` if anything
        // else never answers (seen once on the first sync after keygen, 2026-09-30).
      }().timeout(_syncTimeout);
      final now = DateTime.now();
      state = state.copyWith(
        balances: {...state.balances, vaultId: balance},
        syncing: false,
        clearSyncError: true,
        syncedAt: {...state.syncedAt, vaultId: now},
      );
      unawaited(
        VaultSummaries.write(
          vaultId,
          balanceZat: balance.totalZat,
          syncedAt: now,
        ),
      );
    } catch (e) {
      _dirty = true;
      debugPrint('sync failed at $step: ${describeError(e)}');
      state = state.copyWith(syncing: false, syncError: e);
    }
  }
}

/// The approval window the creator chose for a vault being set up (days).
String _expiryDaysKey(String vaultId) => 'zafe_vault_${vaultId}_expiryDays';

final vaultProvider = NotifierProvider<VaultNotifier, VaultState>(
  VaultNotifier.new,
);
