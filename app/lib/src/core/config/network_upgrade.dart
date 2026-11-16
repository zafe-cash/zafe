import '../../rust/api/proposals.dart' as rust;
import 'network_config.dart';

int? _nu7Height;
bool _nu7Read = false;

int? nu7ActivationHeight() {
  if (!_nu7Read) {
    _nu7Height = rust.nu7ActivationHeight(network: kZafeNetwork);
    _nu7Read = true;
  }
  return _nu7Height;
}
