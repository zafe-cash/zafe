import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:url_launcher/url_launcher.dart';

import '../../core/config/relay_presets.dart';
import '../../core/errors/sync_failure.dart';
import '../../core/errors/zafe_error_copy.dart';
import '../../core/layout/mobile/zafe_screen.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/app_button.dart';
import '../../core/widgets/app_icon.dart';
import '../../core/widgets/app_tappable.dart';
import '../../core/widgets/mobile/mobile_list_row.dart';
import '../../core/widgets/mobile/mobile_surface_card.dart';
import '../../core/widgets/mobile_text_field.dart';
import '../../rust/api/endpoints.dart' as rust;

/// The relay picked for a vault, and its create token when the relay is private.
class RelayChoice {
  const RelayChoice(this.url, {this.token});
  final String url;
  final String? token;
}

/// What the selected relay answered: `info` once it answered, `error` (user copy) when it
/// couldn't be used; neither while checking.
class RelayCheckResult {
  const RelayCheckResult.checking() : info = null, error = null;
  const RelayCheckResult.ok(rust.RelayCheck this.info) : error = null;
  const RelayCheckResult.failed(String this.error) : info = null;

  final rust.RelayCheck? info;
  final String? error;

  bool get checking => info == null && error == null;
  bool get tooOld => info?.tooOld ?? false;
  bool get usable => info != null && !info!.tooOld;
}

/// What the server list opens with (the `/pick-server` route's extra).
class RelayPickerArgs {
  const RelayPickerArgs({
    required this.title,
    required this.selectedUrl,
    this.currentUrl,
    this.token,
    this.askToken = true,
    this.confirmLabel = 'Use this server',
  });

  final String title;
  final String selectedUrl;
  final String? currentUrl;
  final String? token;
  final bool askToken;
  final String confirmLabel;
}

/// Opens the server list as a full screen. Returns the choice on "Use this server",
/// null on Back. `askToken`: offer the create-token field when the server needs one
/// (creating a vault). `currentUrl`: the server the vault is on (moving it), shown but
/// never chosen.
Future<RelayChoice?> showRelayPicker(
  BuildContext context, {
  required String title,
  required String selectedUrl,
  String? currentUrl,
  String? token,
  bool askToken = true,
  String confirmLabel = 'Use this server',
}) => context.push<RelayChoice>(
  '/pick-server',
  extra: RelayPickerArgs(
    title: title,
    selectedUrl: selectedUrl,
    currentUrl: currentUrl,
    token: token,
    askToken: askToken,
    confirmLabel: confirmLabel,
  ),
);

/// Checks the one relay `url` (never the list) and turns a failure into user copy.
Future<RelayCheckResult> checkRelayAt(String url) async {
  try {
    return RelayCheckResult.ok(await rust.checkRelay(relayUrl: url));
  } catch (e) {
    final failure = classifySyncFailure(e, fallback: SyncEndpoint.relay);
    return RelayCheckResult.failed(switch (failure.kind) {
      SyncFailureKind.relayUnreachable ||
      SyncFailureKind.offline => 'Couldn\'t connect.',
      SyncFailureKind.tls => 'Its certificate isn\'t trusted.',
      SyncFailureKind.timeout => 'No answer in time.',
      SyncFailureKind.updateRequired ||
      SyncFailureKind.torConnecting ||
      SyncFailureKind.torFailed => zafeErrorMessage(e),
      _ => 'Not a Zafe server.',
    });
  }
}

/// The server list: Zafe's servers, "Other address" (typed, pasted or scanned), what the
/// selected one says about itself, and its create token when it needs one.
class RelayPickerScreen extends ConsumerStatefulWidget {
  const RelayPickerScreen({
    super.key,
    required this.title,
    required this.initialUrl,
    this.currentUrl,
    this.initialToken,
    this.askToken = true,
    this.confirmLabel = 'Use this server',
    this.check = checkRelayAt,
  });

  final String title;
  final String initialUrl;
  final String? currentUrl;

  /// Asks one relay what it is ([checkRelayAt]; replaced in tests).
  final Future<RelayCheckResult> Function(String url) check;
  final String? initialToken;
  final bool askToken;
  final String confirmLabel;

