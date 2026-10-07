/// The capped mainnet beta (spec §16, M2): until the audit's findings are fixed and the
/// beta ends, mainnet builds say "beta" and ask members to keep a small amount in each
/// vault. The cap is advisory in the app (money can't be refused on receive) and a hard
/// limit on the relay (it takes a fixed number of vaults).
///
/// `--dart-define=ZAFE_BETA=false` removes the label and the cap on a mainnet build (the
/// end of the beta); `--dart-define=ZAFE_BETA_CAP_ZAT=<zatoshi>` sets the per-vault cap
/// (default 5 ZEC). Test networks are never beta: their coins have no value.
library;

import '../formatting/zec_amount.dart';
import 'network_config.dart';

const bool _betaFlag = bool.fromEnvironment('ZAFE_BETA', defaultValue: true);

/// This build runs as the capped mainnet beta.
const bool kIsBeta = kZafeNetwork == 'main' && _betaFlag;

/// The most a vault should hold during the beta, in zatoshi.
const int kBetaVaultCapZat = int.fromEnvironment(
  'ZAFE_BETA_CAP_ZAT',
  defaultValue: 500000000,
);

/// Whether `totalZat` is over the cap (always false outside the beta).
bool overBetaCap(
  BigInt? totalZat, {
  bool beta = kIsBeta,
  int capZat = kBetaVaultCapZat,
}) => beta && capZat > 0 && totalZat != null && totalZat > BigInt.from(capZat);

/// "5 ZEC" for the cap.
String betaCapText({int capZat = kBetaVaultCapZat}) =>
    '${formatZecAmount(BigInt.from(capZat))} $kZcashDefaultCurrencyTicker';

/// The sentence shown on Home and when receiving.
String betaNote({int capZat = kBetaVaultCapZat}) =>
    'Zafe is in beta on mainnet. Keep no more than ${betaCapText(capZat: capZat)} '
    'in a vault.';

/// The warning when a vault holds more than the cap.
String betaOverCapNote({int capZat = kBetaVaultCapZat}) =>
    'This vault holds more than the beta limit of ${betaCapText(capZat: capZat)}. '
    'Move the excess out, and don\'t add more.';
