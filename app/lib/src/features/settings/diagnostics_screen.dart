import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:share_plus/share_plus.dart';

import '../../core/config/network_config.dart';
import '../../core/diagnostics/crash_log.dart';
import '../../core/layout/mobile/zafe_screen.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/app_button.dart';

/// Settings > Diagnostic report: the errors recorded on this phone, shown in full before
/// anything is shared. Zafe sends no telemetry and no crash reports; this is the only way
/// the log leaves the phone, and only if the user taps Share.
class DiagnosticsScreen extends StatefulWidget {
  const DiagnosticsScreen({super.key, this.log});

  /// Defaults to this process's log.
  final CrashLog? log;

  @override
  State<DiagnosticsScreen> createState() => _DiagnosticsScreenState();
}

class _DiagnosticsScreenState extends State<DiagnosticsScreen> {
  late Future<String> _report = _load();

  CrashLog? get _log => widget.log ?? CrashLog.instance;

  Future<String> _load() async =>
      await _log?.report(network: kZafeNetwork) ?? 'No log on this phone.';

  Future<void> _clear() async {
    await _log?.clear();
    setState(() => _report = _load());
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return FutureBuilder<String>(
      future: _report,
      builder: (context, snapshot) {
        final text = snapshot.data;
        return ZafeScreen(
          title: 'Diagnostic report',
          bottom: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              AppButton(
                expand: true,
                onPressed: text == null
                    ? null
                    : () => SharePlus.instance.share(ShareParams(text: text)),
                child: const Text('Share report'),
              ),
              const SizedBox(height: AppSpacing.xs),
              Row(
                children: [
                  Expanded(
                    child: AppButton(
                      expand: true,
                      variant: AppButtonVariant.secondary,
                      onPressed: text == null
                          ? null
                          : () => Clipboard.setData(ClipboardData(text: text)),
                      child: const Text('Copy'),
                    ),
                  ),
                  const SizedBox(width: AppSpacing.xs),
                  Expanded(
                    child: AppButton(
                      expand: true,
                      variant: AppButtonVariant.ghost,
                      onPressed: _clear,
                      child: const Text('Clear log'),
                    ),
                  ),
                ],
              ),
            ],
          ),
          children: [
            Text(
              'Zafe sends no analytics. Errors stay on this phone, with personal details removed.',
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
              child: SelectableText(
                text ?? 'Loading...',
                style: AppTypography.codeSmall.copyWith(
                  color: colors.text.secondary,
                ),
              ),
            ),
          ],
        );
      },
    );
  }
}
