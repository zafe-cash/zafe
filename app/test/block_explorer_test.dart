import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/config/block_explorer.dart';

void main() {
  const txid =
      '10616fc1aa000000000000000000000000000000000000000000000000c84a8447';

  test('transactions open on ZecBlock for the build network', () {
    expect(
      explorerTxUri(txid, network: 'main').toString(),
      'https://zecblock.com/tx/$txid',
    );
    expect(
      explorerTxUri(txid, network: 'test').toString(),
      'https://testnet.zecblock.com/tx/$txid',
    );
  });

  test('regtest has no explorer', () {
    expect(explorerTxUri(txid, network: 'regtest'), isNull);
  });
}
