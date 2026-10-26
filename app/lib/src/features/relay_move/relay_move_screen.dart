import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../core/config/relay_presets.dart';
import '../../core/formatting/member_label.dart';
import '../../core/layout/mobile/zafe_screen.dart';
import '../../core/security/unlock_gate.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/action_button.dart';
import '../../core/widgets/app_button.dart';
import '../../providers/endpoints_provider.dart';
import '../../providers/member_names_provider.dart';
import '../../providers/relay_move_provider.dart';
import '../../providers/tor_provider.dart';
import '../../providers/vault_names_provider.dart';
import '../../providers/vault_provider.dart';
import '../../rust/api/relay_move.dart' as rust;
import '../../core/errors/sync_failure.dart';
import '../home/home_screen.dart' show NoticeCard;
import '../home/sync_status_screen.dart' show currentSyncFailure;

/// "Grants switches when 3 of 4 have moved." (the move's m, which can be more than a
/// payment's t: the move screens never show it as a payment threshold).
String switchesWhenText(String vaultName, int needed, int members) =>
    '$vaultName switches when $needed of $members have moved.';

/// "Payments resume when 1 more member moves to relay.dao.org." (old relay dead, fewer
/// than m moved: nobody can pay until enough members move).
String paymentsResumeText(int missing, String host) =>
    'Payments resume when $missing more ${missing == 1 ? 'member moves' : 'members move'} '
    'to $host.';

/// "Grants moved to relay.dao.org. Move to continue."
String movedToText(String vaultName, String host) =>
    '$vaultName moved to $host. Move to continue.';

/// "Bob must update Zafe first." / "Bob and Carol must update Zafe first."
String mustUpdateText(List<String> names) {
  final who = names.length <= 2
      ? names.join(' and ')
      : '${names.take(names.length - 1).join(', ')} and ${names.last}';
  return '$who must update Zafe first.';
}

/// The server sends no notifications: only Zafe's own servers can (docs/relay-move.md §10).
bool relayHasNoPush(String url) => relayPresetFor(url)?.operator != 'Zafe';

/// A server that isn't on the built-in list learns this phone's IP address unless Tor is on.
bool relaySeesIp(String url, {required bool tor}) =>
    !tor && relayPresetFor(url) == null;

/// The move pages' title: short, so it fits one line; the sentence is the heading.
const kRelayMoveTitle = 'Server';

/// A move page's heading ("Move Grants to relay.dao.org?"), wrapping as needed.
class MoveHeading extends StatelessWidget {
  const MoveHeading(this.text, {super.key});
  final String text;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.only(bottom: AppSpacing.md),
    child: Text(
      text,
      style: AppTypography.headlineSmall.copyWith(
        color: context.colors.text.accent,
      ),
    ),
  );
}

/// After the switch, for the mover.
class MovedBody extends StatelessWidget {
  const MovedBody({super.key, required this.vaultName, required this.host});
  final String vaultName;
  final String host;

  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      MoveHeading('$vaultName moved to $host'),
      Text(
        'Members who haven\'t moved yet see it in Zafe.',
        style: AppTypography.bodyMedium.copyWith(
          color: context.colors.text.secondary,
        ),
      ),
    ],
  );
}

/// Following a move from its link. Nothing connects to the new server until Move.
class FollowMoveScreen extends ConsumerWidget {
  const FollowMoveScreen({super.key, required this.link});

  /// The move link (`https://zafe.cash/move#…`).
  final String link;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final vault = ref.watch(vaultProvider);
    final names = ref.watch(signerNamesProvider);
    final view = ref.watch(relayMoveProvider);
    final name =
        ref.watch(activeVaultNameProvider) ?? vault.summary?.name ?? '';
    final tor = ref.watch(torProvider).enabled;
    ref.watch(endpointsProvider);
    rust.MoveLinkInfo? info;
    try {
      info = rust.parseMoveLink(text: link);
    } catch (_) {}
    if (info == null || info.vaultId != vault.activeId) {
      return ZafeScreen(
        title: kRelayMoveTitle,
        children: [
          Text(
            info == null
                ? 'This isn\'t a move link.'
                : 'This link is for a vault that isn\'t open on this phone.',
            style: AppTypography.bodyMedium.copyWith(
              color: context.colors.text.secondary,
            ),
          ),
        ],
      );
    }
    final parsed = info;
    final notice = view.notice?.link == parsed.link ? view.notice : null;
    final moved =
        (notice?.moved ?? false) ||
        (view.status?.movedElsewhere ?? false) &&
            view.status?.link == parsed.link;
    final mover = notice == null
        ? null
        : memberLabel(notice.moverHex, me: vault.myKeyHex, names: names);
    Future<void> follow() async {
      if (!await confirmUnlock(
        context,
        ref,
        reason: 'Move $name to ${parsed.host}',
      )) {
        throw const ActionCancelled();
      }
      await ref.read(relayMoveProvider.notifier).follow(parsed.link);
    }

