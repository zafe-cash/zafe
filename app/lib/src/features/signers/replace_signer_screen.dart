import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../core/errors/zafe_error_copy.dart';
import '../../core/formatting/member_label.dart';
import '../../core/layout/mobile/zafe_screen.dart';
import '../../core/security/unlock_gate.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/app_button.dart';
import '../../core/widgets/app_icon.dart';
import '../../core/widgets/app_toast.dart';
import '../../core/widgets/mobile/mobile_surface_card.dart';
import '../../core/widgets/mobile/zafe_detail.dart';
import '../../core/widgets/mobile_text_field.dart';
import '../../providers/member_names_provider.dart';
import '../../providers/proposals_provider.dart';
import '../../providers/vault_provider.dart';
import '../../rust/api/repair.dart' as rust;

/// A co-signer approves moving a lost signer's seat to their new phone (spec §10.1):
/// pick the signer, paste the code from their new phone, compare the safety code with
/// them, approve.
class ReplaceSignerScreen extends ConsumerStatefulWidget {
  const ReplaceSignerScreen({super.key, this.signer, this.code});

  /// The signer who lost their phone (key hex), if already known.
  final String? signer;

  /// The new phone's recovery code, if already known (approving a pending move).
  final String? code;

  @override
  ConsumerState<ReplaceSignerScreen> createState() =>
      _ReplaceSignerScreenState();
}

class _ReplaceSignerScreenState extends ConsumerState<ReplaceSignerScreen> {
  final _code = TextEditingController();
  final _codeFocus = FocusNode();
  String? _signer;
  bool _busy = false;

  @override
  void initState() {
    super.initState();
    _signer = widget.signer;
    _code.text = widget.code ?? '';
  }

  @override
  void dispose() {
    _code.dispose();
    _codeFocus.dispose();
    super.dispose();
  }

  rust.RecoveryCodeInfo? get _parsed {
    final text = _code.text.trim();
    if (text.isEmpty) return null;
    try {
      return rust.parseRecoveryCode(code: text);
    } catch (_) {
      return null;
    }
  }

  Future<void> _scan() async {
    final code = await context.push<String>('/scan-recovery');
    if (code != null && mounted) setState(() => _code.text = code);
  }

  Future<void> _paste() async {
    final data = await Clipboard.getData(Clipboard.kTextPlain);
    if (data?.text == null) return;
    setState(() => _code.text = data!.text!.trim());
  }

