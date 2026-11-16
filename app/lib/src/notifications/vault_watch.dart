import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:firebase_core/firebase_core.dart';
import 'package:firebase_messaging/firebase_messaging.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_local_notifications/flutter_local_notifications.dart';
import 'package:path_provider/path_provider.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:workmanager/workmanager.dart';

import '../core/config/endpoints.dart';
import '../core/diagnostics/crash_log.dart';
import '../core/errors/sync_failure.dart' show fromUnixSeconds;
import '../core/errors/zafe_error_copy.dart';
import '../rust/api/error.dart' show ZafeError, ZafeErrorKind;
import '../core/network/tor_setting.dart';
import '../core/storage/member_names.dart';
import '../core/storage/vault_name.dart';
import '../core/storage/vault_summaries.dart';
import '../core/storage/zafe_paths.dart';
import '../core/storage/zafe_secure_store.dart';
import '../features/proposals/proposal_status.dart' show proposalExpired;
import '../providers/privacy_mode_provider.dart' show kPrivacyModeKey;
import '../providers/vault_provider.dart' show kActiveVaultKey;
import '../rust/api/app.dart' show initLogCache;
import '../rust/api/proposals.dart' as rust;
import '../rust/api/received.dart' as rust_received;
import '../rust/api/spends.dart' as rust_spends;
import '../rust/api/tor.dart';
import '../rust/api/vault.dart' as rust_vault;
import '../rust/frb_generated.dart';
import '../providers/setup_step.dart';
import 'check_lock.dart';
import 'vault_updates.dart';
import '../core/storage/wallet_key.dart';

const kVaultCheckTask = 'zafe.vault_check';
const _channelId = 'vault_activity';
const _channelName = 'Vault activity';

final _notifications = FlutterLocalNotificationsPlugin();

final notificationTaps = ValueNotifier<String?>(null);

@pragma('vm:entry-point')
void workmanagerDispatcher() {
  Workmanager().executeTask((task, input) async {
    await checkVaultAndNotify();
    return true;
  });
}

@pragma('vm:entry-point')
Future<void> firebaseBackgroundHandler(RemoteMessage message) async {
  await checkVaultAndNotify();
}

Future<void> _initNotifications() async {
  await _notifications.initialize(
    settings: const InitializationSettings(
      android: AndroidInitializationSettings('@drawable/ic_notification'),
      iOS: DarwinInitializationSettings(
        requestAlertPermission: false,
        requestBadgePermission: false,
        requestSoundPermission: false,
      ),
    ),
    onDidReceiveNotificationResponse: (r) {
      if (r.payload != null) notificationTaps.value = r.payload;
    },
  );
}

Future<void> initVaultNotifications() async {
  await _initNotifications();
  final launch = await _notifications.getNotificationAppLaunchDetails();
  if (launch?.didNotificationLaunchApp ?? false) {
    notificationTaps.value = launch!.notificationResponse?.payload;
  }
}

Future<bool> notificationsEnabled() async =>
    await _notifications
        .resolvePlatformSpecificImplementation<
          AndroidFlutterLocalNotificationsPlugin
        >()
        ?.areNotificationsEnabled() ??
    true;

Future<bool> requestNotifications() async {
  await _notifications
      .resolvePlatformSpecificImplementation<
        AndroidFlutterLocalNotificationsPlugin
      >()
      ?.requestNotificationsPermission();
  return notificationsEnabled();
}

Future<void> startVaultWatch({bool askPermission = true}) async {
  if (askPermission) await requestNotifications();
  if (Platform.isAndroid) {
    await Workmanager().initialize(workmanagerDispatcher);
    await Workmanager().registerPeriodicTask(
      'zafe-vault-check',
      kVaultCheckTask,
      frequency: const Duration(minutes: 15),
      constraints: Constraints(networkType: NetworkType.connected),
      existingWorkPolicy: ExistingPeriodicWorkPolicy.keep,
    );
  }
  await _registerPush(askPermission: askPermission);
}

Future<void> scheduleSoonCheck() async {
  if (!Platform.isAndroid) return;
  await Workmanager().registerOneOffTask(
    'zafe-vault-check-soon',
    kVaultCheckTask,
    initialDelay: const Duration(minutes: 1),
    constraints: Constraints(networkType: NetworkType.connected),
    existingWorkPolicy: ExistingWorkPolicy.replace,
  );
}

bool _listeningInForeground = false;

