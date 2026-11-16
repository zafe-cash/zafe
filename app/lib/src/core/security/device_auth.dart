import 'dart:io';

import 'package:flutter/services.dart';
import 'package:local_auth/local_auth.dart';

enum UnlockOutcome {
  unlocked,

  notRequired,

  noScreenLock,

  cancelled,

  lockedOut,

  failed;

  bool get allowed => switch (this) {
    unlocked || notRequired || noScreenLock => true,
    cancelled || lockedOut || failed => false,
  };
}

abstract interface class DeviceAuthenticator {
  Future<bool> hasScreenLock();

  Future<bool> authenticate(String reason);
}

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

  static LocalAuthExceptionCode errorCode(String name) =>
      LocalAuthExceptionCode.values.asNameMap()[name] ??
      LocalAuthExceptionCode.unknownError;
}

DeviceAuthenticator platformAuthenticator() => Platform.isAndroid
    ? StrongDeviceAuthenticator()
    : LocalDeviceAuthenticator();

class DeviceAuth {
  const DeviceAuth(this._authenticator);
  final DeviceAuthenticator _authenticator;

  Future<bool> hasScreenLock() async {
    try {
      return await _authenticator.hasScreenLock();
    } catch (_) {
      return false;
    }
  }

  Future<UnlockOutcome> unlock({
    required bool required,
    required String reason,
  }) async {
    if (!required) return UnlockOutcome.notRequired;
    final bool hasLock;
    try {
      hasLock = await _authenticator.hasScreenLock();
    } catch (_) {
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

  static bool get prompting => _prompting > 0;

  static UnlockOutcome outcomeForError(LocalAuthExceptionCode code) =>
      switch (code) {
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
