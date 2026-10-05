import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/feedback/payment_feedback.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  const channel = MethodChannel('xyz.zafe/payment_feedback');
  final messenger =
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;

  tearDown(() => messenger.setMockMethodCallHandler(channel, null));

  test('completing the signatures is the bigger moment', () {
    expect(approvalMoment(completed: false), PaymentMoment.approve);
    expect(approvalMoment(completed: true), PaymentMoment.ready);
  });

  test('only new transactions count as arrivals, never the first load', () {
    expect(newArrivals(null, ['a', 'b']), isEmpty);
    expect(newArrivals(['a'], ['a']), isEmpty);
    expect(newArrivals(['a'], ['b', 'a']), {'b'});
    expect(newArrivals(const [], ['a']), {'a'});
  });

  test('every moment has haptic taps starting at once', () {
    for (final m in PaymentMoment.values) {
      expect(PaymentFeedback.taps[m]!.first, 0, reason: m.name);
    }
  });

  test('plays through the native channel with the sound setting', () async {
    final calls = <MethodCall>[];
    messenger.setMockMethodCallHandler(channel, (call) async {
      calls.add(call);
      return true;
    });
    await PaymentFeedback.play(PaymentMoment.sent, sound: false);
    expect(calls.single.method, 'play');
    expect(calls.single.arguments, {'moment': 'sent', 'sound': false});
  });

  test('without a native handler it falls back to haptics', () async {
    final haptics = <String>[];
    messenger.setMockMethodCallHandler(SystemChannels.platform, (call) async {
      if (call.method == 'HapticFeedback.vibrate') {
        haptics.add(call.arguments as String);
      }
      return null;
    });
    await PaymentFeedback.play(PaymentMoment.ready, sound: true);
    await Future<void>.delayed(const Duration(milliseconds: 10));
    expect(haptics.length, PaymentFeedback.taps[PaymentMoment.ready]!.length);
    messenger.setMockMethodCallHandler(SystemChannels.platform, null);
  });
}
