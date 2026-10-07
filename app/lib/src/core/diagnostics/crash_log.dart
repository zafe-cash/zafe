import 'dart:async';
import 'dart:io';

import 'package:flutter/foundation.dart';

import '../errors/zafe_error_copy.dart' show describeError;
import 'scrub.dart';

/// A small crash and error log kept **on this phone only**. Nothing here is sent anywhere:
/// Settings > "Diagnostic report" shows it and the user decides whether to share it.
///
/// Entries are scrubbed (`scrubDiagnostics`) before they are written, and the file keeps
/// only the latest [maxEntries] entries / [maxBytes] bytes.
class CrashLog {
  CrashLog(
    this.file, {
    this.peers = const [],
    this.panicFile,
    DateTime Function()? now,
  }) : _now = now ?? DateTime.now;

  /// The log of the app (`isBackground: false`) or of the background engine, both in
  /// [dir] (`<support>/diagnostics`): each writes its own file and reads the other's, and
  /// the Rust panic hook's `rust-panics.log` is part of the report.
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
    );
  }

  /// The log of this process, set by `main()` (null in tests and before startup).
  static CrashLog? instance;

  /// Where this isolate appends. Each isolate (the app, the WorkManager/FCM background
  /// engine) writes only its own file, so two isolates never race on one file.
  final File file;

  /// The other isolates' files: read, merged by time and cleared, never written here.
  final List<File> peers;

  /// One line per Rust panic (file:line only), written by the bridge's panic hook.
  final File? panicFile;
  final DateTime Function() _now;

  static const maxEntries = 20;
  static const maxBytes = 64 * 1024;
  static const _maxStackLines = 25;
  static const _separator = '\n---\n';

  Future<void> _tail = Future.value();

  /// Appends an entry. Never throws: a broken log must not turn into a second crash.
  Future<void> record(String source, Object error, [StackTrace? stack]) {
    final entry = _format(source, error, stack);
    // One writer at a time, in order.
    return _tail = _tail.then((_) => _append(entry)).catchError((_) {});
  }

  String _format(String source, Object error, StackTrace? stack) {
    final lines = (stack?.toString() ?? '')
        .split('\n')
        .where((l) => l.trim().isNotEmpty)
        .take(_maxStackLines);
    final body = [
      '${_now().toUtc().toIso8601String()} $source',
      // Typed bridge errors carry free text from deep inside Rust; scrub like the rest.
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

  /// The entries of every isolate, oldest first (each starts with a UTC timestamp).
  /// Empty when there are none or the files can't be read.
  Future<List<String>> read() async {
    await _tail;
    final all = [
      ...await _entries(),
      for (final p in peers) ...await _entries(p),
    ];
    // Stable for equal stamps, so one isolate's order is kept.
    final indexed = all.indexed.toList()
      ..sort((a, b) {
        final c = a.$2.compareTo(b.$2);
        return c != 0 ? c : a.$1.compareTo(b.$1);
      });
    return [for (final e in indexed) e.$2];
  }

  /// The Rust panic notes, scrubbed again here. Empty when there are none.
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

  Future<void> clear() async {
    await _tail;
    for (final f in [file, ...peers, ?panicFile]) {
      try {
        await f.delete();
      } on FileSystemException {
        // Already gone.
      }
    }
  }

  /// What the user can share: a header with no identifiers, then the entries.
  Future<String> report({required String network}) async {
    final entries = await read();
    final panics = await readPanics();
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
    return scrubDiagnostics(b.toString());
  }

  static String _short(String v) {
    final line = v.split('\n').first;
    return line.length > 48 ? line.substring(0, 48) : line;
  }

  /// Routes Flutter and uncaught async errors here, after whoever was installed before.
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
