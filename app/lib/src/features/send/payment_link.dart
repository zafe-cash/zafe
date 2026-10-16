/// ZIP 321 payment links (`zcash:...`) opened from other apps or websites ("Pay with
/// Zcash"). Pure, so the rules are unit-tested without Rust or a router.
library;

/// A `zcash:` link waiting to be shown on the payment request screen.
class PendingPaymentLink {
  const PendingPaymentLink(this.raw, this.receivedAt);

  /// The link exactly as the system delivered it (trimmed); parsed by Rust later.
  final String raw;
  final DateTime receivedAt;
}

/// A link that waited longer than this (the app was locked, or a vault was being set
/// up) is dropped instead of shown: paying a request opened long ago, maybe by accident,
/// is worse than asking for the link again.
const kPaymentLinkTtl = Duration(minutes: 10);

/// Shown when [kPaymentLinkTtl] ran out before the link could be shown.
const kPaymentLinkExpiredMessage =
    'It expired before you unlocked. Open it again to pay.';

/// Shown when a link arrives on a device with no vault to pay from.
const kPaymentLinkNoVaultMessage =
    'Set up or join a vault, then open the payment link again.';

/// Whether [raw] is a ZIP 321 payment link (case-insensitive `zcash:` scheme). Matched
/// on the string, since a malformed link must still reach the screen that says so.
bool isPaymentLink(String raw) =>
    raw.trimLeft().toLowerCase().startsWith('zcash:');

/// Whether [link] waited too long to be shown at [now].
bool paymentLinkExpired(PendingPaymentLink link, DateTime now) =>
    now.difference(link.receivedAt) > kPaymentLinkTtl;

/// The least a payment to [recipients] addresses can cost under ZIP 317 (5,000 zats per
/// action, at least 2 actions; each recipient needs an output). The real fee is known
/// once the proposal is built and can be higher (more notes to spend, change).
BigInt minimumFeeZat(int recipients) =>
    BigInt.from(5000 * (recipients < 2 ? 2 : recipients));

/// Whether a vault can pay a request, as far as this device knows before building it.
enum FundsCheck {
  /// Spendable funds cover the amount and the minimum fee.
  enough,

  /// Definitely not enough: see `shortByZat`.
  short,

  /// Can't tell (no amount in the link, or no balance known yet).
  unknown,
}

/// Compares what a request needs ([requestedZat] plus [minimumFeeZat]) with a vault's
/// [spendableZat] when this session synced it, else its last known [totalZat] (which
/// can only prove a shortfall: part of it may be held or unconfirmed).
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
