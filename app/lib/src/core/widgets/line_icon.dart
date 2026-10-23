import 'package:flutter/widgets.dart';

/// An icon at the start of a row of text, centred on the text's first line.
///
/// Rows with `CrossAxisAlignment.start` put the icon's top on the text's top: an icon
/// taller than the line (20-22 px next to a 17 px title) then sits a few pixels low, and
/// one shorter than a large-text line sits high. This box is one line of [line] tall
/// (scaled with the user's text size) and centres the icon in it, overflowing evenly
/// when the icon is taller.
class LineIcon extends StatelessWidget {
  const LineIcon({
    super.key,
    required this.icon,
    required this.size,
    required this.line,
  });

  final Widget icon;

  /// The icon's size (its box is this wide).
  final double size;

  /// The style of the text's first line (usually the title).
  final TextStyle line;

  @override
  Widget build(BuildContext context) {
    final fontSize = MediaQuery.textScalerOf(
      context,
    ).scale(line.fontSize ?? 14);
    final lineHeight = fontSize * (line.height ?? 1.2);
    return SizedBox(
      width: size,
      height: lineHeight,
      child: OverflowBox(
        minHeight: 0,
        maxHeight: double.infinity,
        child: SizedBox(width: size, height: size, child: icon),
      ),
    );
  }
}
