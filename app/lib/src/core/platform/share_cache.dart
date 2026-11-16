import 'dart:io';

Future<void> clearShareCache(Directory cacheDir) async {
  try {
    final folder = Directory('${cacheDir.path}/share_plus');
    if (await folder.exists()) await folder.delete(recursive: true);
  } catch (_) {}
}