Future<void> _registerPush({bool askPermission = true}) async {
  try {
    await Firebase.initializeApp();
  } catch (_) {
    debugPrint('push: Firebase not configured; relying on background checks');
    return;
  }
  final messaging = FirebaseMessaging.instance;
  FirebaseMessaging.onBackgroundMessage(firebaseBackgroundHandler);
  if (!_listeningInForeground) {
    _listeningInForeground = true;
    FirebaseMessaging.onMessage.listen((_) async {
      final prefs = await SharedPreferences.getInstance();
      await checkVaultAndNotify(skipVaultId: prefs.getString(kActiveVaultKey));
    });
  }
  if (askPermission) await messaging.requestPermission();
  await _registerInstallation(messaging);
}

bool _listeningForRegistration = false;

Future<void> _registerInstallation(FirebaseMessaging messaging) async {
  if (!_listeningForRegistration) {
    _listeningForRegistration = true;
    messaging.onRegistered.listen((fid) => unawaited(_registerToken(fid)));
  }
  await messaging.register();
}

Future<void> _registerToken(String token) async {
  final relayUrl = (await ZafeEndpoints.load(reload: true)).relayUrl;
  final platform = Platform.isIOS ? 'apns' : 'fcm-fid';
  var registered = 0;
  for (final v in await ZafeSecureStore.instance.readAll()) {
    if (v.identity == null) continue;
    try {
      await rust_vault.registerPush(
        relayUrl: relayUrl,
        seeds: v.identity!,
        vaultId: v.id,
        material: v.material,
        invite: v.invite,
        platform: platform,
        token: token,
      );
      registered++;
    } catch (e) {
      debugPrint('push: register failed for ${v.id}: ${describeError(e)}');
    }
  }
  debugPrint('push: registered $platform for $registered vault(s)');
}

Future<void> reregisterPush() async {
  try {
    if (Firebase.apps.isEmpty) return;
    await _registerInstallation(FirebaseMessaging.instance);
  } catch (e) {
    debugPrint('push: re-register failed: ${describeError(e)}');
  }
}

Future<File> _seenFile(String vaultId) async =>
    File('${(await ZafePaths.get()).vaultDir(vaultId)}/seen.json');

Future<SeenSnapshot?> _readSeen(String vaultId) async {
  final f = await _seenFile(vaultId);
  if (!await f.exists()) return null;
  try {
    return Map<String, String>.from(jsonDecode(await f.readAsString()) as Map);
  } catch (_) {
    return null;
  }
}

Future<void> _seenWrites = Future.value();

Future<void> recordSeen(
  String vaultId,
  List<rust.ProposalInfo>? proposals, {
  List<rust_received.ReceivedInfo>? received,
  List<rust.SeatMove>? seatMoves,
  List<rust.PendingSeatMove>? pendingSeatMoves,
  List<rust_spends.UnapprovedSpendInfo>? unapprovedSpends,
}) {
  final write = _seenWrites.then((_) async {
    final f = await _seenFile(vaultId);
    await f.parent.create(recursive: true);
    final tmp = File('${f.path}.${DateTime.now().microsecondsSinceEpoch}.tmp');
    try {
      final previous = await _readSeen(vaultId) ?? const {};
      await tmp.writeAsString(
        jsonEncode(
          snapshotOf(
            proposals,
            received: received,
            seatMoves: seatMoves,
            pendingSeatMoves: pendingSeatMoves,
            unapprovedSpends: unapprovedSpends,
            previous: previous,
          ),
        ),
      );
      await tmp.rename(f.path);
    } catch (_) {}
  });
  _seenWrites = write.catchError((_) {});
  return write;
}

String notificationPayload(String vaultId, String proposalId) =>
    '$vaultId:$proposalId';

(String, String)? parsePayload(String payload) {
  final i = payload.indexOf(':');
  return i < 0 ? null : (payload.substring(0, i), payload.substring(i + 1));
}

bool _rustReady = false;

Future<void> _ensureRust() async {
  if (_rustReady) return;
  try {
    await RustLib.init();
  } catch (_) {}
  initLogCache(dir: (await ZafePaths.get()).logDir);
  _rustReady = true;
}

Future<void> checkVaultAndNotify({String? skipVaultId}) async {
  await installBackgroundCrashLog();
  final dir = Directory(
    '${(await getApplicationSupportDirectory()).path}/notifications',
  );
  await CheckLock(dir).run(skipVaultId, (skip) async {
    try {
      await _check(skip);
    } catch (e, st) {
      await _logBackgroundError('check', e, st);
    }
  });
}

Future<void> installBackgroundCrashLog() async {
  if (CrashLog.instance != null) return;
  try {
    WidgetsFlutterBinding.ensureInitialized();
    CrashLog.instance = CrashLog.forDir(
      (await ZafePaths.get()).diagnosticsDir,
      isBackground: true,
    )..install();
  } catch (_) {}
}

