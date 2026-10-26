import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/config/relay_presets.dart';
import 'package:zafe/src/features/home/needs_you.dart';
import 'package:zafe/src/features/home/sync_status_screen.dart';
import 'package:zafe/src/features/settings/relay_picker.dart';
import 'package:zafe/src/providers/relay_move_provider.dart';
import 'package:zafe/src/rust/api/endpoints.dart';
import 'package:zafe/src/rust/api/relay_move.dart';
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
    expect(
      switchesWhenText('Grants', 3, 4),
      'Grants switches when 3 of 4 have moved.',
    );
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
    'only our servers send notifications; unlisted ones see the IP without Tor',
    () {
      expect(relayHasNoPush('https://relay.dao.org'), isTrue);
      expect(relaySeesIp('https://relay.dao.org', tor: false), isTrue);
      expect(relaySeesIp('https://relay.dao.org', tor: true), isFalse);
    },
  );

  group('an outage suggests a listed server', () {
    const presets = [
      RelayPreset(operator: 'Zafe', url: 'https://relay.zafe.cash'),
      RelayPreset(operator: 'Zafe', url: 'https://relay2.zafe.cash'),
      RelayPreset(operator: 'DAO', url: 'https://relay.dao.org'),
    ];
    RelayCheckResult ok() => const RelayCheckResult.ok(
      RelayCheck(
        tooOld: false,
        relayApiVersion: 2,
        push: true,
        createTokenRequired: false,
        maxMembers: 64,
        deliveryRetentionDays: 30,
      ),
    );

    test('the first one that answers, never the one that is down', () async {
      final asked = <String>[];
      final url = await suggestedServer(
        'https://relay.zafe.cash',
        presets: presets,
        check: (u) async {
          asked.add(u);
          return u == 'https://relay2.zafe.cash'
              ? const RelayCheckResult.failed('Couldn\'t connect.')
              : ok();
        },
      );
      expect(url, 'https://relay.dao.org');
      expect(asked, ['https://relay2.zafe.cash', 'https://relay.dao.org']);
    });

    test('none when no other listed server answers', () async {
      final url = await suggestedServer(
        'https://relay.zafe.cash',
        presets: presets,
        check: (_) async => const RelayCheckResult.failed('Couldn\'t connect.'),
      );
      expect(url, isNull);
    });
  });

  group('the move in Needs you', () {
    RelayMoveNotice notice({bool moved = false, String link = 'l1'}) =>
        RelayMoveNotice(
          relayUrl: 'https://relay.dao.org',
          host: 'relay.dao.org',
          link: link,
          moverHex: 'aa',
          arrived: 1,
          needed: 2,
          members: 3,
          moved: moved,
        );
    RelayMoveStatus status({bool movedElsewhere = false}) => RelayMoveStatus(
      relayUrl: 'https://relay.dao.org',
      host: 'relay.dao.org',
      link: 'l2',
      moverHex: 'bb',
      mine: false,
      arrived: const ['bb', 'cc'],
      needed: 2,
      members: 3,
      thisPhoneArrived: false,
      expired: false,
      cancelled: false,
      switched: false,
      movedElsewhere: movedElsewhere,
    );

    test('a member\'s move, until Not now', () {
      final card = moveRequest(RelayMoveView(notice: notice()))!;
      expect((card.link, card.moved, card.arrived), ('l1', false, 1));
      expect(
        moveRequest(RelayMoveView(notice: notice(), notNow: const {'l1'})),
        isNull,
      );
    });

    test('a switch this phone missed comes first and can\'t be put off', () {
      final card = moveRequest(
        RelayMoveView(
          status: status(movedElsewhere: true),
          notice: notice(),
          notNow: const {'l2'},
        ),
      )!;
      expect((card.link, card.moved, card.arrived), ('l2', true, 2));
    });

    test('nothing while two moves were found', () {
      expect(
        moveRequest(RelayMoveView(notice: notice(), twoMoves: true)),
        isNull,
      );
    });
  });
}
