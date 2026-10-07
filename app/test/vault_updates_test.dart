import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/notifications/vault_updates.dart';
import 'package:zafe/src/rust/api/proposals.dart';
import 'package:zafe/src/rust/api/received.dart';
import 'package:zafe/src/rust/api/spends.dart';

ProposalInfo proposal(
  String id, {
  ProposalStage stage = ProposalStage.open,
  MyVote myVote = MyVote.none,
  bool isMine = false,
  bool ready = false,
  bool autoSend = true,
  bool needsReapproval = false,
  int expiryHeight = 0,
  String author = 'aa',
  List<String> rejections = const [],
}) => ProposalInfo(
  id: id,
  author: author,
  isMine: isMine,
  payments: [
    PaymentInfo(
      address: 'uregtest1abcdefghijklmnopqrstuvwxyz0123456789',
      amountZat: BigInt.from(150000000),
      memo: '',
    ),
  ],
  totalZat: BigInt.from(150000000),
  stage: stage,
  approvals: const [],
  rejections: rejections,
  myVote: myVote,
  threshold: 2,
  rejectionThreshold: 2,
  createdAt: BigInt.zero,
  signingStarted: false,
  oneTap: true,
  ready: ready,
  completedByMe: false,
  autoSend: autoSend,
  expiryHeight: expiryHeight,
  needsReapproval: needsReapproval,
  stillSendable: false,
);

ReceivedInfo receipt(String txid, {bool coinbase = false}) => ReceivedInfo(
  txid: txid,
  amountZat: BigInt.from(250000000),
  minedHeight: 10,
  blockTimeSecs: 1700000000,
  confirmations: 3,
  memo: '',
  isCoinbase: coinbase,
);

List<VaultUpdate> updates(
  SeenSnapshot? previous,
  List<ProposalInfo> now, {
  bool hide = false,
}) => vaultUpdates(
  previous: previous,
  proposals: now,
  vaultName: 'Grants',
  hideAmounts: hide,
);

