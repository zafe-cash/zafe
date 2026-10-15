import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:permission_handler/permission_handler.dart'
    show openAppSettings;

import '../core/layout/mobile/app_mobile_sheet.dart';
import '../core/theme/app_theme.dart';
import '../core/widgets/app_button.dart';
import 'vault_watch.dart';

/// Set once the primer was shown: the app never asks on its own again (Home keeps a
/// "Notifications are off" card instead).
const kNotificationsPrimedKey = 'zafe_notifications_primed';

/// Whether this app may show notifications. Re-read on resume (the owner may have
/// changed it in the system settings).
final notificationsEnabledProvider = FutureProvider.autoDispose<bool>(
  (ref) => notificationsEnabled(),
);

/// Says why before the system's permission prompt (a cold system prompt is easy to
/// deny). Returns whether the member wants notifications.
Future<bool> showNotificationPrimer(BuildContext context) async {
  final yes = await showAppMobileSheet<bool>(
    context: context,
    builder: (sheet) => const NotificationPrimerSheet(),
  );
  return yes == true;
}

/// The primer's content: pops `true` for "Turn on".
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
              'Your co-signers will need your approval. Zafe tells you when a payment '
              'waits for you, and when money arrives. Notifications show no amounts on '
              'the lock screen.',
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

/// "Turn on" from the Home card: the system prompt if Android still offers it, else the
/// app's notification settings.
Future<void> enableNotifications(WidgetRef ref) async {
  if (!await requestNotifications()) await openAppSettings();
  ref.invalidate(notificationsEnabledProvider);
}
