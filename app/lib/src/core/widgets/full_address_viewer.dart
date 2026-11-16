import 'package:flutter/widgets.dart';

import '../theme/app_theme.dart';
import 'app_button.dart';
import 'copy_button.dart';

String fullAddressCopyText(String address) => address.trim();

class FullAddressText extends StatelessWidget {
  const FullAddressText({required this.address, this.color, super.key});

  final String address;
  final Color? color;

  @override
  Widget build(BuildContext context) {
    return Text(
      fullAddressCopyText(address),
      key: const ValueKey('full_address_text'),
      softWrap: true,
      style: AppTypography.codeMedium.copyWith(
        color: color ?? context.colors.text.primary,
      ),
    );
  }
}

class FullAddressCopyButton extends StatelessWidget {
  const FullAddressCopyButton({
    required this.address,
    this.expand = false,
    this.size = AppButtonSize.mediumLarge,
    this.label = 'Copy address',
    super.key,
  });

  final String address;
  final bool expand;
  final AppButtonSize size;
  final String label;

  @override
  Widget build(BuildContext context) => CopyButton(
    key: const ValueKey('full_address_copy_button'),
    label: label,
    text: fullAddressCopyText(address),
    variant: AppButtonVariant.primary,
    size: size,
    expand: expand,
  );
}

const kFullAddressCopyActionMinWidth = 96.0;
