import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/errors/sync_failure.dart' show fromUnixSeconds;
import '../../core/errors/zafe_error_copy.dart';
import '../../core/security/unlock_gate.dart';
import '../../core/widgets/action_button.dart' show ActionCancelled;
import '../../providers/proposals_provider.dart';

/// Time left until `completesAt` (Unix seconds, this phone's own clock), short:
/// "2 d 5 h", "3 h", "under 1 h"; null when unknown or past.
String? timeLeft(BigInt? completesAt, {DateTime? now}) {
  final at = fromUnixSeconds(completesAt);
  if (at == null) return null;
  final left = at.difference(now ?? DateTime.now());
  if (left.isNegative) return null;
  if (left.inHours < 1) return 'under 1 h';
  if (left.inDays < 1) return '${left.inHours} h';
  final hours = left.inHours - left.inDays * 24;
  return hours == 0 ? '${left.inDays} d' : '${left.inDays} d $hours h';
}

/// A Unix time (seconds) as "13 Nov".
String? dayOf(BigInt? secs) {
  final at = fromUnixSeconds(secs);
  if (at == null) return null;
  const months = [
    'Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', //
    'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec',
  ];
  return '${at.day} ${months[at.month - 1]}';
}

/// Keep my seat, from the old phone: the unlock, then the keep. Only ever from the
/// owner's tap. Throws [ActionCancelled] when the unlock is cancelled (for an
/// [ActionButton]).
Future<void> keepMySeatOrThrow(BuildContext context, WidgetRef ref) async {
  if (!await confirmUnlock(
    context,
    ref,
    reason: 'Keep your seat',
    checkSeat: false,
  )) {
    throw const ActionCancelled();
  }
  await ref.read(proposalsProvider.notifier).keepSeat();
}

/// [keepMySeatOrThrow] for a card: why it failed, or null when kept or cancelled.
Future<String?> keepMySeat(BuildContext context, WidgetRef ref) async {
  try {
    await keepMySeatOrThrow(context, ref);
    return null;
  } on ActionCancelled {
    return null;
  } catch (e) {
    return zafeErrorMessage(
      e,
      fallback: 'Couldn\'t keep your seat. Try again.',
    );
  }
}
