import 'dart:convert';

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../core/errors/zafe_error_copy.dart';
import '../../core/layout/mobile/zafe_screen.dart';
import '../../core/storage/member_names.dart';
import '../../core/storage/vault_summaries.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/app_button.dart';
import '../../core/widgets/app_icon.dart';
import '../../core/widgets/app_toast.dart';
import '../../core/widgets/mobile_text_field.dart';
import '../../providers/vault_provider.dart';
import '../onboarding/onboarding_art.dart';
import '../../rust/api/backup.dart' as rust;

/// Restore a vault from a backup file or its text form (spec §12.3).
class RestoreScreen extends ConsumerStatefulWidget {
  const RestoreScreen({super.key});

  @override
  ConsumerState<RestoreScreen> createState() => _RestoreScreenState();
}

class _RestoreScreenState extends ConsumerState<RestoreScreen> {
  final _text = TextEditingController();
  final _textFocus = FocusNode();
  final _pass = TextEditingController();
  final _passFocus = FocusNode();
  List<int>? _file;
  String? _fileName;
  bool _busy = false;

  @override
  void dispose() {
    _text.dispose();
    _textFocus.dispose();
    _pass.dispose();
    _passFocus.dispose();
    super.dispose();
  }

  List<int>? get _data =>
      _file ??
      (_text.text.trim().isEmpty ? null : utf8.encode(_text.text.trim()));

  Future<void> _pickFile() async {
    final picked = await FilePicker.pickFile();
    if (picked == null) return;
    final List<int> bytes;
    try {
      bytes = await picked.readAsBytes();
    } catch (_) {
      return;
    }
    if (!mounted) return;
    setState(() {
      _file = bytes;
      _fileName = picked.name;
      _text.clear();
    });
  }

  Future<void> _pasteText() async {
    final data = await Clipboard.getData(Clipboard.kTextPlain);
    if (data?.text == null) return;
    setState(() {
      _text.text = data!.text!.trim();
      _file = null;
      _fileName = null;
    });
  }

  Future<void> _restore() async {
    setState(() => _busy = true);
    try {
      final v = await rust.importVaultBackup(
        data: _data!,
        passphrase: _pass.text,
      );
      final notifier = ref.read(vaultProvider.notifier);
      if (ref.read(vaultProvider).vaults.any((x) => x.id == v.vaultId)) {
        if (mounted) {
          showAppToast(
            context,
            '"${v.name}" is already on this phone',
            iconName: AppIcons.warningCircle,
            tone: AppToastTone.destructive,
          );
        }
        return;
      }
      // The signer names saved with the backup (before the vault becomes active, so the
      // names provider picks them up).
      final names = MemberNames.fromSigners(v.names);
      if (names.isNotEmpty) await MemberNames.write(v.vaultId, names);
      await notifier.addRestoredVault(
        vaultId: v.vaultId,
        identity: v.identitySeeds,
        material: v.material,
        invite: v.invite,
      );
      // A backup of this vault exists (it was just restored from one).
      await VaultSummaries.write(v.vaultId, backedUp: true);
      if (!mounted) return;
      showAppToast(context, '"${v.name}" restored');
      context.go('/home');
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

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return ZafeScreen(
      title: 'Restore vault',
      bottom: AppButton(
        expand: true,
        onPressed: _data != null && _pass.text.isNotEmpty && !_busy
            ? _restore
            : null,
        child: Text(_busy ? 'Decrypting...' : 'Restore vault'),
      ),
      children: [
        const OnboardingBanner('restore_key'),
        const SizedBox(height: AppSpacing.md),
        AppButton(
          expand: true,
          variant: AppButtonVariant.secondary,
          leading: const AppIcon(AppIcons.importWallet, size: 20),
          onPressed: _busy ? null : _pickFile,
          child: Text(_fileName ?? 'Choose backup file'),
        ),
        const SizedBox(height: AppSpacing.s),
        Center(
          child: Text(
            'or paste the backup text',
            style: AppTypography.labelMedium.copyWith(color: colors.text.muted),
          ),
        ),
        const SizedBox(height: AppSpacing.s),
        MobileTextField(
          secret: true,
          controller: _text,
          focusNode: _textFocus,
          hintText: 'zafe-backup-v1:...',
          onChanged: (_) => setState(() {
            _file = null;
            _fileName = null;
          }),
          trailing: GestureDetector(
            onTap: _pasteText,
            child: Container(
              height: 36,
              padding: const EdgeInsets.symmetric(horizontal: AppSpacing.sm),
              alignment: Alignment.center,
              decoration: BoxDecoration(
                color: colors.background.neutralSubtleOpacity,
                borderRadius: BorderRadius.circular(10),
              ),
              child: Text(
                'Paste',
                style: AppTypography.labelLarge.copyWith(
                  color: colors.text.accent,
                ),
              ),
            ),
          ),
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
          hintText: 'The passphrase you chose when backing up',
          onChanged: (_) => setState(() {}),
        ),
      ],
    );
  }
}
