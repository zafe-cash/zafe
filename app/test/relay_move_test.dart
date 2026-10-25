import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/features/relay_move/relay_move_screen.dart';
import 'package:zafe/src/services/invite_links.dart';

void main() {
  test('move links are recognised, other links are not', () {
    expect(isMoveLink('https://zafe.cash/move#0100ab'), isTrue);
    expect(isMoveLink('https://ZAFE.cash/move/#0100ab'), isTrue);
    expect(isMoveLink('zafe://move#0100ab'), isTrue);
    expect(
      isMoveLink('https://zafe.cash/move'),
      isFalse,
      reason: 'no fragment',
    );
    expect(isMoveLink('https://zafe.cash/join#zafe-invite-v3:00'), isFalse);
    expect(isMoveLink('https://evil.example/move#0100ab'), isFalse);
    expect(isMoveLink('zcash:u1abc?amount=1'), isFalse);
  });

  test('move copy', () {
    expect(paymentsSwitchText(3, 4), 'Payments switch when 3 of 4 have moved.');
    expect(
      paymentsResumeText(1, 'relay.dao.org'),
      'Payments resume when 1 more member moves to relay.dao.org.',
    );
    expect(
      paymentsResumeText(2, 'relay.dao.org'),
      'Payments resume when 2 more members move to relay.dao.org.',
    );
    expect(
      movedToText('Grants', 'relay.dao.org'),
      'Grants moved to relay.dao.org. Move to continue.',
    );
    expect(mustUpdateText(['Bob']), 'Bob must update Zafe first.');
    expect(
      mustUpdateText(['Bob', 'Carol', 'Dan']),
      'Bob, Carol and Dan must update Zafe first.',
    );
  });

  test(
    'only our relays send notifications; unlisted ones see the IP without Tor',
    () {
      expect(relayHasNoPush('https://relay.dao.org'), isTrue);
      expect(relaySeesIp('https://relay.dao.org', tor: false), isTrue);
      expect(relaySeesIp('https://relay.dao.org', tor: true), isFalse);
    },
  );
}
