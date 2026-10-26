import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:go_router/go_router.dart';
import 'package:file_picker/file_picker.dart';
import 'package:mobile_scanner/mobile_scanner.dart';
import 'package:permission_handler/permission_handler.dart'
    show openAppSettings;

import '../../core/config/relay_presets.dart';
import '../../core/layout/mobile/mobile_top_nav.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/app_button.dart';
import '../../core/widgets/app_icon.dart';
import '../../rust/api/proposals.dart' as rust_proposals;
import '../../rust/api/repair.dart' as rust_repair;
import '../../rust/api/vault.dart' as rust;
import '../onboarding/invite_link.dart';

/// Camera scanner for a vault invite QR (raw invite or invite link). Pops with the raw
/// invite once a valid one is seen; the Join screen still asks before joining.
class ScanInviteScreen extends StatelessWidget {
  const ScanInviteScreen({super.key});

  @override
  Widget build(BuildContext context) => QrScanScreen(
    title: 'Scan invite',
    rejected:
        'That QR code isn\'t a Zafe invite. Scan the one on the vault creator\'s '
        'setup screen.',
    accept: (raw) {
      final invite = extractInvite(raw);
      if (invite == null) return null;
      try {
        rust.parseInvite(invite: invite);
        return invite;
      } catch (_) {
        return null;
      }
    },
  );
}

/// Scans the recovery code on a signer's new phone (spec §10.1). Pops the code.
class ScanRecoveryScreen extends StatelessWidget {
  const ScanRecoveryScreen({super.key});

  @override
  Widget build(BuildContext context) => QrScanScreen(
    title: 'Scan recovery code',
    rejected:
        'That QR code isn\'t a Zafe recovery code. Scan the one on the "Recover your '
        'seat" screen.',
    accept: (raw) {
      try {
        rust_repair.parseRecoveryCode(code: raw);
        return raw.trim();
      } catch (_) {
        return null;
      }
    },
  );
}

/// Scans a relay's address (`relay.example.org`, an https URL, either with
/// `#<create token>`). Pops the scanned text; the picker checks the relay.
class ScanRelayScreen extends StatelessWidget {
  const ScanRelayScreen({super.key});

  @override
  Widget build(BuildContext context) => QrScanScreen(
    title: 'Scan server',
    rejected: 'That QR code isn\'t a server address.',
    accept: (raw) => parseRelayInput(raw).url == null ? null : raw.trim(),
  );
}

/// Scans the vault link a co-signer shows (seat recovery). Pops the link text.
class ScanSeatLinkScreen extends StatelessWidget {
  const ScanSeatLinkScreen({super.key});

  @override
  Widget build(BuildContext context) => QrScanScreen(
    title: 'Scan vault link',
    rejected:
        'That QR code isn\'t a vault link. Scan the one on your co-signer\'s '
        '"Replace lost phone" screen.',
    accept: (raw) {
      try {
        rust_repair.parseSeatLink(text: raw);
        return raw.trim();
      } catch (_) {
        return null;
      }
    },
  );
}

/// Scans a recipient: an address or a `zcash:` payment link (amount, message, several
/// recipients). Pops the `ScannedRequest`.
class ScanRecipientScreen extends StatefulWidget {
  const ScanRecipientScreen({super.key, required this.network});
  final String network;

  @override
  State<ScanRecipientScreen> createState() => _ScanRecipientScreenState();
}

class _ScanRecipientScreenState extends State<ScanRecipientScreen> {
  String? _problem;

  @override
  Widget build(BuildContext context) => QrScanScreen(
    title: 'Scan recipient',
    rejected: _problem ?? 'That QR code isn\'t a Zcash address.',
    accept: (raw) {
      final request = rust_proposals.parsePaymentRequest(
        networkName: widget.network,
        text: raw,
      );
      if (request.problem.isEmpty) return request;
      if (_problem != request.problem) {
        WidgetsBinding.instance.addPostFrameCallback((_) {
          if (mounted) setState(() => _problem = request.problem);
        });
      }
      return null;
    },
  );
}

/// A camera QR scanner that pops the first code [accept] turns into a value; other codes
/// show [rejected].
class QrScanScreen extends StatefulWidget {
  const QrScanScreen({
    super.key,
    required this.title,
    this.prompt = '',
    required this.rejected,
    required this.accept,
  });
  final String title;
  final String prompt;
  final String rejected;

