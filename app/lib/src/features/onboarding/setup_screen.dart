import 'dart:async';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:path_provider/path_provider.dart';
import 'package:pretty_qr_code/pretty_qr_code.dart';
import 'package:share_plus/share_plus.dart';

import '../../core/layout/mobile/app_mobile_sheet.dart';
import '../../core/layout/mobile/zafe_screen.dart';
import '../../core/formatting/member_label.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/app_button.dart';
import '../../core/widgets/app_copy_feedback.dart';
import '../../core/widgets/app_icon.dart';
import '../../core/widgets/app_toast.dart';
import '../../core/widgets/confirm_option.dart';
import '../../core/widgets/dot_qr_shape.dart';
import '../../core/widgets/mobile/mobile_surface_card.dart';
import '../../providers/vault_provider.dart';
import '../../core/errors/zafe_error_copy.dart';
import '../../rust/api/error.dart' show ZafeError, ZafeErrorKind;
import '../../rust/api/vault.dart' as rust;
import 'invite_link.dart';
import 'onboarding_art.dart';

/// Vault setup: invite, members joining, locking membership, comparing the safety
/// words, and key generation (spec §7.2).
class SetupScreen extends ConsumerStatefulWidget {
  const SetupScreen({super.key});

  @override
  ConsumerState<SetupScreen> createState() => _SetupScreenState();
}

class _SetupScreenState extends ConsumerState<SetupScreen> {
  Timer? _poll;
  bool _compared = false;
  bool _busy = false;
  bool _creatingKeys = false;
  bool _leaving = false;

  /// The relay no longer has this setup: the creator cancelled it.
  bool _gone = false;
  String? _error;

  /// Key generation timed out: the members (keys) who never took part.
  List<String>? _stalled;

  @override
  void initState() {
    super.initState();
    _refresh();
    _poll = Timer.periodic(const Duration(seconds: 2), (_) => _refresh());
  }

  @override
  void dispose() {
    _poll?.cancel();
    super.dispose();
  }

  Future<void> _refresh() async {
    if (_creatingKeys || _leaving) return;
    try {
      await ref.read(vaultProvider.notifier).refreshMembership();
      if (mounted && (_error != null || _gone)) {
        setState(() {
          _error = null;
          _gone = false;
        });
      }
    } catch (e) {
      if (!mounted || _leaving) return;
      if (e is ZafeError && e.kind == ZafeErrorKind.vaultNotOnRelay) {
        // The relay answered, and the setup isn't there any more.
        setState(() {
          _gone = true;
          _error = null;
        });
      } else {
        setState(() => _error = 'Can\'t reach the relay. Retrying...');
      }
    }
  }

