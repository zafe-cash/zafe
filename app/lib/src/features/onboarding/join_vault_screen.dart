import 'dart:async';

import 'package:flutter/material.dart';
import '../../core/storage/my_name.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../core/layout/mobile/zafe_screen.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/action_button.dart';
import '../../core/widgets/app_button.dart';
import '../../core/widgets/app_icon.dart';
import '../../core/widgets/app_tappable.dart';
import '../../core/widgets/mobile_text_field.dart';
import '../../providers/endpoints_provider.dart';
import '../../providers/vault_provider.dart';
import '../settings/relay_picker.dart' show relayHost;
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
  final _myName = TextEditingController();
  final _myNameFocus = FocusNode();
  rust.InviteInfo? _info;
  bool _fromLink = false;

  @override
  void initState() {
    super.initState();
    _prefill(widget.initialInvite);
    unawaited(
      MyName.lastUsed().then((n) {
        if (mounted && _myName.text.isEmpty) _myName.text = n;
      }),
    );
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
    _myName.dispose();
    _myNameFocus.dispose();
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
    await ref
        .read(vaultProvider.notifier)
        .joinVault(
          extractInvite(_invite.text) ?? _invite.text,
          myName: _myName.text,
        );
    if (mounted) context.go('/setup');
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
          trailing: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              // The camera, as an icon: a QR glyph next to an invite field needs no
              // words.
              Tooltip(
                message: 'Scan QR code',
                child: AppTappable(
                  semanticsLabel: 'Scan QR code',
                  onTap: _scan,
                  child: SizedBox(
                    width: 44,
                    height: 44,
                    child: Center(
                      child: AppIcon(
                        AppIcons.qr,
                        size: 22,
                        color: context.colors.icon.accent,
                      ),
                    ),
                  ),
                ),
              ),
              AppButton(
                variant: AppButtonVariant.secondary,
                size: AppButtonSize.mediumLarge,
                onPressed: _paste,
                child: const Text('Paste'),
              ),
            ],
          ),
        ),
        const SizedBox(height: AppSpacing.md),
        if (info != null)
          Text(
            inviteFact(
              name: info.name,
              threshold: info.threshold,
              members: info.members,
              // An invite from an older app runs on this phone's default relay.
              relayHost: relayHost(
                info.relayUrl ?? ref.watch(endpointsProvider).relayUrl,
              ),
            ),
            style: AppTypography.bodyMedium.copyWith(color: colors.text.accent),
          )
        else if (_invite.text.trim().isNotEmpty)
          Text(
            'This doesn\'t look like a Zafe invite.',
            style: AppTypography.bodySmall.copyWith(
              color: colors.text.destructive,
            ),
          ),
        if (info != null) ...[
          const SizedBox(height: AppSpacing.md),
          Text(
            'Your name',
            style: AppTypography.labelMedium.copyWith(
              color: colors.text.secondary,
            ),
          ),
          const SizedBox(height: AppSpacing.xs),
          MobileTextField(
            controller: _myName,
            focusNode: _myNameFocus,
            hintText: 'Shown to the other members',
          ),
        ],
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

/// What an invite is, as a fact: `Grants · 2 of 3 · relay.dao.org`.
String inviteFact({
  required String name,
  required int threshold,
  required int members,
  required String relayHost,
}) => '$name · $threshold of $members · $relayHost';