Future<void> _logBackgroundError(
  String source,
  Object error,
  StackTrace stack,
) async {
  debugPrint('vault check $source failed: ${describeError(error)}');
  if (error is ZafeError &&
      const {
        ZafeErrorKind.network,
        ZafeErrorKind.networkTimeout,
        ZafeErrorKind.torConnecting,
      }.contains(error.kind)) {
    return;
  }
  await CrashLog.instance?.record('background-$source', error, stack);
}

Future<void> _check(String? skipVaultId) async {
  WidgetsFlutterBinding.ensureInitialized();
  await _ensureRust();
  await _initNotifications();
  final vaults = await ZafeSecureStore.instance.readAll();
  final prefs = await SharedPreferences.getInstance();
  await prefs.reload();
  if (!await ensureTorForBackground(
    useTor: prefs.getBool(kUseTorKey) ?? false,
    request: torRequest,
    enable: () async => torConnectionOf(
      await torEnable(
        torDir: (await ZafePaths.get()).torDir,
        timeoutSecs: kBackgroundTorTimeoutSecs,
      ),
    ),
    onError: (e) =>
        debugPrint('vault check: Tor did not connect: ${describeError(e)}'),
  )) {
    debugPrint('vault check: skipped, Tor is not connected');
    return;
  }
  final hideAmounts = prefs.getBool(kPrivacyModeKey) ?? false;
  final endpoints = ZafeEndpoints.fromPrefs(prefs);
  for (final v in vaults) {
    if (v.id == skipVaultId) continue;
    if (v.ready) {
      await _checkVault(v, hideAmounts, endpoints);
    } else if (v.identity != null && v.invite != null) {
      await _checkSetup(v, prefs, endpoints);
    }
  }
}

const kSetupPayload = 'setup';
const kVaultReadyPayload = 'ready';

Future<void> _checkSetup(
  StoredVault v,
  SharedPreferences prefs,
  ZafeEndpoints endpoints,
) async {
  try {
    final p = await stepSetup(
      endpoints: endpoints,
      vaultId: v.id,
      seeds: v.identity!,
      invite: v.invite!,
    );
    final name = rust_vault.parseInvite(invite: v.invite!).name;
    if (p.material != null) {
      await _notifySetup(
        v.id,
        kVaultReadyPayload,
        '$name is ready',
        'Back it up now.',
      );
    } else if (p.safetyNumber != null && !p.started) {
      final key = 'zafe_setup_words_shown_${v.id}';
      if (prefs.getBool(key) ?? false) return;
      await prefs.setBool(key, true);
      await _notifySetup(
        v.id,
        kSetupPayload,
        '$name: compare safety words',
        'Everyone has joined.',
      );
    }
  } catch (e, st) {
    debugPrint('setup check failed for ${v.id}: ${describeError(e)}');
    await _logBackgroundError('setup', e, st);
  }
}

Future<void> _notifySetup(
  String vaultId,
  String what,
  String title,
  String body,
) => _notifications.show(
  id: notificationPayload(vaultId, what).hashCode & 0x7fffffff,
  title: title,
  body: body,
  notificationDetails: const NotificationDetails(
    android: AndroidNotificationDetails(
      _channelId,
      _channelName,
      channelDescription:
          'Payments that need you, payments sent or rejected, and money received',
      importance: Importance.high,
      priority: Priority.high,
      icon: 'ic_notification',
      color: Color(0xFF00736C),
      visibility: NotificationVisibility.private,
    ),
    iOS: DarwinNotificationDetails(),
  ),
  payload: notificationPayload(vaultId, what),
);

