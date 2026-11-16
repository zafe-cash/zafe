import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../core/config/endpoints.dart';
import 'vault_provider.dart';

class EndpointsNotifier extends Notifier<ZafeEndpoints> {
  @override
  ZafeEndpoints build() => ref.watch(vaultBootstrapProvider).endpoints;

  Future<void> set(EndpointKind kind, String url) async {
    await ZafeEndpoints.save(kind, url);
    state = state.withUrl(kind, url);
  }

  Future<void> reset(EndpointKind kind) =>
      set(kind, ZafeEndpoints.defaults.url(kind));
}

final endpointsProvider = NotifierProvider<EndpointsNotifier, ZafeEndpoints>(
  EndpointsNotifier.new,
);
