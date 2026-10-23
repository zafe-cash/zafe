import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

/// Blocks screenshots, screen recording and the app-switcher thumbnail (Android
/// `FLAG_SECURE`) while any [SecureScreen] is mounted. Counted, so a secure screen pushed
/// over another doesn't unblock capture when it closes. iOS can't block a screenshot: it
/// covers the app while the screen is recorded or mirrored (`ZafePrivacy`), and always
/// covers it in the app switcher. No-op where unsupported (tests).
class SecureScreen extends StatefulWidget {
  const SecureScreen({super.key, required this.child});
  final Widget child;

  static const _channel = MethodChannel('xyz.zafe/secure_screen');
  static int _count = 0;

  static Future<void> _set(bool secure) async {
    try {
      await _channel.invokeMethod<void>('setSecure', secure);
    } on MissingPluginException {
      // Platform without a handler (tests).
    } on PlatformException {
      // Best effort: never block the screen itself.
    }
  }

  @override
  State<SecureScreen> createState() => _SecureScreenState();
}

class _SecureScreenState extends State<SecureScreen> {
  @override
  void initState() {
    super.initState();
    if (SecureScreen._count++ == 0) SecureScreen._set(true);
  }

  @override
  void dispose() {
    if (--SecureScreen._count == 0) SecureScreen._set(false);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => widget.child;
}
