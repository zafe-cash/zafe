import 'dart:async';

import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../core/config/network_config.dart';
import '../../core/errors/zafe_error_copy.dart';
import '../../core/formatting/member_label.dart';
import '../../core/formatting/zec_amount.dart';
import '../../core/privacy/privacy_mask.dart';
import '../../core/theme/app_theme.dart';
import '../../core/security/unlock_gate.dart';
import '../../core/widgets/animated_keyed_column.dart';
import '../../core/widgets/app_button.dart';
import '../../core/widgets/app_tappable.dart';
import '../../core/widgets/mobile/mobile_surface_card.dart';
import '../../core/widgets/mobile/zafe_detail.dart';
import '../../providers/member_names_provider.dart';
import '../../providers/privacy_mode_provider.dart';
import '../../providers/proposals_provider.dart';
import '../../providers/vault_provider.dart';
import '../../rust/api/proposals.dart' as rust;
import '../proposals/proposal_status.dart';

/// Declines in flight (hidden at once, so the card leaves on the tap), and the ones that
/// failed (the card comes back with the reason).
class Declines {
  const Declines({this.pending = const {}, this.failed = const {}});
  final Set<String> pending;
  final Map<String, String> failed;
}

class DeclinesNotifier extends Notifier<Declines> {
  @override
  Declines build() => const Declines();

  /// Declining moves no money and needs no check, so the card leaves right away; the
  /// vote (a log entry on the relay, a few seconds) finishes in the background.
  Future<void> decline(String id) async {
    state = Declines(
      pending: {...state.pending, id},
      failed: {...state.failed}..remove(id),
    );
    String? error;
    try {
      await ref.read(proposalsProvider.notifier).reject(id);
    } catch (e) {
      error = zafeErrorMessage(e, fallback: 'Couldn\'t decline. Try again.');
    }
    state = Declines(
      pending: {...state.pending}..remove(id),
      failed: {...state.failed, id: ?error},
    );
  }
}

final declinesProvider = NotifierProvider<DeclinesNotifier, Declines>(
  DeclinesNotifier.new,
);

/// "Needs you" on Home: what the group is waiting on this member for, as request cards
/// (a payment to approve or send, a seat move to check). Shown only when there is
/// something; cards slide out once handled. Each opens where the action happens.
class NeedsYou extends ConsumerStatefulWidget {
  const NeedsYou({super.key, this.height});
  final int? height;

  /// From this many requests on, they stack (the first in full, the others' edges under
  /// it) until opened.
  static const stackFrom = 3;

  @override
  ConsumerState<NeedsYou> createState() => _NeedsYouState();

  /// "Send now" from Home: nothing is left to decide, so after the unlock it sends and
  /// shows the sending screen.
  static Future<void> _send(
    BuildContext context,
    WidgetRef ref,
    String id,
  ) async {
    if (!await confirmUnlock(context, ref, reason: 'Send this payment')) return;
    if (context.mounted) unawaited(context.push('/proposal/$id/send'));
  }

  /// The message, else the recipient; with "expires in ..." when under a day is left.
  static String _detail(rust.ProposalInfo p, int? height) {
    final payment = p.payments.isEmpty ? null : p.payments.first;
    final base = payment == null
        ? 'Back to the vault'
        : p.payments.length > 1
        ? 'To ${p.payments.length} recipients'
        : payment.memo.isNotEmpty
        ? payment.memo
        : 'To ${compactAddress(payment.address)}';
    if (height == null || p.expiryHeight <= 0) return base;
    final secs = blocksToSeconds(height, p.expiryHeight);
    return secs > 0 && secs < 86400
        ? '$base · expires in ${expiresIn(p, height)}'
        : base;
  }
}

