import '../storage/member_names.dart';

/// A member: "You", the label this phone gave them, the name they gave themselves with
/// their short key ("Bob (97c9ce...123e)": any member can pick any name), or the short
/// key alone.
String memberLabel(String keyHex, {String? me, SignerNames? names}) {
  if (keyHex == me) return 'You';
  final n = names ?? SignerNames.none;
  final name = n.nameOf(keyHex);
  if (name == null) return shortKey(keyHex);
  return n.isClaim(keyHex) ? '$name (${shortKey(keyHex)})' : name;
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
