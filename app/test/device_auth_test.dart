import 'package:flutter_test/flutter_test.dart';
import 'package:local_auth/local_auth.dart';
import 'package:zafe/src/core/security/device_auth.dart';

class _FakeAuthenticator implements DeviceAuthenticator {
  _FakeAuthenticator({
    this.hasLock = true,
    this.result = true,
    this.error,
    this.lockError,
  });

  final bool hasLock;
  final bool result;
  final Object? error;
  final Object? lockError;
  final prompts = <String>[];

  @override
  Future<bool> hasScreenLock() async {
    if (lockError != null) throw lockError!;
    return hasLock;
  }

  @override
  Future<bool> authenticate(String reason) async {
    prompts.add(reason);
    if (error != null) throw error!;
    return result;
  }
}

Future<(UnlockOutcome, _FakeAuthenticator)> _run(
  _FakeAuthenticator fake, {
  bool required = true,
}) async {
  final outcome = await DeviceAuth(
    fake,
  ).unlock(required: required, reason: 'Unlock to approve');
  return (outcome, fake);
}

void main() {
  test('setting off: no prompt, allowed', () async {
    final (outcome, fake) = await _run(_FakeAuthenticator(), required: false);
    expect(outcome, UnlockOutcome.notRequired);
    expect(outcome.allowed, isTrue);
    expect(fake.prompts, isEmpty);
  });

  test('no screen lock: no prompt, allowed with a warning', () async {
    final (outcome, fake) = await _run(_FakeAuthenticator(hasLock: false));
    expect(outcome, UnlockOutcome.noScreenLock);
    expect(outcome.allowed, isTrue);
    expect(fake.prompts, isEmpty);
  });

  test('unlocked: prompts with the reason, allowed', () async {
    final (outcome, fake) = await _run(_FakeAuthenticator());
    expect(outcome, UnlockOutcome.unlocked);
    expect(outcome.allowed, isTrue);
    expect(fake.prompts, ['Unlock to approve']);
  });

  test('prompt dismissed without an error: blocked', () async {
    final (outcome, _) = await _run(_FakeAuthenticator(result: false));
    expect(outcome, UnlockOutcome.cancelled);
    expect(outcome.allowed, isFalse);
  });

  test('platform errors map to outcomes', () async {
    const cases = {
      LocalAuthExceptionCode.userCanceled: UnlockOutcome.cancelled,
      LocalAuthExceptionCode.systemCanceled: UnlockOutcome.cancelled,
      LocalAuthExceptionCode.timeout: UnlockOutcome.cancelled,
      LocalAuthExceptionCode.authInProgress: UnlockOutcome.cancelled,
      LocalAuthExceptionCode.temporaryLockout: UnlockOutcome.lockedOut,
      LocalAuthExceptionCode.biometricLockout: UnlockOutcome.lockedOut,
      LocalAuthExceptionCode.noCredentialsSet: UnlockOutcome.noScreenLock,
      LocalAuthExceptionCode.uiUnavailable: UnlockOutcome.failed,
      LocalAuthExceptionCode.deviceError: UnlockOutcome.failed,
      LocalAuthExceptionCode.unknownError: UnlockOutcome.failed,
    };
    for (final MapEntry(key: code, value: expected) in cases.entries) {
      final (outcome, _) = await _run(
        _FakeAuthenticator(error: LocalAuthException(code: code)),
      );
      expect(outcome, expected, reason: code.name);
    }
  });

  test('only a real unlock, the setting or no lock let the action through', () {
    for (final o in UnlockOutcome.values) {
      expect(
        o.allowed,
        o == UnlockOutcome.unlocked ||
            o == UnlockOutcome.notRequired ||
            o == UnlockOutcome.noScreenLock,
        reason: o.name,
      );
    }
  });

  test('unknown exceptions block instead of skipping', () async {
    final (a, _) = await _run(_FakeAuthenticator(error: StateError('boom')));
    expect(a, UnlockOutcome.failed);
    final (b, fake) = await _run(
      _FakeAuthenticator(lockError: StateError('no plugin')),
    );
    expect(b, UnlockOutcome.failed);
    expect(fake.prompts, isEmpty);
  });

  test('hasScreenLock for display falls back to false on errors', () async {
    final auth = DeviceAuth(_FakeAuthenticator(lockError: StateError('x')));
    expect(await auth.hasScreenLock(), isFalse);
  });

  test('the Android prompt\'s error names map like local_auth\'s', () {
    // MainActivity.unlockError answers with these names.
    final cases = {
      'userCanceled': UnlockOutcome.cancelled,
      'systemCanceled': UnlockOutcome.cancelled,
      'timeout': UnlockOutcome.cancelled,
      'authInProgress': UnlockOutcome.cancelled,
      'temporaryLockout': UnlockOutcome.lockedOut,
      'biometricLockout': UnlockOutcome.lockedOut,
      'noCredentialsSet': UnlockOutcome.noScreenLock,
      'unknownError': UnlockOutcome.failed,
      'somethingNew': UnlockOutcome.failed,
    };
    cases.forEach((name, outcome) {
      expect(
        DeviceAuth.outcomeForError(StrongDeviceAuthenticator.errorCode(name)),
        outcome,
        reason: name,
      );
    });
  });
}
