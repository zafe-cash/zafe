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

/// Snapshot read before the first frame, so the router can
/// start on the right screen without a flash.
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

  /// Every vault on this device, in the order added (ready and still setting up).
  final List<StoredVault> vaults;

  /// The vault the screens show; null on the welcome screen (first run, or adding one).
  final String? activeId;

  /// While adding another vault: the vault to go back to if the user cancels.
  final String? returnTo;

  final rust.SetupProgress? membership;

  /// Last known balance per vault id (this session).
  final Map<String, rust.Balance> balances;
  final bool syncing;

  /// Last sync failure (a `ZafeError` from Rust), cleared by the next successful sync.
  final Object? syncError;

  /// When each vault last synced successfully in this session (older ones: the vault's
  /// `summary.json`).
  final Map<String, DateTime> syncedAt;

  /// The sync running now is this vault's first on this device (it has never finished
  /// one before): that one takes a while, so the status says so.
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

/// Owns the device's vaults: which one is active, adding (create/join, membership, key
/// generation), removing, and wallet sync of the active vault.
/// Longest a sync may keep the home screen on "Syncing..." (Rust caps a pass at 5 min).
const _syncTimeout = Duration(minutes: 6);

/// What each step before the Rust call (app paths, the wallet key in secure storage) may
/// take. The first sync of a new vault once sat ~6 minutes (the whole [_syncTimeout]) before
/// Rust was even called; a stuck step now fails in under a minute and is named in the
/// diagnostic report.
const _preSyncStepTimeout = Duration(seconds: 45);

/// A sync slower than this is written to the diagnostic report with its steps.
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

  /// Shows vault [id], reading it from storage again (a background check may have saved
  /// its keys meanwhile).
  Future<void> showVault(String id) async {
    state = state.copyWith(clearReturnTo: true);
    await _reload(activeId: id);
    if (state.hasVault) unawaited(sync());
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
    String myName = '',
    String? relayUrl,
    String? createToken,
  }) async {
    // A fresh member identity per vault, so the relay can't link memberships.
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
    // The invite names the relay; the token stays here (never in the invite) for every
    // setup step until the material holds it.
    await _store.add(
      id: vaultId,
      identity: id.seeds,
      invite: invite,
      createToken: token == null || token.isEmpty ? null : token,
    );
    await MyName.setPending(vaultId, myName);
    // Carried to the others by the creator's round-1 message (`stepSetup`).
    final prefs = await SharedPreferences.getInstance();
    await prefs.setInt(expiryDaysKey(vaultId), expiryDays);
    state = state.copyWith(clearReturnTo: true);
    await _reload(activeId: vaultId);
  }

  /// The creator's invite works for 72 more hours (the relay refuses new members after
  /// that). Run when the setup screen shows the invite and when it's shared. Best effort.
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

  /// Gives up on a setup whose membership is locked but whose keys were never made (a
  /// member never came back). The creator also deletes it on the relay (allowed until
  /// the vault's first log entry), so the others see it was cancelled.
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
      } catch (_) {
        // Forgotten here anyway; the others see it once the relay drops it.
      }
    }
    await removeVault(id);
  }

  /// Abandons the vault setup on screen (before membership is locked). The creator's
  /// setup is deleted on the relay, so the invite stops working for anyone who still has
  /// it, and the "New vault" form comes back with the same details (see
  /// [createPrefillProvider]); a joiner just leaves. `deleted` is false when the relay
  /// couldn't delete the setup (older relay, or offline): the vault is forgotten here
  /// anyway, and the old invite may keep working until the relay drops it.
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
    await MyName.setPending(vaultId, myName);
    state = state.copyWith(clearReturnTo: true);
    await _reload(activeId: vaultId);
  }

  /// Adds a vault restored from a backup and shows it. This device starts with no signing
  /// nonces (backups never carry them), so its first refresh retires the commitments the
  /// previous device published (they can't be signed with) and publishes a fresh pool.
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

  /// One step of the setup on screen (see [stepSetup]); [confirmed] starts this member's
  /// part of key generation. Once the vault exists it becomes ready here.
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
      // A restore of the same vault must show "First sync" again.
      syncedAt: {...state.syncedAt}..remove(id),
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

  /// A sync asked for while one runs (another vault before a switch, the old server or
  /// route, a send that just finished) runs again after it, never dropped.
  final _syncRun = CoalescedRun();

  /// Syncs the active vault's wallet. With `force: false` (the Home poll), it first asks
  /// lightwalletd for the tip (one cheap call) and skips the full sync when nothing moved
  /// since the last successful one.
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
      } catch (_) {
        // Fall through: the full sync reports the failure properly.
      }
    }
    _dirty = false;
    // Keep the last error on screen while retrying, so the status doesn't flicker
    // between "Syncing..." and the failure every poll.
    final seen =
        state.syncedAt.containsKey(vaultId) ||
        (await VaultSummaries.read(vaultId)).syncedAt != null;
    state = state.copyWith(syncing: true, firstSync: !seen);
    // Breadcrumbs: the one unexplained hang (first sync after keygen) left nothing in
    // flight in Rust or on the platform side; if it recurs, the log names the last step.
    var step = 'paths';
    final watch = Stopwatch()..start();
    final marks = <String, int>{}; // step -> seconds spent
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
        // A pass is capped at 5 minutes in Rust; this guard frees `syncing` if anything
        // else never answers (seen once on the first sync after keygen, 2026-09-30).
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
      // A stuck step (not a plain offline failure) is worth a line in the report.
      if (e is TimeoutException || watch.elapsed >= _slowSync) {
        unawaited(
          CrashLog.instance?.record(
                'sync',
                'failed at $step after ${watch.elapsed.inSeconds}s: ${describeError(e)}',
              ) ??
              Future<void>.value(),
        );
      }
      // A listed Zcash server that stops answering: move to the next listed one and
      // sync again (at most one attempt per few minutes, never away from a custom URL).
      final switched = await ref
          .read(serverFailoverProvider.notifier)
          .afterSyncFailure(e);
      state = state.copyWith(syncing: false, firstSync: false, syncError: e);
      if (switched) {
        _syncRun.again();
      } else if (_retryOverTor(e)) {
        // Through Tor the first stream to a server often fails while circuits are still
        // being built (seen on a phone right after "Use Tor": "Can't reach Zcash server"
        // for over a minute, until the next poll). One quick retry.
        _torRetryAt = DateTime.now();
        await Future<void>.delayed(const Duration(seconds: 4));
        _syncRun.again();
      }
    }
  }

  DateTime? _torRetryAt;

  /// A network failure while "Use Tor" is on, and no quick retry in the last minute.
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

/// What the creator had typed on "New vault", kept when they cancel the setup so the form
/// comes back filled in (read once by the create screen).
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

  /// The relay picked under More options (null: the default relay).
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

/// The approval window the creator chose for a vault being set up (days).
final vaultProvider = NotifierProvider<VaultNotifier, VaultState>(
  VaultNotifier.new,
);