  /// The value to return for a scanned text, or `null` to keep scanning.
  final Object? Function(String raw) accept;

  @override
  State<QrScanScreen> createState() => _QrScanScreenState();
}

class _QrScanScreenState extends State<QrScanScreen>
    with WidgetsBindingObserver {
  final _controller = MobileScannerController(
    formats: const [BarcodeFormat.qrCode],
  );
  bool _done = false;
  bool _rejected = false;
  bool _noCodeInImage = false;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    unawaited(_controller.dispose());
    super.dispose();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    switch (state) {
      case AppLifecycleState.resumed:
        // Also picks up a permission granted in system settings meanwhile.
        unawaited(_start());
      case AppLifecycleState.inactive:
        // The permission prompt itself makes the app inactive: leave that alone.
        if (_controller.value.isRunning) unawaited(_controller.stop());
      case AppLifecycleState.detached:
      case AppLifecycleState.hidden:
      case AppLifecycleState.paused:
        break;
    }
  }

  Future<void> _start() async {
    if (_done) return;
    try {
      await _controller.start();
    } catch (_) {
      // Already starting, or failed: the error shows through the controller's state.
    }
  }

  /// Reads a QR code from a picture (a shared QR image or a screenshot of one).
  Future<void> _chooseImage() async {
    final picked = await FilePicker.pickFile(type: FileType.image);
    final path = picked?.path;
    if (path == null || !mounted) return;
    BarcodeCapture? capture;
    try {
      capture = await _controller.analyzeImage(
        path,
        formats: const [BarcodeFormat.qrCode],
      );
    } catch (_) {
      capture = null;
    }
    if (!mounted) return;
    if (capture == null || capture.barcodes.isEmpty) {
      setState(() => _noCodeInImage = true);
      return;
    }
    _noCodeInImage = false;
    _onDetect(capture);
  }

  void _onDetect(BarcodeCapture capture) {
    if (_done) return;
    for (final code in capture.barcodes) {
      final raw = code.rawValue;
      if (raw == null) continue;
      final value = widget.accept(raw);
      if (value != null) {
        _done = true;
        unawaited(HapticFeedback.selectionClick());
        context.pop(value);
        return;
      }
    }
    final sawCode = capture.barcodes.any((c) => c.rawValue != null);
    if (sawCode && !_rejected) setState(() => _rejected = true);
  }

  /// Keeps the camera view (and its state) when the layout below changes shape.
  final _cameraKey = GlobalKey();

  /// The camera above the prompt and the button; beside them in a short, wide window
  /// (landscape); scrolling under a camera of fixed height in a small one (a flip
  /// phone's cover screen).
  Widget _layout(BuildContext context, BoxConstraints box) {
    final colors = context.colors;
    final camera = KeyedSubtree(
      key: _cameraKey,
      child: ClipRRect(
        borderRadius: BorderRadius.circular(AppRadii.xLarge),
        child: ColoredBox(
          color: const Color(0xFF0C0C0C),
          child: MobileScanner(
            controller: _controller,
            onDetect: _onDetect,
            errorBuilder: (context, error) =>
                _ScannerError(error: error, onRetry: _start),
            overlayBuilder: (_, _) => const _Frame(),
          ),
        ),
      ),
    );
    final controls = [
      Text(
        _noCodeInImage
            ? 'No QR code found in that image. Choose a clearer one.'
            : _rejected
            ? widget.rejected
            : widget.prompt,
        textAlign: TextAlign.center,
        style: AppTypography.bodyMedium.copyWith(
          color: _rejected || _noCodeInImage
              ? colors.text.destructive
              : colors.text.secondary,
        ),
      ),
      const SizedBox(height: AppSpacing.sm),
      AppButton(
        expand: true,
        variant: AppButtonVariant.secondary,
        leading: const AppIcon(AppIcons.importWallet, size: 20),
        onPressed: _chooseImage,
        child: const Text('Choose image'),
      ),
    ];
    if (box.maxHeight >= 400) {
      return Column(
        children: [
          Expanded(child: camera),
          const SizedBox(height: AppSpacing.md),
          ...controls,
        ],
      );
    }
    if (box.maxWidth >= 480) {
      return Row(
        children: [
          Expanded(child: camera),
          const SizedBox(width: AppSpacing.md),
          Expanded(
            child: Center(
              child: SingleChildScrollView(
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: controls,
                ),
              ),
            ),
          ),
        ],
      );
    }
    return SingleChildScrollView(
      child: Column(
        children: [
          SizedBox(height: math.max(200, box.maxHeight * 0.75), child: camera),
          const SizedBox(height: AppSpacing.md),
          ...controls,
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return Scaffold(
      backgroundColor: colors.background.window,
      body: SafeArea(
        child: Column(
          children: [
            MobileTopNav.back(
              title: widget.title,
              onBack: () => context.pop(),
              trailing: _TorchButton(controller: _controller),
            ),
            Expanded(
              child: Padding(
                padding: const EdgeInsets.fromLTRB(16, 12, 16, 24),
                child: LayoutBuilder(builder: _layout),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// Rounded-rect guide in the middle of the camera view.
class _Frame extends StatelessWidget {
  const _Frame();

  @override
  Widget build(BuildContext context) {
    // 70% of the view's shorter side: its width on a phone, its height in a wide one.
    return LayoutBuilder(
      builder: (context, box) => Center(
        child: SizedBox.square(
          dimension: box.biggest.shortestSide * 0.7,
          child: DecoratedBox(
            decoration: BoxDecoration(
              borderRadius: BorderRadius.circular(AppRadii.large),
              border: Border.all(
                color: Colors.white.withValues(alpha: 0.85),
                width: 3,
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _TorchButton extends StatelessWidget {
  const _TorchButton({required this.controller});
  final MobileScannerController controller;

  @override
  Widget build(BuildContext context) {
    return ValueListenableBuilder<MobileScannerState>(
      valueListenable: controller,
      builder: (context, state, _) {
        final torch = state.torchState;
        if (torch == TorchState.unavailable || !state.isRunning) {
          return const SizedBox.shrink();
        }
        final on = torch == TorchState.on;
        return IconButton(
          tooltip: on ? 'Turn off flashlight' : 'Turn on flashlight',
          onPressed: () => unawaited(controller.toggleTorch()),
          icon: Icon(
            on ? Icons.flashlight_off_rounded : Icons.flashlight_on_rounded,
            color: context.colors.text.primary,
          ),
        );
      },
    );
  }
}

class _ScannerError extends StatelessWidget {
  const _ScannerError({required this.error, required this.onRetry});
  final MobileScannerException error;
  final Future<void> Function() onRetry;

  @override
  Widget build(BuildContext context) {
    final denied = error.errorCode == MobileScannerErrorCode.permissionDenied;
    const light = Color(0xFFE8ECF2);
    return Padding(
      padding: const EdgeInsets.all(AppSpacing.md),
      child: Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            AppIcon(
              denied ? AppIcons.cameraDenied : AppIcons.warningCircle,
              size: 40,
              color: light,
            ),
            const SizedBox(height: AppSpacing.sm),
            Text(
              denied ? 'Camera access is off' : 'The camera didn\'t start',
              textAlign: TextAlign.center,
              style: AppTypography.bodyLarge.copyWith(
                color: light,
                fontWeight: FontWeight.w600,
              ),
            ),
            const SizedBox(height: AppSpacing.xxs),
            Text(
              denied
                  ? 'Allow camera access in your phone\'s settings, or go back and paste it instead.'
                  : 'Try again, or go back and paste it instead.',
              textAlign: TextAlign.center,
              style: AppTypography.bodySmall.copyWith(
                color: light.withValues(alpha: 0.75),
              ),
            ),
            const SizedBox(height: AppSpacing.md),
            if (denied)
              AppButton(
                onPressed: () => unawaited(openAppSettings()),
                leading: const AppIcon(AppIcons.cog, size: 20),
                child: const Text('Open settings'),
              ),
            const SizedBox(height: AppSpacing.xs),
            AppButton(
              variant: AppButtonVariant.secondary,
              onPressed: () => unawaited(onRetry()),
              child: const Text('Try again'),
            ),
          ],
        ),
      ),
    );
  }
}
