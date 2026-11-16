import 'dart:async';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../core/async/coalesced_run.dart';
import '../core/config/endpoints.dart';
import '../core/diagnostics/crash_log.dart';
import '../core/errors/zafe_error_copy.dart';
import '../core/network/tor_setting.dart' show kUseTorKey;
import '../rust/api/error.dart' show ZafeError, ZafeErrorKind;
import 'tor_provider.dart';
import '../core/storage/vault_summaries.dart';
import '../core/storage/my_name.dart';
import '../core/storage/zafe_paths.dart';
import '../core/storage/wallet_key.dart';
import '../core/storage/zafe_secure_store.dart';
import '../rust/api/vault.dart' as rust;
import '../rust/api/proposals.dart' as proposals_api;
import '../core/security/app_lock.dart';
import 'device_lock_provider.dart' show kAppLockKey;
import 'endpoints_provider.dart';
import 'payment_sounds_provider.dart' show kPaymentSoundsKey;
import 'privacy_mode_provider.dart' show kPrivacyModeKey;
import 'server_failover_provider.dart';
import 'setup_step.dart';
import 'theme_mode_provider.dart' show kThemeModeKey, themeModeFromName;

const kActiveVaultKey = 'zafe_active_vault';

class VaultBootstrap {
  const VaultBootstrap({
    this.vaults = const [],
    this.activeId,
    this.privacyMode = false,
    this.themeMode = ThemeMode.system,
    this.appLock = AppLockDelay.standard,
    this.endpoints = ZafeEndpoints.defaults,
    this.useTor = false,
    this.paymentSounds = true,
  });
  final List<StoredVault> vaults;
  final String? activeId;
  final bool privacyMode;
  final ThemeMode themeMode;
  final AppLockDelay appLock;
  final ZafeEndpoints endpoints;

  final bool useTor;

  final bool paymentSounds;

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
    this.firstSync = false,
  });

  final List<StoredVault> vaults;

  final String? activeId;

  final String? returnTo;

  final rust.SetupProgress? membership;

  final Map<String, rust.Balance> balances;
  final bool syncing;

  final Object? syncError;

  final Map<String, DateTime> syncedAt;

  final bool firstSync;

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
    rust.SetupProgress? membership,
    bool clearMembership = false,
    Map<String, rust.Balance>? balances,
    bool? syncing,
    Object? syncError,
    bool clearSyncError = false,
    Map<String, DateTime>? syncedAt,
    bool? firstSync,
  }) => VaultState(
    vaults: vaults ?? this.vaults,
    activeId: clearActive ? null : (activeId ?? this.activeId),
    returnTo: clearReturnTo ? null : (returnTo ?? this.returnTo),
    membership: clearMembership ? null : (membership ?? this.membership),
    balances: balances ?? this.balances,
    syncing: syncing ?? this.syncing,
    syncError: clearSyncError ? null : (syncError ?? this.syncError),
    syncedAt: syncedAt ?? this.syncedAt,
    firstSync: firstSync ?? this.firstSync,
  );
}

const _syncTimeout = Duration(minutes: 6);

const _preSyncStepTimeout = Duration(seconds: 45);

