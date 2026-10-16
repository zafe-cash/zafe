import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/providers/respond_policy.dart';

void main() {
  final t0 = DateTime(2026, 10, 17, 12);
  DateTime at(int seconds) => t0.add(Duration(seconds: seconds));

  test('without the live watch every refresh answers', () {
    final policy = RespondPolicy();
    expect(policy.take(at(0)), isTrue);
    expect(policy.take(at(1)), isTrue);
  });

  test('while live: on deliveries, and at least once a minute', () {
    final policy = RespondPolicy()..live = true;
    expect(policy.take(at(0)), isTrue, reason: 'first refresh');
    expect(policy.take(at(5)), isFalse);
    policy.inboxNews();
    expect(policy.take(at(6)), isTrue);
    expect(policy.take(at(7)), isFalse);
    expect(policy.take(at(66)), isTrue, reason: 'a minute after the last');
  });

  test('news during an answer and a failed answer are not lost', () {
    final policy = RespondPolicy()..live = true;
    expect(policy.take(at(0)), isTrue);
    policy.inboxNews(); // arrives while answering
    expect(policy.take(at(1)), isTrue);
    policy.failed();
    expect(policy.take(at(2)), isTrue);
    expect(policy.take(at(3)), isFalse);
  });
}
