import 'dart:convert';

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../core/config/network_config.dart';
import '../../core/errors/zafe_error_copy.dart';
import '../../core/formatting/member_label.dart';
import '../../core/formatting/zec_amount.dart';
import '../../core/layout/mobile/app_mobile_sheet.dart';
import '../../core/layout/mobile/mobile_top_nav.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/app_button.dart';
import '../../core/widgets/app_icon.dart';
import '../../core/widgets/app_tappable.dart';
import '../../core/widgets/decimal_amount_input_formatter.dart';
import '../../core/widgets/app_toast.dart';
import '../../core/widgets/mobile/mobile_address_verify_sheet.dart';
import '../../core/widgets/mobile/mobile_review_row.dart';
import '../../core/widgets/mobile/mobile_surface_card.dart';
import '../../core/widgets/mobile/zafe_detail.dart';
import '../../core/widgets/mobile/mobile_tx_fee_info_sheet.dart';
import '../../core/widgets/mobile_text_field.dart';
import '../../providers/privacy_mode_provider.dart';
import '../../providers/proposals_provider.dart';
import '../../core/privacy/amount_display.dart';
import '../../core/security/unlock_gate.dart';
import '../../providers/vault_provider.dart';
import '../../rust/api/proposals.dart' as rust;
import '../proposals/proposal_parts.dart' show RecipientsCard;
import 'recipients_csv.dart';

enum _Step { recipient, amount, review }

/// Payments to propose again (an expired proposal), or from a `zcash:` payment link:
/// opens the review step prefilled, or the amount step when the link left it out.
class SendPrefill {
  const SendPrefill({
    required this.payments,
    required this.autoSend,
    this.fromLink = false,
  });
  final List<rust.PaymentInput> payments;
  final bool autoSend;

  /// From a payment link (the review card says "payment request").
  final bool fromLink;
}

/// Most recipients in one proposal (spec: batch payments, 1..50).
const kMaxRecipients = 50;

/// Plain decimal ZEC for an amount field ("1.5"), the inverse of `parseZec`.
String zecDecimal(BigInt zat) {
  final whole = zat ~/ BigInt.from(100000000);
  final frac = (zat % BigInt.from(100000000))
      .toString()
      .padLeft(8, '0')
      .replaceFirst(RegExp(r'0+$'), '');
  return frac.isEmpty ? '$whole' : '$whole.$frac';
}

/// New payment: a three-step send wizard (recipient, amount, review). The last step
/// logs a proposal for the other signers instead of sending.
class SendScreen extends ConsumerStatefulWidget {
  const SendScreen({super.key, this.prefill});

  /// Starts on the review step with these details (propose again).
  final SendPrefill? prefill;

  @override
  ConsumerState<SendScreen> createState() => _SendScreenState();
}

class _SendScreenState extends ConsumerState<SendScreen> {
  var _step = _Step.recipient;
  final _address = TextEditingController();
  final _addressFocus = FocusNode();
  final _amount = TextEditingController();
  final _amountFocus = FocusNode();
  String _memo = '';
  bool _autoSend = true;
  rust.AddressCheck? _check;
  bool _busy = false;

  /// Recipients already added to this proposal; the fields hold the one being edited.
  final List<rust.PaymentInput> _added = [];

  @override
  void initState() {
    super.initState();
    final prefill = widget.prefill;
    if (prefill != null && prefill.payments.isNotEmpty) {
      _added.addAll(prefill.payments.take(prefill.payments.length - 1));
      _edit(prefill.payments.last);
      _autoSend = prefill.autoSend;
      final allValid = prefill.payments.every(
        (p) => rust
            .checkAddress(networkName: kZafeNetwork, address: p.address)
            .valid,
      );
      final complete = prefill.payments.every((p) => p.amountZat > BigInt.zero);
      if (allValid) _step = complete ? _Step.review : _Step.amount;
    }
  }

  @override
  void dispose() {
    _address.dispose();
    _addressFocus.dispose();
    _amount.dispose();
    _amountFocus.dispose();
    super.dispose();
  }

  /// Field length at the last change, to tell a paste (many characters at once) from
  /// typing.
  int _lastLength = 0;

