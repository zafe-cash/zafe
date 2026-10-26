import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:share_plus/share_plus.dart';

import '../../core/config/relay_presets.dart';
import '../../core/formatting/member_label.dart';
import '../../core/layout/mobile/zafe_screen.dart';
import '../../core/security/unlock_gate.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/action_button.dart';
import '../../core/widgets/app_button.dart';
import '../../core/widgets/app_icon.dart';
import '../../core/widgets/mobile/mobile_list_row.dart';
import '../../core/widgets/mobile/mobile_surface_card.dart';
import '../../providers/endpoints_provider.dart';
import '../../providers/member_names_provider.dart';
import '../../providers/relay_move_provider.dart';
import '../../providers/tor_provider.dart';
import '../../providers/vault_names_provider.dart';
import '../../providers/vault_provider.dart';
import '../../providers/vault_relay.dart';
import '../../rust/api/relay_move.dart' as rust;
import '../../core/errors/sync_failure.dart';
import '../home/home_screen.dart' show NoticeCard;
import '../home/sync_status_sheet.dart' show currentSyncFailure;
import '../settings/relay_picker.dart';

/// "Payments switch when 3 of 4 have moved."
String paymentsSwitchText(int needed, int members) =>
    'Payments switch when $needed of $members have moved.';

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

/// The relay sends no notifications: only Zafe's own relays can (docs/relay-move.md §10).
bool relayHasNoPush(String url) => relayPresetFor(url)?.operator != 'Zafe';

/// A relay that isn't on the built-in list learns this phone's IP address unless Tor is on.
bool relaySeesIp(String url, {required bool tor}) =>
    !tor && relayPresetFor(url) == null;

/// The relay move pages' title: short, so it fits one line; the sentence is the heading.
const kRelayMoveTitle = 'Relay';

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

/// Settings > Relay > Move to another relay: pick the relay, Move (unlock), share the
/// link; then who has moved, until the vault switches.
class RelayMoveScreen extends ConsumerStatefulWidget {
  const RelayMoveScreen({super.key});

  @override
  ConsumerState<RelayMoveScreen> createState() => _RelayMoveScreenState();
}

class _RelayMoveScreenState extends ConsumerState<RelayMoveScreen> {
  RelayChoice? _choice;
  List<String>? _blockers;

  /// This visit showed the move in progress: its switch is then shown once, with Done.
  /// Opened later, the screen starts the next move.
  bool _sawMoving = false;

  @override
  void initState() {
    super.initState();
    ref
        .read(relayMoveProvider.notifier)
        .blockers()
        .then(
          (b) {
            if (mounted) setState(() => _blockers = b);
          },
          onError: (_) {
            if (mounted) setState(() => _blockers = const []);
          },
        );
    ref.read(relayMoveProvider.notifier).refresh();
  }

  String get _vaultName =>
      ref.read(activeVaultNameProvider) ??
      ref.read(vaultProvider).summary?.name ??
      'the vault';

  Future<void> _pick() async {
    final current = ref.read(activeVaultRelayProvider)?.url ?? '';
    final choice = await showRelayPicker(
      context,
      title: 'Move to',
      selectedUrl: _choice?.url ?? current,
      currentUrl: current,
      token: _choice?.token,
      confirmLabel: 'Use this relay',
    );
    if (choice != null && mounted) setState(() => _choice = choice);
  }

  Future<void> _share(String host, String link) => SharePlus.instance.share(
    ShareParams(
      text: rust.moveShareText(vaultName: _vaultName, host: host, link: link),
    ),
  );

  Future<void> _move() async {
    final choice = _choice!;
    if (!await confirmUnlock(
      context,
      ref,
      reason: 'Move $_vaultName to ${relayHost(choice.url)}',
    )) {
      throw const ActionCancelled();
    }
    final started = await ref
        .read(relayMoveProvider.notifier)
        .start(choice.url, token: choice.token);
    if (mounted) await _share(started.host, started.link);
  }

  Future<void> _cancel() async {
    await ref.read(relayMoveProvider.notifier).cancel();
  }

  @override
  Widget build(BuildContext context) {
    final view = ref.watch(relayMoveProvider);
    final vault = ref.watch(vaultProvider);
    final names = ref.watch(signerNamesProvider);
    final me = vault.myKeyHex;
    final name =
        ref.watch(activeVaultNameProvider) ?? vault.summary?.name ?? '';
    final status = view.status;
    if (status != null && view.moving) {
      _sawMoving = true;
      final members = vault.summary?.members ?? const <String>[];
      return ZafeScreen(
        title: kRelayMoveTitle,
        children: [
          MoveHeading('Moving to ${status.host}'),
          MoveProgressBody(
            members: [
              for (final m in [?me, ...members.where((m) => m != me)])
                (
                  memberLabel(m, me: me, names: names),
                  status.arrived.contains(m),
                ),
            ],
            needed: status.needed,
            total: status.members,
            onShare: () => _share(status.host, status.link),
            onCancel: _cancel,
          ),
        ],
      );
    }
    if (status != null && status.switched && status.mine && _sawMoving) {
      return ZafeScreen(
        title: kRelayMoveTitle,
        bottom: AppButton(
          expand: true,
          onPressed: () => context.pop(),
          child: const Text('Done'),
        ),
        children: [MovedBody(vaultName: name, host: status.host)],
      );
    }
    final blockers = _blockers;
    return ZafeScreen(
      title: kRelayMoveTitle,
      bottom: ActionButton(
        label: 'Move',
        enabled: _choice != null && blockers != null && blockers.isEmpty,
        errorFallback: 'Couldn\'t start the move. Try again.',
        onPressed: _move,
      ),
      children: [
        MoveHeading('Move $name to another relay'),
        MoveStartBody(
          host: _choice == null ? null : relayHost(_choice!.url),
          onPick: _pick,
          mustUpdate: blockers == null || blockers.isEmpty
              ? null
              : mustUpdateText([
                  for (final b in blockers)
                    memberLabel(b, me: me, names: names),
                ]),
        ),
      ],
    );
  }
}

