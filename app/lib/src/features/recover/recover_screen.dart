import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:pretty_qr_code/pretty_qr_code.dart';
import 'package:share_plus/share_plus.dart';

import '../../core/errors/zafe_error_copy.dart';
import '../../core/formatting/member_label.dart';
import '../../core/layout/mobile/zafe_screen.dart';
import '../../core/storage/zafe_secure_store.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/action_button.dart';
import '../../core/widgets/copy_button.dart';
import '../../core/widgets/app_button.dart';
import '../../core/widgets/app_icon.dart';
import '../../core/widgets/dot_qr_shape.dart';
import '../../providers/endpoints_provider.dart';
import '../../providers/vault_provider.dart';
import '../../rust/api/repair.dart' as rust;
import '../../rust/api/vault.dart' as rust_vault;

/// A new phone takes over a lost signer's seat without a backup (spec §10.1): it shows a
/// recovery code for a co-signer and a safety code to read out, then waits until enough
/// co-signers approved and their phones rebuilt its key. Before saving, the member checks
/// the vault's address with a co-signer: this phone can't know which vault to expect, so a
/// dishonest relay could otherwise present a vault of its own.
class RecoverScreen extends ConsumerStatefulWidget {
  const RecoverScreen({super.key});

  @override
  ConsumerState<RecoverScreen> createState() => _RecoverScreenState();
}

class _RecoverScreenState extends ConsumerState<RecoverScreen> {
  final _store = ZafeSecureStore.instance;
  List<int>? _seeds;
  rust.RecoveryCode? _code;
  rust.RecoveryProgress? _progress;
  Object? _error;
  Timer? _timer;
  bool _checking = false;
  bool _saving = false;

  /// A finished recovery waiting for the member to confirm the vault.
  rust.RecoveryProgress? _found;

  @override
  void initState() {
    super.initState();
    unawaited(_start());
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  Future<void> _start({bool fresh = false}) async {
    var seeds = fresh ? null : await _store.readRecoveryIdentity();
    if (seeds == null) {
      seeds = rust_vault.generateIdentity().seeds;
      await _store.writeRecoveryIdentity(seeds);
    }
    if (!mounted) return;
    setState(() {
      _seeds = seeds;
      _code = rust.recoveryCode(seeds: seeds!);
      _progress = null;
      _error = null;
    });
    _timer?.cancel();
    // Co-signers wait on this phone's part of each safety check: answer quickly.
    _timer = Timer.periodic(const Duration(seconds: 2), (_) => _check());
    unawaited(_check());
  }

  Future<void> _check() async {
    final seeds = _seeds;
    if (seeds == null || _checking || _saving) return;
    _checking = true;
    try {
      final progress = await rust.checkRecovery(
        relayUrl: ref.read(endpointsProvider).relayUrl,
        seeds: seeds,
      );
      if (!mounted) return;
      setState(() {
        _progress = progress;
        _error = null;
      });
      if (progress.stage == rust.RecoveryStage.done) {
        _timer?.cancel();
        setState(() => _found = progress);
      }
    } catch (e) {
      if (mounted) setState(() => _error = e);
    } finally {
      _checking = false;
    }
  }

  Future<void> _save(rust.RecoveryProgress p) async {
    setState(() => _saving = true);
    _timer?.cancel();
    final notifier = ref.read(vaultProvider.notifier);
    try {
      if (ref.read(vaultProvider).vaults.any((v) => v.id == p.vaultId)) {
        throw UserFacingError('"${p.name}" is already on this phone.');
      }
      await notifier.addRestoredVault(
        vaultId: p.vaultId,
        identity: _seeds!,
        material: p.material,
        invite: p.invite,
      );
    } catch (_) {
      if (mounted) setState(() => _saving = false);
      rethrow;
    }
    await _store.deleteRecoveryIdentity();
    if (!mounted) return;
    // The key now exists only here: offer a backup first, as after key generation.
    context.go('/backup-prompt');
  }

  Future<void> _startOver() async {
    final ok = await showDialog<bool>(
      context: context,
      builder: (c) => AlertDialog(
        title: const Text('Start over?'),
        content: const Text(
          'This phone makes a new code. Send it to your co-signers; approvals of the old one won\'t count.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(c, false),
            child: const Text('Cancel'),
          ),
          TextButton(
            onPressed: () => Navigator.pop(c, true),
            child: const Text('Start over'),
          ),
        ],
      ),
    );
    if (ok == true) {
      setState(() => _found = null);
      await _start(fresh: true);
    }
  }

