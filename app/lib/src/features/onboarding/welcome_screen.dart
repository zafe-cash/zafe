import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../core/layout/app_column.dart';
import '../../core/layout/mobile/app_mobile_sheet.dart';
import '../../core/theme/app_theme.dart';
import '../home/home_screen.dart' show AppNoticeCards;
import '../../core/widgets/app_button.dart';
import '../../core/widgets/app_icon.dart';
import '../../core/widgets/app_tappable.dart';
import '../../providers/vault_provider.dart';
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
    void servers() => context.push('/zcash-server');
    Future<void> cancel() async {
      await ref.read(vaultProvider.notifier).cancelAddVault();
      if (context.mounted) context.go('/home');
    }

    // The title, the card and the buttons, below the hero art (`top` = half its height:
    // the title sits on its faded lower part) or beside it. On a phone the free height
    // goes above the title (buttons at the bottom, in reach); in a large window
    // (`centred`) the block sits in the middle of the space it has.
    Widget content({double top = 0, required bool centred}) => Padding(
      padding: EdgeInsets.fromLTRB(16, centred ? 12 : 0, 16, 12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          SizedBox(height: top),
          Spacer(flex: centred ? 1 : 3),
          Text(
            'Zafe',
            style: AppTypography.hero.copyWith(color: colors.text.accent),
          ),
          const SizedBox(height: AppSpacing.s),
          Text(
            'A shielded multisig for Zcash.',
            style: AppTypography.bodyMedium.copyWith(
              color: colors.text.secondary,
            ),
          ),
          if (centred)
            const SizedBox(height: AppSpacing.md)
          else
            const Spacer(),
          const _InfoCard(),
          const SizedBox(height: AppSpacing.md),
          const AppNoticeCards(),
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
          if (centred) const Spacer(),
        ],
      ),
    );

    // Fills its box when it fits; scrolls on short windows or with large system text
    // (it used to cut off the last buttons on a 360x640 phone). `inset` centres content
    // at most [maxWidth] wide (padding, which intrinsic sizing measures correctly).
    Widget scrolling(double maxWidth, Widget Function(double inset) child) =>
        LayoutBuilder(
          builder: (context, box) => SingleChildScrollView(
            child: ConstrainedBox(
              constraints: BoxConstraints(minHeight: box.maxHeight),
              child: IntrinsicHeight(
                child: child(math.max(0, (box.maxWidth - maxWidth) / 2)),
              ),
            ),
          ),
        );

    final art = WelcomeArt(replay: _doorTaps);
    // Phones, portrait windows and nearly square ones: the art on top, the content
    // below it, scrolling together (content scrolled over a fixed art put buttons on
    // the picture).
    // The scroll view stops above the navigation bar (as before the art scrolled).
    Widget stacked(Size size) => SafeArea(
      top: false,
      left: false,
      right: false,
      child: scrolling(
        kMaxContentWidth,
        (inset) => Stack(
          fit: StackFit.expand,
          children: [
            OnboardingHero('welcome_vault', art: art),
            SafeArea(
              child: Padding(
                padding: EdgeInsets.symmetric(horizontal: inset),
                child: content(
                  top: OnboardingHero.sizeIn(size).height * 0.5,
                  centred: isLargeWindow(context),
                ),
              ),
            ),
            WelcomeDoorTap(onTap: () => _doorTaps.value++),
          ],
        ),
      ),
    );
    // Landscape: the art fills the start half, the content is centred in the other.
    Widget beside() => Row(
      children: [
        Expanded(
          child: _ArtPanel(art: art, onTap: () => _doorTaps.value++),
        ),
        Expanded(
          child: SafeArea(
            left: false,
            child: scrolling(
              440,
              (inset) => Padding(
                padding: EdgeInsets.symmetric(horizontal: inset),
                child: content(centred: true),
              ),
            ),
          ),
        ),
      ],
    );

    final scaffold = Scaffold(
      backgroundColor: colors.background.window,
      // Clear of a hinge or a half-opened fold; otherwise the whole window.
      body: AppColumn(
        maxWidth: double.infinity,
        child: Builder(
          builder: (context) {
            final size = MediaQuery.sizeOf(context);
            final wide = size.width >= 600 && size.width >= size.height * 1.2;
            return Stack(
              fit: StackFit.expand,
              children: [
                wide ? beside() : stacked(size),
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
                  label: 'Zcash server',
                  onTap: servers,
                ),
              ],
            );
          },
        ),
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
      return SingleChildScrollView(
        // Scrolls once the sheet runs out of height (a short window).
        child: Padding(
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
        ),
      );
    },
  );
  if (route != null && context.mounted) context.push(route);
}

/// The Welcome art filling the start half of a landscape window: cropped to fill it,
/// fading into the window colour at the top (status bar), the bottom and toward the
/// content. Tapping it turns the door's wheel.
class _ArtPanel extends StatelessWidget {
  const _ArtPanel({required this.art, required this.onTap});
  final Widget art;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final window = context.colors.background.window;
    final clear = window.withValues(alpha: 0);
    Widget fade(Gradient gradient) => Positioned.fill(
      child: IgnorePointer(
        child: DecoratedBox(decoration: BoxDecoration(gradient: gradient)),
      ),
    );
    return ExcludeSemantics(
      child: GestureDetector(
        key: kHeroArtKey,
        behavior: HitTestBehavior.opaque,
        onTap: onTap,
        child: ClipRect(
          child: Stack(
            fit: StackFit.expand,
            children: [
              FittedBox(
                fit: BoxFit.cover,
                child: SizedBox(width: 1080, height: 1240, child: art),
              ),
              fade(
                LinearGradient(
                  begin: Alignment.topCenter,
                  end: Alignment.bottomCenter,
                  stops: const [0, 0.1, 0.8, 1],
                  colors: [
                    window.withValues(alpha: 0.85),
                    clear,
                    clear,
                    window,
                  ],
                ),
              ),
              fade(
                LinearGradient(
                  begin: AlignmentDirectional.centerStart,
                  end: AlignmentDirectional.centerEnd,
                  stops: const [0, 0.7, 1],
                  colors: [clear, clear, window],
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
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
