import '../core/errors/sync_failure.dart' show formatSince;
import '../core/formatting/member_label.dart';
import '../core/formatting/zec_amount.dart';
import '../core/privacy/amount_display.dart';
import '../features/proposals/proposal_status.dart' show needsMyAction;
import '../rust/api/proposals.dart' as rust;
import '../rust/api/received.dart' as rust;
import '../rust/api/spends.dart' as rust;

class VaultUpdate {
  const VaultUpdate({
    required this.proposalId,
    required this.title,
    required this.body,
  });

  final String proposalId;
  final String title;
  final String body;

  @override
  String toString() => 'VaultUpdate($proposalId, $title, $body)';
}

typedef SeenSnapshot = Map<String, String>;

const kReceivedPrefix = 'rx:';

const kReceivedMarker = 'rx:*';

String receivedKey(String txid) => '$kReceivedPrefix$txid';

const kSeatMovePrefix = 'mv:';

const kSeatMoveMarker = 'mv:*';

String seatMoveKey(rust.SeatMove m) =>
    '$kSeatMovePrefix${m.oldKeyHex}:${m.newKeyHex}';

const kKeepSeatPrefix = 'ks:';

String keepSeatKey(rust.PendingSeatMove m) =>
    '$kKeepSeatPrefix${m.replacement}:${m.newKeyHex}';

const kUnapprovedPrefix = 'sp:';

String unapprovedKey(String txid) => '$kUnapprovedPrefix$txid';

String unapprovedSpendText({
  DateTime? fromCopyAt,
  bool notification = false,
  DateTime? now,
}) {
  final base = notification
      ? 'A transaction spent this vault\'s funds with no approved payment. '
            'Open Zafe and check with the other members now.'
      : 'Check with the other members now.';
  return fromCopyAt == null
      ? base
      : '$base Based on this phone\'s history from ${formatSince(fromCopyAt, now: now)}.';
}

String reapprovalKey(String proposalId) => 're:$proposalId';

String seenKey(rust.ProposalInfo p) =>
    '${p.stage.name}/${p.myVote.name}/${p.ready}';

List<VaultUpdate> vaultUpdates({
  required SeenSnapshot? previous,
  required List<rust.ProposalInfo> proposals,
  required String vaultName,
  required bool hideAmounts,
  List<rust.ReceivedInfo> received = const [],
  Map<String, String> names = const {},
  List<rust.SeatMove> seatMoves = const [],
  List<rust.PendingSeatMove> pendingSeatMoves = const [],
  List<rust.UnapprovedSpendInfo> unapprovedSpends = const [],
  DateTime? unapprovedFromCopyAt,
  String? me,
}) {
  String? nameOf(String keyHex) {
    final n = names[keyHex]?.trim();
    return n == null || n.isEmpty ? null : '$n (${shortKey(keyHex)})';
  }

  final out = <VaultUpdate>[];
  for (final s in unapprovedSpends) {
    if (previous?.containsKey(unapprovedKey(s.txid)) ?? false) continue;
    out.add(
      VaultUpdate(
        proposalId: unapprovedKey(s.txid),
        title: '$vaultName: money left without approval',
        body: unapprovedSpendText(
          fromCopyAt: unapprovedFromCopyAt,
          notification: true,
        ),
      ),
    );
  }
  for (final m in pendingSeatMoves) {
    if (m.oldKeyHex != me || !m.canKeep) continue;
    if (previous?.containsKey(keepSeatKey(m)) ?? false) continue;
    out.add(
      VaultUpdate(
        proposalId: keepSeatKey(m),
        title: '$vaultName: your seat is moving to a new phone',
        body: 'Still your phone? Open Zafe and tap Keep my seat.',
      ),
    );
  }
  if (previous == null) return out;
  if (previous.containsKey(kReceivedMarker)) {
    for (final r in received) {
      if (previous.containsKey(receivedKey(r.txid))) continue;
      final amount = amountWithTicker(
        ZecAmount.fromZatoshi(r.amountZat).activity.amountText,
        hide: hideAmounts,
      );
      out.add(
        VaultUpdate(
          proposalId: receivedKey(r.txid),
          title:
              '$vaultName: ${r.isCoinbase ? 'mining reward' : 'payment'} received',
          body: hideAmounts ? 'Open Zafe to see it.' : '+$amount',
        ),
      );
    }
  }
  if (previous.containsKey(kSeatMoveMarker)) {
    for (final m in seatMoves) {
      final key = seatMoveKey(m);
      if (previous.containsKey(key) || m.approvals.contains(me)) continue;
      if (m.oldKeyHex == me) {
        out.add(
          VaultUpdate(
            proposalId: key,
            title: '$vaultName: someone asked to move your seat',
            body: 'If this is still your phone, call your co-signers now.',
          ),
        );
        continue;
      }
      final who = nameOf(m.oldKeyHex) ?? 'A signer';
      out.add(
        VaultUpdate(
          proposalId: key,
          title: '$vaultName: a signer lost their phone',
          body:
              '$who is moving to a new phone. Compare safety words with them in '
              'Signers.',
        ),
      );
    }
  }
  for (final p in proposals) {
    if (p.needsReapproval && !previous.containsKey(reapprovalKey(p.id))) {
      out.add(
        VaultUpdate(
          proposalId: p.id,
          title: '$vaultName: approve a payment again',
          body:
              'A signing round didn\'t finish. Approve again so the payment can be sent.',
        ),
      );
    }
    final before = previous[p.id];
    if (before == seenKey(p)) continue;
    final beforeStage = before?.split('/').first;
    final amount = amountWithTicker(
      ZecAmount.fromZatoshi(p.totalZat).activity.amountText,
      hide: hideAmounts,
    );
    final to = p.payments.length == 1
        ? compactAddress(p.payments.first.address)
        : '${p.payments.length} recipients';
    final what = p.payments.isEmpty
        ? 'Funds back to the vault (cancels a signed payment)'
        : hideAmounts
        ? 'A payment'
        : '$amount to $to';
    VaultUpdate update(String title, String body) =>
        VaultUpdate(proposalId: p.id, title: '$vaultName: $title', body: body);

    switch (p.stage) {
      case rust.ProposalStage.open:
        if (before == null && !p.isMine && p.myVote == rust.MyVote.none) {
          final by = nameOf(p.author);
          out.add(
            update(
              'payment needs your approval',
              by == null
                  ? what
                  : '$by proposed ${hideAmounts ? 'a payment' : what}.',
            ),
          );
        }
      case rust.ProposalStage.approved:
        if (beforeStage == rust.ProposalStage.approved.name &&
            before!.endsWith('/${p.ready}')) {
          break;
        }
        if (p.ready && p.autoSend) break;
        out.add(
          update(
            p.ready ? 'payment ready to send' : 'payment approved',
            p.ready
                ? '$what. Every signature is in; any signer can send it.'
                : '$what. Open Zafe to collect signatures and send.',
          ),
        );
      case rust.ProposalStage.sent:
        out.add(update('payment sent', what));
      case rust.ProposalStage.rejected:
        final by = [
          for (final k in p.rejections)
            if (nameOf(k) != null) nameOf(k)!,
        ];
        out.add(
          update(
            'payment rejected',
            by.isEmpty ? what : '$what. Rejected by ${by.join(', ')}.',
          ),
        );
      case rust.ProposalStage.cancelled:
        final by = p.isMine ? null : nameOf(p.author);
        out.add(
          update(
            'payment cancelled',
            by == null ? what : '$what. Cancelled by $by.',
          ),
        );
    }
  }
  return out;
}

