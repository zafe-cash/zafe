import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:zafe/src/core/layout/mobile/mobile_top_nav.dart';
import 'package:zafe/src/core/storage/zafe_secure_store.dart';
import 'package:zafe/src/core/theme/app_theme.dart';
import 'package:zafe/src/features/onboarding/create_vault_screen.dart';
import 'package:zafe/src/features/onboarding/setup_screen.dart';
import 'package:zafe/src/notifications/notification_primer.dart';
import 'package:zafe/src/providers/vault_provider.dart';
import 'package:zafe/src/rust/api/vault.dart' as rust;
import 'package:zafe/src/rust/frb_generated.dart';

const _vaultId = 'aa';
const _me = '1d426b3769983ed225fd919e80f48a4de61c76cdf31e4c805e78b81873000bef';

/// The only bridge calls this flow makes once cancelling is faked: reading the invite
/// and this member's key.
class _Api implements RustLibApi {
  @override
  String crateApiVaultIdentityPublicKey({required List<int> seeds}) => _me;

  @override
  rust.InviteInfo crateApiVaultParseInvite({required String invite}) =>
      const rust.InviteInfo(
        vaultId: _vaultId,
        name: 'Grants committee',
        threshold: 2,
        members: 3,
        creatorHex: _me,
      );

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

class _Vault extends VaultNotifier {
  @override
  VaultState build() => VaultState(
    vaults: [
      StoredVault(id: _vaultId, identity: Uint8List(32), invite: 'invite'),
    ],
    activeId: _vaultId,
    membership: const rust.SetupProgress(
      members: [_me],
      sealed: false,
      isCreator: true,
      waitingFor: [],
      started: false,
      keygenWaitingFor: [],
    ),
  );

  @override
  Future<rust.SetupProgress> setupStep({String? confirmed}) async =>
      state.membership!;

  @override
  Future<void> renewInvite() async {}

  @override
  Future<({bool creator, bool deleted})> cancelSetup() async {
    state = const VaultState();
    return (creator: true, deleted: true);
  }
}

/// Fixed pumps: the setup screen animates for as long as it is open.
Future<void> _settle(WidgetTester tester) async {
  for (var i = 0; i < 10; i++) {
    await tester.pump(const Duration(milliseconds: 100));
  }
}

Future<void> _cancelSetup(WidgetTester tester) async {
  tester.view.devicePixelRatio = 3;
  tester.view.physicalSize = const Size(900 * 3, 1400 * 3);
  addTearDown(tester.view.reset);
  SharedPreferences.setMockInitialValues({kNotificationsPrimedKey: true});
  final router = GoRouter(
    initialLocation: '/setup',
    routes: [
      GoRoute(
        path: '/welcome',
        builder: (_, _) => const Scaffold(body: Text('Welcome stand-in')),
      ),
      GoRoute(path: '/create', builder: (_, _) => const CreateVaultScreen()),
      GoRoute(path: '/setup', builder: (_, _) => const SetupScreen()),
    ],
  );
  addTearDown(router.dispose);
  await tester.pumpWidget(
    ProviderScope(
      overrides: [vaultProvider.overrideWith(_Vault.new)],
      child: MaterialApp.router(
        routerConfig: router,
        builder: (context, child) =>
            AppTheme(data: AppThemeData.light, child: child!),
      ),
    ),
  );
  await tester.pump();

  await tester.tap(find.text('Cancel setup'));
  await _settle(tester);
  // The confirm dialog's button, over the screen's.
  await tester.tap(find.text('Cancel setup').last);
  await _settle(tester);
  expect(find.byType(CreateVaultScreen), findsOneWidget);
}

void main() {
  setUpAll(() => RustLib.initMock(api: _Api()));

  testWidgets('the back button works on "New vault" after a cancelled setup', (
    tester,
  ) async {
    await _cancelSetup(tester);
    final back = find.descendant(
      of: find.byType(MobileTopNav),
      matching: find.byType(GestureDetector),
    );
    await tester.tap(back.first);
    await _settle(tester);
    expect(find.byType(CreateVaultScreen), findsNothing);
    expect(find.text('Welcome stand-in'), findsOneWidget);
  });

  testWidgets('Android back also leaves "New vault" after a cancelled setup', (
    tester,
  ) async {
    await _cancelSetup(tester);
    await tester.binding.handlePopRoute();
    await _settle(tester);
    expect(find.byType(CreateVaultScreen), findsNothing);
    expect(find.text('Welcome stand-in'), findsOneWidget);
  });
}
