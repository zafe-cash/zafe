import 'dart:async';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:path_provider/path_provider.dart';
import 'package:share_plus/share_plus.dart';

import '../../core/errors/zafe_error_copy.dart';
import '../../core/layout/mobile/zafe_screen.dart';
import '../../core/security/unlock_gate.dart';
import '../../core/storage/member_names.dart';
import '../../core/storage/vault_summaries.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/app_button.dart';
import '../../core/widgets/app_icon.dart';
import '../../core/widgets/app_toast.dart';
import '../../core/widgets/mobile/mobile_surface_card.dart';
import '../../core/widgets/mobile_text_field.dart';
import '../../providers/proposals_provider.dart';
import '../../providers/vault_names_provider.dart';
import '../../providers/vault_provider.dart';
import '../onboarding/onboarding_art.dart';
import 'backup_prompt_screen.dart' show backupStatusProvider;
import '../../rust/api/backup.dart' as rust;

/// Export this device's copy of the vault as a passphrase-encrypted backup (spec §12.2).
class ExportScreen extends ConsumerStatefulWidget {
  const ExportScreen({super.key});

  @override
  ConsumerState<ExportScreen> createState() => _ExportScreenState();
}

class _ExportScreenState extends ConsumerState<ExportScreen> {
  final _pass = TextEditingController();
  final _passFocus = FocusNode();
  final _confirm = TextEditingController();
  final _confirmFocus = FocusNode();
  bool _busy = false;
  rust.ExportedBackup? _done;

  @override
  void dispose() {
    _pass.dispose();
    _passFocus.dispose();
    _confirm.dispose();
    _confirmFocus.dispose();
    super.dispose();
  }

  rust.PassphraseCheck get _check =>
      rust.checkBackupPassphrase(passphrase: _pass.text);

  bool get _ready =>
      _pass.text.isNotEmpty && _check.ok && _confirm.text == _pass.text;

  void _suggest() {
    final words = rust.suggestBackupPassphrase();
    setState(() {
      _pass.text = words;
      _confirm.text = words;
    });
  }

