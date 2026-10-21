import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../core/feedback/payment_feedback.dart';
import '../../core/layout/mobile/zafe_screen.dart';
import '../../core/config/network_upgrade.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/action_button.dart';
import '../../core/widgets/app_button.dart';
import '../../core/widgets/app_icon.dart';
import '../../core/widgets/app_loading_icon.dart';
import '../../core/layout/mobile/app_mobile_sheet.dart';
import '../../core/widgets/mobile/mobile_surface_card.dart';
import '../../providers/member_names_provider.dart';
import '../../providers/payment_sounds_provider.dart';
import '../../providers/proposals_provider.dart';
import '../../core/security/unlock_gate.dart';
import '../../providers/vault_provider.dart';
import '../../rust/api/proposals.dart' as rust;
import '../send/send_screen.dart' show SendPrefill;
import 'proposal_parts.dart';
import 'proposal_status.dart';

/// One payment proposal: what it pays, this device's independent check, the signers'
/// votes, and the next action (approve/reject, or collect signatures and send).
class ProposalScreen extends ConsumerStatefulWidget {
  const ProposalScreen({super.key, required this.id});
  final String id;

  @override
  ConsumerState<ProposalScreen> createState() => _ProposalScreenState();
}

class _ProposalScreenState extends ConsumerState<ProposalScreen> {
  Timer? _poll;

  @override
  void initState() {
    super.initState();
    Future.microtask(() => ref.read(proposalsProvider.notifier).refresh());
    _poll = Timer.periodic(const Duration(seconds: 5), (_) {
      final send = ref.read(proposalsProvider).sends[widget.id];
      if (!(send?.running ?? false)) {
        ref.read(proposalsProvider.notifier).refresh();
      }
    });
  }

  @override
  void dispose() {
    _poll?.cancel();
    super.dispose();
  }

  /// "Approve again" (a signing round didn't finish, or the phone was restored).
  Future<void> _reapprove() async {
    await _unlock('Approve this payment. It can\'t be undone.');
    final r = await ref.read(proposalsProvider.notifier).approve(widget.id);
    unawaited(
      PaymentFeedback.play(
        approvalMoment(completed: r.completed),
        sound: ref.read(paymentSoundsProvider),
      ),
    );
    if (!mounted) return;
    if (r.completed && r.autoSend) {
      unawaited(context.push('/proposal/${widget.id}/send'));
    } else if (!r.completed) {
      _leave();
    }
  }

  /// The request stays on screen from the tap until its button has shown "Approved"
  /// or "Declined" (the vote changes the payment, which would swap the page at once).
  bool _holdRequest = false;
  rust.ApproveResult? _approved;

  /// "Approve" on a request: the unlock prompt is the confirmation.
  Future<void> _approveRequest() async {
    setState(() => _holdRequest = true);
    try {
      if (!await confirmUnlock(
        context,
        ref,
        reason: 'Approve this payment. It can\'t be undone.',
      )) {
        throw const ActionCancelled();
      }
      final r = await ref.read(proposalsProvider.notifier).approve(widget.id);
      _approved = r;
      unawaited(
        PaymentFeedback.play(
          approvalMoment(completed: r.completed),
          sound: ref.read(paymentSoundsProvider),
        ),
      );
    } catch (_) {
      if (mounted) setState(() => _holdRequest = false);
      rethrow;
    }
  }

  void _afterApprove() {
    final r = _approved;
    if (!mounted || r == null) return;
    // Leaving keeps the request on screen as it slides away; only a payment that stays
    // here (complete, sent by hand) switches to its status.
    if (r.completed && r.autoSend) {
      // This approval completed the signatures: watch it go out.
      context.push('/proposal/${widget.id}/send');
    } else if (!r.completed) {
      // Nothing left to do here until more signers approve.
      _leave();
    } else {
      setState(() => _holdRequest = false);
    }
  }

  Future<void> _declineRequest() async {
    setState(() => _holdRequest = true);
    try {
      await ref.read(proposalsProvider.notifier).reject(widget.id);
    } catch (_) {
      if (mounted) setState(() => _holdRequest = false);
      rethrow;
    }
  }

  /// Back to where the payment was opened from (Home if it was opened directly).
  void _leave() {
    if (context.canPop()) {
      context.pop();
    } else {
      context.go('/home');
    }
  }

  Future<void> _startSend() async {
    if (!await confirmUnlock(
      context,
      ref,
      reason: 'Unlock to send this payment',
    )) {
      return;
    }
    if (mounted) context.push('/proposal/${widget.id}/send');
  }

