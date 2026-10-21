import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../core/layout/mobile/app_mobile_sheet.dart';
import '../../core/theme/app_theme.dart';
import '../home/home_screen.dart' show AppNoticeCards;
import '../../core/widgets/app_button.dart';
import '../../core/widgets/app_icon.dart';
import '../../core/widgets/app_tappable.dart';
import '../../providers/endpoints_provider.dart';
import '../../providers/vault_provider.dart';
import '../settings/endpoint_sheet.dart';
import 'onboarding_art.dart';
import 'welcome_art.dart';
import '../../core/widgets/line_icon.dart';

/// Taps on the Welcome door (one Welcome screen exists at a time): each replays its
/// unlock.
final _doorTaps = ValueNotifier<int>(0);

class WelcomeScreen extends ConsumerWidget {
  const WelcomeScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.colors;
    // Adding another vault: offer the way back to the one that was on screen.
    final adding = ref.watch(vaultProvider.select((v) => v.isAdding));
    // A build without a relay URL can't create or join until one is set here.
    final noRelay = ref.watch(
      endpointsProvider.select((e) => e.relayIsPlaceholder),
    );
    void servers() => showServerSettingsSheet(context, ref);
    Future<void> cancel() async {
      await ref.read(vaultProvider.notifier).cancelAddVault();
      if (context.mounted) context.go('/home');
    }

    // The text block over the hero art; the top gap keeps the title below the art's
    // busy middle (the art is full width, 1240/1080 as tall) on short screens.
    Widget content(double width) => Padding(
      padding: const EdgeInsets.fromLTRB(16, 0, 16, 12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          SizedBox(height: width * 1240 / 1080 * 0.5),
          const Spacer(flex: 3),
          Text(
            'Zafe',
            style: TextStyle(
              fontFamily: 'Space Grotesk',
              fontSize: 48,
              height: 1.1,
              letterSpacing: -1.35,
              color: colors.text.accent,
            ),
          ),
          const SizedBox(height: AppSpacing.s),
          Text(
            'A shielded multisig for Zcash.',
            style: AppTypography.bodyMedium.copyWith(
              color: colors.text.secondary,
            ),
          ),
          const Spacer(),
          const _InfoCard(),
          const SizedBox(height: AppSpacing.md),
          const AppNoticeCards(),
          if (noRelay) ...[
            _NoRelayNote(onTap: servers),
            const SizedBox(height: AppSpacing.s),
          ],
          AppButton(
            expand: true,
            onPressed: () => context.push('/create'),
            leading: const AppIcon(AppIcons.addNew, size: 20),
            child: const Text('Create a vault'),
          ),
          const SizedBox(height: AppSpacing.s),
          AppButton(
            expand: true,
            variant: AppButtonVariant.secondary,
            onPressed: () => context.push('/join'),
            leading: const AppIcon(AppIcons.link, size: 20),
            child: const Text('Join with an invite'),
          ),
          const SizedBox(height: AppSpacing.xs),
          AppButton(
            expand: true,
            variant: AppButtonVariant.ghost,
            onPressed: () => _alreadyInAVault(context),
            child: const Text("I'm already in a vault"),
          ),
        ],
      ),
    );

    final scaffold = Scaffold(
      backgroundColor: colors.background.window,
      body: Stack(
        fit: StackFit.expand,
        children: [
          OnboardingHero('welcome_vault', art: WelcomeArt(replay: _doorTaps)),
          SafeArea(
            // Fills the screen when it fits; scrolls on short phones or with large
            // system text (it used to cut off the last buttons on a 360x640 phone).
            child: LayoutBuilder(
              builder: (context, constraints) => SingleChildScrollView(
                child: ConstrainedBox(
                  constraints: BoxConstraints(minHeight: constraints.maxHeight),
                  child: IntrinsicHeight(child: content(constraints.maxWidth)),
                ),
              ),
            ),
          ),

          WelcomeDoorTap(onTap: () => _doorTaps.value++),
          if (adding)
            _CornerButton(
              alignment: Alignment.topLeft,
              icon: AppIcons.cross,
              label: 'Cancel adding a vault',
              onTap: cancel,
            ),
          _CornerButton(
            alignment: Alignment.topRight,
            icon: AppIcons.cog,
            label: 'Server settings',
            onTap: servers,
          ),
        ],
      ),
    );
    if (!adding) return scaffold;
    return PopScope(
      canPop: false,
      onPopInvokedWithResult: (didPop, _) {
        if (!didPop) cancel();
      },
      child: scaffold,
    );
  }
}

