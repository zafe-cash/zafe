import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../core/layout/mobile/mobile_top_nav.dart';
import '../../core/layout/mobile/mobile_top_scroll_fade.dart';
import '../../core/formatting/member_label.dart';
import '../../core/security/unlock_gate.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/app_confirm.dart';
import '../../core/widgets/action_button.dart';
import '../../core/widgets/app_button.dart';
import '../../core/widgets/app_icon.dart';
import '../../core/widgets/mobile/mobile_surface_card.dart';
import '../../core/widgets/mobile/zafe_detail.dart';
import '../../providers/member_names_provider.dart';
import '../../providers/proposals_provider.dart';
import '../../providers/vault_provider.dart';
import '../../rust/api/proposals.dart' as rust show RepairInfo, RepairRestart;
import '../home/rename_sheet.dart';
import '../../core/widgets/line_icon.dart';
import '../backup/backup_prompt_screen.dart' show backupStatusProvider;
import 'keep_seat.dart';

/// The vault's signers and its approval rule (a tab). Signer management, such as
/// rotating a key, will live here.
class SignersScreen extends ConsumerWidget {
  const SignersScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.colors;
    final vault = ref.watch(vaultProvider);
    final summary = vault.summary;
    if (summary == null) return const SizedBox.shrink();
    final names = ref.watch(signerNamesProvider);
    final me = vault.myKeyHex;
    final threshold = summary.threshold;
    final members = summary.members;
    final proposals = ref.watch(proposalsProvider);
    // Unknown until the vault log has been read.
    final backedUp = proposals.loaded ? proposals.backedUp : null;
    // This phone hasn't saved a backup (the same check as Home's reminder).
    final iNeedBackup = ref.watch(backupStatusProvider).value == false;

