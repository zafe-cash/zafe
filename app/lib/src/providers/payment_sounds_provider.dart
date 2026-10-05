import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'vault_provider.dart';

const kPaymentSoundsKey = 'zafe_payment_sounds';

/// "Payment sounds" (Settings): the sound on approve, send and receive. On by default;
/// haptics play either way, and the phone's silent mode always wins.
class PaymentSoundsNotifier extends Notifier<bool> {
  @override
  bool build() => ref.watch(vaultBootstrapProvider).paymentSounds;

  Future<void> toggle() async {
    state = !state;
    final prefs = await SharedPreferences.getInstance();
    await prefs.setBool(kPaymentSoundsKey, state);
  }
}

final paymentSoundsProvider = NotifierProvider<PaymentSoundsNotifier, bool>(
  PaymentSoundsNotifier.new,
);
