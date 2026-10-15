import 'dart:io';

import 'package:flutter/services.dart';
import 'package:local_auth/local_auth.dart';

/// Result of asking the phone's owner to unlock before a sensitive action.
enum UnlockOutcome {
  /// The owner unlocked (biometrics or PIN/pattern/password).
  unlocked,

  /// The "Require unlock to approve" setting is off.
  notRequired,

  /// The phone has no screen lock at all, so there is nothing to prompt with.
  /// The action goes ahead; the app warns once and in Settings.
  noScreenLock,

  /// The owner dismissed the prompt, or the system interrupted it.
  cancelled,

  /// Too many failed attempts; the phone asks to wait or unlock another way.
  lockedOut,

  /// The prompt couldn't be shown or failed for another reason.
  failed;

  /// Whether the gated action may go ahead.
  bool get allowed => switch (this) {
    unlocked || notRequired || noScreenLock => true,
    cancelled || lockedOut || failed => false,
  };
}

/// The platform side of the unlock prompt, behind an interface so the decision
/// logic can be tested with a fake.
abstract interface class DeviceAuthenticator {
  /// True when the phone has a screen lock or enrolled biometrics.
  Future<bool> hasScreenLock();

  /// Shows the system prompt. Returns false when the owner fails or dismisses it
  /// without an error; throws [LocalAuthException] for everything else.
  Future<bool> authenticate(String reason);
}

/// [DeviceAuthenticator] backed by `local_auth`: biometrics or the device
/// credential (PIN, pattern, password), whichever the owner picks.
class LocalDeviceAuthenticator implements DeviceAuthenticator {
  LocalDeviceAuthenticator([LocalAuthentication? auth])
    : _auth = auth ?? LocalAuthentication();
  final LocalAuthentication _auth;

  @override
  Future<bool> hasScreenLock() => _auth.isDeviceSupported();

  @override
  Future<bool> authenticate(String reason) => _auth.authenticate(
    localizedReason: reason,
    biometricOnly: false,
    persistAcrossBackgrounding: true,
  );
}

/// [DeviceAuthenticator] for Android (`xyz.zafe/device_auth` in `MainActivity`): a
/// strong biometric (class 3) or the PIN, pattern or password. local_auth also offers
/// weak biometrics (camera face unlock, which a photo can fool) and can't be told not to.
class StrongDeviceAuthenticator implements DeviceAuthenticator {
  static const _channel = MethodChannel('xyz.zafe/device_auth');

  @override
  Future<bool> hasScreenLock() async =>
      await _channel.invokeMethod<bool>('hasScreenLock') ?? false;

  @override
  Future<bool> authenticate(String reason) async {
    final error = await _channel.invokeMethod<String>('authenticate', {
      'reason': reason,
    });
    if (error == null) return true;
    throw LocalAuthException(code: errorCode(error));
  }

  /// The native side's error names are local_auth's, so [DeviceAuth] maps both alike.
  static LocalAuthExceptionCode errorCode(String name) =>
      LocalAuthExceptionCode.values.asNameMap()[name] ??
      LocalAuthExceptionCode.unknownError;
}

/// The authenticator for this platform.
DeviceAuthenticator platformAuthenticator() => Platform.isAndroid
    ? StrongDeviceAuthenticator()
    : LocalDeviceAuthenticator();

/// Decides whether to prompt, skip or block, and maps the platform's answer to an
/// [UnlockOutcome]. Pure apart from the authenticator.
class DeviceAuth {
  const DeviceAuth(this._authenticator);
  final DeviceAuthenticator _authenticator;

  /// For display only (the Settings warning); [unlock] makes its own check.
  Future<bool> hasScreenLock() async {
    try {
      return await _authenticator.hasScreenLock();
    } catch (_) {
      return false;
    }
  }

  /// Asks for an unlock when [required]. Never throws.
  Future<UnlockOutcome> unlock({
    required bool required,
    required String reason,
  }) async {
    if (!required) return UnlockOutcome.notRequired;
    final bool hasLock;
    try {
      hasLock = await _authenticator.hasScreenLock();
    } catch (_) {
      // Can't tell: fail closed rather than skip the prompt.
      return UnlockOutcome.failed;
    }
    if (!hasLock) return UnlockOutcome.noScreenLock;
    _prompting++;
    try {
      final ok = await _authenticator.authenticate(reason);
      return ok ? UnlockOutcome.unlocked : UnlockOutcome.cancelled;
    } on LocalAuthException catch (e) {
      return outcomeForError(e.code);
    } catch (_) {
      return UnlockOutcome.failed;
    } finally {
      _prompting--;
    }
  }

  static int _prompting = 0;

  /// True while a system unlock prompt is up. The PIN screen is another activity, so
  /// the app goes to the background behind it; the app lock must not count that.
  static bool get prompting => _prompting > 0;

  static UnlockOutcome outcomeForError(LocalAuthExceptionCode code) =>
      switch (code) {
        // The lock was removed between the check and the prompt.
        LocalAuthExceptionCode.noCredentialsSet => UnlockOutcome.noScreenLock,
        LocalAuthExceptionCode.userCanceled ||
        LocalAuthExceptionCode.systemCanceled ||
        LocalAuthExceptionCode.timeout ||
        LocalAuthExceptionCode.userRequestedFallback ||
        LocalAuthExceptionCode.authInProgress => UnlockOutcome.cancelled,
        LocalAuthExceptionCode.temporaryLockout ||
        LocalAuthExceptionCode.biometricLockout => UnlockOutcome.lockedOut,
        _ => UnlockOutcome.failed,
      };
}