    return SafeArea(
      bottom: false,
      child: Column(
        children: [
          const MobileTopNav.back(title: 'Signers'),
          Expanded(
            child: MobileTopScrollFade(
              child: ListView(
                padding: const EdgeInsets.fromLTRB(16, 12, 16, 112),
                children: [
                  MobileSurfaceCard(
                    cornerRadius: AppRadii.large,
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          '$threshold of ${members.length} to send',
                          style: AppTypography.headlineSmall.copyWith(
                            color: colors.text.accent,
                          ),
                        ),
                      ],
                    ),
                  ),
                  for (final m in proposals.pendingSeatMoves) ...[
                    const SizedBox(height: AppSpacing.md),
                    PendingSeatCard(
                      who: memberLabel(m.oldKeyHex, me: me, names: names),
                      mine: m.oldKeyHex == me,
                      canKeep: m.canKeep,
                      left: timeLeft(m.completesAt),
                      keptOn: dayOf(m.keptAt),
                      helped: m.helped.length,
                      helpers: m.helpers.length,
                      stopped: [
                        for (final k in m.stopped)
                          memberLabel(k, me: me, names: names),
                      ],
                      onKeep: m.oldKeyHex == me && m.canKeep
                          ? () => keepMySeatOrThrow(context, ref)
                          : null,
                      onStop: m.helpers.contains(me) && !m.stopped.contains(me)
                          ? () => _stopHelping(
                              context,
                              ref,
                              m.replacement,
                              memberLabel(m.oldKeyHex, me: me, names: names),
                            )
                          : null,
                    ),
                  ],
                  if (proposals.seatFinishing) ...[
                    const SizedBox(height: AppSpacing.md),
                    Text(
                      'Your seat is still finishing on the server.',
                      style: AppTypography.bodySmall.copyWith(
                        color: colors.text.secondary,
                      ),
                    ),
                  ],
                  for (final move in proposals.seatMoves) ...[
                    const SizedBox(height: AppSpacing.md),
                    SeatMoveCard(
                      who: memberLabel(move.oldKeyHex, me: me, names: names),
                      approvals: move.approvals.length,
                      needed: move.needed,
                      keptOn: dayOf(move.keptAt),
                      // The signer who lost the phone can't approve; nor twice.
                      onApprove:
                          move.oldKeyHex == me || move.approvals.contains(me)
                          ? null
                          : () => context.push(
                              '/replace-signer',
                              extra: (move.oldKeyHex, move.code),
                            ),
                    ),
                  ],
                  // A pending move's helpers show on its card until its wait is over
                  // here; after that one who never logged its part can be replaced.
                  for (final r in proposals.repairs)
                    if (r.newKeyHex != me &&
                        (!r.pending ||
                            proposals.pendingSeatMoves.any(
                              (m) =>
                                  m.replacement == r.replacement &&
                                  m.oldKeyHex != me &&
                                  m.completesAt != null &&
                                  timeLeft(m.completesAt) == null,
                            ))) ...[
                      const SizedBox(height: AppSpacing.md),
                      RepairCard(
                        who: memberLabel(r.newKeyHex, me: me, names: names),
                        helpers: [
                          for (final h in r.helpers)
                            (h, memberLabel(h, me: me, names: names)),
                        ],
                        meHelping: r.helpers.contains(me),
                        offers: [
                          for (final x in r.restarts)
                            if (_change(r, x) case (final stalled, final taker))
                              _offer(
                                context,
                                ref,
                                r.replacement,
                                x,
                                stalled,
                                taker,
                                me: me,
                                label: (k) =>
                                    memberLabel(k, me: me, names: names),
                              ),
                        ],
                        // One offer at a time from this phone.
                        onTakeOver: r.restarts.any((x) => x.asked.contains(me))
                            ? null
                            : (stalled, name) => _takeOver(
                                context,
                                ref,
                                r.replacement,
                                stalled,
                                name,
                                me!,
                              ),
                      ),
                    ],
                  if (backedUp != null) ...[
                    const SizedBox(height: AppSpacing.md),
                    BackupHealthCard(
                      signers: members.length,
                      threshold: threshold,
                      backedUp: members.where(backedUp.contains).length,
                      onBackUp: iNeedBackup
                          ? () => context.push('/export')
                          : null,
                    ),
                  ],
                  const SizedBox(height: AppSpacing.md),
                  MobileSurfaceCard(
                    cornerRadius: AppRadii.large,
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        for (final m in members)
                          SignerRow(
                            keyHex: m,
                            me: me,
                            name: names[m],
                            trailing: backedUp == null
                                ? null
                                : BackupLabel(backedUp: backedUp.contains(m)),
                            onTap: m == me
                                ? () => showMyNameSheet(context, ref)
                                : () => showRenameSignerSheet(context, ref, m),
                          ),
                      ],
                    ),
                  ),
                  if (members.length > 1) ...[
                    const SizedBox(height: AppSpacing.md),
                    AppButton(
                      expand: true,
                      variant: AppButtonVariant.secondary,
                      leading: const AppIcon(AppIcons.renew, size: 20),
                      onPressed: () => context.push('/replace-signer'),
                      child: const Text('Replace a lost phone'),
                    ),
                  ],
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}

/// Stop helping a pending seat move, for good: a confirm (it can't be undone), then the
/// unlock. Throws [ActionCancelled] when either is declined (for an [ActionButton]).
Future<void> _stopHelping(
  BuildContext context,
  WidgetRef ref,
  BigInt replacement,
  String who,
) async {
  final ok = await showAppConfirm(
    context,
    title: 'Stop helping move $who\'s seat?',
    body:
        'Only if $who says they didn\'t lose their phone. You can\'t help this move again.',
    confirmLabel: 'Stop helping',
  );
  if (!ok || !context.mounted) throw const ActionCancelled();
  if (!await confirmUnlock(context, ref, reason: 'Stop helping this move')) {
    throw const ActionCancelled();
  }
  await ref.read(proposalsProvider.notifier).stopHelping(replacement);
}

/// The one helper a restart replaces and the member taking its place, when it changes
/// exactly one (what this app asks for).
(String, String)? _change(rust.RepairInfo r, rust.RepairRestart x) {
  final leaving = r.helpers.where((h) => !x.helpers.contains(h)).toList();
  final taking = x.helpers.where((h) => !r.helpers.contains(h)).toList();
  return leaving.length == 1 && taking.length == 1
      ? (leaving.single, taking.single)
      : null;
}

/// How the card shows a member's offer to help instead of `stalled`.
RepairOffer _offer(
  BuildContext context,
  WidgetRef ref,
  BigInt replacement,
  rust.RepairRestart x,
  String stalled,
  String taker, {
  required String? me,
  required String Function(String keyHex) label,
}) {
  final waiting = x.helpers
      .where((h) => !x.asked.contains(h))
      .map((h) => h == me ? 'you' : label(h));
  return (
    text: taker == me
        ? 'You offered to help instead of ${label(stalled)}.'
        : '${label(taker)} offers to help instead of ${label(stalled)}.',
    waitingFor: waiting.isEmpty ? null : waiting.join(' and '),
    onAgree: x.helpers.contains(me) && !x.asked.contains(me)
        ? () => _agree(context, ref, replacement, stalled, taker, label)
        : null,
  );
}

/// Asks, then takes over from a helper who isn't rebuilding a moved seat's key. The
/// rebuild starts over once the other helpers agree.
Future<void> _takeOver(
  BuildContext context,
  WidgetRef ref,
  BigInt replacement,
  String stalled,
  String name,
  String me,
) async {
  final ok = await showAppConfirm(
    context,
    title: 'Help instead of $name?',
    body:
        'Your phone takes $name\'s place once the other helpers agree, and the rebuild '
        'starts over. Only do this if $name can\'t open Zafe for a while.',
    confirmLabel: 'Help instead',
    destructive: false,
  );
  if (!ok || !context.mounted) throw const ActionCancelled();
  if (!await confirmUnlock(context, ref, reason: 'Help rebuild a key')) {
    throw const ActionCancelled();
  }
  await ref
      .read(proposalsProvider.notifier)
      .retryRepair(replacement, stalled, me);
}

/// A helper agrees that `taker` helps instead of `stalled`.
Future<void> _agree(
  BuildContext context,
  WidgetRef ref,
  BigInt replacement,
  String stalled,
  String taker,
  String Function(String keyHex) label,
) async {
  final ok = await showAppConfirm(
    context,
    title: 'Let ${label(taker)} help?',
    body:
        'The rebuild starts over with ${label(taker)} instead of ${label(stalled)}. '
        'Only agree if ${label(stalled)} can\'t open Zafe for a while.',
    confirmLabel: 'Agree',
    destructive: false,
  );
  if (!ok || !context.mounted) throw const ActionCancelled();
  if (!await confirmUnlock(context, ref, reason: 'Help rebuild a key')) {
    throw const ActionCancelled();
  }
  await ref
      .read(proposalsProvider.notifier)
      .retryRepair(replacement, stalled, taker);
}

/// The Signers tab's backup card text: what losing phones without a backup means for a
/// vault of [signers] needing [threshold], with [backedUp] backups attested.
@visibleForTesting
String backupHealthText({
  required int signers,
  required int threshold,
  required int backedUp,
}) {
  final missing = signers - backedUp;
  final spare = signers - threshold;
  final String body;
  if (missing <= 0) {
    body = 'Every signer has saved a backup of their key.';
  } else if (missing <= spare) {
    body =
        '${missing == 1 ? '1 signer hasn\'t' : '$missing signers haven\'t'} saved '
        'a backup yet. The vault keeps working if up to $spare '
        '${spare == 1 ? 'signer loses their phone' : 'signers lose their phones'}, '
        'but each loss without a backup uses up that margin.';
  } else {
    final who = missing == 1 ? '1 signer hasn\'t' : '$missing signers haven\'t';
    // No spare seat (n-of-n): one lost phone without a backup is enough.
    final loss = spare == 0
        ? (missing == 1
              ? 'If they lose their phone'
              : 'If any of them loses their phone')
        : 'If more than $spare of '
              '${spare == 1 ? 'them loses their phone' : 'them lose their phones'}';
    body = '$who saved a backup yet. $loss, the funds are lost for good.';
  }
  return body;
}

/// How many signers told the vault they have a backup, and what it means if they don't
/// (spec §12.1–12.2).
class BackupHealthCard extends StatelessWidget {
  const BackupHealthCard({
    super.key,
    required this.signers,
    required this.threshold,
    required this.backedUp,
    this.onBackUp,
  });