  Future<void> _startOver() async {
    if (!await confirmUnlock(
      context,
      ref,
      reason: 'Unlock to start a new signing round',
    )) {
      return;
    }
    // Reset the round before the sending screen opens (it would resume the old one).
    await ref.read(proposalsProvider.notifier).startOver(widget.id);
    if (mounted) context.push('/proposal/${widget.id}/send');
  }

  Future<void> _cancel(rust.ProposalInfo p) async {
    final cancel = await showAppMobileSheet<bool>(
      context: context,
      builder: (sheet) {
        final colors = sheet.colors;
        return MobileModalScaffold(
          title: 'Cancel this payment?',
          onClose: () => Navigator.of(sheet).pop(false),
          leading: AppIcon(
            AppIcons.warning,
            size: 20,
            color: colors.icon.destructive,
            patina: colors.icon.destructive,
          ),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Text(
                p.ready
                    ? 'Its funds become free for new payments. All signatures are in, so it can still be sent until it expires, unless a new payment uses the same funds.'
                    : 'Signers stop working on it and its funds become available for new '
                          'payments. This can\'t be undone.',
                style: AppTypography.bodyMedium.copyWith(
                  color: colors.text.accent,
                ),
              ),
              const SizedBox(height: AppSpacing.md),
              AppButton(
                expand: true,
                variant: AppButtonVariant.destructive,
                onPressed: () => Navigator.of(sheet).pop(true),
                child: const Text('Cancel payment'),
              ),
              const SizedBox(height: AppSpacing.xs),
              AppButton(
                expand: true,
                variant: AppButtonVariant.ghost,
                onPressed: () => Navigator.of(sheet).pop(false),
                child: const Text('Keep it'),
              ),
            ],
          ),
        );
      },
    );
    if (cancel != true || !mounted) throw const ActionCancelled();
    await ref.read(proposalsProvider.notifier).cancel(widget.id);
    // Leave from here, not from the button's onDone: cancelling refreshes the payment,
    // the page rebuilds as "Cancelled" without this button, and its onDone never ran.
    if (mounted) _leave();
  }

  /// The unlock prompt before an action; backing out cancels it quietly.
  Future<void> _unlock(String reason) async {
    if (!await confirmUnlock(context, ref, reason: reason)) {
      throw const ActionCancelled();
    }
  }

  Future<void> _invalidate(rust.ProposalInfo p) async {
    await _unlock('Propose moving these funds back to the vault');
    final id = await ref.read(proposalsProvider.notifier).invalidate(p.id);
    if (mounted) unawaited(context.push('/proposal/$id'));
  }

  /// Cancels a payment that can no longer be signed the one-tap way and proposes the
  /// same payment again, so everyone can approve it with one tap.
  Future<void> _repropose(rust.ProposalInfo p) async {
    await _unlock('Propose this payment again');
    final id = await ref.read(proposalsProvider.notifier).proposeAgain(p.id);
    if (mounted) context.pushReplacement('/proposal/$id');
  }

  void _proposeAgain(rust.ProposalInfo p) {
    context.push(
      '/send',
      extra: SendPrefill(
        payments: [
          for (final x in p.payments)
            rust.PaymentInput(
              address: x.address,
              amountZat: x.amountZat,
              memo: x.memo,
            ),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final vault = ref.watch(vaultProvider);
    final proposals = ref.watch(proposalsProvider);
    final p = proposals.byId(widget.id);
    final me = vault.myKeyHex;
    final height = vault.balance?.height;

    if (p == null) {
      return ZafeScreen(
        title: 'Payment',
        children: [
          const SizedBox(height: 120),
          Center(
            child: proposals.loaded
                ? Text(
                    'Proposal not found',
                    style: AppTypography.bodyMedium.copyWith(
                      color: colors.text.secondary,
                    ),
                  )
                : AppLoadingIcon(size: 24, color: colors.icon.muted),
          ),
        ],
      );
    }

    // A leftover send state (a failed attempt) means nothing once the payment is closed.
    final closed =
        p.stage == rust.ProposalStage.cancelled ||
        p.stage == rust.ProposalStage.rejected;
    final send = closed ? null : proposals.sends[p.id];
    final progress = send?.progress;
    final sending = send?.running ?? false;
    final sent =
        p.stage == rust.ProposalStage.sent ||
        progress?.stage == rust.SendStage.sent;

    // A payment that waits for this member's vote: a request with Approve and Decline
    // always in reach.
    final request =
        _holdRequest ||
        !sent &&
            send == null &&
            p.stage == rust.ProposalStage.open &&
            p.myVote == rust.MyVote.none &&
            !p.needsReapproval &&
            !p.needsNewProposal &&
            !proposalExpired(p, height);
    if (request) {
      final review = ref.watch(proposalReviewProvider(p.id));
      final verified = review.value?.verified == true;
      return ZafeScreen(
        title: 'Payment request',
        bottom: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            ActionButton(
              label: 'Approve',
              icon: AppIcons.check,
              doneLabel: 'Approved',
              enabled: verified || _holdRequest,
              onPressed: _approveRequest,
              onDone: _afterApprove,
            ),
            const SizedBox(height: AppSpacing.xs),
            ActionButton(
              label: 'Decline',
              variant: AppButtonVariant.ghost,
              doneLabel: 'Declined',
              onPressed: _declineRequest,
              onDone: () {
                if (mounted) _leave();
              },
            ),
          ],
        ),
        children: [
          PaymentRequestView(
            proposal: p,
            members: vault.summary!.members,
            me: me,
            names: ref.watch(signerNamesProvider),
            height: height,
          ),
        ],
      );
    }

    return PopScope(
      canPop: !sending,
      child: ZafeScreen(
        title: sending ? 'Sending...' : _screenTitle(p, height),
        showBack: !sending,
        children: [
          ProposalBody(
            proposal: p,
            members: vault.summary!.members,
            me: me,
            send: send,
            height: height,
            names: ref.watch(signerNamesProvider),
          ),
          const SizedBox(height: AppSpacing.lg),
          ..._actions(p, sent: sent, send: send, height: height),
        ],
      ),
    );
  }

  List<Widget> _approveAgain(TextStyle note) => [
    ActionButton(
      label: 'Approve again',
      icon: AppIcons.check,
      onPressed: _reapprove,
    ),
    const SizedBox(height: AppSpacing.xs),
    Text(
      'Approve again: your last signing round didn\'t finish, or this phone was restored.',
      textAlign: TextAlign.center,
      style: note,
    ),
  ];

  List<Widget> _actions(
    rust.ProposalInfo p, {
    required bool sent,
    required SendState? send,
    required int? height,
  }) {
    final colors = context.colors;
    if (sent) return const [];
    final note = AppTypography.bodySmall.copyWith(color: colors.text.secondary);

    final sending = send?.running ?? false;
    if (send != null) {
      final progress = send.progress;
      return [
        MobileSurfaceCard(
          cornerRadius: AppRadii.large,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  if (sending)
                    AppLoadingIcon(size: 20, color: colors.icon.accent),
                  if (!sending)
                    AppIcon(
                      AppIcons.warning,
                      size: 20,
                      color: colors.icon.destructive,
                      patina: colors.icon.destructive,
                    ),
                  const SizedBox(width: AppSpacing.xs),
                  Expanded(
                    child: Text(
                      sending
                          ? p.ready
                                ? 'Sending...'
                                : progress == null
                                ? 'Asking signers for signatures...'
                                : 'Signatures ${progress.received} of ${progress.needed}'
                          : 'Not sent yet',
                      style: AppTypography.labelLarge.copyWith(
                        color: colors.text.accent,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                  ),
                ],
              ),
              const SizedBox(height: AppSpacing.xs),
              Text(
                sending
                    ? p.ready
                          ? 'Every signature is in. Building the transaction; keep Zafe open.'
                          : 'The private transaction proof is built on this phone meanwhile. Keep Zafe open.'
                    : send.error!,
                style: AppTypography.bodySmall.copyWith(
                  color: sending
                      ? colors.text.secondary
                      : colors.text.destructive,
                ),
              ),
            ],
          ),
        ),
        if (!sending && p.needsReapproval) ...[
          const SizedBox(height: AppSpacing.sm),
          ..._approveAgain(note),
        ] else if (!sending) ...[
          const SizedBox(height: AppSpacing.sm),
          AppButton(
            expand: true,
            leading: const AppIcon(AppIcons.renew, size: 20),
            onPressed: _startSend,
            child: const Text('Try again'),
          ),
          if (send.timedOut && !p.ready) ...[
            const SizedBox(height: AppSpacing.s),
            AppButton(
              expand: true,
              variant: AppButtonVariant.ghost,
              onPressed: _startOver,
              child: const Text('Start over with other signers'),
            ),
            const SizedBox(height: AppSpacing.xs),
            Text(
              'Starts over without the signers who didn\'t answer. Those who signed approve again.',
              textAlign: TextAlign.center,
              style: note,
            ),
          ],
          // A payment that can't be sent (it fails the check, for example after a network
          // upgrade) must still be cancellable by its author, or it holds its notes until it
          // expires.
          if (p.isMine) ...[
            const SizedBox(height: AppSpacing.s),
            ActionButton(
              label: 'Cancel payment',
              variant: AppButtonVariant.ghost,
              onPressed: () => _cancel(p),
            ),
          ],
        ],
      ];
    }

    if (proposalExpired(p, height)) {
      return [
        Text(
          'This payment expired before it was sent. Nothing left the vault.',
          textAlign: TextAlign.center,
          style: note,
        ),
        const SizedBox(height: AppSpacing.sm),
        AppButton(
          expand: true,
          leading: const AppIcon(AppIcons.renew, size: 20),
          onPressed: () => _proposeAgain(p),
          child: const Text('Propose again'),
        ),
      ];
    }

    final footer = [
      if (height != null && p.expiryHeight > 0) ...[
        const SizedBox(height: AppSpacing.sm),
        Text(
          'Expires in ${expiresIn(p, height, nu7Height: nu7ActivationHeight())} if it isn\'t sent.',
          textAlign: TextAlign.center,
          style: note,
        ),
      ],
      if (p.isMine) ...[
        const SizedBox(height: AppSpacing.s),
        ActionButton(
          label: 'Cancel payment',
          variant: AppButtonVariant.ghost,
          onPressed: () => _cancel(p),
        ),
      ],
    ];

    if (p.needsReapproval) return [..._approveAgain(note), ...footer];

    // A signer's phone was restored or replaced while this payment was open, so its
    // signatures can't all be collected any more. Nothing was sent.
    if (p.needsNewProposal) {
      return [
        Text(
          p.isMine
              ? 'A signer\'s phone was replaced, so this payment can\'t be signed. Nothing was sent. Propose it again.'
              : 'A signer\'s phone was replaced, so this payment can\'t be signed. Nothing was sent. Ask its proposer to propose it again.',
          textAlign: TextAlign.center,
          style: note,
        ),
        if (p.isMine) ...[
          const SizedBox(height: AppSpacing.sm),
          ActionButton(
            label: 'Propose again',
            icon: AppIcons.renew,
            onPressed: () => _repropose(p),
          ),
        ],
        ...footer,
      ];
    }

    switch (p.stage) {
      case rust.ProposalStage.open:
        if (p.myVote != rust.MyVote.none) {
          final missing = p.threshold - p.approvals.length;
          final signed = p.oneTap && p.myVote == rust.MyVote.approved;
          return [
            Text(
              'You ${p.myVote == rust.MyVote.approved ? (signed ? 'approved and signed' : 'approved') : 'rejected'} this payment. '
              'Waiting for $missing more approval${missing == 1 ? '' : 's'}.',
              textAlign: TextAlign.center,
              style: AppTypography.bodySmall.copyWith(
                color: colors.text.secondary,
              ),
            ),
            ...footer,
          ];
        }
        // Not voted yet: the request view shows Approve and Decline instead.
        return footer;
      case rust.ProposalStage.approved when p.ready:
        return [
          AppButton(
            expand: true,
            leading: const AppIcon(AppIcons.plane, size: 20),
            onPressed: _startSend,
            child: const Text('Send now'),
          ),
          ...footer,
        ];
      case rust.ProposalStage.approved:
        return [
          AppButton(
            expand: true,
            leading: const AppIcon(AppIcons.plane, size: 20),
            onPressed: _startSend,
            child: Text(
              p.signingStarted ? 'Resume sending' : 'Collect signatures & send',
            ),
          ),
          ...footer,
        ];
      case rust.ProposalStage.cancelled when p.invalidatedBy != null:
        return [
          Text(
            'Being made unsendable: a proposal moves its funds back to the vault.',
            textAlign: TextAlign.center,
            style: note,
          ),
          const SizedBox(height: AppSpacing.s),
          AppButton(
            expand: true,
            variant: AppButtonVariant.secondary,
            onPressed: () => context.push('/proposal/${p.invalidatedBy}'),
            child: const Text('Open that proposal'),
          ),
        ];
      case rust.ProposalStage.cancelled when p.stillSendable:
        return [
          Text(
            'All signatures were in, so it can still be sent until it expires. Move its '
            'funds back to the vault to stop that (needs ${p.threshold} approvals).',
            textAlign: TextAlign.center,
            style: note,
          ),
          const SizedBox(height: AppSpacing.sm),
          ActionButton(
            label: 'Make it unsendable',
            icon: AppIcons.shieldKeyhole,
            onPressed: () => _invalidate(p),
          ),
        ];
      case rust.ProposalStage.rejected:
      case rust.ProposalStage.cancelled:
      case rust.ProposalStage.sent:
        return const [];
    }
  }
}

/// Short page titles (the nav title has room for about 20 characters).
String _screenTitle(rust.ProposalInfo p, int? height) =>
    proposalExpired(p, height)
    ? 'Expired'
    : switch (p.stage) {
        rust.ProposalStage.open =>
          p.myVote == rust.MyVote.none ? 'Review payment' : 'Awaiting votes',
        _ => proposalTitle(p),
      };
