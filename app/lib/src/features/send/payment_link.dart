library;

class PendingPaymentLink {
  const PendingPaymentLink(this.raw, this.receivedAt);

  final String raw;
  final DateTime receivedAt;
}

const kPaymentLinkTtl = Duration(minutes: 10);

const kPaymentLinkExpiredMessage =
    'It expired before you unlocked. Open it again to pay.';

const kPaymentLinkNoVaultMessage =
    'Set up or join a vault, then open the payment link again.';

bool isPaymentLink(String raw) =>
    raw.trimLeft().toLowerCase().startsWith('zcash:');

bool paymentLinkExpired(PendingPaymentLink link, DateTime now) =>
    now.difference(link.receivedAt) > kPaymentLinkTtl;

BigInt minimumFeeZat(int recipients) =>
    BigInt.from(5000 * (recipients < 2 ? 2 : recipients));

enum FundsCheck { enough, short, unknown }

({FundsCheck check, BigInt shortByZat}) checkFunds({
  required BigInt requestedZat,
  required int recipients,
  BigInt? spendableZat,
  BigInt? totalZat,
}) {
  final unknown = (check: FundsCheck.unknown, shortByZat: BigInt.zero);
  if (requestedZat <= BigInt.zero) return unknown;
  final needed = requestedZat + minimumFeeZat(recipients);
  final have = spendableZat ?? totalZat;
  if (have == null) return unknown;
  if (have < needed) {
    return (check: FundsCheck.short, shortByZat: needed - have);
  }
  return spendableZat != null
      ? (check: FundsCheck.enough, shortByZat: BigInt.zero)
      : unknown;
}
