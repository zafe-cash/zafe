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
import '../../core/widgets/action_button.dart';
import '../../core/widgets/app_button.dart';
import '../../core/widgets/app_icon.dart';
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

  /// Why Paste filled nothing (an empty clipboard, or text that isn't a backup).
  String? _pasteProblem;

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
      // Next: the words.
      _passFocus.requestFocus();
    });
  }

  /// Fills the field only with backup text: an empty clipboard (Zafe clears a copied
  /// backup after 60 s) or something else on it says so instead of doing nothing, and
  /// unknown clipboard text never lands in the field.
  Future<void> _pasteText() async {
    final pasted = await pastedBackupText();
    if (!mounted) return;
    setState(() {
      _pasteProblem = pasted.problem;
      if (pasted.text == null) return;
      _text.text = pasted.text!;
      _file = null;
      _fileName = null;
    });
    // Next: the words.
    if (pasted.text != null) _passFocus.requestFocus();
  }

  Future<void> _restore() async {
    setState(() => _busy = true);
    try {
      final v = await _open(_data!, _pass.text);
      final notifier = ref.read(vaultProvider.notifier);
      if (ref.read(vaultProvider).vaults.any((x) => x.id == v.vaultId)) {
        throw UserFacingError('"${v.name}" is already on this phone.');
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
      // Home showing the vault is the confirmation.
      if (mounted) context.go('/home');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  /// Opens the backup with what was typed, exactly (a passphrase from before the
  /// words were generated may care about case and spaces); if that fails and tidying
  /// it changes something (spaces, line breaks, capitals around the 12 words), once
  /// more tidied. The first error is the one shown.
  static Future<rust.ImportedVault> _open(List<int> data, String typed) async {
    try {
      return await rust.importVaultBackup(data: data, passphrase: typed);
    } catch (e, st) {
      final tidy = typed.trim().toLowerCase().split(RegExp(r'\s+')).join(' ');
      if (tidy == typed) rethrow;
      try {
        return await rust.importVaultBackup(data: data, passphrase: tidy);
      } catch (_) {
        Error.throwWithStackTrace(e, st);
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return ZafeScreen(
      title: 'Restore vault',
      bottom: ActionButton(
        label: 'Restore vault',
        busyLabel: 'Decrypting...',
        enabled: _data != null && _pass.text.isNotEmpty,
        onPressed: _restore,
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
        // A chosen file is the backup: no second way to give it (tap the file to
        // choose another).
        if (_fileName == null) ...[
          const SizedBox(height: AppSpacing.s),
          Center(
            child: Text(
              'or paste the backup text',
              style: AppTypography.labelMedium.copyWith(
                color: colors.text.muted,
              ),
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
              _pasteProblem = null;
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
          if (_pasteProblem != null)
            Padding(
              padding: const EdgeInsets.only(
                top: AppSpacing.xs,
                left: AppSpacing.xxs,
              ),
              child: Text(
                _pasteProblem!,
                style: AppTypography.labelLarge.copyWith(
                  color: colors.text.destructive,
                ),
              ),
            ),
        ],
        const SizedBox(height: AppSpacing.md),
        Text(
          'Backup words',
          style: AppTypography.labelLarge.copyWith(
            color: colors.text.secondary,
          ),
        ),
        const SizedBox(height: AppSpacing.xs),
        MobileTextField(
          secret: true,
          controller: _pass,
          focusNode: _passFocus,
          // Backups made before the words were generated took a typed passphrase:
          // it goes in the same field.
          hintText: 'The 12 words, or your passphrase',
          onChanged: (_) => setState(() {}),
        ),
      ],
    );
  }
}

/// The text form's prefix (`zafe_core::backup` `TEXT_PREFIX`, `zafe-backup-v1:`), without
/// the version so a later format still reads as a backup here.
const _backupTextPrefix = 'zafe-backup-v';

/// What the restore screen's Paste found on the clipboard: backup [text], or the
/// [problem] to show instead.
typedef PastedBackup = ({String? text, String? problem});

@visibleForTesting
Future<PastedBackup> pastedBackupText() async {
  final text =
      (await Clipboard.getData(Clipboard.kTextPlain))?.text?.trim() ?? '';
  if (text.isEmpty) {
    return (
      text: null,
      problem:
          'Nothing on the clipboard. Copy the backup text again, or choose the file.',
    );
  }
  if (!text.startsWith(_backupTextPrefix)) {
    return (
      text: null,
      problem: 'That isn\'t backup text. It starts with zafe-backup-v1:',
    );
  }
  return (text: text, problem: null);
}
