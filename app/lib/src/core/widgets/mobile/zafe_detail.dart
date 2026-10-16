import 'dart:math' as math;

import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../../config/block_explorer.dart';
import '../../config/network_config.dart';
import '../../formatting/member_label.dart';
import '../../privacy/privacy_mask.dart';
import '../../theme/app_theme.dart';
import '../app_icon.dart';
import '../app_tappable.dart';

// Detail vocabulary shared by the payment review and proposal screens: the payment
// card, detail rows, signer tiles/rows and the approval dots.

/// 40px ZEC coin (fixed Zcash yellow).
class ZecCoin extends StatelessWidget {
  const ZecCoin({super.key});

  @override
  Widget build(BuildContext context) => Container(
    width: 40,
    height: 40,
    decoration: const BoxDecoration(
      color: Color(0xFFF4B728),
      shape: BoxShape.circle,
    ),
    alignment: Alignment.center,
    child: const AppIcon(
      AppIcons.zcashCurrency,
      size: 22,
      color: Color(0xFFFFFFFF),
    ),
  );
}

/// The payment at the top of the review and proposal screens: amount in large type on
/// the vault card (`colors.vaultCard`, dark or light with the theme), and the recipient (shielded address, compact) with a
/// "Full address" action.
class PaymentCard extends StatelessWidget {
  const PaymentCard({
    super.key,
    required this.amountText,
    this.address,
    this.onFullAddress,
    this.label = 'PAYMENT',
    this.recipients = 1,
    this.hidden = false,
    this.strikethrough = false,
  });

  /// Amount without the ticker ("1.5").
  final String amountText;

  /// First recipient's address; null for money the vault received (no recipient box).
  final String? address;
  final VoidCallback? onFullAddress;
  final String label;
  final int recipients;
  final bool hidden;

  /// Rejected or cancelled payments.
  final bool strikethrough;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final card = colors.vaultCard;
    final decoration = strikethrough
        ? TextDecoration.lineThrough
        : TextDecoration.none;
    return Container(
      decoration: BoxDecoration(
        color: card.background,
        borderRadius: BorderRadius.circular(AppRadii.large),
        // Flat with a hairline, like every other card: a drop shadow under the pale
        // light-mode card read as a grey rim.
        border: Border.all(color: card.border, width: 1),
      ),
      clipBehavior: Clip.antiAlias,
      child: Stack(
        children: [
          Positioned.fill(
            child: CustomPaint(
              painter: _PaymentDialPainter(
                color: card.accent,
                opacity: card.dialOpacity,
              ),
            ),
          ),
          Padding(
            padding: const EdgeInsets.fromLTRB(20, 18, 16, 16),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Text(
                      label,
                      style: AppTypography.labelSmall.copyWith(
                        color: card.textSecondary,
                        letterSpacing: 1.6,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                    const SizedBox(width: AppSpacing.xs),
                    AppIcon(
                      AppIcons.shieldKeyhole,
                      size: 14,
                      color: card.accent,
                    ),
                  ],
                ),
                const SizedBox(height: AppSpacing.s),
                // Shrinks to fit rather than cut the ticker on narrow phones with
                // large system text.
                FittedBox(
                  fit: BoxFit.scaleDown,
                  alignment: Alignment.centerLeft,
                  child: Text.rich(
                    TextSpan(
                      children: [
                        TextSpan(
                          text: hidden ? fixedPrivacyMask() : amountText,
                          style: TextStyle(
                            fontFamily: 'Space Grotesk',
                            fontWeight: FontWeight.w500,
                            fontSize: 42,
                            height: 1.05,
                            letterSpacing: -1.6,
                            color: card.text.withValues(
                              alpha: strikethrough ? 0.55 : 1,
                            ),
                            decoration: decoration,
                            decorationColor: card.text.withValues(alpha: 0.55),
                          ),
                        ),
                        TextSpan(
                          text: ' $kZcashDefaultCurrencyTicker',
                          style: TextStyle(
                            fontFamily: 'Space Grotesk',
                            fontWeight: FontWeight.w500,
                            fontSize: 20,
                            color: card.ticker.withValues(
                              alpha: strikethrough ? 0.55 : 1,
                            ),
                          ),
                        ),
                      ],
                    ),
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
                if (address != null) ...[
                  const SizedBox(height: AppSpacing.md),
                  _Recipient(
                    address: address!,
                    recipients: recipients,
                    onFullAddress: onFullAddress ?? () {},
                  ),
                ],
              ],
            ),
          ),
        ],
      ),
    );
  }
}

