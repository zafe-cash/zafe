import 'package:flutter/widgets.dart';

/// A column whose children (each with a unique key) grow and fade in when they appear
/// and shrink and fade out when they're gone, instead of jumping (250 ms). Children that
/// stay keep their state. With reduced motion, changes are instant.
class AnimatedKeyedColumn extends StatefulWidget {
  const AnimatedKeyedColumn({
    super.key,
    required this.children,
    this.spacing = 0,
  });

  /// Every child needs a key that is unique in this column.
  final List<Widget> children;
  final double spacing;

  static const duration = Duration(milliseconds: 250);

  @override
  State<AnimatedKeyedColumn> createState() => _AnimatedKeyedColumnState();
}

class _AnimatedKeyedColumnState extends State<AnimatedKeyedColumn> {
  /// Current and leaving children in display order.
  late List<_Entry> _entries = [
    for (final c in widget.children) _Entry(c, appearing: false),
  ];

  @override
  void didUpdateWidget(AnimatedKeyedColumn old) {
    super.didUpdateWidget(old);
    final before = {for (final e in _entries) e.child.key!: e};
    final keys = {for (final c in widget.children) c.key!};
    // The new order, then each leaving child put back at its old position.
    final out = [
      for (final c in widget.children)
        _Entry(
          c,
          appearing: !before.containsKey(c.key) || before[c.key]!.leaving,
        ),
    ];
    for (var i = 0; i < _entries.length; i++) {
      final e = _entries[i];
      if (!keys.contains(e.child.key)) {
        out.insert(i.clamp(0, out.length), e..leaving = true);
      }
    }
    _entries = out;
  }

  void _gone(Key key) {
    if (!mounted) return;
    setState(
      () => _entries.removeWhere((e) => e.leaving && e.child.key == key),
    );
  }

  @override
  Widget build(BuildContext context) {
    final still = MediaQuery.disableAnimationsOf(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        for (final e in _entries)
          if (!(still && e.leaving))
            _AnimatedEntry(
              key: ValueKey(e.child.key),
              leaving: e.leaving,
              appearing: e.appearing && !still,
              spacing: widget.spacing,
              onGone: () => _gone(e.child.key!),
              child: e.child,
            ),
      ],
    );
  }
}

class _Entry {
  _Entry(this.child, {required this.appearing});
  final Widget child;
  final bool appearing;
  bool leaving = false;
}

class _AnimatedEntry extends StatefulWidget {
  const _AnimatedEntry({
    super.key,
    required this.child,
    required this.leaving,
    required this.appearing,
    required this.spacing,
    required this.onGone,
  });
  final Widget child;
  final bool leaving;
  final bool appearing;
  final double spacing;
  final VoidCallback onGone;

  @override
  State<_AnimatedEntry> createState() => _AnimatedEntryState();
}

class _AnimatedEntryState extends State<_AnimatedEntry>
    with SingleTickerProviderStateMixin {
  late final AnimationController _c = AnimationController(
    vsync: this,
    duration: AnimatedKeyedColumn.duration,
    value: widget.appearing ? 0 : 1,
  );
  late final Animation<double> _curve = CurvedAnimation(
    parent: _c,
    curve: Curves.easeOutCubic,
    reverseCurve: Curves.easeInCubic,
  );

  @override
  void initState() {
    super.initState();
    if (widget.appearing) _c.forward();
  }

  @override
  void didUpdateWidget(_AnimatedEntry old) {
    super.didUpdateWidget(old);
    if (widget.leaving && !old.leaving) {
      _c.reverse().whenComplete(widget.onGone);
    } else if (!widget.leaving && old.leaving) {
      _c.forward();
    }
  }

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => SizeTransition(
    sizeFactor: _curve,
    alignment: const Alignment(-1.0, -1.0),
    child: FadeTransition(
      opacity: _curve,
      child: Padding(
        padding: EdgeInsets.only(bottom: widget.spacing),
        child: widget.child,
      ),
    ),
  );
}
