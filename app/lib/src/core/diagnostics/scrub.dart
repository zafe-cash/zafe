/// Removes what must never leave the phone from a line of diagnostic text: addresses,
/// keys and other long tokens, links, amounts, vault ids and app-data paths. The log is
/// only ever shared by the user, but they should not have to read it for secrets.
///
/// Errs on the side of removing too much; a report that says `<hex>` is still useful.
String scrubDiagnostics(String input) {
  var s = input;
  for (final (pattern, replacement) in _rules) {
    s = s.replaceAll(pattern, replacement);
  }
  return s;
}

final _rules = <(RegExp, String)>[
  // Links first: they can carry invites, payment requests and tokens.
  (RegExp(r'[a-zA-Z][a-zA-Z0-9+.-]*://\S+'), '<url>'),
  // Our own bearer formats (invites, backups, recovery codes), and `zcash:` requests.
  (RegExp(r'zafe-[a-z]+-v\d+:\S+'), '<secret>'),
  (RegExp(r'zcash:\S+'), '<payment-request>'),
  // Zcash addresses (unified, sapling, transparent, TEX), mainnet and test.
  (
    RegExp(
      r'\b(?:u|utest|uregtest|zs|ztestsapling|zregtestsapling|tex|textest)1[02-9ac-hj-np-z]{20,}',
    ),
    '<address>',
  ),
  (RegExp(r'\bt[1-3][1-9A-HJ-NP-Za-km-z]{30,}'), '<address>'),
  // Keys, hashes, txids, commitments: any long hex run.
  (RegExp(r'\b[0-9a-fA-F]{32,}\b'), '<hex>'),
  // Vault ids and other UUID-like ids.
  (
    RegExp(
      r'\b[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}\b',
    ),
    '<id>',
  ),
  // Amounts with a unit.
  (
    RegExp(
      r'\d[\d,]*(?:\.\d+)?\s*(?:ZEC|TAZ|zats?|zatoshis?)\b',
      caseSensitive: false,
    ),
    '<amount>',
  ),
  // Per-vault and per-app paths (vault ids, user names, app ids).
  (RegExp(r'/vaults/[^/\s]+'), '/vaults/<id>'),
  (
    RegExp(
      r'/(?:data|home|Users|var)/[^\s:)]*?(?=/(?:files|app_flutter|Library|cache|lib)\b)',
    ),
    '<app-dir>',
  ),
  // Whatever long opaque token is left (base64, base64url, passphrases glued together).
  (RegExp(r'\b[A-Za-z0-9_\-+/=]{40,}\b'), '<token>'),
];