    return ZafeScreen(
      title: kRelayMoveTitle,
      bottom: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          ActionButton(
            label: 'Move',
            doneLabel: 'Moved',
            errorFallback: 'Couldn\'t move. Try again.',
            onPressed: follow,
            onDone: () {
              if (context.mounted) context.go('/home');
            },
          ),
          const SizedBox(height: AppSpacing.xs),
          AppButton(
            expand: true,
            variant: AppButtonVariant.ghost,
            onPressed: () {
              ref.read(relayMoveProvider.notifier).notNow(parsed.link);
              context.pop();
            },
            child: const Text('Not now'),
          ),
        ],
      ),
      children: [
        FollowMoveBody(
          vaultName: name,
          host: parsed.host,
          mover: mover,
          arrived: notice?.arrived,
          members: notice?.members,
          moved: moved,
          noPush: relayHasNoPush(parsed.relayUrl),
          seesIp: relaySeesIp(parsed.relayUrl, tor: tor),
        ),
      ],
    );
  }
}

/// The follow screen's text from plain data (rendered without Rust in previews).
class FollowMoveBody extends StatelessWidget {
  const FollowMoveBody({
    super.key,
    required this.vaultName,
    required this.host,
    this.mover,
    this.arrived,
    this.members,
    this.moved = false,
    this.noPush = false,
    this.seesIp = false,
  });

  final String vaultName;
  final String host;

  /// Who started it, from the move notice; null for a link alone ("From a link").
  final String? mover;
  final int? arrived;
  final int? members;

  /// The vault already switched there.
  final bool moved;
  final bool noPush;
  final bool seesIp;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final secondary = AppTypography.bodyMedium.copyWith(
      color: colors.text.secondary,
    );
    final heading = moved
        ? '$vaultName moved to $host'
        : 'Move $vaultName to $host?';
    final lines = <String>[
      if (moved)
        'Move to continue.'
      else if (mover != null && arrived != null && members != null)
        '$mover is moving it. $arrived of $members have moved.'
      else
        'From a link',
      'Your funds don\'t move.',
    ];
    final facts = <String>[
      if (noPush) 'No notifications on this server',
      if (seesIp) '$host will see your IP address. Tor hides it.',
    ];
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        MoveHeading(heading),
        for (final l in lines) ...[
          Text(
            l,
            style: AppTypography.bodyLarge.copyWith(color: colors.text.accent),
          ),
          const SizedBox(height: AppSpacing.xs),
        ],
        if (facts.isNotEmpty) const SizedBox(height: AppSpacing.sm),
        for (final f in facts) ...[
          Text(f, style: secondary),
          const SizedBox(height: AppSpacing.xs),
        ],
      ],
    );
  }
}

/// Home, under Needs you: this phone's own move while it runs ("Moving Grants to
/// relay.dao.org · 1 of 3 moved", opens the Server screen), or two moves found. A move
/// another member started is a Needs you card ([NeedsYou]).
class RelayMoveCard extends ConsumerWidget {
  const RelayMoveCard({super.key, this.gap = AppSpacing.md});
  final double gap;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final view = ref.watch(relayMoveProvider);
    final vault = ref.watch(vaultProvider);
    final name =
        ref.watch(activeVaultNameProvider) ?? vault.summary?.name ?? '';
    if (view.twoMoves) {
      return Padding(
        padding: EdgeInsets.only(bottom: gap),
        child: NoticeCard(
          title: 'Two moves found',
          body: 'Ask your group which server to use.',
          onTap: () => context.push('/vault-server'),
        ),
      );
    }
    final status = view.status;
    if (status == null || !view.moving) return const SizedBox.shrink();
    return Padding(
      padding: EdgeInsets.only(bottom: gap),
      child: NoticeCard(
        title: 'Moving $name to ${status.host}',
        body: '${status.arrived.length} of ${status.members} moved',
        warning: false,
        onTap: () => context.push('/vault-server'),
      ),
    );
  }
}

/// The payment screen's line while the vault's server is down and a move hasn't reached m
/// yet: "Payments resume when 1 more member moves to relay.dao.org."
class PaymentsResumeLine extends ConsumerWidget {
  const PaymentsResumeLine({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final view = ref.watch(relayMoveProvider);
    final status = view.status;
    final down =
        currentSyncFailure(ref)?.kind == SyncFailureKind.relayUnreachable;
    if (!down || status == null || !view.moving) return const SizedBox.shrink();
    final missing = status.needed - status.arrived.length;
    if (missing <= 0) return const SizedBox.shrink();
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 8, 16, 0),
      child: Text(
        paymentsResumeText(missing, status.host),
        style: AppTypography.bodyMedium.copyWith(
          color: context.colors.text.warning,
        ),
      ),
    );
  }
}
