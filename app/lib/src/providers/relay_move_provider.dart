import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../core/errors/zafe_error_copy.dart';
import '../rust/api/error.dart';
import '../rust/api/relay_move.dart' as rust;
import 'endpoints_provider.dart';
import 'proposals_provider.dart';
import 'vault_provider.dart';

class RelayMoveView {
  const RelayMoveView({
    this.status,
    this.notice,
    this.notNow = const {},
    this.twoMoves = false,
  });

  final rust.RelayMoveStatus? status;
  final rust.RelayMoveNotice? notice;

  final Set<String> notNow;

  final bool twoMoves;

  rust.RelayMoveNotice? get card {
    final n = notice;
    if (n == null || notNow.contains(n.link)) return null;
    return n;
  }

  bool get moving {
    final s = status;
    return s != null &&
        s.thisPhoneArrived &&
        !s.switched &&
        !s.cancelled &&
        !s.expired &&
        !s.movedElsewhere;
  }

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

  static final _notNow = <String>{};

  @override
  RelayMoveView build() {
    ref.watch(vaultProvider.select((v) => v.activeId));
    return RelayMoveView(notNow: Set.of(_notNow));
  }

  String get _relayUrl => ref.read(endpointsProvider).relayUrl;

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

  Future<rust.RelayMoveStatus> follow(String link) async {
    final vault = ref.read(vaultProvider);
    final status = await rust.followRelayMove(
      relayUrl: _relayUrl,
      seeds: vault.identity!,
      material: vault.material!,
      link: link,
    );
    state = state.copyWith(status: status, clearNotice: true);
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

  Future<List<String>> blockers() async {
    final vault = ref.read(vaultProvider);
    return rust.relayMoveBlockers(
      relayUrl: _relayUrl,
      seeds: vault.identity!,
      material: vault.material!,
    );
  }

  void notNow(String link) {
    _notNow.add(link);
    state = state.copyWith(notNow: Set.of(_notNow));
  }
}

final relayMoveProvider = NotifierProvider<RelayMoveNotifier, RelayMoveView>(
  RelayMoveNotifier.new,
);
