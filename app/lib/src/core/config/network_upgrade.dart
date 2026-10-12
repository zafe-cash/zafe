import '../../rust/api/proposals.dart' as rust;
import 'network_config.dart';

int? _nu7Height;
bool _nu7Read = false;

/// The block height of the NU7 network upgrade on this build's network, when the Zcash
/// libraries know it (null otherwise). Blocks come every 75 s before it and every 25 s after,
/// so a block count only becomes a time once this is known. Read once from Rust.
int? nu7ActivationHeight() {
  if (!_nu7Read) {
    _nu7Height = rust.nu7ActivationHeight(network: kZafeNetwork);
    _nu7Read = true;
  }
  return _nu7Height;
}
