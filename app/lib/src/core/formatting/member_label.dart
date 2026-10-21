/// A member: "You", the local name this device gave them, or a short key.
/// [withKey] adds the short key after a name ("Bob (97c9ce...123e)") where who did
/// something matters (who proposed a payment): names are chosen by members themselves.
String memberLabel(
  String keyHex, {
  String? me,
  Map<String, String> names = const {},
  bool withKey = false,
}) {
  if (keyHex == me) return 'You';
  final name = names[keyHex];
  if (name != null && name.isNotEmpty) {
    return withKey ? '$name (${shortKey(keyHex)})' : name;
  }
  return shortKey(keyHex);
}

/// "97c9ce...123e".
String shortKey(String keyHex) {
  if (keyHex.length < 14) return keyHex;
  return '${keyHex.substring(0, 6)}...${keyHex.substring(keyHex.length - 4)}';
}

/// Compact address: 7 + " .... " + 7.
String compactAddress(String address) {
  if (address.length <= 18) return address;
  return '${address.substring(0, 7)} .... ${address.substring(address.length - 7)}';
}
