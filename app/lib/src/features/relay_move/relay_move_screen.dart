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

String switchesWhenText(String vaultName, int needed, int members) =>
    '$vaultName switches when $needed of $members have moved.';

String paymentsResumeText(int missing, String host) =>
    'Payments resume when $missing more ${missing == 1 ? 'member moves' : 'members move'} '
    'to $host.';

String movedToText(String vaultName, String host) =>
    '$vaultName moved to $host. Move to continue.';

String mustUpdateText(List<String> names) {
  final who = names.length <= 2
      ? names.join(' and ')
      : '${names.take(names.length - 1).join(', ')} and ${names.last}';
  return '$who must update Zafe first.';
}

bool relayHasNoPush(String url) => relayPresetFor(url)?.operator != 'Zafe';

bool relaySeesIp(String url, {required bool tor}) =>
    !tor && relayPresetFor(url) == null;

const kRelayMoveTitle = 'Server';

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

class FollowMoveScreen extends ConsumerWidget {
  const FollowMoveScreen({super.key, required this.link});

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

  final String? mover;
  final int? arrived;
  final int? members;

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
