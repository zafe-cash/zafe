import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../core/config/network_config.dart';
import '../../core/feedback/app_haptics.dart';
import '../../core/formatting/zec_amount.dart';
import '../../core/privacy/amount_display.dart';
import '../../rust/api/vault.dart' show Balance;
import '../../core/layout/mobile/mobile_top_nav.dart';
import '../../core/layout/mobile/mobile_top_scroll_fade.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/app_button.dart';
import '../../core/widgets/app_icon.dart';
import '../../core/widgets/app_tappable.dart';
import '../../notifications/notification_primer.dart';
import '../../notifications/vault_watch.dart';
import '../../providers/app_notices_provider.dart';
import '../../providers/endpoints_provider.dart';
import '../../services/live_vault_watch.dart';
import '../backup/backup_prompt_screen.dart' show backupStatusProvider;
import '../vaults/vault_emblem.dart';
import '../vaults/vault_switcher_sheet.dart';
import '../../providers/privacy_mode_provider.dart';
import '../../providers/vault_names_provider.dart';
import 'needs_you.dart';
import '../../core/errors/sync_failure.dart';
import '../../notifications/vault_updates.dart' show unapprovedSpendText;
import '../relay_move/relay_move_screen.dart' show RelayMoveCard;
import 'sync_status_screen.dart';
import '../../providers/proposals_provider.dart';
import '../../providers/received_provider.dart';
import '../../providers/tor_provider.dart';
import '../../providers/unapproved_spends_provider.dart';
import '../../core/privacy/privacy_mask.dart';
import '../proposals/activity_feed.dart';
import '../proposals/proposal_status.dart' show needsMyAction;
import '../../providers/other_vaults_waiting_provider.dart';
import '../../providers/vault_provider.dart';
import '../../providers/vault_relay.dart';
import '../../providers/zec_price_provider.dart';
import '../../services/app_update.dart';
import '../../core/widgets/line_icon.dart';

class HomeScreen extends ConsumerStatefulWidget {
  const HomeScreen({super.key});

  @override
  ConsumerState<HomeScreen> createState() => _HomeScreenState();
}