  @override
  ConsumerState<RelayPickerScreen> createState() => _RelayPickerScreenState();
}

class _RelayPickerScreenState extends ConsumerState<RelayPickerScreen> {
  late String _url = widget.initialUrl;
  RelayCheckResult _result = const RelayCheckResult.checking();
  late final _other = TextEditingController(
    text: relayPresetFor(widget.initialUrl) == null
        ? Uri.tryParse(widget.initialUrl)?.authority ?? ''
        : '',
  );
  final _otherFocus = FocusNode();
  late final _token = TextEditingController(text: widget.initialToken ?? '');
  final _tokenFocus = FocusNode();
  late bool _otherOpen = relayPresetFor(widget.initialUrl) == null;
  String? _otherError;

  /// "Use this server" was tapped with an unchecked typed address: confirm once it checks out.
  bool _confirmWhenChecked = false;

  /// Bumped on every selection, so a slow answer for an earlier one is dropped.
  int _generation = 0;

  @override
  void initState() {
    super.initState();
    _other.addListener(_otherChanged);
    _check();
  }

  void _otherChanged() => setState(() {});

  @override
  void dispose() {
    _other.dispose();
    _otherFocus.dispose();
    _token.dispose();
    _tokenFocus.dispose();
    super.dispose();
  }

  Future<void> _check() async {
    final generation = ++_generation;
    final url = _url;
    setState(() => _result = const RelayCheckResult.checking());
    final result = await widget.check(url);
    if (!mounted || generation != _generation) return;
    setState(() => _result = result);
    if (_confirmWhenChecked) {
      _confirmWhenChecked = false;
      if (_ready) _confirm();
    }
  }

  void _select(String url, {String? token}) {
    setState(() {
      _url = url;
      _otherError = null;
      if (token != null) _token.text = token;
    });
    _check();
  }

  void _pickPreset(RelayPreset preset) {
    _otherOpen = false;
    _select(preset.url);
  }

  void _submitOther(String text) {
    final input = parseRelayInput(text);
    if (input.url == null) {
      _confirmWhenChecked = false;
      setState(() => _otherError = input.error);
      return;
    }
    _other.text = Uri.tryParse(input.url!)?.authority ?? input.url!;
    _select(input.url!, token: input.token);
  }

  Future<void> _scan() async {
    final scanned = await context.push<String>('/scan-relay');
    if (scanned == null || !mounted) return;
    setState(() => _otherOpen = true);
    _submitOther(scanned);
  }

  Future<void> _paste() async {
    final data = await Clipboard.getData(Clipboard.kTextPlain);
    final text = data?.text?.trim();
    if (text == null || text.isEmpty || !mounted) return;
    _submitOther(text);
  }

  bool get _needsToken =>
      widget.askToken && (_result.info?.createTokenRequired ?? false);

  bool get _isCurrent {
    final current = widget.currentUrl;
    return current != null && sameRelayUrl(_url, current);
  }

  /// The typed address isn't the selected one yet (the keyboard was closed without Done).
  bool get _otherPending {
    final text = _other.text.trim();
    return _otherOpen &&
        text.isNotEmpty &&
        text != relayHost(_url) &&
        text != _url;
  }

  bool get _ready =>
      _result.usable &&
      !_isCurrent &&
      (!_needsToken || _token.text.trim().isNotEmpty);

  void _confirm() {
    final token = _token.text.trim();
    Navigator.of(
      context,
    ).pop(RelayChoice(_url, token: _needsToken ? token : null));
  }