  /// Set when this phone has no backup yet: the card offers "Back up now".
  final VoidCallback? onBackUp;

  final int signers;
  final int threshold;
  final int backedUp;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final ok = backedUp >= signers;
    final body = backupHealthText(
      signers: signers,
      threshold: threshold,
      backedUp: backedUp,
    );
    return MobileSurfaceCard(
      cornerRadius: AppRadii.large,
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          LineIcon(
            size: 22,
            line: AppTypography.labelLarge,
            icon: AppIcon(
              ok ? AppIcons.checkCircle : AppIcons.warningCircle,
              size: 22,
              color: ok ? colors.icon.accent : colors.icon.warning,
            ),
          ),
          const SizedBox(width: AppSpacing.s),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  'Backups: $backedUp of $signers',
                  style: AppTypography.labelLarge.copyWith(
                    color: colors.text.accent,
                  ),
                ),
                const SizedBox(height: AppSpacing.xxs),
                Text(
                  body,
                  style: AppTypography.bodySmall.copyWith(
                    color: colors.text.secondary,
                  ),
                ),
                if (onBackUp != null) ...[
                  const SizedBox(height: AppSpacing.sm),
                  AppButton(
                    size: AppButtonSize.medium,
                    onPressed: onBackUp,
                    child: const Text('Back up now'),
                  ),
                ],
              ],
            ),
          ),
        ],
      ),
    );
  }
}

