import 'package:flutter/services.dart';

abstract final class AppHaptics {
  static const _channel = MethodChannel('xyz.zafe/haptics');

  static Future<void> digit() => HapticFeedback.lightImpact();

  static Future<void> auxiliaryKey() => HapticFeedback.selectionClick();

  static Future<void> privacyToggle() => HapticFeedback.mediumImpact();

  static Future<void> copy() => HapticFeedback.lightImpact();

  static Future<void> votingAnswersComplete() => HapticFeedback.lightImpact();

  static Future<void> error() async {
    if (await _nativeError()) return;
    await HapticFeedback.heavyImpact();
    await Future<void>.delayed(const Duration(milliseconds: 90));
    await HapticFeedback.heavyImpact();
  }

  static Future<bool> _nativeError() async {
    try {
      return await _channel.invokeMethod<bool>('error') ?? false;
    } on PlatformException {
      return false;
    } on MissingPluginException {
      return false;
    }
  }
}
