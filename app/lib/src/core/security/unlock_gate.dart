import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../providers/device_lock_provider.dart';

/// Asks the owner to unlock before a sensitive action. Returns true when the action
/// may go ahead, so callers just `if (!await confirmUnlock(...)) return;`.
/// Always asked: a vault never approves, sends or exports without the owner (there is
/// no setting to turn it off).
Future<bool> confirmUnlock(
  BuildContext context,
  WidgetRef ref, {
  required String reason,
}) async {
  final outcome = await ref
      .read(deviceAuthProvider)
      .unlock(required: true, reason: reason);
  // The system prompt already says why it failed or locked out; a phone without a
  // screen lock gets a permanent note in Settings.
  return outcome.allowed;
}