/// "Backed up" or "No backup" at the end of a signer row.
class BackupLabel extends StatelessWidget {
  const BackupLabel({super.key, required this.backedUp});

  final bool backedUp;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return Text(
      backedUp ? 'Backed up' : 'No backup',
      style: AppTypography.bodySmall.copyWith(
        color: backedUp ? colors.text.muted : colors.text.warning,
      ),
    );
  }
}

/// A signer moving to a new phone, waiting for co-signers' approvals.
class SeatMoveCard extends StatelessWidget {
  const SeatMoveCard({
    super.key,
    required this.who,
    required this.approvals,
    required this.needed,
    this.keptOn,
    this.onApprove,
  });

  final String who;
  final int approvals;
  final int needed;

  /// The day the seat's old phone said it still has the seat ("13 Nov"), if it did.
  final String? keptOn;

  /// Null when this member can't approve (it's their seat, or they already did).
  final VoidCallback? onApprove;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return MobileSurfaceCard(
      cornerRadius: AppRadii.large,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              AppIcon(AppIcons.renew, size: 22, color: colors.icon.accent),
              const SizedBox(width: AppSpacing.s),
              Expanded(
                child: Text(
                  '$who is moving to a new phone',
                  style: AppTypography.labelLarge.copyWith(
                    color: colors.text.accent,
                  ),
                ),
              ),
            ],
          ),
          if (keptOn case final day?) ...[
            const SizedBox(height: AppSpacing.xs),
            Text(
              'The old phone said it still has this seat on $day: call $who first.',
              style: AppTypography.bodySmall.copyWith(
                color: colors.text.warning,
              ),
            ),
          ],
          const SizedBox(height: AppSpacing.xs),
          Text(
            onApprove == null
                ? 'Approvals: $approvals of $needed.'
                : 'Approvals: $approvals of $needed. Compare safety words with $who '
                      'before approving.',
            style: AppTypography.bodySmall.copyWith(
              color: colors.text.secondary,
            ),
          ),
          if (onApprove != null) ...[
            const SizedBox(height: AppSpacing.s),
            SizedBox(
              width: double.infinity,
              child: AppButton(
                expand: true,
                size: AppButtonSize.medium,
                onPressed: onApprove,
                child: const Text('Check and approve'),
              ),
            ),
          ],
        ],
      ),
    );
  }
}

/// A seat move with every approval, waiting out the helpers' 72 h. On the old phone it
/// offers Keep my seat (first round only); elsewhere it shows the countdown by this
/// phone's clock.
class PendingSeatCard extends StatelessWidget {
  const PendingSeatCard({
    super.key,
    required this.who,
    required this.mine,
    required this.canKeep,
    required this.left,
    required this.helped,
    required this.helpers,
    this.stopped = const [],
    this.keptOn,
    this.onKeep,
    this.onStop,
  });

  final String who;

  /// This phone's own seat.
  final bool mine;
  final bool canKeep;

  /// Time left ("2 d 5 h"); null once over or before this phone counted.
  final String? left;
  final int helped;
  final int helpers;
  final String? keptOn;

  /// Helpers who stopped helping (labels): the seat can't switch while they are named.
  final List<String> stopped;
  final Future<void> Function()? onKeep;

