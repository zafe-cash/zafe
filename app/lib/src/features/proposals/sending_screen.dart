import 'dart:async';

import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../core/feedback/payment_feedback.dart';
import '../../core/security/unlock_gate.dart';
import '../../core/widgets/mobile/mobile_transaction_progress_screen.dart';
import '../../providers/payment_sounds_provider.dart';
import '../../providers/proposals_provider.dart';
import '../../rust/api/proposals.dart' as rust;
import '../onboarding/onboarding_art.dart';

/// Transaction progress screen for sending a payment (spec: send status, 4.6).
/// The send itself belongs to the provider, so leaving this screen doesn't stop it.
class SendingScreen extends ConsumerStatefulWidget {
  const SendingScreen({super.key, required this.id});
  final String id;

  @override
  ConsumerState<SendingScreen> createState() => _SendingScreenState();
}

class _SendingScreenState extends ConsumerState<SendingScreen> {
  MobileTransactionProgressPhase? _announced;

  @override
  void initState() {
    super.initState();
    Future.microtask(() {
      final state = ref.read(proposalsProvider);
      final send = state.sends[widget.id];
      final p = state.byId(widget.id);
      final alreadySent = p?.stage == rust.ProposalStage.sent;
      if (!alreadySent &&
          !(send?.running ?? false) &&
          send?.progress?.stage != rust.SendStage.sent) {
        unawaited(ref.read(proposalsProvider.notifier).startSend(widget.id));
      }
    });
  }

  /// One sound and haptic per outcome.
  void _announce(MobileTransactionProgressPhase phase) {
    if (_announced == phase) return;
    _announced = phase;
    final sound = ref.read(paymentSoundsProvider);
    if (phase == MobileTransactionProgressPhase.succeeded) {
      unawaited(PaymentFeedback.play(PaymentMoment.sent, sound: sound));
    }
    if (phase == MobileTransactionProgressPhase.failed) {
      unawaited(PaymentFeedback.play(PaymentMoment.failed, sound: sound));
    }
  }

  @override
  Widget build(BuildContext context) {
    final state = ref.watch(proposalsProvider);
    final send = state.sends[widget.id];
    final p = state.byId(widget.id);
    final progress = send?.progress;
    final sent =
        p?.stage == rust.ProposalStage.sent ||
        progress?.stage == rust.SendStage.sent;
    final failed = !sent && send?.error != null;
    // Failed because this member's signature went into an unfinished round: it has to
    // approve again (on the payment page) before any new round can include it.
    final reapprove = failed && (p?.needsReapproval ?? false);
    // Chosen signers didn't answer: a new round can go to other approvers.
    final canStartOver =
        failed &&
        !reapprove &&
        (send?.timedOut ?? false) &&
        !(p?.ready ?? false);

    final phase = sent
        ? MobileTransactionProgressPhase.succeeded
        : failed
        ? MobileTransactionProgressPhase.failed
        : MobileTransactionProgressPhase.inProgress;
    if (phase != MobileTransactionProgressPhase.inProgress) {
      WidgetsBinding.instance.addPostFrameCallback((_) => _announce(phase));
    }

    final (title, body) = switch (phase) {
      MobileTransactionProgressPhase.succeeded => (
        'Sent!',
        'It will confirm on-chain shortly. Track it in Activity.',
      ),
      MobileTransactionProgressPhase.failed => (
        'Send failed',
        '${send!.error!} Nothing was sent; the vault\'s funds haven\'t moved.',
      ),
      _ => (
        'Sending...',
        (p?.ready ?? false) || progress == null || progress.needed == 0
            ? 'Building the private transaction and submitting it to the network...'
            : 'Signatures ${progress.received} of ${progress.needed}. The private proof is '
                  'built on this phone meanwhile.',
      ),
    };

    return MobileTransactionProgressScreen(
      phase: phase,
      title: title,
      body: body,
      // The send continues in the background, so leaving is always safe.
      canPop: true,
      bodyMaxWidth: 260,
      background: const IllustrationBackground('sent_slot'),
      primaryActionLabel: switch (phase) {
        MobileTransactionProgressPhase.succeeded => 'Done',
        MobileTransactionProgressPhase.failed when reapprove => 'Approve again',
        MobileTransactionProgressPhase.failed => 'Try again',
        _ => null,
      },
      onPrimaryAction: switch (phase) {
        MobileTransactionProgressPhase.succeeded => () => context.go('/home'),
        MobileTransactionProgressPhase.failed when reapprove => () {
          context.go('/home');
          context.push('/proposal/${widget.id}');
        },
        MobileTransactionProgressPhase.failed => () async {
          if (!await confirmUnlock(
            context,
            ref,
            reason: 'Unlock to send this payment',
          )) {
            return;
          }
          _announced = null;
          unawaited(ref.read(proposalsProvider.notifier).startSend(widget.id));
        },
        _ => null,
      },
      secondaryActionLabel: switch (phase) {
        MobileTransactionProgressPhase.failed when canStartOver =>
          'Start over with other signers',
        MobileTransactionProgressPhase.failed => 'Return home',
        MobileTransactionProgressPhase.inProgress =>
          'Keep sending in background',
        _ => null,
      },
      onSecondaryAction: switch (phase) {
        MobileTransactionProgressPhase.failed when canStartOver => () async {
          if (!await confirmUnlock(
            context,
            ref,
            reason: 'Unlock to start a new signing round',
          )) {
            return;
          }
          _announced = null;
          await ref.read(proposalsProvider.notifier).startOver(widget.id);
        },
        MobileTransactionProgressPhase.failed ||
        MobileTransactionProgressPhase.inProgress => () => context.go('/home'),
        _ => null,
      },
    );
  }
}
