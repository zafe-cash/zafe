import 'package:flutter_riverpod/flutter_riverpod.dart';

/// Something that happened without a tap (a payment link expired, sync moved to
/// another server): shown as a card on Home, or on Welcome before any vault exists,
/// until dismissed. Not for the result of an action: that shows where it was done.
class AppNotice {
  const AppNotice({required this.id, required this.title, required this.body});

  /// A newer notice with the same id replaces the older one.
  final String id;
  final String title;
  final String body;
}

class AppNoticesNotifier extends Notifier<List<AppNotice>> {
  @override
  List<AppNotice> build() => const [];

  void show(AppNotice notice) =>
      state = [...state.where((n) => n.id != notice.id), notice];

  void dismiss(String id) => state = [...state.where((n) => n.id != id)];
}

final appNoticesProvider =
    NotifierProvider<AppNoticesNotifier, List<AppNotice>>(
      AppNoticesNotifier.new,
    );
