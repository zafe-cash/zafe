import 'package:flutter/widgets.dart';
import 'package:flutter_svg/flutter_svg.dart';

import '../theme/app_theme.dart';
import 'app_loading_icon.dart';

abstract final class AppIcons {
  static const addNew = 'add_new';
  static const arrowDown = 'arrow_down';
  static const arrowDownCircle = 'arrow_down_circle';
  static const arrowUpRight = 'arrow_up_right';
  static const more = 'more';
  static const bell = 'bell';
  static const book = 'book';
  static const cameraDenied = 'camera_denied';
  static const cancel = 'cancel';
  static const check = 'check';
  static const checkCircle = 'check_circle';
  static const chevronBackward = 'chevron_backward';
  static const chevronForward = 'chevron_forward';
  static const cog = 'cog';
  static const copy = 'copy';
  static const cross = 'cross';
  static const day = 'day';
  static const edit = 'edit';
  static const editFilled = 'edit_filled';
  static const endpoint = 'endpoint';
  static const eye = 'eye';
  static const eyeClosed = 'eye_closed';
  static const globe = 'globe';
  static const help = 'help';
  static const history = 'history';
  static const home = 'home';
  static const importWallet = 'import_wallet';
  static const key = 'key';
  static const link = 'link';
  static const loader = 'loader';
  static const lock = 'lock';
  static const monitor = 'monitor';
  static const night = 'night';
  static const plane = 'plane';
  static const plus = 'plus';
  static const qr = 'qr';
  static const renew = 'renew';
  static const server = 'server';
  static const share = 'share';
  static const shieldKeyhole = 'shield_keyhole';
  static const sound = 'sound';
  static const soundOff = 'sound_off';
  static const theme = 'theme';
  static const time = 'time';
  static const tor = 'tor';
  static const trash = 'trash';
  static const unlock = 'unlock';
  static const user = 'user';
  static const users = 'users';
  static const wallet = 'wallet';
  static const warning = 'warning';
  static const warningCircle = 'warning_circle';
  static const zcashCurrency = 'zcash_currency';

  static const marks = {zcashCurrency};
}

class AppIcon extends StatelessWidget {
  const AppIcon(
    this.name, {
    this.size = AppIconSize.medium,
    this.color,
    this.animated = true,
    this.semanticLabel,
    this.active = false,
    this.patina,
    super.key,
  });

  final bool active;

  final Color? patina;

  final String name;

  final double size;

  final Color? color;

  final bool animated;

  final String? semanticLabel;

  @override
  Widget build(BuildContext context) {
    final resolved =
        color ?? IconTheme.of(context).color ?? context.colors.icon.regular;
    if (name == AppIcons.loader) {
      return AppLoadingIcon(
        size: size,
        color: resolved,
        animated: animated,
        semanticLabel: semanticLabel,
      );
    }
    if (!AppIcons.marks.contains(name)) {
      if (active) {
        return SvgPicture.asset(
          'assets/icons/patina/${name}_active.svg',
          width: size,
          height: size,
          colorFilter: ColorFilter.mode(resolved, BlendMode.srcIn),
          semanticsLabel: semanticLabel,
        );
      }
      final fill = patina ?? _defaultPatina(context.colors, resolved);
      return SvgPicture.asset(
        'assets/icons/patina/$name.svg',
        width: size,
        height: size,
        colorMapper: PatinaColorMapper(
          outline: resolved,
          fill: fill.withValues(alpha: fill.a * kPatinaFillOpacity),
        ),
        semanticsLabel: semanticLabel,
      );
    }
    return SvgPicture.asset(
      'assets/icons/$name.svg',
      width: size,
      height: size,
      colorFilter: ColorFilter.mode(resolved, BlendMode.srcIn),
      semanticsLabel: semanticLabel,
    );
  }
}

Color _defaultPatina(AppColors c, Color resolved) {
  final semantic = {
    c.icon.warning,
    c.icon.destructive,
    c.icon.destructiveLight,
    c.icon.value,
    c.text.warning,
    c.text.destructive,
    c.text.destructiveLight,
    c.text.value,
  };
  return semantic.contains(resolved) ? resolved : c.icon.brand;
}

const kPatinaFillOpacity = 0.38;

@immutable
class PatinaColorMapper extends ColorMapper {
  const PatinaColorMapper({required this.outline, required this.fill});
  final Color outline;
  final Color fill;

  static const _outlineKey = Color(0xFF000000);
  static const _fillKey = Color(0xFFFF00FF);

  @override
  Color substitute(
    String? id,
    String elementName,
    String attributeName,
    Color color,
  ) {
    if (color == _outlineKey) return outline;
    if (color == _fillKey) return fill;
    return color;
  }

  @override
  bool operator ==(Object other) =>
      other is PatinaColorMapper &&
      other.outline == outline &&
      other.fill == fill;

  @override
  int get hashCode => Object.hash(outline, fill);
}