void main() {
  test('signers this device named are mentioned by name', () {
    const names = {'aa': 'Alice', 'bb': 'Bob'};
    List<VaultUpdate> named(
      SeenSnapshot previous,
      ProposalInfo p, {
      bool hide = false,
    }) => vaultUpdates(
      previous: previous,
      proposals: [p],
      vaultName: 'Grants',
      hideAmounts: hide,
      names: names,
    );
    expect(
      named({}, proposal('p1')).single.body,
      'Alice proposed 1.5 TAZ to uregtes .... 3456789.',
    );
    expect(
      named({}, proposal('p1'), hide: true).single.body,
      'Alice proposed a payment.',
    );
    // Unnamed proposer: no mention.
    expect(
      named({}, proposal('p1', author: 'cc')).single.body,
      '1.5 TAZ to uregtes .... 3456789',
    );
    expect(
      named(
        {'p1': 'open/none/false'},
        proposal(
          'p1',
          stage: ProposalStage.rejected,
          rejections: ['bb', 'cc', 'aa'],
        ),
      ).single.body,
      '1.5 TAZ to uregtes .... 3456789. Rejected by Bob, Alice.',
    );
    expect(
      named({
        'p1': 'open/none/false',
      }, proposal('p1', stage: ProposalStage.cancelled)).single.body,
      '1.5 TAZ to uregtes .... 3456789. Cancelled by Alice.',
    );
  });

  test('a fresh install announces nothing', () {
    expect(updates(null, [proposal('p1')]), isEmpty);
  });

  test('a new proposal from someone else needs approval', () {
    final u = updates({}, [proposal('p1')]);
    expect(u, hasLength(1));
    expect(u.single.title, 'Grants: payment needs your approval');
    expect(u.single.body, '1.5 TAZ to uregtes .... 3456789');
    expect(u.single.proposalId, 'p1');
  });

  test('my own proposal is not announced to me', () {
    expect(updates({}, [proposal('p1', isMine: true)]), isEmpty);
  });

  test('nothing changes, nothing is announced', () {
    final p = proposal('p1');
    expect(updates(snapshotOf([p]), [p]), isEmpty);
  });

  test(
    'ready to send is announced only when the proposer chose manual send',
    () {
      final before = snapshotOf([proposal('p1', myVote: MyVote.approved)]);
      final manual = proposal(
        'p1',
        stage: ProposalStage.approved,
        myVote: MyVote.approved,
        ready: true,
        autoSend: false,
      );
      final auto = proposal(
        'p1',
        stage: ProposalStage.approved,
        myVote: MyVote.approved,
        ready: true,
      );
      expect(
        updates(before, [manual]).single.title,
        'Grants: payment ready to send',
      );
      expect(
        updates(before, [auto]),
        isEmpty,
        reason: 'the completing signer sends it',
      );
    },
  );

  test('sent and rejected are announced', () {
    final before = snapshotOf([proposal('p1'), proposal('p2')]);
    final u = updates(before, [
      proposal('p1', stage: ProposalStage.sent),
      proposal('p2', stage: ProposalStage.rejected),
    ]);
    expect(u.map((x) => x.title), [
      'Grants: payment sent',
      'Grants: payment rejected',
    ]);
  });

  test('privacy mode leaves amounts and addresses out', () {
    final u = updates({}, [proposal('p1')], hide: true);
    expect(u.single.body, 'A payment');
  });

  group('received payments', () {
    List<VaultUpdate> received(
      SeenSnapshot? previous,
      List<ReceivedInfo> now, {
      bool hide = false,
    }) => vaultUpdates(
      previous: previous,
      proposals: const [],
      vaultName: 'Grants',
      hideAmounts: hide,
      received: now,
    );

    test('a new receipt is announced once', () {
      final before = snapshotOf(const [], received: [receipt('t1')]);
      final now = [receipt('t2'), receipt('t1')];
      final u = received(before, now);
      expect(u, hasLength(1));
      expect(u.single.title, 'Grants: payment received');
      expect(u.single.body, '+2.5 TAZ');
      expect(u.single.proposalId, 'rx:t2');
      expect(received(snapshotOf(const [], received: now), now), isEmpty);
    });

    test('snapshots from before receipts were tracked announce none', () {
      final old = snapshotOf([proposal('p1')]);
      expect(old.containsKey(kReceivedMarker), isFalse);
      expect(received(old, [receipt('t1')]), isEmpty);
    });

    test('mining rewards and privacy mode', () {
      final before = snapshotOf(const [], received: const []);
      final u = received(before, [receipt('t1', coinbase: true)], hide: true);
      expect(u.single.title, 'Grants: mining reward received');
      expect(u.single.body, 'Open Zafe to see it.');
    });

    test('recording one kind keeps the other', () {
      final both = snapshotOf([proposal('p1')], received: [receipt('t1')]);
      final proposalsOnly = snapshotOf([proposal('p2')], previous: both);
      expect(proposalsOnly.keys, containsAll(['p2', 'rx:t1', kReceivedMarker]));
      expect(proposalsOnly.containsKey('p1'), isFalse);
      final receivedOnly = snapshotOf(
        null,
        received: [receipt('t2')],
        previous: both,
      );
      expect(receivedOnly.keys, containsAll(['p1', 'rx:t2', kReceivedMarker]));
      expect(receivedOnly.containsKey('rx:t1'), isFalse);
    });
  });

  group('seat moves', () {
    SeatMove move({List<String> approvals = const ['bb']}) => SeatMove(
      oldKeyHex: 'aa',
      newKeyHex: 'ff',
      safetyCode: '1234 5678',
      approvals: approvals,
      needed: 2,
      code: 'zafe-recover-v1:00',
    );
    List<VaultUpdate> moves(
      SeenSnapshot previous,
      List<SeatMove> now, {
      String me = 'cc',
    }) => vaultUpdates(
      previous: previous,
      proposals: const [],
      vaultName: 'Grants',
      hideAmounts: false,
      seatMoves: now,
      me: me,
      names: const {'aa': 'Alice'},
    );

    test('a new pending move is announced once, by name', () {
      final before = snapshotOf(const [], seatMoves: const []);
      final u = moves(before, [move()]);
      expect(u.single.title, 'Grants: a signer lost their phone');
      expect(u.single.body, startsWith('Alice is moving to a new phone.'));
      expect(u.single.proposalId, startsWith(kSeatMovePrefix));
      final after = snapshotOf(const [], seatMoves: [move()], previous: before);
      expect(moves(after, [move()]), isEmpty);
    });

    test('not to the lost seat, nor to members who already approved', () {
      final before = snapshotOf(const [], seatMoves: const []);
      expect(moves(before, [move()], me: 'aa'), isEmpty);
      expect(moves(before, [move()], me: 'bb'), isEmpty);
    });

    test('snapshots from before moves were tracked announce none', () {
      expect(moves(snapshotOf(const []), [move()]), isEmpty);
    });

    test('recording proposals keeps the moves, and the other way round', () {
      final both = snapshotOf(const [], seatMoves: [move()]);
      final proposalsOnly = snapshotOf([proposal('p1')], previous: both);
      expect(proposalsOnly.keys, containsAll(['p1', kSeatMoveMarker]));
      expect(proposalsOnly.keys.any((k) => k.startsWith('mv:aa')), isTrue);
      final movesOnly = snapshotOf(
        null,
        seatMoves: const [],
        previous: proposalsOnly,
      );
      expect(movesOnly.keys, containsAll(['p1', kSeatMoveMarker]));
      expect(movesOnly.keys.any((k) => k.startsWith('mv:aa')), isFalse);
    });
  });

  group('unapproved spends', () {
    final spend = UnapprovedSpendInfo(txid: 'ab12', minedHeight: 0);
    List<VaultUpdate> spends(SeenSnapshot? previous, {bool hide = false}) =>
        vaultUpdates(
          previous: previous,
          proposals: const [],
          vaultName: 'Grants',
          hideAmounts: hide,
          unapprovedSpends: [spend],
        );

    test('announced even on a fresh install and in privacy mode', () {
      final fresh = spends(null);
      expect(fresh.single.proposalId, unapprovedKey('ab12'));
      expect(fresh.single.title, contains('without approval'));
      expect(spends(const {}, hide: true), hasLength(1));
    });

    test('announced once', () {
      final seen = snapshotOf(const [], unapprovedSpends: [spend]);
      expect(seen.containsKey(unapprovedKey('ab12')), isTrue);
      expect(spends(seen), isEmpty);
    });

    test('recording proposals keeps the spends', () {
      final seen = snapshotOf(const [], unapprovedSpends: [spend]);
      final next = snapshotOf([proposal('p1')], previous: seen);
      expect(next.containsKey(unapprovedKey('ab12')), isTrue);
    });
  });
}
