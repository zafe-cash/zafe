import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';
import 'package:zafe/src/core/theme/app_theme.dart';
import 'package:zafe/src/features/home/needs_you.dart';
import 'package:zafe/src/providers/member_names_provider.dart';
import 'package:zafe/src/providers/privacy_mode_provider.dart';
import 'package:zafe/src/providers/proposals_provider.dart';
import 'package:zafe/src/providers/vault_provider.dart';
import 'package:zafe/src/rust/api/proposals.dart';

import 'vault_updates_test.dart' show proposal;

class _Proposals extends ProposalsNotifier {
  _Proposals(this.items);
  final List<ProposalInfo> items;
  @override
  ProposalsState build() => ProposalsState(items: items, loaded: true);
}

class _Vault extends VaultNotifier {
  @override
  VaultState build() => const VaultState();
}

class _Privacy extends PrivacyModeNotifier {
  @override
  bool build() => false;
}

/// Whether Home is the visible tab (other tabs mute its tickers).
final _visible = ValueNotifier(true);

Future<List<String>> _pump(WidgetTester tester, int n) async {
  final opened = <String>[];
  final router = GoRouter(
    routes: [
      GoRoute(
        path: '/',
        builder: (_, _) => ValueListenableBuilder(
          valueListenable: _visible,
          builder: (_, visible, _) => TickerMode(
            enabled: visible,
            child: const Scaffold(
              body: SingleChildScrollView(child: NeedsYou(height: 100)),
            ),
          ),
        ),
      ),
      GoRoute(
        path: '/proposal/:id',
        builder: (_, s) {
          opened.add(s.pathParameters['id']!);
          return const SizedBox();
        },
      ),
    ],
  );
  await tester.pumpWidget(
    ProviderScope(
      overrides: [
        proposalsProvider.overrideWith(
          () => _Proposals([
            for (var i = 0; i < n; i++) proposal('p$i', author: 'a' * 64),
          ]),
        ),
        vaultProvider.overrideWith(_Vault.new),
        privacyModeProvider.overrideWith(_Privacy.new),
        signerNamesProvider.overrideWithValue(const {}),
      ],
      child: MaterialApp.router(
        routerConfig: router,
        builder: (context, child) =>
            AppTheme(data: AppThemeData.dark, child: child!),
      ),
    ),
  );
  await tester.pumpAndSettle();
  return opened;
}

void main() {
  testWidgets('three requests stack; tapping the top card fans them out', (
    tester,
  ) async {
    final opened = await _pump(tester, 3);
    expect(find.text('3 waiting'), findsOneWidget);
    await tester.tap(find.textContaining('1.5').first);
    await tester.pumpAndSettle();
    expect(
      opened,
      isEmpty,
      reason: 'the first tap opens the stack, not a payment',
    );
    expect(find.text('3 waiting'), findsNothing);
    // Open: a card's tap now opens its payment.
    await tester.tap(find.textContaining('1.5').first);
    await tester.pumpAndSettle();
    expect(opened, hasLength(1));
  });

  testWidgets('two requests do not stack', (tester) async {
    final opened = await _pump(tester, 2);
    expect(find.text('2 waiting'), findsNothing);
    await tester.tap(find.textContaining('1.5').first);
    await tester.pumpAndSettle();
    expect(opened, hasLength(1));
  });

  testWidgets('switching to another tab folds the stack', (tester) async {
    await _pump(tester, 3);
    await tester.tap(find.textContaining('1.5').first);
    await tester.pumpAndSettle();
    expect(find.text('3 waiting'), findsNothing);
    _visible.value = false;
    await tester.pumpAndSettle();
    _visible.value = true;
    await tester.pumpAndSettle();
    expect(find.text('3 waiting'), findsOneWidget);
  });
}
