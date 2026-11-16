import 'package:flutter_riverpod/flutter_riverpod.dart';

class AppNotice {
  const AppNotice({required this.id, required this.title, required this.body});

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
