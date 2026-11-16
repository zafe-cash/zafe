import '../config/network_config.dart';
import 'privacy_mask.dart';

String amountWithTicker(
  String amountText, {
  required bool hide,
  int maskLength = kDefaultPrivacyMaskLength,
}) => hideAmountIfPrivacyMode(
  '$amountText $kZcashDefaultCurrencyTicker',
  privacyModeEnabled: hide,
  maskLength: maskLength,
);
