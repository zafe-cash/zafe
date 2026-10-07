import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/errors/zafe_error_copy.dart';
import 'package:zafe/src/rust/api/error.dart';

void main() {
  test(
    'relay trust errors have their own copy, none says to switch relays',
    () {
      for (final kind in [
        ZafeErrorKind.relayRolledBack,
        ZafeErrorKind.relayLostVault,
        ZafeErrorKind.relayForked,
        ZafeErrorKind.relayMembership,
      ]) {
        final text = zafeErrorMessage(
          ZafeError(kind: kind, message: 'x', endpoint: ZafeEndpoint.relay),
          fallback: 'FALLBACK',
        );
        expect(text, isNot('FALLBACK'), reason: '$kind');
        expect(text.toLowerCase(), isNot(contains('another relay')));
      }
    },
  );
}
