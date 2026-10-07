import 'dart:async';

import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../core/config/endpoints.dart';
import '../core/errors/zafe_error_copy.dart';
import '../core/storage/zafe_paths.dart';
import '../core/storage/zafe_secure_store.dart';
import '../core/storage/vault_summaries.dart';
import '../notifications/vault_updates.dart' show actionableCount;
import '../notifications/vault_watch.dart' show recordSeen, reregisterPush;
import '../features/proposals/proposal_status.dart' show proposalExpired;
import '../rust/api/error.dart';
import '../rust/api/proposals.dart' as rust;
import '../rust/api/relay_log.dart' as rust_relay_log;
import '../rust/api/repair.dart' as rust_repair;
import 'endpoints_provider.dart';
import 'vault_provider.dart';

/// A send in progress (or just failed) on this device.
class SendState {
  const SendState({this.progress, this.error, this.timedOut = false});

  /// Latest progress; `null` until the first event.
  final rust.SendProgress? progress;

  /// Friendly error once a send attempt failed (the send is then no longer running).
  final String? error;

  /// The attempt failed because chosen signers didn't answer in time: the leader can
  /// wait and try again, or start a new signing round.
  final bool timedOut;

  bool get running => error == null && progress?.stage != rust.SendStage.sent;
}

class ProposalsState {
  const ProposalsState({
    this.items = const [],
    this.loaded = false,
    this.error,
    this.sends = const {},
    this.newerVersionEntries = 0,
    this.refreshedAt,
    this.sharedNames = const {},
    this.backedUp = const {},
    this.seatMoves = const [],
    this.repairs = const [],
  });

  final List<rust.ProposalInfo> items;
  final bool loaded;

  /// Why the last refresh failed (usually the relay); cleared by the next success.
  final Object? error;

  /// Last successful refresh (this session).
  final DateTime? refreshedAt;

  /// Sends started on this device, by proposal id.
  final Map<String, SendState> sends;

  /// Vault log entries from a newer Zafe that this build skipped (ask to update).
  final int newerVersionEntries;

  /// Names members gave themselves in the vault log (key hex → name).
  final Map<String, String> sharedNames;

  /// Members (key hex) who attested a backup of their current keys in the vault log.
  final Set<String> backedUp;

  /// Signers moving to a new phone, waiting for approvals.
  final List<rust.SeatMove> seatMoves;

  /// Moved seats whose key isn't rebuilt on the new phone yet.
  final List<rust.RepairInfo> repairs;

  rust.ProposalInfo? byId(String id) {
    for (final p in items) {
      if (p.id == id) return p;
    }
    return null;
  }

  ProposalsState copyWith({
    List<rust.ProposalInfo>? items,
    bool? loaded,
    Object? error,
    bool clearError = false,
    Map<String, SendState>? sends,
    int? newerVersionEntries,
    DateTime? refreshedAt,
    Map<String, String>? sharedNames,
    Set<String>? backedUp,
    List<rust.SeatMove>? seatMoves,
    List<rust.RepairInfo>? repairs,
  }) => ProposalsState(
    items: items ?? this.items,
    loaded: loaded ?? this.loaded,
    error: clearError ? null : (error ?? this.error),
    sends: sends ?? this.sends,
    newerVersionEntries: newerVersionEntries ?? this.newerVersionEntries,
    refreshedAt: refreshedAt ?? this.refreshedAt,
    sharedNames: sharedNames ?? this.sharedNames,
    backedUp: backedUp ?? this.backedUp,
    seatMoves: seatMoves ?? this.seatMoves,
    repairs: repairs ?? this.repairs,
  );
}

/// Vault proposals from the log, plus the member actions on them.
///
/// Each refresh also keeps this device's one-tap commitments topped up (in Rust), answers
/// interactive signing requests, and starts the auto-send for any proposal whose
/// signatures this member's approval completed (so it survives leaving the screen or
/// restarting the app).
class ProposalsNotifier extends Notifier<ProposalsState> {
  bool _refreshing = false;

  /// A refresh was asked for while one was running (see [refreshSoon]).
  bool _again = false;

  /// The proving key is built once per process (see `_prewarm`).
  static bool _prewarmed = false;
  final _subscriptions = <String, StreamSubscription<rust.SendProgress>>{};

