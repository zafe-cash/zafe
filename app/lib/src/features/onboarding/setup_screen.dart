import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:pretty_qr_code/pretty_qr_code.dart';
import 'package:share_plus/share_plus.dart';

import '../../core/layout/mobile/app_mobile_sheet.dart';
import '../../core/layout/mobile/zafe_screen.dart';
import '../../core/formatting/member_label.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/copy_button.dart';
import '../../core/widgets/action_button.dart';
import '../../core/widgets/app_button.dart';
import '../../core/widgets/app_icon.dart';
import '../../core/widgets/dot_qr_shape.dart';
import '../../core/widgets/mobile/mobile_surface_card.dart';
import '../../providers/vault_provider.dart';
import '../../rust/api/error.dart' show ZafeError, ZafeErrorKind;
import '../../rust/api/vault.dart' as rust;
import 'invite_link.dart';
import '../../notifications/notification_primer.dart' show startWatchWithPrimer;
import 'key_shards_art.dart';
import 'onboarding_art.dart';

/// Vault setup: invite, members joining, comparing the safety words, and key generation
/// (spec §7.2). Every step also runs in background checks, so nobody has to keep this
/// screen open: membership locks itself once full, and "They match" is the one tap.
class SetupScreen extends ConsumerStatefulWidget {
  const SetupScreen({super.key});

  @override
  ConsumerState<SetupScreen> createState() => _SetupScreenState();
}

class _SetupScreenState extends ConsumerState<SetupScreen> {
  Timer? _poll;
  bool _leaving = false;
  bool _done = false;

  /// The relay no longer has this setup: the creator cancelled it.
  bool _gone = false;
  String? _error;

  /// The keys exist: the shards join, then the backup prompt opens.
  bool _keysReady = false;

