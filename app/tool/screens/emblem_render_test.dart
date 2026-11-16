import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/features/vaults/vault_emblem.dart';

void main() {
  testWidgets('render vault emblems', (tester) async {
    final byMotif = <int, List<String>>{};
    for (var i = 0; byMotif.values.fold(0, (n, l) => n + l.length) < 48; i++) {
      final id = i.toRadixString(16).padLeft(32, '0');
      final list = byMotif.putIfAbsent(EmblemLook.of(id).motif, () => []);
      if (list.length < 6) list.add(id);
    }
    final key = GlobalKey();
    await tester.pumpWidget(
      MaterialApp(
        home: RepaintBoundary(
          key: key,
          child: Container(
            color: const Color(0xFF080B0B),
            padding: const EdgeInsets.all(16),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                for (var m = 0; m < VaultEmblem.motifCount; m++)
                  Padding(
                    padding: const EdgeInsets.only(bottom: 12),
                    child: Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        for (final id in byMotif[m] ?? const <String>[])
                          Padding(
                            padding: const EdgeInsets.only(right: 12),
                            child: VaultEmblem(vaultId: id, size: 56),
                          ),
                        const SizedBox(width: 12),
                        for (final id in (byMotif[m] ?? const <String>[]).take(
                          1,
                        ))
                          VaultEmblem(vaultId: id, size: 24),
                      ],
                    ),
                  ),
              ],
            ),
          ),
        ),
      ),
    );
    final boundary =
        key.currentContext!.findRenderObject()! as RenderRepaintBoundary;
    final bytes = await tester.runAsync(() async {
      final image = await boundary.toImage(pixelRatio: 3);
      return (await image.toByteData(
        format: ui.ImageByteFormat.png,
      ))!.buffer.asUint8List();
    });
    Directory('build/screen_preview').createSync(recursive: true);
    File('build/screen_preview/vault_emblems.png').writeAsBytesSync(bytes!);
  });
}