Future<void> _checkVault(
  StoredVault v,
  bool hideAmounts,
  ZafeEndpoints endpoints,
) async {
  try {
    final seeds = v.identity!;
    final material = v.material!;
    final paths = await ZafePaths.get();
    final stateDir = await paths.stateDir(v.id);
    final dbKey = walletKeyFor(seeds, v.id);
    final summary = rust_vault.vaultSummary(material: material);
    debugPrint('vault check: ${summary.name}');

    int? height;
    try {
      final balance = await rust_vault.syncVault(
        dbDir: paths.dbDir,
        dbKey: dbKey,
        lightwalletdUrl: endpoints.lightwalletdUrl,
        relayUrl: endpoints.relayUrl,
        seeds: seeds,
        material: material,
      );
      height = balance.height;
      await VaultSummaries.write(
        v.id,
        balanceZat: balance.totalZat,
        syncedAt: DateTime.now(),
      );
    } catch (_) {}
    List<rust_received.ReceivedInfo>? received;
    try {
      received = await rust_received.listReceived(
        dbDir: paths.dbDir,
        dbKey: dbKey,
        material: material,
      );
    } catch (_) {}
    List<rust_spends.UnapprovedSpendInfo>? unapproved;
    DateTime? unapprovedFromCopyAt;
    try {
      final found = await rust_spends.unapprovedSpends(
        relayUrl: endpoints.relayUrl,
        dbDir: paths.dbDir,
        dbKey: dbKey,
        seeds: seeds,
        material: material,
      );
      unapproved = found.spends;
      unapprovedFromCopyAt = fromUnixSeconds(found.fromCopyAt);
    } catch (_) {}
    final vaultName = VaultName.display(
      summary.name,
      await VaultName.read(v.id),
    );
    rust.ProposalList list;
    try {
      list = await rust.listProposals(
        relayUrl: endpoints.relayUrl,
        stateDir: stateDir,
        seeds: seeds,
        material: material,
        tipHeight: height,
      );
    } catch (_) {
      final alerts = vaultUpdates(
        previous: await _readSeen(v.id),
        proposals: const [],
        vaultName: vaultName,
        hideAmounts: hideAmounts,
        unapprovedSpends: unapproved ?? const [],
        unapprovedFromCopyAt: unapprovedFromCopyAt,
      ).where((u) => u.proposalId.startsWith(kUnapprovedPrefix));
      for (final u in alerts) {
        await _notifySetup(v.id, u.proposalId, u.title, u.body);
      }
      if (unapproved != null) {
        await recordSeen(v.id, null, unapprovedSpends: unapproved);
      }
      rethrow;
    }
    var proposals = list.items;
    if (list.updatedMaterial case final updated?) {
      await ZafeSecureStore.instance.writeMaterial(v.id, updated);
    }
    try {
      await rust.answerSigningRequests(
        relayUrl: endpoints.relayUrl,
        lightwalletdUrl: endpoints.lightwalletdUrl,
        dbDir: paths.dbDir,
        dbKey: dbKey,
        stateDir: stateDir,
        seeds: seeds,
        material: material,
        tipHeight: height,
      );
    } catch (_) {}

    for (final p in proposals) {
      if (p.stage == rust.ProposalStage.approved &&
          p.ready &&
          p.autoSend &&
          p.completedByMe &&
          !proposalExpired(p, height)) {
        try {
          await rust
              .sendProposal(
                relayUrl: endpoints.relayUrl,
                lightwalletdUrl: endpoints.lightwalletdUrl,
                dbDir: paths.dbDir,
                dbKey: dbKey,
                stateDir: stateDir,
                seeds: seeds,
                material: material,
                proposalId: p.id,
              )
              .drain<void>();
        } catch (_) {}
      }
    }
    list = await rust.listProposals(
      relayUrl: endpoints.relayUrl,
      stateDir: stateDir,
      seeds: seeds,
      material: material,
      tipHeight: height,
    );
    proposals = list.items;
    await VaultSummaries.write(
      v.id,
      actionable: actionableCount(proposals, height: height),
    );

    final updates = vaultUpdates(
      previous: await _readSeen(v.id),
      proposals: proposals,
      vaultName: vaultName,
      hideAmounts: hideAmounts,
      received: received ?? const [],
      seatMoves: list.seatMoves,
      pendingSeatMoves: list.pendingSeatMoves,
      unapprovedSpends: unapproved ?? const [],
      unapprovedFromCopyAt: unapprovedFromCopyAt,
      me: rust_vault.identityPublicKey(seeds: seeds),
      names: MemberNames.merge({
        for (final n in list.sharedNames) n.keyHex: n.name,
      }, await MemberNames.read(v.id)),
    );
    for (final u in updates) {
      await _notifications.show(
        id: notificationPayload(v.id, u.proposalId).hashCode & 0x7fffffff,
        title: u.title,
        body: u.body,
        notificationDetails: const NotificationDetails(
          android: AndroidNotificationDetails(
            _channelId,
            _channelName,
            channelDescription:
                'Payments that need you, payments sent or rejected, and money received',
            importance: Importance.high,
            priority: Priority.high,
            icon: 'ic_notification',
            color: Color(0xFF00736C),
            visibility: NotificationVisibility.private,
          ),
          iOS: DarwinNotificationDetails(),
        ),
        payload: notificationPayload(v.id, u.proposalId),
      );
    }
    await recordSeen(
      v.id,
      proposals,
      received: received,
      seatMoves: list.seatMoves,
      pendingSeatMoves: list.pendingSeatMoves,
      unapprovedSpends: unapproved,
    );
    debugPrint(
      'vault check: ${summary.name}: ${updates.length} notification(s)',
    );
  } catch (e, st) {
    debugPrint('vault check failed for ${v.id}: ${describeError(e)}');
    await _logBackgroundError('vault', e, st);
  }
}
