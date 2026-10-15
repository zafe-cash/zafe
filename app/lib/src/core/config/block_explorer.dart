import 'package:url_launcher/url_launcher.dart';

import 'network_config.dart';

/// Where a transaction id opens: ZecBlock (as Vizor uses), on the build's network.
/// Regtest has no public explorer.
Uri? explorerTxUri(String txid, {String network = kZafeNetwork}) {
  final host = switch (network) {
    'main' || 'mainnet' => 'zecblock.com',
    'test' || 'testnet' => 'testnet.zecblock.com',
    _ => null,
  };
  return host == null ? null : Uri.https(host, '/tx/$txid');
}

/// Opens [txid] in the explorer. Returns false when there is none (regtest) or no app
/// can open it; the caller then copies the id instead.
Future<bool> openTxInExplorer(String txid) async {
  final uri = explorerTxUri(txid);
  if (uri == null) return false;
  try {
    return await launchUrl(uri, mode: LaunchMode.externalApplication);
  } catch (_) {
    return false;
  }
}
