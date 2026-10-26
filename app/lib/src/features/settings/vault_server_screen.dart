import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:share_plus/share_plus.dart';

import '../../core/config/relay_presets.dart';
import '../../core/errors/sync_failure.dart';
import '../../core/formatting/member_label.dart';
import '../../core/layout/mobile/zafe_screen.dart';
import '../../core/security/unlock_gate.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/action_button.dart';
import '../../core/widgets/app_button.dart';
import '../../core/widgets/app_icon.dart';
import '../../core/widgets/mobile/mobile_list_row.dart';
import '../../core/widgets/mobile/mobile_surface_card.dart';
import '../../core/widgets/mobile/zafe_detail.dart';
import '../../core/widgets/mobile_text_field.dart';
import '../../providers/member_names_provider.dart';
import '../../providers/relay_move_provider.dart';
import '../../providers/vault_names_provider.dart';
import '../../providers/vault_provider.dart';
import '../../providers/vault_relay.dart';
import '../../rust/api/relay_move.dart' as rust_move;
import '../../rust/api/vault.dart' as rust;
import '../home/sync_status_screen.dart' show currentSyncFailure;
import '../relay_move/relay_move_screen.dart';
import 'relay_picker.dart';

/// Settings > `<vault name>` > Server: the server everyone in the vault uses (docs/relay-move.md),
/// what it says about itself, its create token when it is private, and **Move to another
/// server**. While a move runs, the same screen is its waiting room: who has moved, a
/// Nudge for each member who hasn't, and Cancel. Moving is a state of this one place, never
/// a separate flow.
class VaultServerScreen extends ConsumerStatefulWidget {
  const VaultServerScreen({super.key});

  @override
  ConsumerState<VaultServerScreen> createState() => _VaultServerScreenState();
}

class _VaultServerScreenState extends ConsumerState<VaultServerScreen> {
  RelayCheckResult _result = const RelayCheckResult.checking();
  final _token = TextEditingController();
  final _tokenFocus = FocusNode();
  List<String>? _blockers;

  /// This visit showed the move in progress: its switch then shows once, with Done.
  bool _sawMoving = false;

  @override
  void initState() {
    super.initState();
    final url = ref.read(activeVaultRelayProvider)?.url;
    if (url != null) {
      checkRelayAt(url).then((r) {
        if (mounted) setState(() => _result = r);
      });
    }
    final moves = ref.read(relayMoveProvider.notifier);
    moves.blockers().then(
      (b) {
        if (mounted) setState(() => _blockers = b);
      },
      onError: (_) {
        if (mounted) setState(() => _blockers = const []);
      },
    );
    moves.refresh();
  }

  @override
  void dispose() {
    _token.dispose();
    _tokenFocus.dispose();
    super.dispose();
  }

  String get _vaultName =>
      ref.read(activeVaultNameProvider) ??
      ref.read(vaultProvider).summary?.name ??
      'the vault';

  /// Saves the token into the vault's material (any member may need it to restore the
  /// vault on a private server).
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

  /// Move to another server: the list (the current one shown, never chosen), the unlock,
  /// then this screen becomes the waiting room. The others see it in Needs you; Nudge
  /// sends one of them the link.
  Future<void> _move() async {
    final current = ref.read(activeVaultRelayProvider)?.url ?? '';
    final choice = await showRelayPicker(
      context,
      title: 'Move to',
      selectedUrl: current,
      currentUrl: current,
    );
    if (choice == null || !mounted) throw const ActionCancelled();
    if (!await confirmUnlock(
      context,
      ref,
      reason: 'Move $_vaultName to ${relayHost(choice.url)}',
    )) {
      throw const ActionCancelled();
    }
    await ref
        .read(relayMoveProvider.notifier)
        .start(choice.url, token: choice.token);
  }

  Future<void> _nudge(String host, String link) => SharePlus.instance.share(
    ShareParams(
      text: rust_move.moveShareText(
        vaultName: _vaultName,
        host: host,
        link: link,
      ),
    ),
  );

