/// Runs one task at a time without dropping requests: a call while it runs asks for one
/// more run after it (any number of calls meanwhile = one more run), with `force` if any
/// of them asked for it. A guard that just returned lost the request (a vault switched to
/// mid-sync, a server change, a send that just finished waited for the next poll).
class CoalescedRun {
  CoalescedRun({this.maxRuns = 4});

  /// Bounded, so a stream of requests can't keep one call alive.
  final int maxRuns;

  bool _busy = false;
  bool _again = false;
  bool _againForce = false;

  bool get busy => _busy;

  /// Runs [task] now, or once more after the running one.
  Future<void> run(bool force, Future<void> Function(bool force) task) async {
    if (_busy) {
      again(force: force);
      return;
    }
    _busy = true;
    try {
      var f = force;
      for (var i = 0; i < maxRuns; i++) {
        _again = false;
        _againForce = false;
        await task(f);
        if (!_again) break;
        f = _againForce;
      }
    } finally {
      _busy = false;
    }
  }

  /// From inside the task: run again after this pass.
  void again({bool force = true}) {
    _again = true;
    _againForce = _againForce || force;
  }
}