class _NeedsYouState extends ConsumerState<NeedsYou> {
  /// The member opened the stack; it folds back once fewer than [NeedsYou.stackFrom]
  /// requests remain.
  bool _open = false;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    // Home stays mounted behind the other tabs (their tickers are muted): leaving it
    // folds the stack, so it's tidy on the way back.
    if (!TickerMode.valuesOf(context).enabled) _open = false;
  }

  @override
  Widget build(BuildContext context) {
    final height = widget.height;
    final colors = context.colors;
    final proposals = ref.watch(proposalsProvider);
    final me = ref.watch(vaultProvider).myKeyHex;
    final names = ref.watch(signerNamesProvider);
    final hide = ref.watch(privacyModeProvider);
    final declines = ref.watch(declinesProvider);
    final waiting =
        proposals.items
            .where(
              (p) =>
                  needsMyAction(p, height: height) &&
                  !declines.pending.contains(p.id),
            )
            .length +
        proposals.seatMoves
            .where((m) => m.oldKeyHex != me && !m.approvals.contains(me))
            .length;
    final stacks = waiting >= NeedsYou.stackFrom;
    if (!stacks && _open) _open = false;
    // Stacked, the top card's tap opens the stack (its buttons still act).
    var first = true;
    VoidCallback tap(VoidCallback open) {
      final collapsedTop = first && stacks && !_open;
      first = false;
      if (collapsedTop) return () => setState(() => _open = true);
      // Leaving for a payment folds the stack, so Home is tidy on the way back.
      return () {
        if (_open) setState(() => _open = false);
        open();
      };
    }

    final cards = <Widget>[
      for (final p in proposals.items)
        if (needsMyAction(p, height: height) &&
            !declines.pending.contains(p.id))
          RequestCard(
            key: ValueKey('p:${p.id}'),
            tileKey: p.author,
            who: memberLabel(p.author, me: me, names: names),
            amount: p.payments.isEmpty
                ? null
                : hide
                ? fixedPrivacyMask(length: 3)
                : ZecAmount.fromZatoshi(p.totalZat).activity.amountText,
            detail: NeedsYou._detail(p, height),
            onTap: tap(() => context.push('/proposal/${p.id}')),
            // Approve opens the request (the recipient and this phone's check come
            // first, then the fingerprint); Decline moves no money, so it acts here. A
            // payment with every approval is sent right away, after the unlock.
            action: p.stage == rust.ProposalStage.approved && !p.needsReapproval
                ? 'Send now'
                : 'Approve',
            onAction:
                p.stage == rust.ProposalStage.approved && !p.needsReapproval
                ? () => NeedsYou._send(context, ref, p.id)
                : () => context.push('/proposal/${p.id}'),
            onDecline:
                p.stage == rust.ProposalStage.open &&
                    p.myVote == rust.MyVote.none
                ? () => ref.read(declinesProvider.notifier).decline(p.id)
                : null,
            error: declines.failed[p.id],
          ),
      for (final m in proposals.seatMoves)
        if (m.oldKeyHex != me && !m.approvals.contains(me))
          RequestCard(
            key: ValueKey('m:${m.oldKeyHex}:${m.newKeyHex}'),
            tileKey: m.oldKeyHex,
            who: memberLabel(m.oldKeyHex, me: me, names: names),
            detail:
                'Moving to a new phone · ${m.approvals.length} of ${m.needed} approved',
            action: 'Compare safety words',
            onTap: tap(
              () =>
                  context.push('/replace-signer', extra: (m.oldKeyHex, m.code)),
            ),
          ),
    ];
    return AnimatedSize(
      duration: AnimatedKeyedColumn.duration,
      curve: Curves.easeOutCubic,
      alignment: Alignment.topCenter,
      child: cards.isEmpty
          ? const SizedBox(width: double.infinity)
          : Padding(
              padding: const EdgeInsets.fromLTRB(
                AppSpacing.xs,
                AppSpacing.s,
                AppSpacing.xs,
                AppSpacing.s,
              ),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Row(
                    children: [
                      Expanded(
                        child: Text(
                          'Needs you',
                          style: AppTypography.labelLarge.copyWith(
                            color: colors.text.brand,
                            fontWeight: FontWeight.w600,
                          ),
                        ),
                      ),
                      if (stacks && !_open)
                        Text(
                          '${cards.length} waiting',
                          style: AppTypography.labelMedium.copyWith(
                            color: colors.text.secondary,
                          ),
                        ),
                    ],
                  ),
                  const SizedBox(height: AppSpacing.s),
                  if (stacks)
                    _FanStack(
                      open: _open,
                      cards: cards,
                      onOpen: () => setState(() => _open = true),
                    )
                  else
                    AnimatedKeyedColumn(spacing: AppSpacing.s, children: cards),
                ],
              ),
            ),
    );
  }
}

/// Three or more requests: the first in full with the edges of the next two under it,
/// like a stack of notifications. Opening it fans the cards out: each grows down out of
/// the stack while the edges fade (300 ms). Taps reach only the cards that are out.
class _FanStack extends StatefulWidget {
  const _FanStack({
    required this.open,
    required this.cards,
    required this.onOpen,
  });
  final bool open;
  final List<Widget> cards;

  /// The edges under the top card open the stack too.
  final VoidCallback onOpen;

  @override
  State<_FanStack> createState() => _FanStackState();
}

