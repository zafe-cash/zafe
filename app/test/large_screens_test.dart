// Large screens, foldables and multi-window: which windows rotate, where a page's column
// goes (centred, clear of a hinge), and what the column reports below it.
import 'dart:ui';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:zafe/src/core/layout/app_column.dart';
import 'package:zafe/src/core/platform/orientation_policy.dart';

MediaQueryData _media(
  Size size, {
  EdgeInsets padding = EdgeInsets.zero,
  List<DisplayFeature> features = const [],
}) => MediaQueryData(
  size: size,
  padding: padding,
  viewPadding: padding,
  displayFeatures: features,
);

void main() {
  group('allowsRotation', () {
    bool? rotates(Size display, [Size? window]) =>
        allowsRotation(display: display, window: window ?? display);

    test('phones, cover screens and flip phones stay portrait', () {
      expect(rotates(const Size(390, 844)), isFalse);
      expect(
        rotates(const Size(344, 882)),
        isFalse,
      ); // a foldable's cover screen
      expect(rotates(const Size(412, 1005)), isFalse); // a flip phone, open
      expect(
        rotates(const Size(466, 678)),
        isFalse,
      ); // a short, wide outer screen
    });

    test('tablets and unfolded foldables rotate, also in split screen', () {
      expect(rotates(const Size(800, 1280)), isTrue);
      expect(
        rotates(const Size(673, 841)),
        isTrue,
      ); // a foldable's inner screen
      expect(rotates(const Size(1280, 800), const Size(640, 800)), isTrue);
    });

    test('a window larger than the display reported at start still counts', () {
      // iOS reads the display once, at start: on the outer screen, then unfolded.
      expect(rotates(const Size(466, 678), const Size(626, 890)), isTrue);
    });

    test('nothing is decided before any size is known', () {
      expect(rotates(Size.zero), isNull);
    });
  });

  group('appColumnRect', () {
    test('a phone gets the whole window', () {
      const size = Size(412, 915);
      expect(
        appColumnRect(_media(size), TextDirection.ltr),
        Offset.zero & size,
      );
    });

    test('a wide window gets a centred column', () {
      expect(
        appColumnRect(_media(const Size(1280, 800)), TextDirection.ltr),
        const Rect.fromLTWH(360, 0, kMaxContentWidth, 800),
      );
    });

    test('a hinge: the start side, never across it', () {
      final media = _media(
        const Size(1114, 720),
        features: const [
          DisplayFeature(
            bounds: Rect.fromLTWH(540, 0, 34, 720),
            type: DisplayFeatureType.hinge,
            state: DisplayFeatureState.unknown,
          ),
        ],
      );
      expect(
        appColumnRect(media, TextDirection.ltr),
        const Rect.fromLTWH(0, 0, 540, 720),
      );
      expect(
        appColumnRect(media, TextDirection.rtl),
        const Rect.fromLTWH(574, 0, 540, 720),
      );
    });

    test('a half-opened fold splits the window; a flat one does not', () {
      DisplayFeature fold(DisplayFeatureState state) => DisplayFeature(
        bounds: const Rect.fromLTWH(0, 450, 840, 0),
        type: DisplayFeatureType.fold,
        state: state,
      );
      const size = Size(840, 900);
      expect(
        appColumnRect(
          _media(size, features: [fold(DisplayFeatureState.postureHalfOpened)]),
          TextDirection.ltr,
        ),
        const Rect.fromLTWH(140, 0, kMaxContentWidth, 450),
      );
      expect(
        appColumnRect(
          _media(size, features: [fold(DisplayFeatureState.postureFlat)]),
          TextDirection.ltr,
        ),
        const Rect.fromLTWH(140, 0, kMaxContentWidth, 900),
      );
    });
  });

  test('the column reports its own size, insets and features', () {
    final media = _media(
      const Size(1114, 720),
      padding: const EdgeInsets.fromLTRB(30, 24, 30, 16),
      features: const [
        DisplayFeature(
          bounds: Rect.fromLTWH(540, 0, 34, 720),
          type: DisplayFeatureType.hinge,
          state: DisplayFeatureState.unknown,
        ),
      ],
    );
    final inside = columnMediaQuery(
      media,
      appColumnRect(media, TextDirection.ltr),
    );
    expect(inside.size, const Size(540, 720));
    // The right inset is on the other screen.
    expect(inside.padding, const EdgeInsets.fromLTRB(30, 24, 0, 16));
    expect(inside.displayFeatures, isEmpty);
  });

  testWidgets('phones lock to portrait; unfolding unlocks, folding locks', (
    tester,
  ) async {
    final calls = <Object?>[];
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.platform,
      (call) async {
        if (call.method == 'SystemChrome.setPreferredOrientations') {
          calls.add(call.arguments);
        }
        return null;
      },
    );
    addTearDown(tester.view.reset);
    addTearDown(tester.view.display.reset);
    void show(Size logical) {
      tester.view.display.size = logical * tester.view.devicePixelRatio;
      tester.view.physicalSize = logical * tester.view.devicePixelRatio;
    }

    show(const Size(344, 882)); // cover screen
    OrientationPolicy.install();
    await tester.pump();
    expect(calls, [
      ['DeviceOrientation.portraitUp'],
    ]);

    show(const Size(673, 841)); // unfolded
    await tester.pump();
    expect(calls.last, isEmpty); // whatever the platform allows

    show(const Size(841, 673)); // rotated: nothing new to ask
    await tester.pump();
    expect(calls, hasLength(2));

    show(const Size(344, 882)); // folded again
    await tester.pump();
    expect(calls.last, ['DeviceOrientation.portraitUp']);
  });
}
