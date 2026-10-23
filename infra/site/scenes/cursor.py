# The site's pointer (src/styles/site.css): a tilted dart with a white edge, drawn here and
# shown by the OS (CSS `cursor: image-set(...)`), so it never lags behind a busy page.
# Ink by default, teal over anything clickable. Writes public/assets/ui/cursor{,-link}{,@2x}.png.
#   python scenes/cursor.py
from pathlib import Path
from PIL import Image, ImageDraw, ImageFilter

SIZE = 24  # CSS px; the hotspot is the tip (3, 2), as in site.css
DART = [(3, 2), (19.5, 9.6), (11.7, 11.7), (9.6, 19.5)]
EDGE = 1.6  # white edge, CSS px
OUT = Path(__file__).resolve().parent.parent / 'public/assets/ui'


def draw(fill, scale):
    k = scale * 8  # supersample, then downsample
    pts = [(x * k, y * k) for x, y in DART]
    shape = Image.new('RGBA', (SIZE * k, SIZE * k), (0, 0, 0, 0))
    d = ImageDraw.Draw(shape)
    r = EDGE * k
    d.polygon(pts, fill='white')
    for a, b in zip(pts, pts[1:] + pts[:1]):  # stroke with round joins
        d.line([a, b], fill='white', width=round(2 * r))
        d.ellipse([a[0] - r, a[1] - r, a[0] + r, a[1] + r], fill='white')
    d.polygon(pts, fill=fill)
    # A soft shadow below, so it reads on white paper as well as on the dark vault.
    alpha = shape.getchannel('A')
    shadow = Image.new('RGBA', shape.size, (14, 19, 19, 0))
    shadow.putalpha(alpha.point(lambda v: v * 0.28))
    shadow = shadow.transform(shape.size, Image.AFFINE, (1, 0, 0, 0, 1, -1.2 * k)).filter(ImageFilter.GaussianBlur(1.4 * k))
    return Image.alpha_composite(shadow, shape).resize((SIZE * scale, SIZE * scale), Image.LANCZOS)


for name, fill in (('cursor', '#0E1313'), ('cursor-link', '#00736C')):
    for scale, suffix in ((1, ''), (2, '@2x')):
        draw(fill, scale).save(OUT / f'{name}{suffix}.png', optimize=True)
        print(OUT / f'{name}{suffix}.png')