  /// Backs out of the setup before membership is locked. The creator's setup is deleted
  /// on the relay (the invite stops working) and the "New vault" form comes back filled
  /// in; a joiner just leaves.
  Future<void> _cancel({required bool creator, bool confirm = true}) async {
    if (_busy || _leaving) return;
    if (confirm) {
      final go = await showAppMobileSheet<bool>(
        context: context,
        builder: (sheet) {
          final colors = sheet.colors;
          return MobileModalScaffold(
            title: creator ? 'Cancel this setup?' : 'Leave this setup?',
            onClose: () => Navigator.of(sheet).pop(false),
            constrainBody: true,
            child: SingleChildScrollView(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Text(
                    creator
                        ? 'The invite stops working, and anyone who already joined will see '
                              'that you cancelled. You can set it up again with the same '
                              'details, and send a new invite.'
                        : 'You\'ll need a new invite to join again.',
                    style: AppTypography.bodyMedium.copyWith(
                      color: colors.text.accent,
                    ),
                  ),
                  const SizedBox(height: AppSpacing.md),
                  AppButton(
                    expand: true,
                    variant: AppButtonVariant.destructive,
                    onPressed: () => Navigator.of(sheet).pop(true),
                    child: Text(creator ? 'Cancel setup' : 'Leave'),
                  ),
                  const SizedBox(height: AppSpacing.xs),
                  AppButton(
                    expand: true,
                    variant: AppButtonVariant.ghost,
                    onPressed: () => Navigator.of(sheet).pop(false),
                    child: const Text('Keep waiting'),
                  ),
                ],
              ),
            ),
          );
        },
      );
      if (go != true || !mounted) return;
    }
    setState(() => _leaving = true);
    _poll?.cancel();
    final notifier = ref.read(vaultProvider.notifier);
    final result = await notifier.cancelSetup();
    if (!mounted) return;
    if (result.creator) {
      // Back to "New vault", filled in. With other vaults on this device, the onboarding
      // screens are shown the same way "Add vault" does.
      if (ref.read(vaultProvider).hasVault) notifier.beginAddVault();
      if (!result.deleted) {
        showAppToast(
          context,
          'Cancelled here, but the relay didn\'t confirm. The old invite may still work for a while.',
          iconName: AppIcons.warningCircle,
        );
      }
      context.go('/create');
    } else {
      context.go(ref.read(vaultProvider).hasVault ? '/home' : '/welcome');
    }
  }

  Future<void> _seal() async {
    setState(() => _busy = true);
    try {
      await ref.read(vaultProvider.notifier).seal();
    } catch (e) {
      if (mounted) {
        showAppToast(
          context,
          zafeErrorMessage(
            e,
            fallback: 'Couldn\'t lock membership. Try again.',
          ),
          iconName: AppIcons.warningCircle,
          tone: AppToastTone.destructive,
        );
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  /// After membership is locked: leaves the setup on this phone (a member never took
  /// part, or the group gave up). Everyone else has to do the same.
  Future<void> _abandon() async {
    if (_busy || _leaving) return;
    final go = await showAppMobileSheet<bool>(
      context: context,
      builder: (sheet) {
        final colors = sheet.colors;
        return MobileModalScaffold(
          title: 'Abandon this setup?',
          onClose: () => Navigator.of(sheet).pop(false),
          constrainBody: true,
          child: SingleChildScrollView(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Text(
                  'Nothing is lost: no keys or funds exist yet. Tell the other members, '
                  'so they abandon it too, then start again with a new invite.',
                  style: AppTypography.bodyMedium.copyWith(
                    color: colors.text.accent,
                  ),
                ),
                const SizedBox(height: AppSpacing.md),
                AppButton(
                  expand: true,
                  variant: AppButtonVariant.destructive,
                  onPressed: () => Navigator.of(sheet).pop(true),
                  child: const Text('Abandon setup'),
                ),
                const SizedBox(height: AppSpacing.xs),
                AppButton(
                  expand: true,
                  variant: AppButtonVariant.ghost,
                  onPressed: () => Navigator.of(sheet).pop(false),
                  child: const Text('Keep waiting'),
                ),
              ],
            ),
          ),
        );
      },
    );
    if (go != true || !mounted) return;
    setState(() => _leaving = true);
    _poll?.cancel();
    await ref.read(vaultProvider.notifier).abandonSetup();
    if (!mounted) return;
    context.go(ref.read(vaultProvider).hasVault ? '/home' : '/welcome');
  }

  Future<void> _createKeys(String safetyNumber) async {
    setState(() {
      _creatingKeys = true;
      _stalled = null;
    });
    _poll?.cancel();
    try {
      await ref.read(vaultProvider.notifier).createKeys(safetyNumber);
      // Right after creation: the key share now exists only on this phone.
      if (mounted) context.go('/backup-prompt');
    } catch (e) {
      if (mounted) {
        final stalled =
            e is ZafeError && e.kind == ZafeErrorKind.keygenIncomplete;
        setState(() {
          _creatingKeys = false;
          _stalled = stalled ? e.members : null;
        });
        if (!stalled) {
          showAppToast(
            context,
            zafeErrorMessage(e, fallback: 'Key generation failed. Try again.'),
            iconName: AppIcons.warningCircle,
            tone: AppToastTone.destructive,
          );
        }
        _poll = Timer.periodic(const Duration(seconds: 2), (_) => _refresh());
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final vault = ref.watch(vaultProvider);
    final info = vault.inviteInfo;
    final membership = vault.membership;
    if (info == null) return const SizedBox.shrink();

    final joined = membership?.members.length ?? 1;
    final everyoneJoined = joined == info.members;
    final sealed = membership?.sealed ?? false;
    final isCreator = membership?.isCreator ?? false;

    if (_creatingKeys) return const _CreatingKeys();

    Widget? bottom;
    if (_gone && !sealed) {
      bottom = AppButton(
        expand: true,
        onPressed: () => _cancel(creator: false, confirm: false),
        child: const Text('Leave setup'),
      );
    } else if (sealed) {
      final words = membership?.safetyNumber;
      bottom = AppButton(
        expand: true,
        onPressed: _compared && words != null ? () => _createKeys(words) : null,
        leading: const AppIcon(AppIcons.key, size: 20),
        child: Text(_stalled == null ? 'Create vault keys' : 'Try again'),
      );
    } else if (isCreator) {
      bottom = AppButton(
        expand: true,
        onPressed: everyoneJoined && !_busy ? _seal : null,
        leading: const AppIcon(AppIcons.lock, size: 20),
        child: Text(
          everyoneJoined ? 'Lock membership' : 'Waiting for members...',
        ),
      );
    }

    return PopScope(
      // Android's back button backs out of the setup too (it asks first); once
      // membership is locked there is no way back.
      canPop: false,
      onPopInvokedWithResult: (didPop, _) {
        if (!didPop && !sealed && !_gone) _cancel(creator: isCreator);
      },
      child: ZafeScreen(
        title: info.name,
        showBack: false,
        bottom: bottom,
        children: [
          if (_gone && !sealed) ...[
            _Hint(
              text: isCreator
                  ? 'This setup is no longer on the relay.'
                  : 'The creator cancelled this setup. Ask them for a new invite.',
            ),
            const SizedBox(height: AppSpacing.md),
          ],
          if (!sealed && isCreator) ...[
            _InviteCard(invite: vault.invite!, vaultName: info.name),
            const SizedBox(height: AppSpacing.md),
          ],
          _MembersCard(
            members: membership?.members ?? const [],
            expected: info.members,
            threshold: info.threshold,
            myKeyHex: _myKeyHex(vault),
            notHere: sealed ? membership?.waitingFor ?? const [] : const [],
          ),
          const SizedBox(height: AppSpacing.md),
          if (_error != null)
            Text(
              _error!,
              style: AppTypography.bodySmall.copyWith(
                color: context.colors.text.warning,
              ),
            ),
          if (!sealed && !isCreator)
            _Hint(
              text: everyoneJoined
                  ? 'Everyone has joined. Waiting for the creator to lock membership.'
                  : 'Waiting for the other members to join with the invite.',
            ),
          if (sealed && membership != null) ...[
            SetupSafetyWordsCard(
              words: membership.safetyNumber,
              waiting: membership.waitingFor.length,
            ),
            if (membership.safetyNumber != null) ...[
              const SizedBox(height: AppSpacing.sm),
              ConfirmOption(
                selected: _compared,
                label: 'I compared these words with every member',
                onTap: () => setState(() => _compared = !_compared),
              ),
            ],
          ],
          if (sealed && _stalled != null) ...[
            const SizedBox(height: AppSpacing.md),
            _Hint(text: stalledText(_stalled!, me: _myKeyHex(vault))),
          ],
          if (sealed) ...[
            const SizedBox(height: AppSpacing.sm),
            AppButton(
              expand: true,
              variant: AppButtonVariant.ghost,
              onPressed: _busy || _leaving ? null : _abandon,
              child: const Text('Abandon setup'),
            ),
          ],
          if (!sealed && !_gone) ...[
            const SizedBox(height: AppSpacing.sm),
            AppButton(
              expand: true,
              variant: AppButtonVariant.ghost,
              onPressed: _busy || _leaving
                  ? null
                  : () => _cancel(creator: isCreator),
              child: Text(isCreator ? 'Cancel setup' : 'Leave setup'),
            ),
          ],
        ],
      ),
    );
  }

  String? _myKeyHex(VaultState vault) {
    final id = vault.identity;
    return id == null ? null : rust.identityPublicKey(seeds: id);
  }
}

class _InviteCard extends StatelessWidget {
  const _InviteCard({required this.invite, required this.vaultName});
  final String invite;
  final String vaultName;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    // The QR carries the link too, so a phone's own camera app can open Zafe with it.
    final link = inviteLink(invite);
    return MobileSurfaceCard(
      cornerRadius: AppRadii.large,
      padding: const EdgeInsets.fromLTRB(16, 24, 16, 16),
      child: Column(
        children: [
          Text(
            'Invite members',
            style: AppTypography.bodyLarge.copyWith(
              color: colors.text.accent,
              fontWeight: FontWeight.w600,
            ),
          ),
          const SizedBox(height: AppSpacing.xxs),
          Text(
            'Send it only to the people who will co-sign, over a channel you trust. Anyone '
            'with it can take a seat until you lock membership, so check the member list '
            'before you do.',
            textAlign: TextAlign.center,
            style: AppTypography.bodySmall.copyWith(
              color: colors.text.secondary,
            ),
          ),
          const SizedBox(height: AppSpacing.sm),
          Container(
            width: 220,
            height: 220,
            padding: const EdgeInsets.all(AppSpacing.sm),
            decoration: BoxDecoration(
              color: colors.surface.qrCode,
              borderRadius: BorderRadius.circular(AppRadii.large),
            ),
            child: PrettyQrView.data(
              data: link,
              decoration: const PrettyQrDecoration(
                shape: DotQrShape(color: Color(0xFF0C0C0C)),
              ),
            ),
          ),
          const SizedBox(height: AppSpacing.sm),
          Row(
            children: [
              Expanded(
                child: AppButton(
                  variant: AppButtonVariant.secondary,
                  onPressed: () => SharePlus.instance.share(
                    ShareParams(
                      subject: 'Zafe vault invite',
                      text: 'Join "$vaultName" on Zafe: $link',
                    ),
                  ),
                  leading: const AppIcon(AppIcons.share, size: 20),
                  child: const Text('Share link'),
                ),
              ),
              const SizedBox(width: AppSpacing.xs),
              Expanded(
                child: AppButton(
                  variant: AppButtonVariant.secondary,
                  onPressed: () => copyTextWithToast(
                    context,
                    text: link,
                    toastMessage: 'Invite link copied',
                    sensitive: true,
                  ),
                  leading: const AppIcon(AppIcons.copy, size: 20),
                  child: const Text('Copy'),
                ),
              ),
            ],
          ),
          const SizedBox(height: AppSpacing.xs),
          AppButton(
            expand: true,
            variant: AppButtonVariant.ghost,
            onPressed: () => shareInviteQrImage(link, vaultName),
            leading: const AppIcon(AppIcons.qr, size: 20),
            child: const Text('Share QR image'),
          ),
        ],
      ),
    );
  }
}

/// Shares the invite QR as a PNG (the screen itself blocks screenshots), for a co-signer
/// to scan from another screen or pick with "Choose image" on Join. Same data as the
/// link, so the same warning applies. Plain black squares on white with a quiet zone, so
/// any scanner reads it, even shown on a dark background.
Future<void> shareInviteQrImage(String link, String vaultName) async {
  final qr = QrImage(
    QrCode.fromData(data: link, errorCorrectLevel: QrErrorCorrectLevel.M),
  );
  final bytes = await qr.toImageAsBytes(
    size: 1024,
    decoration: const PrettyQrDecoration(
      background: Color(0xFFFFFFFF),
      quietZone: PrettyQrQuietZone.standard,
    ),
  );
  if (bytes == null) return;
  final safe = vaultName.replaceAll(RegExp(r'[^A-Za-z0-9_-]+'), '-');
  final name = 'Zafe-invite-$safe.png';
  final file = File('${(await getTemporaryDirectory()).path}/$name');
  await file.writeAsBytes(bytes.buffer.asUint8List(), flush: true);
  await SharePlus.instance.share(
    ShareParams(
      files: [XFile(file.path, mimeType: 'image/png')],
      fileNameOverrides: [name],
      text:
          'Zafe invite to "$vaultName". Scan it in Zafe (Join, then Scan QR code, or '
          'Choose image). Only for the people who will co-sign.',
    ),
  );
  // The share sheet has copied or sent it; don't leave the invite in the cache.
  try {
    await file.delete();
  } catch (_) {}
}

class _MembersCard extends StatelessWidget {
  const _MembersCard({
    required this.members,
    required this.expected,
    required this.threshold,
    required this.myKeyHex,
    this.notHere = const [],
  });

  final List<String> members;
  final int expected;
  final int threshold;
  final String? myKeyHex;

  /// Members whose phone hasn't sent its part of the safety check yet.
  final List<String> notHere;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return MobileSurfaceCard(
      cornerRadius: AppRadii.large,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Expanded(
                child: Text(
                  'Members',
                  style: AppTypography.labelLarge.copyWith(
                    color: colors.text.accent,
                    fontWeight: FontWeight.w600,
                  ),
                ),
              ),
              Text(
                '${members.length} of $expected joined',
                style: AppTypography.labelMedium.copyWith(
                  color: colors.text.secondary,
                ),
              ),
            ],
          ),
          const SizedBox(height: AppSpacing.xs),
          for (final m in members)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: AppSpacing.xs),
              child: Row(
                children: [
                  Container(
                    width: 32,
                    height: 32,
                    decoration: BoxDecoration(
                      color: colors.background.neutralSubtleOpacity,
                      shape: BoxShape.circle,
                    ),
                    alignment: Alignment.center,
                    child: AppIcon(
                      AppIcons.user,
                      size: 16,
                      color: colors.icon.regular,
                    ),
                  ),
                  const SizedBox(width: AppSpacing.s),
                  Text(
                    '${m.substring(0, 8)}...${m.substring(m.length - 6)}',
                    style: AppTypography.codeSmall.copyWith(
                      color: colors.text.primary,
                    ),
                  ),
                  if (m == myKeyHex) ...[
                    const SizedBox(width: AppSpacing.xs),
                    Text(
                      'You',
                      style: AppTypography.labelMedium.copyWith(
                        color: colors.text.secondary,
                      ),
                    ),
                  ],
                  if (notHere.contains(m)) ...[
                    const Spacer(),
                    Text(
                      'Not here yet',
                      style: AppTypography.labelMedium.copyWith(
                        color: colors.text.muted,
                      ),
                    ),
                  ],
                ],
              ),
            ),
          for (var i = members.length; i < expected; i++)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: AppSpacing.xs),
              child: Row(
                children: [
                  Container(
                    width: 32,
                    height: 32,
                    decoration: BoxDecoration(
                      shape: BoxShape.circle,
                      border: Border.all(color: colors.border.regular),
                    ),
                  ),
                  const SizedBox(width: AppSpacing.s),
                  Text(
                    'Waiting...',
                    style: AppTypography.bodySmall.copyWith(
                      color: colors.text.muted,
                    ),
                  ),
                ],
              ),
            ),
          const SizedBox(height: AppSpacing.xs),
          Text(
            '$threshold signatures needed for each payment.',
            style: AppTypography.bodySmall.copyWith(
              color: colors.text.secondary,
            ),
          ),
        ],
      ),
    );
  }
}

