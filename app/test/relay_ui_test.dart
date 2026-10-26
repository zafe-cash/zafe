import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/config/network_config.dart';
import 'package:zafe/src/core/config/relay_presets.dart';
import 'package:zafe/src/core/errors/sync_failure.dart';
import 'package:zafe/src/core/theme/app_theme.dart';
import 'package:zafe/src/features/home/sync_status_sheet.dart'
    show homeStatusLabel;
import 'package:zafe/src/features/onboarding/join_vault_screen.dart'
    show inviteFact;
import 'package:zafe/src/features/settings/relay_picker.dart';
import 'package:zafe/src/notifications/vault_updates.dart'
    show unapprovedSpendText;
import 'package:zafe/src/rust/api/endpoints.dart';

RelayCheck _check({bool tooOld = false, bool push = true}) => RelayCheck(
  tooOld: tooOld,
  relayApiVersion: tooOld ? 0 : 2,
  push: push,
  createTokenRequired: false,
  maxMembers: 64,
  deliveryRetentionDays: 30,
);

void main() {
  group('relay address input', () {
    test('a bare host gets https', () {
      final r = parseRelayInput('relay.dao.org', network: 'main');
      expect(r.url, 'https://relay.dao.org');
      expect(r.token, isNull);
    });

    test('a token after # is split off, never part of the URL', () {
      final r = parseRelayInput(' relay.dao.org#s3cret ', network: 'main');
      expect(r.url, 'https://relay.dao.org');
      expect(r.token, 's3cret');
      final full = parseRelayInput(
        'https://relay.dao.org:8443#t',
        network: 'main',
      );
      expect(full.url, 'https://relay.dao.org:8443');
      expect(full.token, 't');
      expect(parseRelayInput('relay.dao.org#', network: 'main').token, isNull);
    });

    test('paths and plain http are refused', () {
      expect(parseRelayInput('relay.dao.org/x', network: 'main').url, isNull);
      expect(
        parseRelayInput('http://relay.dao.org', network: 'main').url,
        isNull,
      );
      expect(parseRelayInput('', network: 'main').error, isNotNull);
    });

    test('the build relay is listed and recognised', () {
      if (!kZafeRelayIsPlaceholder) {
        expect(relayPresetFor(kZafeRelayUrl), isNotNull);
      }
      expect(relayPresetFor('https://relay.dao.org'), isNull);
      for (final p in kMainnetRelayPresets) {
        expect(p.operator, isNotEmpty);
        expect(p.url, startsWith('https://'));
      }
    });
  });

  test(
    'a relay check result is usable only when it answered and is current',
    () {
      expect(const RelayCheckResult.checking().usable, isFalse);
      expect(const RelayCheckResult.failed('x').usable, isFalse);
      expect(RelayCheckResult.ok(_check(tooOld: true)).usable, isFalse);
      expect(RelayCheckResult.ok(_check(tooOld: true)).tooOld, isTrue);
      expect(RelayCheckResult.ok(_check()).usable, isTrue);
    },
  );

  test('the join screen states the relay as a fact', () {
    expect(
      inviteFact(
        name: 'Grants',
        threshold: 2,
        members: 3,
        relayHost: 'relay.dao.org',
      ),
      'Grants · 2 of 3 · relay.dao.org',
    );
  });

  group('outage', () {
    final now = DateTime(2026, 10, 25, 15, 0);

    test('since: the time today, the date before', () {
      expect(formatSince(DateTime(2026, 10, 25, 9, 14), now: now), '09:14');
      expect(formatSince(DateTime(2026, 10, 24, 23, 59), now: now), '24 Oct');
    });

    test('Home says since when the relay went away', () {
      const f = SyncFailure(kind: SyncFailureKind.relayUnreachable);
      expect(
        homeStatusLabel(
          f,
          relayDownSince: DateTime(2026, 10, 25, 9, 14),
          now: now,
        ),
        'Can\'t reach relay · since 09:14',
      );
      expect(homeStatusLabel(f), 'Can\'t reach relay');
      expect(f.explanation, contains('funds are safe'));
      expect(f.explanation, contains('until the relay is back'));
    });

    test('the spend alert says when it ran from this phone\'s history', () {
      expect(unapprovedSpendText(), isNot(contains('history')));
      expect(
        unapprovedSpendText(
          fromCopyAt: DateTime(2026, 10, 25, 9, 14),
          now: now,
        ),
        endsWith('Based on this phone\'s history from 09:14.'),
      );
      expect(unapprovedSpendText(notification: true), contains('Open Zafe'));
    });

    test('Unix seconds from Rust', () {
      expect(fromUnixSeconds(null), isNull);
      expect(
        fromUnixSeconds(BigInt.from(1761384840))!.millisecondsSinceEpoch,
        1761384840000,
      );
    });
  });

  group('picker', () {
    // Opens the picker on `current` (a relay the vault is on) and records what it pops.
    Future<List<RelayChoice?>> open(WidgetTester tester, String current) async {
      final popped = <RelayChoice?>[];
      await tester.pumpWidget(
        ProviderScope(
          child: MaterialApp(
            home: AppTheme(
              data: AppThemeData.dark,
              child: Builder(
                builder: (context) => Center(
                  child: TextButton(
                    onPressed: () async => popped.add(
                      await Navigator.of(context).push<RelayChoice>(
                        MaterialPageRoute(
                          builder: (_) => AppTheme(
                            data: AppThemeData.dark,
                            child: Material(
                              child: SingleChildScrollView(
                                child: RelayPickerSheet(
                                  title: 'Move to',
                                  initialUrl: current,
                                  currentUrl: current,
                                  check: (_) async =>
                                      RelayCheckResult.ok(_check()),
                                ),
                              ),
                            ),
                          ),
                        ),
                      ),
                    ),
                    child: const Text('open'),
                  ),
                ),
              ),
            ),
          ),
        ),
      );
      await tester.tap(find.text('open'));
      await tester.pumpAndSettle();
      return popped;
    }

    testWidgets('the current relay is shown but never chosen', (tester) async {
      final popped = await open(tester, 'https://relay-a.example.org');
      expect(find.text('Current'), findsOneWidget);
      await tester.tap(find.text('Use this relay'));
      await tester.pumpAndSettle();
      expect(popped, isEmpty);
    });

    testWidgets('a typed address counts without the keyboard\'s Done', (
      tester,
    ) async {
      final popped = await open(tester, 'https://relay-a.example.org');
      await tester.enterText(find.byType(TextField), 'relay-b.example.org');
      await tester.pump();
      await tester.tap(find.text('Use this relay'));
      await tester.pumpAndSettle();
      expect(popped.single?.url, 'https://relay-b.example.org');
    });
  });

  test('same relay URL', () {
    expect(
      sameRelayUrl('https://Relay.example.org', 'https://relay.example.org/'),
      isTrue,
    );
    expect(
      sameRelayUrl('http://127.0.0.1:18902', 'http://127.0.0.1:18903'),
      isFalse,
    );
    expect(
      sameRelayUrl('http://127.0.0.1:18902', 'https://127.0.0.1:18902'),
      isFalse,
    );
  });
}
