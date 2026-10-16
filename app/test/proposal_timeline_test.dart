import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/features/proposals/proposal_timeline.dart';
import 'package:zafe/src/rust/api/proposals.dart';

import 'vault_updates_test.dart' show proposal;

void main() {
  final a = 'aa' * 32, b = 'bb' * 32, c = 'cc' * 32;
  final names = {a: 'Alice', b: 'Bob', c: 'Carol'};
  List<String> titles(ProposalInfo p, {bool sending = false}) => [
    for (final s in timelineSteps(
      p,
      members: [a, b, c],
      me: b,
      names: names,
      sending: sending,
    ))
      s.title,
  ];

  test('an open payment says who it still waits for', () {
    final steps = timelineSteps(
      proposal('p', author: a, myVote: MyVote.approved),
      members: [a, b, c],
      me: b,
      names: names,
    );
    expect(steps.first.title, 'Proposed by Alice');
    expect(steps.last.state, StepState.current);
  });

  test('ends with sending, sent or why it stopped', () {
    expect(
      titles(proposal('p', author: a, stage: ProposalStage.approved)).last,
      'Ready to send',
    );
    expect(
      titles(
        proposal('p', author: a, stage: ProposalStage.approved),
        sending: true,
      ).last,
      'Sending',
    );
    expect(
      titles(proposal('p', author: a, stage: ProposalStage.sent)).last,
      'Sent',
    );
    expect(
      titles(proposal('p', author: a, stage: ProposalStage.cancelled)).last,
      'Cancelled',
    );
    expect(
      timelineSteps(
        proposal('p', author: a, expiryHeight: 10),
        members: [a, b, c],
        me: b,
        names: names,
        height: 10,
      ).last.title,
      'Expired',
    );
  });
}
