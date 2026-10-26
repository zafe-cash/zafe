import 'dart:async';

import 'package:flutter/widgets.dart';

import '../platform/sensitive_clipboard.dart';
import '../theme/app_theme.dart';

/// Under a button that copied a secret with [copySecret]: how long it stays on the
/// clipboard ("On the clipboard for 42 s"), then that it's gone. Zafe clears a copied
/// secret after [kSecretClipboardLifetime], so a backup copied now and pasted later
/// (into a password manager, or a restore after wiping the app) would otherwise be
/// silently missing.
class ClipboardCountdown extends StatefulWidget {
  const ClipboardCountdown({super.key, required this.copiedAt});

  /// When the secret was copied; restarts the countdown when it changes.
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
    // Whole seconds, rounded up: "1 s" until it is really gone.
    final seconds = (left.inMilliseconds / 1000).ceil();
    return Text(
      // Not "cleared": Zafe leaves the clipboard alone if something else was copied
      // since, and either way the secret is no longer there to paste.
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
