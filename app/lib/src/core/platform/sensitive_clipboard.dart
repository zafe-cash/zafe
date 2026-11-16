import 'dart:async';

import 'package:flutter/services.dart';

const kSecretClipboardLifetime = Duration(seconds: 60);

const _channel = MethodChannel('xyz.zafe/clipboard');

Future<void> copySecret(String text) async {
  try {
    await _channel.invokeMethod<void>('copySensitive', {'text': text});
  } on MissingPluginException {
    await Clipboard.setData(ClipboardData(text: text));
  }
  Timer(kSecretClipboardLifetime, () => unawaited(_clearIfUnchanged(text)));
}

Future<void> _clearIfUnchanged(String text) async {
  try {
    await _channel.invokeMethod<void>('clearIfUnchanged', {'text': text});
  } on MissingPluginException {
    await _clearInDart(text);
  } on PlatformException {
    await _clearInDart(text);
  }
}

Future<void> _clearInDart(String text) async {
  final current = await Clipboard.getData(Clipboard.kTextPlain);
  if (current?.text == text) {
    await Clipboard.setData(const ClipboardData(text: ''));
  }
}
