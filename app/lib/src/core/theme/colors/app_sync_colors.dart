import 'package:flutter/painting.dart';

import '../primitives.dart';

/// The sync status in the top nav: green when in sync, orange when something failed.
///
/// * [text], [glow] — "Synced" and the status indicator's glow: a muted green (status,
///   not money: gold is the value colour; teal means tappable). Chosen 2026-10-27.
/// * [textError], [glowError] — A failure ("Can't reach server"): the warning orange,
///   text and glow alike, so the whole status changes colour.
/// * [torText], [torGlow] — The same two while "Use Tor" is on: Tor's purple, so the
///   status line shows at a glance that traffic goes through Tor.
class AppSyncColors {
  const AppSyncColors({
    required this.text,
    required this.textError,
    required this.glow,
    required this.glowError,
    required this.torText,
    required this.torGlow,
  });

  final Color text;
  final Color textError;
  final Color glow;
  final Color glowError;
  final Color torText;
  final Color torGlow;

  static const dark = AppSyncColors(
    text: Color(0xFF6BD99A),
    textError: OrangePrimitives.p500Dark,
    glow: Color(0xFF2E8A57),
    glowError: OrangePrimitives.p500Dark,
    torText: Color(0xFFC9A6E4),
    torGlow: Color(0xFF9A5CC9),
  );

  static const light = AppSyncColors(
    text: Color(0xFF1E7A47),
    textError: OrangePrimitives.p400Light,
    glow: Color(0xFF4CB57D),
    glowError: OrangePrimitives.p400Light,
    torText: Color(0xFF6B3A8C),
    torGlow: Color(0xFFA678C8),
  );
}