  @override
  Widget build(BuildContext context) {
    return ZafeScreen(
      title: widget.title,
      bottom: RelayPickerActions(
        confirmLabel: widget.confirmLabel,
        onRetry: _result.error != null && !_otherPending && !_isCurrent
            ? _check
            : null,
        onConfirm: _otherPending
            ? () {
                _confirmWhenChecked = true;
                _submitOther(_other.text);
              }
            : _ready
            ? _confirm
            : null,
        onRunYourOwn: () => launchUrl(
          Uri.parse(kRunYourOwnRelayUrl),
          mode: LaunchMode.externalApplication,
        ),
      ),
      children: [
        RelayPickerBody(
          presets: kRelayPresets,
          selectedUrl: _url,
          selectedIsCurrent: _isCurrent,
          result: _result,
          otherOpen: _otherOpen,
          other: _other,
          otherFocus: _otherFocus,
          otherError: _otherError,
          token: _needsToken ? _token : null,
          tokenFocus: _tokenFocus,
          onSelect: _pickPreset,
          onOther: () => setState(() {
            _otherOpen = true;
            _otherFocus.requestFocus();
          }),
          onOtherSubmit: _submitOther,
          onOtherPaste: _paste,
          onScan: _scan,
          onTokenChanged: (_) => setState(() {}),
        ),
      ],
    );
  }
}

/// The server list's buttons: use the selected one (or try it again when it didn't
/// answer), and the way to run one's own.
class RelayPickerActions extends StatelessWidget {
  const RelayPickerActions({
    super.key,
    required this.confirmLabel,
    this.onConfirm,
    this.onRetry,
    this.onRunYourOwn,
  });

  final String confirmLabel;
  final VoidCallback? onConfirm;
  final VoidCallback? onRetry;
  final VoidCallback? onRunYourOwn;

  @override
  Widget build(BuildContext context) => Column(
    mainAxisSize: MainAxisSize.min,
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      if (onRetry != null)
        AppButton(
          expand: true,
          variant: AppButtonVariant.secondary,
          onPressed: onRetry,
          child: const Text('Try again'),
        )
      else
        AppButton(
          expand: true,
          onPressed: onConfirm,
          child: Text(confirmLabel),
        ),
      const SizedBox(height: AppSpacing.xs),
      AppButton(
        expand: true,
        variant: AppButtonVariant.ghost,
        onPressed: onRunYourOwn,
        child: const Text('Run your own server →'),
      ),
    ],
  );
}

/// The server list's content from plain data (rendered without Rust in previews): the
/// listed servers and "Other address" in one card, the typed address, what the selected
/// server said, and the create token when it needs one.
class RelayPickerBody extends StatelessWidget {
  const RelayPickerBody({
    super.key,
    required this.presets,
    required this.selectedUrl,
    this.selectedIsCurrent = false,
    required this.result,
    required this.otherOpen,
    required this.other,
    required this.otherFocus,
    this.otherError,
    this.token,
    this.tokenFocus,
    this.onSelect,
    this.onOther,
    this.onOtherSubmit,
    this.onOtherPaste,
    this.onScan,
    this.onTokenChanged,
  });

  final List<RelayPreset> presets;
  final String selectedUrl;

  /// The selected relay is the one the vault is on (moving it): shown as current.
  final bool selectedIsCurrent;
  final RelayCheckResult result;
  final bool otherOpen;
  final TextEditingController other;
  final FocusNode otherFocus;
  final String? otherError;

