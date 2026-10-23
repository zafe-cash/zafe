import 'package:firebase_core/firebase_core.dart';

/// Firebase identifiers for the iOS app, from the build (they come from the Firebase
/// console's `GoogleService-Info.plist` and are not secrets). iOS builds carry them as
/// dart-defines instead of shipping the plist, so no Xcode project edit is needed.
const _apiKey = String.fromEnvironment('ZAFE_FIREBASE_IOS_API_KEY');
const _appId = String.fromEnvironment('ZAFE_FIREBASE_IOS_APP_ID');
const _senderId = String.fromEnvironment('ZAFE_FIREBASE_IOS_SENDER_ID');
const _projectId = String.fromEnvironment('ZAFE_FIREBASE_IOS_PROJECT_ID');
const _bundleId = String.fromEnvironment(
  'ZAFE_FIREBASE_IOS_BUNDLE_ID',
  defaultValue: 'xyz.zafe.zafe',
);

/// The options, or null when the build has no Firebase iOS app (push stays off and the
/// app relies on background checks).
FirebaseOptions? iosFirebaseOptions() => firebaseOptionsFrom(
  apiKey: _apiKey,
  appId: _appId,
  senderId: _senderId,
  projectId: _projectId,
  bundleId: _bundleId,
);

/// Pure part of [iosFirebaseOptions]: all four identifiers must be present.
FirebaseOptions? firebaseOptionsFrom({
  required String apiKey,
  required String appId,
  required String senderId,
  required String projectId,
  required String bundleId,
}) {
  if (apiKey.isEmpty || appId.isEmpty || senderId.isEmpty || projectId.isEmpty) {
    return null;
  }
  return FirebaseOptions(
    apiKey: apiKey,
    appId: appId,
    messagingSenderId: senderId,
    projectId: projectId,
    iosBundleId: bundleId,
  );
}
