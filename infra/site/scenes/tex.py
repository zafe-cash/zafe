# The scene's small printed and drawn details, as textures: the payment slip (its address is
# what the magnifier enlarges), the three signers' portraits, the clock face, note scribbles
# and the embossed wax seal. Drawn with ImageMagick in the app's fonts and the site palette,
# so they're authored and reproducible.
#   python3 tex.py <out dir>
import os
import subprocess
import sys

OUT = sys.argv[1] if len(sys.argv) > 1 else "tex"
FONTS = os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../../app/assets/fonts")
MONO = os.path.join(FONTS, "JetBrainsMono-Medium.ttf")
SANS = os.path.join(FONTS, "DMSans-Medium.ttf")
HEAD = os.path.join(FONTS, "SpaceGrotesk-SemiBold.ttf")
INK, MUTED, BRAND, GOLD = "#1E2B2A", "#5B6765", "#00736C", "#8A5E00"
ADDRESS = ("u1k3v8m", "f6d8s0a")  # the card's recipient, shortened the same way


def im(*args):
    subprocess.run(["convert", *args], check=True)


def slip(path):
    """The top payment slip: what Maya proposed, printed. Transparent, ink only."""
    im("-size", "1024x700", "xc:none",
       "-font", SANS, "-pointsize", "30", "-fill", MUTED, "-annotate", "+70+92", "PAYMENT REQUEST",
       "-font", HEAD, "-pointsize", "120", "-fill", INK, "-annotate", "+64+238", "12.50",
       "-font", HEAD, "-pointsize", "54", "-fill", GOLD, "-annotate", "+400+238", "ZEC",
       "-stroke", "#B9BFB8", "-strokewidth", "3", "-draw", "line 70,290 954,290", "-stroke", "none",
       "-font", SANS, "-pointsize", "30", "-fill", MUTED, "-annotate", "+70+356", "To",
       "-font", MONO, "-pointsize", "44", "-fill", INK, "-annotate", "+180+358", f"{ADDRESS[0]} .... {ADDRESS[1]}",
       "-font", SANS, "-pointsize", "30", "-fill", MUTED, "-annotate", "+70+446", "Memo",
       "-font", SANS, "-pointsize", "38", "-fill", INK, "-annotate", "+180+448", "Audit milestone 2",
       "-stroke", "#B9BFB8", "-strokewidth", "3", "-draw", "line 70,500 954,500", "-stroke", "none",
       "-fill", BRAND, "-draw", "roundrectangle 70,560 330,620 30,30",
       "-font", SANS, "-pointsize", "30", "-fill", "#FFFFFF", "-annotate", "+100+601", "2 of 3 to send",
       path)


def lens(path):
    """What the magnifier shows: the slip's address line, 1.8x, centred on a square."""
    im("-size", "512x512", "xc:none",
       "-stroke", "#B9BFB8", "-strokewidth", "5", "-draw", "line -40,140 560,140", "-stroke", "none",
       "-font", SANS, "-pointsize", "54", "-fill", MUTED, "-annotate", "+0+262", "To",
       "-font", MONO, "-pointsize", "80", "-fill", INK, "-annotate", "+96+266", ADDRESS[0],
       "-font", MONO, "-pointsize", "80", "-fill", INK, "-annotate", "+96+372", f"... {ADDRESS[1][:3]}",
       path)


def portrait(path, skin, hair, top, kind):
    """One signer: shoulders, head and hair as flat shapes on a paper card."""
    back = {"bob": "path 'M 66,236 C 52,110 78,82 128,82 C 178,82 204,110 190,236 Z'"}.get(kind)
    fringe = {
        "bob": "path 'M 76,150 C 70,96 186,90 182,150 C 150,128 112,118 76,150 Z'",
        "short": "path 'M 78,150 C 70,82 190,72 182,148 C 160,122 100,120 78,150 Z'",
        "beard": "path 'M 80,140 C 76,84 186,78 180,140 C 160,118 100,118 80,140 Z'",
    }[kind]
    args = ["-size", "256x320", "xc:#FBF6EA"]
    if back:
        args += ["-fill", hair, "-draw", back]
    args += ["-fill", top, "-draw", "path 'M 30,330 C 34,250 70,232 128,232 C 186,232 222,250 226,330 Z'",
             "-fill", skin, "-draw", "roundrectangle 112,190 144,240 10,10",
             "-fill", skin, "-draw", "ellipse 128,160 50,60 0,360",
             "-fill", hair, "-draw", fringe]
    if kind == "beard":
        args += ["-fill", hair, "-draw", "path 'M 82,170 C 90,236 166,236 174,170 C 160,196 96,196 82,170 Z'"]
    im(*args, path)