class _Recipient extends StatelessWidget {
  const _Recipient({
    required this.address,
    required this.recipients,
    required this.onFullAddress,
  });
  final String address;
  final int recipients;
  final VoidCallback onFullAddress;

  @override
  Widget build(BuildContext context) {
    final card = context.colors.vaultCard;
    final info = Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            AppIcon(AppIcons.shieldKeyhole, size: 12, color: card.accent),
            const SizedBox(width: 4),
            Flexible(
              child: Text(
                recipients == 1 ? 'Shielded' : '$recipients recipients',
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: AppTypography.labelSmall.copyWith(
                  color: card.textSecondary,
                ),
              ),
            ),
          ],
        ),
        const SizedBox(height: 4),
        Text(
          compactAddress(address),
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
          style: TextStyle(
            fontFamily: 'JetBrains Mono',
            fontSize: 13,
            height: 1.3,
            color: card.text,
          ),
        ),
      ],
    );
    final button = AppTappable(
      onTap: onFullAddress,
      semanticsLabel: 'Full address',
      child: Container(
        height: 32,
        padding: const EdgeInsets.symmetric(horizontal: 10),
        decoration: BoxDecoration(
          color: card.chip,
          borderRadius: BorderRadius.circular(AppRadii.xSmall),
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            AppIcon(AppIcons.eye, size: 14, color: card.text),
            const SizedBox(width: 6),
            Text(
              'Full address',
              style: AppTypography.labelSmall.copyWith(
                color: card.text,
                fontWeight: FontWeight.w600,
              ),
            ),
          ],
        ),
      ),
    );
    return Container(
      padding: const EdgeInsets.fromLTRB(12, 10, 8, 10),
      decoration: BoxDecoration(
        color: card.inset,
        borderRadius: BorderRadius.circular(AppRadii.small),
        border: Border.all(color: card.insetBorder),
      ),
      // Zafe: at large system text the button goes under the address (side by side
      // they squeezed each other to a few letters).
      child: MediaQuery.textScalerOf(context).scale(10) >= 15
          ? Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                info,
                const SizedBox(height: AppSpacing.xs),
                button,
              ],
            )
          : Row(
              children: [
                Expanded(child: info),
                const SizedBox(width: AppSpacing.xs),
                button,
              ],
            ),
    );
  }
}

/// A quarter of the vault dial (rings and ticks) behind the payment card's top-right.
class _PaymentDialPainter extends CustomPainter {
  const _PaymentDialPainter({required this.color, required this.opacity});
  final Color color;
  final double opacity;

  Color _a(double alpha) => color.withValues(alpha: alpha * opacity);

  @override
  void paint(Canvas canvas, Size size) {
    final center = Offset(size.width + 8, -12);
    final glow = size.width * 0.55;
    canvas.drawCircle(
      center,
      glow,
      Paint()
        ..shader = RadialGradient(
          colors: [_a(0.2), _a(0)],
        ).createShader(Rect.fromCircle(center: center, radius: glow)),
    );
    final ring = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1;
    for (var i = 0; i < 4; i++) {
      ring.color = _a(0.20 - i * 0.04);
      canvas.drawCircle(center, 56.0 + i * 30, ring);
    }
    final tick = Paint()
      ..color = _a(0.33)
      ..strokeWidth = 1.5
      ..strokeCap = StrokeCap.round;
    const outer = 56.0 + 30;
    for (var i = 0; i < 72; i++) {
      final dir = Offset.fromDirection(i * 5 * math.pi / 180);
      final len = i % 6 == 0 ? 8.0 : 4.0;
      canvas.drawLine(center + dir * (outer - len), center + dir * outer, tick);
    }
  }

  @override
  bool shouldRepaint(covariant _PaymentDialPainter oldDelegate) =>
      oldDelegate.color != color || oldDelegate.opacity != opacity;
}

class DetailRow extends StatelessWidget {
  const DetailRow({
    super.key,
    required this.label,
    required this.value,
    this.trailing,
    this.onHelp,
  });
  final String label;
  final String value;
  final Widget? trailing;