  @override
  Widget build(BuildContext context) {
    final relay = ref.watch(activeVaultRelayProvider);
    final view = ref.watch(relayMoveProvider);
    final vault = ref.watch(vaultProvider);
    final names = ref.watch(signerNamesProvider);
    final me = vault.myKeyHex;
    final name =
        ref.watch(activeVaultNameProvider) ?? vault.summary?.name ?? '';
    final status = view.status;
    if (relay == null) return const ZafeScreen(title: 'Server', children: []);

    if (status != null && view.moving) {
      _sawMoving = true;
      final members = vault.summary?.members ?? const <String>[];
      final down =
          currentSyncFailure(ref)?.kind == SyncFailureKind.relayUnreachable;
      final missing = status.needed - status.arrived.length;
      return ZafeScreen(
        title: 'Server',
        bottom: status.mine
            ? ActionButton(
                label: 'Cancel move',
                variant: AppButtonVariant.ghost,
                errorFallback: 'Couldn\'t cancel. Try again.',
                onPressed: () => ref.read(relayMoveProvider.notifier).cancel(),
              )
            : null,
        children: [
          MoveWaitingRoom(
            vaultName: name,
            host: status.host,
            members: [
              for (final m in [?me, ...members.where((m) => m != me)])
                (
                  memberLabel(m, me: me, names: names),
                  status.arrived.contains(m),
                  m == me ? null : () => _nudge(status.host, status.link),
                ),
            ],
            needed: status.needed,
            total: status.members,
            resume: down && missing > 0
                ? paymentsResumeText(missing, status.host)
                : null,
          ),
        ],
      );
    }
    if (status != null && status.switched && status.mine && _sawMoving) {
      return ZafeScreen(
        title: 'Server',
        bottom: AppButton(
          expand: true,
          onPressed: () => context.pop(),
          child: const Text('Done'),
        ),
        children: [MovedBody(vaultName: name, host: status.host)],
      );
    }
    final blockers = _blockers;
    return ZafeScreen(
      title: 'Server',
      bottom: ActionButton(
        label: 'Move to another server',
        variant: AppButtonVariant.secondary,
        enabled: blockers != null && blockers.isEmpty && !view.twoMoves,
        errorFallback: 'Couldn\'t start the move. Try again.',
        onPressed: _move,
      ),
      children: [
        VaultServerBody(
          vaultName: name,
          url: relay.url,
          hasCreateToken: relay.hasCreateToken,
          result: _result,
          token: _token,
          tokenFocus: _tokenFocus,
          onTokenChanged: (_) => setState(() {}),
          onSaveToken: _token.text.trim().isEmpty ? null : _saveToken,
          twoMoves: view.twoMoves,
          mustUpdate: blockers == null || blockers.isEmpty
              ? null
              : mustUpdateText([
                  for (final b in blockers)
                    memberLabel(b, me: me, names: names),
                ]),
        ),
      ],
    );
  }
}

/// The Server screen from plain data (rendered without Rust in previews): the facts in
/// one card, what the server is for, its create token when it is private and needs one,
/// and why Move is off (a member must update, two moves found).
class VaultServerBody extends StatelessWidget {
  const VaultServerBody({
    super.key,
    required this.vaultName,
    required this.url,
    required this.hasCreateToken,
    required this.result,
    required this.token,
    required this.tokenFocus,
    this.onTokenChanged,
    this.onSaveToken,
    this.mustUpdate,
    this.twoMoves = false,
  });

