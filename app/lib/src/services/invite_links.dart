import 'dart:async';

import 'package:app_links/app_links.dart';
import 'package:flutter/foundation.dart';

import '../features/onboarding/invite_link.dart';
import '../features/send/payment_link.dart';

final inviteLinks = ValueNotifier<String?>(null);

final paymentLinks = ValueNotifier<PendingPaymentLink?>(null);

final moveLinks = ValueNotifier<String?>(null);

bool isMoveLink(String link) {
  final uri = Uri.tryParse(link.trim());
  if (uri == null || !uri.hasFragment) return false;
  final scheme = uri.scheme.toLowerCase();
  final host = uri.host.toLowerCase();
  if (scheme == 'zafe' || scheme == 'zafe-testnet') return host == 'move';
  return scheme == 'https' &&
      (host == 'zafe.cash' ||
          (kZafeLinkHost.isNotEmpty && host == kZafeLinkHost.toLowerCase())) &&
      (uri.path == '/move' || uri.path == '/move/');
}

StreamSubscription<String>? _subscription;

void initInviteLinks() {
  _subscription ??= AppLinks().stringLinkStream.listen((raw) {
    final link = raw.trim();
    if (isPaymentLink(link)) {
      paymentLinks.value = PendingPaymentLink(link, DateTime.now());
      return;
    }
    if (isMoveLink(link)) {
      moveLinks.value = link;
      return;
    }
    final uri = Uri.tryParse(link);
    if (uri == null || !isInviteLink(uri)) return;
    inviteLinks.value = inviteFromLink(uri) ?? link;
  }, onError: (Object e) => debugPrint('app link: $e'));
}