class SetupSafetyWordsCard extends StatelessWidget {
  const SetupSafetyWordsCard({
    super.key,
    required this.words,
    required this.waiting,
  });

  /// The four words, or `null` while members' parts of the check are still arriving.
  final String? words;

  /// How many members haven't sent their part yet.
  final int waiting;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final words = this.words;
    final muted = AppTypography.bodySmall.copyWith(
      color: colors.text.darkCard.withValues(alpha: 0.7),
    );
    return Container(
      padding: const EdgeInsets.fromLTRB(16, 24, 16, 24),
      decoration: BoxDecoration(
        color: colors.background.darkCard,
        borderRadius: BorderRadius.circular(AppRadii.large),
        border: Border.all(color: const Color(0x12FFFFFF), width: 1.5),
      ),
      child: Column(
        children: [
          Row(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              AppIcon(
                AppIcons.shieldKeyhole,
                size: 20,
                color: colors.text.darkCard,
              ),
              const SizedBox(width: AppSpacing.xs),
              Text(
                'Safety words',
                style: AppTypography.labelMedium.copyWith(
                  color: colors.text.darkCard,
                ),
              ),
            ],
          ),
          const SizedBox(height: AppSpacing.sm),
          if (words == null) ...[
            AppIcon(AppIcons.loader, size: 24, color: colors.text.darkCard),
            const SizedBox(height: AppSpacing.sm),
            Text(
              waiting == 1
                  ? 'Waiting for 1 member to open Zafe. The words appear once every member\'s '
                        'phone has taken part.'
                  : 'Waiting for $waiting members to open Zafe. The words appear once every '
                        'member\'s phone has taken part.',
              textAlign: TextAlign.center,
              style: muted,
            ),
          ] else ...[
            Wrap(
              alignment: WrapAlignment.center,
              spacing: AppSpacing.md,
              runSpacing: AppSpacing.xs,
              children: [
                for (final w in words.split(' '))
                  Text(
                    w,
                    style: AppTypography.headlineLarge.copyWith(
                      color: colors.text.darkCard,
                    ),
                  ),
              ],
            ),
            const SizedBox(height: AppSpacing.sm),
            Text(
              'Read them to each member over a call, or compare in person. Every member must '
              'see the same words. If anyone sees different ones, stop: someone may be '
              'impersonating a member.',
              textAlign: TextAlign.center,
              style: muted,
            ),
          ],
        ],
      ),
    );
  }
}

