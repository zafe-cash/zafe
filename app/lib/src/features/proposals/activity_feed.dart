import 'package:flutter/widgets.dart';
import 'package:go_router/go_router.dart';

import '../../core/widgets/app_icon.dart';
import '../../core/widgets/mobile/mobile_list_row.dart';
import '../../rust/api/proposals.dart' as rust;
import '../../rust/api/received.dart' as rust;
import '../received/received_row.dart';
import 'proposal_status.dart';

/// One row of the vault's activity: a payment proposal, or money received.
sealed class ActivityItem {
  const ActivityItem();

  /// When it happened: the proposal's creation, or the receiving block's time. `null` when
  /// unknown, or for a received payment that is not mined yet.
  DateTime? get time;
}

class ProposalActivity extends ActivityItem {
  const ProposalActivity(this.proposal);
  final rust.ProposalInfo proposal;

  @override
  DateTime? get time => proposal.createdAt == BigInt.zero
      ? null
      : DateTime.fromMillisecondsSinceEpoch(proposal.createdAt.toInt() * 1000);
}

class ReceivedActivity extends ActivityItem {
  const ReceivedActivity(this.received);
  final rust.ReceivedInfo received;

  bool get pending => received.minedHeight == 0;

  @override
  DateTime? get time => received.blockTimeSecs == 0
      ? null
      : DateTime.fromMillisecondsSinceEpoch(received.blockTimeSecs * 1000);
}

/// The vault switched to another relay: "Grants moved to relay.dao.org".
class RelayMovedActivity extends ActivityItem {
  const RelayMovedActivity({required this.vaultName, required this.record});
  final String vaultName;
  final rust.RelayMoveRecord record;

  String get label => '$vaultName moved to ${record.host}';

  @override
  DateTime? get time => record.at == BigInt.zero
      ? null
      : DateTime.fromMillisecondsSinceEpoch(record.at.toInt() * 1000);
}

/// Sort key, newest largest: pending receipts first, then by time; unknown times last.
int _key(ActivityItem item) {
  if (item is ReceivedActivity && item.pending) return 1 << 62;
  return item.time?.millisecondsSinceEpoch ?? -1;
}

/// Merges proposals and received payments, newest first. Both lists arrive newest first;
/// each keeps its own order (ties go to the proposal).
List<ActivityItem> mergeActivity(
  List<rust.ProposalInfo> proposals,
  List<rust.ReceivedInfo> received, {
  List<RelayMovedActivity> moves = const [],
}) {
  final a = [for (final p in proposals) ProposalActivity(p)];
  final b = [for (final r in received) ReceivedActivity(r)];
  final out = <ActivityItem>[];
  var i = 0, j = 0;
  while (i < a.length && j < b.length) {
    if (_key(a[i]) >= _key(b[j])) {
      out.add(a[i++]);
    } else {
      out.add(b[j++]);
    }
  }
  out
    ..addAll(a.skip(i))
    ..addAll(b.skip(j));
  // Moves are rare: each goes before the first item older than it.
  for (final m in moves) {
    final at = out.indexWhere((x) => _key(x) < _key(m));
    at < 0 ? out.add(m) : out.insert(at, m);
  }
  return out;
}

/// The row for an activity item; tapping opens its detail page.
class ActivityRow extends StatelessWidget {
  const ActivityRow({
    super.key,
    required this.item,
    this.hideAmount = false,
    this.height,
  });
  final ActivityItem item;
  final bool hideAmount;

  /// Synced chain tip, to show expired proposals.
  final int? height;

  @override
  Widget build(BuildContext context) => switch (item) {
    ProposalActivity(:final proposal) => ProposalRow(
      proposal: proposal,
      hideAmount: hideAmount,
      height: height,
      onTap: () => context.push('/proposal/${proposal.id}'),
    ),
    ReceivedActivity(:final received) => ReceivedRow(
      received: received,
      hideAmount: hideAmount,
      onTap: () => context.push('/received/${received.txid}'),
    ),
    final RelayMovedActivity moved => MobileListRow(
      leading: const AppIcon(AppIcons.endpoint, size: 20),
      label: moved.label,
      minRowHeight: 56,
    ),
  };
}
