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
  CrashLog(this.file, {DateTime Function()? now}) : _now = now ?? DateTime.now;

  /// The log of this process, set by `main()` (null in tests and before startup).
  static CrashLog? instance;

  final File file;
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

  Future<List<String>> _entries() async {
    try {
      final text = await file.readAsString();
      return text.isEmpty ? <String>[] : text.split(_separator);
    } on FileSystemException {
      return <String>[];
    }
  }

  /// The entries, newest last. Empty when there are none or the file can't be read.
  Future<List<String>> read() async {
    await _tail;
    return _entries();
  }

  Future<void> clear() async {
    await _tail;
    try {
      await file.delete();
    } on FileSystemException {
      // Already gone.
    }
  }

  /// What the user can share: a header with no identifiers, then the entries.
  Future<String> report({required String network}) async {
    final entries = await read();
    final b = StringBuffer()
      ..writeln('Zafe diagnostic report')
      ..writeln('Network: $network')
      ..writeln(
        'Platform: ${Platform.operatingSystem} ${Platform.operatingSystemVersion}',
      )
      ..writeln('Entries: ${entries.length}')
      ..writeln()
      ..writeln(
        entries.isEmpty ? 'No errors recorded.' : entries.join(_separator),
      );
    return scrubDiagnostics(b.toString());
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