class _Hint extends StatelessWidget {
  const _Hint({required this.text});
  final String text;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return Row(
      children: [
        AppIcon(AppIcons.loader, size: 16, color: colors.icon.muted),
        const SizedBox(width: AppSpacing.xs),
        Expanded(
          child: Text(
            text,
            style: AppTypography.bodySmall.copyWith(
              color: colors.text.secondary,
            ),
          ),
        ),
      ],
    );
  }
}

class _CreatingKeys extends StatelessWidget {
  const _CreatingKeys();

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    return Scaffold(
      backgroundColor: colors.background.window,
      body: Stack(
        fit: StackFit.expand,
        children: [
          const OnboardingHero('key_shards'),
          SafeArea(
            child: Padding(
              padding: const EdgeInsets.all(AppSpacing.sm),
              child: Column(
                mainAxisAlignment: MainAxisAlignment.end,
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  AppIcon(AppIcons.loader, size: 24, color: colors.icon.accent),
                  const SizedBox(height: AppSpacing.sm),
                  Text(
                    'Creating your vault keys...',
                    style: TextStyle(
                      fontFamily: 'Space Grotesk',
                      fontSize: 24,
                      height: 28 / 24,
                      color: colors.text.accent,
                    ),
                  ),
                  const SizedBox(height: AppSpacing.xs),
                  Text(
                    'Keep Zafe open. Your share is being created together with the other members; '
                    'the full key never exists anywhere.',
                    style: AppTypography.bodyMedium.copyWith(
                      color: colors.text.secondary,
                    ),
                  ),
                  const SizedBox(height: AppSpacing.lg),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}

/// What the setup screen says after key generation timed out waiting for `missing`.
String stalledText(List<String> missing, {String? me}) {
  final who = missing.where((k) => k != me).map(shortKey).toList();
  final names = switch (who.length) {
    0 => 'some members',
    1 => who.single,
    _ => '${who.sublist(0, who.length - 1).join(', ')} and ${who.last}',
  };
  return 'Key generation stopped: $names never took part. Ask them to open Zafe and keep '
      'this screen open, then try again together. Or abandon this setup and start over.';
}
