import '../../rust/api/proposals.dart' as rust;

const kMaxMemoBytes = 512;

const kMaxCsvErrors = 8;

class RecipientsImport {
  const RecipientsImport({required this.payments, required this.errors});

  final List<rust.PaymentInput> payments;

  final List<String> errors;

  bool get ok => errors.isEmpty && payments.isNotEmpty;
}

RecipientsImport parseRecipientsCsv(
  String text, {
  required int room,
  required rust.AddressCheck Function(String address) checkAddress,
  required BigInt? Function(String amount) parseAmount,
  required int Function(String memo) memoLength,
}) {
  final payments = <rust.PaymentInput>[];
  final errors = <String>[];
  var first = true;
  for (final (line, cells) in _csvRows(text)) {
    if (cells.every((c) => c.trim().isEmpty)) continue;
    final header = first && cells.first.trim().toLowerCase() == 'address';
    first = false;
    if (header) continue;
    final where = 'Row $line';
    final extra = cells.skip(3).any((c) => c.trim().isNotEmpty);
    if (cells.length < 2 || extra) {
      errors.add('$where: expected address,amount and an optional memo');
      continue;
    }
    final address = cells[0].trim();
    final amountText = cells[1].trim();
    final memo = cells.length > 2 ? cells[2] : '';
    final problems = <String>[];
    if (address.isEmpty) {
      problems.add('address missing');
    } else {
      final check = checkAddress(address);
      if (!check.valid) {
        problems.add(
          check.reason.isEmpty ? 'invalid address' : _lower(check.reason),
        );
      }
    }
    final amount = amountText.isEmpty ? null : parseAmount(amountText);
    if (amountText.isEmpty) {
      problems.add('amount missing');
    } else if (amount == null) {
      problems.add('"$amountText" isn\'t a valid amount (up to 8 decimals)');
    } else if (amount <= BigInt.zero) {
      problems.add('amount must be more than 0');
    }
    final memoBytes = memoLength(memo);
    if (memoBytes > kMaxMemoBytes) {
      problems.add('memo is $memoBytes bytes (at most $kMaxMemoBytes)');
    }
    if (problems.isNotEmpty) {
      errors.add('$where: ${problems.join('; ')}');
      continue;
    }
    payments.add(
      rust.PaymentInput(address: address, amountZat: amount!, memo: memo),
    );
  }
  if (errors.isEmpty && payments.isEmpty) {
    errors.add(
      'No recipients found. Use one row per payment: address,amount,memo',
    );
  }
  if (errors.isEmpty && payments.length > room) {
    errors.add(
      room <= 0
          ? 'This payment already has the most recipients it can hold'
          : 'The file has ${payments.length} recipients, but only $room more fit in '
                'one payment',
    );
  }
  if (errors.length > kMaxCsvErrors) {
    final more = errors.length - kMaxCsvErrors;
    errors
      ..removeRange(kMaxCsvErrors, errors.length)
      ..add('and $more more ${more == 1 ? 'problem' : 'problems'}');
  }
  return RecipientsImport(
    payments: errors.isEmpty ? payments : const [],
    errors: errors,
  );
}

String _lower(String s) =>
    s.isEmpty ? s : '${s[0].toLowerCase()}${s.substring(1)}';

List<(int, List<String>)> _csvRows(String text) {
  if (text.startsWith('﻿')) text = text.substring(1);
  final rows = <(int, List<String>)>[];
  var cells = <String>[];
  final cell = StringBuffer();
  var line = 1;
  var start = 1;
  var quoted = false;
  var any = false;
  void endCell() {
    cells.add(cell.toString());
    cell.clear();
  }

  void endRow() {
    endCell();
    rows.add((start, cells));
    cells = <String>[];
    any = false;
  }

  for (var i = 0; i < text.length; i++) {
    final c = text[i];
    if (quoted) {
      if (c == '"') {
        if (i + 1 < text.length && text[i + 1] == '"') {
          cell.write('"');
          i++;
        } else {
          quoted = false;
        }
      } else {
        if (c == '\n') line++;
        cell.write(c);
      }
      continue;
    }
    switch (c) {
      case '"':
        quoted = true;
        any = true;
      case ',':
        endCell();
        any = true;
      case '\r':
        break;
      case '\n':
        endRow();
        line++;
        start = line;
      default:
        cell.write(c);
        any = true;
    }
  }
  if (any || cell.isNotEmpty) endRow();
  return rows;
}
