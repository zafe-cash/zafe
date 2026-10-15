import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../providers/device_lock_provider.dart';
import '../widgets/app_icon.dart';
import '../widgets/app_toast.dart';
import 'device_auth.dart';

/// Asks the owner to unlock before a sensitive action. Returns true when the action
/// may go ahead. On cancel or failure it shows a neutral toast and returns false,
/// so callers just `if (!await confirmUnlock(...)) return;`.
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
  if (!context.mounted) return false;
  final message = switch (outcome) {
    UnlockOutcome.unlocked || UnlockOutcome.notRequired => null,
    UnlockOutcome.noScreenLock =>
      await takeNoScreenLockWarning()
          ? 'This phone has no screen lock. Set one to protect approvals'
          : null,
    // Backing out of the prompt is its own answer: nothing to say.
    UnlockOutcome.cancelled => null,
    UnlockOutcome.lockedOut => 'Too many attempts. Try again later',
    UnlockOutcome.failed => 'Couldn\'t confirm it\'s you. Nothing was done',
  };
  if (!context.mounted) return false;
  if (message != null) {
    showAppToast(
      context,
      message,
      iconName: outcome == UnlockOutcome.noScreenLock
          ? AppIcons.warningCircle
          : AppIcons.lock,
      duration: outcome == UnlockOutcome.noScreenLock
          ? const Duration(seconds: 4)
          : AppToast.defaultDuration,
    );
  }
  return outcome.allowed;
}
