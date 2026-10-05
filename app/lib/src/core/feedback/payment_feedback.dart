import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

/// The moments a payment goes through, each with its own sound and haptic taps.
///
/// The sounds are one signature (scripts/sounds/sounds.py): approving strikes its first
/// note, the approval that completes the signatures strikes two, and the payment going
/// out plays all three.
enum PaymentMoment { approve, ready, sent, received, failed }

/// Plays payment moments through `xyz.zafe/payment_feedback` (Android: SoundPool for
/// the sound, silenced on silent/vibrate; view haptics in the sound's tempo).
abstract final class PaymentFeedback {
  static const _channel = MethodChannel('xyz.zafe/payment_feedback');

  /// Haptic taps per moment (ms), matching the native handler: the fallback where the
  /// platform has no handler (iOS until it gets one).
  @visibleForTesting
  static const taps = <PaymentMoment, List<int>>{
    PaymentMoment.approve: [0],
    PaymentMoment.ready: [0, 120],
    PaymentMoment.sent: [0, 130, 260],
    PaymentMoment.received: [0, 110],
    PaymentMoment.failed: [0, 120],
  };

  /// Fire-and-forget: never delays the caller. `sound` is the "Payment sounds" setting;
  /// haptics play either way.
  static Future<void> play(PaymentMoment moment, {required bool sound}) async {
    try {
      final handled = await _channel.invokeMethod<bool>('play', {
        'moment': moment.name,
        'sound': sound,
      });
      if (handled == true) return;
    } on PlatformException {
      // Fall through to haptics alone.
    } on MissingPluginException {
      // No native handler (iOS for now, tests): haptics alone.
    }
    var last = 0;
    for (final at in taps[moment]!) {
      await Future<void>.delayed(Duration(milliseconds: at - last));
      last = at;
      unawaited(HapticFeedback.lightImpact());
    }
  }
}

/// What an approval sounds like: completing the signatures is a bigger moment than
/// adding one.
PaymentMoment approvalMoment({required bool completed}) =>
    completed ? PaymentMoment.ready : PaymentMoment.approve;

/// Transaction ids in `next` that weren't in `previous`: money that arrived while the
/// list was on screen. Nothing on the first load (`previous == null`), so history and
/// switching vaults stay quiet.
Set<String> newArrivals(Iterable<String>? previous, Iterable<String> next) {
  if (previous == null) return const {};
  return next.toSet().difference(previous.toSet());
}