  /// The member says the vault isn't theirs: drop it and this phone's identity.
  Future<void> _reject() async {
    await _store.deleteRecoveryIdentity();
    if (!mounted) return;
    setState(() => _found = null);
    await _start(fresh: true);
    // Shown on the new code's screen until the next check succeeds.
    if (mounted) {
      setState(
        () => _error = const UserFacingError(
          'Not saved. Tell your co-signers, and share this new code.',
        ),
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    final found = _found;
    if (found != null) {
      final summary = rust_vault.vaultSummary(material: found.material);
      return ZafeScreen(
        title: 'Check your vault',
        bottom: ActionButton(
          label: 'It matches: save the vault',
          errorFallback: 'Couldn\'t save the vault.',
          onPressed: () => _save(found),
        ),
        children: [
          VaultCheckBody(
            name: found.name,
            address: summary.address,
            onReject: _saving ? null : _reject,
          ),
        ],
      );
    }
    final code = _code;
    return ZafeScreen(
      title: 'Recover your seat',
      bottom: code == null
          ? null
          : AppButton(
              expand: true,
              leading: const AppIcon(AppIcons.share, size: 20),
              onPressed: () => SharePlus.instance.share(
                ShareParams(
                  // The safety words are compared by voice: whoever can swap this
                  // message can't make both phones show the same words.
                  text:
                      'Please move my Zafe seat to my new phone: in Zafe, open Signers, '
                      '"Replace a lost phone", and paste this code. I\'ll call to '
                      'compare safety words.\n\n${code.code}',
                ),
              ),
              child: const Text('Send code to a co-signer'),
            ),
      children: [
        if (code != null)
          RecoverBody(
            code: code.code,
            checks: _progress?.checks ?? const [],
            stage: _progress?.stage,
            received: _progress?.received ?? 0,
            needed: _progress?.needed ?? 0,
            error: _error == null
                ? null
                : zafeErrorMessage(
                    _error!,
                    fallback: 'Can\'t reach the relay.',
                  ),
            onStartOver: _startOver,
          ),
      ],
    );
  }
}

/// The recovery screen's content from plain data (rendered without Rust in previews).
class RecoverBody extends StatelessWidget {
  const RecoverBody({
    super.key,
    required this.code,
    this.checks = const [],
    this.stage,
    this.received = 0,
    this.needed = 0,
    this.error,
    this.onStartOver,
  });

  final String code;

  /// Each co-signer's words, once that co-signer opened the code.
  final List<rust.CosignerWords> checks;
  final rust.RecoveryStage? stage;
  final int received;
  final int needed;
  final String? error;
  final VoidCallback? onStartOver;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final status = switch (stage) {
      rust.RecoveryStage.repairing =>
        needed == 0
            ? 'Approved. Your co-signers\' phones are rebuilding your key.'
            : 'Approved. Rebuilding your key: $received of $needed co-signers done.',
      rust.RecoveryStage.done => 'Done. Saving the vault on this phone.',
      _ => 'Waiting for your co-signers to approve.',
    };
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(
          'Send this code to your co-signers. Once enough approve, their phones rebuild your key here.',
          style: AppTypography.bodyMedium.copyWith(
            color: colors.text.secondary,
          ),
        ),
        const SizedBox(height: AppSpacing.md),
        Center(
          child: Container(
            width: 232,
            height: 232,
            padding: const EdgeInsets.all(14),
            decoration: BoxDecoration(
              color: colors.background.darkCard,
              borderRadius: BorderRadius.circular(AppRadii.xLarge),
              border: Border.all(color: colors.border.subtleOpacity),
            ),
            child: PrettyQrView.data(
              data: code,
              decoration: PrettyQrDecoration(
                shape: DotQrShape(color: colors.text.darkCard),
              ),
            ),
          ),
        ),
        const SizedBox(height: AppSpacing.s),
        Center(
          child: CopyButton(
            label: 'Copy code',
            text: code,
            size: AppButtonSize.medium,
            expand: false,
          ),
        ),
        // The safety words, once a co-signer has opened the code.
        if (checks.isNotEmpty) ...[
          const SizedBox(height: AppSpacing.md),
          Text(
            'Safety words',
            style: AppTypography.labelLarge.copyWith(
              color: colors.text.secondary,
            ),
          ),
          const SizedBox(height: AppSpacing.xxs),
          for (final c in checks) ...[
            Text(
              'With co-signer ${shortKey(c.cosignerKeyHex)}',
              style: AppTypography.bodySmall.copyWith(
                color: colors.text.secondary,
              ),
            ),
            Text(
              c.words,
              style: AppTypography.headlineMedium.copyWith(
                color: colors.text.accent,
              ),
            ),
            const SizedBox(height: AppSpacing.xs),
          ],
          Text(
            'Compare them with each co-signer on a call or in person. If the words differ, or someone you didn\'t ask appears, stop.',
            style: AppTypography.bodySmall.copyWith(color: colors.text.muted),
          ),
        ],
        const SizedBox(height: AppSpacing.md),
        Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            AppIcon(
              error != null ? AppIcons.warningCircle : AppIcons.time,
              size: 20,
              color: error != null ? colors.icon.warning : colors.icon.accent,
            ),
            const SizedBox(width: AppSpacing.xs),
            Expanded(
              child: Text(
                error ?? status,
                style: AppTypography.bodySmall.copyWith(
                  color: error != null
                      ? colors.text.warning
                      : colors.text.secondary,
                ),
              ),
            ),
          ],
        ),
        const SizedBox(height: AppSpacing.md),
        Center(
          child: AppButton(
            variant: AppButtonVariant.ghost,
            size: AppButtonSize.small,
            onPressed: onStartOver,
            child: const Text('Start over with a new code'),
          ),
        ),
      ],
    );
  }
}

