import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/formatting/zec_amount.dart';
import '../../core/layout/mobile/zafe_screen.dart';
import '../../core/theme/app_theme.dart';
import '../../core/widgets/app_icon.dart';
import '../../core/widgets/app_loading_icon.dart';
import '../../core/widgets/mobile/mobile_surface_card.dart';
import '../../core/widgets/mobile/zafe_detail.dart';
import '../../providers/privacy_mode_provider.dart';
import '../../providers/received_provider.dart';
import '../../rust/api/received.dart' as rust;
import '../proposals/proposal_status.dart' show formatTimestamp;
import 'received_row.dart';

/// Money the vault received: amount, status, when, memo and transaction id. Shielded
/// payments don't reveal their sender, so there is no "from".
class ReceivedScreen extends ConsumerWidget {
  const ReceivedScreen({super.key, required this.txid});
  final String txid;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.colors;
    final received = ref.watch(receivedProvider);
    final r = received.byTxid(txid);
    if (r == null) {
      return ZafeScreen(
        title: 'Received',
        children: [
          const SizedBox(height: 120),
          Center(
            child: received.loaded
                ? Text(
                    'Payment not found',
                    style: AppTypography.bodyMedium.copyWith(
                      color: colors.text.secondary,
                    ),
                  )
                : AppLoadingIcon(size: 24, color: colors.icon.muted),
          ),
        ],
      );
    }
    final hide = ref.watch(privacyModeProvider);
    return ZafeScreen(
      title: 'Received',
      children: [
        PaymentCard(
          label: r.isCoinbase ? 'MINING REWARD' : 'RECEIVED',
          amountText:
              '+${ZecAmount.fromZatoshi(r.amountZat).receipt.amountText}',
          hidden: hide,
        ),
        const SizedBox(height: AppSpacing.md),
        MobileSurfaceCard(
          cornerRadius: AppRadii.large,
          padding: const EdgeInsets.symmetric(
            horizontal: AppSpacing.sm,
            vertical: AppSpacing.lg,
          ),
          child: Column(
            children: [
              DetailRow(
                label: 'Status',
                value: '',
                trailing: _StatusChip(received: r),
              ),
              if (r.blockTimeSecs > 0) ...[
                const DetailDivider(),
                DetailRow(
                  label: 'Received',
                  value: formatTimestamp(BigInt.from(r.blockTimeSecs)),
                ),
              ],
              if (r.minedHeight > 0) ...[
                const DetailDivider(),
                DetailRow(label: 'Block', value: '${r.minedHeight}'),
                const DetailDivider(),
                DetailRow(label: 'Confirmations', value: '${r.confirmations}'),
              ],
              const DetailDivider(),
              TxIdRow(txid: r.txid),
            ],
          ),
        ),
        if (r.memo.isNotEmpty) ...[
          const SizedBox(height: AppSpacing.md),
          MobileSurfaceCard(
            cornerRadius: AppRadii.large,
            padding: const EdgeInsets.all(AppSpacing.sm),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  'Message',
                  style: AppTypography.labelLarge.copyWith(
                    color: colors.text.secondary,
                  ),
                ),
                const SizedBox(height: AppSpacing.xs),
                SelectableText(
                  r.memo,
                  style: AppTypography.bodyMedium.copyWith(
                    color: colors.text.accent,
                  ),
                ),
              ],
            ),
          ),
        ],
      ],
    );
  }
}

class _StatusChip extends StatelessWidget {
  const _StatusChip({required this.received});
  final rust.ReceivedInfo received;

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final confirmed = receivedStatus(received) == 'Confirmed';
    final color = confirmed ? colors.text.value : colors.text.secondary;
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        AppIcon(
          confirmed ? AppIcons.checkCircle : AppIcons.time,
          size: 16,
          color: color,
        ),
        const SizedBox(width: AppSpacing.xxs),
        Text(
          receivedStatus(received),
          style: AppTypography.labelLarge.copyWith(color: color),
        ),
      ],
    );
  }
}
