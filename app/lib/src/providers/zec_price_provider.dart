import 'dart:async';
import 'dart:convert';

import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../core/config/network_config.dart';
import '../core/errors/zafe_error_copy.dart';
import '../core/formatting/zec_amount.dart';
import '../rust/api/price.dart' as rust;

/// How often the price is fetched while the app is in the foreground.
const kZecPriceRefresh = Duration(minutes: 3);

/// How long a fetched price may be shown: a stale cache or failed refreshes hide it after.
const kZecPriceTtl = Duration(hours: 1);

const kZecPriceCacheKey = 'zafe_zec_usd_v1';

/// Dollars only mean something on mainnet: test coins have no value.
const kShowsFiat = kZafeNetwork == 'main';

/// A ZEC/USD price and when it was fetched.
class ZecPrice {
  const ZecPrice(this.usd, this.fetchedAt);
  final double usd;
  final DateTime fetchedAt;

  bool freshAt(DateTime now) {
    final age = now.toUtc().difference(fetchedAt.toUtc());
    return !age.isNegative && age < kZecPriceTtl;
  }

  String encode() =>
      jsonEncode({'usd': usd, 'at': fetchedAt.toUtc().millisecondsSinceEpoch});

  static ZecPrice? decode(String? raw) {
    if (raw == null) return null;
    try {
      final json = jsonDecode(raw);
      if (json is! Map) return null;
      final usd = json['usd'];
      final at = json['at'];
      if (usd is! num || at is! int || !usd.isFinite || usd <= 0) return null;
      return ZecPrice(
        usd.toDouble(),
        DateTime.fromMillisecondsSinceEpoch(at, isUtc: true),
      );
    } catch (_) {
      return null;
    }
  }
}

/// "$1,234.56" for `zat` at `usdPerZec`; null without a price.
String? fiatText(BigInt zat, double? usdPerZec) {
  if (usdPerZec == null) return null;
  final value = zat.toDouble() / zatoshiPerZec.toDouble() * usdPerZec;
  if (!value.isFinite) return null;
  final parts = value.toStringAsFixed(2).split('.');
  final whole = parts.first;
  final grouped = StringBuffer();
  for (var i = 0; i < whole.length; i++) {
    if (i > 0 && (whole.length - i) % 3 == 0) grouped.write(',');
    grouped.write(whole[i]);
  }
  return '\$$grouped.${parts.last}';
}

/// The ZEC/USD price to show, or null (not mainnet, never fetched, or older than
/// [kZecPriceTtl]). Starts from the cached price, refreshes every [kZecPriceRefresh]
/// while the app is in the foreground. Fetched in Rust, so it follows the Tor setting.
class ZecPriceNotifier extends Notifier<double?> {
  Timer? _timer;
  ZecPrice? _last;

  @override
  double? build() {
    if (!kShowsFiat) return null;
    final lifecycle = AppLifecycleListener(
      onResume: () => unawaited(_fetch()),
      onHide: () => _timer?.cancel(),
    );
    ref.onDispose(() {
      _timer?.cancel();
      lifecycle.dispose();
    });
    unawaited(_start());
    return null;
  }

  Future<void> _start() async {
    final prefs = await SharedPreferences.getInstance();
    final cached = ZecPrice.decode(prefs.getString(kZecPriceCacheKey));
    if (cached != null && cached.freshAt(DateTime.now())) {
      _last = cached;
      state = cached.usd;
    }
    await _fetch();
  }

  Future<void> _fetch() async {
    _timer?.cancel();
    _timer = Timer(kZecPriceRefresh, () => unawaited(_fetch()));
    try {
      final usd = await rust.zecUsdPrice();
      _last = ZecPrice(usd, DateTime.now());
      state = usd;
      final prefs = await SharedPreferences.getInstance();
      await prefs.setString(kZecPriceCacheKey, _last!.encode());
    } catch (e) {
      debugPrint('price failed: ${describeError(e)}');
      // Keep showing the last price until it's too old to mean anything.
      if (!(_last?.freshAt(DateTime.now()) ?? false)) state = null;
    }
  }
}

final zecPriceProvider = NotifierProvider<ZecPriceNotifier, double?>(
  ZecPriceNotifier.new,
);