  /// Adds a help icon after the label that calls this (e.g. to open an explanation).
  final VoidCallback? onHelp;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final labelText = Text(
      label,
      style: AppTypography.labelLarge.copyWith(color: colors.text.secondary),
    );
    final help = onHelp == null
        ? null
        : AppTappable(
            onTap: onHelp,
            semanticsLabel: 'What is "$label"?',
            child: Padding(
              padding: const EdgeInsets.all(AppSpacing.xxs),
              child: AppIcon(AppIcons.help, size: 16, color: colors.icon.muted),
            ),
          );
    // Zafe: at large system text (1.5x and up) label and value don't fit side by side:
    // the value goes under the label.
    if (MediaQuery.textScalerOf(context).scale(10) >= 15) {
      return Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Flexible(child: labelText),
              ?help,
            ],
          ),
          Row(
            children: [
              Expanded(
                child: Text(
                  value,
                  style: AppTypography.labelLarge.copyWith(
                    color: colors.text.accent,
                  ),
                ),
              ),
              if (trailing != null) ...[
                const SizedBox(width: AppSpacing.xs),
                trailing!,
              ],
            ],
          ),
        ],
      );
    }
    return ConstrainedBox(
      constraints: const BoxConstraints(minHeight: 32),
      // Zafe: label (up to 2/5) and value (up to 3/5) each wrap only when they don't
      // fit, label left and value right, so a short value never breaks mid-word next
      // to a long label on a narrow phone.
      child: Row(
        mainAxisAlignment: MainAxisAlignment.spaceBetween,
        children: [
          Flexible(
            flex: 2,
            child: Text(
              label,
              style: AppTypography.labelLarge.copyWith(
                color: colors.text.secondary,
              ),
            ),
          ),
          if (onHelp != null)
            AppTappable(
              onTap: onHelp,
              semanticsLabel: 'What is "$label"?',
              child: Padding(
                padding: const EdgeInsets.all(AppSpacing.xxs),
                child: AppIcon(
                  AppIcons.help,
                  size: 16,
                  color: colors.icon.muted,
                ),
              ),
            ),
          const SizedBox(width: AppSpacing.sm),
          // The value and its icon sit together at the right edge (spread apart by
          // spaceBetween, a value with an icon floated mid-row).
          Flexible(
            flex: 3,
            child: Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                Flexible(
                  child: Text(
                    value,
                    textAlign: TextAlign.end,
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                    style: AppTypography.labelLarge.copyWith(
                      color: colors.text.accent,
                    ),
                  ),
                ),
                if (trailing != null) ...[
                  const SizedBox(width: AppSpacing.xs),
                  trailing!,
                ],
              ],
            ),
          ),
        ],
      ),
    );
  }
}

/// A transaction id: tap opens it in the block explorer (ZecBlock), long-press copies
/// it. Where there is no explorer (regtest) or nothing can open it, a tap copies.
class TxIdRow extends StatefulWidget {
  const TxIdRow({super.key, required this.txid});
  final String txid;

  @override
  State<TxIdRow> createState() => _TxIdRowState();
}

/// Tap opens the explorer (copies where there is none); long press copies. A copy
/// turns the icon into a check for a moment, no toast.
class _TxIdRowState extends State<TxIdRow> {
  bool _copied = false;

  Future<void> _copy() async {
    await Clipboard.setData(ClipboardData(text: widget.txid));
    unawaited(HapticFeedback.lightImpact());
    if (!mounted) return;
    setState(() => _copied = true);
    await Future<void>.delayed(const Duration(milliseconds: 1400));
    if (mounted) setState(() => _copied = false);
  }

  @override
  Widget build(BuildContext context) {
    final txid = widget.txid;
    final explorer = explorerTxUri(txid) != null;
    return GestureDetector(
      onLongPress: _copy,
      child: AppTappable(
        onTap: () async {
          if (!await openTxInExplorer(txid) && mounted) await _copy();
        },
        semanticsLabel: explorer
            ? 'Open transaction in block explorer'
            : 'Copy transaction ID',
        child: DetailRow(
          label: 'Tx ID',
          value: txid.length > 16
              ? '${txid.substring(0, 8)}...${txid.substring(txid.length - 8)}'
              : txid,
          trailing: AnimatedSwitcher(
            duration: const Duration(milliseconds: 160),
            transitionBuilder: (child, a) =>
                ScaleTransition(scale: a, child: child),
            child: _copied
                ? AppIcon(
                    AppIcons.check,
                    key: const ValueKey('copied'),
                    size: 16,
                    color: context.colors.icon.value,
                  )
                : AppIcon(
                    explorer ? AppIcons.arrowUpRight : AppIcons.copy,
                    key: const ValueKey('icon'),
                    size: 16,
                    color: context.colors.icon.muted,
                  ),
          ),
        ),
      ),
    );
  }
}

class DetailDivider extends StatelessWidget {
  const DetailDivider({super.key});

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.symmetric(vertical: AppSpacing.sm),
    child: Container(height: 1, color: context.colors.border.regular),
  );
}