/// A member back on a new phone: restore from a backup if they have one, otherwise
/// co-signers move the seat (two paths whose names mean little to a newcomer, so the
/// question picks for them).
Future<void> _alreadyInAVault(BuildContext context) async {
  final route = await showAppMobileSheet<String>(
    context: context,
    builder: (sheetContext) {
      final colors = sheetContext.colors;
      void go(String route) => Navigator.of(sheetContext).pop(route);
      return Padding(
        padding: const EdgeInsets.fromLTRB(
          AppSpacing.sm,
          AppSpacing.md,
          AppSpacing.sm,
          AppSpacing.md,
        ),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text(
              'Do you have your backup?',
              textAlign: TextAlign.center,
              style: AppTypography.headlineSmall.copyWith(
                color: colors.text.accent,
              ),
            ),
            const SizedBox(height: AppSpacing.xs),
            Text(
              'Without one, your co-signers can move your seat to this phone.',
              textAlign: TextAlign.center,
              style: AppTypography.bodyMedium.copyWith(
                color: colors.text.secondary,
              ),
            ),
            const SizedBox(height: AppSpacing.md),
            AppButton(
              expand: true,
              onPressed: () => go('/restore'),
              child: const Text('Yes, restore it'),
            ),
            const SizedBox(height: AppSpacing.s),
            AppButton(
              expand: true,
              variant: AppButtonVariant.secondary,
              onPressed: () => go('/recover'),
              child: const Text('No, ask my co-signers'),
            ),
          ],
        ),
      );
    },
  );
  if (route != null && context.mounted) context.push(route);
}

/// A round button in a top corner, over the hero art.
class _CornerButton extends StatelessWidget {
  const _CornerButton({
    required this.alignment,
    required this.icon,
    required this.label,
    required this.onTap,
  });
  final Alignment alignment;
  final String icon;
  final String label;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return SafeArea(
      child: Align(
        alignment: alignment,
        child: Padding(
          padding: const EdgeInsets.all(AppSpacing.sm),
          child: AppTappable(
            onTap: onTap,
            semanticsLabel: label,
            child: Container(
              width: 40,
              height: 40,
              decoration: BoxDecoration(
                color: colors.background.ground,
                shape: BoxShape.circle,
              ),
              alignment: Alignment.center,
              child: AppIcon(icon, size: 20, color: colors.icon.accent),
            ),
          ),
        ),
      ),
    );
  }
}

/// This build has no relay URL, so creating or joining would fail.
class _NoRelayNote extends StatelessWidget {
  const _NoRelayNote({required this.onTap});
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return AppTappable(
      onTap: onTap,
      semanticsLabel: 'No relay set. Open server settings',
      child: Row(
        children: [
          AppIcon(
            AppIcons.warning,
            size: 16,
            color: colors.icon.warning,
            patina: colors.icon.warning,
          ),
          const SizedBox(width: AppSpacing.xs),
          Expanded(
            child: Text.rich(
              TextSpan(
                text: 'No relay set. ',
                children: [
                  TextSpan(
                    text: 'Add one',
                    style: TextStyle(
                      decoration: TextDecoration.underline,
                      decorationColor: colors.text.warning,
                    ),
                  ),
                ],
              ),
              style: AppTypography.bodySmall.copyWith(
                color: colors.text.warning,
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _InfoCard extends StatelessWidget {
  const _InfoCard();

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    Widget row(String icon, String title, String body) => Padding(
      padding: const EdgeInsets.symmetric(vertical: AppSpacing.xs),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          LineIcon(
            size: 20,
            line: AppTypography.labelLarge,
            icon: AppIcon(icon, size: 20, color: colors.text.darkCard),
          ),
          const SizedBox(width: AppSpacing.s),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  title,
                  style: AppTypography.labelLarge.copyWith(
                    color: colors.text.darkCard,
                  ),
                ),
                const SizedBox(height: AppSpacing.xxs),
                Text(
                  body,
                  style: AppTypography.bodySmall.copyWith(
                    color: colors.text.darkCard.withValues(alpha: 0.7),
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    );
    return Container(
      padding: const EdgeInsets.all(AppSpacing.sm),
      decoration: BoxDecoration(
        color: colors.background.darkCard,
        borderRadius: BorderRadius.circular(AppRadii.large),
        border: Border.all(color: const Color(0x12FFFFFF), width: 1.5),
      ),
      child: Column(
        children: [
          row(
            AppIcons.shieldKeyhole,
            'Private by default',
            'Balances, payments and memos stay shielded.',
          ),
          row(
            AppIcons.users,
            'Shared control',
            'Payments go out only when enough members sign.',
          ),
          row(
            AppIcons.key,
            'No single key',
            'Each member holds one share. The full key never exists.',
          ),
        ],
      ),
    );
  }
}
