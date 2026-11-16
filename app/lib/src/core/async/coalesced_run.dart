class CoalescedRun {
  CoalescedRun({this.maxRuns = 4});

  final int maxRuns;

  bool _busy = false;
  bool _again = false;
  bool _againForce = false;

  bool get busy => _busy;

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

  void again({bool force = true}) {
    _again = true;
    _againForce = _againForce || force;
  }
}
