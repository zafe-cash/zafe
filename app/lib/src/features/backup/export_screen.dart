import 'dart:async';
import 'dart:io';
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:path_provider/path_provider.dart';
import 'package:share_plus/share_plus.dart';

import '../../core/layout/mobile/zafe_screen.dart';
import '../../providers/endpoints_provider.dart';
import '../../core/platform/sensitive_clipboard.dart';
import '../../core/security/unlock_gate.dart';
import '../../core/storage/member_names.dart';
import '../../core/storage/vault_summaries.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/action_button.dart';
import '../../core/widgets/app_button.dart';
import '../../core/widgets/app_icon.dart';
import '../../core/widgets/mobile/mobile_surface_card.dart';
import '../../core/widgets/mobile_text_field.dart';
import '../../providers/proposals_provider.dart';
import '../../providers/vault_names_provider.dart';
import '../../providers/vault_provider.dart';
import '../../core/widgets/line_icon.dart';
import 'backup_prompt_screen.dart' show backupStatusProvider;
import '../../rust/api/backup.dart' as rust;

/// Export this device's copy of the vault as a passphrase-encrypted backup (spec §12.2).
/// The passphrase is always generated: 12 words the member saves, then proves by typing
/// three of them back (choosing one was left to people, and "Suggest" filled both
/// fields, so nothing ever checked the words were written down).
class ExportScreen extends ConsumerStatefulWidget {
  const ExportScreen({super.key});

  @override
  ConsumerState<ExportScreen> createState() => _ExportScreenState();
}

class _ExportScreenState extends ConsumerState<ExportScreen> {
  /// The 12 words, made once per visit.
  late final List<String> _words = rust
      .suggestBackupPassphrase()
      .split(' ')
      .where((w) => w.isNotEmpty)
      .toList();

  /// Which three words (0-based) the check asks for, in order.
  late final List<int> _asked = () {
    final rng = math.Random.secure();
    final picked = <int>{};
    while (picked.length < 3) {
      picked.add(rng.nextInt(_words.length));
    }
    return picked.toList()..sort();
  }();
  final _answers = [for (var i = 0; i < 3; i++) TextEditingController()];
  final _focus = [for (var i = 0; i < 3; i++) FocusNode()];

  /// The words are on screen (false: the check).
  bool _showWords = true;
  rust.ExportedBackup? _done;

  @override
  void dispose() {
    for (final c in _answers) {
      c.dispose();
    }
    for (final f in _focus) {
      f.dispose();
    }
    super.dispose();
  }

  bool _matches(int i) =>
      _answers[i].text.trim().toLowerCase() == _words[_asked[i]].toLowerCase();

  bool get _checked =>
      [for (var i = 0; i < 3; i++) _matches(i)].every((m) => m);

  Future<void> _export() async {
    if (!await confirmUnlock(context, ref, reason: 'Export a backup')) {
      throw const ActionCancelled();
    }
    final vault = ref.read(vaultProvider);
    final backup = await rust.exportVaultBackup(
      relayUrl: ref.read(endpointsProvider).relayUrl,
      seeds: vault.identity!,
      material: vault.material!,
      invite: vault.invite!,
      names: MemberNames.toSigners(await MemberNames.read(vault.activeId!)),
      passphrase: _words.join(' '),
    );
    if (mounted) setState(() => _done = backup);
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
    unawaited(ref.read(proposalsProvider.notifier).refreshFresh());
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
    if (result.status != ShareResultStatus.dismissed) {
      await _saved();
      // Saved somewhere: nothing left to do here.
      if (mounted) context.go('/home');
    }
    // The share sheet has copied or sent it; don't leave the backup lying in the cache.
    try {
      await file.delete();
    } catch (_) {}
  }

