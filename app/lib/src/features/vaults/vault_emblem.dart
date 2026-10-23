import 'dart:math' as math;

import 'package:flutter/widgets.dart';

/// A vault's picture: one of [VaultEmblem.motifCount] small illustrations on a dark
/// tile, with the motif and its colours picked from the vault id, so every member's
/// phone shows the same picture for the same vault.
class VaultEmblem extends StatelessWidget {
  const VaultEmblem({required this.vaultId, this.size = 40, super.key});

  /// Vault id (hex).
  final String vaultId;
  final double size;

  static const motifCount = 8;

  @override
  Widget build(BuildContext context) {
    final look = EmblemLook.of(vaultId);
    return Semantics(
      label: 'Vault picture',
      image: true,
      child: Container(
        width: size,
        height: size,
        decoration: BoxDecoration(
          color: look.tile,
          borderRadius: BorderRadius.circular(size * 0.3),
          border: Border.all(color: look.ink.withValues(alpha: 0.35)),
        ),
        clipBehavior: Clip.antiAlias,
        child: CustomPaint(painter: EmblemPainter(look)),
      ),
    );
  }
}

/// Motif and colours for one vault.
@immutable
class EmblemLook {
  const EmblemLook({
    required this.motif,
    required this.tile,
    required this.ink,
    required this.accent,
  });

  /// Deterministic from the id: FNV-1a over its characters picks the motif and palette.
  factory EmblemLook.of(String vaultId) {
    var h = 0x811c9dc5;
    for (final c in vaultId.codeUnits) {
      h = ((h ^ c) * 0x01000193) & 0xffffffff;
    }
    final palette = _palettes[(h >> 8) % _palettes.length];
    return EmblemLook(
      motif: h % VaultEmblem.motifCount,
      tile: palette.$1,
      ink: palette.$2,
      accent: palette.$3,
    );
  }

  final int motif;
  final Color tile;
  final Color ink;
  final Color accent;

  // (tile, ink, accent): dark tiles in both themes (pictures, not surfaces). Verdigris is
  // the brand and gold is keys and funds, with pale verdigris and bronze as companions;
  // no violet, coral or blue (see docs/brand.md). Keep six entries in this order: the
  // vault id's hash picks one, so members agree.
  static const _palettes = <(Color, Color, Color)>[
    (Color(0xFF091312), Color(0xFF51DDD2), Color(0xFFF3BA3C)), // teal on ink
    (
      Color(0xFF1B170C),
      Color(0xFFF3BA3C),
      Color(0xFF51DDD2),
    ), // gold on dark bronze
    (Color(0xFF0C1719), Color(0xFFD9F5F2), Color(0xFF51DDD2)), // pale verdigris
    (
      Color(0xFF004A46),
      Color(0xFFD9F5F2),
      Color(0xFFF3BA3C),
    ), // the app icon's tile
    (Color(0xFF1A140B), Color(0xFFC9923A), Color(0xFFD9F5F2)), // deep bronze
    (
      Color(0xFF0B1716),
      Color(0xFF84E1D9),
      Color(0xFFF3BA3C),
    ), // light verdigris
  ];
}

/// Paints [EmblemLook.motif] into a unit square scaled to the canvas.
class EmblemPainter extends CustomPainter {
  const EmblemPainter(this.look);
  final EmblemLook look;

  @override
  void paint(Canvas canvas, Size size) {
    canvas.save();
    canvas.scale(size.width / 100, size.height / 100);
    final stroke = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = 6
      ..strokeCap = StrokeCap.round
      ..strokeJoin = StrokeJoin.round
      ..color = look.ink;
    final fill = Paint()..color = look.ink;
    final accent = Paint()..color = look.accent;
    switch (look.motif) {
      case 0:
        _dial(canvas, stroke, accent);
      case 1:
        _keyhole(canvas, fill, accent);
      case 2:
        _peaks(canvas, fill, accent);
      case 3:
        _waves(canvas, stroke, accent);
      case 4:
        _coins(canvas, stroke, accent);
      case 5:
        _gem(canvas, stroke, accent);
      case 6:
        _sun(canvas, stroke, accent);
      default:
        _orbit(canvas, stroke, accent);
    }
    canvas.restore();
  }

