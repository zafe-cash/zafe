import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../providers/device_lock_provider.dart';
import '../../providers/vault_provider.dart';
import '../layout/app_column.dart';
import '../theme/app_theme.dart';
import '../widgets/app_button.dart';
import '../widgets/app_icon.dart';
import 'app_lock.dart';
import 'device_auth.dart';

class AppLockGate extends ConsumerStatefulWidget {
  const AppLockGate({super.key, required this.child});
  final Widget child;

  @override
  ConsumerState<AppLockGate> createState() => _AppLockGateState();
}

class _AppLockGateState extends ConsumerState<AppLockGate>
    with WidgetsBindingObserver {
  DateTime? _awaySince;
  bool _awayDuringPrompt = false;

  bool _prompting = false;
  String? _message;

  bool _covered = false;

  bool? _topResumed;
  AppLifecycleState _state = AppLifecycleState.resumed;

  static const _privacy = MethodChannel('xyz.zafe/privacy');

  void _cover(bool covered) {
    if (covered != _covered && mounted) setState(() => _covered = covered);
  }

  void _updateCover() => _cover(
    privacyCoverFor(
          _state,
          prompting: DeviceAuth.prompting,
          topResumed: _topResumed,
        ) ==
        CoverMode.now,
  );

  Future<void> _onPrivacyCall(MethodCall call) async {
    if (call.method != 'topResumed') return;
    _topResumed = call.arguments as bool?;
    _updateCover();
  }

  Future<void> _readTopResumed() async {
    final bool? topResumed;
    try {
      topResumed = await _privacy.invokeMethod<bool>('isTopResumed');
    } on MissingPluginException {
      _updateCover();
      return;
    } on PlatformException {
      _updateCover();
      return;
    }
    _topResumed = topResumed;
    _updateCover();
  }

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _privacy.setMethodCallHandler(_onPrivacyCall);
    unawaited(_readTopResumed());
    if (ref.read(appLockedProvider)) {
      WidgetsBinding.instance.addPostFrameCallback((_) => unawaited(_unlock()));
    }
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _privacy.setMethodCallHandler(null);
    super.dispose();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    _state = state;
    _updateCover();
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

class LockScreen extends StatelessWidget {
  const LockScreen({
    super.key,
    required this.onUnlock,
    this.busy = false,
    this.message,
  });

  final VoidCallback onUnlock;
  final bool busy;

  final String? message;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return Material(
      color: colors.background.window,
      child: AppColumn(
        child: SafeArea(
          child: LayoutBuilder(
            builder: (context, box) => SingleChildScrollView(
              child: ConstrainedBox(
                constraints: BoxConstraints(minHeight: box.maxHeight),
                child: IntrinsicHeight(
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
                          style: AppTypography.title.copyWith(
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
              ),
            ),
          ),
        ),
      ),
    );
  }
}