  void _validateAddress() {
    final text = _address.text.trim();
    final pasted = _address.text.length - _lastLength > 1;
    _lastLength = _address.text.length;
    // A payment link pasted into the field (keyboard paste skips `_paste`). Typed links
    // aren't applied as they grow: `zcash:<address>` parses before `?amount=` is typed.
    if (pasted && text.toLowerCase().startsWith('zcash:')) {
      final request = rust.parsePaymentRequest(
        networkName: kZafeNetwork,
        text: text,
      );
      if (request.problem.isEmpty) return _apply(request);
    }
    setState(
      () => _check = text.isEmpty
          ? null
          : rust.checkAddress(networkName: kZafeNetwork, address: text),
    );
  }

  Future<void> _paste() async {
    final data = await Clipboard.getData(Clipboard.kTextPlain);
    final text = data?.text?.trim();
    if (text == null || text.isEmpty) return;
    if (text.toLowerCase().startsWith('zcash:')) {
      final request = rust.parsePaymentRequest(
        networkName: kZafeNetwork,
        text: text,
      );
      if (request.problem.isEmpty) return _apply(request);
    }
    _address.text = text;
    _validateAddress();
  }

  Future<void> _scan() async {
    final request = await context.push<rust.ScannedRequest>('/scan-recipient');
    if (request != null && mounted) _apply(request);
  }

  /// Fills the draft from a scanned or pasted request: one recipient goes on to the
  /// amount (prefilled when the request names one); several become a batch to review.
  void _apply(rust.ScannedRequest request) {
    final payments = request.payments;
    final room = kMaxRecipients - _added.length;
    if (payments.isEmpty || room <= 0) return;
    setState(() {
      final take = payments.take(room).toList();
      for (final p in take.take(take.length - 1)) {
        _added.add(
          rust.PaymentInput(
            address: p.address,
            amountZat: p.amountZat,
            memo: p.memo,
          ),
        );
      }
      final last = take.last;
      _address.text = last.address;
      _amount.text = last.amountZat == BigInt.zero
          ? ''
          : zecDecimal(last.amountZat);
      _memo = last.memo;
      _validateAddress();
      final complete = take.every((p) => p.amountZat > BigInt.zero);
      _step = take.length > 1 && complete ? _Step.review : _Step.amount;
    });
  }

  BigInt? get _amountZat => rust.parseZec(text: _amount.text);

  /// The payment being edited.
  rust.PaymentInput get _current => rust.PaymentInput(
    address: _address.text.trim(),
    amountZat: _amountZat ?? BigInt.zero,
    memo: _memo,
  );

  /// Every payment of the proposal, in order.
  List<rust.PaymentInput> get _payments => [..._added, _current];

  BigInt get _addedTotal =>
      _added.fold(BigInt.zero, (sum, p) => sum + p.amountZat);

  /// Puts `p` in the fields.
  void _edit(rust.PaymentInput p) {
    _address.text = p.address;
    _amount.text = p.amountZat == BigInt.zero ? '' : zecDecimal(p.amountZat);
    _memo = p.memo;
    _check = rust.checkAddress(networkName: kZafeNetwork, address: p.address);
  }

  /// Parks the current payment and starts a new recipient.
  void _addRecipient() {
    setState(() {
      _added.add(_current);
      _address.clear();
      _amount.clear();
      _memo = '';
      _check = null;
      _step = _Step.recipient;
    });
  }

  /// Drops payment `i` of [_payments] (not the last remaining one).
  void _removeRecipient(int i) {
    setState(() {
      if (i < _added.length) {
        _added.removeAt(i);
      } else if (_added.isNotEmpty) {
        _edit(_added.removeLast());
      }
    });
  }

  /// Adds recipients from a CSV file (address,amount[,memo]) to the batch. On the review
  /// step the payment being edited stays; on the recipient step the file replaces it.
  /// Nothing is added unless every row is valid.
  Future<void> _importCsv() async {
    FilePickerResult? picked;
    try {
      picked = await FilePicker.pickFiles(withData: true);
    } catch (_) {
      picked = null;
    }
    final bytes = picked?.files.singleOrNull?.bytes;
    if (bytes == null || !mounted) return;
    void fail(String message) => showAppToast(
      context,
      message,
      iconName: AppIcons.warningCircle,
      tone: AppToastTone.destructive,
    );
    if (bytes.length > 1024 * 1024) {
      return fail('That file is too big for a list of recipients');
    }
    final String text;
    try {
      text = utf8.decode(bytes);
    } on FormatException {
      return fail('That file isn\'t a CSV text file');
    }
    final onReview = _step == _Step.review;
    final keep = onReview ? _payments : List.of(_added);
    final result = parseRecipientsCsv(
      text,
      room: kMaxRecipients - keep.length,
      checkAddress: (a) =>
          rust.checkAddress(networkName: kZafeNetwork, address: a),
      parseAmount: (t) => rust.parseZec(text: t),
      memoLength: (m) => rust.memoLength(memo: m),
    );
    if (!result.ok) {
      await showAppMobileSheet<void>(
        context: context,
        builder: (_) => _CsvErrorsSheet(errors: result.errors),
      );
      return;
    }
    final all = [...keep, ...result.payments];
    setState(() {
      _added
        ..clear()
        ..addAll(all.take(all.length - 1));
      _edit(all.last);
      _step = _Step.review;
    });
    final n = result.payments.length;
    showAppToast(context, 'Added $n ${n == 1 ? 'recipient' : 'recipients'}');
  }