/// A member's tile: a rounded square with a mirrored 5 x 5 pattern and a hue both
/// derived from the member key, so the same signer looks the same on every device.
/// [me] adds a brand outline.
class SignerTile extends StatelessWidget {
  const SignerTile({
    super.key,
    required this.keyHex,
    this.me = false,
    this.size = 32,
  });
  final String keyHex;
  final bool me;
  final double size;

  @override
  Widget build(BuildContext context) {
    final dark = context.appTheme == AppThemeData.dark;
    final seed = _SignerSeed.of(keyHex);
    final background = HSLColor.fromAHSL(
      1,
      seed.hue,
      dark ? 0.16 : 0.22,
      dark ? 0.17 : 0.91,
    ).toColor();
    final foreground = HSLColor.fromAHSL(
      1,
      seed.hue,
      dark ? 0.28 : 0.26,
      dark ? 0.52 : 0.48,
    ).toColor();
    return Container(
      width: size,
      height: size,
      decoration: BoxDecoration(
        color: background,
        borderRadius: BorderRadius.circular(size * 0.28),
        border: me
            ? Border.all(color: context.colors.border.brandStrong, width: 1.5)
            : null,
      ),
      child: CustomPaint(
        painter: _SignerPatternPainter(seed.cells, foreground),
      ),
    );
  }
}

class _SignerSeed {
  const _SignerSeed(this.hue, this.cells);
  final double hue;

  /// 15 bits: 5 rows x 3 columns (mirrored to 5 columns).
  final int cells;

  static _SignerSeed of(String keyHex) {
    final bytes = <int>[];
    for (var i = 0; i + 1 < keyHex.length && bytes.length < 4; i += 2) {
      final b = int.tryParse(keyHex.substring(i, i + 2), radix: 16);
      if (b == null) break;
      bytes.add(b);
    }
    if (bytes.length < 4) {
      // Not a hex key: fall back to a stable string hash.
      var h = 0x811C9DC5;
      for (final c in keyHex.codeUnits) {
        h = ((h ^ c) * 0x01000193) & 0xFFFFFFFF;
      }
      bytes
        ..clear()
        ..addAll([h >> 24, (h >> 16) & 0xFF, (h >> 8) & 0xFF, h & 0xFF]);
    }
    final hue = (((bytes[0] << 8) | bytes[1]) % 360).toDouble();
    var cells = ((bytes[2] << 8) | bytes[3]) & 0x7FFF;
    // Keep the tile from looking empty or solid.
    final filled = _popCount(cells);
    if (filled < 5) cells |= 0x4A52;
    if (filled > 11) cells &= 0x5AD5;
    return _SignerSeed(hue, cells);
  }

  static int _popCount(int v) {
    var n = 0;
    for (; v != 0; v >>= 1) {
      n += v & 1;
    }
    return n;
  }
}

class _SignerPatternPainter extends CustomPainter {
  const _SignerPatternPainter(this.cells, this.color);
  final int cells;
  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    final inset = size.width * 0.2;
    final cell = (size.width - 2 * inset) / 5;
    final paint = Paint()..color = color;
    final r = Radius.circular(cell * 0.3);
    for (var row = 0; row < 5; row++) {
      for (var col = 0; col < 3; col++) {
        if (cells & (1 << (row * 3 + col)) == 0) continue;
        for (final c in {col, 4 - col}) {
          canvas.drawRRect(
            RRect.fromRectAndRadius(
              Rect.fromLTWH(
                inset + c * cell + cell * 0.08,
                inset + row * cell + cell * 0.08,
                cell * 0.84,
                cell * 0.84,
              ),
              r,
            ),
            paint,
          );
        }
      }
    }
  }

  @override
  bool shouldRepaint(_SignerPatternPainter old) =>
      old.cells != cells || old.color != color;
}

/// A signer: tile, name ("You", the local name, or the short key; with a name the short
/// key shows below) and an optional trailing tag. Tappable when [onTap] is set.
class SignerRow extends StatelessWidget {
  const SignerRow({
    super.key,
    required this.keyHex,
    required this.me,
    this.name,
    this.trailing,
    this.onTap,
  });
  final String keyHex;

  /// This device's key (for "You"), if known.
  final String? me;

