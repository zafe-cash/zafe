import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../providers/device_lock_provider.dart';
import '../../providers/vault_provider.dart';
import '../theme/app_theme.dart';
import '../widgets/app_button.dart';
import '../widgets/app_icon.dart';
import 'app_lock.dart';
import 'device_auth.dart';

/// Covers the whole app (routes, sheets, toasts) with [LockScreen] while
/// `appLockedProvider` is set, and sets it when the app comes back after the "Lock
/// app" delay. What's underneath stays mounted, so unlocking returns to the same
/// screen, but gets no input, focus or semantics. Used as `MaterialApp.builder`.
class AppLockGate extends ConsumerStatefulWidget {
  const AppLockGate({super.key, required this.child});
  final Widget child;

  @override
  ConsumerState<AppLockGate> createState() => _AppLockGateState();
}

class _AppLockGateState extends ConsumerState<AppLockGate>
    with WidgetsBindingObserver {
  /// When the app last left the foreground, and whether an unlock prompt was up then.
  DateTime? _awaySince;
  bool _awayDuringPrompt = false;

  bool _prompting = false;
  String? _message;

  /// The app isn't in front (app switcher, notification shade, a system dialog): hide
  /// the screen so the recent-apps card and anyone glancing at it see no balances.
  bool _covered = false;

  void _cover(bool covered) {
    if (covered != _covered && mounted) setState(() => _covered = covered);
  }

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    if (ref.read(appLockedProvider)) {
      WidgetsBinding.instance.addPostFrameCallback((_) => unawaited(_unlock()));
    }
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    _cover(state != AppLifecycleState.resumed);
    switch (state) {
      case AppLifecycleState.hidden || AppLifecycleState.paused:
        if (_awaySince == null) {
          _awaySince = DateTime.now();
          _awayDuringPrompt = DeviceAuth.prompting;
        }
      case AppLifecycleState.resumed:
        final since = _awaySince;
        _awaySince = null;
        if (since == null) return;
        final lock = lockOnResume(
          delay: ref.read(appLockDelayProvider),
          hasVaults: ref.read(vaultProvider).vaults.isNotEmpty,
          away: DateTime.now().difference(since),
          duringPrompt: _awayDuringPrompt,
        );
        if (lock) _lock();
        // Back from our own prompt (cancelled): wait for a tap instead of asking again.
        if (ref.read(appLockedProvider) && !_awayDuringPrompt) {
          unawaited(_unlock());
        }
      case AppLifecycleState.inactive || AppLifecycleState.detached:
        break;
    }
  }

  void _lock() {
    FocusManager.instance.primaryFocus?.unfocus();
    setState(() => _message = null);
    ref.read(appLockedProvider.notifier).lock();
  }

  Future<void> _unlock() async {
    if (_prompting || !mounted) return;
    setState(() {
      _prompting = true;
      _message = null;
    });
    final outcome = await ref
        .read(deviceAuthProvider)
        .unlock(required: true, reason: 'Unlock Zafe');
    if (!mounted) return;
    setState(() {
      _prompting = false;
      _message = switch (outcome) {
        UnlockOutcome.unlocked ||
        UnlockOutcome.notRequired ||
        // Nothing to unlock with: Settings warns about it permanently.
        UnlockOutcome.noScreenLock => null,
        UnlockOutcome.cancelled => null,
        UnlockOutcome.lockedOut => 'Too many attempts. Try again later',
        UnlockOutcome.failed => 'Couldn\'t confirm it\'s you',
      };
    });
    if (outcome.allowed) ref.read(appLockedProvider.notifier).unlock();
  }

  @override
  Widget build(BuildContext context) {
    final locked = ref.watch(appLockedProvider);
    return Stack(
      fit: StackFit.expand,
      children: [
        ExcludeFocus(
          excluding: locked,
          child: ExcludeSemantics(
            excluding: locked,
            child: IgnorePointer(ignoring: locked, child: widget.child),
          ),
        ),
        if (_covered && !locked) const PrivacyCover(),
        if (locked)
          LockScreen(
            busy: _prompting,
            message: _message,
            onUnlock: () => unawaited(_unlock()),
          ),
      ],
    );
  }
}

/// Shown while the app isn't in front: the window colour and the lock tile, nothing
/// else. Android 13+ shows the running app live in the app switcher, which no window
/// flag short of blocking screenshots hides.
class PrivacyCover extends StatelessWidget {
  const PrivacyCover({super.key});

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return ColoredBox(
      color: colors.background.window,
      child: Center(child: _LockTile(colors: colors)),
    );
  }
}

class _LockTile extends StatelessWidget {
  const _LockTile({required this.colors});
  final AppColors colors;

  @override
  Widget build(BuildContext context) => Container(
    width: 72,
    height: 72,
    decoration: BoxDecoration(
      color: colors.background.brandSubtle,
      borderRadius: BorderRadius.circular(AppRadii.large),
    ),
    alignment: Alignment.center,
    child: AppIcon(AppIcons.lock, size: 32, color: colors.icon.brand),
  );
}

/// "Zafe is locked": shown over everything until the owner unlocks. Plain data, so it
/// can be rendered without a device.
class LockScreen extends StatelessWidget {
  const LockScreen({
    super.key,
    required this.onUnlock,
    this.busy = false,
    this.message,
  });

  final VoidCallback onUnlock;
  final bool busy;

  /// Why the last attempt didn't unlock (null when there's nothing to say).
  final String? message;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return Material(
      color: colors.background.window,
      child: SafeArea(
        child: Padding(
          padding: const EdgeInsets.fromLTRB(16, 0, 16, 12),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              const Spacer(),
              Center(child: _LockTile(colors: colors)),
              const SizedBox(height: AppSpacing.md),
              Text(
                'Zafe is locked',
                textAlign: TextAlign.center,
                style: TextStyle(
                  fontFamily: 'Space Grotesk',
                  fontSize: 28,
                  height: 1.2,
                  color: colors.text.accent,
                ),
              ),
              if (message != null) ...[
                const SizedBox(height: AppSpacing.s),
                Text(
                  message!,
                  textAlign: TextAlign.center,
                  style: AppTypography.bodyMedium.copyWith(
                    color: colors.text.destructive,
                  ),
                ),
              ],
              const Spacer(),
              AppButton(
                expand: true,
                leading: const AppIcon(AppIcons.unlock, size: 20),
                onPressed: busy ? null : onUnlock,
                child: const Text('Unlock'),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