const _slowSync = Duration(seconds: 45);

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

  Future<void> replaceMaterial(String id, List<int> material) async {
    await _store.writeMaterial(id, material);
    final vaults = await _store.readAll();
    state = state.copyWith(vaults: vaults);
  }

  Future<void> switchTo(String id) async {
    if (id == state.activeId && !state.isAdding) return;
    state = state.copyWith(clearReturnTo: true);
    await _reload(activeId: id);
    unawaited(sync());
  }

  Future<void> showVault(String id) async {
    state = state.copyWith(clearReturnTo: true);
    await _reload(activeId: id);
    if (state.hasVault) unawaited(sync());
  }

  void beginAddVault() {
    final current = state.activeId;
    if (current == null) return;
    state = state.copyWith(
      returnTo: current,
      clearActive: true,
      clearMembership: true,
    );
  }

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
    String myName = '',
    String? relayUrl,
    String? createToken,
  }) async {
    final id = rust.generateIdentity();
    final token = createToken?.trim();
    final invite = await rust.createVault(
      relayUrl: relayUrl ?? _endpoints.relayUrl,
      seeds: id.seeds,
      name: name,
      threshold: threshold,
      members: members,
      createToken: token == null || token.isEmpty ? null : token,
    );
    final vaultId = rust.parseInvite(invite: invite).vaultId;
    await _store.add(
      id: vaultId,
      identity: id.seeds,
      invite: invite,
      createToken: token == null || token.isEmpty ? null : token,
    );
    await MyName.setPending(vaultId, myName);
    final prefs = await SharedPreferences.getInstance();
    await prefs.setInt(expiryDaysKey(vaultId), expiryDays);
    state = state.copyWith(clearReturnTo: true);
    await _reload(activeId: vaultId);
  }

  Future<void> renewInvite() async {
    final seeds = state.identity, invite = state.invite;
    if (seeds == null || invite == null) return;
    final creator =
        state.inviteInfo?.creatorHex == rust.identityPublicKey(seeds: seeds);
    if (!creator || (state.membership?.sealed ?? false)) return;
    try {
      await rust.renewInvite(
        relayUrl: _endpoints.relayUrl,
        seeds: seeds,
        invite: invite,
      );
    } catch (e) {
      debugPrint('renewing the invite failed: $e');
    }
  }

  Future<void> abandonSetup() async {
    final id = state.activeId;
    final seeds = state.identity;
    final invite = state.invite;
    final info = state.inviteInfo;
    if (id == null) return;
    if (seeds != null &&
        invite != null &&
        info?.creatorHex == rust.identityPublicKey(seeds: seeds)) {
      try {
        await rust.cancelSetup(
          relayUrl: _endpoints.relayUrl,
          seeds: seeds,
          invite: invite,
        );
      } catch (_) {}
    }
    await removeVault(id);
  }

  Future<({bool creator, bool deleted})> cancelSetup() async {
    final id = state.activeId;
    final seeds = state.identity;
    final invite = state.invite;
    final info = state.inviteInfo;
    if (id == null || seeds == null || invite == null || info == null) {
      return (creator: false, deleted: true);
    }
    final creator = info.creatorHex == rust.identityPublicKey(seeds: seeds);
    var deleted = true;
    if (creator) {
      final prefs = await SharedPreferences.getInstance();
      ref
          .read(createPrefillProvider.notifier)
          .set(
            CreatePrefill(
              name: info.name,
              threshold: info.threshold,
              members: info.members,
              expiryDays: prefs.getInt(expiryDaysKey(id)) ?? 7,
              relayUrl: info.relayUrl,
              createToken: await _store.setupCreateToken(id),
            ),
          );
      try {
        deleted = await rust.cancelSetup(
          relayUrl: _endpoints.relayUrl,
          seeds: seeds,
          invite: invite,
        );
      } catch (_) {
        deleted = false;
      }
    }
    await removeVault(id);
    return (creator: creator, deleted: deleted);
  }

  Future<void> joinVault(String invite, {String myName = ''}) async {
    final trimmed = invite.trim();
    final vaultId = rust.parseInvite(invite: trimmed).vaultId;
    if (state.vaults.any((v) => v.id == vaultId)) {
      await switchTo(vaultId);
      return;
    }
    final id = rust.generateIdentity();
    await rust.joinVault(
      relayUrl: _endpoints.relayUrl,
      seeds: id.seeds,
      invite: trimmed,
    );
    await _store.add(id: vaultId, identity: id.seeds, invite: trimmed);
    await MyName.setPending(vaultId, myName);
    state = state.copyWith(clearReturnTo: true);
    await _reload(activeId: vaultId);
  }

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
    await proposals_api.markPoolReset(
      stateDir: await (await ZafePaths.get()).stateDir(vaultId),
    );
    state = state.copyWith(clearReturnTo: true);
    await _reload(activeId: vaultId);
    unawaited(sync());
  }

  Future<rust.SetupProgress> setupStep({String? confirmed}) async {
    final vaultId = state.activeId!;
    final progress = await stepSetup(
      endpoints: _endpoints,
      vaultId: vaultId,
      seeds: state.identity!,
      invite: state.invite!,
      confirmed: confirmed,
    );
    state = state.copyWith(membership: progress);
    if (progress.material != null) {
      await _reload(activeId: vaultId);
      unawaited(sync());
    }
    return progress;
  }

  Future<void> removeVault(String id) async {
    await _store.remove(id);
    await (await ZafePaths.get()).deleteVault(id);
    final rest = state.vaults.where((v) => v.id != id).toList();
    final next = state.activeId == id
        ? (rest.where((v) => v.ready).firstOrNull ?? rest.firstOrNull)?.id
        : state.activeId;
    state = state.copyWith(
      balances: {...state.balances}..remove(id),
      syncedAt: {...state.syncedAt}..remove(id),
      clearReturnTo: true,
    );
    await _reload(activeId: next, clearActive: next == null);
    if (next != null) unawaited(sync());
  }

  bool _dirty = true;

  void markDirty() => _dirty = true;

  final _syncRun = CoalescedRun();

  Future<void> sync({bool force = true}) =>
      _syncRun.run(force, (f) => _syncOnce(force: f));

  Future<void> _syncOnce({required bool force}) async {
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
      } catch (_) {}
    }
    _dirty = false;
    final seen =
        state.syncedAt.containsKey(vaultId) ||
        (await VaultSummaries.read(vaultId)).syncedAt != null;
    state = state.copyWith(syncing: true, firstSync: !seen);
    var step = 'paths';
    final watch = Stopwatch()..start();
    final marks = <String, int>{};
    var mark = Duration.zero;
    void lap(String done) {
      marks[done] = (watch.elapsed - mark).inSeconds;
      mark = watch.elapsed;
    }

    try {
      final balance = await () async {
        final paths = await ZafePaths.get().timeout(_preSyncStepTimeout);
        lap('paths');
        step = 'wallet key';
        final dbKey = walletKeyFor(seeds, vaultId);
        lap('wallet key');
        step = 'rust sync';
        return rust.syncVault(
          dbDir: paths.dbDir,
          dbKey: dbKey,
          lightwalletdUrl: _endpoints.lightwalletdUrl,
          relayUrl: _endpoints.relayUrl,
          seeds: seeds,
          material: material,
        );
      }().timeout(_syncTimeout);
      lap('rust sync');
      if (watch.elapsed >= _slowSync) {
        unawaited(
          CrashLog.instance?.record(
                'slow-sync',
                'took ${watch.elapsed.inSeconds}s: ${marks.entries.map((e) => '${e.key}=${e.value}s').join(' ')}',
              ) ??
              Future<void>.value(),
        );
      }
      final now = DateTime.now();
      state = state.copyWith(
        balances: {...state.balances, vaultId: balance},
        syncing: false,
        firstSync: false,
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
      if (e is TimeoutException || watch.elapsed >= _slowSync) {
        unawaited(
          CrashLog.instance?.record(
                'sync',
                'failed at $step after ${watch.elapsed.inSeconds}s: ${describeError(e)}',
              ) ??
              Future<void>.value(),
        );
      }
      final switched = await ref
          .read(serverFailoverProvider.notifier)
          .afterSyncFailure(e);
      state = state.copyWith(syncing: false, firstSync: false, syncError: e);
      if (switched) {
        _syncRun.again();
      } else if (_retryOverTor(e)) {
        _torRetryAt = DateTime.now();
        await Future<void>.delayed(const Duration(seconds: 4));
        _syncRun.again();
      }
    }
  }

  DateTime? _torRetryAt;

  bool _retryOverTor(Object e) {
    if (!ref.read(torProvider).enabled) return false;
    if (e is! ZafeError ||
        !const {
          ZafeErrorKind.network,
          ZafeErrorKind.networkTimeout,
        }.contains(e.kind)) {
      return false;
    }
    final last = _torRetryAt;
    return last == null ||
        DateTime.now().difference(last) > const Duration(minutes: 1);
  }
}

class CreatePrefill {
  const CreatePrefill({
    required this.name,
    required this.threshold,
    required this.members,
    required this.expiryDays,
    this.relayUrl,
    this.createToken,
  });

  final String name;
  final int threshold;
  final int members;
  final int expiryDays;

  final String? relayUrl;
  final String? createToken;
}

class CreatePrefillNotifier extends Notifier<CreatePrefill?> {
  @override
  CreatePrefill? build() => null;

  void set(CreatePrefill? value) => state = value;
}

final createPrefillProvider =
    NotifierProvider<CreatePrefillNotifier, CreatePrefill?>(
      CreatePrefillNotifier.new,
    );

final vaultProvider = NotifierProvider<VaultNotifier, VaultState>(
  VaultNotifier.new,
);
