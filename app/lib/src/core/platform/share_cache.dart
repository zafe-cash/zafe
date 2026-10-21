import 'dart:io';

/// Deletes what the share sheet left behind: `share_plus` copies each shared file into
/// `<cache>/share_plus` and only clears that folder at the next share, so a backup file
/// would sit there until then. Run at launch, never right after a share (the receiving
/// app may still be reading the file).
Future<void> clearShareCache(Directory cacheDir) async {
  try {
    final folder = Directory('${cacheDir.path}/share_plus');
    if (await folder.exists()) await folder.delete(recursive: true);
  } catch (_) {
    // Best effort.
  }
}
