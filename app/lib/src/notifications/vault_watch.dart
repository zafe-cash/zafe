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

/// Vault notifications (spec §6.1). The relay is blind: a push (FCM) or a periodic
/// background check (WorkManager) only wakes the app; it then reads the encrypted vault
/// log itself and shows local notifications for what changed since it last looked.

const kVaultCheckTask = 'zafe.vault_check';
const _channelId = 'vault_activity';
const _channelName = 'Vault activity';

final _notifications = FlutterLocalNotificationsPlugin();

/// Proposal ids from tapped notifications, for the router to open.
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

/// Main isolate, before the first frame: notification tap handling and the proposal a
/// tapped notification launched the app with.
Future<void> initVaultNotifications() async {
  await _initNotifications();
  final launch = await _notifications.getNotificationAppLaunchDetails();
  if (launch?.didNotificationLaunchApp ?? false) {
    notificationTaps.value = launch!.notificationResponse?.payload;
  }
}

/// Whether this app may show notifications (always true where it can't tell).
Future<bool> notificationsEnabled() async =>
    await _notifications
        .resolvePlatformSpecificImplementation<
          AndroidFlutterLocalNotificationsPlugin
        >()
        ?.areNotificationsEnabled() ??
    true;

/// Shows the system's notification permission prompt (Android 13+). Returns whether
/// notifications are on afterwards.
Future<bool> requestNotifications() async {
  await _notifications
      .resolvePlatformSpecificImplementation<
        AndroidFlutterLocalNotificationsPlugin
      >()
      ?.requestNotificationsPermission();
  return notificationsEnabled();
}

/// Once a vault exists: schedule the periodic background check and register for pushes
/// when Firebase is configured. [askPermission]: show the system prompt first (only after
/// the app's own primer said why).
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

/// When the app goes to the background: check again soon, so a proposal made while the
/// member just looked away still gets announced without waiting for the periodic check.
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

/// FCM: only when the app was built with a Firebase config (google-services.json).
/// Push registration runs again on a relay change: listen in the foreground once.
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
  // A push while the app is open: the open vault refreshes live (its relay watch), but
  // the others had nothing (no notification, no dot in the vault switcher) until the app
  // went to the background. Check them; the open one is the app's.
  if (!_listeningInForeground) {
    _listeningInForeground = true;
    FirebaseMessaging.onMessage.listen((_) async {
      final prefs = await SharedPreferences.getInstance();
      await checkVaultAndNotify(skipVaultId: prefs.getString(kActiveVaultKey));
    });
  }
  if (askPermission) await messaging.requestPermission();
  final token = await messaging.getToken();
  if (token != null) await _registerToken(token);
  messaging.onTokenRefresh.listen(_registerToken);
}

/// Registers the push token for every vault on this device (each with its own member
/// identity) with the relay currently configured.
Future<void> _registerToken(String token) async {
  final relayUrl = (await ZafeEndpoints.load(reload: true)).relayUrl;
  // Vaults still being set up too: their setup steps run in background checks.
  for (final v in await ZafeSecureStore.instance.readAll()) {
    if (v.identity == null) continue;
    try {
      await rust_vault.registerPush(
        relayUrl: relayUrl,
        seeds: v.identity!,
        vaultId: v.id,
        platform: Platform.isIOS ? 'apns' : 'fcm',
        token: token,
      );
    } catch (e) {
      debugPrint('push: register failed for ${v.id}: ${describeError(e)}');
    }
  }
}

