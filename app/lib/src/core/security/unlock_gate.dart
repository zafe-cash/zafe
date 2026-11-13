import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../features/signers/keep_seat.dart' show keepMySeat, timeLeft;
import '../../providers/device_lock_provider.dart';
import '../../providers/proposals_provider.dart';
import '../../providers/vault_provider.dart';
import '../widgets/app_confirm.dart';

/// Asks the owner to unlock before a sensitive action. Returns true when the action
/// may go ahead, so callers just `if (!await confirmUnlock(...)) return;`.
/// Always asked: a vault never approves, sends or exports without the owner (there is
/// no setting to turn it off).
///
/// While this phone's own seat is moving to a new phone (every approval is in), any such
/// action asks first whether to keep it (`checkSeat`): the owner is plainly using this
/// phone. The action goes on either way.
Future<bool> confirmUnlock(
  BuildContext context,
  WidgetRef ref, {
  required String reason,
  bool checkSeat = true,
}) async {
  final pending = checkSeat
      ? ref
            .read(proposalsProvider)
            .myPendingSeat(ref.read(vaultProvider).myKeyHex)
      : null;
  if (pending != null && context.mounted) {
    final left = timeLeft(pending.completesAt);
    final keep = await showAppConfirm(
      context,
      title: 'Your seat is moving to a new phone',
      body:
          'Your co-signers approved moving it${left == null ? '' : '; it moves in $left'}. '
          'If this is your phone, keep your seat and call them.',
      confirmLabel: 'Keep my seat',
      cancelLabel: 'Not now',
      destructive: false,
    );
    if (keep && context.mounted) await keepMySeat(context, ref);
    if (!context.mounted) return false;
  }
  final outcome = await ref
      .read(deviceAuthProvider)
      .unlock(required: true, reason: reason);
  // The system prompt already says why it failed or locked out; a phone without a
  // screen lock gets a permanent note in Settings.
  return outcome.allowed;
}
