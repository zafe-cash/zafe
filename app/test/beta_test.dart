import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/config/beta.dart';

void main() {
  test('the note says beta and sets no limit', () {
    expect(betaNote, contains('beta'));
    expect(betaNote.toLowerCase(), isNot(contains('no more than')));
  });

  test('regtest builds are not beta', () {
    // The tests run as a regtest build.
    expect(kIsBeta, isFalse);
  });
}
