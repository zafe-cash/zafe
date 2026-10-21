import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/features/send/recipients_csv.dart';
import 'package:zafe/src/rust/api/proposals.dart';

/// Fakes of the bridge validators: addresses starting with "u" are valid; amounts are
/// decimal ZEC with up to 8 places; memo length is UTF-8 bytes.
AddressCheck check(String a) => a.startsWith('u')
    ? const AddressCheck(valid: true, reason: '')
    : const AddressCheck(valid: false, reason: 'Not a Zcash address');

BigInt? zec(String t) {
  final m = RegExp(r'^(\d+)(?:\.(\d{1,8}))?$').firstMatch(t);
  if (m == null) return null;
  final frac = (m.group(2) ?? '').padRight(8, '0');
  return BigInt.parse(m.group(1)!) * BigInt.from(100000000) +
      BigInt.parse(frac);
}

RecipientsImport parse(String text, {int room = 50}) => parseRecipientsCsv(
  text,
  room: room,
  checkAddress: check,
  parseAmount: zec,
  memoLength: (m) => utf8.encode(m).length,
);

void main() {
  test('rows with an optional header, memo and quoted fields', () {
    final r = parse(
      'address,amount,memo\r\n'
      'ua1,1.5,Grant #1\r\n'
      '\r\n'
      'ua2,0.00000001\r\n'
      'ua3,2,"Milestone 2, part ""b""\nthanks"\r\n',
    );
    expect(r.errors, isEmpty);
    expect(r.ok, isTrue);
    expect(r.payments.map((p) => p.address), ['ua1', 'ua2', 'ua3']);
    expect(r.payments.map((p) => p.amountZat), [
      BigInt.from(150000000),
      BigInt.one,
      BigInt.from(200000000),
    ]);
    expect(r.payments[0].memo, 'Grant #1');
    expect(r.payments[1].memo, '');
    expect(r.payments[2].memo, 'Milestone 2, part "b"\nthanks');
  });

  test('no header and a BOM are fine', () {
    final r = parse('﻿ua1,3\nua2,4,hi');
    expect(r.errors, isEmpty);
    expect(r.payments, hasLength(2));
  });

  test('every bad row is reported with its line number, nothing imported', () {
    final r = parse(
      'address,amount,memo\n'
      'ua1,1\n'
      't1bad,1\n'
      'ua2,abc\n'
      'ua3,0\n'
      'ua4,1.123456789\n'
      'ua5\n'
      ',1\n'
      'ua6,1,${'x' * 513}\n'
      'ua7,1,memo,extra\n',
    );
    expect(r.payments, isEmpty);
    expect(r.ok, isFalse);
    expect(r.errors, [
      'Row 3: not a Zcash address',
      'Row 4: "abc" isn\'t a valid amount (up to 8 decimals)',
      'Row 5: amount must be more than 0',
      'Row 6: "1.123456789" isn\'t a valid amount (up to 8 decimals)',
      'Row 7: expected address,amount and an optional memo',
      'Row 8: address missing',
      'Row 9: memo is 513 bytes (at most 512)',
      'Row 10: expected address,amount and an optional memo',
    ]);
  });

  test('line numbers count lines inside quoted memos', () {
    final r = parse('ua1,1,"two\nlines"\nbad,1\n');
    expect(r.errors, ['Row 3: not a Zcash address']);
  });

  test('long error lists are cut short', () {
    final r = parse(List.filled(12, 'bad,1').join('\n'));
    expect(r.errors, hasLength(kMaxCsvErrors + 1));
    expect(r.errors.last, 'and 4 more problems');
  });

  test('the recipient cap is respected', () {
    final rows = List.generate(5, (i) => 'ua$i,1').join('\n');
    expect(parse(rows, room: 5).ok, isTrue);
    expect(parse(rows, room: 4).errors, [
      'The file has 5 recipients, but only 4 more fit in one payment',
    ]);
    expect(parse(rows, room: 0).errors, [
      'This payment already has the most recipients it can hold',
    ]);
  });

  test('an empty file has no recipients', () {
    expect(
      parse('address,amount\n\n').errors.single,
      startsWith('No recipients'),
    );
  });
}
