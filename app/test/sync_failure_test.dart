import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/errors/sync_failure.dart';
import 'package:zafe/src/rust/api/error.dart';

ZafeError err(
  ZafeErrorKind kind, [
  ZafeEndpoint endpoint = ZafeEndpoint.none,
]) => ZafeError(kind: kind, message: 'detail', endpoint: endpoint);

void main() {
  group('classifySyncFailure', () {
    test('unreachable servers are told apart by endpoint', () {
      expect(
        classifySyncFailure(
          err(ZafeErrorKind.network, ZafeEndpoint.lightwalletd),
        ).kind,
        SyncFailureKind.lightwalletdUnreachable,
      );
      expect(
        classifySyncFailure(
          err(ZafeErrorKind.network, ZafeEndpoint.relay),
        ).kind,
        SyncFailureKind.relayUnreachable,
      );
      // No endpoint in the error: the call's server decides.
      expect(
        classifySyncFailure(
          err(ZafeErrorKind.network),
          fallback: SyncEndpoint.relay,
        ).kind,
        SyncFailureKind.relayUnreachable,
      );
      expect(
        classifySyncFailure(err(ZafeErrorKind.network)).kind,
        SyncFailureKind.offline,
      );
    });

    test('typed kinds map one to one and keep the endpoint', () {
      final cases = {
        ZafeErrorKind.tls: SyncFailureKind.tls,
        ZafeErrorKind.networkTimeout: SyncFailureKind.timeout,
        ZafeErrorKind.serverBehind: SyncFailureKind.serverBehind,
        ZafeErrorKind.wrongNetwork: SyncFailureKind.wrongNetwork,
        ZafeErrorKind.relayOutdated: SyncFailureKind.relayOutdated,
        ZafeErrorKind.relayRolledBack: SyncFailureKind.relayRolledBack,
        ZafeErrorKind.relayLostVault: SyncFailureKind.relayLostVault,
        ZafeErrorKind.relayForked: SyncFailureKind.relayForked,
        ZafeErrorKind.relayMembership: SyncFailureKind.relayMembership,
      };
      cases.forEach((kind, expected) {
        final f = classifySyncFailure(err(kind, ZafeEndpoint.lightwalletd));
        expect(f.kind, expected, reason: '$kind');
        expect(f.endpoint, SyncEndpoint.lightwalletd, reason: '$kind');
        expect(f.detail, 'detail');
      });
    });

    test('device and app problems name no server', () {
      final db = classifySyncFailure(
        err(ZafeErrorKind.walletDatabase),
        fallback: SyncEndpoint.lightwalletd,
      );
      expect(db.kind, SyncFailureKind.walletDatabase);
      expect(db.endpoint, isNull);
      final update = classifySyncFailure(
        err(ZafeErrorKind.updateRequired, ZafeEndpoint.relay),
      );
      expect(update.kind, SyncFailureKind.updateRequired);
      expect(update.endpoint, isNull);
    });

    test('anything else is "other", including non-Zafe errors', () {
      expect(
        classifySyncFailure(err(ZafeErrorKind.verification)).kind,
        SyncFailureKind.other,
      );
      final f = classifySyncFailure(
        StateError('boom'),
        fallback: SyncEndpoint.lightwalletd,
      );
      expect(f.kind, SyncFailureKind.other);
      expect(f.endpoint, SyncEndpoint.lightwalletd);
    });

    test('every kind has short status copy', () {
      for (final kind in SyncFailureKind.values) {
        final f = SyncFailure(kind: kind);
        expect(f.statusLabel.length, lessThanOrEqualTo(24), reason: '$kind');
        expect(f.title, isNotEmpty);
        expect(f.explanation, isNotEmpty);
      }
    });
  });

  group('homeSyncFailure', () {
    test('nothing failed', () {
      expect(homeSyncFailure(), isNull);
    });

    test('both servers unreachable reads as offline', () {
      final f = homeSyncFailure(
        syncError: err(ZafeErrorKind.network, ZafeEndpoint.lightwalletd),
        relayError: err(ZafeErrorKind.network, ZafeEndpoint.relay),
      );
      expect(f!.kind, SyncFailureKind.offline);
      expect(f.endpoint, isNull);
    });

    test('the wallet sync failure comes first, then the relay', () {
      expect(
        homeSyncFailure(
          syncError: err(ZafeErrorKind.tls, ZafeEndpoint.lightwalletd),
          relayError: err(ZafeErrorKind.network, ZafeEndpoint.relay),
        )!.kind,
        SyncFailureKind.tls,
      );
      final relayOnly = homeSyncFailure(
        relayError: err(ZafeErrorKind.networkTimeout),
      )!;
      expect(relayOnly.kind, SyncFailureKind.timeout);
      expect(relayOnly.endpoint, SyncEndpoint.relay);
    });
  });

  test('formatLastSuccess', () {
    final now = DateTime(2026, 9, 30, 14, 0);
    expect(formatLastSuccess(null, now: now), 'Not yet');
    expect(formatLastSuccess(now, now: now), 'Just now');
    expect(
      formatLastSuccess(now.subtract(const Duration(minutes: 5)), now: now),
      '5 min ago',
    );
    expect(
      formatLastSuccess(now.subtract(const Duration(hours: 3)), now: now),
      '3 h ago',
    );
    expect(
      formatLastSuccess(DateTime(2026, 9, 28, 9, 5), now: now),
      '28 Sep, 09:05',
    );
  });

  test(
    'only a rolled back or lost relay can be restored, on the same relay',
    () {
      for (final k in SyncFailureKind.values) {
        final f = SyncFailure(kind: k);
        final restorable =
            k == SyncFailureKind.relayRolledBack ||
            k == SyncFailureKind.relayLostVault;
        expect(f.canRestoreRelay, restorable, reason: '$k');
        expect(f.explanation.toLowerCase(), isNot(contains('another relay')));
      }
    },
  );

  test('only a forked relay offers to follow it, with a plain warning', () {
    for (final k in SyncFailureKind.values) {
      expect(
        SyncFailure(kind: k).canFollowRelay,
        k == SyncFailureKind.relayForked,
        reason: '$k',
      );
    }
    expect(followRelayWarning, contains('lost'));
    expect(followRelayWarning, contains('other members'));
    expect(followRelayWarning.toLowerCase(), contains('funds are not touched'));
    expect(followRelayDone(0), isNot(contains('dropped')));
    expect(followRelayDone(3), contains('3 entries'));
  });

  test('a relay with other members is not trusted and points to Settings', () {
    const f = SyncFailure(kind: SyncFailureKind.relayMembership);
    expect(f.explanation, contains('funds are safe'));
    expect(f.suggestsSettings, isTrue);
    expect(f.canRestoreRelay, isFalse);
    final c = classifySyncFailure(
      ZafeError(
        kind: ZafeErrorKind.relayMembership,
        message: 'x',
        endpoint: ZafeEndpoint.relay,
      ),
    );
    expect(c.kind, SyncFailureKind.relayMembership);
  });
}