  BigInt get _spendable =>
      ref.read(vaultProvider).balance?.spendableZat ?? BigInt.zero;

  String get _title => switch (_step) {
    _Step.recipient => 'Select recipient',
    _Step.amount => 'Enter amount',
    _Step.review => 'Review payment',
  };

  void _back() {
    switch (_step) {
      case _Step.recipient when _added.isNotEmpty:
        // Abandon the new recipient: back to the review of the ones already added.
        setState(() {
          _edit(_added.removeLast());
          _step = _Step.review;
        });
      case _Step.recipient:
        context.pop();
      case _Step.amount:
        setState(() => _step = _Step.recipient);
      case _Step.review:
        setState(() => _step = _Step.amount);
    }
  }

  /// Paying the vault's own address would only shuffle its notes and cost a fee.
  bool _isOwn(String address) {
    final own = ref.read(vaultProvider).summary?.address;
    return own != null && own == address.trim();
  }

  Future<void> _propose() async {
    if (_payments.any((p) => _isOwn(p.address))) {
      showAppToast(
        context,
        'This payment goes to the vault\'s own address. Pick another recipient.',
        iconName: AppIcons.warningCircle,
        tone: AppToastTone.destructive,
      );
      return;
    }
    if (!await confirmUnlock(
      context,
      ref,
      reason: 'Unlock to propose this payment',
    )) {
      return;
    }
    setState(() => _busy = true);
    try {
      final id = await ref
          .read(proposalsProvider.notifier)
          .propose(payments: _payments, autoSend: _autoSend);
      if (!mounted) return;
      context.go('/home');
      context.push('/proposal/$id');
    } catch (e) {
      debugPrint('propose failed: ${describeError(e)}');
      if (mounted) {
        showAppToast(
          context,
          zafeErrorMessage(
            e,
            fallback: 'Couldn\'t create the proposal. Try again.',
          ),
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
    return PopScope(
      canPop: _step == _Step.recipient && _added.isEmpty && !_busy,
      onPopInvokedWithResult: (didPop, _) {
        if (!didPop && !_busy) _back();
      },
      child: Scaffold(
        backgroundColor: colors.background.window,
        resizeToAvoidBottomInset: true,
        body: AppToastHost(
          child: SafeArea(
            child: Column(
              children: [
                MobileTopNav.back(title: _title, onBack: _busy ? null : _back),
                Expanded(
                  child: switch (_step) {
                    _Step.recipient => _recipientStep(),
                    _Step.amount => _amountStep(),
                    _Step.review => _reviewStep(),
                  },
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }

  // --- Step 1 --------------------------------------------------------------------------

  Widget _recipientStep() {
    final colors = context.colors;
    final check = _check;
    final own = check?.valid == true && _isOwn(_address.text);
    final invalid = own || (check != null && !check.valid);
    final filled = _address.text.trim().isNotEmpty;
    return Column(
      children: [
        Expanded(
          child: ListView(
            padding: const EdgeInsets.fromLTRB(16, 12, 16, 24),
            children: [
              MobileTextField(
                controller: _address,
                focusNode: _addressFocus,
                hintText: 'Zcash address',
                onChanged: (_) => _validateAddress(),
                restingBorderColor: invalid ? colors.border.destructive : null,
                leading: AppIcon(
                  AppIcons.plane,
                  size: 20,
                  color: filled ? colors.icon.accent : colors.icon.regular,
                ),
                trailing: _Pill(
                  label: filled ? 'Clear' : 'Paste',
                  onTap: filled
                      ? () {
                          _address.clear();
                          _validateAddress();
                        }
                      : _paste,
                ),
              ),
              if (invalid)
                Padding(
                  padding: const EdgeInsets.only(
                    top: AppSpacing.xs,
                    left: AppSpacing.xxs,
                  ),
                  child: Text(
                    own
                        ? 'This is the vault\'s own address. Enter a recipient.'
                        : check!.reason,
                    style: AppTypography.labelLarge.copyWith(
                      color: colors.text.destructive,
                    ),
                  ),
                ),
              const SizedBox(height: AppSpacing.base),
              Row(
                children: [
                  AppIcon(
                    AppIcons.shieldKeyhole,
                    size: 16,
                    color: colors.icon.muted,
                  ),
                  const SizedBox(width: AppSpacing.xs),
                  Expanded(
                    child: Text(
                      'Vault payments are always shielded. Paste a unified address.',
                      style: AppTypography.bodySmall.copyWith(
                        color: colors.text.secondary,
                      ),
                    ),
                  ),
                ],
              ),
            ],
          ),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 0, 16, AppSpacing.s),
          child: AppButton(
            expand: true,
            variant: AppButtonVariant.secondary,
            leading: const AppIcon(AppIcons.qr, size: 20),
            onPressed: _scan,
            child: const Text('Scan QR code'),
          ),
        ),
        if (_added.length < kMaxRecipients - 1)
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 0, 16, AppSpacing.s),
            child: AppButton(
              expand: true,
              variant: AppButtonVariant.ghost,
              leading: const AppIcon(AppIcons.importWallet, size: 20),
              onPressed: _importCsv,
              child: const Text('Import recipients from CSV'),
            ),
          ),
        _Cta(
          label: filled ? 'Continue' : 'Enter address to continue',
          onPressed: check?.valid == true && !own
              ? () => setState(() => _step = _Step.amount)
              : null,
        ),
      ],
    );
  }

  // --- Step 2 --------------------------------------------------------------------------

  Widget _amountStep() {
    final colors = context.colors;
    final amount = _amountZat;
    final tooMuch = amount != null && _addedTotal + amount > _spendable;
    final memoLen = rust.memoLength(memo: _memo);
    final label = amount == null || amount == BigInt.zero
        ? 'Enter amount to continue'
        : tooMuch
        ? 'Not enough $kZcashDefaultCurrencyTicker'
        : 'Finish & review';
    return Column(
      children: [
        Expanded(
          child: ListView(
            padding: const EdgeInsets.fromLTRB(16, 16, 16, 24),
            children: [
              SizedBox(
                height: 44,
                child: Row(
                  children: [
                    const ZecCoin(),
                    const SizedBox(width: AppSpacing.xs),
                    Expanded(
                      child: Text(
                        '${amountWithTicker(ZecAmount.fromZatoshi(_spendable - _addedTotal).balance.amountText, hide: ref.watch(privacyModeProvider))} available',
                        style: AppTypography.labelLarge.copyWith(
                          color: colors.text.accent,
                          fontWeight: FontWeight.w600,
                        ),
                      ),
                    ),
                  ],
                ),
              ),
              const SizedBox(height: AppSpacing.md),
              SizedBox(
                height: 64,
                child: Row(
                  mainAxisAlignment: MainAxisAlignment.center,
                  crossAxisAlignment: CrossAxisAlignment.baseline,
                  textBaseline: TextBaseline.alphabetic,
                  children: [
                    IntrinsicWidth(
                      child: TextField(
                        controller: _amount,
                        focusNode: _amountFocus,
                        autofocus: true,
                        keyboardType: const TextInputType.numberWithOptions(
                          decimal: true,
                        ),
                        inputFormatters: [
                          DecimalAmountInputFormatter(
                            maxFractionDigits: 8,
                            maxLength: 17,
                          ),
                        ],
                        onChanged: (_) => setState(() {}),
                        textAlign: TextAlign.end,
                        cursorColor: colors.text.accent,
                        style: TextStyle(
                          fontFamily: 'Space Grotesk',
                          fontSize: 48,
                          height: 1.1,
                          color: tooMuch
                              ? colors.text.destructive
                              : colors.text.accent,
                        ),
                        decoration: InputDecoration(
                          border: InputBorder.none,
                          isCollapsed: true,
                          hintText: '0',
                          hintStyle: TextStyle(
                            fontFamily: 'Space Grotesk',
                            fontSize: 48,
                            color: colors.text.disabled,
                          ),
                        ),
                      ),
                    ),
                    Text(
                      ' $kZcashDefaultCurrencyTicker',
                      style: TextStyle(
                        fontFamily: 'Space Grotesk',
                        fontSize: 38,
                        color: colors.text.accent.withValues(alpha: 0.5),
                      ),
                    ),
                  ],
                ),
              ),
              const SizedBox(height: AppSpacing.xs),
              Center(
                child: Text(
                  'Network fee is added when the transaction is built',
                  style: AppTypography.bodySmall.copyWith(
                    color: colors.text.muted,
                  ),
                ),
              ),
              const SizedBox(height: AppSpacing.md),
              Text(
                'Sending to',
                style: AppTypography.labelMedium.copyWith(
                  color: colors.text.secondary,
                ),
              ),
              const SizedBox(height: AppSpacing.xs),
              _AddressRow(address: _address.text.trim()),
              const SizedBox(height: AppSpacing.s),
              AppTappable(
                onTap: _editMemo,
                child: SizedBox(
                  height: 44,
                  child: Row(
                    children: [
                      AppIcon(
                        AppIcons.edit,
                        size: 20,
                        color: colors.icon.regular,
                      ),
                      const SizedBox(width: AppSpacing.xs),
                      Expanded(
                        child: Text(
                          _memo.isEmpty ? 'Add short encrypted message' : _memo,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: AppTypography.labelLarge.copyWith(
                            color: colors.text.primary,
                          ),
                        ),
                      ),
                      if (_memo.isNotEmpty)
                        Text(
                          '$memoLen/512',
                          style: AppTypography.labelMedium.copyWith(
                            color: colors.text.muted,
                          ),
                        ),
                    ],
                  ),
                ),
              ),
            ],
          ),
        ),
        _Cta(
          label: label,
          onPressed: amount != null && amount > BigInt.zero && !tooMuch
              ? () {
                  FocusScope.of(context).unfocus();
                  setState(() => _step = _Step.review);
                }
              : null,
        ),
      ],
    );
  }

  Future<void> _editMemo() async {
    final result = await showAppMobileSheet<String>(
      context: context,
      builder: (sheetContext) => _MemoSheet(initial: _memo),
    );
    if (result != null) setState(() => _memo = result);
  }

  // --- Step 3 --------------------------------------------------------------------------

  Widget _reviewStep() {
    final colors = context.colors;
    final summary = ref.watch(vaultProvider).summary!;
    final payments = _payments;
    final total = payments.fold(BigInt.zero, (sum, p) => sum + p.amountZat);
    final amount = ZecAmount.fromZatoshi(total).receipt;
    final address = payments.first.address;
    final batch = payments.length > 1;
    return ListView(
      padding: const EdgeInsets.fromLTRB(16, 12, 16, 12),
      children: [
        PaymentCard(
          label: widget.prefill?.fromLink ?? false
              ? (batch ? 'BATCH PAYMENT REQUEST' : 'PAYMENT REQUEST')
              : (batch ? 'NEW BATCH PAYMENT' : 'NEW PAYMENT'),
          amountText: amount.amountText,
          address: address,
          recipients: payments.length,
          onFullAddress: () => showMobileAddressVerifySheet(
            context,
            title: 'Full address',
            address: address,
          ),
        ),
        if (batch) ...[
          const SizedBox(height: AppSpacing.md),
          RecipientsCard.fromInputs(
            payments,
            onRemove: _busy ? null : _removeRecipient,
          ),
        ],
        const SizedBox(height: AppSpacing.s),
        AppButton(
          expand: true,
          variant: AppButtonVariant.secondary,
          leading: const AppIcon(AppIcons.plus, size: 20),
          onPressed: _busy || payments.length >= kMaxRecipients
              ? null
              : _addRecipient,
          child: Text(
            payments.length >= kMaxRecipients
                ? '$kMaxRecipients recipients at most'
                : 'Add another recipient',
          ),
        ),
        if (payments.length < kMaxRecipients) ...[
          const SizedBox(height: AppSpacing.xs),
          AppButton(
            expand: true,
            variant: AppButtonVariant.ghost,
            leading: const AppIcon(AppIcons.importWallet, size: 20),
            onPressed: _busy ? null : _importCsv,
            child: const Text('Import CSV'),
          ),
        ],
        const SizedBox(height: AppSpacing.lg),
        MobileSurfaceCard(
          cornerRadius: AppRadii.large,
          padding: const EdgeInsets.symmetric(
            horizontal: AppSpacing.sm,
            vertical: AppSpacing.lg,
          ),
          child: Column(
            children: [
              if (!batch) ...[
                DetailRow(
                  label: 'Message',
                  value: _memo.isEmpty ? 'None' : _memo,
                ),
                const DetailDivider(),
              ],
              DetailRow(
                label: 'Approvals needed',
                value:
                    '${summary.threshold} of ${summary.members.length} signers',
              ),
              const DetailDivider(),
              DetailRow(
                label: 'Tx fee',
                value: 'Set by ZIP 317',
                trailing: AppTappable(
                  onTap: () => showMobileTxFeeInfoSheet(context),
                  semanticsLabel: 'About the network fee',
                  child: AppIcon(
                    AppIcons.help,
                    size: 20,
                    color: colors.icon.muted,
                  ),
                ),
              ),
              const DetailDivider(),
              MergeSemantics(
                child: Row(
                  children: [
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            'Send when approved',
                            style: AppTypography.labelLarge.copyWith(
                              color: colors.text.accent,
                            ),
                          ),
                          const SizedBox(height: AppSpacing.xxs),
                          Text(
                            _autoSend
                                ? 'The last signer to approve sends it right away.'
                                : 'A signer sends it when you choose.',
                            style: AppTypography.bodySmall.copyWith(
                              color: colors.text.secondary,
                            ),
                          ),
                        ],
                      ),
                    ),
                    Switch(
                      value: _autoSend,
                      onChanged: _busy
                          ? null
                          : (v) => setState(() => _autoSend = v),
                      // Brand fill when on, muted when off: the default Material colours
                      // made the two states hard to tell apart.
                      activeThumbColor: colors.button.primary.label,
                      activeTrackColor: colors.button.primary.bg,
                      inactiveThumbColor: colors.icon.muted,
                      inactiveTrackColor: colors.background.raised,
                      trackOutlineColor: WidgetStatePropertyAll(
                        colors.border.regular,
                      ),
                    ),
                  ],
                ),
              ),
            ],
          ),
        ),
        const SizedBox(height: AppSpacing.sm),
        Text(
          'Nothing is sent yet. Each signer checks this payment on their own device and '
          'approves with one tap; ${summary.threshold} approvals complete it.',
          style: AppTypography.bodySmall.copyWith(color: colors.text.secondary),
        ),
        const SizedBox(height: AppSpacing.lg),
        AppButton(
          expand: true,
          leading: _busy ? null : const AppIcon(AppIcons.plane, size: 20),
          onPressed: _busy ? null : _propose,
          child: Text(_busy ? 'Building transaction...' : 'Propose payment'),
        ),
        const SizedBox(height: AppSpacing.s),
        AppButton(
          expand: true,
          variant: AppButtonVariant.ghost,
          onPressed: _busy ? null : () => context.go('/home'),
          child: const Text('Cancel'),
        ),
      ],
    );
  }
}

// --- Pieces ------------------------------------------------------------------------------

class _Cta extends StatelessWidget {
  const _Cta({required this.label, required this.onPressed});
  final String label;
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.fromLTRB(16, 0, 16, 12),
    child: AppButton(expand: true, onPressed: onPressed, child: Text(label)),
  );
}

class _Pill extends StatelessWidget {
  const _Pill({required this.label, required this.onTap});
  final String label;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return AppTappable(
      onTap: onTap,
      child: Container(
        height: 36,
        padding: const EdgeInsets.symmetric(horizontal: AppSpacing.sm),
        alignment: Alignment.center,
        decoration: BoxDecoration(
          color: colors.background.neutralSubtleOpacity,
          borderRadius: BorderRadius.circular(10),
        ),
        child: Text(
          label,
          style: AppTypography.labelLarge.copyWith(color: colors.text.accent),
        ),
      ),
    );
  }
}

class _AddressRow extends StatelessWidget {
  const _AddressRow({required this.address});
  final String address;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return SizedBox(
      height: 68,
      child: Row(
        children: [
          MobileReviewIconBadge(
            child: AppIcon(
              AppIcons.wallet,
              size: 20,
              color: colors.icon.regular,
            ),
          ),
          const SizedBox(width: AppSpacing.s),
          Expanded(
            child: Column(
              mainAxisAlignment: MainAxisAlignment.center,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  'Shielded address',
                  style: AppTypography.labelLarge.copyWith(
                    color: colors.text.accent,
                    fontWeight: FontWeight.w600,
                  ),
                ),
                const SizedBox(height: AppSpacing.xxs),
                Text(
                  compactAddress(address),
                  style: AppTypography.labelMedium.copyWith(
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

/// Why a recipients file couldn't be imported, row by row.
class _CsvErrorsSheet extends StatelessWidget {
  const _CsvErrorsSheet({required this.errors});
  final List<String> errors;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 24, 16, 16),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            'Couldn\'t import this file',
            style: AppTypography.bodyLarge.copyWith(
              color: colors.text.accent,
              fontWeight: FontWeight.w600,
            ),
          ),
          const SizedBox(height: AppSpacing.xs),
          Text(
            'Nothing was added. Fix these rows and import again. Each row is '
            'address,amount,memo with the amount in ZEC (like 1.5); the memo is optional.',
            style: AppTypography.bodySmall.copyWith(
              color: colors.text.secondary,
            ),
          ),
          const SizedBox(height: AppSpacing.s),
          for (final e in errors)
            Padding(
              padding: const EdgeInsets.only(bottom: AppSpacing.xxs),
              child: Text(
                e,
                style: AppTypography.labelLarge.copyWith(
                  color: colors.text.destructive,
                ),
              ),
            ),
          const SizedBox(height: AppSpacing.md),
          AppButton(
            expand: true,
            variant: AppButtonVariant.secondary,
            onPressed: () => Navigator.of(context).pop(),
            child: const Text('OK'),
          ),
        ],
      ),
    );
  }
}

class _MemoSheet extends StatefulWidget {
  const _MemoSheet({required this.initial});
  final String initial;