  Future<void> _approve() async {
    final signer = _signer!;
    if (!await confirmUnlock(
      context,
      ref,
      reason: 'Unlock to approve moving a signer\'s seat',
    )) {
      return;
    }
    setState(() => _busy = true);
    try {
      final moved = await ref
          .read(proposalsProvider.notifier)
          .approveSeatMove(signer, _code.text.trim());
      if (!mounted) return;
      showAppToast(
        context,
        moved
            ? 'Seat moved. Your phone helps rebuild their key now.'
            : 'Approved. The move needs more co-signers.',
      );
      context.pop();
    } catch (e) {
      if (mounted) {
        showAppToast(
          context,
          zafeErrorMessage(e, fallback: 'Couldn\'t approve. Try again.'),
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
    final vault = ref.watch(vaultProvider);
    final me = vault.myKeyHex;
    final names = ref.watch(signerNamesProvider);
    final others = (vault.summary?.members ?? const <String>[])
        .where((m) => m != me)
        .toList();
    final parsed = _parsed;
    final invalid = _code.text.trim().isNotEmpty && parsed == null;
    final who = _signer == null
        ? 'them'
        : memberLabel(_signer!, me: me, names: names);
    return ZafeScreen(
      title: 'Replace lost phone',
      bottom: AppButton(
        expand: true,
        onPressed: _signer != null && parsed != null && !_busy
            ? _approve
            : null,
        child: Text(_busy ? 'Approving...' : 'Approve move'),
      ),
      children: [
        Text(
          'When a signer loses their phone and has no backup, co-signers can move their '
          'seat to a new phone. It takes as many approvals as a payment, and the '
          'approvers\' phones then rebuild the signer\'s key on the new phone.',
          style: AppTypography.bodyMedium.copyWith(
            color: colors.text.secondary,
          ),
        ),
        const SizedBox(height: AppSpacing.md),
        Text(
          'Who lost their phone?',
          style: AppTypography.labelLarge.copyWith(
            color: colors.text.secondary,
          ),
        ),
        const SizedBox(height: AppSpacing.xs),
        MobileSurfaceCard(
          cornerRadius: AppRadii.large,
          child: Column(
            children: [
              for (final m in others)
                SignerRow(
                  keyHex: m,
                  me: me,
                  name: names[m],
                  trailing: AppIcon(
                    m == _signer ? AppIcons.checkCircle : AppIcons.user,
                    size: 20,
                    active: m == _signer,
                    color: m == _signer
                        ? colors.icon.accent
                        : colors.icon.muted,
                  ),
                  onTap: widget.code == null
                      ? () => setState(() => _signer = m)
                      : null,
                ),
            ],
          ),
        ),
        const SizedBox(height: AppSpacing.md),
        Text(
          'Code from their new phone',
          style: AppTypography.labelLarge.copyWith(
            color: colors.text.secondary,
          ),
        ),
        const SizedBox(height: AppSpacing.xs),
        MobileTextField(
          secret: true,
          controller: _code,
          focusNode: _codeFocus,
          hintText: 'zafe-recover-v1:...',
          enabled: widget.code == null,
          onChanged: (_) => setState(() {}),
          trailing: widget.code != null
              ? null
              : Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    GestureDetector(
                      onTap: _paste,
                      child: Container(
                        height: 36,
                        padding: const EdgeInsets.symmetric(
                          horizontal: AppSpacing.sm,
                        ),
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
                    Semantics(
                      button: true,
                      label: 'Scan QR code',
                      child: GestureDetector(
                        onTap: _scan,
                        child: Padding(
                          padding: const EdgeInsets.symmetric(
                            horizontal: AppSpacing.xs,
                          ),
                          child: AppIcon(
                            AppIcons.qr,
                            size: 22,
                            color: colors.icon.accent,
                          ),
                        ),
                      ),
                    ),
                  ],
                ),
        ),
        if (invalid) ...[
          const SizedBox(height: AppSpacing.xs),
          Text(
            'That isn\'t a recovery code. It starts with "zafe-recover-v1:".',
            style: AppTypography.bodySmall.copyWith(
              color: colors.text.destructive,
            ),
          ),
        ],
        if (parsed != null) ...[
          const SizedBox(height: AppSpacing.md),
          SafetyCodeCard(safetyCode: parsed.safetyCode, who: who),
        ],
      ],
    );
  }
}

/// The new phone's safety code, to compare with the signer before approving.
class SafetyCodeCard extends StatelessWidget {
  const SafetyCodeCard({
    super.key,
    required this.safetyCode,
    required this.who,
  });

  final String safetyCode;
  final String who;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return MobileSurfaceCard(
      cornerRadius: AppRadii.large,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            'Safety code',
            style: AppTypography.labelLarge.copyWith(
              color: colors.text.secondary,
            ),
          ),
          const SizedBox(height: AppSpacing.xxs),
          Text(
            safetyCode,
            style: AppTypography.headlineMedium.copyWith(
              color: colors.text.accent,
              fontFeatures: const [FontFeature.tabularFigures()],
            ),
          ),
          const SizedBox(height: AppSpacing.xxs),
          Text(
            'Call $who or meet in person, and ask them to read the safety code on their '
            'new phone. Approve only if it\'s exactly this. Anyone with this seat can '
            'approve payments and sees everything in the vault.',
            style: AppTypography.bodySmall.copyWith(
              color: colors.text.secondary,
            ),
          ),
        ],
      ),
    );
  }
}
