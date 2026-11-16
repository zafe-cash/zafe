import 'package:flutter/material.dart';

import '../app_column.dart';

class PreparedModalSheetRoute<T> extends ModalBottomSheetRoute<T> {
  PreparedModalSheetRoute({
    required super.builder,
    required super.capturedThemes,
    required super.modalBarrierColor,
    required super.barrierLabel,
    required super.barrierOnTapHint,
    required super.isDismissible,
    required super.enableDrag,
  }) : super(
         isScrollControlled: true,
         useSafeArea: true,
         backgroundColor: Colors.transparent,
         elevation: 0,
         constraints: const BoxConstraints(maxWidth: kMaxContentWidth),
       );

  bool _waiting = true;
  bool _disposed = false;
  bool _popped = false;

  @override
  void install() {
    super.install();
    offstage = true;
  }

  @override
  TickerFuture didPush() {
    final pushed = super.didPush();
    controller!.stop(canceled: false);
    return pushed;
  }

  void cornersReady() {
    if (!_waiting || _disposed) return;
    _waiting = false;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (_disposed || _popped || !isActive) return;
      offstage = false;
      controller!.forward(from: 0);
    });
    WidgetsBinding.instance.ensureVisualUpdate();
  }

  @override
  bool didPop(T? result) {
    _waiting = false;
    _popped = true;
    return super.didPop(result);
  }

  @override
  void dispose() {
    _disposed = true;
    super.dispose();
  }
}