  @override
  State<_MemoSheet> createState() => _MemoSheetState();
}

class _MemoSheetState extends State<_MemoSheet> {
  late final _text = TextEditingController(text: widget.initial);

  @override
  void dispose() {
    _text.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final len = rust.memoLength(memo: _text.text);
    final tooLong = len > 512;
    return Padding(
      padding: EdgeInsets.fromLTRB(
        16,
        24,
        16,
        16 + MediaQuery.viewInsetsOf(context).bottom,
      ),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              Expanded(
                child: Text(
                  'Message',
                  style: AppTypography.bodyLarge.copyWith(
                    color: colors.text.accent,
                    fontWeight: FontWeight.w600,
                  ),
                ),
              ),
              Text(
                '$len/512',
                style: AppTypography.labelMedium.copyWith(
                  color: tooLong ? colors.text.destructive : colors.text.muted,
                ),
              ),
            ],
          ),
          const SizedBox(height: AppSpacing.s),
          Container(
            height: 148,
            padding: const EdgeInsets.all(AppSpacing.s),
            decoration: BoxDecoration(
              color: colors.background.ground,
              borderRadius: BorderRadius.circular(12),
            ),
            child: TextField(
              controller: _text,
              autofocus: true,
              maxLines: null,
              expands: true,
              onChanged: (_) => setState(() {}),
              style: AppTypography.bodyMedium.copyWith(
                color: colors.text.primary,
              ),
              decoration: InputDecoration(
                border: InputBorder.none,
                isCollapsed: true,
                hintText: 'Only the recipient can read this',
                hintStyle: AppTypography.bodyMedium.copyWith(
                  color: colors.text.disabled,
                ),
              ),
            ),
          ),
          if (tooLong)
            Padding(
              padding: const EdgeInsets.only(top: AppSpacing.xs),
              child: Text(
                'Message is too long',
                style: AppTypography.labelLarge.copyWith(
                  color: colors.text.destructive,
                ),
              ),
            ),
          const SizedBox(height: AppSpacing.md),
          AppButton(
            expand: true,
            onPressed: tooLong
                ? null
                : () => Navigator.of(context).pop(_text.text.trim()),
            child: const Text('Save message'),
          ),
          const SizedBox(height: AppSpacing.xs),
          AppButton(
            expand: true,
            variant: AppButtonVariant.ghost,
            onPressed: () => Navigator.of(context).pop(),
            child: const Text('Cancel'),
          ),
        ],
      ),
    );
  }
}