class _HomeScreenState extends ConsumerState<HomeScreen>
    with WidgetsBindingObserver {
  Timer? _poll;

  /// Other members' activity as it happens (relay long poll); the poll is the fallback.
  late final LiveVaultWatch _live = LiveVaultWatch(
    onRefresh: () => ref.read(proposalsProvider.notifier).refreshSoon(),
    onInbox: () =>
        ref.read(proposalsProvider.notifier).respondPolicy.inboxNews(),
    onPollIntervalChanged: (_) {
      ref.read(proposalsProvider.notifier).respondPolicy.live =
          _live.policy.live;
      if (_poll != null) _startPoll();
    },
  );

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    // Vaults from before per-vault relays keep the relay they use (the app-wide setting
    // is gone).
    Future.microtask(() => pinLegacyRelays(ref));
    Future.microtask(_refresh);
    _startPoll();
    // A vault exists from here on: notifications, background checks, push.
    unawaited(_startWatch());
    Future.microtask(_startLive);
    // Another vault or default relay: watch that one instead.
    ref.listenManual(
      vaultProvider.select((v) => v.activeId),
      (_, _) => _startLive(),
    );
    ref.listenManual(
      endpointsProvider.select((e) => e.relayUrl),
      (_, _) => _startLive(),
    );
    // The vault moved to another server: watch that one (the old watch kept reaching
    // the old server and marked the vault's server reachable).
    ref.listenManual(
      activeVaultRelayProvider.select((r) => (r?.url, r?.label)),
      (_, _) => _startLive(),
    );
  }

  /// (Re)starts the poll at the interval the live watch allows.
  void _startPoll() {
    _poll?.cancel();
    _poll = Timer.periodic(_live.pollInterval, (_) => _refresh());
  }

  /// Watches the active vault while the app is in the foreground.
  void _startLive() {
    if (!mounted) return;
    final vault = ref.read(vaultProvider);
    final foreground =
        WidgetsBinding.instance.lifecycleState != AppLifecycleState.paused &&
        WidgetsBinding.instance.lifecycleState != AppLifecycleState.hidden &&
        WidgetsBinding.instance.lifecycleState != AppLifecycleState.detached;
    if (!foreground || !vault.hasVault) {
      _live.stop();
      return;
    }
    _live.start(
      relayUrl: ref.read(endpointsProvider).relayUrl,
      seeds: vault.identity!,
      material: vault.material!,
    );
  }

  /// Background checks and pushes; the first time, the notifications primer before the
  /// system's permission prompt (never the prompt on its own, and never again after).
  Future<void> _startWatch() async {
    await startWatchWithPrimer(context);
    if (mounted) ref.invalidate(notificationsEnabledProvider);
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.paused) {
      // Stop polling and watching while in the background (the process may stay alive):
      // background checks and pushes take over, and only they may announce new activity.
      _live.stop();
      _poll?.cancel();
      _poll = null;
      unawaited(scheduleSoonCheck());
    }
    if (state == AppLifecycleState.resumed) {
      // The owner may have turned notifications on in the system settings.
      ref.invalidate(notificationsEnabledProvider);
      if (_poll == null) _startPoll();
      if (!_live.running) _startLive();
      unawaited(_refresh(force: true));
    }
  }

  /// The poll: proposals, then the wallet only if the chain tip moved (or proposals
  /// changed); `force` syncs regardless (resume, first open).
  Future<void> _refresh({bool force = false}) async {
    // Proposals first: answering signing requests needs the synced tip, which the
    // previous sync already stored.
    // Both read before the first await: Home can be gone when the refresh ends (a
    // vault removed or switched), and `ref` must not be used after that.
    final proposals = ref.read(proposalsProvider.notifier);
    final vault = ref.read(vaultProvider.notifier);
    await proposals.refresh();
    if (!mounted) return;
    await vault.sync(force: force);
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _live.stop();
    _poll?.cancel();
    _poll = null;
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final vault = ref.watch(vaultProvider);
    final summary = vault.summary;
    if (summary == null) return const SizedBox.shrink();
    final needsBackup = ref.watch(backupStatusProvider).value == false;
    final funded = (vault.balance?.totalZat ?? BigInt.zero) > BigInt.zero;
    final unapproved = ref.watch(unapprovedSpendsProvider);

    // A failure stays on screen while the next attempt runs, so the status doesn't
    // flicker between "Syncing..." and the error on every poll.
    final failure = currentSyncFailure(ref);
    // "Use Tor" while Tor isn't connected: nothing syncs, say why.
    final tor = ref.watch(torProvider);
    final torLabel = tor.homeLabel;
    final failing =
        tor.failed ||
        (torLabel == null && failure != null && !failure.isTransient);
    final String syncLabel;
    if (torLabel != null) {
      syncLabel = torLabel;
    } else if (failure != null) {
      // Just what failed; since when is on the Status screen ("Down since").
      syncLabel = failure.statusLabel;
    } else if (vault.syncing) {
      syncLabel = vault.firstSync ? 'First sync...' : 'Syncing...';
    } else if (vault.balance != null) {
      syncLabel = 'Synced';
    } else {
      syncLabel = 'Connecting...';
    }

    return Scaffold(
      backgroundColor: colors.background.window,
      body: SafeArea(
        bottom: false,
        child: Column(
          children: [
            MobileTopNav.account(
              accountName: ref.watch(activeVaultNameProvider) ?? summary.name,
              syncLabel: syncLabel,
              // Green when in sync, orange (text and bar) when something failed.
              syncLabelColor: failing
                  ? colors.sync.textError
                  : tor.enabled
                  ? colors.sync.torText
                  : colors.sync.text,
              // Tor's purple while Tor is on, so the status line shows it.
              syncIndicatorColor: failing
                  ? colors.sync.glowError
                  : tor.enabled
                  ? colors.sync.torGlow
                  : null,
              syncAnimated:
                  tor.connecting ||
                  (vault.syncing && (failure?.isTransient ?? true)),
              onSyncTap: () => context.push('/status'),
              onAccountTap: () => showVaultSwitcher(context),
              avatar: Stack(
                clipBehavior: Clip.none,
                children: [
                  VaultEmblem(vaultId: vault.activeId!),
                  // Another vault has a payment waiting for you (the switcher shows
                  // which).
                  if (ref.watch(otherVaultsWaitingProvider).value ?? false)
                    Positioned(
                      right: -2,
                      top: -2,
                      child: Semantics(
                        label: 'Another vault needs you',
                        child: Container(
                          width: 12,
                          height: 12,
                          decoration: BoxDecoration(
                            color: colors.background.brandStrong,
                            shape: BoxShape.circle,
                            border: Border.all(
                              color: colors.background.window,
                              width: 2,
                            ),
                          ),
                        ),
                      ),
                    ),
                ],
              ),
            ),
            Expanded(
              child: MobileTopScrollFade(
                // Pull down to check for news and sync now.
                child: RefreshIndicator(
                  color: colors.text.brand,
                  backgroundColor: colors.background.base,
                  onRefresh: () => _refresh(force: true),
                  child: ListView(
                    physics: const AlwaysScrollableScrollPhysics(),
                    padding: const EdgeInsets.fromLTRB(16, 16, 16, 64 + 48),
                    children: [
                      CountUp(
                        value: vault.balance?.totalZat,
                        builder: (shown) => BalanceCard(
                          totalZat: shown,
                          // The total includes money still confirming: what's pending
                          // shows in the activity rows, not on the card.
                          fiatText: vault.balance == null
                              ? null
                              : fiatText(
                                  vault.balance!.totalZat,
                                  ref.watch(zecPriceProvider),
                                ),
                          confirmingText: _confirmingText(
                            vault.balance,
                            hide: ref.watch(privacyModeProvider),
                          ),
                          hidden: ref.watch(privacyModeProvider),
                          threshold: summary.threshold,
                          members: summary.members.length,
                          onToggle: () {
                            AppHaptics.privacyToggle();
                            ref.read(privacyModeProvider.notifier).toggle();
                          },
                        ),
                      ),
                      const SizedBox(height: AppSpacing.s),
                      // Only once the balance is known: while it loads, a vault with
                      // history would flash the first-deposit prompt.
                      if (vault.balance?.totalZat == BigInt.zero)
                        AppButton(
                          expand: true,
                          onPressed: () => context.push('/receive'),
                          leading: const AppIcon(AppIcons.addNew, size: 20),
                          child: const Text(
                            'Receive your first $kZcashDefaultCurrencyTicker',
                          ),
                        )
                      else
                        Row(
                          children: [
                            Expanded(
                              child: AppButton(
                                expand: true,
                                onPressed: () => context.push('/send'),
                                leading: const AppIcon(
                                  AppIcons.plane,
                                  size: 20,
                                ),
                                child: const Text('New payment'),
                              ),
                            ),
                            const SizedBox(width: AppSpacing.xs),
                            Expanded(
                              child: AppButton(
                                expand: true,
                                variant: AppButtonVariant.secondary,
                                onPressed: () => context.push('/receive'),
                                leading: const AppIcon(
                                  AppIcons.arrowDownCircle,
                                  size: 20,
                                ),
                                child: const Text('Receive'),
                              ),
                            ),
                          ],
                        ),
                      const SizedBox(height: AppSpacing.md),
                      if (unapproved.spends.isNotEmpty) ...[
                        NoticeCard(
                          title: 'Money left without approval',
                          body: unapprovedSpendText(
                            fromCopyAt: fromUnixSeconds(unapproved.fromCopyAt),
                          ),
                          onTap: () => context.go('/activity'),
                        ),
                        const SizedBox(height: AppSpacing.md),
                      ],
                      // What the group is waiting on this member for, before any notice.
                      NeedsYou(height: vault.balance?.height),
                      // This phone's own move while it runs, or two moves found (a
                      // move another member started is a Needs you card).
                      const RelayMoveCard(),
                      // Things that happened on their own (a link expired, the server
                      // changed): tap to dismiss.
                      const AppNoticeCards(gap: AppSpacing.md),
                      // Once the vault holds money, a missing backup outranks every
                      // other notice (a lost phone could cost the group its funds).
                      // An empty vault leaves it to the Signers tab's backup card.
                      if (needsBackup && funded) ...[
                        NoticeCard(
                          title: 'Back up this vault',
                          body:
                              'It holds money, and your key share lives only on this '
                              'phone',
                          onTap: () => context.push('/export'),
                        ),
                        const SizedBox(height: AppSpacing.md),
                      ],
                      if (ref.watch(
                            proposalsProvider.select(
                              (p) => p.newerVersionEntries,
                            ),
                          ) >
                          0) ...[
                        const NoticeCard(
                          title: 'Update Zafe',
                          body:
                              'Other members use a newer version. Some vault activity '
                              'only shows after you update.',
                        ),
                        const SizedBox(height: AppSpacing.md),
                      ],
                      if (ref.watch(appUpdateProvider) ==
                          AppUpdateState.ready) ...[
                        // Never in the middle of sending a payment: the card says so
                        // and waits.
                        if (ref
                            .watch(proposalsProvider)
                            .sends
                            .values
                            .any((s) => s.running))
                          const NoticeCard(
                            title: 'Update ready',
                            body: 'Restarts after the payment is sent',
                            warning: false,
                          )
                        else
                          NoticeCard(
                            title: 'Update ready',
                            body: 'Restart Zafe to finish updating',
                            warning: false,
                            onTap: () => unawaited(
                              ref.read(appUpdateProvider.notifier).restart(),
                            ),
                          ),
                        const SizedBox(height: AppSpacing.md),
                      ],
                      if (ref.watch(notificationsEnabledProvider).value ==
                          false) ...[
                        NoticeCard(
                          title: 'Notifications are off',
                          body:
                              'You won\'t hear when a payment needs your approval',
                          onTap: () => enableNotifications(ref),
                        ),
                        const SizedBox(height: AppSpacing.md),
                      ],
                      _Payments(
                        proposals: ref.watch(proposalsProvider),
                        received: ref.watch(receivedProvider),
                        hideAmounts: ref.watch(privacyModeProvider),
                        height: vault.balance?.height,
                        funded: funded,
                      ),
                    ],
                  ),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// Rolls a balance up to its new value when it grows (money arrived): 900 ms, easing
/// out, from the old value. Decreases and the first value show at once, as does
/// everything with reduced motion.
class CountUp extends StatefulWidget {
  const CountUp({super.key, required this.value, required this.builder});
  final BigInt? value;
  final Widget Function(BigInt? shown) builder;

  @override
  State<CountUp> createState() => _CountUpState();
}

class _CountUpState extends State<CountUp> with SingleTickerProviderStateMixin {
  late final AnimationController _c = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 900),
  );
  BigInt? _from;

  @override
  void didUpdateWidget(CountUp old) {
    super.didUpdateWidget(old);
    final before = old.value, now = widget.value;
    if (before != null &&
        now != null &&
        now > before &&
        !MediaQuery.disableAnimationsOf(context)) {
      _from = _shown(old.value);
      _c.forward(from: 0);
    }
  }

  BigInt? _shown(BigInt? target) {
    final from = _from;
    if (target == null || from == null || !_c.isAnimating) return target;
    final t = Curves.easeOutCubic.transform(_c.value);
    final delta = (target - from).toDouble() * t;
    return from + BigInt.from(delta.round());
  }

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: _c,
    builder: (context, _) => widget.builder(_shown(widget.value)),
  );
}

/// Zafe's vault card: the balance on the vault card (`colors.vaultCard`: ink in dark
/// mode, pale verdigris in light mode) with safe-dial rings, and the approval rule as
/// signer dots along the bottom.
class BalanceCard extends StatelessWidget {
  const BalanceCard({
    super.key,
    required this.totalZat,
    required this.fiatText,
    this.confirmingText,
    required this.hidden,
    required this.onToggle,
    required this.threshold,
    required this.members,
  });

  /// The total in dollars ("$1,234.56"); null off mainnet or without a price.
  final String? fiatText;

  /// Money in the total that can't be spent yet ("0.001 ZEC confirming"); null when none.
  final String? confirmingText;
  final BigInt? totalZat;
  final bool hidden;
  final VoidCallback onToggle;
  final int threshold;
  final int members;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final total = totalZat;
    final amount = total == null
        ? '—'
        : ZecAmount.fromZatoshi(total).compactBalance.amountText;
    final card = colors.vaultCard;

    // Taller with large system text (amount, label and lines grow; it overflowed).
    final grow = (MediaQuery.textScalerOf(context).scale(16) - 16).clamp(0, 40);
    return Container(
      height: 216 + grow * 4,
      // Plain, like every other card: no dial, no border.
      decoration: BoxDecoration(
        color: card.background,
        borderRadius: BorderRadius.circular(AppRadii.large),
      ),
      clipBehavior: Clip.antiAlias,
      child: Stack(
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(20, 18, 16, 18),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    // The label takes what it needs and ellipsizes only when the
                    // card is too narrow (narrow phones, large system text).
                    AppIcon(
                      AppIcons.zcashCurrency,
                      size: 18,
                      color: card.textSecondary,
                    ),
                    const SizedBox(width: AppSpacing.xs),
                    Expanded(
                      child: Text(
                        'Vault balance',
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: AppTypography.labelMedium.copyWith(
                          color: card.textSecondary,
                          fontWeight: FontWeight.w500,
                        ),
                      ),
                    ),
                    AppTappable(
                      onTap: onToggle,
                      semanticsLabel: hidden ? 'Show balance' : 'Hide balance',
                      child: Container(
                        width: 32,
                        height: 32,
                        decoration: BoxDecoration(
                          color: card.chip,
                          borderRadius: BorderRadius.circular(AppRadii.xSmall),
                        ),
                        alignment: Alignment.center,
                        child: AppIcon(
                          hidden ? AppIcons.eyeClosed : AppIcons.eye,
                          size: 16,
                          color: card.text,
                        ),
                      ),
                    ),
                  ],
                ),
                const Spacer(),
                // Shrinks to fit, never cut: the exact balance must stay readable at
                // large text sizes.
                FittedBox(
                  fit: BoxFit.scaleDown,
                  alignment: Alignment.centerLeft,
                  child: Text.rich(
                    TextSpan(
                      children: [
                        TextSpan(
                          text: hidden ? fixedPrivacyMask() : amount,
                          style: AppTypography.hero.copyWith(color: card.text),
                        ),
                        TextSpan(
                          text: ' $kZcashDefaultCurrencyTicker',
                          style: AppTypography.heroTicker.copyWith(
                            color: card.ticker,
                          ),
                        ),
                      ],
                    ),
                    maxLines: 1,
                  ),
                ),
                const SizedBox(height: AppSpacing.xs),
                if (fiatText case final fiat?)
                  Text(
                    hidden ? '\$${fixedPrivacyMask()}' : fiat,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: AppTypography.bodyMedium.copyWith(
                      color: card.textSecondary,
                    ),
                  ),
                if (confirmingText case final confirming?)
                  Padding(
                    padding: const EdgeInsets.only(top: 2),
                    child: Text(
                      confirming,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: AppTypography.bodySmall.copyWith(
                        color: card.textSecondary,
                      ),
                    ),
                  ),
                const SizedBox(height: AppSpacing.sm),
                _ThresholdStrip(threshold: threshold, members: members),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

/// The approval rule as dots: [threshold] filled accent dots out of [members].
class _ThresholdStrip extends StatelessWidget {
  const _ThresholdStrip({required this.threshold, required this.members});
  final int threshold;
  final int members;

  @override
  Widget build(BuildContext context) {
    final card = context.colors.vaultCard;
    return Row(
      children: [
        for (var i = 0; i < members; i++)
          Container(
            width: 10,
            height: 10,
            margin: const EdgeInsets.only(right: 6),
            decoration: BoxDecoration(
              shape: BoxShape.circle,
              color: i < threshold ? card.accent : null,
              border: Border.all(
                color: i < threshold ? card.accent : card.emptyDot,
                width: 1.5,
              ),
            ),
          ),
        const SizedBox(width: 4),
        Flexible(
          child: Text(
            '$threshold of $members signers to send',
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: AppTypography.labelSmall.copyWith(color: card.textSecondary),
          ),
        ),
      ],
    );
  }
}

/// Rows shown on Home; everything else is one tap away under "See all".
const kRecentActivityLimit = 4;

/// "Recent activity": header with "See all", the newest [kRecentActivityLimit] rows 12
/// apart, and the empty state.
class _Payments extends StatelessWidget {
  const _Payments({
    required this.proposals,
    required this.received,
    required this.hideAmounts,
    this.height,
    this.funded = false,
  });
  final ProposalsState proposals;
  final ReceivedState received;
  final bool hideAmounts;
  final int? height;

  /// The vault holds money: the empty state suggests a payment, else receiving first.
  final bool funded;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    // Payments that need this member are listed above, under "Needs you".
    final items = mergeActivity([
      for (final p in proposals.items)
        if (!needsMyAction(p, height: height)) p,
    ], received.items).take(kRecentActivityLimit).toList();
    return Padding(
      padding: const EdgeInsets.symmetric(
        horizontal: AppSpacing.xs,
        vertical: AppSpacing.s,
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          ConstrainedBox(
            constraints: const BoxConstraints(minHeight: 24),
            child: Row(
              children: [
                Expanded(
                  child: Text(
                    'Recent activity',
                    style: AppTypography.labelLarge.copyWith(
                      color: colors.text.accent,
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                ),
                if (items.isNotEmpty)
                  AppTappable(
                    onTap: () => context.go('/activity'),
                    semanticsLabel: 'See all activity',
                    child: Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Text(
                          'See all',
                          style: AppTypography.labelLarge.copyWith(
                            color: colors.button.ghost.label,
                          ),
                        ),
                        AppIcon(
                          AppIcons.chevronForward,
                          size: 16,
                          color: colors.button.ghost.label,
                        ),
                      ],
                    ),
                  ),
              ],
            ),
          ),
          const SizedBox(height: AppSpacing.md),
          if (items.isEmpty)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: AppSpacing.md),
              child: Center(
                child: Column(
                  children: [
                    Text(
                      proposals.loaded
                          ? 'No activity yet'
                          : 'Loading activity...',
                      style: AppTypography.headlineSmall.copyWith(
                        color: colors.text.accent,
                      ),
                    ),
                    if (proposals.loaded) ...[
                      const SizedBox(height: AppSpacing.xxs),
                      Text(
                        funded
                            ? 'How about proposing\nyour first payment?'
                            : 'Receive funds first: share the\nvault address with the payer.',
                        textAlign: TextAlign.center,
                        style: AppTypography.bodyMedium.copyWith(
                          color: colors.text.secondary,
                        ),
                      ),
                    ],
                  ],
                ),
              ),
            ),
          for (var i = 0; i < items.length; i++) ...[
            if (i > 0) const SizedBox(height: AppSpacing.s),
            ActivityRow(
              item: items[i],
              hideAmount: hideAmounts,
              height: height,
            ),
          ],
        ],
      ),
    );
  }
}

/// The app's notices ([appNoticesProvider]) as cards, each followed by [gap]; tapping
/// one dismisses it.
class AppNoticeCards extends ConsumerWidget {
  const AppNoticeCards({super.key, this.gap = AppSpacing.s});
  final double gap;

  @override
  Widget build(BuildContext context, WidgetRef ref) => Column(
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      for (final n in ref.watch(appNoticesProvider)) ...[
        NoticeCard(
          title: n.title,
          body: n.body,
          warning: false,
          onTap: () => ref.read(appNoticesProvider.notifier).dismiss(n.id),
        ),
        SizedBox(height: gap),
      ],
    ],
  );
}

/// Until this device's copy of the vault is backed up (spec §12.2 "backup health").
/// Home entry card (backup reminder, update notice); a chevron when it opens a page.
class NoticeCard extends StatelessWidget {
  const NoticeCard({
    super.key,
    required this.title,
    required this.body,
    this.onTap,
    this.warning = true,
  });
  final String title;
  final String body;
  final VoidCallback? onTap;

  /// A warning (backup reminder) or good news (an update is ready).
  final bool warning;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    // Home entry card: ground, radius 24, 1.5px
    // white @ 7% border, icon + title/chevron + body.
    return AppTappable(
      onTap: onTap,
      semanticsLabel: title,
      child: Container(
        constraints: const BoxConstraints(minHeight: 77),
        padding: const EdgeInsets.symmetric(
          horizontal: AppSpacing.sm,
          vertical: AppSpacing.s,
        ),
        decoration: BoxDecoration(
          color: colors.background.ground,
          borderRadius: BorderRadius.circular(AppRadii.large),
        ),
        foregroundDecoration: BoxDecoration(
          borderRadius: BorderRadius.circular(AppRadii.large),
          border: Border.all(color: colors.border.subtleOpacity, width: 1.5),
        ),
        child: Padding(
          padding: const EdgeInsets.all(AppSpacing.xxs),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              LineIcon(
                size: 20,
                line: AppTypography.labelLarge,
                icon: warning
                    ? AppIcon(
                        AppIcons.warning,
                        size: 20,
                        color: colors.icon.warning,
                        patina: colors.icon.warning,
                      )
                    : AppIcon(
                        AppIcons.renew,
                        size: 20,
                        color: colors.icon.accent,
                      ),
              ),
              const SizedBox(width: AppSpacing.s),
              Expanded(
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Row(
                      children: [
                        Expanded(
                          child: Text(
                            title,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: AppTypography.labelLarge.copyWith(
                              color: colors.text.accent,
                            ),
                          ),
                        ),
                        if (onTap != null)
                          AppIcon(
                            AppIcons.chevronForward,
                            size: 20,
                            color: colors.icon.accent,
                          ),
                      ],
                    ),
                    const SizedBox(height: AppSpacing.xs),
                    Text(
                      body,
                      maxLines: 3,
                      style: AppTypography.bodyMedium.copyWith(
                        color: colors.text.secondary,
                        height: 17 / 16,
                        letterSpacing: -0.04,
                      ),
                    ),
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

/// "0.001 ZEC confirming" for money in the total that can't be spent yet (received funds
/// and the vault's own change wait for confirmations); null when there is none.
String? _confirmingText(Balance? balance, {required bool hide}) {
  if (balance == null) return null;
  final pending = balance.incomingPendingZat + balance.changePendingZat;
  if (pending <= BigInt.zero) return null;
  final amount = amountWithTicker(
    ZecAmount.fromZatoshi(pending).balance.amountText,
    hide: hide,
  );
  return '$amount confirming';
}