  /// A safe dial: ring, ticks and a handle.
  void _dial(Canvas c, Paint s, Paint a) {
    const o = Offset(50, 50);
    c.drawCircle(o, 30, s);
    final tick = Paint()
      ..color = s.color
      ..strokeWidth = 4
      ..strokeCap = StrokeCap.round;
    for (var i = 0; i < 12; i++) {
      final d = Offset.fromDirection(i * math.pi / 6);
      c.drawLine(o + d * 20, o + d * (i % 3 == 0 ? 13 : 16), tick);
    }
    c.drawCircle(o, 6, a);
  }

  /// A keyhole under an arch.
  void _keyhole(Canvas c, Paint f, Paint a) {
    c.drawCircle(const Offset(50, 40), 14, f);
    c.drawPath(
      Path()
        ..moveTo(42, 46)
        ..lineTo(58, 46)
        ..lineTo(63, 76)
        ..lineTo(37, 76)
        ..close(),
      f,
    );
    c.drawCircle(const Offset(50, 40), 5, a);
  }

  /// Two peaks and a sun.
  void _peaks(Canvas c, Paint f, Paint a) {
    c.drawCircle(const Offset(68, 32), 9, a);
    c.drawPath(
      Path()
        ..moveTo(14, 78)
        ..lineTo(40, 36)
        ..lineTo(58, 64)
        ..lineTo(66, 52)
        ..lineTo(86, 78)
        ..close(),
      f,
    );
  }

  /// Three waves.
  void _waves(Canvas c, Paint s, Paint a) {
    for (var row = 0; row < 3; row++) {
      final y = 34.0 + row * 16;
      final p = Path()..moveTo(18, y);
      for (var i = 0; i < 4; i++) {
        final x = 18.0 + i * 16;
        p.quadraticBezierTo(x + 4, y - 7, x + 8, y);
        p.quadraticBezierTo(x + 12, y + 7, x + 16, y);
      }
      c.drawPath(p, row == 1 ? (Paint.from(s)..color = a.color) : s);
    }
  }

  /// A stack of coins.
  void _coins(Canvas c, Paint s, Paint a) {
    final coin = Paint.from(s)..strokeWidth = 5;
    for (var i = 0; i < 3; i++) {
      final y = 68.0 - i * 13;
      c.drawOval(
        Rect.fromCenter(center: Offset(50, y), width: 44, height: 16),
        coin,
      );
    }
    c.drawOval(
      Rect.fromCenter(center: const Offset(50, 42), width: 20, height: 6),
      a,
    );
  }

  /// A cut gem.
  void _gem(Canvas c, Paint s, Paint a) {
    final outline = Path()
      ..moveTo(30, 36)
      ..lineTo(70, 36)
      ..lineTo(82, 48)
      ..lineTo(50, 82)
      ..lineTo(18, 48)
      ..close();
    c.drawPath(outline, s);
    final facet = Paint.from(s)..strokeWidth = 4;
    c.drawLine(const Offset(18, 48), const Offset(82, 48), facet);
    c.drawPath(
      Path()
        ..moveTo(40, 48)
        ..lineTo(50, 36)
        ..lineTo(60, 48)
        ..lineTo(50, 82)
        ..close(),
      a,
    );
  }

  /// A sun with rays.
  void _sun(Canvas c, Paint s, Paint a) {
    const o = Offset(50, 50);
    c.drawCircle(o, 13, a);
    final ray = Paint.from(s)..strokeWidth = 5;
    for (var i = 0; i < 8; i++) {
      final d = Offset.fromDirection(i * math.pi / 4);
      c.drawLine(o + d * 21, o + d * 31, ray);
    }
  }

  /// A planet with a ring and a moon.
  void _orbit(Canvas c, Paint s, Paint a) {
    c.save();
    c.translate(50, 50);
    c.rotate(-0.45);
    c.drawOval(
      Rect.fromCenter(center: Offset.zero, width: 74, height: 26),
      Paint.from(s)..strokeWidth = 4,
    );
    c.restore();
    c.drawCircle(const Offset(50, 50), 15, Paint()..color = s.color);
    // The moon at the lower-left end of the ring: the top-right corner is where the
    // "something waits" badge sits, and a moon there read as a second dot.
    c.drawCircle(const Offset(22, 74), 6, a);
  }

  @override
  bool shouldRepaint(covariant EmblemPainter old) =>
      old.look.motif != look.motif ||
      old.look.ink != look.ink ||
      old.look.tile != look.tile;
}
