import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/features/signers/signers_screen.dart';

void main() {
  String t(int n, int t, int b) =>
      backupHealthText(signers: n, threshold: t, backedUp: b);

  test('every backup saved', () {
    expect(t(2, 2, 2), 'Every signer has saved a backup of their key.');
  });

  test('n-of-n: any loss without a backup loses the funds (no "more than 0")', () {
    expect(
      t(2, 2, 0),
      '2 signers haven\'t saved a backup yet. If any of them loses their phone, '
      'the funds are lost for good.',
    );
    expect(
      t(2, 2, 1),
      '1 signer hasn\'t saved a backup yet. If they lose their phone, the funds '
      'are lost for good.',
    );
  });

  test('within the margin', () {
    expect(
      t(3, 2, 2),
      '1 signer hasn\'t saved a backup yet. The vault keeps working if up to 1 '
      'signer loses their phone, but each loss without a backup uses up that margin.',
    );
  });

  test('beyond the margin', () {
    expect(
      t(4, 2, 1),
      '3 signers haven\'t saved a backup yet. If more than 2 of them lose their '
      'phones, the funds are lost for good.',
    );
  });
}