class _FanStackState extends State<_FanStack>
    with SingleTickerProviderStateMixin {
  late final AnimationController _c = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 300),
    value: widget.open ? 1 : 0,
  );
  late final Animation<double> _t = CurvedAnimation(
    parent: _c,
    curve: Curves.easeOutCubic,
    reverseCurve: Curves.easeInCubic,
  );

  static const _edge = 8.0;

  @override
  void didUpdateWidget(_FanStack old) {
    super.didUpdateWidget(old);
    if (widget.open != old.open) {
      if (MediaQuery.disableAnimationsOf(context)) {
        _c.value = widget.open ? 1 : 0;
      } else if (widget.open) {
        _c.forward();
      } else {
        _c.reverse();
      }
    }
  }

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final edges = (widget.cards.length - 1).clamp(0, 2);
    Widget edge(int i) => Container(
      height: 24,
      margin: EdgeInsets.symmetric(horizontal: AppSpacing.s * (i + 1)),
      // Surfaces fading back, no outline: in dark mode an outline read as wire.
      decoration: BoxDecoration(
        color: colors.background.ground.withValues(alpha: i == 0 ? 0.7 : 0.4),
        borderRadius: BorderRadius.circular(AppRadii.large),
      ),
    );
    return AnimatedBuilder(
      animation: _t,
      builder: (context, _) {
        final t = _t.value;
        return Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Stack(
              clipBehavior: Clip.none,
              children: [
                if (t < 1)
                  for (var i = edges - 1; i >= 0; i--)
                    Positioned(
                      left: 0,
                      right: 0,
                      bottom: (edges - 1 - i) * _edge * (1 - t),
                      child: GestureDetector(
                        behavior: HitTestBehavior.opaque,
                        onTap: widget.onOpen,
                        child: Opacity(opacity: 1 - t, child: edge(i)),
                      ),
                    ),
                Padding(
                  padding: EdgeInsets.only(bottom: _edge * edges * (1 - t)),
                  child: widget.cards.first,
                ),
              ],
            ),
            for (final card in widget.cards.skip(1))
              IgnorePointer(
                ignoring: t < 1,
                child: Padding(
                  padding: EdgeInsets.only(top: AppSpacing.s * t),
                  child: ClipRect(
                    child: Align(
                      alignment: Alignment.topCenter,
                      heightFactor: t,
                      child: Opacity(
                        opacity: t,
                        child: Transform.translate(
                          offset: Offset(0, -24 * (1 - t)),
                          child: card,
                        ),
                      ),
                    ),
                  ),
                ),
              ),
          ],
        );
      },
    );
  }
}

/// One request: who it's from, a detail line (the message, or what is asked) and the
/// amount, then a button naming the decision across the card (tonal, so several cards
/// don't drown out the screen's main action). The card itself opens the payment;
/// [onAction] defaults to the same.
class RequestCard extends StatelessWidget {
  const RequestCard({
    super.key,
    required this.tileKey,
    required this.who,
    this.amount,
    required this.detail,
    required this.action,
    required this.onTap,
    this.onAction,
    this.onDecline,
    this.error,
  });

  /// Key hex the signer tile is drawn from.
  final String tileKey;
  final String who;

  /// Without the ticker ("0.8"); the card adds it.
  final String? amount;
  final String detail;
  final String action;
  final VoidCallback onTap;
  final VoidCallback? onAction;

  /// Shows "Decline" next to the action (a vote), done right on the card.
  final VoidCallback? onDecline;

  /// Why the last attempt on this card failed, shown under its buttons.
  final String? error;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final strong = AppTypography.labelLarge.copyWith(
      color: colors.text.accent,
      fontWeight: FontWeight.w600,
    );
    return MobileSurfaceCard(
      cornerRadius: AppRadii.large,
      padding: const EdgeInsets.all(AppSpacing.s),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          AppTappable(
            onTap: onTap,
            semanticsLabel: '${amount == null ? '' : '$amount. '}$who. $detail',
            child: Row(
              children: [
                SignerTile(keyHex: tileKey, size: 40),
                const SizedBox(width: AppSpacing.s),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      // The amount leads (what is asked); who and why under it.
                      if (amount == null)
                        Text(
                          who,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: strong,
                        )
                      else
                        Text.rich(
                          TextSpan(
                            children: [
                              TextSpan(text: amount),
                              TextSpan(
                                text: ' $kZcashDefaultCurrencyTicker',
                                style: TextStyle(
                                  fontSize: 13,
                                  color: colors.text.value,
                                ),
                              ),
                            ],
                          ),
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: strong.copyWith(
                            fontFamily: 'Space Grotesk',
                            fontSize: 18,
                          ),
                        ),
                      const SizedBox(height: 2),
                      Text(
                        amount == null ? detail : '$who · $detail',
                        maxLines: 2,
                        overflow: TextOverflow.ellipsis,
                        style: AppTypography.bodySmall.copyWith(
                          color: colors.text.secondary,
                        ),
                      ),
                    ],
                  ),
                ),
              ],
            ),
          ),
          const SizedBox(height: AppSpacing.s),
          Row(
            children: [
              if (onDecline != null) ...[
                Expanded(
                  child: AppButton(
                    expand: true,
                    variant: AppButtonVariant.ghost,
                    size: AppButtonSize.medium,
                    onPressed: onDecline,
                    child: const Text('Decline'),
                  ),
                ),
                const SizedBox(width: AppSpacing.xs),
              ],
              Expanded(
                child: AppButton(
                  expand: true,
                  size: AppButtonSize.medium,
                  enabledBackgroundColor: colors.background.brandAlpha,
                  enabledLabelColor: colors.text.brand,
                  enabledBorderColor: const Color(0x00000000),
                  onPressed: onAction ?? onTap,
                  child: Text(action),
                ),
              ),
            ],
          ),
          if (error != null) ...[
            const SizedBox(height: AppSpacing.xs),
            Text(
              error!,
              textAlign: TextAlign.center,
              style: AppTypography.bodySmall.copyWith(
                color: colors.text.destructive,
              ),
            ),
          ],
        ],
      ),
    );
  }
}
