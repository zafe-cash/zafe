/// The mainnet beta label (spec §16, M2): until the audit's findings are fixed, mainnet
/// builds say "beta" so members know the app is young. There is no limit on vaults or on
/// what a vault holds.
///
/// `--dart-define=ZAFE_BETA=false` removes the label on a mainnet build (the end of the
/// beta). Test networks are never beta: their coins have no value.
library;

import 'network_config.dart';

const bool _betaFlag = bool.fromEnvironment('ZAFE_BETA', defaultValue: true);

/// This build runs as the mainnet beta.
const bool kIsBeta = kZafeNetwork == 'main' && _betaFlag;

/// The sentence shown on Home and when receiving.
const String betaNote =
    'Zafe is in beta on mainnet. Its security audit is not finished yet.';
