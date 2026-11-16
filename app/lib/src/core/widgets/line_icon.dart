import 'package:flutter/widgets.dart';

class LineIcon extends StatelessWidget {
  const LineIcon({
    super.key,
    required this.icon,
    required this.size,
    required this.line,
  });

  final Widget icon;

  final double size;

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