def clock(path):
    """A plain face with twelve ticks (the hands are geometry)."""
    draw = []
    import math
    for k in range(12):
        a = math.radians(k * 30)
        r0, r1 = (176, 220) if k % 3 == 0 else (196, 220)
        x0, y0 = 256 + r0 * math.sin(a), 256 - r0 * math.cos(a)
        x1, y1 = 256 + r1 * math.sin(a), 256 - r1 * math.cos(a)
        draw += ["-strokewidth", "10" if k % 3 == 0 else "6", "-draw", f"line {x0:.1f},{y0:.1f} {x1:.1f},{y1:.1f}"]
    im("-size", "512x512", "xc:none", "-stroke", INK, *draw, path)


def scribble(path, seed):
    """Two or three handwritten-looking lines on a sticky note."""
    import random
    rnd = random.Random(seed)
    paths = []
    for row in range(3 if seed % 2 else 2):
        y = 80 + row * 70
        pts = [f"M 40,{y}"]
        x = 40
        while x < 200 - row * 30:
            x += rnd.randint(14, 26)
            pts.append(f"Q {x - 8},{y - rnd.randint(8, 22)} {x},{y + rnd.randint(-4, 4)}")
        paths += ["-draw", f"path '{' '.join(pts)}'"]
    im("-size", "256x256", "xc:none", "-fill", "none", "-stroke", "#5A4A20", "-strokewidth", "6",
       "-draw", "stroke-linecap round", *paths, path)


def gauge(path):
    """The pneumatic tube's pressure dial: ticks on a 240° arc, the top third in red."""
    import math
    draw = []
    for k in range(13):
        a = math.radians(-120 + k * 20)
        r0 = 170 if k % 2 == 0 else 188
        x0, y0 = 256 + r0 * math.sin(a), 256 - r0 * math.cos(a)
        x1, y1 = 256 + 214 * math.sin(a), 256 - 214 * math.cos(a)
        draw += ["-stroke", "#B8452E" if k >= 9 else INK, "-strokewidth", "9" if k % 2 == 0 else "5",
                 "-draw", f"line {x0:.1f},{y0:.1f} {x1:.1f},{y1:.1f}"]
    im("-size", "512x512", "xc:none", *draw,
       "-stroke", "none", "-fill", INK, "-font", SANS, "-pointsize", "40", "-annotate", "+214+380", "kPa", path)


def words(path):
    """A handwritten-style card with the four safety words: everyone's card is the same."""
    im("-size", "512x340", "xc:none",
       "-font", SANS, "-pointsize", "28", "-fill", MUTED, "-annotate", "+44+66", "SAFETY WORDS",
       "-font", HEAD, "-pointsize", "56", "-fill", INK, "-annotate", "+44+156", "orbit  velvet",
       "-font", HEAD, "-pointsize", "56", "-fill", INK, "-annotate", "+44+236", "canyon  harbor",
       "-stroke", BRAND, "-strokewidth", "8", "-fill", "none",
       "-draw", "path 'M 400,290 L 424,314 L 470,262'", path)


def plate(path):
    """The strongbox's lock plate: three keyholes, one per signer."""
    holes = []
    for x in (128, 256, 384):
        holes += ["-draw", f"circle {x},120 {x},142",
                  "-draw", f"polygon {x - 12},128 {x + 12},128 {x + 18},196 {x - 18},196"]
    im("-size", "512x256", "xc:none", "-fill", "#2A1D10", *holes, path)


def seal(path):
    """The pressed wax: a raised ring and a Z, lighter and darker than the wax."""
    im("-size", "512x512", "xc:none",
       "-fill", "none", "-stroke", "#3FA196", "-strokewidth", "20", "-draw", "circle 256,256 256,82",
       "-stroke", "#0B4C48", "-strokewidth", "8", "-draw", "circle 256,256 256,112",
       "-stroke", "none", "-fill", "#3FA196",
       "-draw", "polygon 170,170 342,170 342,206 226,318 342,318 342,352 170,352 170,316 286,206 170,206",
       path)


if __name__ == "__main__":
    os.makedirs(OUT, exist_ok=True)
    slip(os.path.join(OUT, "slip.png"))
    lens(os.path.join(OUT, "lens.png"))
    portrait(os.path.join(OUT, "maya.png"), "#E7B48E", "#3A2418", "#D0704F", "bob")
    portrait(os.path.join(OUT, "you.png"), "#C68A62", "#2A1D16", "#14716A", "short")
    portrait(os.path.join(OUT, "ravi.png"), "#9C6544", "#1F1611", "#DDA437", "beard")
    clock(os.path.join(OUT, "clock.png"))
    scribble(os.path.join(OUT, "note0.png"), 1)
    scribble(os.path.join(OUT, "note1.png"), 2)
    seal(os.path.join(OUT, "seal.png"))
    gauge(os.path.join(OUT, "gauge.png"))
    words(os.path.join(OUT, "words.png"))
    plate(os.path.join(OUT, "plate.png"))
    print("textures in", OUT)
