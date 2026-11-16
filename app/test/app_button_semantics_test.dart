import 'dart:ui' show Tristate;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/theme/app_theme.dart';
import 'package:zafe/src/core/widgets/app_button.dart';

Future<void> _pump(WidgetTester tester, Widget button) => tester.pumpWidget(
  MaterialApp(
    home: AppTheme(
      data: AppThemeData.dark,
      child: Scaffold(body: Center(child: button)),
    ),
  ),
);

List<SemanticsNode> _nodesWithLabel(WidgetTester tester, String label) {
  final out = <SemanticsNode>[];
  void visit(SemanticsNode node) {
    if (node.label.contains(label)) out.add(node);
    node.visitChildren((child) {
      visit(child);
      return true;
    });
  }

  visit(
    tester.binding.renderViews.first.owner!.semanticsOwner!.rootSemanticsNode!,
  );
  return out;
}

void main() {
  testWidgets('a button is one accessible node with its label', (tester) async {
    final handle = tester.ensureSemantics();
    var taps = 0;
    await _pump(
      tester,
      AppButton(
        onPressed: () => taps++,
        leading: const Icon(Icons.add),
        child: const Text('Add another recipient'),
      ),
    );
    final nodes = _nodesWithLabel(tester, 'Add another recipient');
    expect(nodes, hasLength(1), reason: 'the label must not be its own node');
    final data = nodes.single.getSemanticsData();
    expect(data.label, 'Add another recipient');
    expect(data.flagsCollection.isButton, isTrue);
    expect(data.flagsCollection.isEnabled, Tristate.isTrue);
    expect(data.hasAction(SemanticsAction.tap), isTrue);
    expect(data.hasAction(SemanticsAction.focus), isTrue);
    final root = tester
        .binding
        .renderViews
        .first
        .owner!
        .semanticsOwner!
        .rootSemanticsNode!;
    final focusable = <SemanticsNode>[];
    void visit(SemanticsNode node) {
      if (node.getSemanticsData().flagsCollection.isFocused != Tristate.none) {
        focusable.add(node);
      }
      node.visitChildren((child) {
        visit(child);
        return true;
      });
    }

    visit(root);
    expect(focusable, [nodes.single]);

    tester.semantics.tap(find.semantics.byLabel('Add another recipient'));
    expect(taps, 1);
    handle.dispose();
  });

  testWidgets('a disabled button is one node, marked disabled', (tester) async {
    final handle = tester.ensureSemantics();
    await _pump(
      tester,
      const AppButton(onPressed: null, child: Text('Send now')),
    );
    final nodes = _nodesWithLabel(tester, 'Send now');
    expect(nodes, hasLength(1));
    final data = nodes.single.getSemanticsData();
    expect(data.flagsCollection.isButton, isTrue);
    expect(data.flagsCollection.isEnabled, Tristate.isFalse);
    expect(data.hasAction(SemanticsAction.tap), isFalse);
    handle.dispose();
  });
}
