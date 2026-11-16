import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

enum PaymentMoment { approve, ready, sent, received, failed }

abstract final class PaymentFeedback {
  static const _channel = MethodChannel('xyz.zafe/payment_feedback');

  @visibleForTesting
  static const taps = <PaymentMoment, List<int>>{
    PaymentMoment.approve: [0],
    PaymentMoment.ready: [0, 120],
    PaymentMoment.sent: [0, 130, 260],
    PaymentMoment.received: [0, 110],
    PaymentMoment.failed: [0, 120],
  };

  static Future<bool> _playNative(
    PaymentMoment moment, {
    required bool sound,
  }) async {
    try {
      return await _channel.invokeMethod<bool>('play', {
            'moment': moment.name,
            'sound': sound,
          }) ??
          false;
    } on PlatformException {
      return false;
    } on MissingPluginException {
      return false;
    }
  }

  static Future<void> play(PaymentMoment moment, {required bool sound}) async {
    if (await _playNative(moment, sound: sound)) return;
    var last = 0;
    for (final at in taps[moment]!) {
      await Future<void>.delayed(Duration(milliseconds: at - last));
      last = at;
      unawaited(HapticFeedback.lightImpact());
    }
  }
}

PaymentMoment approvalMoment({required bool completed}) =>
    completed ? PaymentMoment.ready : PaymentMoment.approve;

Set<String> newArrivals(Iterable<String>? previous, Iterable<String> next) {
  if (previous == null) return const {};
  return next.toSet().difference(previous.toSet());
}