  /// The name to show: this device's label for the signer, else the name they shared.
  final String? name;
  final Widget? trailing;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final isMe = keyHex == me;
    final named = isMe || (name?.isNotEmpty ?? false);
    final row = Padding(
      padding: const EdgeInsets.symmetric(vertical: 6),
      child: Row(
        children: [
          SignerTile(keyHex: keyHex, me: isMe),
          const SizedBox(width: AppSpacing.s),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  isMe && (name?.isNotEmpty ?? false)
                      ? 'You · $name'
                      : memberLabel(keyHex, me: me, names: {keyHex: ?name}),
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: named
                      ? AppTypography.labelLarge.copyWith(
                          color: colors.text.primary,
                          fontWeight: FontWeight.w600,
                        )
                      : AppTypography.labelLarge.copyWith(
                          fontFamily: 'JetBrains Mono',
                          color: colors.text.primary,
                        ),
                ),
                if (named)
                  Text(
                    shortKey(keyHex),
                    style: AppTypography.labelSmall.copyWith(
                      fontFamily: 'JetBrains Mono',
                      color: colors.text.muted,
                    ),
                  ),
              ],
            ),
          ),
          if (trailing != null) ...[
            const SizedBox(width: AppSpacing.xs),
            trailing!,
          ],
        ],
      ),
    );
    if (onTap == null) return row;
    return AppTappable(
      onTap: onTap,
      semanticsLabel: (name?.isNotEmpty ?? false)
          ? 'Rename $name'
          : 'Name ${memberLabel(keyHex, me: me)}',
      child: row,
    );
  }
}

enum SignerMark { approved, rejected, waiting }

/// One dot per signer, approvals first (brand), then open seats (outline), then
/// rejections (muted, crossed). A short bar after the [threshold]-th dot marks how
/// many approvals the payment needs.
class ApprovalDots extends StatelessWidget {
  const ApprovalDots({
    super.key,
    required this.approved,
    required this.rejected,
    required this.members,
    required this.threshold,
  });
  final int approved;
  final int rejected;
  final int members;
  final int threshold;

  List<SignerMark> get marks {
    final waiting = math.max(0, members - approved - rejected);
    return [
      for (var i = 0; i < approved; i++) SignerMark.approved,
      for (var i = 0; i < waiting; i++) SignerMark.waiting,
      for (var i = 0; i < rejected; i++) SignerMark.rejected,
    ];
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final all = marks;
    return Semantics(
      label: '$approved of $threshold approvals',
      child: Wrap(
        spacing: 6,
        runSpacing: 6,
        crossAxisAlignment: WrapCrossAlignment.center,
        children: [
          for (var i = 0; i < all.length; i++) ...[
            _Dot(mark: all[i]),
            if (i == threshold - 1 && i != all.length - 1)
              Container(
                width: 1.5,
                height: 12,
                margin: const EdgeInsets.symmetric(horizontal: 2),
                decoration: BoxDecoration(
                  color: colors.border.medium,
                  borderRadius: BorderRadius.circular(1),
                ),
              ),
          ],
        ],
      ),
    );
  }
}

class _Dot extends StatelessWidget {
  const _Dot({required this.mark});
  final SignerMark mark;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    const size = 10.0;
    return switch (mark) {
      SignerMark.approved => Container(
        width: size,
        height: size,
        decoration: BoxDecoration(
          shape: BoxShape.circle,
          color: colors.icon.brand,
        ),
      ),
      SignerMark.waiting => Container(
        width: size,
        height: size,
        decoration: BoxDecoration(
          shape: BoxShape.circle,
          border: Border.all(color: colors.border.strong, width: 1.5),
        ),
      ),
      SignerMark.rejected => Container(
        width: size,
        height: size,
        decoration: BoxDecoration(
          shape: BoxShape.circle,
          color: colors.background.destructiveAlpha,
          border: Border.all(color: colors.border.destructive),
        ),
        alignment: Alignment.center,
        child: AppIcon(
          AppIcons.cross,
          size: 7,
          color: colors.icon.destructive,
          patina: colors.icon.destructive,
        ),
      ),
    };
  }
}

/// Quiet vote status at the end of signer rows (icon + text).
class SignerTag extends StatelessWidget {
  const SignerTag({
    super.key,
    required this.label,
    required this.color,
    this.background,
    this.icon,
    this.iconColor,
  });
  final String label;
  final Color color;

  /// Only for states that need attention; otherwise the tag is plain text.
  final Color? background;
  final String? icon;
  final Color? iconColor;

  @override
  Widget build(BuildContext context) {
    final row = Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        if (icon != null) ...[
          AppIcon(icon!, size: 13, color: iconColor ?? color),
          const SizedBox(width: 4),
        ],
        Text(label, style: AppTypography.labelMedium.copyWith(color: color)),
      ],
    );
    if (background == null) return row;
    return Container(
      height: 24,
      padding: const EdgeInsets.symmetric(horizontal: 8),
      decoration: BoxDecoration(
        color: background,
        borderRadius: BorderRadius.circular(AppRadii.xSmall),
      ),
      child: row,
    );
  }
}
