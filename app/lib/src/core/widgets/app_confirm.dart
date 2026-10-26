import 'package:flutter/material.dart' show Dialog, showDialog;
import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../theme/app_theme.dart';
import 'app_button.dart';
import 'app_icon.dart';

/// Asks before something that can't be undone (remove a vault, cancel a payment, drop
/// this phone's history): a centred card with the question, what happens, and two
/// buttons. True only for [confirmLabel]; Back, a tap outside and [cancelLabel] are
/// false. The one surface allowed over a sheet or a screen (sheets never stack).
Future<bool> showAppConfirm(
  BuildContext context, {
  required String title,
  required String body,
  required String confirmLabel,
  String cancelLabel = 'Cancel',
  bool destructive = true,
}) async {
  final appTheme = context.appTheme;
  ProviderContainer? container;
  try {
    container = ProviderScope.containerOf(context, listen: false);
  } on StateError {
    container = null;
  }
  final ok = await showDialog<bool>(
    context: context,
    barrierColor: context.colors.background.neutralScrim,
    builder: (dialog) {
      final card = AppTheme(
        data: appTheme,
        child: AppConfirmCard(
          title: title,
          body: body,
          confirmLabel: confirmLabel,
          cancelLabel: cancelLabel,
          destructive: destructive,
          onConfirm: () => Navigator.of(dialog).pop(true),
          onCancel: () => Navigator.of(dialog).pop(false),
        ),
      );
      return container == null
          ? card
          : UncontrolledProviderScope(container: container, child: card);
    },
  );
  return ok ?? false;
}

/// The confirm dialog's card from plain data (rendered without a route in previews).
class AppConfirmCard extends StatelessWidget {
  const AppConfirmCard({
    super.key,
    required this.title,
    required this.body,
    required this.confirmLabel,
    this.cancelLabel = 'Cancel',
    this.destructive = true,
    this.onConfirm,
    this.onCancel,
  });

  final String title;
  final String body;
  final String confirmLabel;
  final String cancelLabel;
  final bool destructive;
  final VoidCallback? onConfirm;
  final VoidCallback? onCancel;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return Dialog(
      backgroundColor: colors.background.base,
      surfaceTintColor: const Color(0x00000000),
      insetPadding: const EdgeInsets.symmetric(
        horizontal: AppSpacing.sm,
        vertical: AppSpacing.md,
      ),
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(AppRadii.xLarge),
      ),
      child: Padding(
        padding: const EdgeInsets.fromLTRB(
          AppSpacing.sm,
          AppSpacing.md,
          AppSpacing.sm,
          AppSpacing.sm,
        ),
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              if (destructive) ...[
                Align(
                  alignment: Alignment.centerLeft,
                  child: AppIcon(
                    AppIcons.warning,
                    size: 24,
                    color: colors.icon.destructive,
                    patina: colors.icon.destructive,
                  ),
                ),
                const SizedBox(height: AppSpacing.s),
              ],
              Text(
                title,
                style: AppTypography.bodyLarge.copyWith(
                  fontWeight: FontWeight.w600,
                  color: colors.text.accent,
                ),
              ),
              const SizedBox(height: AppSpacing.xs),
              Text(
                body,
                style: AppTypography.bodyMedium.copyWith(
                  color: colors.text.secondary,
                ),
              ),
              const SizedBox(height: AppSpacing.md),
              AppButton(
                expand: true,
                variant: destructive
                    ? AppButtonVariant.destructive
                    : AppButtonVariant.primary,
                onPressed: onConfirm,
                child: Text(confirmLabel),
              ),
              const SizedBox(height: AppSpacing.xs),
              AppButton(
                expand: true,
                variant: AppButtonVariant.ghost,
                onPressed: onCancel,
                child: Text(cancelLabel),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
