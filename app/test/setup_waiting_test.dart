import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/features/onboarding/setup_screen.dart';

void main() {
  const a = '1d426b3769983ed225fd919e80f48a4de61c76cdf31e4c805e78b81873000bef';
  const b = '24bafbab373b21c99c7ea2e902f0034ec45a8c27a87f1880090043a3f458ec55';
  const c = 'dccacc0943e4b6f5fbcf9fa4b0b6be7b0b6e6b6e2d7f7f2a5c1a2b3c4d89b50b';

  test('names each member the setup waits for by short key', () {
    expect(waitingText([b]), 'Waiting for 24bafb...ec55.');
    expect(waitingText([b, c]), 'Waiting for 24bafb...ec55 and dccacc...b50b.');
  });

  test('never names this phone itself', () {
    expect(waitingText([a, b], me: a), isNot(contains('1d426b')));
    expect(waitingText([a], me: a), 'Waiting for the other members.');
  });
}