  @override
  ProposalsState build() {
    // A different vault on screen starts from scratch (and cancels this one's listeners).
    ref.watch(vaultProvider.select((v) => v.activeId));
    ref.onDispose(() {
      for (final s in _subscriptions.values) {
        s.cancel();
      }
    });
    return const ProposalsState();
  }

  VaultState get _vault => ref.read(vaultProvider);

  static String _fingerprint(List<rust.ProposalInfo> items) => [
    for (final p in items)
      '${p.id}:${p.stage.name}:${p.approvals.length}:${p.rejections.length}',
  ].join(',');

  ZafeEndpoints get _endpoints => ref.read(endpointsProvider);

  /// Synced chain tip of the active vault (null before the first sync).
  int? get _height => _vault.balance?.height;

  /// Refreshes now, or right after the running refresh if there is one: news that
  /// arrives during a refresh may not be in it. For live activity events.
  void refreshSoon() {
    if (_refreshing) {
      _again = true;
      return;
    }
    unawaited(refresh());
  }

  Future<void> refresh() async {
    final vault = _vault;
    if (!vault.hasVault || _refreshing) return;
    _refreshing = true;
    try {
      final paths = await ZafePaths.get();
      final list = await rust.listProposals(
        relayUrl: _endpoints.relayUrl,
        stateDir: await paths.stateDir(vault.activeId!),
        seeds: vault.identity!,
        material: vault.material!,
        tipHeight: _height,
      );
      final items = list.items;
      // Proposals hold notes: when they change, the next wallet sync must run in full.
      if (_fingerprint(items) != _fingerprint(state.items)) {
        ref.read(vaultProvider.notifier).markDirty();
      }
      state = state.copyWith(
        items: items,
        loaded: true,
        clearError: true,
        newerVersionEntries: list.newerVersionEntries,
        refreshedAt: DateTime.now(),
        sharedNames: {for (final n in list.sharedNames) n.keyHex: n.name},
        backedUp: list.backedUp.toSet(),
        seatMoves: list.seatMoves,
        repairs: list.repairs,
      );
      // A signer's seat moved to a new phone: keep this device's copy of the
      // membership current.
      final updated = list.updatedMaterial;
      if (updated != null) {
        await ref
            .read(vaultProvider.notifier)
            .replaceMaterial(vault.activeId!, updated);
      }
      if (!list.backedUp.contains(vault.myKeyHex)) {
        unawaited(_attestIfBackedUp(vault.activeId!));
      }
      // Seen on screen: never announced from the background. Only while the app is in
      // the foreground; a refresh running in the background must not swallow news.
      if (WidgetsBinding.instance.lifecycleState == AppLifecycleState.resumed) {
        unawaited(
          recordSeen(vault.activeId!, items, seatMoves: list.seatMoves),
        );
      }
      unawaited(
        VaultSummaries.write(
          vault.activeId!,
          actionable: actionableCount(items, height: _height),
        ),
      );
      _autoSend();
      _prewarm();
      await _answerRequests(paths);
    } catch (e) {
      debugPrint('refresh failed: ${describeError(e)}');
      state = state.copyWith(error: e);
    } finally {
      _refreshing = false;
      if (_again) {
        _again = false;
        unawaited(refresh());
      }
    }
  }

  /// Once any payment is approved, this device may be the one to send it: build the
  /// proving key in the background (once per process) so the send doesn't wait for it.
  void _prewarm() {
    if (_prewarmed) return;
    final approved = state.items.any(
      (p) =>
          p.stage == rust.ProposalStage.approved &&
          !proposalExpired(p, _height),
    );
    if (!approved) return;
    _prewarmed = true;
    unawaited(
      rust.prewarmProver().catchError((Object e) {
        _prewarmed = false;
        debugPrint('prover prewarm failed: ${describeError(e)}');
      }),
    );
  }

  /// The member whose approval completed the signatures sends, if the proposer asked for
  /// it. A failed attempt is not retried automatically (the screen offers "Try again").
  void _autoSend() {
    for (final p in state.items) {
      final due =
          p.stage == rust.ProposalStage.approved &&
          p.ready &&
          p.autoSend &&
          p.completedByMe &&
          !proposalExpired(p, _height);
      if (due && !state.sends.containsKey(p.id)) startSend(p.id);
    }
  }

