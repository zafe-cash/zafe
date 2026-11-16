import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../diagnostics/crash_log.dart';

class SecureScreen extends StatefulWidget {
  const SecureScreen({super.key, required this.child});
  final Widget child;

  static const _channel = MethodChannel('xyz.zafe/secure_screen');
  static int _count = 0;

  static Future<void> _set(bool secure) async {
    try {
      await _channel.invokeMethod<void>('setSecure', secure);
    } on Exception catch (error, stack) {
      await CrashLog.instance?.record('secure-screen', error, stack);
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