  final String vaultName;
  final String url;
  final bool hasCreateToken;
  final RelayCheckResult result;
  final TextEditingController token;
  final FocusNode tokenFocus;
  final ValueChanged<String>? onTokenChanged;
  final Future<void> Function()? onSaveToken;
  final String? mustUpdate;
  final bool twoMoves;

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
        ? 'Can\'t reach it'
        : 'Connected';
    final secondary = AppTypography.bodyMedium.copyWith(
      color: colors.text.secondary,
    );
    // A private server that needs a token this phone doesn't have yet.
    final askToken = !hasCreateToken && (info?.createTokenRequired ?? false);
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        MobileSurfaceCard(
          cornerRadius: AppRadii.large,
          child: Column(
            children: [
              DetailRow(label: 'Address', value: relayHost(url)),
              if (operator != null) DetailRow(label: 'Run by', value: operator),
              DetailRow(label: 'Status', value: status),
              if (info != null && !info.tooOld)
                DetailRow(
                  label: 'Notifications',
                  value: info.push ? 'On' : 'Not on this server',
                ),
              if (hasCreateToken)
                const DetailRow(label: 'Create token', value: 'Saved'),
            ],
          ),
        ),
        const SizedBox(height: AppSpacing.sm),
        Text(
          'Everyone in $vaultName uses this server to work on payments together. '
          'It only sees encrypted messages.',
          style: secondary,
        ),
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
        if (twoMoves) ...[
          const SizedBox(height: AppSpacing.sm),
          Text(
            'Two moves were found. Ask your group which server to use.',
            style: AppTypography.bodyMedium.copyWith(
              color: colors.text.warning,
            ),
          ),
        ] else if (mustUpdate != null) ...[
          const SizedBox(height: AppSpacing.sm),
          Text(
            mustUpdate!,
            style: AppTypography.bodyMedium.copyWith(
              color: colors.text.warning,
            ),
          ),
        ],
      ],
    );
  }
}

/// A move in progress, like setup's waiting room: who has moved, a Nudge (the link, sent
/// to that member) for each one who hasn't, and when the vault switches.
class MoveWaitingRoom extends StatelessWidget {
  const MoveWaitingRoom({
    super.key,
    required this.vaultName,
    required this.host,
    required this.members,
    required this.needed,
    required this.total,
    this.resume,
  });

  final String vaultName;
  final String host;

  /// (label, moved, nudge), this phone first (no nudge).
  final List<(String, bool, VoidCallback?)> members;
  final int needed;
  final int total;

  /// "Payments resume when ..." while the old server is down.
  final String? resume;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final waiting = [
      for (final (label, moved, _) in members)
        if (!moved) label,
    ];
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        MoveHeading('Moving $vaultName to $host'),
        MobileSurfaceCard(
          cornerRadius: AppRadii.large,
          child: Column(
            children: [
              for (final (label, moved, nudge) in members)
                MobileListRow(
                  leading: AppIcon(
                    moved ? AppIcons.checkCircle : AppIcons.time,
                    size: 20,
                    active: moved,
                    color: moved ? colors.icon.accent : colors.icon.muted,
                  ),
                  label: label,
                  value: moved ? 'Moved' : null,
                  minRowHeight: 48,
                  trailing: moved || nudge == null
                      ? null
                      : AppButton(
                          size: AppButtonSize.small,
                          enabledBackgroundColor: colors.background.brandAlpha,
                          enabledLabelColor: colors.text.brand,
                          enabledBorderColor: const Color(0x00000000),
                          onPressed: nudge,
                          child: const Text('Nudge'),
                        ),
                ),
            ],
          ),
        ),
        const SizedBox(height: AppSpacing.sm),
        Text(
          resume ?? switchesWhenText(vaultName, needed, total),
          style: AppTypography.bodyMedium.copyWith(
            color: resume == null ? colors.text.secondary : colors.text.warning,
          ),
        ),
        if (waiting.isNotEmpty) ...[
          const SizedBox(height: AppSpacing.xs),
          Text(
            'They see the move in Zafe. Nudge sends one of them the link.',
            style: AppTypography.bodyMedium.copyWith(
              color: colors.text.secondary,
            ),
          ),
        ],
      ],
    );
  }
}
