import 'dart:async';

import 'package:flutter/widgets.dart';

import '../platform/sensitive_clipboard.dart';
import '../theme/app_theme.dart';

class ClipboardCountdown extends StatefulWidget {
  const ClipboardCountdown({super.key, required this.copiedAt});

  final DateTime copiedAt;

  @override
  State<ClipboardCountdown> createState() => _ClipboardCountdownState();
}

class _ClipboardCountdownState extends State<ClipboardCountdown> {
  Timer? _tick;

  @override
  void initState() {
    super.initState();
    _start();
  }

  @override
  void didUpdateWidget(ClipboardCountdown old) {
    super.didUpdateWidget(old);
    if (old.copiedAt != widget.copiedAt) _start();
  }

  void _start() {
    _tick?.cancel();
    _tick = Timer.periodic(const Duration(seconds: 1), (t) {
      if (!mounted) return;
      setState(() {});
      if (_left == Duration.zero) t.cancel();
    });
  }

  Duration get _left {
    final left =
        kSecretClipboardLifetime - DateTime.now().difference(widget.copiedAt);
    return left.isNegative ? Duration.zero : left;
  }

  @override
  void dispose() {
    _tick?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final left = _left;
    final seconds = (left.inMilliseconds / 1000).ceil();
    return Text(
      seconds > 0
          ? 'On the clipboard for $seconds s'
          : 'No longer on the clipboard',
      textAlign: TextAlign.center,
      style: AppTypography.bodySmall.copyWith(
        color: context.colors.text.secondary,
      ),
    );
  }
}