  Future<void> _answerRequests(ZafePaths paths) async {
    final vault = _vault;
    try {
      await rust.answerSigningRequests(
        relayUrl: _endpoints.relayUrl,
        lightwalletdUrl: _endpoints.lightwalletdUrl,
        dbDir: paths.dbDir,
        dbKey: await ZafeSecureStore.instance.walletKey(vault.activeId!),
        stateDir: await paths.stateDir(vault.activeId!),
        seeds: vault.identity!,
        material: vault.material!,
        tipHeight: _height,
      );
    } catch (_) {
      // Not synced yet or offline: the next poll retries.
    }
  }

  /// Logs a proposal paying `payments` (one, or a batch of up to 50).
  Future<String> propose({
    required List<rust.PaymentInput> payments,
    required bool autoSend,
  }) async {
    final vault = _vault;
    final paths = await ZafePaths.get();
    final id = await rust.proposePayment(
      relayUrl: _endpoints.relayUrl,
      lightwalletdUrl: _endpoints.lightwalletdUrl,
      dbDir: paths.dbDir,
      dbKey: await ZafeSecureStore.instance.walletKey(vault.activeId!),
      seeds: vault.identity!,
      material: vault.material!,
      payments: payments,
      autoSend: autoSend,
    );
    await refresh();
    return id;
  }

  Future<rust.ReviewInfo> review(String id) async {
    final vault = _vault;
    final paths = await ZafePaths.get();
    return rust.reviewProposal(
      relayUrl: _endpoints.relayUrl,
      lightwalletdUrl: _endpoints.lightwalletdUrl,
      dbDir: paths.dbDir,
      dbKey: await ZafeSecureStore.instance.walletKey(vault.activeId!),
      seeds: vault.identity!,
      material: vault.material!,
      proposalId: id,
    );
  }

  /// Approves (and, for one-tap proposals, signs). If this approval completed the
  /// signatures and the proposer asked for auto-send, the refresh starts sending.
  Future<rust.ApproveResult> approve(String id) async {
    final vault = _vault;
    final paths = await ZafePaths.get();
    final result = await rust.approveProposal(
      relayUrl: _endpoints.relayUrl,
      lightwalletdUrl: _endpoints.lightwalletdUrl,
      dbDir: paths.dbDir,
      dbKey: await ZafeSecureStore.instance.walletKey(vault.activeId!),
      stateDir: await paths.stateDir(vault.activeId!),
      seeds: vault.identity!,
      material: vault.material!,
      proposalId: id,
    );
    // A new approval makes an earlier failed send's message stale.
    if (state.sends[id] case final send? when !send.running) {
      state = state.copyWith(sends: {...state.sends}..remove(id));
    }
    await refresh();
    return result;
  }

  Future<void> reject(String id) async {
    final vault = _vault;
    await rust.rejectProposal(
      relayUrl: _endpoints.relayUrl,
      seeds: vault.identity!,
      material: vault.material!,
      proposalId: id,
    );
    await refresh();
  }

  /// Vaults this session already tried to attest a backup for.
  static final _attested = <String>{};

  /// Tells the other members this device has a backup (export, or a restore from one)
  /// when the log doesn't say so yet. Best effort, once per vault per session.
  Future<void> _attestIfBackedUp(String id) async {
    if (!(await VaultSummaries.read(id)).backedUp || !_attested.add(id)) return;
    final vault = _vault;
    if (vault.activeId != id) return;
    try {
      await rust.attestBackup(
        relayUrl: _endpoints.relayUrl,
        seeds: vault.identity!,
        material: vault.material!,
      );
      refreshSoon();
    } catch (e) {
      debugPrint('backup attestation failed: ${describeError(e)}');
    }
  }

  /// Puts the vault's history back on the relay from this phone's copy (the relay lost
  /// it or was rewound; spec §6.3). Returns how many log entries went back (0: the relay
  /// already had all of them).
  Future<int> restoreRelay() async {
    final vault = _vault;
    final restored = await rust_relay_log.restoreRelay(
      relayUrl: _endpoints.relayUrl,
      seeds: vault.identity!,
      material: vault.material!,
    );
    // A restored relay has forgotten this phone's push token.
    unawaited(reregisterPush());
    await refresh();
    return restored;
  }

  /// Approves moving `oldKeyHex`'s seat to the phone that showed `code`. Returns whether
  /// the seat moved (this was the last approval needed).
  Future<bool> approveSeatMove(String oldKeyHex, String code) async {
    final vault = _vault;
    final moved = await rust_repair.approveSeatMove(
      relayUrl: _endpoints.relayUrl,
      seeds: vault.identity!,
      material: vault.material!,
      oldKeyHex: oldKeyHex,
      code: code,
    );
    await refresh();
    return moved;
  }

