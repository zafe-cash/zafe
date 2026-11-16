import 'dart:async';
import 'dart:io';

import 'package:flutter/foundation.dart';

import '../errors/zafe_error_copy.dart' show describeError;
import 'scrub.dart';

class CrashLog {
  CrashLog(
    this.file, {
    this.peers = const [],
    this.panicFile,
    this.timingFile,
    DateTime Function()? now,
  }) : _now = now ?? DateTime.now;

  factory CrashLog.forDir(String dir, {required bool isBackground}) {
    final mine = File(
      '$dir/${isBackground ? 'crashes-bg.log' : 'crashes.log'}',
    );
    final other = File(
      '$dir/${isBackground ? 'crashes.log' : 'crashes-bg.log'}',
    );
    return CrashLog(
      mine,
      peers: [other],
      panicFile: File('$dir/rust-panics.log'),
      timingFile: File('$dir/timings.log'),
    );
  }

  static CrashLog? instance;

  final File file;

  final List<File> peers;

  final File? panicFile;

  final File? timingFile;
  final DateTime Function() _now;

  static const maxEntries = 20;
  static const maxBytes = 64 * 1024;
  static const _maxStackLines = 25;
  static const _separator = '\n---\n';

  Future<void> _tail = Future.value();

  Future<void> record(String source, Object error, [StackTrace? stack]) {
    final entry = _format(source, error, stack);
    return _tail = _tail.then((_) => _append(entry)).catchError((_) {});
  }

  String _format(String source, Object error, StackTrace? stack) {
    final lines = (stack?.toString() ?? '')
        .split('\n')
        .where((l) => l.trim().isNotEmpty)
        .take(_maxStackLines);
    final body = [
      '${_now().toUtc().toIso8601String()} $source',
      error.runtimeType.toString(),
      describeError(error),
      ...lines,
    ].join('\n');
    return scrubDiagnostics(body);
  }

  Future<void> _append(String entry) async {
    await file.parent.create(recursive: true);
    final entries = await _entries();
    entries.add(entry);
    var kept = entries.length > maxEntries
        ? entries.sublist(entries.length - maxEntries)
        : entries;
    var text = kept.join(_separator);
    while (text.length > maxBytes && kept.length > 1) {
      kept = kept.sublist(1);
      text = kept.join(_separator);
    }
    if (text.length > maxBytes) text = text.substring(text.length - maxBytes);
    await file.writeAsString(text, flush: true);
  }

  Future<List<String>> _entries([File? from]) async {
    try {
      final text = await (from ?? file).readAsString();
      return text.isEmpty ? <String>[] : text.split(_separator);
    } on FileSystemException {
      return <String>[];
    }
  }

  Future<List<String>> read() async {
    await _tail;
    final all = [
      ...await _entries(),
      for (final p in peers) ...await _entries(p),
    ];
    final indexed = all.indexed.toList()
      ..sort((a, b) {
        final c = a.$2.compareTo(b.$2);
        return c != 0 ? c : a.$1.compareTo(b.$1);
      });
    return [for (final e in indexed) e.$2];
  }

  Future<List<String>> readPanics() async {
    final f = panicFile;
    if (f == null) return <String>[];
    try {
      return [
        for (final l in (await f.readAsString()).split('\n'))
          if (l.trim().isNotEmpty) scrubDiagnostics(l.trim()),
      ];
    } on FileSystemException {
      return <String>[];
    }
  }

  Future<List<String>> readTimings({int last = 40}) async {
    final f = timingFile;
    if (f == null) return <String>[];
    try {
      final lines = [
        for (final l in (await f.readAsString()).split('\n'))
          if (l.trim().isNotEmpty) scrubDiagnostics(l.trim()),
      ];
      return lines.length > last ? lines.sublist(lines.length - last) : lines;
    } on FileSystemException {
      return <String>[];
    }
  }

  Future<void> clear() async {
    await _tail;
    for (final f in [file, ...peers, ?panicFile, ?timingFile]) {
      try {
        await f.delete();
      } on FileSystemException {
        if (await f.exists()) rethrow;
      }
    }
  }

  Future<String> report({required String network}) async {
    final entries = await read();
    final panics = await readPanics();
    final timings = await readTimings();
    final b = StringBuffer()
      ..writeln('Zafe diagnostic report')
      ..writeln('Network: $network')
      ..writeln(
        'Platform: ${Platform.operatingSystem} ${_short(Platform.operatingSystemVersion)}',
      )
      ..writeln('Entries: ${entries.length}')
      ..writeln()
      ..writeln(
        entries.isEmpty ? 'No errors recorded.' : entries.join(_separator),
      );
    if (panics.isNotEmpty) {
      b
        ..writeln()
        ..writeln('Rust panics (location only): ${panics.length}')
        ..writeln(panics.join('\n'));
    }
    if (timings.isNotEmpty) {
      b
        ..writeln()
        ..writeln(
          'Slow steps (the first call of each kind, and anything over 1.5 s):',
        )
        ..writeln(timings.join('\n'));
    }
    return scrubDiagnostics(b.toString());
  }

  static String _short(String v) {
    final line = v.split('\n').first;
    return line.length > 48 ? line.substring(0, 48) : line;
  }

  void install() {
    final previousFlutter = FlutterError.onError;
    FlutterError.onError = (details) {
      unawaited(record('flutter', details.exception, details.stack));
      previousFlutter?.call(details);
    };
    final previousPlatform = PlatformDispatcher.instance.onError;
    PlatformDispatcher.instance.onError = (error, stack) {
      unawaited(record('uncaught', error, stack));
      return previousPlatform?.call(error, stack) ?? false;
    };
  }
}
