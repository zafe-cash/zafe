import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:path_provider/path_provider.dart';
import 'package:share_plus/share_plus.dart';

import '../../core/layout/mobile/mobile_top_nav.dart';
import '../../core/layout/mobile/mobile_top_scroll_fade.dart';
import '../../core/errors/zafe_error_copy.dart';
import '../../core/security/unlock_gate.dart';
import '../../core/storage/member_names.dart';
import '../../core/storage/zafe_paths.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/app_button.dart';
import '../../core/widgets/app_icon.dart';
import '../../core/widgets/app_toast.dart';
import '../../providers/endpoints_provider.dart';
import '../../providers/privacy_mode_provider.dart';
import '../../providers/proposals_provider.dart';
import '../../providers/received_provider.dart';
import '../../providers/vault_provider.dart';
import '../onboarding/onboarding_art.dart';
import '../../rust/api/history.dart' as rust_history;
import 'activity_feed.dart';
import '../../core/storage/wallet_key.dart';

/// Exports the vault history as CSV (spec §11.3) through the share sheet. It lists every
/// payment and memo, so it asks for an unlock first.
Future<void> exportHistory(BuildContext context, WidgetRef ref) async {
  if (!await confirmUnlock(
    context,
    ref,
    reason: 'Unlock to export the vault history',
  )) {
    return;
  }
  final vault = ref.read(vaultProvider);
  try {
    final paths = await ZafePaths.get();
    final csv = await rust_history.exportHistoryCsv(
      relayUrl: ref.read(endpointsProvider).relayUrl,
      dbDir: paths.dbDir,
      dbKey: walletKeyFor(vault.identity!, vault.activeId!),
      seeds: vault.identity!,
      material: vault.material!,
      names: MemberNames.toSigners(
        MemberNames.merge(
          ref.read(proposalsProvider).sharedNames,
          await MemberNames.read(vault.activeId!),
        ),
      ),
    );
    final name = vault.summary!.name.replaceAll(
      RegExp(r'[^A-Za-z0-9_-]+'),
      '-',
    );
    final d = DateTime.now();
    String two(int n) => n.toString().padLeft(2, '0');
    final fileName =
        'Zafe-$name-history-${d.year}-${two(d.month)}-${two(d.day)}.csv';
    final file = File('${(await getTemporaryDirectory()).path}/$fileName');
    await file.writeAsString(csv, flush: true);
    await SharePlus.instance.share(
      ShareParams(files: [XFile(file.path)], fileNameOverrides: [fileName]),
    );
    try {
      await file.delete();
    } catch (_) {}
  } catch (e) {
    debugPrint('history export failed: ${describeError(e)}');
    if (context.mounted) {
      showAppToast(
        context,
        zafeErrorMessage(
          e,
          fallback: 'Couldn\'t export the history. Try again.',
        ),
        iconName: AppIcons.warningCircle,
        tone: AppToastTone.destructive,
      );
    }
  }
}

