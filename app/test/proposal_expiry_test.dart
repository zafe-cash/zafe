import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/features/proposals/proposal_status.dart';
import 'package:zafe/src/features/send/send_screen.dart' show zecDecimal;
import 'package:zafe/src/notifications/vault_updates.dart';
import 'package:zafe/src/rust/api/proposals.dart';

import 'vault_updates_test.dart' show proposal;

void main() {
  group('expiry', () {
    final open = proposal('p', expiryHeight: 1000);

    test(
      'an unsent proposal expires once the tip reaches its expiry height',
      () {
        expect(proposalExpired(open, null), isFalse, reason: 'height unknown');
        expect(proposalExpired(open, 999), isFalse);
        expect(proposalExpired(open, 1000), isTrue);
        expect(proposalTitle(open, height: 1000), 'Expired');
        expect(proposalTitle(open, height: 999), 'Needs your approval');
      },
    );

    test(
      'finished proposals and ones without an expiry never show as expired',
      () {
        for (final stage in [
          ProposalStage.sent,
          ProposalStage.rejected,
          ProposalStage.cancelled,
        ]) {
          expect(
            proposalExpired(proposal('p', stage: stage, expiryHeight: 10), 99),
            isFalse,
          );
        }
        expect(proposalExpired(proposal('p'), 99), isFalse);
      },
    );

    test('time left reads in minutes, hours or days (75 s blocks)', () {
      expect(expiresIn(open, 999), 'about 1 minute');
      expect(expiresIn(open, 1000 - 40), 'about 50 minutes');
      expect(expiresIn(open, 1000 - 48), 'about 1 hour');
      expect(expiresIn(open, 1000 - 8064 ~/ 8), 'about 21 hours');
      final week = proposal('p', expiryHeight: 8064);
      expect(expiresIn(week, 0), 'about 7 days');
    });

    test('blocks are 75 s before the NU7 upgrade and 25 s from it on', () {
      // 1,152 blocks a day before the upgrade, 3,456 after.
      expect(blocksToSeconds(0, 1152), 86400);
      expect(blocksToSeconds(0, 1152, nu7Height: 5000), 86400);
      expect(blocksToSeconds(5000, 5000 + 3456, nu7Height: 5000), 86400);
      // Across it: 1,000 blocks at 75 s, then 1,000 at 25 s.
      expect(blocksToSeconds(4000, 6000, nu7Height: 5000), 75000 + 25000);
      // Nothing left, or the chain already past it.
      expect(blocksToSeconds(10, 10, nu7Height: 5), 0);
      expect(blocksToSeconds(20, 10), 0);
      final week = proposal('p', expiryHeight: 8064);
      expect(expiresIn(week, 0, nu7Height: 0), 'about 2 days');
    });

    test('expired proposals need no action', () {
      final items = [
        proposal('a', expiryHeight: 1000),
        proposal('b', stage: ProposalStage.approved, expiryHeight: 1000),
      ];
      expect(actionableCount(items, height: 999), 2);
      expect(actionableCount(items, height: 1000), 0);
      expect(actionableCount(items), 2);
    });
  });

  group('approve again', () {
    final p = proposal(
      'p',
      stage: ProposalStage.approved,
      myVote: MyVote.approved,
      needsReapproval: true,
    );

    test('is announced once, and counts as needing action', () {
      final first = vaultUpdates(
        previous: snapshotOf([proposal('p', stage: ProposalStage.approved)]),
        proposals: [p],
        vaultName: 'Grants',
        hideAmounts: false,
      );
      expect(first.map((u) => u.title), ['Grants: approve a payment again']);
      final again = vaultUpdates(
        previous: snapshotOf([p]),
        proposals: [p],
        vaultName: 'Grants',
        hideAmounts: false,
      );
      expect(again, isEmpty);
      expect(
        actionableCount([
          proposal('q', myVote: MyVote.approved, needsReapproval: true),
        ]),
        1,
      );
    });
  });

  test('needs you: what this member must do, and nothing else', () {
    final vote = proposal('a');
    final voted = proposal('b', myVote: MyVote.approved);
    final send = proposal(
      'c',
      stage: ProposalStage.approved,
      myVote: MyVote.approved,
      ready: true,
      autoSend: false,
    );
    final autoSending = proposal(
      'd',
      stage: ProposalStage.approved,
      myVote: MyVote.approved,
      ready: true,
    );
    final expired = proposal('e', expiryHeight: 10);
    expect(needsMyAction(vote), isTrue);
    expect(needsMyAction(voted), isFalse);
    expect(needsMyAction(send), isTrue);
    expect(needsMyAction(autoSending), isFalse);
    expect(needsMyAction(expired, height: 10), isFalse);
  });

  test('propose again fills the amount field with plain decimal ZEC', () {
    expect(zecDecimal(BigInt.from(150000000)), '1.5');
    expect(zecDecimal(BigInt.from(100000000)), '1');
    expect(zecDecimal(BigInt.one), '0.00000001');
    expect(zecDecimal(BigInt.zero), '0');
  });
}
