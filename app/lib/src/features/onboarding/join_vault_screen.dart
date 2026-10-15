import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../core/layout/mobile/zafe_screen.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/action_button.dart';
import '../../core/widgets/app_button.dart';
import '../../core/widgets/app_icon.dart';
import '../../core/widgets/mobile_text_field.dart';
import '../../providers/vault_provider.dart';
import '../../rust/api/vault.dart' as rust;
import 'invite_link.dart';
import 'onboarding_art.dart';

class JoinVaultScreen extends ConsumerStatefulWidget {
  const JoinVaultScreen({super.key, this.initialInvite});

  /// Filled in from an invite link. Joining still needs a tap.
  final String? initialInvite;

  @override
  ConsumerState<JoinVaultScreen> createState() => _JoinVaultScreenState();
}

class _JoinVaultScreenState extends ConsumerState<JoinVaultScreen> {
  final _invite = TextEditingController();
  final _focus = FocusNode();
  rust.InviteInfo? _info;
  bool _busy = false;
  bool _fromLink = false;

  @override
  void initState() {
    super.initState();
    _prefill(widget.initialInvite);
  }

  @override
  void didUpdateWidget(JoinVaultScreen old) {
    super.didUpdateWidget(old);
    if (widget.initialInvite != old.initialInvite) {
      _prefill(widget.initialInvite);
      setState(() {});
    }
  }

  void _prefill(String? invite) {
    if (invite == null || invite.isEmpty) return;
    _invite.text = invite;
    _fromLink = true;
    _info = _tryParse(invite);
  }

  /// Accepts a raw invite or an invite link (pasted, typed or scanned).
  static rust.InviteInfo? _tryParse(String text) {
    final invite = extractInvite(text);
    if (invite == null) return null;
    try {
      return rust.parseInvite(invite: invite);
    } catch (_) {
      return null;
    }
  }

  @override
  void dispose() {
    _invite.dispose();
    _focus.dispose();
    super.dispose();
  }

  void _parse() {
    _info = _tryParse(_invite.text);
    setState(() {});
  }

  Future<void> _scan() async {
    final scanned = await context.push<String>('/scan-invite');
    if (scanned == null || !mounted) return;
    _invite.text = scanned;
    _fromLink = false;
    _parse();
  }

  Future<void> _paste() async {
    final data = await Clipboard.getData(Clipboard.kTextPlain);
    if (data?.text != null) {
      _invite.text = data!.text!.trim();
      _fromLink = false;
      _parse();
    }
  }

  Future<void> _join() async {
    setState(() => _busy = true);
    try {
      await ref
          .read(vaultProvider.notifier)
          .joinVault(extractInvite(_invite.text) ?? _invite.text);
      if (mounted) context.go('/setup');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final info = _info;
    return ZafeScreen(
      title: 'Join a vault',
      bottom: ActionButton(
        label: 'Join vault',
        enabled: info != null,
        errorFallback: 'Couldn\'t join. Try again.',
        onPressed: _join,
      ),
      children: [
        const OnboardingBanner('join_doorway'),
        const SizedBox(height: AppSpacing.md),
        Text(
          'Invite',
          style: AppTypography.labelMedium.copyWith(
            color: colors.text.secondary,
          ),
        ),
        const SizedBox(height: AppSpacing.xs),
        MobileTextField(
          secret: true,
          controller: _invite,
          focusNode: _focus,
          hintText: 'Paste the invite or link you received',
          onChanged: (_) {
            _fromLink = false;
            _parse();
          },
          trailing: AppButton(
            variant: AppButtonVariant.secondary,
            size: AppButtonSize.mediumLarge,
            onPressed: _paste,
            child: const Text('Paste'),
          ),
        ),
        const SizedBox(height: AppSpacing.sm),
        AppButton(
          expand: true,
          variant: AppButtonVariant.secondary,
          onPressed: _busy ? null : _scan,
          leading: const AppIcon(AppIcons.qr, size: 20),
          child: const Text('Scan QR code'),
        ),
        const SizedBox(height: AppSpacing.md),
        if (info != null)
          Text(
            '"${info.name}": ${info.threshold} of ${info.members} signatures needed.',
            style: AppTypography.bodyMedium.copyWith(color: colors.text.accent),
          )
        else if (_invite.text.trim().isNotEmpty)
          Text(
            'This doesn\'t look like a Zafe invite.',
            style: AppTypography.bodySmall.copyWith(
              color: colors.text.destructive,
            ),
          ),
        if (info != null && _fromLink) ...[
          const SizedBox(height: AppSpacing.sm),
          Text(
            'Opened from a link. Join only if you expected this invite. You\'ll compare safety words with every member first.',
            style: AppTypography.bodySmall.copyWith(
              color: colors.text.secondary,
            ),
          ),
        ],
      ],
    );
  }
}
