import 'package:flutter/widgets.dart';

import '../../core/formatting/member_label.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/animated_keyed_column.dart';
import '../../core/widgets/app_icon.dart';
import '../../core/widgets/app_loading_icon.dart';
import '../../core/widgets/mobile/mobile_surface_card.dart';
import '../../rust/api/proposals.dart' as rust;
import 'proposal_status.dart';

enum StepState { done, current, failed }

/// One line of a payment's history.
class TimelineStep {
  const TimelineStep(this.state, this.title, [this.detail]);
  final StepState state;
  final String title;
  final String? detail;
}

/// What happened to a payment, oldest first, ending with where it stands now (who it
/// waits for, ready, sending, sent, or why it won't be). Votes carry no time, so only
/// the proposal does.
List<TimelineStep> timelineSteps(
  rust.ProposalInfo p, {
  required List<String> members,
  required String? me,
  required Map<String, String> names,
  int? height,
  bool sending = false,
}) {
  String who(String k) => memberLabel(k, me: me, names: names);
  final steps = <TimelineStep>[
    TimelineStep(
      StepState.done,
      'Proposed by ${who(p.author)}',
      p.createdAt > BigInt.zero ? formatTimestamp(p.createdAt) : null,
    ),
    for (final a in p.approvals)
      TimelineStep(StepState.done, 'Approved by ${who(a)}'),
    for (final r in p.rejections)
      TimelineStep(StepState.failed, 'Declined by ${who(r)}'),
  ];
  if (proposalExpired(p, height)) {
    steps.add(
      const TimelineStep(StepState.failed, 'Expired', 'Nothing left the vault'),
    );
    return steps;
  }
  switch (p.stage) {
    case rust.ProposalStage.sent:
      steps.add(const TimelineStep(StepState.done, 'Sent'));
    case rust.ProposalStage.cancelled:
      steps.add(const TimelineStep(StepState.failed, 'Cancelled'));
    case rust.ProposalStage.rejected:
      steps.add(const TimelineStep(StepState.failed, 'Won\'t be sent'));
    case rust.ProposalStage.approved:
      steps.add(
        TimelineStep(StepState.current, sending ? 'Sending' : 'Ready to send'),
      );
    case rust.ProposalStage.open:
      final need = p.threshold - p.approvals.length;
      final waiting = [
        for (final m in members)
          if (!p.approvals.contains(m) && !p.rejections.contains(m)) who(m),
      ];
      steps.add(
        TimelineStep(
          StepState.current,
          need == 1
              ? 'Waiting for 1 more approval'
              : 'Waiting for $need more approvals',
          waiting.isEmpty ? null : _either(waiting),
        ),
      );
  }
  return steps;
}

/// "Bob", "Bob or Carol", "Bob, Carol or Dan".
String _either(List<String> names) => names.length == 1
    ? names.single
    : '${names.sublist(0, names.length - 1).join(', ')} or ${names.last}';

/// The history as a vertical line of steps: a filled node for what's done, a ring
/// (or a spinner while sending) for where it stands, a cross for a decline or an end.
class ProposalTimeline extends StatelessWidget {
  const ProposalTimeline({super.key, required this.steps});
  final List<TimelineStep> steps;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    Widget node(TimelineStep s) => SizedBox(
      width: 20,
      height: 20,
      child: switch (s.state) {
        StepState.done => Container(
          decoration: BoxDecoration(
            color: s.title == 'Sent'
                ? colors.icon.value
                : colors.background.brandStrong,
            shape: BoxShape.circle,
          ),
          child: Center(
            child: AppIcon(
              AppIcons.check,
              size: 12,
              color: colors.text.inverse,
            ),
          ),
        ),
        StepState.current when s.title == 'Sending' => AppLoadingIcon(
          size: 20,
          color: colors.icon.brand,
        ),
        StepState.current => Container(
          decoration: BoxDecoration(
            shape: BoxShape.circle,
            border: Border.all(color: colors.icon.brand, width: 2),
          ),
        ),
        StepState.failed => Container(
          decoration: BoxDecoration(
            color: colors.background.destructiveAlpha,
            shape: BoxShape.circle,
          ),
          child: Center(
            child: AppIcon(
              AppIcons.cross,
              size: 12,
              color: colors.icon.destructive,
            ),
          ),
        ),
      },
    );
    return MobileSurfaceCard(
      cornerRadius: AppRadii.large,
      // A vote that arrives while the page is open grows in as a new step.
      child: AnimatedKeyedColumn(
        children: [
          for (var i = 0; i < steps.length; i++)
            IntrinsicHeight(
              key: ValueKey('${steps[i].state}:${steps[i].title}'),
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Column(
                    children: [
                      node(steps[i]),
                      if (i < steps.length - 1)
                        Expanded(
                          child: Container(
                            width: 2,
                            margin: const EdgeInsets.symmetric(vertical: 2),
                            color: steps[i + 1].state == StepState.done
                                ? colors.background.brandStrong.withValues(
                                    alpha: 0.5,
                                  )
                                : colors.border.regular,
                          ),
                        ),
                    ],
                  ),
                  const SizedBox(width: AppSpacing.s),
                  Expanded(
                    child: Padding(
                      padding: EdgeInsets.only(
                        bottom: i < steps.length - 1 ? AppSpacing.sm : 0,
                      ),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            steps[i].title,
                            style: AppTypography.labelLarge.copyWith(
                              color: steps[i].state == StepState.failed
                                  ? colors.text.destructive
                                  : colors.text.accent,
                              fontWeight: steps[i].state == StepState.current
                                  ? FontWeight.w600
                                  : FontWeight.w400,
                            ),
                          ),
                          if (steps[i].detail != null)
                            Text(
                              steps[i].detail!,
                              style: AppTypography.bodySmall.copyWith(
                                color: colors.text.secondary,
                              ),
                            ),
                        ],
                      ),
                    ),
                  ),
                ],
              ),
            ),
        ],
      ),
    );
  }
}
