/// When a refresh also answers signing requests (`rust.answerSigningRequests`): it reads
/// the inbox every time, about 2 s over Tor, and with one-tap signing there is rarely
/// anything in it. While the live watch runs it says when something was delivered; the
/// inbox is still read once a minute (it also acknowledges finished messages). Without
/// the watch every refresh answers, as the poll is then the only way to notice requests.
/// Pure, so it can be unit-tested.
class RespondPolicy {
  /// The longest a refresh goes without reading the inbox while the watch is live.
  static const maxQuiet = Duration(seconds: 60);

  /// Whether the live watch reaches the relay (it then reports deliveries).
  bool live = false;

  bool _news = true;
  DateTime? _last;

  /// Something was delivered to this member (or may have been).
  void inboxNews() => _news = true;

  /// Whether this refresh should answer; if so, the news so far counts as handled (news
  /// arriving while it answers is kept for the next refresh).
  bool take(DateTime now) {
    final last = _last;
    final due =
        !live || _news || last == null || now.difference(last) >= maxQuiet;
    if (!due) return false;
    _news = false;
    _last = now;
    return true;
  }

  /// Answering failed: try again on the next refresh.
  void failed() => _news = true;
}
