import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../core/config/network_config.dart';
import '../../core/formatting/zec_amount.dart';
import '../../core/layout/mobile/app_mobile_sheet.dart';
import '../../core/layout/mobile/zafe_screen.dart';
import '../../core/privacy/amount_display.dart';
import '../../core/storage/vault_summaries.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/app_button.dart';
import '../../core/widgets/app_icon.dart';
import '../../core/widgets/app_tappable.dart';
import '../../core/widgets/full_address_viewer.dart';
import '../../core/widgets/mobile/mobile_surface_card.dart';
import '../../core/widgets/mobile/zafe_detail.dart';
import '../../providers/privacy_mode_provider.dart';
import '../../providers/vault_names_provider.dart';
import '../../providers/vault_provider.dart';
import '../../rust/api/proposals.dart' as rust;
import '../../rust/api/vault.dart' as rust_vault;
import '../vaults/vault_emblem.dart';
import 'payment_link.dart';
import 'send_screen.dart' show SendPrefill, kMaxRecipients;

/// A vault the request could be paid from.
class PayFromVault {
  const PayFromVault({
    required this.id,
    required this.name,
    required this.threshold,
    required this.members,
    this.balanceZat,
    this.spendableZat,
  });
  final String id;
  final String name;
  final int threshold;
  final int members;

  /// Last known balance (null before the first sync).
  final BigInt? balanceZat;

  /// Spendable now (only for vaults synced in this session).
  final BigInt? spendableZat;
}

/// What [payments] ask for in total.
BigInt requestedTotal(List<rust.ScannedPayment> payments) =>
    payments.fold(BigInt.zero, (sum, p) => sum + p.amountZat);

/// Opened by a `zcash:` payment link from another app or website. Shows what the link
/// asks for, warns that anyone can make one, lets the owner pick the vault to pay from,
/// and needs an explicit "I know who sent this" before the send flow opens with the
/// details filled in. Nothing is proposed from here.
class PaymentRequestScreen extends ConsumerStatefulWidget {
  const PaymentRequestScreen({super.key, required this.link});
  final PendingPaymentLink link;

  @override
  ConsumerState<PaymentRequestScreen> createState() =>
      _PaymentRequestScreenState();
}

class _PaymentRequestScreenState extends ConsumerState<PaymentRequestScreen> {
  late final rust.ScannedRequest _request = rust.parsePaymentRequest(
    networkName: kZafeNetwork,
    text: widget.link.raw,
  );
  String? _vaultId;

  /// The owner tapped a vault (otherwise the default may move to one that can pay).
  bool _picked = false;
  bool _confirmed = false;
  bool _busy = false;
  final Map<String, VaultSummaryInfo> _saved = {};

  @override
  void initState() {
    super.initState();
    final state = ref.read(vaultProvider);
    final ready = state.vaults.where((v) => v.ready).toList();
    _vaultId = ready.any((v) => v.id == state.activeId)
        ? state.activeId
        : ready.firstOrNull?.id;
    for (final v in ready) {
      unawaited(
        VaultSummaries.read(v.id).then((info) {
          if (mounted) setState(() => _saved[v.id] = info);
        }),
      );
    }
  }

  void _close() => context.canPop() ? context.pop() : context.go('/home');