/// Before the move: the relay to move to, how the others follow, who must update.
class MoveStartBody extends StatelessWidget {
  const MoveStartBody({
    super.key,
    required this.host,
    this.onPick,
    this.mustUpdate,
  });

  /// The picked relay, or null before one is picked.
  final String? host;
  final VoidCallback? onPick;
  final String? mustUpdate;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        MobileSurfaceCard(
          cornerRadius: AppRadii.large,
          child: MobileListRow(
            leading: const AppIcon(AppIcons.endpoint, size: 20),
            label: 'Relay',
            value: host ?? 'Choose',
            showChevron: true,
            minRowHeight: 44,
            onTap: onPick,
          ),
        ),
        const SizedBox(height: AppSpacing.md),
        Text(
          'Everyone moves by opening a link you send them.',
          style: AppTypography.bodyMedium.copyWith(
            color: colors.text.secondary,
          ),
        ),
        if (mustUpdate != null) ...[
          const SizedBox(height: AppSpacing.sm),
          Text(
            mustUpdate!,
            style: AppTypography.bodyMedium.copyWith(
              color: colors.text.warning,
            ),
          ),
        ],
      ],
    );
  }
}

/// During the move: who has moved, when payments switch, share, cancel.
class MoveProgressBody extends StatelessWidget {
  const MoveProgressBody({
    super.key,
    required this.members,
    required this.needed,
    required this.total,
    this.resume,
    this.onShare,
    this.onCancel,
  });

  /// (label, moved), this phone first.
  final List<(String, bool)> members;
  final int needed;
  final int total;

  /// "Payments resume when ..." when the old relay is down.
  final String? resume;
  final Future<void> Function()? onShare;
  final Future<void> Function()? onCancel;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        MobileSurfaceCard(
          cornerRadius: AppRadii.large,
          child: Column(
            children: [
              for (final (label, moved) in members)
                MobileListRow(
                  leading: AppIcon(
                    moved ? AppIcons.checkCircle : AppIcons.time,
                    size: 20,
                    active: moved,
                    color: moved ? colors.icon.accent : colors.icon.muted,
                  ),
                  label: label,
                  value: moved ? null : 'not yet',
                  minRowHeight: 44,
                ),
            ],
          ),
        ),
        const SizedBox(height: AppSpacing.md),
        Text(
          resume ?? paymentsSwitchText(needed, total),
          style: AppTypography.bodyMedium.copyWith(
            color: colors.text.secondary,
          ),
        ),
        const SizedBox(height: AppSpacing.md),
        ActionButton(
          label: 'Share link',
          variant: AppButtonVariant.secondary,
          onPressed: onShare ?? () async {},
        ),
        const SizedBox(height: AppSpacing.xs),
        ActionButton(
          label: 'Cancel move',
          variant: AppButtonVariant.ghost,
          errorFallback: 'Couldn\'t cancel. Try again.',
          onPressed: onCancel ?? () async {},
        ),
      ],
    );
  }
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
        'Members who haven\'t moved yet can still open your link.',
        style: AppTypography.bodyMedium.copyWith(
          color: context.colors.text.secondary,
        ),
      ),
    ],
  );
}

/// Following a move, from its link or the Home card. Nothing connects to the new relay
/// until Move.
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
      if (noPush) 'No notifications on this relay',
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

/// The Home card for a move: one another member started ("Move Grants to
/// relay.dao.org?"), or a switch this phone hasn't followed ("Grants moved to ...").
/// "Not now" on the follow screen hides it until the app starts again.
class RelayMoveCard extends ConsumerWidget {
  const RelayMoveCard({super.key, this.gap = AppSpacing.md});
  final double gap;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final view = ref.watch(relayMoveProvider);
    final vault = ref.watch(vaultProvider);
    final names = ref.watch(signerNamesProvider);
    final name =
        ref.watch(activeVaultNameProvider) ?? vault.summary?.name ?? '';
    final status = view.status;
    (String, String, String)? card;
    final n = view.card;
    if (status != null &&
        status.movedElsewhere &&
        !view.notNow.contains(status.link)) {
      card = (
        'Moved to ${status.host}',
        movedToText(name, status.host),
        status.link,
      );
    } else if (n != null && n.moved) {
      card = ('Moved to ${n.host}', movedToText(name, n.host), n.link);
    } else if (n != null) {
      final mover = memberLabel(n.moverHex, me: vault.myKeyHex, names: names);
      card = (
        'Move to ${n.host}?',
        '$mover is moving $name. ${n.arrived} of ${n.members} have moved.',
        n.link,
      );
    }
    if (view.twoMoves) {
      return Padding(
        padding: EdgeInsets.only(bottom: gap),
        child: const NoticeCard(
          title: 'Two moves found',
          body: 'Ask your group which relay to use.',
        ),
      );
    }
    if (card == null) return const SizedBox.shrink();
    final (title, body, link) = card;
    return Padding(
      padding: EdgeInsets.only(bottom: gap),
      child: NoticeCard(
        title: title,
        body: body,
        warning: false,
        onTap: () => context.push(
          Uri(
            path: '/relay-move/follow',
            queryParameters: {'link': link},
          ).toString(),
        ),
      ),
    );
  }
}

/// The payment screen's line while the vault's relay is down and a move hasn't reached m
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
