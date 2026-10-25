import 'dart:async';

import 'package:app_links/app_links.dart';
import 'package:flutter/foundation.dart';

import '../features/onboarding/invite_link.dart';
import '../features/send/payment_link.dart';

/// Invites from opened invite links (`zafe://join?invite=...`, or
/// `https://<ZAFE_LINK_HOST>/join#...` through App Links / Universal Links), for the router
/// to show on the Join screen. Holds the raw invite, or the whole link when it doesn't carry
/// a valid one (the Join screen then says it isn't an invite). Links never join by
/// themselves.
final inviteLinks = ValueNotifier<String?>(null);

/// The latest ZIP 321 `zcash:` payment link the app was opened with, for the router to
/// show on the payment request screen once the app is unlocked. One at a time: a newer
/// link replaces one still waiting. Links never pay by themselves.
final paymentLinks = ValueNotifier<PendingPaymentLink?>(null);

/// The latest relay move link (`https://zafe.cash/move#…`, docs/relay-move.md §4.3), for
/// the router to show on the follow screen. Opening it contacts nothing: following takes
/// a tap there.
final moveLinks = ValueNotifier<String?>(null);

/// A move link: `https://zafe.cash/move#…` (or on this build's link host), or
/// `zafe://move#…`.
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

/// Listens for invite and payment links. The stream also delivers the link that
/// launched the app (cold start), so there is no separate initial-link read. Read as
/// strings: a payment link goes to Rust exactly as delivered.
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
