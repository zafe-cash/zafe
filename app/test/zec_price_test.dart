import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/providers/zec_price_provider.dart';

void main() {
  test('dollars with thousands separators and cents', () {
    final zat = BigInt.from(4237485000); // 42.37485 ZEC
    expect(fiatText(zat, 133.77), r'$5,668.48');
    expect(fiatText(BigInt.zero, 133.77), r'$0.00');
    expect(
      fiatText(BigInt.from(100000000) * BigInt.from(10000), 1000),
      r'$10,000,000.00',
    );
    expect(fiatText(zat, null), isNull);
  });

  test('a cached price is read back and expires after an hour', () {
    final at = DateTime.utc(2026, 10, 6, 12);
    final price = ZecPrice.decode(ZecPrice(41.5, at).encode())!;
    expect(price.usd, 41.5);
    expect(price.freshAt(at.add(const Duration(minutes: 59))), isTrue);
    expect(price.freshAt(at.add(const Duration(hours: 1))), isFalse);
    // A clock that went backwards doesn't make an old price look fresh.
    expect(price.freshAt(at.subtract(const Duration(minutes: 1))), isFalse);
  });

  test('a bad cache is ignored', () {
    for (final raw in [
      null,
      '',
      'x',
      '{}',
      '{"usd":-1,"at":1}',
      '{"usd":"1","at":1}',
    ]) {
      expect(ZecPrice.decode(raw), isNull, reason: raw);
    }
  });

  test('dollars only on mainnet builds', () {
    expect(kShowsFiat, isFalse); // tests build for regtest
  });
}
