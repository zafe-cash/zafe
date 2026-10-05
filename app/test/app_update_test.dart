import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/services/app_update.dart';

UpdateAction decide({
  bool available = true,
  bool downloaded = false,
  int priority = 0,
  bool immediateAllowed = true,
  bool flexibleAllowed = true,
  bool sending = false,
}) => decideUpdate(
  available: available,
  downloaded: downloaded,
  priority: priority,
  immediateAllowed: immediateAllowed,
  flexibleAllowed: flexibleAllowed,
  sending: sending,
);

void main() {
  test('no update, nothing to do', () {
    expect(decide(available: false), UpdateAction.none);
  });

  test('a normal release downloads in the background', () {
    expect(decide(priority: 0), UpdateAction.flexible);
    expect(
      decide(priority: kCriticalUpdatePriority - 1),
      UpdateAction.flexible,
    );
  });

  test('a critical release takes over the screen', () {
    expect(decide(priority: kCriticalUpdatePriority), UpdateAction.immediate);
    expect(decide(priority: 5), UpdateAction.immediate);
  });

  test('never takes over the screen while a payment is being sent', () {
    expect(decide(priority: 5, sending: true), UpdateAction.flexible);
  });

  test('falls back to what Play allows', () {
    expect(decide(priority: 5, immediateAllowed: false), UpdateAction.flexible);
    expect(decide(flexibleAllowed: false), UpdateAction.none);
  });

  test('an update downloaded earlier waits for a restart', () {
    expect(
      decide(available: false, downloaded: true),
      UpdateAction.readyToInstall,
    );
  });
}
