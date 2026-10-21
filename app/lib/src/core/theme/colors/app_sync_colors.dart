import 'package:flutter/painting.dart';

import '../primitives.dart';

/// The sync status in the top nav.
///
/// * [text] — "Synced" label: the value colour's pale step (Zcash gold).
/// * [textError] — Label when a sync failed.
/// * [glow] — The status indicator's glow.
/// * [torText], [torGlow] — The same two while "Use Tor" is on: Tor's purple, so the
///   status line shows at a glance that traffic goes through Tor.
class AppSyncColors {
  const AppSyncColors({
    required this.text,
    required this.textError,
    required this.glow,
    required this.torText,
    required this.torGlow,
  });

  final Color text;
  final Color textError;
  final Color glow;
  final Color torText;
  final Color torGlow;

  static const dark = AppSyncColors(
    text: GoldPrimitives.p800Dark,
    textError: Primitives.p900Alpha50Dark,
    glow: Primitives.p500Dark,
    torText: Color(0xFFC9A6E4),
    torGlow: Color(0xFF9A5CC9),
  );

  static const light = AppSyncColors(
    text: GoldPrimitives.p500Light,
    textError: Primitives.p900Alpha50Light,
    glow: GoldPrimitives.p300Light,
    torText: Color(0xFF6B3A8C),
    torGlow: Color(0xFFA678C8),
  );
}