/// After the relay URL changed: register the push token with the new relay.
Future<void> reregisterPush() async {
  try {
    if (Firebase.apps.isEmpty) return;
    final token = await FirebaseMessaging.instance.getToken();
    if (token != null) await _registerToken(token);
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

/// Serializes snapshot writes in this isolate (proposals and received payments are
/// recorded separately, and each write merges with the file).
Future<void> _seenWrites = Future.value();

/// Records what this device has seen of a vault (also called by the app on every refresh,
/// so things seen in the app are never announced again from the background). A `null`
/// list keeps what was recorded for that kind.
Future<void> recordSeen(
  String vaultId,
  List<rust.ProposalInfo>? proposals, {
  List<rust_received.ReceivedInfo>? received,
  List<rust.SeatMove>? seatMoves,
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

/// Notification payloads carry both ids, so a tap can switch to the right vault.
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
  } catch (_) {
    // Already initialized on this isolate (one-off tasks can run on the main engine).
  }
  // Every isolate that talks to the relay checks it against this phone's log copy.
  initLogCache(dir: (await ZafePaths.get()).logDir);
  _rustReady = true;
}

/// The background check: sync, read the vault log, answer interactive signing requests,
/// finish an auto-send this member owes, and notify about changes. Never throws.
/// [skipVaultId]: a vault the app shows live right now (a push while the app is open),
/// left to the app so nothing on screen is also announced.
Future<void> checkVaultAndNotify({String? skipVaultId}) async {
  await installBackgroundCrashLog();
  // One check at a time (the periodic task, the one-off after backgrounding and a push can
  // fire together, seen on the emulator); one that arrives meanwhile makes it run again.
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

/// Background engines are separate isolates with their own memory: install the scrubbed
/// local log there too (its own file, `crashes-bg.log`; the app's report merges both).
/// A check that runs on the main engine keeps the app's log. Never throws.
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
  // Offline, a timeout or Tor still connecting is routine for a check that runs at any
  // time: not logged, so it can't push real errors out of the 20-entry log.
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
  // "Use Tor" (read after the reload): nothing may connect before the route is Tor, and
  // a Tor that can't connect in time ends this check (the next one tries again).
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
  // The URLs the user set in Settings (read after the reload: prefs cache per isolate).
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

/// Payload of the "compare safety words" and "vault ready" notifications.
const kSetupPayload = 'setup';
const kVaultReadyPayload = 'ready';

/// A vault still being set up: one setup step (it can lock membership, send this
/// member's part of the safety check, continue key generation, and save the vault once
/// it exists). Two notifications only, the moments that need the member: the words are
/// there to compare, and the vault is ready to back up.
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

/// One vault: sync, read the log, answer interactive signing requests, finish an auto-send
/// this member owes, update the switcher summary, and notify about changes.
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
    } catch (_) {} // offline is routine: not logged
    List<rust_received.ReceivedInfo>? received;
    try {
      received = await rust_received.listReceived(
        dbDir: paths.dbDir,
        dbKey: dbKey,
        material: material,
      );
    } catch (_) {}
    List<rust_spends.UnapprovedSpendInfo>? unapproved;
    try {
      unapproved = await rust_spends.unapprovedSpends(
        relayUrl: endpoints.relayUrl,
        dbDir: paths.dbDir,
        dbKey: dbKey,
        seeds: seeds,
        material: material,
      );
    } catch (_) {}
    var list = await rust.listProposals(
      relayUrl: endpoints.relayUrl,
      stateDir: stateDir,
      seeds: seeds,
      material: material,
      tipHeight: height,
    );
    var proposals = list.items;
    // A signer's seat moved: keep this device's copy of the membership current.
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

    // An approval this member made completed the signatures, but the app closed before
    // sending: send now.
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
      vaultName: VaultName.display(summary.name, await VaultName.read(v.id)),
      hideAmounts: hideAmounts,
      received: received ?? const [],
      seatMoves: list.seatMoves,
      unapprovedSpends: unapproved ?? const [],
      me: rust_vault.identityPublicKey(seeds: seeds),
      // Read here: this may run in a background isolate without the app's providers.
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
            // The Z from the app icon (scripts/brand/brand.py), tinted brand verdigris.
            icon: 'ic_notification',
            color: Color(0xFF00736C),
            // Amounts and recipients stay off the lock screen (Android shows the app
            // name and "Contents hidden" until the phone is unlocked).
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