  Future<void> _continue() async {
    final id = _vaultId;
    if (id == null || !_confirmed || _busy) return;
    setState(() => _busy = true);
    final router = GoRouter.of(context);
    final vaults = ref.read(vaultProvider);
    if (vaults.activeId != id || vaults.isAdding) {
      await ref.read(vaultProvider.notifier).switchTo(id);
    }
    router.go('/home');
    unawaited(
      router.push(
        '/send',
        extra: SendPrefill(
          payments: [
            for (final p in _request.payments)
              rust.PaymentInput(
                address: p.address,
                amountZat: p.amountZat,
                memo: p.memo,
              ),
          ],
          autoSend: true,
          fromLink: true,
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final state = ref.watch(vaultProvider);
    final names = ref.watch(vaultNamesProvider);
    final vaults = [
      for (final v in state.vaults.where((v) => v.ready))
        () {
          final summary = rust_vault.vaultSummary(material: v.material!);
          return PayFromVault(
            id: v.id,
            name: names[v.id] ?? summary.name,
            threshold: summary.threshold,
            members: summary.members.length,
            balanceZat:
                state.balances[v.id]?.totalZat ?? _saved[v.id]?.balanceZat,
            spendableZat: state.balances[v.id]?.spendableZat,
          );
        }(),
    ];
    // Default to a vault that can pay when the first choice can't.
    final requested = requestedTotal(_request.payments);
    bool short(PayFromVault v) =>
        checkFunds(
          requestedZat: requested,
          recipients: _request.payments.length,
          spendableZat: v.spendableZat,
          totalZat: v.balanceZat,
        ).check ==
        FundsCheck.short;
    final current = vaults.where((v) => v.id == _vaultId).firstOrNull;
    if (!_picked && current != null && short(current)) {
      _vaultId = vaults.where((v) => !short(v)).firstOrNull?.id ?? _vaultId;
    }
    final tooMany = _request.payments.length > kMaxRecipients;
    return PaymentRequestView(
      payments: _request.payments,
      problem: tooMany
          ? 'This link asks for more than $kMaxRecipients payments'
          : _request.problem,
      vaults: vaults,
      selectedId: _vaultId,
      onSelect: (id) => setState(() {
        _vaultId = id;
        _picked = true;
      }),
      confirmed: _confirmed,
      onConfirm: () => setState(() => _confirmed = !_confirmed),
      hideBalances: ref.watch(privacyModeProvider),
      busy: _busy,
      onContinue: () => unawaited(_continue()),
      onDecline: _close,
    );
  }
}

/// The payment request page, from plain data (renders without Rust).
class PaymentRequestView extends StatelessWidget {
  const PaymentRequestView({
    super.key,
    required this.payments,
    required this.problem,
    required this.vaults,
    required this.selectedId,
    required this.onSelect,
    required this.confirmed,
    required this.onConfirm,
    required this.onContinue,
    required this.onDecline,
    this.hideBalances = false,
    this.busy = false,
  });

  final List<rust.ScannedPayment> payments;

  /// Why the link can't be paid (empty when it can).
  final String problem;
  final List<PayFromVault> vaults;
  final String? selectedId;
  final ValueChanged<String> onSelect;
  final bool confirmed;
  final VoidCallback onConfirm;
  final VoidCallback onContinue;
  final VoidCallback onDecline;
  final bool hideBalances;
  final bool busy;

  /// Every vault in a sheet; one that can't pay is shown but can't be picked.
  Future<void> _pickVault(
    BuildContext context,
    ({FundsCheck check, BigInt shortByZat}) Function(PayFromVault) funds,
  ) async {
    final picked = await showAppMobileSheet<String>(
      context: context,
      builder: (sheet) => MobileModalScaffold(
        title: 'Pay from',
        onClose: () => Navigator.of(sheet).pop(),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            for (var i = 0; i < vaults.length; i++) ...[
              if (i > 0) const DetailDivider(),
              _VaultChoice(
                vault: vaults[i],
                selected: vaults[i].id == selectedId,
                funds: funds(vaults[i]),
                onTap: funds(vaults[i]).check == FundsCheck.short
                    ? null
                    : () => Navigator.of(sheet).pop(vaults[i].id),
                hideBalance: hideBalances,
              ),
            ],
          ],
        ),
      ),
    );
    if (picked != null) onSelect(picked);
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    if (problem.isNotEmpty) {
      return ZafeScreen(
        title: 'Payment request',
        onBack: onDecline,
        bottom: AppButton(
          expand: true,
          variant: AppButtonVariant.secondary,
          onPressed: onDecline,
          child: const Text('Close'),
        ),
        children: [
          _Callout(
            title: 'This link can\'t be paid',
            body: '$problem. Nothing was sent. Ask the sender for a new link.',
          ),
        ],
      );
    }
    final selected = vaults.where((v) => v.id == selectedId).firstOrNull;
    final requested = requestedTotal(payments);
    ({FundsCheck check, BigInt shortByZat}) funds(PayFromVault v) => checkFunds(
      requestedZat: requested,
      recipients: payments.length,
      spendableZat: v.spendableZat,
      totalZat: v.balanceZat,
    );
    final selectedFunds = selected == null ? null : funds(selected);
    final canPay =
        selectedFunds != null && selectedFunds.check != FundsCheck.short;
    final sectionStyle = AppTypography.labelLarge.copyWith(
      color: colors.text.secondary,
    );
    return ZafeScreen(
      title: 'Payment request',
      onBack: onDecline,
      bottom: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          AppButton(
            expand: true,
            onPressed: confirmed && canPay && !busy ? onContinue : null,
            child: const Text('Continue'),
          ),
          const SizedBox(height: AppSpacing.xs),
          AppButton(
            expand: true,
            variant: AppButtonVariant.ghost,
            onPressed: busy ? null : onDecline,
            child: const Text('Don\'t pay'),
          ),
        ],
      ),
      children: [
        const _Callout(
          title: 'Check who sent this',
          body:
              'Zafe can\'t tell who made this payment link. Continue only if you trust where it came from.',
        ),
        const SizedBox(height: AppSpacing.md),
        Text(
          payments.length == 1
              ? 'Requested payment'
              : 'Requested payments (${payments.length})',
          style: sectionStyle,
        ),
        const SizedBox(height: AppSpacing.xs),
        for (var i = 0; i < payments.length; i++) ...[
          if (i > 0) const SizedBox(height: AppSpacing.s),
          _RequestedPayment(payment: payments[i]),
        ],
        const SizedBox(height: AppSpacing.md),
        Text('Pay from', style: sectionStyle),
        const SizedBox(height: AppSpacing.xs),
        // Only the chosen vault; "Change" opens the others in a sheet.
        if (selected != null)
          MobileSurfaceCard(
            cornerRadius: AppRadii.large,
            padding: const EdgeInsets.symmetric(
              horizontal: AppSpacing.sm,
              vertical: AppSpacing.xs,
            ),
            child: Row(
              children: [
                Expanded(
                  child: _VaultChoice(
                    vault: selected,
                    selected: true,
                    funds: funds(selected),
                    onTap: null,
                    hideBalance: hideBalances,
                  ),
                ),
                if (vaults.length > 1 && !busy)
                  AppTappable(
                    onTap: () => _pickVault(context, funds),
                    semanticsLabel: 'Change vault',
                    child: ExcludeSemantics(
                      child: Padding(
                        padding: const EdgeInsets.only(left: AppSpacing.xs),
                        child: Text(
                          'Change',
                          style: AppTypography.labelLarge.copyWith(
                            color: colors.text.brand,
                            fontWeight: FontWeight.w600,
                          ),
                        ),
                      ),
                    ),
                  ),
              ],
            ),
          ),
        if (selected != null && !canPay) ...[
          const SizedBox(height: AppSpacing.s),
          Text(
            vaults.length > 1
                ? 'None of your vaults has enough to pay this request.'
                : '"${selected.name}" doesn\'t have enough to pay this request.',
            style: AppTypography.bodySmall.copyWith(color: colors.text.warning),
          ),
        ],
        const SizedBox(height: AppSpacing.md),
        _ConfirmBox(
          checked: confirmed,
          label:
              'I know who sent this, and the address and amount are what I '
              'expect',
          onTap: busy ? null : onConfirm,
        ),
      ],
    );
  }
}

