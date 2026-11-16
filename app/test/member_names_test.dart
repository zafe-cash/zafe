import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/formatting/member_label.dart';
import 'package:zafe/src/core/storage/member_names.dart';
import 'package:zafe/src/rust/api/names.dart';

void main() {
  const key =
      '97c9ce75aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa123e';

  test('a member shows as You, this phone\'s label, or a short key', () {
    expect(
      memberLabel(
        key,
        me: key,
        names: SignerNames(local: {key: 'Alice'}),
      ),
      'You',
    );
    expect(
      memberLabel(key, names: SignerNames(local: {key: 'Alice'})),
      'Alice',
    );
    expect(
      memberLabel(key, names: SignerNames(local: {key: ''})),
      '97c9ce...123e',
    );
    expect(memberLabel(key), '97c9ce...123e');
  });

  test('a name a member gave itself always shows with its key', () {
    final names = SignerNames(shared: {key: 'Alic\u0435'});
    expect(memberLabel(key, names: names), 'Alic\u0435 (97c9ce...123e)');
    expect(names.nameOf(key), 'Alic\u0435');
    expect(names.isClaim(key), isTrue);
  });

  test('a claim of another key\'s label is dropped', () {
    final alice = 'a' * 64;
    final mallory = 'c' * 64;
    final names = SignerNames(
      local: {alice: 'Alice'},
      shared: {mallory: 'alice ', alice: 'Ally'},
    );
    expect(memberLabel(alice, names: names), 'Alice');
    expect(memberLabel(mallory, names: names), 'cccccc...cccc');
    expect(names.merged, {alice: 'Alice'});
  });

  test('this phone\'s label wins over the name a member shared', () {
    final other = 'b' * 64;
    expect(
      MemberNames.merge(
        {key: 'Alice', other: 'Bob'},
        {key: 'Alice (treasurer)'},
      ),
      {key: 'Alice (treasurer)', other: 'Bob'},
    );
    expect(MemberNames.merge(const {}, {key: 'A'}), {key: 'A'});
  });

  test('names are trimmed, single-spaced and capped', () {
    expect(MemberNames.clean('  Alice \n  (treasurer) '), 'Alice (treasurer)');
    expect(MemberNames.clean('   '), '');
    expect(MemberNames.clean('x' * 50).length, MemberNames.maxLength);
  });

  test('names round-trip through backups, cleaned on restore', () {
    final other = 'b' * 64;
    final signers = MemberNames.toSigners({key: 'Alice'});
    expect(signers, [SignerName(keyHex: key, name: 'Alice')]);
    expect(
      MemberNames.fromSigners([
        ...signers,
        SignerName(keyHex: other, name: '  Bob   the  builder '),
        SignerName(keyHex: 'c' * 64, name: '   '),
        const SignerName(keyHex: 'not-hex', name: 'Mallory'),
      ]),
      {key: 'Alice', other: 'Bob the builder'},
    );
  });
}
