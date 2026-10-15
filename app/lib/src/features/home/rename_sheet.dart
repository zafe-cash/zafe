import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/layout/mobile/app_mobile_sheet.dart';
import '../../core/storage/member_names.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/action_button.dart';
import '../../core/widgets/app_button.dart';
import '../../core/widgets/mobile_text_field.dart';
import '../../core/storage/vault_name.dart';
import '../../providers/member_names_provider.dart';
import '../../providers/proposals_provider.dart';
import '../../providers/vault_names_provider.dart';
import '../../providers/vault_provider.dart';

/// Names a signer on this device (other members never see it).
Future<void> showRenameSignerSheet(
  BuildContext context,
  WidgetRef ref,
  String keyHex,
) async {
  final current = ref.read(memberNamesProvider)[keyHex] ?? '';
  await showAppMobileSheet<void>(
    context: context,
    builder: (sheet) => _RenameSheet(
      title: 'Name this signer',
      body: 'Only you see this label.',
      hint: 'e.g. Alice (treasurer)',
      maxLength: MemberNames.maxLength,
      current: current,
      removeLabel: 'Remove name',
      onSave: (name) =>
          ref.read(memberNamesProvider.notifier).rename(keyHex, name),
    ),
  );
}

/// This member's name for the other signers, shared through the vault log.
Future<void> showMyNameSheet(BuildContext context, WidgetRef ref) async {
  final me = ref.read(vaultProvider).myKeyHex;
  if (me == null) return;
  final current = ref.read(proposalsProvider).sharedNames[me] ?? '';
  await showAppMobileSheet<void>(
    context: context,
    builder: (sheet) => _RenameSheet(
      title: 'Your name',
      hint: 'e.g. Alice',
      maxLength: MemberNames.maxLength,
      current: current,
      removeLabel: 'Remove name',
      errorFallback: 'Couldn\'t save your name. Try again.',
      onSave: (name) async {
        if (MemberNames.clean(name) == current) return;
        await ref
            .read(proposalsProvider.notifier)
            .setMyName(MemberNames.clean(name));
      },
    ),
  );
}

/// Renames the active vault on this device (other members keep their names for it).
Future<void> showRenameVaultSheet(BuildContext context, WidgetRef ref) async {
  final id = ref.read(vaultProvider).activeId;
  final creator = ref.read(vaultProvider).summary?.name;
  if (id == null || creator == null) return;
  final current = ref.read(vaultNamesProvider)[id] ?? '';
  await showAppMobileSheet<void>(
    context: context,
    builder: (sheet) => _RenameSheet(
      title: 'Rename vault',
      body: 'Only on this phone. Created as "$creator".',
      hint: creator,
      maxLength: VaultName.maxLength,
      current: current.isEmpty ? creator : current,
      removeLabel: current.isEmpty ? null : 'Use "$creator"',
      onSave: (name) => ref
          .read(vaultNamesProvider.notifier)
          .rename(id, name.trim() == creator ? '' : name),
    ),
  );
}

class _RenameSheet extends StatefulWidget {
  const _RenameSheet({
    required this.title,
    this.body,
    required this.hint,
    required this.maxLength,
    required this.current,
    required this.onSave,
    this.removeLabel,
    this.errorFallback,
  });
  final String title;
  final String? body;
  final String hint;
  final int maxLength;
  final String current;

  /// Label of the button that clears the name; hidden when null.
  final String? removeLabel;

  /// Saves the name (empty clears it); the sheet closes once it's done, and a failure
  /// shows under the button.
  final Future<void> Function(String name) onSave;
  final String? errorFallback;

  @override
  State<_RenameSheet> createState() => _RenameSheetState();
}

class _RenameSheetState extends State<_RenameSheet> {
  late final _name = TextEditingController(text: widget.current);
  final _focus = FocusNode();

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) => _focus.requestFocus());
  }

  @override
  void dispose() {
    _name.dispose();
    _focus.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return MobileModalScaffold(
      title: widget.title,
      onClose: () => Navigator.of(context).pop(),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (widget.body != null) ...[
            Text(
              widget.body!,
              style: AppTypography.bodyMedium.copyWith(
                color: colors.text.secondary,
              ),
            ),
            const SizedBox(height: AppSpacing.sm),
          ],
          MobileTextField(
            controller: _name,
            focusNode: _focus,
            hintText: widget.hint,
            textInputAction: TextInputAction.done,
            inputFormatters: [
              LengthLimitingTextInputFormatter(widget.maxLength),
            ],
          ),
          const SizedBox(height: AppSpacing.md),
          ActionButton(
            label: 'Save',
            errorFallback: widget.errorFallback,
            onPressed: () => widget.onSave(_name.text),
            onDone: () => Navigator.of(context).pop(),
          ),
          if (widget.removeLabel != null && widget.current.isNotEmpty) ...[
            const SizedBox(height: AppSpacing.xs),
            ActionButton(
              label: widget.removeLabel!,
              variant: AppButtonVariant.ghost,
              errorFallback: widget.errorFallback,
              onPressed: () => widget.onSave(''),
              onDone: () => Navigator.of(context).pop(),
            ),
          ],
        ],
      ),
    );
  }
}
