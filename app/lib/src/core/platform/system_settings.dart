import 'package:flutter/services.dart';

const _channel = MethodChannel('xyz.zafe/system_settings');

Future<bool> openSecuritySettings() async {
  try {
    return await _channel.invokeMethod<bool>('openSecurity') ?? false;
  } on MissingPluginException {
    return false;
  } on PlatformException {
    return false;
  }
}