  Future<void> _copyText() async {
    await copySecret(_done!.text);
    await _saved();
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
            text: 'Store the file and passphrase apart.',
          ),
          const SizedBox(height: AppSpacing.md),
          AppButton(
            expand: true,
            leading: const AppIcon(AppIcons.share, size: 20),
            onPressed: _shareFile,
            child: const Text('Save or share the file'),
          ),
          const SizedBox(height: AppSpacing.s),
          ActionButton(
            label: 'Copy as text',
            icon: AppIcons.copy,
            doneLabel: 'Copied',
            resetAfterDone: true,
            variant: AppButtonVariant.secondary,
            onPressed: _copyText,
          ),
        ],
      );
    }

    if (_showWords) {
      return ZafeScreen(
        title: 'Back up vault',
        bottom: AppButton(
          expand: true,
          onPressed: () => setState(() => _showWords = false),
          child: const Text('I saved them'),
        ),
        children: [
          _Note(
            icon: AppIcons.lock,
            iconColor: colors.icon.accent,
            text:
                'Anyone with the file and these words can sign for you in "$name".',
          ),
          const SizedBox(height: AppSpacing.md),
          BackupWordGrid(words: _words),
          const SizedBox(height: AppSpacing.sm),
          ActionButton(
            label: 'Copy words',
            icon: AppIcons.copy,
            doneLabel: 'Copied',
            resetAfterDone: true,
            variant: AppButtonVariant.secondary,
            onPressed: () => copySecret(_words.join(' ')),
          ),
          const SizedBox(height: AppSpacing.sm),
          Text(
            'Write them down or keep them in a password manager. Zafe can\'t show them again.',
            style: AppTypography.bodySmall.copyWith(
              color: colors.text.secondary,
            ),
          ),
        ],
      );
    }

    return ZafeScreen(
      title: 'Check your words',
      bottom: ActionButton(
        label: 'Create backup',
        busyLabel: 'Encrypting...',
        enabled: _checked,
        onPressed: _export,
      ),
      children: [
        for (var i = 0; i < 3; i++) ...[
          Text(
            'Word ${_asked[i] + 1}',
            style: AppTypography.labelLarge.copyWith(
              color: colors.text.secondary,
            ),
          ),
          const SizedBox(height: AppSpacing.xs),
          MobileTextField(
            secret: true,
            controller: _answers[i],
            focusNode: _focus[i],
            onChanged: (_) => setState(() {}),
          ),
          if (_answers[i].text.trim().isNotEmpty && !_matches(i))
            Padding(
              padding: const EdgeInsets.only(top: AppSpacing.xs),
              child: Text(
                "That's not word ${_asked[i] + 1}",
                style: AppTypography.bodySmall.copyWith(
                  color: colors.text.destructive,
                ),
              ),
            ),
          const SizedBox(height: AppSpacing.sm),
        ],
        AppButton(
          variant: AppButtonVariant.ghost,
          onPressed: () => setState(() => _showWords = true),
          child: const Text('Show the words again'),
        ),
      ],
    );
  }
}

/// The backup words, numbered: two columns, one with large text (two wrapped them).
class BackupWordGrid extends StatelessWidget {
  const BackupWordGrid({super.key, required this.words});
  final List<String> words;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final oneColumn = MediaQuery.textScalerOf(context).scale(16) > 21;
    final half = oneColumn ? words.length : (words.length / 2).ceil();
    Widget column(int from, int to) => Expanded(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          for (var i = from; i < to; i++)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: AppSpacing.xs),
              child: Text.rich(
                TextSpan(
                  children: [
                    TextSpan(
                      text: '${i + 1}'.padLeft(2),
                      style: TextStyle(color: colors.text.secondary),
                    ),
                    TextSpan(text: '  ${words[i]}'),
                  ],
                ),
                style: TextStyle(
                  fontFamily: 'JetBrains Mono',
                  fontSize: 16,
                  height: 1.4,
                  color: colors.text.primary,
                ),
              ),
            ),
        ],
      ),
    );
    return MobileSurfaceCard(
      cornerRadius: AppRadii.large,
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [column(0, half), if (!oneColumn) column(half, words.length)],
      ),
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
          LineIcon(
            size: 20,
            line: AppTypography.bodySmall,
            icon: AppIcon(icon, size: 20, color: iconColor),
          ),
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
