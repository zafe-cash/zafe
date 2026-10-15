import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../platform/sensitive_clipboard.dart';
import 'app_toast.dart';

/// Copies [text] and confirms with a toast. [sensitive] for secrets: see [copySecret].
void copyTextWithToast(
  BuildContext context, {
  required String text,
  required String toastMessage,
  bool sensitive = false,
}) {
  unawaited(
    _copyTextWithToast(
      context,
      text: text,
      toastMessage: toastMessage,
      sensitive: sensitive,
    ),
  );
}

Future<void> _copyTextWithToast(
  BuildContext context, {
  required String text,
  required String toastMessage,
  required bool sensitive,
}) async {
  try {
    if (sensitive) {
      await copySecret(text);
    } else {
      await Clipboard.setData(ClipboardData(text: text));
    }
  } catch (_) {
    return;
  }
  if (!context.mounted) return;
  showAppToast(context, toastMessage);
}
