import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/theme/app_theme.dart';
import 'package:zafe/src/features/onboarding/create_vault_screen.dart';
import 'package:zafe/src/providers/vault_provider.dart';

class _Vault extends VaultNotifier {
  @override
  VaultState build() => const VaultState();
}

Future<ProviderContainer> _pump(
  WidgetTester tester, {
  CreatePrefill? prefill,
}) async {
  tester.view.devicePixelRatio = 3;
  tester.view.physicalSize = const Size(900 * 3, 1400 * 3);
  addTearDown(tester.view.reset);
  final container = ProviderContainer(
    overrides: [vaultProvider.overrideWith(_Vault.new)],
  );
  addTearDown(container.dispose);
  container.read(createPrefillProvider.notifier).set(prefill);
  await tester.pumpWidget(
    UncontrolledProviderScope(
      container: container,
      child: MaterialApp(
        builder: (context, child) =>
            AppTheme(data: AppThemeData.light, child: child!),
        home: const CreateVaultScreen(),
      ),
    ),
  );
  await tester.pump();
  return container;
}

void main() {
  testWidgets('"New vault" comes back filled in after a cancelled setup', (
    tester,
  ) async {
    final container = await _pump(
      tester,
      prefill: const CreatePrefill(
        name: 'Grants committee',
        threshold: 3,
        members: 5,
        expiryDays: 14,
      ),
    );
    expect(find.text('Grants committee'), findsOneWidget);
    expect(find.text('5'), findsWidgets); // the members stepper
    expect(find.text('3'), findsWidgets); // the approvals stepper
    // Read once: the screen clears it, so a later "New vault" starts empty.
    await tester.pump();
    expect(container.read(createPrefillProvider), isNull);
  });

  testWidgets('"New vault" starts empty without a cancelled setup', (
    tester,
  ) async {
    await _pump(tester);
    expect(find.text('Grants committee'), findsNothing);
  });
}
