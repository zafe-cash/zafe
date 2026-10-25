import 'dart:async';

import 'package:flutter/services.dart';

/// How long a copied secret stays on the clipboard before Zafe clears it (if nothing
/// else was copied meanwhile).
const kSecretClipboardLifetime = Duration(seconds: 60);

const _channel = MethodChannel('xyz.zafe/clipboard');

/// Copies a secret (an invite, backup text, a recovery code, the viewing key). On
/// Android it is flagged sensitive, so the system hides it from the copy preview and
/// keyboard suggestions, and on iOS it stays off Universal Clipboard and expires; everywhere it is cleared after [kSecretClipboardLifetime]
/// unless something else was copied since.
Future<void> copySecret(String text) async {
  try {
    await _channel.invokeMethod<void>('copySensitive', {'text': text});
  } on MissingPluginException {
    // No handler (tests): a plain copy.
    await Clipboard.setData(ClipboardData(text: text));
  }
  Timer(kSecretClipboardLifetime, () => unawaited(_clearIfUnchanged(text)));
}

Future<void> _clearIfUnchanged(String text) async {
  try {
    await _channel.invokeMethod<void>('clearIfUnchanged', {'text': text});
  } on MissingPluginException {
    final current = await Clipboard.getData(Clipboard.kTextPlain);
    if (current?.text == text) {
      await Clipboard.setData(const ClipboardData(text: ''));
    }
  } on PlatformException {
    // Best effort.
  }
}