  Future<void> _export() async {
    if (!await confirmUnlock(
      context,
      ref,
      reason: 'Unlock to export a backup',
    )) {
      return;
    }
    final vault = ref.read(vaultProvider);
    setState(() => _busy = true);
    try {
      final backup = await rust.exportVaultBackup(
        seeds: vault.identity!,
        material: vault.material!,
        invite: vault.invite!,
        names: MemberNames.toSigners(await MemberNames.read(vault.activeId!)),
        passphrase: _pass.text,
      );
      setState(() => _done = backup);
    } catch (e) {
      if (mounted) {
        showAppToast(
          context,
          zafeErrorMessage(e),
          iconName: AppIcons.warningCircle,
          tone: AppToastTone.destructive,
        );
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  String get _fileName {
    final name = ref.read(activeVaultNameProvider) ?? 'vault';
    final safe = name.replaceAll(RegExp(r'[^A-Za-z0-9_-]+'), '-');
    final d = DateTime.now();
    String two(int n) => n.toString().padLeft(2, '0');
    return 'Zafe-$safe-${d.year}-${two(d.month)}-${two(d.day)}.zafebackup';
  }

  /// The backup left the app (saved or sent): stop the reminder and tell the other
  /// members (the next refresh attests it in the vault log).
  Future<void> _saved() async {
    await VaultSummaries.write(
      ref.read(vaultProvider).activeId!,
      backedUp: true,
    );
    ref.invalidate(backupStatusProvider);
    unawaited(ref.read(proposalsProvider.notifier).refresh());
  }

  Future<void> _shareFile() async {
    final backup = _done!;
    final dir = await getTemporaryDirectory();
    final file = File('${dir.path}/$_fileName');
    await file.writeAsBytes(backup.bytes, flush: true);
    final result = await SharePlus.instance.share(
      ShareParams(files: [XFile(file.path)], fileNameOverrides: [_fileName]),
    );
    // `unavailable`: the platform can't tell whether a target was picked.
    if (result.status != ShareResultStatus.dismissed) await _saved();
    // The share sheet has copied or sent it; don't leave the backup lying in the cache.
    try {
      await file.delete();
    } catch (_) {}
  }

  Future<void> _copyText() async {
    await Clipboard.setData(ClipboardData(text: _done!.text));
    await _saved();
    if (mounted) {
      showAppToast(
        context,
        'Backup copied. Paste it into your password manager',
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final name = ref.watch(activeVaultNameProvider) ?? 'this vault';
    if (_done != null) {
      return ZafeScreen(
        title: 'Backup ready',
        children: [
          _Note(
            icon: AppIcons.checkCircle,
            iconColor: colors.icon.value,
            text:
                'Encrypted with your passphrase. Keep the file and the passphrase in different '
                'places: together they give full signing power in "$name".',
          ),
          const SizedBox(height: AppSpacing.md),
          AppButton(
            expand: true,
            leading: const AppIcon(AppIcons.share, size: 20),
            onPressed: _shareFile,
            child: const Text('Save or share the file'),
          ),
          const SizedBox(height: AppSpacing.s),
          AppButton(
            expand: true,
            variant: AppButtonVariant.secondary,
            leading: const AppIcon(AppIcons.copy, size: 20),
            onPressed: _copyText,
            child: const Text('Copy as text'),
          ),
          const SizedBox(height: AppSpacing.s),
          AppButton(
            expand: true,
            variant: AppButtonVariant.ghost,
            onPressed: () => context.go('/home'),
            child: const Text('Done'),
          ),
          const SizedBox(height: AppSpacing.md),
          Text(
            'Moving to a new phone? Restore this backup there first, check the vault works, '
            'then remove it from this phone in Settings.',
            style: AppTypography.bodySmall.copyWith(
              color: colors.text.secondary,
            ),
          ),
        ],
      );
    }

    final check = _check;
    final mismatch = _confirm.text.isNotEmpty && _confirm.text != _pass.text;
    return ZafeScreen(
      title: 'Back up vault',
      bottom: AppButton(
        expand: true,
        onPressed: _ready && !_busy ? _export : null,
        child: Text(_busy ? 'Encrypting...' : 'Create backup'),
      ),
      children: [
        const OnboardingBanner('backup_seal'),
        const SizedBox(height: AppSpacing.md),
        _Note(
          icon: AppIcons.lock,
          iconColor: colors.icon.accent,
          text:
              'A backup restores your seat in "$name" on another phone: your key share and '
              'vault access. Anyone with the backup and its passphrase gets your signing power '
              'here, so choose a strong passphrase and keep it separate from the file. '
              'Signing nonces are never included.',
        ),
        const SizedBox(height: AppSpacing.md),
        Text(
          'Passphrase',
          style: AppTypography.labelLarge.copyWith(
            color: colors.text.secondary,
          ),
        ),
        const SizedBox(height: AppSpacing.xs),
        MobileTextField(
          secret: true,
          controller: _pass,
          focusNode: _passFocus,
          hintText: '12 words, or a long unique phrase',
          onChanged: (_) => setState(() {}),
          trailing: _Pill(label: 'Suggest', onTap: _suggest),
        ),
        const SizedBox(height: AppSpacing.xs),
        if (_pass.text.isNotEmpty)
          Text(
            check.ok ? 'Strong passphrase' : check.hint,
            style: AppTypography.bodySmall.copyWith(
              color: check.ok ? colors.text.value : colors.text.destructive,
            ),
          ),
        const SizedBox(height: AppSpacing.sm),
        Text(
          'Confirm passphrase',
          style: AppTypography.labelLarge.copyWith(
            color: colors.text.secondary,
          ),
        ),
        const SizedBox(height: AppSpacing.xs),
        MobileTextField(
          secret: true,
          controller: _confirm,
          focusNode: _confirmFocus,
          hintText: 'Type it again',
          onChanged: (_) => setState(() {}),
        ),
        if (mismatch)
          Padding(
            padding: const EdgeInsets.only(top: AppSpacing.xs),
            child: Text(
              "Passphrases don't match",
              style: AppTypography.bodySmall.copyWith(
                color: colors.text.destructive,
              ),
            ),
          ),
        const SizedBox(height: AppSpacing.sm),
        Text(
          'Write the passphrase down. Zafe can\'t recover it for you.',
          style: AppTypography.bodySmall.copyWith(color: colors.text.secondary),
        ),
      ],
    );
  }
}

class _Note extends StatelessWidget {
  const _Note({
    required this.icon,
    required this.iconColor,
    required this.text,
  });
  final String icon;
  final Color iconColor;
  final String text;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return MobileSurfaceCard(
      cornerRadius: AppRadii.large,
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          AppIcon(icon, size: 20, color: iconColor),
          const SizedBox(width: AppSpacing.s),
          Expanded(
            child: Text(
              text,
              style: AppTypography.bodySmall.copyWith(
                color: colors.text.primary,
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _Pill extends StatelessWidget {
  const _Pill({required this.label, required this.onTap});
  final String label;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return GestureDetector(
      onTap: onTap,
      child: Container(
        height: 36,
        padding: const EdgeInsets.symmetric(horizontal: AppSpacing.sm),
        alignment: Alignment.center,
        decoration: BoxDecoration(
          color: colors.background.neutralSubtleOpacity,
          borderRadius: BorderRadius.circular(10),
        ),
        child: Text(
          label,
          style: AppTypography.labelLarge.copyWith(color: colors.text.accent),
        ),
      ),
    );
  }
}
