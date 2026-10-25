import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../core/config/relay_presets.dart';
import '../../core/layout/mobile/app_mobile_sheet.dart';
import '../../core/security/unlock_gate.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/action_button.dart';
import '../../core/widgets/app_button.dart';
import '../../core/widgets/mobile/zafe_detail.dart';
import '../../core/widgets/mobile_text_field.dart';
import '../../providers/relay_move_provider.dart';
import '../../providers/vault_names_provider.dart';
import '../../providers/vault_provider.dart';
import '../../providers/vault_relay.dart';
import '../../rust/api/vault.dart' as rust;
import 'relay_picker.dart';

/// Settings > "Relay · everyone in `vault`": the vault's own relay (every member uses
/// it), what it says about itself, its create token when it is private, and **Move to
/// another relay** (docs/relay-move.md §4.2).
Future<void> showVaultRelaySheet(BuildContext context) =>
    showAppMobileSheet<void>(
      context: context,
      builder: (_) => const _VaultRelaySheet(),
    );

class _VaultRelaySheet extends ConsumerStatefulWidget {
  const _VaultRelaySheet();

  @override
  ConsumerState<_VaultRelaySheet> createState() => _VaultRelaySheetState();
}

class _VaultRelaySheetState extends ConsumerState<_VaultRelaySheet> {
  RelayCheckResult _result = const RelayCheckResult.checking();
  final _token = TextEditingController();
  final _tokenFocus = FocusNode();

  @override
  void initState() {
    super.initState();
    final url = ref.read(activeVaultRelayProvider)?.url;
    if (url != null) {
      checkRelayAt(url).then((r) {
        if (mounted) setState(() => _result = r);
      });
    }
  }

  @override
  void dispose() {
    _token.dispose();
    _tokenFocus.dispose();
    super.dispose();
  }

  /// Saves the token into the vault's material (any member may need it to restore the
  /// vault on a private relay).
  Future<void> _saveToken() async {
    final vault = ref.read(vaultProvider);
    final id = vault.activeId, material = vault.material;
    if (id == null || material == null) return;
    if (!await confirmUnlock(
      context,
      ref,
      reason: 'Confirm it\'s you to save the create token',
    )) {
      throw const ActionCancelled();
    }
    final updated = rust.setVaultCreateToken(
      material: material,
      token: _token.text.trim(),
    );
    await ref.read(vaultProvider.notifier).replaceMaterial(id, updated);
    _token.clear();
  }

  @override
  Widget build(BuildContext context) {
    final relay = ref.watch(activeVaultRelayProvider);
    final name =
        ref.watch(activeVaultNameProvider) ??
        ref.watch(vaultProvider.select((v) => v.summary?.name)) ??
        '';
    if (relay == null) return const SizedBox.shrink();
    final moving = ref.watch(relayMoveProvider.select((m) => m.moving));
    final movingTo = ref.watch(relayMoveProvider.select((m) => m.status?.host));
    return MobileModalScaffold(
      title: vaultRelayTitle(name),
      titleMaxLines: 2,
      onClose: () => Navigator.of(context).pop(),
      child: VaultRelayBody(
        url: relay.url,
        hasCreateToken: relay.hasCreateToken,
        result: _result,
        token: _token,
        tokenFocus: _tokenFocus,
        onTokenChanged: (_) => setState(() {}),
        onSaveToken: _token.text.trim().isEmpty ? null : _saveToken,
        movingTo: moving ? movingTo : null,
        onMove: () {
          final router = GoRouter.of(context);
          Navigator.of(context).pop();
          router.push('/relay-move');
        },
      ),
    );
  }
}

/// The vault relay sheet's content from plain data (rendered without Rust in previews).
class VaultRelayBody extends StatelessWidget {
  const VaultRelayBody({
    super.key,
    required this.url,
    required this.hasCreateToken,
    required this.result,
    required this.token,
    required this.tokenFocus,
    this.onTokenChanged,
    this.onSaveToken,
    this.onMove,
    this.movingTo,
  });

  final String url;
  final bool hasCreateToken;
  final RelayCheckResult result;
  final TextEditingController token;
  final FocusNode tokenFocus;
  final ValueChanged<String>? onTokenChanged;
  final Future<void> Function()? onSaveToken;

  /// Opens the move: "Move to another relay", or "Moving to relay.dao.org" while one runs.
  final VoidCallback? onMove;
  final String? movingTo;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final info = result.info;
    final operator = relayPresetFor(url)?.operator;
    final status = result.checking
        ? 'Checking...'
        : result.tooOld
        ? 'Needs an update'
        : result.error != null
        ? 'Unavailable'
        : 'Connected';
    // A saved token, or a private relay that needs one: show where it stands.
    final askToken = !hasCreateToken && (info?.createTokenRequired ?? false);
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        DetailRow(label: 'Address', value: relayHost(url)),
        if (operator != null) DetailRow(label: 'Run by', value: operator),
        DetailRow(label: 'Status', value: status),
        if (hasCreateToken)
          const DetailRow(label: 'Create token', value: 'Saved'),
        if (info != null && !info.tooOld && !info.push) ...[
          const SizedBox(height: AppSpacing.xs),
          Text(
            'No notifications',
            style: AppTypography.bodySmall.copyWith(
              color: colors.text.secondary,
            ),
          ),
        ],
        if (result.error != null) ...[
          const SizedBox(height: AppSpacing.xs),
          Text(
            result.error!,
            style: AppTypography.bodySmall.copyWith(
              color: colors.text.destructive,
            ),
          ),
        ],
        if (askToken) ...[
          const SizedBox(height: AppSpacing.md),
          Text(
            'Create token',
            style: AppTypography.labelMedium.copyWith(
              color: colors.text.secondary,
            ),
          ),
          const SizedBox(height: AppSpacing.xs),
          MobileTextField(
            secret: true,
            controller: token,
            focusNode: tokenFocus,
            hintText: 'Needed to restore the vault here',
            textInputAction: TextInputAction.done,
            inputFormatters: [LengthLimitingTextInputFormatter(200)],
            onChanged: onTokenChanged,
          ),
          const SizedBox(height: AppSpacing.sm),
          ActionButton(
            label: 'Save token',
            doneLabel: 'Saved',
            enabled: onSaveToken != null,
            errorFallback: 'Couldn\'t save the token.',
            onPressed: onSaveToken ?? () async {},
          ),
        ],
        if (onMove != null) ...[
          const SizedBox(height: AppSpacing.md),
          AppButton(
            expand: true,
            variant: AppButtonVariant.secondary,
            onPressed: onMove,
            child: Text(
              movingTo == null
                  ? 'Move to another relay'
                  : 'Moving to $movingTo',
            ),
          ),
        ],
      ],
    );
  }
}

/// The Settings row and sheet title: the relay belongs to the whole vault.
String vaultRelayTitle(String vaultName) => 'Relay · everyone in $vaultName';