/// Every payment in the vault, sent and received, grouped into sections: "This week", then
/// month and year, then "Earlier"; one card per section.
class ActivityScreen extends ConsumerWidget {
  const ActivityScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.colors;
    final proposals = ref.watch(proposalsProvider);
    final received = ref.watch(receivedProvider);
    final hide = ref.watch(privacyModeProvider);
    final height = ref.watch(vaultProvider.select((v) => v.balance?.height));
    final sections = activitySections(
      mergeActivity(proposals.items, received.items),
    );
    return Scaffold(
      backgroundColor: colors.background.window,
      body: AppToastHost(
        child: SafeArea(
          bottom: false,
          child: Column(
            children: [
              MobileTopNav.back(
                title: 'Activity',
                trailing: AppButton(
                  variant: AppButtonVariant.secondary,
                  size: AppButtonSize.small,
                  leading: const AppIcon(AppIcons.share, size: 16),
                  onPressed: () => exportHistory(context, ref),
                  child: const Text('Export'),
                ),
              ),
              Expanded(
                child: MobileTopScrollFade(
                  child: ListView(
                    padding: const EdgeInsets.fromLTRB(4, 12, 4, 112),
                    children: [
                      if (sections.isEmpty && proposals.loaded)
                        const _EmptyCard()
                      else if (sections.isEmpty)
                        _MessageCard(
                          text: proposals.error != null
                              ? 'Couldn\'t load activity. Try again in a moment.'
                              : 'Loading activity...',
                          error: proposals.error != null,
                        ),
                      for (final (title, rows) in sections) ...[
                        _SectionCard(
                          title: title,
                          children: [
                            for (final item in rows)
                              ActivityRow(
                                item: item,
                                hideAmount: hide,
                                height: height,
                              ),
                          ],
                        ),
                        const SizedBox(height: AppSpacing.md),
                      ],
                    ],
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

/// Newest first (`mergeActivity`'s order), grouped by section title. Payments still
/// being mined belong to "This week".
List<(String, List<ActivityItem>)> activitySections(
  List<ActivityItem> items, {
  DateTime? now,
}) {
  final out = <(String, List<ActivityItem>)>[];
  for (final item in items) {
    final pending = item is ReceivedActivity && item.pending;
    final title = sectionTitle(
      pending ? (now ?? DateTime.now()) : item.time,
      now: now,
    );
    if (out.isEmpty || out.last.$1 != title) out.add((title, []));
    out.last.$2.add(item);
  }
  return out;
}

String sectionTitle(DateTime? timestamp, {DateTime? now}) {
  if (timestamp == null) return 'Earlier';
  final local = timestamp.toLocal();
  final today = now ?? DateTime.now();
  final weekStart = DateTime(
    today.year,
    today.month,
    today.day,
  ).subtract(Duration(days: today.weekday - DateTime.monday));
  if (!local.isBefore(weekStart) &&
      local.isBefore(weekStart.add(const Duration(days: 7)))) {
    return 'This week';
  }
  const months = [
    'January', 'February', 'March', 'April', 'May', 'June', //
    'July', 'August', 'September', 'October', 'November', 'December',
  ];
  return '${months[local.month - 1]} ${local.year}';
}

class _SectionCard extends StatelessWidget {
  const _SectionCard({required this.title, required this.children});
  final String title;
  final List<Widget> children;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return Container(
      padding: const EdgeInsets.fromLTRB(16, 24, 16, 24),
      decoration: BoxDecoration(
        color: colors.background.ground,
        borderRadius: BorderRadius.circular(AppRadii.large),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SizedBox(
            height: 24,
            child: Text(
              title,
              style: AppTypography.labelLarge.copyWith(
                color: colors.text.secondary,
                fontWeight: FontWeight.w600,
              ),
            ),
          ),
          const SizedBox(height: AppSpacing.s),
          for (var i = 0; i < children.length; i++) ...[
            if (i > 0) const SizedBox(height: AppSpacing.s),
            children[i],
          ],
        ],
      ),
    );
  }
}

class _MessageCard extends StatelessWidget {
  const _MessageCard({required this.text, this.error = false});
  final String text;
  final bool error;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return Container(
      height: 160,
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: colors.background.ground,
        borderRadius: BorderRadius.circular(AppRadii.large),
      ),
      child: Text(
        text,
        style: AppTypography.labelLarge.copyWith(
          color: error ? colors.text.destructive : colors.text.secondary,
        ),
      ),
    );
  }
}

/// Empty state: a quiet shelf with a blank ledger, and the message below it.
class _EmptyCard extends StatelessWidget {
  const _EmptyCard();

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return Container(
      padding: const EdgeInsets.fromLTRB(8, 8, 8, 24),
      decoration: BoxDecoration(
        color: colors.background.ground,
        borderRadius: BorderRadius.circular(AppRadii.large),
      ),
      child: Column(
        children: [
          const OnboardingBanner('empty_ledger', aspectRatio: 1080 / 440),
          const SizedBox(height: AppSpacing.md),
          Text(
            'No activity yet',
            style: AppTypography.labelLarge.copyWith(
              color: colors.text.secondary,
            ),
          ),
        ],
      ),
    );
  }
}
