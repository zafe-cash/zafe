"""Regenerates the site's images and web fonts from the app (run from the repo root).

    cd app && HOME_NOTICE=0 HOME_FIAT=0 flutter test --dart-define=ZAFE_NETWORK=main tool/screens/ && flutter test tool/illustrations/preview_test.dart
    python3 infra/site/assets.py          # needs: pip install pillow fonttools brotli

Images: the app's own renders (fake data, app/build/screen_preview/) and illustrations
(app/build/illustration_preview/), cropped to the part the page talks about and saved as
WebP (light theme; the site has no dark mode). Fonts: the app's DM Sans, Space Grotesk and JetBrains Mono, subset
to Latin and saved as WOFF2, with their OFL licences. The outputs are committed, so
building or deploying the site doesn't need Flutter or Python packages.
"""

import os
import shutil

from fontTools import subset
from PIL import Image

ROOT = os.path.normpath(os.path.join(os.path.dirname(__file__), "..", ".."))
BUILD = os.path.join(ROOT, "app", "build")
APP_FONTS = os.path.join(ROOT, "app", "assets", "fonts")
OUT = os.path.join(ROOT, "infra", "site", "public", "assets")

# (output name, source under app/build without the _light/_dark suffix, crop box). Screen
# renders are 780 px wide (390 logical at 2x); boxes are in those pixels.
PHONE = (0, 0, 780, 1688)  # a phone viewport: 390 x 844 at 2x

CROPS = [
    # The scroll story: Bob's phone in dark mode, yours in light (proposal_render_test's
    # story_* scenarios and home_render_test with HOME_NOTICE=0). Proposing is Bob's
    # approval, so he goes from the review straight to his proposal, waiting; you get the
    # request with Approve pinned at the bottom (SPOT.approve in story-screens.js).
    ("story_bob_home", "screen_preview/home", PHONE, "dark"),
    ("story_bob_propose", "screen_preview/story_bob_propose", PHONE, "dark"),
    ("story_bob_approved", "screen_preview/story_bob_approved", PHONE, "dark"),
    ("story_me_home", "screen_preview/home", PHONE, "light"),
    ("story_me_request", "screen_preview/story_me_request", PHONE, "light"),
    ("story_me_sent", "screen_preview/story_me_sent", PHONE, "light"),
    # The sending screen (sending_render_test): the story's finale.
    ("story_me_sending", "screen_preview/sending_progress", PHONE, "light"),
    ("story_me_done", "screen_preview/sending_sent", PHONE, "light"),
    # The vault door with its keys, behind the closing call to action (a dark card, so
    # the dark render).
    ("art_vault", "illustration_preview/welcome_vault", (0, 120, 1080, 1240), "dark"),
]

FONTS = [
    "DMSans-Regular",
    "DMSans-Medium",
    "SpaceGrotesk-Medium",
    "SpaceGrotesk-SemiBold",
    "JetBrainsMono-Regular",
]
# Basic Latin, Latin-1, and the punctuation the copy uses (dashes, quotes, bullet,
# ellipsis, arrows, minus).
UNICODES = "U+0000-00FF,U+2013-2014,U+2018-201D,U+2022,U+2026,U+2190-2193,U+2212"


def screens():
    out = os.path.join(OUT, "screens")
    if os.path.isdir(out):
        shutil.rmtree(out)  # drop crops that are no longer listed
    os.makedirs(out)
    for name, source, box, *theme in CROPS:
        for theme in theme or ("light",):  # the site is light only
            src = os.path.join(BUILD, f"{source}_{theme}.png")
            if not os.path.exists(src):
                raise SystemExit(f"missing {src}: run the render tests first (see the docstring)")
            image = Image.open(src).convert("RGB").crop(box)
            if name.startswith("art_") and image.width * 4 > image.height * 5:
                # Pillar art shares a 5:4 tile: extend the sky rather than crop the scene.
                padded = Image.new("RGB", (image.width, image.width * 4 // 5), image.getpixel((4, 4)))
                padded.paste(image, (0, padded.height - image.height))
                image = padded
            dest = os.path.join(out, f"{name}_{theme}.webp")
            image.save(dest, "WEBP", quality=86, method=6)
            print(f"{dest}  {image.size[0]}x{image.size[1]}  {os.path.getsize(dest) // 1024} KB")


def fonts():
    out = os.path.join(OUT, "fonts")
    os.makedirs(out, exist_ok=True)
    for name in FONTS:
        dest = os.path.join(out, f"{name}.woff2")
        subset.main([
            os.path.join(APP_FONTS, f"{name}.ttf"),
            f"--unicodes={UNICODES}",
            "--layout-features=kern,liga,calt,tnum",
            "--flavor=woff2",
            f"--output-file={dest}",
        ])
        print(f"{dest}  {os.path.getsize(dest) // 1024} KB")
    for licence in ("DMSans-OFL.txt", "SpaceGrotesk-OFL.txt", "JetBrainsMono-OFL.txt"):
        shutil.copy(os.path.join(APP_FONTS, "licenses", licence), out)


if __name__ == "__main__":
    screens()
    fonts()