  /// This phone helps and may stop (only if the seat's owner says they still have it).
  final Future<void> Function()? onStop;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final when = left == null ? 'soon' : 'in $left';
    final text = mine
        ? canKeep
              ? 'Every approval is in. It moves $when unless you keep it.'
              : 'You kept it on $keptOn; your co-signers asked again. Call them.'
        : canKeep
        ? 'Every approval is in. It moves $when unless their old phone keeps it. '
              'Helpers done: $helped of $helpers.'
        : 'Asked again after the old phone kept it on $keptOn. It moves $when. '
              'Helpers done: $helped of $helpers.';
    return MobileSurfaceCard(
      cornerRadius: AppRadii.large,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              AppIcon(AppIcons.renew, size: 22, color: colors.icon.accent),
              const SizedBox(width: AppSpacing.s),
              Expanded(
                child: Text(
                  mine
                      ? 'Your seat is moving to a new phone'
                      : '$who is moving to a new phone',
                  style: AppTypography.labelLarge.copyWith(
                    color: colors.text.accent,
                  ),
                ),
              ),
            ],
          ),
          const SizedBox(height: AppSpacing.xs),
          Text(
            text,
            style: AppTypography.bodySmall.copyWith(
              color: mine ? colors.text.warning : colors.text.secondary,
            ),
          ),
          for (final who in stopped) ...[
            const SizedBox(height: AppSpacing.xs),
            Text(
              '$who stopped helping',
              style: AppTypography.bodySmall.copyWith(
                color: colors.text.warning,
              ),
            ),
          ],
          if (onStop != null) ...[
            const SizedBox(height: AppSpacing.s),
            SizedBox(
              width: double.infinity,
              child: ActionButton(
                label: 'Stop helping',
                doneLabel: 'Stopped',
                variant: AppButtonVariant.secondary,
                size: AppButtonSize.medium,
                errorFallback: 'Couldn\'t stop. Try again.',
                onPressed: onStop!,
              ),
            ),
          ],
          if (onKeep != null) ...[
            const SizedBox(height: AppSpacing.s),
            SizedBox(
              width: double.infinity,
              child: ActionButton(
                label: 'Keep my seat',
                doneLabel: 'Kept',
                size: AppButtonSize.medium,
                errorFallback: 'Couldn\'t keep your seat. Try again.',
                onPressed: onKeep!,
              ),
            ),
          ],
        ],
      ),
    );
  }
}

/// A member's offer to help instead of a stalled helper: what the card says, who still
/// has to agree, and this phone's "Agree" when it is one of them.
typedef RepairOffer = ({
  String text,
  String? waitingFor,
  Future<void> Function()? onAgree,
});

/// A moved seat whose key the helpers' phones are still rebuilding on the new phone. A
/// member who isn't helping can offer to take over from a helper who can't; the rebuild
/// starts over once the other helpers agree.
class RepairCard extends StatelessWidget {
  const RepairCard({
    super.key,
    required this.who,
    required this.helpers,
    required this.meHelping,
    this.offers = const [],
    this.onTakeOver,
  });

  final String who;

  /// `(key hex, label)` of the helpers in the current attempt.
  final List<(String, String)> helpers;
  final bool meHelping;
  final List<RepairOffer> offers;
  final Future<void> Function(String stalled, String name)? onTakeOver;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final names = helpers.map((h) => h.$2).join(' and ');
    return MobileSurfaceCard(
      cornerRadius: AppRadii.large,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              AppIcon(AppIcons.key, size: 22, color: colors.icon.accent),
              const SizedBox(width: AppSpacing.s),
              Expanded(
                child: Text(
                  'Rebuilding $who\'s key',
                  style: AppTypography.labelLarge.copyWith(
                    color: colors.text.accent,
                  ),
                ),
              ),
            ],
          ),
          const SizedBox(height: AppSpacing.xs),
          Text(
            meHelping
                ? 'Your phone is a helper, with $names. It does its part whenever '
                      'Zafe runs.'
                : '$names rebuild it on $who\'s new phone when they open Zafe. If '
                      'one can\'t for a while, help in their place.',
            style: AppTypography.bodySmall.copyWith(
              color: colors.text.secondary,
            ),
          ),
          for (final o in offers) ...[
            const SizedBox(height: AppSpacing.s),
            Text(
              o.waitingFor == null
                  ? o.text
                  : '${o.text} Waiting for ${o.waitingFor} to agree.',
              style: AppTypography.bodySmall.copyWith(
                color: colors.text.secondary,
              ),
            ),
            if (o.onAgree != null) ...[
              const SizedBox(height: AppSpacing.xs),
              SizedBox(
                width: double.infinity,
                child: ActionButton(
                  label: 'Agree',
                  variant: AppButtonVariant.secondary,
                  size: AppButtonSize.medium,
                  errorFallback: 'Couldn\'t agree. Try again.',
                  onPressed: o.onAgree!,
                ),
              ),
            ],
          ],
          if (!meHelping && onTakeOver != null)
            for (final (key, name) in helpers) ...[
              const SizedBox(height: AppSpacing.s),
              SizedBox(
                width: double.infinity,
                child: ActionButton(
                  label: 'Help instead of $name',
                  variant: AppButtonVariant.secondary,
                  size: AppButtonSize.medium,
                  errorFallback: 'Couldn\'t start over. Try again.',
                  onPressed: () => onTakeOver!(key, name),
                ),
              ),
            ],
        ],
      ),
    );
  }
}