SeenSnapshot snapshotOf(
  List<rust.ProposalInfo>? proposals, {
  List<rust.ReceivedInfo>? received,
  List<rust.SeatMove>? seatMoves,
  List<rust.PendingSeatMove>? pendingSeatMoves,
  List<rust.UnapprovedSpendInfo>? unapprovedSpends,
  SeenSnapshot previous = const {},
}) => {
  if (proposals == null)
    for (final e in previous.entries)
      if (!e.key.startsWith(kReceivedPrefix) &&
          !e.key.startsWith(kSeatMovePrefix) &&
          !e.key.startsWith(kKeepSeatPrefix) &&
          !e.key.startsWith(kUnapprovedPrefix))
        e.key: e.value,
  if (pendingSeatMoves == null)
    for (final e in previous.entries)
      if (e.key.startsWith(kKeepSeatPrefix)) e.key: e.value,
  if (pendingSeatMoves != null)
    for (final m in pendingSeatMoves) keepSeatKey(m): '',
  if (unapprovedSpends == null)
    for (final e in previous.entries)
      if (e.key.startsWith(kUnapprovedPrefix)) e.key: e.value,
  if (unapprovedSpends != null)
    for (final s in unapprovedSpends) unapprovedKey(s.txid): '',
  if (seatMoves == null)
    for (final e in previous.entries)
      if (e.key.startsWith(kSeatMovePrefix)) e.key: e.value,
  if (seatMoves != null) ...{
    kSeatMoveMarker: '',
    for (final m in seatMoves) seatMoveKey(m): '',
  },
  if (proposals != null)
    for (final p in proposals) p.id: seenKey(p),
  if (proposals != null)
    for (final p in proposals)
      if (p.needsReapproval) reapprovalKey(p.id): '',
  if (received == null)
    for (final e in previous.entries)
      if (e.key.startsWith(kReceivedPrefix)) e.key: e.value,
  if (received != null) ...{
    kReceivedMarker: '',
    for (final r in received) receivedKey(r.txid): '',
  },
};

int actionableCount(List<rust.ProposalInfo> proposals, {int? height}) =>
    proposals.where((p) => needsMyAction(p, height: height)).length;
