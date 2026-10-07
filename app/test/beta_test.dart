import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/config/beta.dart';

void main() {
  final cap = 500000000; // 5 ZEC
  test('over the cap only in the beta, and only strictly above it', () {
    expect(overBetaCap(BigInt.from(cap), beta: true, capZat: cap), isFalse);
    expect(overBetaCap(BigInt.from(cap + 1), beta: true, capZat: cap), isTrue);
    expect(overBetaCap(BigInt.from(cap + 1), beta: false, capZat: cap), isFalse);
    expect(overBetaCap(null, beta: true, capZat: cap), isFalse);
    // A cap of 0 turns the limit off.
    expect(overBetaCap(BigInt.from(1 << 40), beta: true, capZat: 0), isFalse);
  });

  test('the copy names the cap', () {
    expect(betaCapText(capZat: cap), contains('5'));
    expect(betaNote(capZat: cap), contains(betaCapText(capZat: cap)));
    expect(betaOverCapNote(capZat: cap), contains('more than'));
  });

  test('regtest builds are not beta', () {
    // The tests run as a regtest build.
    expect(kIsBeta, isFalse);
  });
}