/// Warning box: a title and a few sentences, in the warning colour.
class _Callout extends StatelessWidget {
  const _Callout({required this.title, required this.body});
  final String title;
  final String body;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return Container(
      padding: const EdgeInsets.all(AppSpacing.sm),
      decoration: BoxDecoration(
        color: colors.background.ground,
        borderRadius: BorderRadius.circular(AppRadii.large),
        border: Border.all(color: colors.icon.warning.withValues(alpha: 0.5)),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          AppIcon(
            AppIcons.warning,
            size: 20,
            color: colors.icon.warning,
            patina: colors.icon.warning,
          ),
          const SizedBox(width: AppSpacing.s),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  title,
                  style: AppTypography.labelLarge.copyWith(
                    color: colors.text.accent,
                    fontWeight: FontWeight.w600,
                  ),
                ),
                const SizedBox(height: AppSpacing.xxs),
                Text(
                  body,
                  style: AppTypography.bodySmall.copyWith(
                    color: colors.text.secondary,
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

/// One payment the link asks for: amount, the full address, memo, and what the link
/// says about itself (marked as unverified).
class _RequestedPayment extends StatelessWidget {
  const _RequestedPayment({required this.payment});
  final rust.ScannedPayment payment;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final labelStyle = AppTypography.labelMedium.copyWith(
      color: colors.text.secondary,
    );
    final valueStyle = AppTypography.bodyMedium.copyWith(
      color: colors.text.primary,
    );
    Widget claim(String label, String value) => Padding(
      padding: const EdgeInsets.only(top: AppSpacing.s),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(label, style: labelStyle),
          const SizedBox(height: AppSpacing.xxs),
          Text('"$value"', style: valueStyle),
        ],
      ),
    );
    final hasAmount = payment.amountZat > BigInt.zero;
    return MobileSurfaceCard(
      cornerRadius: AppRadii.large,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('Amount', style: labelStyle),
          const SizedBox(height: AppSpacing.xxs),
          Text(
            hasAmount
                ? amountWithTicker(
                    ZecAmount.fromZatoshi(payment.amountZat).receipt.amountText,
                    // The owner has to check the amount, so it's never masked.
                    hide: false,
                  )
                : 'Not set: you\'ll enter it next',
            style: hasAmount
                ? TextStyle(
                    fontFamily: 'Space Grotesk',
                    fontSize: 28,
                    height: 1.2,
                    color: colors.text.value,
                  )
                : valueStyle,
          ),
          const SizedBox(height: AppSpacing.s),
          Text('To', style: labelStyle),
          const SizedBox(height: AppSpacing.xxs),
          FullAddressText(address: payment.address),
          if (payment.label.isNotEmpty)
            claim('Name in the link (not verified)', payment.label),
          if (payment.message.isNotEmpty)
            claim('Message in the link (not verified)', payment.message),
          if (payment.memo.isNotEmpty)
            claim('Memo sent with the payment', payment.memo),
        ],
      ),
    );
  }
}

