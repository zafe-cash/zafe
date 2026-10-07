import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:share_plus/share_plus.dart';

import '../../core/config/network_config.dart';
import '../../core/diagnostics/crash_log.dart';
import '../../core/layout/mobile/app_mobile_sheet.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/app_button.dart';

/// "Diagnostic report": the errors recorded on this phone, shown in full before anything
/// is shared. Zafe sends no telemetry and no crash reports; this is the only way the log
/// leaves the phone, and only if the user taps Share.
Future<void> showDiagnosticsSheet(BuildContext context) =>
    showAppMobileSheet<void>(
      context: context,
      builder: (_) => const DiagnosticsSheet(),
    );

class DiagnosticsSheet extends StatefulWidget {
  const DiagnosticsSheet({super.key, this.log});

  /// Defaults to this process's log.
  final CrashLog? log;

  @override
  State<DiagnosticsSheet> createState() => _DiagnosticsSheetState();
}

class _DiagnosticsSheetState extends State<DiagnosticsSheet> {
  late Future<String> _report = _load();

  CrashLog? get _log => widget.log ?? CrashLog.instance;

  Future<String> _load() async =>
      await _log?.report(network: kZafeNetwork) ?? 'No log on this device.';

  Future<void> _clear() async {
    await _log?.clear();
    setState(() => _report = _load());
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return MobileModalScaffold(
      title: 'Diagnostic report',
      onClose: () => Navigator.of(context).pop(),
      child: FutureBuilder<String>(
        future: _report,
        builder: (context, snapshot) {
          final text = snapshot.data;
          return Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Text(
                'Zafe sends no analytics and no crash reports. Errors are kept '
                'here, on this phone, with addresses, keys, links, amounts and '
                'vault ids removed. Read it, then share it only if you want to '
                'report a problem.',
                style: AppTypography.bodyMedium.copyWith(
                  color: colors.text.secondary,
                ),
              ),
              const SizedBox(height: AppSpacing.sm),
              Container(
                padding: const EdgeInsets.all(AppSpacing.sm),
                decoration: BoxDecoration(
                  color: colors.background.neutralSubtleOpacity,
                  borderRadius: BorderRadius.circular(AppRadii.medium),
                ),
                constraints: const BoxConstraints(maxHeight: 260),
                child: SingleChildScrollView(
                  child: SelectableText(
                    text ?? 'Loading…',
                    style: AppTypography.codeSmall.copyWith(
                      color: colors.text.secondary,
                    ),
                  ),
                ),
              ),
              const SizedBox(height: AppSpacing.md),
              AppButton(
                expand: true,
                onPressed: text == null
                    ? null
                    : () => SharePlus.instance.share(ShareParams(text: text)),
                child: const Text('Share report'),
              ),
              const SizedBox(height: AppSpacing.xs),
              AppButton(
                expand: true,
                variant: AppButtonVariant.secondary,
                onPressed: text == null
                    ? null
                    : () => Clipboard.setData(ClipboardData(text: text)),
                child: const Text('Copy'),
              ),
              const SizedBox(height: AppSpacing.xs),
              AppButton(
                expand: true,
                variant: AppButtonVariant.ghost,
                onPressed: _clear,
                child: const Text('Clear log'),
              ),
            ],
          );
        },
      ),
    );
  }
}
