String scrubDiagnostics(String input) {
  var s = input;
  for (final (pattern, replacement) in _rules) {
    s = s.replaceAll(pattern, replacement);
  }
  return s;
}

final _rules = <(RegExp, String)>[
  (RegExp(r'[a-zA-Z][a-zA-Z0-9+.-]*://\S+'), '<url>'),
  (RegExp(r'zafe-[a-z]+-v\d+:\S+'), '<secret>'),
  (RegExp(r'zcash:\S+'), '<payment-request>'),
  (
    RegExp(
      r'\b(?:u|utest|uregtest|zs|ztestsapling|zregtestsapling|tex|textest)1[02-9ac-hj-np-z]{20,}',
    ),
    '<address>',
  ),
  (RegExp(r'\bt[1-3][1-9A-HJ-NP-Za-km-z]{30,}'), '<address>'),
  (RegExp(r'\b[0-9a-fA-F]{32,}\b'), '<hex>'),
  (
    RegExp(
      r'\b[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}\b',
    ),
    '<id>',
  ),
  (
    RegExp(
      r'\d[\d,]*(?:\.\d+)?\s*(?:ZEC|TAZ|zats?|zatoshis?)\b',
      caseSensitive: false,
    ),
    '<amount>',
  ),
  (RegExp(r'/vaults/[^/\s]+'), '/vaults/<id>'),
  (
    RegExp(
      r'/(?:data|home|Users|var)/[^\s:)]*?(?=/(?:files|app_flutter|Library|cache|lib)\b)',
    ),
    '<app-dir>',
  ),
  (RegExp(r'\b[A-Za-z0-9_\-+/=]{40,}\b'), '<token>'),
];