  /// The create-token field, when the selected relay needs one.
  final TextEditingController? token;
  final FocusNode? tokenFocus;
  final ValueChanged<RelayPreset>? onSelect;
  final VoidCallback? onOther;
  final ValueChanged<String>? onOtherSubmit;
  final VoidCallback? onOtherPaste;
  final VoidCallback? onScan;
  final ValueChanged<String>? onTokenChanged;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final custom = relayPresetFor(selectedUrl, presets) == null;
    Widget mark(bool selected) => SizedBox(
      width: 20,
      child: selected
          ? AppIcon(AppIcons.check, size: 20, color: colors.icon.accent)
          : null,
    );
    // Only the selected relay was asked; the others show who runs them.
    String? status(String fallback) => selectedIsCurrent
        ? 'Current'
        : result.checking
        ? 'Checking...'
        : result.tooOld
        ? 'Needs an update'
        : result.error != null
        ? 'Unavailable'
        : fallback;
    Color statusColor() => result.error != null && !selectedIsCurrent
        ? colors.text.destructive
        : colors.text.secondary;
    final info = result.info;
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        MobileSurfaceCard(
          cornerRadius: AppRadii.large,
          child: Column(
            children: [
              for (final preset in presets)
                MobileListRow(
                  leading: mark(preset.url == selectedUrl),
                  label: relayHost(preset.url),
                  value: preset.url == selectedUrl
                      ? status(preset.operator)
                      : preset.operator,
                  valueColor: preset.url == selectedUrl
                      ? statusColor()
                      : colors.text.secondary,
                  labelColor: preset.url == selectedUrl && result.tooOld
                      ? colors.text.muted
                      : null,
                  minRowHeight: 48,
                  onTap: onSelect == null ? null : () => onSelect!(preset),
                ),
              MobileListRow(
                leading: mark(custom),
                label: 'Other address',
                value: custom ? status(relayHost(selectedUrl)) : null,
                valueColor: custom ? statusColor() : colors.text.secondary,
                labelColor: custom && result.tooOld ? colors.text.muted : null,
                minRowHeight: 48,
                onTap: onOther,
              ),
            ],
          ),
        ),
        if (otherOpen) ...[
          const SizedBox(height: AppSpacing.xs),
          MobileTextField(
            controller: other,
            focusNode: otherFocus,
            hintText: 'relay.example.org',
            keyboardType: TextInputType.url,
            textInputAction: TextInputAction.done,
            inputFormatters: [LengthLimitingTextInputFormatter(300)],
            onSubmitted: onOtherSubmit,
            trailing: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Tooltip(
                  message: 'Scan QR code',
                  child: AppTappable(
                    semanticsLabel: 'Scan QR code',
                    onTap: onScan,
                    child: SizedBox(
                      width: 44,
                      height: 44,
                      child: Center(
                        child: AppIcon(
                          AppIcons.qr,
                          size: 22,
                          color: colors.icon.accent,
                        ),
                      ),
                    ),
                  ),
                ),
                AppButton(
                  variant: AppButtonVariant.secondary,
                  size: AppButtonSize.mediumLarge,
                  onPressed: onOtherPaste,
                  child: const Text('Paste'),
                ),
              ],
            ),
          ),
          if (otherError != null) ...[
            const SizedBox(height: AppSpacing.xs),
            _Line(otherError!, color: colors.text.destructive),
          ],
        ],
        // The current server is never picked: what it answered doesn't matter here.
        if (selectedIsCurrent)
          ...[]
        else if (result.tooOld) ...[
          const SizedBox(height: AppSpacing.xs),
          _Line(
            'It runs an older Zafe server and can\'t hold new vaults.',
            color: colors.text.secondary,
          ),
        ] else if (result.error != null) ...[
          const SizedBox(height: AppSpacing.xs),
          _Line(result.error!, color: colors.text.destructive),
        ],
        if (!selectedIsCurrent &&
            info != null &&
            !info.tooOld &&
            !info.push) ...[
          const SizedBox(height: AppSpacing.xs),
          _Line('No notifications', color: colors.text.secondary),
        ],
        if (token != null) ...[
          const SizedBox(height: AppSpacing.sm),
          Text(
            'Create token',
            style: AppTypography.labelMedium.copyWith(
              color: colors.text.secondary,
            ),
          ),
          const SizedBox(height: AppSpacing.xs),
          MobileTextField(
            secret: true,
            controller: token!,
            focusNode: tokenFocus ?? FocusNode(),
            hintText: 'From the server\'s operator',
            textInputAction: TextInputAction.done,
            inputFormatters: [LengthLimitingTextInputFormatter(200)],
            onChanged: onTokenChanged,
          ),
        ],
      ],
    );
  }
}

class _Line extends StatelessWidget {
  const _Line(this.text, {required this.color});
  final String text;
  final Color color;

  @override
  Widget build(BuildContext context) =>
      Text(text, style: AppTypography.bodySmall.copyWith(color: color));
}

/// The host shown for a relay URL (`Uri` drops the default https port).
String relayHost(String url) => Uri.tryParse(url)?.authority ?? url;

/// Whether two relay URLs name the same relay (scheme, host and port).
bool sameRelayUrl(String a, String b) {
  final x = Uri.tryParse(a.trim()), y = Uri.tryParse(b.trim());
  if (x == null || y == null) return a.trim() == b.trim();
  return x.scheme.toLowerCase() == y.scheme.toLowerCase() &&
      x.authority.toLowerCase() == y.authority.toLowerCase();
}