class _VaultChoice extends StatelessWidget {
  const _VaultChoice({
    required this.vault,
    required this.selected,
    required this.funds,
    required this.onTap,
    required this.hideBalance,
  });
  final PayFromVault vault;
  final bool selected;
  final ({FundsCheck check, BigInt shortByZat}) funds;
  final VoidCallback? onTap;
  final bool hideBalance;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final balance = vault.spendableZat ?? vault.balanceZat;
    final rule = '${vault.threshold} of ${vault.members} to approve';
    final short = funds.check == FundsCheck.short;
    final shortText = hideBalance
        ? 'Not enough funds'
        : 'Not enough funds: short by '
              '${amountWithTicker(ZecAmount.fromZatoshi(funds.shortByZat).receipt.amountText, hide: false)}';
    final detail = balance == null
        ? rule
        : '${amountWithTicker(ZecAmount.fromZatoshi(balance).balance.amountText, hide: hideBalance)}'
              '${vault.spendableZat != null ? ' spendable' : ''} · $rule';
    final row = Padding(
      padding: const EdgeInsets.symmetric(vertical: AppSpacing.xs),
      child: Row(
        children: [
          VaultEmblem(vaultId: vault.id),
          const SizedBox(width: AppSpacing.s),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  vault.name,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: AppTypography.labelLarge.copyWith(
                    color: colors.text.accent,
                    fontWeight: FontWeight.w600,
                  ),
                ),
                const SizedBox(height: AppSpacing.xxs),
                Text(
                  detail,
                  style: AppTypography.labelMedium.copyWith(
                    color: colors.text.secondary,
                  ),
                ),
                if (short) ...[
                  const SizedBox(height: AppSpacing.xxs),
                  Text(
                    shortText,
                    style: AppTypography.labelMedium.copyWith(
                      color: colors.text.warning,
                    ),
                  ),
                ],
              ],
            ),
          ),
          if (onTap != null) ...[
            const SizedBox(width: AppSpacing.xs),
            _Radio(selected: selected),
          ],
        ],
      ),
    );
    if (onTap == null) return row;
    return AppTappable(
      onTap: onTap,
      semanticsLabel:
          '${vault.name}${selected ? ', selected' : ''}${short ? ', $shortText' : ''}',
      child: ExcludeSemantics(child: row),
    );
  }
}

class _Radio extends StatelessWidget {
  const _Radio({required this.selected});
  final bool selected;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return Container(
      width: 24,
      height: 24,
      decoration: BoxDecoration(
        shape: BoxShape.circle,
        color: selected
            ? colors.background.inverse
            : colors.background.neutralSubtleOpacity,
      ),
      alignment: Alignment.center,
      child: selected
          ? AppIcon(AppIcons.check, size: 14, color: colors.icon.inverse)
          : null,
    );
  }
}

/// The owner's explicit "I checked": Continue stays off until it's ticked.
class _ConfirmBox extends StatelessWidget {
  const _ConfirmBox({
    required this.checked,
    required this.label,
    required this.onTap,
  });
  final bool checked;
  final String label;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return AppTappable(
      onTap: onTap,
      semanticsLabel: '$label${checked ? ', checked' : ''}',
      child: ExcludeSemantics(
        child: Container(
          padding: const EdgeInsets.all(AppSpacing.sm),
          decoration: BoxDecoration(
            color: colors.background.ground,
            borderRadius: BorderRadius.circular(AppRadii.medium),
            border: Border.all(
              color: checked ? colors.border.strong : colors.border.subtle,
              width: checked ? 1.5 : 1,
            ),
          ),
          child: Row(
            children: [
              Container(
                width: 24,
                height: 24,
                decoration: BoxDecoration(
                  borderRadius: BorderRadius.circular(6),
                  color: checked
                      ? colors.background.inverse
                      : colors.background.neutralSubtleOpacity,
                ),
                alignment: Alignment.center,
                child: checked
                    ? AppIcon(
                        AppIcons.check,
                        size: 14,
                        color: colors.icon.inverse,
                      )
                    : null,
              ),
              const SizedBox(width: AppSpacing.s),
              Expanded(
                child: Text(
                  label,
                  style: AppTypography.bodyMediumStrong.copyWith(
                    color: colors.text.accent,
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