  @override
  void initState() {
    super.initState();
    _refresh();
    _poll = Timer.periodic(const Duration(seconds: 2), (_) => _refresh());
    // Background checks and pushes carry the setup on while Zafe is closed, and say
    // when it's time to compare words; the primer comes first, once.
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) unawaited(startWatchWithPrimer(context));
      // The creator is looking at the invite: it gets another 3 days (a setup nobody
      // returns to lets its invite lapse).
      if (mounted) unawaited(ref.read(vaultProvider.notifier).renewInvite());
    });
  }

  @override
  void dispose() {
    _poll?.cancel();
    super.dispose();
  }

  /// The poll's step running now: a poll skips while one runs (over Tor a step can take
  /// longer than the poll interval). "They match" doesn't wait for it: two steps sending
  /// at once is safe (the relay's out-of-order refusal is retried in Rust).
  Future<void>? _step;
  DateTime? _lastPoll;

  /// While the words are on screen waiting for "They match", nothing changes but a
  /// cancelled setup: check less often.
  static const _wordsPoll = Duration(seconds: 10);

  Future<void> _refresh({String? confirmed}) async {
    if (confirmed != null) return _runStep(confirmed: confirmed);
    if (_step != null) return;
    final m = ref.read(vaultProvider).membership;
    final waitingForTap =
        m != null && m.sealed && m.safetyNumber != null && !m.started;
    final last = _lastPoll;
    if (waitingForTap &&
        last != null &&
        DateTime.now().difference(last) < _wordsPoll) {
      return;
    }
    _lastPoll = DateTime.now();
    final step = _runStep();
    _step = step;
    try {
      await step;
    } finally {
      if (identical(_step, step)) _step = null;
    }
  }

  Future<void> _runStep({String? confirmed}) async {
    if (_leaving || _done) return;
    try {
      final p = await ref
          .read(vaultProvider.notifier)
          .setupStep(confirmed: confirmed);
      if (p.material != null && mounted && !_done) {
        // The vault exists: its key share now lives only on this phone. On the key
        // screen the shards join first, then the backup prompt.
        _done = true;
        _poll?.cancel();
        if (ref.read(vaultProvider).membership?.started ?? false) {
          setState(() => _keysReady = true);
        } else {
          context.go('/backup-prompt');
        }
        return;
      }
      if (mounted && (_error != null || _gone)) {
        setState(() {
          _error = null;
          _gone = false;
        });
      }
    } catch (e) {
      if (!mounted || _leaving) return;
      if (e is ZafeError && e.kind == ZafeErrorKind.vaultNotOnRelay) {
        // The relay answered, and the setup isn't there any more.
        setState(() {
          _gone = true;
          _error = null;
        });
      } else if (confirmed != null) {
        rethrow;
      } else {
        setState(() => _error = 'Can\'t reach the relay. Retrying...');
      }
    }
  }

  /// Backs out of the setup before membership is locked. The creator's setup is deleted
  /// on the relay (the invite stops working) and the "New vault" form comes back filled
  /// in; a joiner just leaves.
  Future<void> _cancel({required bool creator, bool confirm = true}) async {
    if (_leaving) return;
    if (confirm) {
      final go = await showAppMobileSheet<bool>(
        context: context,
        builder: (sheet) {
          final colors = sheet.colors;
          return MobileModalScaffold(
            title: creator ? 'Cancel this setup?' : 'Leave this setup?',
            onClose: () => Navigator.of(sheet).pop(false),
            constrainBody: true,
            child: SingleChildScrollView(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Text(
                    creator
                        ? 'The invite stops working and those who joined see that you cancelled. You can set it up again.'
                        : 'You\'ll need a new invite to join again.',
                    style: AppTypography.bodyMedium.copyWith(
                      color: colors.text.accent,
                    ),
                  ),
                  const SizedBox(height: AppSpacing.md),
                  AppButton(
                    expand: true,
                    variant: AppButtonVariant.destructive,
                    onPressed: () => Navigator.of(sheet).pop(true),
                    child: Text(creator ? 'Cancel setup' : 'Leave'),
                  ),
                  const SizedBox(height: AppSpacing.xs),
                  AppButton(
                    expand: true,
                    variant: AppButtonVariant.ghost,
                    onPressed: () => Navigator.of(sheet).pop(false),
                    child: const Text('Keep waiting'),
                  ),
                ],
              ),
            ),
          );
        },
      );
      if (go != true || !mounted) return;
    }
    setState(() => _leaving = true);
    _poll?.cancel();
    final notifier = ref.read(vaultProvider.notifier);
    final result = await notifier.cancelSetup();
    if (!mounted) return;
    if (result.creator) {
      // Back to "New vault", filled in. With other vaults on this device, the onboarding
      // screens are shown the same way "Add vault" does.
      if (ref.read(vaultProvider).hasVault) notifier.beginAddVault();
      context.go('/create');
    } else {
      context.go(ref.read(vaultProvider).hasVault ? '/home' : '/welcome');
    }
  }

  /// After membership is locked: leaves the setup (a member never came back, the words
  /// differ, or the group gave up). The creator's cancels it for everyone.
  Future<void> _abandon() async {
    if (_leaving) return;
    final go = await showAppMobileSheet<bool>(
      context: context,
      builder: (sheet) {
        final colors = sheet.colors;
        return MobileModalScaffold(
          title: 'Abandon this setup?',
          onClose: () => Navigator.of(sheet).pop(false),
          constrainBody: true,
          child: SingleChildScrollView(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Text(
                  'No keys or funds exist yet. Start over with a new invite.',
                  style: AppTypography.bodyMedium.copyWith(
                    color: colors.text.accent,
                  ),
                ),
                const SizedBox(height: AppSpacing.md),
                AppButton(
                  expand: true,
                  variant: AppButtonVariant.destructive,
                  onPressed: () => Navigator.of(sheet).pop(true),
                  child: const Text('Abandon setup'),
                ),
                const SizedBox(height: AppSpacing.xs),
                AppButton(
                  expand: true,
                  variant: AppButtonVariant.ghost,
                  onPressed: () => Navigator.of(sheet).pop(false),
                  child: const Text('Keep waiting'),
                ),
              ],
            ),
          ),
        );
      },
    );
    if (go != true || !mounted) return;
    setState(() => _leaving = true);
    _poll?.cancel();
    await ref.read(vaultProvider.notifier).abandonSetup();
    if (!mounted) return;
    context.go(ref.read(vaultProvider).hasVault ? '/home' : '/welcome');
  }

  /// "They match": starts this member's part of key generation (the screen then shows
  /// the keys being made). A failure shows under the button.
  Future<void> _confirm(String words) => _refresh(confirmed: words);

  @override
  Widget build(BuildContext context) {
    final vault = ref.watch(vaultProvider);
    final info = vault.inviteInfo;
    final membership = vault.membership;
    if (info == null) return const SizedBox.shrink();

    final sealed = membership?.sealed ?? false;
    final isCreator = membership?.isCreator ?? false;

    final started = membership?.started ?? false;
    final words = membership?.safetyNumber;

    // After "They match": the key shards while the keys are made, here or in the
    // background (Zafe can be closed).
    if ((started && membership != null) || _keysReady) {
      return CreatingKeysScreen(
        ready: _keysReady,
        onReady: () {
          if (mounted) context.go('/backup-prompt');
        },
        waiting: membership == null || membership.keygenWaitingFor.isEmpty
            ? null
            : waitingText(membership.keygenWaitingFor, me: _myKeyHex(vault)),
        onAbandon: _leaving ? null : _abandon,
      );
    }

    Widget? bottom;
    if (_gone && !sealed) {
      bottom = AppButton(
        expand: true,
        onPressed: () => _cancel(creator: false, confirm: false),
        child: const Text('Leave setup'),
      );
    } else if (sealed && words != null && !started) {
      bottom = ActionButton(
        label: 'They match',
        icon: AppIcons.check,
        errorFallback: 'Couldn\'t start. Try again.',
        onPressed: () => _confirm(words),
      );
    }

    return PopScope(
      // Android's back button backs out of the setup too (it asks first); once
      // membership is locked there is no way back.
      canPop: false,
      onPopInvokedWithResult: (didPop, _) {
        if (!didPop && !sealed && !_gone) _cancel(creator: isCreator);
      },
      child: ZafeScreen(
        title: info.name,
        showBack: false,
        bottom: bottom,
        children: [
          if (_gone && !sealed) ...[
            _Hint(
              text: isCreator
                  ? 'This setup is no longer on the relay.'
                  : 'The creator cancelled this setup. Ask them for a new invite.',
            ),
            const SizedBox(height: AppSpacing.md),
          ],
          if (!sealed && isCreator) ...[
            _InviteCard(
              invite: vault.invite!,
              vaultName: info.name,
              // Sharing it again gives it another 3 days.
              onShare: () =>
                  unawaited(ref.read(vaultProvider.notifier).renewInvite()),
            ),
            const SizedBox(height: AppSpacing.md),
          ],
          _MembersCard(
            members: membership?.members ?? const [],
            expected: info.members,
            threshold: info.threshold,
            myKeyHex: _myKeyHex(vault),
            waiting: !sealed
                ? const []
                : started
                ? membership?.keygenWaitingFor ?? const []
                : membership?.waitingFor ?? const [],
          ),
          const SizedBox(height: AppSpacing.md),
          if (_error != null)
            Text(
              _error!,
              style: AppTypography.bodySmall.copyWith(
                color: context.colors.text.warning,
              ),
            ),
          if (!sealed && !_gone)
            _Hint(
              text: 'You can close Zafe. It tells you when it\'s your turn.',
            ),
          if (sealed && membership != null && !started)
            SetupSafetyWordsCard(
              words: words,
              waiting: membership.waitingFor.length,
            ),
          if (sealed) ...[
            const SizedBox(height: AppSpacing.sm),
            AppButton(
              expand: true,
              variant: AppButtonVariant.ghost,
              onPressed: _leaving ? null : _abandon,
              // Different words: nobody should make keys with this setup.
              child: Text(
                words != null ? 'They don\'t match' : 'Abandon setup',
              ),
            ),
          ],
          if (!sealed && !_gone) ...[
            const SizedBox(height: AppSpacing.sm),
            AppButton(
              expand: true,
              variant: AppButtonVariant.ghost,
              onPressed: _leaving ? null : () => _cancel(creator: isCreator),
              child: Text(isCreator ? 'Cancel setup' : 'Leave setup'),
            ),
          ],
        ],
      ),
    );
  }

  String? _myKeyHex(VaultState vault) {
    final id = vault.identity;
    return id == null ? null : rust.identityPublicKey(seeds: id);
  }
}