/// The recovered vault, checked with a co-signer before it is saved.
class VaultCheckBody extends StatelessWidget {
  const VaultCheckBody({
    super.key,
    required this.name,
    required this.address,
    this.onReject,
  });

  final String name;
  final String address;
  final VoidCallback? onReject;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final tail = address.length > 8
        ? address.substring(address.length - 8)
        : address;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(
          'Your key is back. Before saving "$name", ask a co-signer to read you the '
          'end of the vault address (Home, Receive).',
          style: AppTypography.bodyMedium.copyWith(
            color: colors.text.secondary,
          ),
        ),
        const SizedBox(height: AppSpacing.md),
        Text(
          'Vault address ends in',
          style: AppTypography.labelLarge.copyWith(
            color: colors.text.secondary,
          ),
        ),
        const SizedBox(height: AppSpacing.xxs),
        Text(
          tail,
          style: AppTypography.headlineMedium.copyWith(
            color: colors.text.accent,
          ),
        ),
        const SizedBox(height: AppSpacing.xs),
        Text(
          address,
          style: AppTypography.codeSmall.copyWith(color: colors.text.muted),
        ),
        const SizedBox(height: AppSpacing.md),
        Text(
          'If it\'s different, don\'t save it: it may be a fake vault.',
          style: AppTypography.bodySmall.copyWith(color: colors.text.muted),
        ),
        const SizedBox(height: AppSpacing.md),
        Center(
          child: AppButton(
            variant: AppButtonVariant.ghost,
            size: AppButtonSize.small,
            onPressed: onReject,
            child: const Text('It doesn\'t match'),
          ),
        ),
      ],
    );
  }
}
