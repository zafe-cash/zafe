import os
import shutil

from fontTools import subset
from PIL import Image

ROOT = os.path.normpath(os.path.join(os.path.dirname(__file__), "..", ".."))
BUILD = os.path.join(ROOT, "app", "build")
APP_FONTS = os.path.join(ROOT, "app", "assets", "fonts")
OUT = os.path.join(ROOT, "infra", "site", "public", "assets")

PHONE = (0, 0, 780, 1688)

CROPS = [
    ("story_bob_home", "screen_preview/home", PHONE, "dark"),
    ("story_bob_propose", "screen_preview/story_bob_propose", PHONE, "dark"),
    ("story_bob_approved", "screen_preview/story_bob_approved", PHONE, "dark"),
    ("story_me_home", "screen_preview/home", PHONE, "light"),
    ("story_me_request", "screen_preview/story_me_request", PHONE, "light"),
    ("story_me_sent", "screen_preview/story_me_sent", PHONE, "light"),
    ("story_me_sending", "screen_preview/sending_progress", PHONE, "light"),
    ("story_me_done", "screen_preview/sending_sent", PHONE, "light"),
    ("art_vault", "illustration_preview/welcome_vault", (0, 120, 1080, 1240), "dark"),
    ("docs_server", "screen_preview/move_server", (0, 0, 780, 640)),
    ("docs_server_list", "screen_preview/relay_picker_other_token", (0, 0, 780, 860)),
    ("docs_move_waiting", "screen_preview/move_waiting", (0, 0, 780, 920)),
    ("docs_move_card", "screen_preview/move_needs_you", (0, 148, 780, 438)),
]

FONTS = [
    "DMSans-Regular",
    "DMSans-Medium",
    "SpaceGrotesk-Medium",
    "SpaceGrotesk-SemiBold",
    "JetBrainsMono-Regular",
]
UNICODES = "U+0000-00FF,U+2013-2014,U+2018-201D,U+2022,U+2026,U+2190-2193,U+2212"


def screens():
    out = os.path.join(OUT, "screens")
    if os.path.isdir(out):
        shutil.rmtree(out)
    os.makedirs(out)
    for name, source, box, *theme in CROPS:
        for theme in theme or ("light",):
            src = os.path.join(BUILD, f"{source}_{theme}.png")
            if not os.path.exists(src):
                raise SystemExit(f"missing {src}: run the render tests first (see the docstring)")
            image = Image.open(src).convert("RGB").crop(box)
            if name.startswith("art_") and image.width * 4 > image.height * 5:
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