class _InviteCard extends StatelessWidget {
  const _InviteCard({
    required this.invite,
    required this.vaultName,
    required this.onShare,
  });
  final String invite;
  final String vaultName;
  final VoidCallback onShare;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    // The QR carries the link too, so a phone's own camera app can open Zafe with it.
    final link = inviteLink(invite);
    return MobileSurfaceCard(
      cornerRadius: AppRadii.large,
      padding: const EdgeInsets.fromLTRB(16, 24, 16, 16),
      child: Column(
        children: [
          Text(
            'Invite members',
            style: AppTypography.bodyLarge.copyWith(
              color: colors.text.accent,
              fontWeight: FontWeight.w600,
            ),
          ),
          const SizedBox(height: AppSpacing.xxs),
          Text(
            'Send it only to your co-signers, over a channel you trust. Anyone with it can join, for 3 days.',
            textAlign: TextAlign.center,
            style: AppTypography.bodySmall.copyWith(
              color: colors.text.secondary,
            ),
          ),
          const SizedBox(height: AppSpacing.sm),
          Container(
            width: 220,
            height: 220,
            padding: const EdgeInsets.all(AppSpacing.sm),
            decoration: BoxDecoration(
              color: colors.surface.qrCode,
              borderRadius: BorderRadius.circular(AppRadii.large),
            ),
            child: PrettyQrView.data(
              data: link,
              decoration: const PrettyQrDecoration(
                shape: DotQrShape(color: Color(0xFF0C0C0C)),
              ),
            ),
          ),
          const SizedBox(height: AppSpacing.sm),
          Row(
            children: [
              Expanded(
                child: AppButton(
                  variant: AppButtonVariant.secondary,
                  onPressed: () {
                    onShare();
                    SharePlus.instance.share(
                      ShareParams(
                        subject: 'Zafe vault invite',
                        text: 'Join "$vaultName" on Zafe: $link',
                      ),
                    );
                  },
                  leading: const AppIcon(AppIcons.share, size: 20),
                  child: const Text('Share link'),
                ),
              ),
              const SizedBox(width: AppSpacing.xs),
              Expanded(
                child: CopyButton(
                  label: 'Copy',
                  text: link,
                  sensitive: true,
                  variant: AppButtonVariant.secondary,
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }
}

class _MembersCard extends StatelessWidget {
  const _MembersCard({
    required this.members,
    required this.expected,
    required this.threshold,
    required this.myKeyHex,
    this.waiting = const [],
  });

  final List<String> members;
  final int expected;
  final int threshold;
  final String? myKeyHex;

  /// Members the setup is waiting for (their phone hasn't done its part yet).
  final List<String> waiting;

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
              Expanded(
                child: Text(
                  'Members',
                  style: AppTypography.labelLarge.copyWith(
                    color: colors.text.accent,
                    fontWeight: FontWeight.w600,
                  ),
                ),
              ),
              Text(
                '${members.length} of $expected joined',
                style: AppTypography.labelMedium.copyWith(
                  color: colors.text.secondary,
                ),
              ),
            ],
          ),
          const SizedBox(height: AppSpacing.xs),
          for (final m in members)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: AppSpacing.xs),
              child: Row(
                children: [
                  Container(
                    width: 32,
                    height: 32,
                    decoration: BoxDecoration(
                      color: colors.background.neutralSubtleOpacity,
                      shape: BoxShape.circle,
                    ),
                    alignment: Alignment.center,
                    child: AppIcon(
                      AppIcons.user,
                      size: 16,
                      color: colors.icon.regular,
                    ),
                  ),
                  const SizedBox(width: AppSpacing.s),
                  Text(
                    '${m.substring(0, 8)}...${m.substring(m.length - 6)}',
                    style: AppTypography.codeSmall.copyWith(
                      color: colors.text.primary,
                    ),
                  ),
                  if (m == myKeyHex) ...[
                    const SizedBox(width: AppSpacing.xs),
                    Text(
                      'You',
                      style: AppTypography.labelMedium.copyWith(
                        color: colors.text.secondary,
                      ),
                    ),
                  ],
                  if (waiting.contains(m)) ...[
                    const Spacer(),
                    Text(
                      'Waiting',
                      style: AppTypography.labelMedium.copyWith(
                        color: colors.text.muted,
                      ),
                    ),
                  ],
                ],
              ),
            ),
          for (var i = members.length; i < expected; i++)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: AppSpacing.xs),
              child: Row(
                children: [
                  Container(
                    width: 32,
                    height: 32,
                    decoration: BoxDecoration(
                      shape: BoxShape.circle,
                      border: Border.all(color: colors.border.regular),
                    ),
                  ),
                  const SizedBox(width: AppSpacing.s),
                  Text(
                    'Waiting...',
                    style: AppTypography.bodySmall.copyWith(
                      color: colors.text.muted,
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

class SetupSafetyWordsCard extends StatelessWidget {
  const SetupSafetyWordsCard({
    super.key,
    required this.words,
    required this.waiting,
  });

  /// The four words, or `null` while members' parts of the check are still arriving.
  final String? words;

  /// How many members haven't sent their part yet.
  final int waiting;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final words = this.words;
    final muted = AppTypography.bodySmall.copyWith(
      color: colors.text.darkCard.withValues(alpha: 0.7),
    );
    return Container(
      padding: const EdgeInsets.fromLTRB(16, 24, 16, 24),
      decoration: BoxDecoration(
        color: colors.background.darkCard,
        borderRadius: BorderRadius.circular(AppRadii.large),
        border: Border.all(color: const Color(0x12FFFFFF), width: 1.5),
      ),
      child: Column(
        children: [
          Row(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              AppIcon(
                AppIcons.shieldKeyhole,
                size: 20,
                color: colors.text.darkCard,
              ),
              const SizedBox(width: AppSpacing.xs),
              Text(
                'Safety words',
                style: AppTypography.labelMedium.copyWith(
                  color: colors.text.darkCard,
                ),
              ),
            ],
          ),
          const SizedBox(height: AppSpacing.sm),
          if (words == null) ...[
            AppIcon(AppIcons.loader, size: 24, color: colors.text.darkCard),
            const SizedBox(height: AppSpacing.sm),
            Text(
              waiting == 1
                  ? 'Waiting for 1 member.'
                  : 'Waiting for $waiting members.',
              textAlign: TextAlign.center,
              style: muted,
            ),
          ] else ...[
            Wrap(
              alignment: WrapAlignment.center,
              spacing: AppSpacing.md,
              runSpacing: AppSpacing.xs,
              children: [
                for (final w in words.split(' '))
                  Text(
                    w,
                    style: AppTypography.headlineLarge.copyWith(
                      color: colors.text.darkCard,
                    ),
                  ),
              ],
            ),
            const SizedBox(height: AppSpacing.sm),
            Text(
              'Compare them with every member on a call, in person or in a chat you trust. If anyone sees different words, stop.',
              textAlign: TextAlign.center,
              style: muted,
            ),
          ],
        ],
      ),
    );
  }
}

class _Hint extends StatelessWidget {
  const _Hint({required this.text});
  final String text;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return Row(
      children: [
        AppIcon(AppIcons.loader, size: 16, color: colors.icon.muted),
        const SizedBox(width: AppSpacing.xs),
        Expanded(
          child: Text(
            text,
            style: AppTypography.bodySmall.copyWith(
              color: colors.text.secondary,
            ),
          ),
        ),
      ],
    );
  }
}

/// Who key generation is waiting for (`missing`, key hex; this member left out).
String waitingText(List<String> missing, {String? me}) {
  final who = missing.where((k) => k != me).map(shortKey).toList();
  final names = switch (who.length) {
    0 => 'the other members',
    1 => who.single,
    _ => '${who.sublist(0, who.length - 1).join(', ')} and ${who.last}',
  };
  return 'Waiting for $names.';
}

/// The vault's keys being made (after this member confirmed the words): the key shards,
/// who it waits for, and a way out if someone never comes back.
class CreatingKeysScreen extends StatelessWidget {
  const CreatingKeysScreen({
    super.key,
    this.waiting,
    this.onAbandon,
    this.ready = false,
    this.onReady,
  });

  /// The keys exist: the shards join into one key, then [onReady].
  final bool ready;
  final VoidCallback? onReady;

  /// "Waiting for ..." when other members haven't done their part yet.
  final String? waiting;
  final VoidCallback? onAbandon;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return Scaffold(
      backgroundColor: colors.background.window,
      body: Stack(
        fit: StackFit.expand,
        children: [
          OnboardingHero(
            'key_shards',
            art: KeyShardsArt(assembled: ready, onAssembled: onReady),
          ),
          SafeArea(
            // Scrolls on small phones with large text; otherwise sits at the bottom.
            child: LayoutBuilder(
              builder: (context, box) => SingleChildScrollView(
                padding: const EdgeInsets.all(AppSpacing.sm),
                child: ConstrainedBox(
                  constraints: BoxConstraints(
                    minHeight: box.maxHeight - 2 * AppSpacing.sm,
                  ),
                  child: IntrinsicHeight(
                    child: Column(
                      mainAxisAlignment: MainAxisAlignment.end,
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        AppIcon(
                          ready ? AppIcons.check : AppIcons.loader,
                          size: 24,
                          color: ready ? colors.icon.value : colors.icon.accent,
                        ),
                        const SizedBox(height: AppSpacing.sm),
                        Text(
                          ready
                              ? 'Vault keys ready'
                              : 'Creating your vault keys...',
                          style: TextStyle(
                            fontFamily: 'Space Grotesk',
                            fontSize: 24,
                            height: 28 / 24,
                            color: colors.text.accent,
                          ),
                        ),
                        const SizedBox(height: AppSpacing.xs),
                        Text(
                          ready
                              ? 'Your share is on this phone.'
                              : waiting == null
                              ? 'You can close Zafe. It tells you when the vault is ready.'
                              : '$waiting You can close Zafe. It tells you when the vault is ready.',
                          style: AppTypography.bodyMedium.copyWith(
                            color: colors.text.secondary,
                          ),
                        ),
                        const SizedBox(height: AppSpacing.md),
                        AppButton(
                          expand: true,
                          variant: AppButtonVariant.ghost,
                          onPressed: ready ? null : onAbandon,
                          child: const Text('Abandon setup'),
                        ),
                      ],
                    ),
                  ),
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}
