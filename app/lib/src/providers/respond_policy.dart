class RespondPolicy {
  static const maxQuiet = Duration(seconds: 60);

  bool live = false;

  bool _news = true;
  DateTime? _last;

  void inboxNews() => _news = true;

  bool take(DateTime now) {
    final last = _last;
    final due =
        !live || _news || last == null || now.difference(last) >= maxQuiet;
    if (!due) return false;
    _news = false;
    _last = now;
    return true;
  }

  void failed() => _news = true;
}
