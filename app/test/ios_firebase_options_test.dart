import 'package:firebase_core/firebase_core.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/notifications/ios_firebase_options.dart';

void main() {
  test('iOS push stays off unless every Firebase identifier is set', () {
    FirebaseOptions? options({String key = 'k', String app = 'a'}) =>
        firebaseOptionsFrom(
          apiKey: key,
          appId: app,
          senderId: '1',
          projectId: 'p',
          bundleId: 'xyz.zafe.zafe',
        );
    expect(options(), isNotNull);
    expect(options()!.iosBundleId, 'xyz.zafe.zafe');
    expect(options(key: ''), isNull);
    expect(options(app: ''), isNull);
  });
}
