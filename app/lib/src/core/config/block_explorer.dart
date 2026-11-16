import 'package:url_launcher/url_launcher.dart';

import 'network_config.dart';

Uri? explorerTxUri(String txid, {String network = kZafeNetwork}) {
  final host = switch (network) {
    'main' || 'mainnet' => 'zecblock.com',
    'test' || 'testnet' => 'testnet.zecblock.com',
    _ => null,
  };
  return host == null ? null : Uri.https(host, '/tx/$txid');
}

Future<bool> openTxInExplorer(String txid) async {
  final uri = explorerTxUri(txid);
  if (uri == null) return false;
  try {
    return await launchUrl(uri, mode: LaunchMode.externalApplication);
  } catch (_) {
    return false;
  }
}
