import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:permission_handler/permission_handler.dart'
    show openAppSettings;

import '../core/layout/mobile/app_mobile_sheet.dart';
import '../core/theme/app_theme.dart';
import '../core/widgets/app_button.dart';
import 'vault_watch.dart';

const kNotificationsPrimedKey = 'zafe_notifications_primed';

final notificationsEnabledProvider = FutureProvider.autoDispose<bool>(
  (ref) => notificationsEnabled(),
);

Future<bool> showNotificationPrimer(BuildContext context) async {
  final yes = await showAppMobileSheet<bool>(
    context: context,
    builder: (sheet) => const NotificationPrimerSheet(),
  );
  return yes == true;
}

class NotificationPrimerSheet extends StatelessWidget {
  const NotificationPrimerSheet({super.key});

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return MobileModalScaffold(
      title: 'Turn on notifications',
      onClose: () => Navigator.of(context).pop(false),
      constrainBody: true,
      child: SingleChildScrollView(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text(
              'Know when it\'s your turn: comparing safety words, approving a payment. No amounts show on the lock screen.',
              style: AppTypography.bodyMedium.copyWith(
                color: colors.text.secondary,
              ),
            ),
            const SizedBox(height: AppSpacing.md),
            AppButton(
              expand: true,
              onPressed: () => Navigator.of(context).pop(true),
              child: const Text('Turn on'),
            ),
            const SizedBox(height: AppSpacing.xs),
            AppButton(
              expand: true,
              variant: AppButtonVariant.ghost,
              onPressed: () => Navigator.of(context).pop(false),
              child: const Text('Not now'),
            ),
          ],
        ),
      ),
    );
  }
}

Future<void> startWatchWithPrimer(BuildContext context) async {
  final prefs = await SharedPreferences.getInstance();
  var ask = false;
  if (!(prefs.getBool(kNotificationsPrimedKey) ?? false)) {
    await prefs.setBool(kNotificationsPrimedKey, true);
    if (!await notificationsEnabled()) {
      if (!context.mounted) return;
      ask = await showNotificationPrimer(context);
    }
  }
  await startVaultWatch(askPermission: ask);
}

Future<void> enableNotifications(WidgetRef ref) async {
  if (!await requestNotifications()) await openAppSettings();
  ref.invalidate(notificationsEnabledProvider);
}
