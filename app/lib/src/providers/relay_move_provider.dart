import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../core/errors/zafe_error_copy.dart';
import '../rust/api/error.dart';
import '../rust/api/relay_move.dart' as rust;
import 'endpoints_provider.dart';
import 'proposals_provider.dart';
import 'vault_provider.dart';

/// The active vault's relay move as this phone sees it (docs/relay-move.md §4.2-4.5):
/// the move it takes part in (`status`), and a move another member announced on the
/// vault's relay (`notice`, the Home card). Nothing here connects to a relay the member
/// didn't choose: the notice comes from the vault's own relay, the status reaches the new
/// relay only once this phone moved there.
class RelayMoveView {
  const RelayMoveView({
    this.status,
    this.notice,
    this.notNow = const {},
    this.twoMoves = false,
  });

  final rust.RelayMoveStatus? status;
  final rust.RelayMoveNotice? notice;

  /// Move links whose card the member put off with "Not now" (until the next start).
  final Set<String> notNow;

  /// Two moves switched from the same history (a member signed both): follow neither.
  final bool twoMoves;

  /// The Home card's move, if it should show.
  rust.RelayMoveNotice? get card {
    final n = notice;
    if (n == null || notNow.contains(n.link)) return null;
    return n;
  }

  /// This phone is in a move that hasn't switched yet.
  bool get moving {
    final s = status;
    return s != null &&
        s.thisPhoneArrived &&
        !s.switched &&
        !s.cancelled &&
        !s.expired &&
        !s.movedElsewhere;
  }

  /// The vault moved and this phone hasn't followed: the relay it moved to.
  String? get movedTo {
    if (status case final s? when s.movedElsewhere) return s.host;
    if (notice case final n? when n.moved) return n.host;
    return null;
  }

  RelayMoveView copyWith({
    rust.RelayMoveStatus? status,
    bool clearStatus = false,
    rust.RelayMoveNotice? notice,
    bool clearNotice = false,
    Set<String>? notNow,
    bool? twoMoves,
  }) => RelayMoveView(
    status: clearStatus ? null : (status ?? this.status),
    notice: clearNotice ? null : (notice ?? this.notice),
    notNow: notNow ?? this.notNow,
    twoMoves: twoMoves ?? this.twoMoves,
  );
}

class RelayMoveNotifier extends Notifier<RelayMoveView> {
  bool _refreshing = false;

  /// Kept across vault switches: "Not now" lasts until the app starts again.
  static final _notNow = <String>{};

  @override
  RelayMoveView build() {
    ref.watch(vaultProvider.select((v) => v.activeId));
    return RelayMoveView(notNow: Set.of(_notNow));
  }

  String get _relayUrl => ref.read(endpointsProvider).relayUrl;

  /// Reads this phone's move and the vault relay's move notices (after each vault
  /// refresh). Errors leave the last view.
  Future<void> refresh() async {
    final vault = ref.read(vaultProvider);
    if (!vault.hasVault || _refreshing) return;
    _refreshing = true;
    try {
      final status = await rust.relayMoveStatus(
        relayUrl: _relayUrl,
        seeds: vault.identity!,
        material: vault.material!,
      );
      rust.RelayMoveNotice? notice;
      var twoMoves = false;
      try {
        notice = await rust.relayMoveNotice(
          relayUrl: _relayUrl,
          seeds: vault.identity!,
          material: vault.material!,
        );
      } on ZafeError catch (e) {
        twoMoves = e.kind == ZafeErrorKind.twoMoves;
        debugPrint('move notice: ${describeError(e)}');
      } catch (e) {
        debugPrint('move notice: ${describeError(e)}');
      }
      state = state.copyWith(
        status: status,
        clearStatus: status == null,
        notice: notice,
        clearNotice: notice == null,
        twoMoves: twoMoves,
      );
    } catch (e) {
      debugPrint('relay move: ${describeError(e)}');
    } finally {
      _refreshing = false;
    }
  }

  /// Starts moving the vault to `relayUrl` (the caller unlocked first).
  Future<rust.RelayMoveStarted> start(String relayUrl, {String? token}) async {
    final vault = ref.read(vaultProvider);
    final started = await rust.startRelayMove(
      relayUrl: _relayUrl,
      seeds: vault.identity!,
      material: vault.material!,
      newRelayUrl: relayUrl,
      createToken: token,
    );
    await refresh();
    return started;
  }

  /// Follows a move: the first contact with its relay (the caller unlocked first, after
  /// the member tapped Move).
  Future<rust.RelayMoveStatus> follow(String link) async {
    final vault = ref.read(vaultProvider);
    final status = await rust.followRelayMove(
      relayUrl: _relayUrl,
      seeds: vault.identity!,
      material: vault.material!,
      link: link,
    );
    state = state.copyWith(status: status, clearNotice: true);
    // A switch rewrites the vault's material: the next refresh saves it.
    await ref.read(proposalsProvider.notifier).refreshFresh();
    return status;
  }

  Future<void> cancel() async {
    final vault = ref.read(vaultProvider);
    await rust.cancelRelayMove(
      relayUrl: _relayUrl,
      seeds: vault.identity!,
      material: vault.material!,
    );
    await refresh();
  }

  /// Members (hex keys) whose app must update before the vault can move.
  Future<List<String>> blockers() async {
    final vault = ref.read(vaultProvider);
    return rust.relayMoveBlockers(
      relayUrl: _relayUrl,
      seeds: vault.identity!,
      material: vault.material!,
    );
  }

  /// "Not now": hides the card for this move until the app starts again.
  void notNow(String link) {
    _notNow.add(link);
    state = state.copyWith(notNow: Set.of(_notNow));
  }
}

final relayMoveProvider = NotifierProvider<RelayMoveNotifier, RelayMoveView>(
  RelayMoveNotifier.new,
);
