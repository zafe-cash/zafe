import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/features/proposals/activity_feed.dart';
import 'package:zafe/src/features/proposals/activity_screen.dart';
import 'package:zafe/src/rust/api/proposals.dart';
import 'package:zafe/src/rust/api/received.dart';

ProposalInfo proposal(String id, int createdAt) => ProposalInfo(
  id: id,
  author: 'aa',
  isMine: false,
  payments: const [],
  totalZat: BigInt.one,
  stage: ProposalStage.sent,
  approvals: const [],
  rejections: const [],
  myVote: MyVote.none,
  threshold: 2,
  rejectionThreshold: 2,
  createdAt: BigInt.from(createdAt),
  signingStarted: false,
  oneTap: true,
  ready: false,
  completedByMe: false,
  autoSend: true,
  expiryHeight: 0,
  needsReapproval: false,
  stillSendable: false,
  needsNewProposal: false,
);

ReceivedInfo receipt(String txid, int time, {int height = 10}) => ReceivedInfo(
  txid: txid,
  amountZat: BigInt.one,
  minedHeight: height,
  blockTimeSecs: time,
  confirmations: height == 0 ? 0 : 1,
  memo: '',
  isCoinbase: false,
);

String id(ActivityItem item) => switch (item) {
  ProposalActivity(:final proposal) => proposal.id,
  ReceivedActivity(:final received) => received.txid,
  final RelayMovedActivity m => 'moved-${m.record.logIndex}',
};

void main() {
  test('merges newest first, pending receipts on top, unknown times last', () {
    final merged = mergeActivity(
      [proposal('p3', 300), proposal('p1', 100), proposal('p0', 0)],
      [receipt('pending', 0, height: 0), receipt('r2', 200), receipt('r0', 50)],
    );
    expect(merged.map(id), ['pending', 'p3', 'r2', 'p1', 'r0', 'p0']);
  });

  test('pending receipts belong to this week', () {
    final now = DateTime(2026, 9, 30, 12);
    final sections = activitySections(
      mergeActivity(
        [proposal('p', DateTime(2026, 8, 3).millisecondsSinceEpoch ~/ 1000)],
        [receipt('pending', 0, height: 0)],
      ),
      now: now,
    );
    expect(sections.map((s) => s.$1), ['This week', 'August 2026']);
  });
}