  /// Takes over from `stalledKeyHex`, a helper not doing its part in rebuilding the key of
  /// the seat moved at `replacement`.
  Future<void> retryRepair(BigInt replacement, String stalledKeyHex) async {
    final vault = _vault;
    await rust.retryRepair(
      relayUrl: _endpoints.relayUrl,
      seeds: vault.identity!,
      material: vault.material!,
      replacement: replacement,
      stalledKeyHex: stalledKeyHex,
    );
    await refresh();
  }

  /// Sets this member's name for the other members (empty clears it).
  Future<void> setMyName(String name) async {
    final vault = _vault;
    await rust.setMyName(
      relayUrl: _endpoints.relayUrl,
      seeds: vault.identity!,
      material: vault.material!,
      name: name,
    );
    await refresh();
  }

  /// Cancels a proposal this member authored.
  Future<void> cancel(String id) async {
    final vault = _vault;
    await rust.cancelProposal(
      relayUrl: _endpoints.relayUrl,
      seeds: vault.identity!,
      material: vault.material!,
      proposalId: id,
    );
    await refresh();
  }

  /// Proposes moving a cancelled payment's funds back to the vault so it can never be
  /// sent (every signature for it may already be out). Returns the new proposal id.
  Future<String> invalidate(String id) async {
    final vault = _vault;
    final paths = await ZafePaths.get();
    final newId = await rust.invalidateProposal(
      relayUrl: _endpoints.relayUrl,
      lightwalletdUrl: _endpoints.lightwalletdUrl,
      dbDir: paths.dbDir,
      dbKey: await ZafeSecureStore.instance.walletKey(vault.activeId!),
      seeds: vault.identity!,
      material: vault.material!,
      proposalId: id,
    );
    await refresh();
    return newId;
  }

  /// Abandons this device's unfinished signing round and starts a new one with the
  /// approvers whose signatures are still unused.
  Future<void> startOver(String id) async {
    if (state.sends[id]?.running ?? false) return;
    final paths = await ZafePaths.get();
    rust.restartSigning(
      stateDir: await paths.stateDir(_vault.activeId!),
      proposalId: id,
    );
    await startSend(id);
  }

  /// Sends a proposal: directly from the approvals' signatures when they are complete
  /// (one tap), otherwise by asking the approvers to sign (interactive). Progress and
  /// errors land in `state.sends[id]`.
  Future<void> startSend(String id) async {
    if (state.sends[id]?.running ?? false) return;
    _setSend(id, const SendState());
    final vault = _vault;
    final paths = await ZafePaths.get();
    await _subscriptions.remove(id)?.cancel();
    _subscriptions[id] = rust
        .sendProposal(
          relayUrl: _endpoints.relayUrl,
          lightwalletdUrl: _endpoints.lightwalletdUrl,
          dbDir: paths.dbDir,
          dbKey: await ZafeSecureStore.instance.walletKey(vault.activeId!),
          stateDir: await paths.stateDir(vault.activeId!),
          seeds: vault.identity!,
          material: vault.material!,
          proposalId: id,
        )
        .listen(
          (p) {
            final error = p.error;
            if (p.stage == rust.SendStage.failed && error != null) {
              _fail(id, error);
            } else {
              _setSend(id, SendState(progress: p));
            }
          },
          onError: (Object e) => _fail(id, e),
          onDone: () {
            _subscriptions.remove(id);
            unawaited(refresh());
            unawaited(ref.read(vaultProvider.notifier).sync());
          },
        );
  }

  void _fail(String id, Object e) {
    debugPrint('send failed: ${describeError(e)}');
    _setSend(
      id,
      SendState(
        error: zafeErrorMessage(e, fallback: 'Send failed. Try again.'),
        timedOut: e is ZafeError && e.kind == ZafeErrorKind.timeout,
      ),
    );
  }

  void _setSend(String id, SendState send) {
    state = state.copyWith(sends: {...state.sends, id: send});
  }
}

final proposalsProvider = NotifierProvider<ProposalsNotifier, ProposalsState>(
  ProposalsNotifier.new,
);

/// This device's independent check of one proposal (re-run when the page opens).
final proposalReviewProvider = FutureProvider.autoDispose
    .family<rust.ReviewInfo, String>(
      (ref, id) => ref.read(proposalsProvider.notifier).review(id),
    );
