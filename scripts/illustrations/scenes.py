#!/usr/bin/env python3
"""Builds Zafe's illustrations into app/assets/illustrations as <name>_light.svg / <name>_dark.svg.

    python3 scripts/illustrations/scenes.py               # all scenes
    python3 scripts/illustrations/scenes.py key_shards    # one scene

Add a scene: write `def my_scene(p): ...` (copy scene_template.py) and register it in SCENES.
"""
import argparse
import math
import random
from pathlib import Path

from zafe_art import PALETTES, coin, defs, dial, f, key, line_d, pts, sparks, stone_wall, svg

ASSETS = Path(__file__).resolve().parents[2] / "app" / "assets" / "illustrations"


# ---------------------------------------------------------------- (a) welcome vault
# The Welcome door's hand wheel centre; the app turns the wheel layer around it
# (lib/src/features/onboarding/welcome_art.dart).
WELCOME_WHEEL = (540, 560)


def welcome(p, layer=None):
    """The vault door: three of five keys turned, so it opens. `layer` writes one part for
    the app's unlock animation (all on the scene's canvas, each part in its final place):
    "bg" (wall, floor, frame), "boltsL"/"boltsR", "door" (face and dial), "glow<k>" (behind a
    turned key), "locks" (escutcheons, keyholes), "key<k>" (a turned key), "wheel",
    "glint" (the wheel's light, which stays put), "top" (floor coins, vignette). The gold
    arc joining the keys is drawn by the app (it grows), so no layer has it.
    None: the whole still scene."""
    W, H = 1080, 1240
    rnd = random.Random(7)
    cx, cy = 540, 560
    parts = []  # (layer, svg) in drawing order

    class _B(list):
        tag = "bg"

        def append(self, x):
            parts.append((self.tag, x))
            super().append(x)

    b = _B()
    b.append(f'<rect width="{W}" height="{H}" fill="{p["sky1"]}"/>')
    b.append(stone_wall(p, rnd, 0, 0, W, 930, hole=(cx, cy, 395)))
    b.append(f'<rect width="{W}" height="{H}" fill="url(#grit)" opacity=".55"/>')
    # floor
    b.append(f'<rect y="930" width="{W}" height="{H-930}" fill="{p["stone3"]}"/>')
    for i, y in enumerate([930, 975, 1035, 1110, 1200]):
        b.append(f'<path d="M0 {y}H{W}" stroke="{p["ink"]}" stroke-width="3"/>')
    for k in range(-8, 9):
        b.append(f'<path d="M{f(cx + k*40)} 930L{f(cx + k*190)} {H}" stroke="{p["ink"]}" stroke-width="2.5"/>')
    b.append(f'<rect y="930" width="{W}" height="{H-930}" fill="url(#dots)" opacity="{p["texop"]}"/>')
    # recess shadow around the door
    b.append(f'<circle cx="{cx}" cy="{cy}" r="410" fill="{p["ink"]}" opacity=".35"/>')
    b.append(f'<circle cx="{cx}" cy="{cy}" r="392" fill="{p["stone3"]}" stroke="{p["ink"]}" stroke-width="4"/>')
    b.append(f'<circle cx="{cx}" cy="{cy}" r="392" fill="url(#hatch2)" opacity=".45"/>')
    # frame ring
    b.append(f'<circle cx="{cx}" cy="{cy}" r="368" fill="{p["metal"]}" stroke="{p["ink"]}" stroke-width="5"/>')
    b.append(f'<circle cx="{cx}" cy="{cy}" r="336" fill="none" stroke="{p["ink"]}" stroke-width="4"/>')
    for a in range(0, 360, 12):
        r = math.radians(a)
        b.append(f'<circle cx="{f(cx+352*math.cos(r))}" cy="{f(cy+352*math.sin(r))}" r="6" fill="{p["metal2"]}" stroke="{p["ink"]}" stroke-width="2.5"/>')
    # locking bolts at left/right into the frame
    for side in (-1, 1):
        b.tag = "boltsL" if side < 0 else "boltsR"
        for dy in (-120, 0, 120):
            x = cx + side * 318
            b.append(f'<rect x="{f(x - 40 if side > 0 else x - 20)}" y="{cy+dy-14}" width="60" height="28" rx="6" fill="{p["metal2"]}" stroke="{p["ink"]}" stroke-width="3.5"/>')
    # door face
    b.tag = "door"
    b.append(f'<circle cx="{cx}" cy="{cy}" r="318" fill="{p["stone2"]}" stroke="{p["ink"]}" stroke-width="5"/>')
    b.append(f'<circle cx="{cx}" cy="{cy}" r="318" fill="url(#grit)" opacity=".4"/>')
    # lower-right shading on the door (crescent of hatch)
    b.append(f'<path d="M{cx+318} {cy}A318 318 0 0 1 {cx} {cy+318}A318 318 0 0 1 {cx-225} {cy+225}A300 300 0 0 0 {cx+318} {cy}Z" bb="{cx-230} {cy} {cx+318} {cy+318}" fill="url(#hatch)" opacity=".55"/>')
    b.append(f'<circle cx="{cx}" cy="{cy}" r="290" fill="none" stroke="{p["ink"]}" stroke-width="3"/>')
    b.append(dial(p, cx, cy, 316, ticks=90, major=6, color=p["ink"], op=".55", rings=()))  # the dial motif
    b.append(f'<circle cx="{cx}" cy="{cy}" r="178" fill="none" stroke="{p["ink"]}" stroke-width="3"/>')
    b.append(f'<path d="M{cx-270} {cy-40}A275 275 0 0 1 {cx-40} {cy-272}" fill="none" stroke="{p["hi"]}" stroke-width="4" stroke-linecap="round" opacity=".7"/>')
    # threshold arc joining the three turned keys (members 0,1,4 -> top, upper-right, upper-left)
    ring = 225
    ang = [-90 + 72 * k for k in range(5)]
    turned = [0, 1, 4]
    a0, a1 = math.radians(ang[4]), math.radians(ang[1])
    b.tag = "arc"
    b.append(f'<path d="M{f(cx+ring*math.cos(a0))} {f(cy+ring*math.sin(a0))}A{ring} {ring} 0 0 1 {f(cx+ring*math.cos(a1))} {f(cy+ring*math.sin(a1))}" fill="none" stroke="{p["gold"]}" stroke-width="10" stroke-linecap="round" opacity=".9"/>')
    b.tag = "door"
    b.append(f'<circle cx="{cx}" cy="{cy}" r="{ring}" fill="none" stroke="{p["ink"]}" stroke-width="2.5" stroke-dasharray="4 10" opacity=".8"/>')
    for k, a in enumerate(ang):
        r = math.radians(a)
        kx, ky = cx + ring * math.cos(r), cy + ring * math.sin(r)
        on = k in turned
        if on:
            b.tag = f"glow{k}"
            b.append(f'<circle cx="{f(kx)}" cy="{f(ky)}" r="110" fill="url(#glow)"/>')
        # escutcheon
        b.tag = "locks"
        b.append(f'<circle cx="{f(kx)}" cy="{f(ky)}" r="38" fill="{p["metal2"] if not on else p["gold2"]}" stroke="{p["ink"]}" stroke-width="4"/>')
        b.append(f'<circle cx="{f(kx)}" cy="{f(ky)}" r="28" fill="none" stroke="{p["ink"]}" stroke-width="2"/>')
        if on:
            # key inserted: bow pointing outward, rotated a quarter turn (turned)
            t = a + 180  # turned a quarter: the bow lies along the ring
            bx, by = kx, ky - 96
            lobes = "".join(
                f'<circle cx="{f(bx + 24*math.cos(math.radians(q)))}" cy="{f(by + 24*math.sin(math.radians(q)))}" r="22" fill="url(#goldg)" stroke="{p["ink"]}" stroke-width="4"/>'
                for q in (-90, 30, 150))
            b.tag = f"key{k}"
            b.append(f'<g transform="rotate({f(t)} {f(kx)} {f(ky)})">'
                     f'<rect x="{f(kx-8)}" y="{f(ky-76)}" width="16" height="70" rx="3" fill="url(#goldg)" stroke="{p["ink"]}" stroke-width="4"/>'
                     f'<rect x="{f(kx-17)}" y="{f(ky-50)}" width="34" height="12" rx="4" fill="{p["gold2"]}" stroke="{p["ink"]}" stroke-width="3.5"/>'
                     + lobes +
                     f'<circle cx="{f(bx)}" cy="{f(by)}" r="22" fill="url(#goldg)"/>'
                     f'<circle cx="{f(bx)}" cy="{f(by)}" r="9" fill="{p["stone2"]}" stroke="{p["ink"]}" stroke-width="3.5"/>'
                     f'<path d="M{f(bx-34)} {f(by-6)}a36 36 0 0 1 18 -22" fill="none" stroke="{p["gold3"]}" stroke-width="4" stroke-linecap="round"/>'
                     f'<circle cx="{f(kx)}" cy="{f(ky)}" r="13" fill="{p["gold"]}" stroke="{p["ink"]}" stroke-width="3"/>'
                     "</g>")
        else:
            # empty keyhole
            b.tag = "locks"
            b.append(f'<path d="M{f(kx)} {f(ky-14)}a10 10 0 1 1 -.1 0zM{f(kx-6)} {f(ky-2)}h12l5 24h-22z" fill="{p["ink"]}"/>')
    # central hand wheel: rim, five spokes (one per member), brand grips
    assert (cx, cy) == WELCOME_WHEEL
    b.tag = "wheel"
    b.append(f'<circle cx="{cx}" cy="{cy}" r="122" fill="none" stroke="{p["ink"]}" stroke-width="30"/>')
    b.append(f'<circle cx="{cx}" cy="{cy}" r="122" fill="none" stroke="{p["metal2"]}" stroke-width="20"/>')
    glint = [f'<path d="M{cx-110} {cy-30}A114 114 0 0 1 {cx-30} {cy-110}" fill="none" stroke="{p["hi"]}" stroke-width="5" stroke-linecap="round"/>',
             f'<path d="M{cx+116} {cy+20}A118 118 0 0 1 {cx+20} {cy+116}" fill="none" stroke="{p["ink"]}" stroke-width="6" stroke-linecap="round" opacity=".6"/>']
    b.tag = "glint"
    for g in glint:
        b.append(g)
    b.tag = "wheel"
    for k, a in enumerate(ang):
        r = math.radians(a + 36)
        x2, y2 = cx + 110 * math.cos(r), cy + 110 * math.sin(r)
        b.append(f'<path d="M{cx} {cy}L{f(x2)} {f(y2)}" stroke="{p["ink"]}" stroke-width="24" stroke-linecap="round"/>')
        b.append(f'<path d="M{cx} {cy}L{f(x2)} {f(y2)}" stroke="{p["metal2"]}" stroke-width="14" stroke-linecap="round"/>')
        gx, gy = cx + 150 * math.cos(r), cy + 150 * math.sin(r)
        b.append(f'<path d="M{f(cx+124*math.cos(r))} {f(cy+124*math.sin(r))}L{f(gx)} {f(gy)}" stroke="{p["ink"]}" stroke-width="26" stroke-linecap="round"/>')
        b.append(f'<path d="M{f(cx+124*math.cos(r))} {f(cy+124*math.sin(r))}L{f(gx)} {f(gy)}" stroke="{p["brand"]}" stroke-width="17" stroke-linecap="round"/>')
    b.append(f'<circle cx="{cx}" cy="{cy}" r="50" fill="{p["metal"]}" stroke="{p["ink"]}" stroke-width="5"/>')
    b.append(f'<circle cx="{cx}" cy="{cy}" r="50" fill="url(#hatch2)" opacity=".35"/>')
    b.append(f'<circle cx="{cx}" cy="{cy}" r="22" fill="{p["brand2"]}" stroke="{p["ink"]}" stroke-width="4"/>')
    b.tag = "top"
    # coins on the floor
    for (x, y, rx, rot) in [(150, 1010, 34, -8), (205, 1040, 30, 6), (930, 1000, 36, 10),
                            (880, 1060, 28, -4), (990, 1075, 32, 0), (120, 1090, 26, 12)]:
        b.append(coin(p, x, y, rx, rx * 0.42, rot))
    b.append(f'<rect width="{W}" height="{H}" fill="url(#vig)"/>')
    if layer:
        return svg(W, H, defs(p) + "".join(x for t, x in parts if t == layer), p["dot"])
    return svg(W, H, defs(p) + "".join(b), p["dot"])


# ---------------------------------------------------------------- (b) create: stone circle
def create(p):
    W, H = 1080, 560
    rnd = random.Random(3)
    b = [f'<rect width="{W}" height="{H}" fill="url(#sky)"/>']
    b.append(f'<rect width="{W}" height="{H}" fill="url(#fine)" opacity="{float(p["texop"])*0.7:.2f}"/>')
    # rising coin-moon
    b.append(f'<circle cx="540" cy="250" r="230" fill="url(#glow)" opacity=".7"/>')
    b.append(f'<circle cx="540" cy="250" r="118" fill="url(#goldg)" stroke="{p["ink"]}" stroke-width="4"/>')
    b.append(f'<circle cx="540" cy="250" r="88" fill="none" stroke="{p["gold2"]}" stroke-width="3"/>')
    b.append(f'<path d="M462 238a80 80 0 0 1 50 -62" fill="none" stroke="{p["gold3"]}" stroke-width="5" stroke-linecap="round"/>')
    # distant hills
    hill = [(0, 360)] + [(x, 350 - 30 * math.sin(x / 150) - rnd.randint(0, 10)) for x in range(0, W + 60, 60)] + [(W, H), (0, H)]
    b.append(f'<polygon points="{pts(hill)}" fill="{p["stone3"]}" stroke="{p["ink"]}" stroke-width="3"/>')
    b.append(f'<polygon points="{pts(hill)}" fill="url(#dots)" opacity="{p["texop"]}"/>')
    # ground
    b.append(f'<path d="M0 430Q540 380 1080 430V560H0Z" fill="{p["stone"]}" stroke="{p["ink"]}" stroke-width="4"/>')
    b.append(f'<path d="M0 470Q540 430 1080 470V560H0Z" bb="0 440 1080 560" fill="url(#hatch2)" opacity=".35"/>')
    for _ in range(40):  # grass ticks
        x, y = rnd.randint(0, W), rnd.randint(440, 550)
        b.append(f'<path d="M{x} {y}l-4 -12M{x+5} {y}l2 -14" stroke="{p["ink"]}" stroke-width="2" opacity=".6"/>')

    def stone(x, base, w, h, lit, lean=0):
        t = base - h
        poly = [(x - w / 2 + lean, t + 10), (x - w / 2 + 8 + lean, t), (x + w / 2 - 6 + lean, t + 4),
                (x + w / 2 + 4, base), (x - w / 2 - 4, base)]
        s = [f'<polygon points="{pts(poly)}" fill="{p["stone2"]}" stroke="{p["ink"]}" stroke-width="4" stroke-linejoin="round"/>']
        sh = [(x + w * 0.1 + lean, t + 6), (x + w / 2 - 6 + lean, t + 4), (x + w / 2 + 4, base), (x + w * 0.15, base)]
        s.append(f'<polygon points="{pts(sh)}" fill="url(#hatch)" opacity=".6"/>')
        s.append(f'<path d="M{f(x-w/2+10+lean)} {f(t+18)}V{f(base-10)}" stroke="{p["hi"]}" stroke-width="3" opacity=".6"/>')
        if lit:
            ry = t + h * 0.4
            s.append(f'<circle cx="{f(x)}" cy="{f(ry)}" r="{f(w*0.9)}" fill="url(#glow)"/>')
            s.append(f'<path d="M{f(x)} {f(ry-18)}l12 18l-12 18l-12 -18z" fill="{p["gold"]}" stroke="{p["ink"]}" stroke-width="3" stroke-linejoin="round"/>')
        else:
            s.append(f'<path d="M{f(x)} {f(t+h*0.4-16)}l10 16l-10 16l-10 -16z" fill="none" stroke="{p["ink"]}" stroke-width="3" opacity=".7"/>')
        return "".join(s)

    # outer ring of unlit stones
    b.append(stone(250, 420, 56, 150, False, -4))
    b.append(stone(830, 420, 56, 150, False, 4))
    b.append(stone(120, 540, 76, 200, False, -6))
    b.append(stone(960, 540, 76, 190, False, 6))
    # the stones that complete the arch
    b.append(stone(410, 480, 92, 260, True))
    b.append(stone(670, 480, 92, 260, True))
    lint = [(344, 218), (736, 212), (742, 262), (338, 268)]
    b.append(f'<circle cx="540" cy="240" r="120" fill="url(#glow)"/>')
    b.append(f'<polygon points="{pts(lint)}" fill="{p["stone2"]}" stroke="{p["ink"]}" stroke-width="4" stroke-linejoin="round"/>')
    b.append(f'<polygon points="340,248 740,242 742,262 338,268" fill="url(#hatch)" opacity=".6"/>')
    b.append(f'<path d="M362 228H716" stroke="{p["hi"]}" stroke-width="3" opacity=".6"/>')
    b.append(f'<path d="M540 222l13 18l-13 18l-13 -18z" fill="{p["gold"]}" stroke="{p["ink"]}" stroke-width="3" stroke-linejoin="round"/>')
    # brand thread of light connecting the lit stones on the ground
    b.append(f'<path d="M410 486Q540 520 670 486" fill="none" stroke="{p["brand"]}" stroke-width="6" stroke-dasharray="2 12" stroke-linecap="round"/>')
    b.append(f'<rect width="{W}" height="{H}" fill="url(#vig)"/>')
    return svg(W, H, defs(p) + "".join(b), p["dot"])


# ---------------------------------------------------------------- (c) join: doorway + sealed invite
def join(p):
    W, H = 1080, 560
    rnd = random.Random(11)
    b = [f'<rect width="{W}" height="{H}" fill="{p["sky1"]}"/>']
    b.append('<g transform="translate(0 30)">')
    b.append(stone_wall(p, rnd, 0, -60, W, 440, row_h=54, blk=(100, 180)))
    b.append(f'<rect y="-60" width="{W}" height="500" fill="url(#grit)" opacity=".55"/>')
    cx = 540
    # arch opening
    arch = f"M{cx-150} 440V190A150 150 0 0 1 {cx+150} 190V440Z"
    # voussoirs
    for k in range(11):
        a0 = math.radians(180 + k * 180 / 11)
        a1 = math.radians(180 + (k + 1) * 180 / 11)
        r0, r1 = 150, 205
        q = [(cx + r0 * math.cos(a0), 190 + r0 * math.sin(a0)), (cx + r1 * math.cos(a0), 190 + r1 * math.sin(a0)),
             (cx + r1 * math.cos(a1), 190 + r1 * math.sin(a1)), (cx + r0 * math.cos(a1), 190 + r0 * math.sin(a1))]
        b.append(f'<polygon points="{pts(q)}" fill="{p["stone2"] if k != 5 else p["brand2"]}" stroke="{p["ink"]}" stroke-width="4" stroke-linejoin="round"/>')
    for side in (-1, 1):
        b.append(f'<rect x="{cx + side*177 - 27}" y="190" width="54" height="250" fill="{p["stone2"]}" stroke="{p["ink"]}" stroke-width="4"/>')
        for y in (250, 320, 390):
            b.append(f'<path d="M{cx + side*177 - 27} {y}h54" stroke="{p["ink"]}" stroke-width="3"/>')
    # warm light inside
    b.append(f'<path d="{arch}" fill="{p["gold3"]}" stroke="{p["ink"]}" stroke-width="5"/>')
    b.append(f'<circle cx="{cx}" cy="300" r="220" fill="url(#glow)"/>')
    # inner room hint: far wall line + a pedestal with a coin
    b.append(f'<path d="M{cx-150} 380H{cx+150}" stroke="{p["gold2"]}" stroke-width="3" opacity=".7"/>')
    # door leaf swung open inward (left), in perspective
    leaf = [(cx - 150, 440), (cx - 150, 190), (cx - 70, 150 + 25), (cx - 70, 420)]
    b.append(f'<path d="M{cx-150} 440V190Q{cx-120} 150 {cx-70} 158V420Z" fill="{p["brand3"]}" stroke="{p["ink"]}" stroke-width="4" stroke-linejoin="round"/>')
    b.append(f'<path d="M{cx-150} 440V190Q{cx-120} 150 {cx-70} 158V420Z" bb="{cx-150} 150 {cx-70} 440" fill="url(#hatch2)" opacity=".45"/>')
    for y in (230, 360):
        b.append(f'<path d="M{cx-150} {y}L{cx-70} {y-12}" stroke="{p["ink"]}" stroke-width="9"/>')
        b.append(f'<path d="M{cx-150} {y}L{cx-70} {y-12}" stroke="{p["metal2"]}" stroke-width="4"/>')
    # light spilling on the floor
    b.append(f'<rect y="440" width="{W}" height="{H-400}" fill="{p["stone3"]}"/>')
    b.append(f'<path d="M0 440H{W}" stroke="{p["ink"]}" stroke-width="5"/>')
    b.append(f'<path d="M{cx-150} 440L{cx-330} {H}H{cx+390}L{cx+150} 440Z" fill="{p["gold"]}" opacity=".35"/>')
    for y in (470, 510):
        b.append(f'<path d="M0 {y}H{W}" stroke="{p["ink"]}" stroke-width="2.5" opacity=".7"/>')
    b.append(f'<rect y="440" width="{W}" height="{H-440}" fill="url(#dots)" opacity="{p["texop"]}"/>')
    # step
    b.append(f'<path d="M{cx-200} 440h400l20 26h-440z" fill="{p["stone2"]}" stroke="{p["ink"]}" stroke-width="4" stroke-linejoin="round"/>')
    # sealed invite on the step
    ex, ey = cx + 60, 462
    b.append(f'<g transform="rotate(-8 {ex} {ey})">'
             f'<rect x="{ex-70}" y="{ey-40}" width="140" height="86" rx="4" fill="{p["paper"]}" stroke="{p["ink"]}" stroke-width="4"/>'
             f'<path d="M{ex-70} {ey-40}L{ex} {ey+8}L{ex+70} {ey-40}" fill="none" stroke="{p["ink"]}" stroke-width="3.5" stroke-linejoin="round"/>'
             f'<circle cx="{ex}" cy="{ey+8}" r="20" fill="{p["brand"]}" stroke="{p["ink"]}" stroke-width="3.5"/>'
             f'<path d="M{ex-7} {ey+8}l5 6l10 -12" fill="none" stroke="{p["gold3"]}" stroke-width="3.5" stroke-linecap="round" stroke-linejoin="round"/>'
             "</g>")
    # wall lanterns
    for side in (-1, 1):
        lx = cx + side * 300
        b.append(f'<circle cx="{lx}" cy="250" r="90" fill="url(#glow)"/>')
        b.append(f'<path d="M{lx} 205v18" stroke="{p["ink"]}" stroke-width="4"/>')
        b.append(f'<path d="M{lx-16} 225h32l-6 50h-20z" fill="{p["gold"]}" stroke="{p["ink"]}" stroke-width="4" stroke-linejoin="round"/>')
        b.append(f'<path d="M{lx-20} 225h40M{lx-14} 277h28" stroke="{p["ink"]}" stroke-width="5" stroke-linecap="round"/>')
    b.append("</g>")
    b.append(f'<rect width="{W}" height="{H}" fill="url(#vig)"/>')
    return svg(W, H, defs(p) + "".join(b), p["dot"])


# ---------------------------------------------------------------- (d) keys: one key, never in one place
def key_shapes(p, fill, stroke, sw, extra=""):
    """A large ornate key in local coordinates, centred on the origin, pointing right."""
    ink = stroke
    out = []
    for q in (180, 60, -60):
        x, y = -300 + 62 * math.cos(math.radians(q)), 62 * math.sin(math.radians(q))
        out.append(f'<circle cx="{f(x)}" cy="{f(y)}" r="66" fill="{fill}" stroke="{ink}" stroke-width="{sw}" {extra}/>')
    out.append(f'<circle cx="-300" cy="0" r="74" fill="{fill}" stroke="{ink}" stroke-width="{sw}" {extra}/>')
    out.append(f'<rect x="-232" y="-34" width="570" height="68" rx="10" fill="{fill}" stroke="{ink}" stroke-width="{sw}" {extra}/>')
    out.append(f'<rect x="-196" y="-58" width="38" height="116" rx="8" fill="{fill}" stroke="{ink}" stroke-width="{sw}" {extra}/>')
    out.append(f'<path d="M212 30V150H256V108H288V150H338V30Z" fill="{fill}" stroke="{ink}" stroke-width="{sw}" stroke-linejoin="round" {extra}/>')
    return "".join(out)


# Where each shard sits apart from its place in the whole key (dx, dy, degrees). The
# app's key-shards animation reads the same numbers (lib/src/features/onboarding/
# key_shards_art.dart): change both together.
SHARD_OFFSETS = [(-80, -18, -7), (-30, 48, 6), (0, -46, -4), (34, 42, 6), (84, -14, 10)]


def shards(p, layer=None):
    """The key cut into five shards, held apart. `layer` writes one layer of it for the
    app's animation, on the same 1080 x 1240 canvas: "bg" (sky, rays, rings, the ghost
    key), "piece0".."piece4" (one shard and its edges, in its place in the whole key),
    "top" (sparks and the vignette). None: the whole still scene."""
    W, H = 1080, 1240
    rnd = random.Random(5)
    cx, cy = 540, 470
    b = [f'<rect width="{W}" height="{H}" fill="url(#sky)"/>']
    for a in range(0, 360, 6):
        r = math.radians(a)
        b.append(f'<path d="M{f(cx+140*math.cos(r))} {f(cy+140*math.sin(r))}L{f(cx+900*math.cos(r))} {f(cy+900*math.sin(r))}" stroke="{p["stone3"] if not p["dark"] else p["stone2"]}" stroke-width="{2 if a % 12 else 3.5}" opacity=".8"/>')
    b.append(f'<circle cx="{cx}" cy="{cy}" r="480" fill="url(#fine)" opacity="{p["texop"]}"/>')
    b.append(f'<circle cx="{cx}" cy="{cy}" r="420" fill="url(#glow)" opacity=".35"/>')
    for r in (330, 430):
        b.append(f'<circle cx="{cx}" cy="{cy}" r="{r}" fill="none" stroke="{p["hi"] if p["dark"] else p["ink"]}" stroke-width="2.5" stroke-dasharray="3 12" opacity=".5"/>')
    rot = -20
    # ghost of the whole key: dashed, empty. It is never assembled.
    ghost = p["hi"] if p["dark"] else p["ink"]
    b.append(f'<g transform="translate({cx} {cy}) rotate({rot})" opacity=".55">'
             + key_shapes(p, "none", ghost, 3, 'stroke-dasharray="8 10"') + "</g>")
    b[-1] = b[-1].replace(f'rotate({rot})"', f'rotate({rot}) scale(1.04)"')
    # the shards: the same key cut into five jagged pieces, each held apart
    cuts = [-420, -148, -28, 90, 200, 420]

    def zig(x, down=True):
        ys = list(range(-160, 161, 32))
        pts_ = [(x + (rnd.randint(-16, 16) if -48 < y < 48 or x < -150 else 0), y) for y in ys]
        return pts_ if down else pts_[::-1]

    edges = [zig(c) for c in cuts]
    # A layer piece sits in its place; the app moves it out and back.
    offsets = [(0, 0, 0)] * 5 if layer and layer.startswith("piece") else SHARD_OFFSETS
    bg_end = len(b)
    pieces = {i: [] for i in range(5)}
    for i in range(5):
        left, right = edges[i], edges[i + 1]
        poly = left[::-1] + right  # up the left edge, down the right
        poly = [(x, y) for x, y in left] + [(x, y) for x, y in right[::-1]]
        dx, dy, dr = offsets[i]
        body = key_shapes(p, "url(#goldg)", p["ink"], 5)
        # engraving + shade inside the piece
        body += f'<path d="M-150 -16H320" stroke="{p["gold3"]}" stroke-width="6" stroke-linecap="round"/>'
        body += f'<rect x="-232" y="8" width="570" height="26" fill="url(#hatch)" opacity=".35"/>'
        body += f'<circle cx="-300" cy="0" r="30" fill="{p["sky1"]}" stroke="{p["ink"]}" stroke-width="5"/>'
        mid = (cuts[i] + cuts[i + 1]) / 2
        gx = -300 if i == 0 else mid
        gy = -62 if i == 0 else 0
        body += f'<circle cx="{f(gx)}" cy="{f(gy)}" r="18" fill="{p["brand"]}" stroke="{p["ink"]}" stroke-width="4"/>'
        body += f'<path d="M{f(gx-8)} {f(gy-5)}a9 9 0 0 1 9 -6" stroke="{p["gold3"]}" stroke-width="3" fill="none" stroke-linecap="round"/>' 
        g = (f'<g transform="translate({f(cx+dx)} {f(cy+dy)}) rotate({rot+dr}) scale(1.04)">'
             f'<clipPath id="c{i}"><polygon points="{pts(poly)}"/></clipPath>'
             f'<g clip-path="url(#c{i})">{body}</g>'
             "</g>")
        b.append(g)
        pieces[i].append(g)
        # a member mark on each piece
        mx = max(-300, min(300, mid)) if i else -300
        gx = cx + dx + mx * math.cos(math.radians(rot + dr))
        gy = cy + dy + mx * math.sin(math.radians(rot + dr))

    # jagged ink edges drawn on top so the breaks read clearly
    for i in range(5):
        dx, dy, dr = offsets[i]
        for e in (edges[i], edges[i + 1]):
            if abs(e[0][0]) >= 420:
                continue
            b.append(f'<g transform="translate({f(cx+dx)} {f(cy+dy)}) rotate({rot+dr}) scale(1.04)"><clipPath id="k{i}{f(e[0][0]+500)}">{key_shapes(p, "#000", "none", 0)}</clipPath>'
                     f'<path clip-path="url(#k{i}{f(e[0][0]+500)})" d="{line_d(e)}" fill="none" stroke="{p["ink"]}" stroke-width="7" stroke-linejoin="round"/></g>')
            pieces[i].append(b[-1])
    top_start = len(b)
    for _ in range(30):
        a = math.radians(rnd.randint(0, 359))
        r = rnd.randint(300, 520)
        x, y = cx + r * math.cos(a), cy + r * math.sin(a)
        s_ = rnd.randint(4, 10)
        b.append(f'<path d="M{f(x)} {f(y-s_)}L{f(x+s_/3)} {f(y)}L{f(x)} {f(y+s_)}L{f(x-s_/3)} {f(y)}Z" fill="{p["gold"]}"/>')
    b.append(f'<rect width="{W}" height="{H}" fill="url(#vig)"/>')
    if layer == "bg":
        return svg(W, H, defs(p) + "".join(b[:bg_end]), p["dot"])
    if layer and layer.startswith("piece"):
        return svg(W, H, defs(p) + "".join(pieces[int(layer[5:])]), p["dot"])
    if layer == "top":
        return svg(W, H, defs(p) + "".join(b[top_start:]), p["dot"])
    return svg(W, H, defs(p) + "".join(b), p["dot"])


# ---------------------------------------------------------------- shared props
def envelope(p, x, y, w, h, flap_open=False):
    """A sealed-backup envelope: body at (x, y) size w x h; the flap closed (V) or open (up)."""
    cx = x + w / 2
    ink = p["ink"]
    o = []
    if flap_open:
        o.append(f'<path d="M{f(x)} {f(y)}L{f(cx)} {f(y - h * 0.62)}L{f(x + w)} {f(y)}Z" fill="{p["stone2"] if p["dark"] else p["stone"]}" stroke="{ink}" stroke-width="5" stroke-linejoin="round"/>')
        o.append(f'<path d="M{f(cx)} {f(y - h * 0.62)}L{f(x + w)} {f(y)}H{f(cx + 30)}Z" bb="{f(cx)} {f(y - h * 0.62)} {f(x + w)} {f(y)}" fill="url(#hatch2)" opacity=".4"/>')
    o.append(f'<rect x="{f(x)}" y="{f(y)}" width="{f(w)}" height="{f(h)}" rx="8" fill="{p["paper"]}" stroke="{ink}" stroke-width="5"/>')
    o.append(f'<path d="M{f(x)} {f(y + h)}L{f(cx)} {f(y + h * 0.48)}L{f(x + w)} {f(y + h)}" fill="none" stroke="{ink}" stroke-width="3.5" stroke-linejoin="round"/>')
    o.append(f'<path d="M{f(cx)} {f(y + h * 0.48)}L{f(x + w)} {f(y + h)}H{f(cx + 40)}Z" bb="{f(cx)} {f(y + h * 0.48)} {f(x + w)} {f(y + h)}" fill="url(#hatch)" opacity=".3"/>')
    if not flap_open:
        o.append(f'<path d="M{f(x + 4)} {f(y + 4)}L{f(cx)} {f(y + h * 0.62)}L{f(x + w - 4)} {f(y + 4)}Z" fill="{p["paper"]}" stroke="{ink}" stroke-width="4" stroke-linejoin="round"/>')
        o.append(f'<path d="M{f(cx)} {f(y + h * 0.62)}L{f(x + w - 4)} {f(y + 4)}H{f(cx + 60)}Z" bb="{f(cx)} {f(y)} {f(x + w)} {f(y + h * 0.62)}" fill="url(#hatch2)" opacity=".35"/>')
    o.append(f'<path d="M{f(x + 14)} {f(y + 14)}H{f(x + w * 0.3)}" stroke="{p["hi"]}" stroke-width="3" opacity=".7" stroke-linecap="round"/>')
    return "".join(o)


def dial_lock(p, cx, cy, r=58, shackle_open=False):
    """A combination padlock with a round dial face: the passphrase."""
    ink = p["ink"]
    o = []
    lift = 34 if shackle_open else 0
    sh = f"M{f(cx - r * 0.5)} {f(cy - r * 0.55 - lift)}V{f(cy - r * 1.05 - lift)}A{f(r * 0.5)} {f(r * 0.5)} 0 0 1 {f(cx + r * 0.5)} {f(cy - r * 1.05 - lift)}V{f(cy - r * 0.55)}"
    o.append(f'<path d="{sh}" fill="none" stroke="{ink}" stroke-width="22" stroke-linecap="round"/>')
    o.append(f'<path d="{sh}" fill="none" stroke="{p["metal2"]}" stroke-width="12" stroke-linecap="round"/>')
    o.append(f'<circle cx="{f(cx)}" cy="{f(cy)}" r="{f(r)}" fill="{p["metal"]}" stroke="{ink}" stroke-width="5"/>')
    o.append(f'<circle cx="{f(cx)}" cy="{f(cy)}" r="{f(r)}" fill="url(#hatch2)" opacity=".35"/>')
    o.append(f'<circle cx="{f(cx)}" cy="{f(cy)}" r="{f(r * 0.74)}" fill="{p["stone2"]}" stroke="{p["brand"]}" stroke-width="7"/>')
    o.append(f'<circle cx="{f(cx)}" cy="{f(cy)}" r="{f(r * 0.74 + 4)}" fill="none" stroke="{ink}" stroke-width="2.5"/>')
    d = []
    for i in range(24):
        a = math.radians(i * 15 - 90)
        l = r * (0.2 if i % 6 == 0 else 0.1)
        rr = r * 0.66
        d.append(f"M{f(cx + rr * math.cos(a))} {f(cy + rr * math.sin(a))}L{f(cx + (rr - l) * math.cos(a))} {f(cy + (rr - l) * math.sin(a))}")
    o.append(f'<path d="{"".join(d)}" stroke="{ink}" stroke-width="2.5" stroke-linecap="round"/>')
    o.append(f'<circle cx="{f(cx)}" cy="{f(cy)}" r="{f(r * 0.24)}" fill="url(#goldg)" stroke="{ink}" stroke-width="4"/>')
    o.append(f'<path d="M{f(cx)} {f(cy - r * 0.62)}l{f(r * 0.1)} {f(-r * 0.2)}h{f(-r * 0.2)}z" transform="rotate(180 {f(cx)} {f(cy - r * 0.62)})" fill="{p["gold"]}" stroke="{ink}" stroke-width="2.5" stroke-linejoin="round"/>')
    o.append(f'<path d="M{f(cx - r * 0.8)} {f(cy - r * 0.2)}A{f(r * 0.82)} {f(r * 0.82)} 0 0 1 {f(cx - r * 0.2)} {f(cy - r * 0.8)}" fill="none" stroke="{p["hi"]}" stroke-width="4" stroke-linecap="round" opacity=".8"/>')
    return "".join(o)


def word_tiles(p, rnd, x, y, n=3):
    """A few blank word cards, fanned: the passphrase (no text in the art)."""
    o = []
    for i in range(n):
        tx, ty, r = x + i * 18, y + i * 64, (-10, 4, -3)[i % 3]
        w = (150, 128, 140)[i % 3]
        o.append(f'<g transform="rotate({r} {f(tx)} {f(ty)})">'
                 f'<rect x="{f(tx)}" y="{f(ty)}" width="{w}" height="46" rx="10" fill="{p["paper"]}" stroke="{p["ink"]}" stroke-width="4"/>'
                 f'<path d="M{f(tx + 18)} {f(ty + 23)}h{f(w * 0.34)}m14 0h{f(w * 0.22)}" stroke="{p["light"] if p["dark"] else p["stone3"]}" stroke-width="8" stroke-linecap="round"/>'
                 f'<circle cx="{f(tx + w - 16)}" cy="{f(ty + 23)}" r="5" fill="{p["brand"]}"/>'
                 "</g>")
    return "".join(o)


def ledge(p, y, W, H):
    o = [f'<rect y="{y}" width="{W}" height="{H - y}" fill="{p["stone3"]}"/>',
         f'<path d="M0 {y}H{W}" stroke="{p["ink"]}" stroke-width="5"/>',
         f'<path d="M0 {y + 8}H{W}" stroke="{p["hi"]}" stroke-width="2.5" opacity=".45"/>',
         f'<rect y="{y}" width="{W}" height="{H - y}" fill="url(#dots)" opacity="{p["texop"]}"/>']
    return "".join(o)


# ---------------------------------------------------------------- (e) backup: key share sealed with a passphrase
def backup(p):
    W, H = 1080, 560
    rnd = random.Random(21)
    cx = 540
    b = [f'<rect width="{W}" height="{H}" fill="url(#sky)"/>']
    b.append(f'<rect width="{W}" height="{H}" fill="url(#fine)" opacity="{float(p["texop"]) * 0.7:.2f}"/>')
    b.append(dial(p, cx, 300, 300, ticks=72, major=6, op=".35", rings=(0, 40)))
    b.append(f'<circle cx="{cx}" cy="300" r="300" fill="url(#glow)" opacity=".55"/>')
    b.append(ledge(p, 470, W, H))
    # passphrase words on the left, a dotted thread to the lock
    b.append(word_tiles(p, rnd, 110, 236))
    b.append(f'<path d="M270 300Q380 250 470 330" fill="none" stroke="{p["brand"]}" stroke-width="5" stroke-dasharray="2 12" stroke-linecap="round"/>')
    # envelope with the key share inside (a dashed ghost: sealed, not visible)
    ex, ey, ew, eh = cx - 190, 200, 380, 260
    b.append(f'<ellipse cx="{cx}" cy="474" rx="230" ry="16" fill="{p["ink"]}" opacity=".35"/>')
    b.append(envelope(p, ex, ey, ew, eh))
    b.append(key(p, ex + 44, ey + eh - 46, 0.8, 0, ghost=p["gold2"]))
    # light leaking from the seal
    b.append(f'<circle cx="{cx}" cy="{ey + eh * 0.62}" r="120" fill="url(#glow)"/>')
    b.append(dial_lock(p, cx, ey + eh * 0.62, 56))
    # the key share glints on the right, going in
    b.append(sparks(p, rnd, 5, 250, 60, 830, 200, avoid=(cx, 330, 190)))
    b.append(sparks(p, rnd, 6, 780, 180, 980, 440))
    b.append(f'<rect width="{W}" height="{H}" fill="url(#vig)"/>')
    return svg(W, H, defs(p) + "".join(b), p["dot"])


# ---------------------------------------------------------------- (f) restore: the seal opens, the key goes home
def restore(p):
    W, H = 1080, 560
    rnd = random.Random(22)
    b = [f'<rect width="{W}" height="{H}" fill="url(#sky)"/>']
    b.append(f'<rect width="{W}" height="{H}" fill="url(#fine)" opacity="{float(p["texop"]) * 0.7:.2f}"/>')
    b.append(ledge(p, 470, W, H))
    # the vault on the right: a round door with a glowing, empty keyhole
    vx, vy, vr = 770, 300, 170
    b.append(f'<rect x="{vx - 210}" y="{vy - 215}" width="420" height="385" rx="26" fill="{p["stone"]}" stroke="{p["ink"]}" stroke-width="5"/>')
    b.append(f'<rect x="{vx - 210}" y="{vy - 215}" width="420" height="385" rx="26" fill="url(#grit)" opacity=".5"/>')
    b.append(f'<path d="M{vx + 60} {vy - 215}H{vx + 184}A26 26 0 0 1 {vx + 210} {vy - 189}V{vy + 144}A26 26 0 0 1 {vx + 184} {vy + 170}H{vx + 60}Z" bb="{vx + 60} {vy - 215} {vx + 210} {vy + 170}" fill="url(#hatch)" opacity=".35"/>')
    for sx in (-180, 180):
        b.append(f'<circle cx="{vx + sx}" cy="{vy - 185}" r="8" fill="{p["metal2"]}" stroke="{p["ink"]}" stroke-width="3"/>')
        b.append(f'<circle cx="{vx + sx}" cy="{vy + 140}" r="8" fill="{p["metal2"]}" stroke="{p["ink"]}" stroke-width="3"/>')
    b.append(f'<circle cx="{vx}" cy="{vy}" r="{vr}" fill="{p["metal"]}" stroke="{p["ink"]}" stroke-width="5"/>')
    b.append(f'<circle cx="{vx}" cy="{vy}" r="{vr - 22}" fill="{p["stone2"]}" stroke="{p["ink"]}" stroke-width="4"/>')
    b.append(f'<circle cx="{vx}" cy="{vy}" r="{vr - 22}" fill="url(#grit)" opacity=".4"/>')
    b.append(dial(p, vx, vy, vr - 30, ticks=60, major=5, color=p["ink"], op=".75", rings=(0,)))
    b.append(f'<path d="M{vx - vr + 30} {vy - 30}A{vr - 26} {vr - 26} 0 0 1 {vx - 30} {vy - vr + 30}" fill="none" stroke="{p["hi"]}" stroke-width="4" stroke-linecap="round" opacity=".7"/>')
    b.append(f'<circle cx="{vx}" cy="{vy}" r="110" fill="url(#glowl)"/>')
    b.append(f'<circle cx="{vx}" cy="{vy}" r="46" fill="{p["metal2"]}" stroke="{p["ink"]}" stroke-width="4"/>')
    b.append(f'<circle cx="{vx}" cy="{vy}" r="46" fill="none" stroke="{p["brand"]}" stroke-width="6" opacity=".9"/>')
    b.append(f'<path d="M{vx} {vy - 18}a11 11 0 1 1 -.1 0zM{vx - 7} {vy - 4}h14l6 26h-26z" fill="{p["ink"]}"/>')
    # the opened backup on the left: envelope, flap up, lock open beside it
    ex, ey, ew, eh = 70, 300, 260, 170
    b.append(f'<ellipse cx="{ex + ew / 2}" cy="474" rx="160" ry="12" fill="{p["ink"]}" opacity=".35"/>')
    b.append(envelope(p, ex, ey, ew, eh, flap_open=True))
    b.append(f'<circle cx="{ex + ew / 2}" cy="{ey}" r="120" fill="url(#glow)"/>')
    b.append(dial_lock(p, 380, 428, 38, shackle_open=True))
    # the key's path home: a dotted arc from the envelope to the keyhole
    b.append(f'<path d="M{ex + ew / 2} {ey - 10}Q300 70 {vx - 70} {vy - 6}" fill="none" stroke="{p["gold"]}" stroke-width="5" stroke-dasharray="2 13" stroke-linecap="round"/>')
    b.append(f'<circle cx="440" cy="170" r="110" fill="url(#glow)"/>')
    b.append(key(p, 395, 175, 0.95, 22))
    b.append(sparks(p, rnd, 12, 150, 60, 560, 260, avoid=(440, 190, 80)))
    b.append(f'<rect width="{W}" height="{H}" fill="url(#vig)"/>')
    return svg(W, H, defs(p) + "".join(b), p["dot"])


# ---------------------------------------------------------------- (g) sent: coins leave through the vault's slot
# The sent scene's coin trails (start in the slot, control point, end) and where each
# coin sits on its trail. The app's "Sent" animation flies the coins along the same
# curves (lib/src/features/proposals/sent_art.dart): change both together.
SENT_TRAILS = [[(480, 300 + 20), (250, 560), (70, 1000)], [(600, 300 + 20), (840, 520), (1010, 940)]]
SENT_COIN_U = (0.22, 0.48, 0.74)


def sent(p, layer=None):
    """Full-page background behind the send progress screen: detail at the top and the
    edges, the middle (status circle and text) left empty. `layer`, for the app's
    animation: "bg" (below the slot's light), "glow" (the light), "fore" (slot, trails,
    sparks, ripples; no coins), "coin" (one coin on a small canvas)."""
    W, H = 1080, 2340
    rnd = random.Random(31)
    cx, cy, R = 540, -150, 600
    b = [f'<rect width="{W}" height="{H}" fill="{p["sky1"]}"/>',
         f'<rect width="{W}" height="900" fill="url(#skyt)"/>']
    # the vault door, mostly above the screen
    b.append(f'<circle cx="{cx}" cy="{cy}" r="{R + 30}" fill="{p["ink"]}" opacity="{".3" if p["dark"] else ".1"}"/>')
    b.append(f'<circle cx="{cx}" cy="{cy}" r="{R}" fill="{p["metal"]}" stroke="{p["ink"]}" stroke-width="5"/>')
    b.append(f'<circle cx="{cx}" cy="{cy}" r="{R - 34}" fill="{p["stone2"]}" stroke="{p["ink"]}" stroke-width="5"/>')
    b.append(f'<circle cx="{cx}" cy="{cy}" r="{R - 34}" fill="url(#grit)" opacity=".35"/>')
    rv = []
    for a in range(10, 171, 8):
        r_ = math.radians(a)
        rv.append(f'M{f(cx + (R - 17) * math.cos(r_))} {f(cy + (R - 17) * math.sin(r_))}h.1')
    b.append(f'<path d="{"".join(rv)}" stroke="{p["ink"]}" stroke-width="15" stroke-linecap="round"/>')
    b.append(f'<path d="{"".join(rv)}" stroke="{p["metal2"]}" stroke-width="9" stroke-linecap="round"/>')
    b.append(f'<path d="M{cx + R - 34} {cy}A{R - 34} {R - 34} 0 0 1 {cx} {cy + R - 34}A{R - 34} {R - 34} 0 0 1 {f(cx - 0.5 * (R - 34))} {f(cy + 0.866 * (R - 34))}A{R - 60} {R - 60} 0 0 0 {cx + R - 34} {cy}Z" bb="{f(cx - 0.5 * (R - 34))} {cy} {cx + R - 34} {cy + R - 34}" fill="url(#hatch)" opacity=".45"/>')
    b.append(dial(p, cx, cy, R - 60, ticks=90, major=6, color=p["ink"], op=".6", rings=(0, 44)))
    b.append(f'<path d="M{f(cx - (R - 70) * 0.98)} {f(cy + (R - 70) * 0.2)}A{R - 70} {R - 70} 0 0 0 {f(cx - (R - 70) * 0.6)} {f(cy + (R - 70) * 0.8)}" fill="none" stroke="{p["hi"]}" stroke-width="4" stroke-linecap="round" opacity=".6"/>')
    # the slot, light pouring out
    sy = cy + R - 150
    slot_glow = f'<ellipse cx="{cx}" cy="{sy + 60}" rx="340" ry="200" fill="url(#glow)"/>'
    glow_at = len(b)  # "bg" ends here, "fore" starts: the glow sits between them
    if layer is None:
        b.append(slot_glow)
    b.append(f'<rect x="{cx - 150}" y="{sy - 18}" width="300" height="36" rx="18" fill="{p["metal2"]}" stroke="{p["ink"]}" stroke-width="5"/>')
    b.append(f'<rect x="{cx - 128}" y="{sy - 7}" width="256" height="14" rx="7" fill="{p["gold3"]}" stroke="{p["ink"]}" stroke-width="3"/>')
    # brand member marks around the slot (the signatures that let it out)
    for dx in (-210, -180, 180, 210):
        b.append(f'<circle cx="{cx + dx}" cy="{sy}" r="9" fill="{p["brand"]}" stroke="{p["ink"]}" stroke-width="3"/>')
    # coins drifting out to the edges along two dotted trails
    trails = [[(cx - 60, sy + 20), (250, 560), (70, 1000)], [(cx + 60, sy + 20), (840, 520), (1010, 940)]]
    assert trails == SENT_TRAILS, "keep SENT_TRAILS (and the app) in step"
    for t in trails:
        b.append(f'<path d="M{t[0][0]} {t[0][1]}Q{t[1][0]} {t[1][1]} {t[2][0]} {t[2][1]}" fill="none" stroke="{p["gold"]}" stroke-width="4" stroke-dasharray="2 14" stroke-linecap="round" opacity=".7"/>')

    def on(t, u):
        (x0, y0), (x1, y1), (x2, y2) = t
        return ((1 - u) ** 2 * x0 + 2 * (1 - u) * u * x1 + u * u * x2,
                (1 - u) ** 2 * y0 + 2 * (1 - u) * u * y1 + u * u * y2)
    coins = {}
    for side, t in enumerate(trails):
        for k, u in enumerate(SENT_COIN_U):
            x, y = on(t, u)
            r_ = 40 - k * 9
            coins[f"coin{side}{k}"] = f'<g opacity="{1 - k * 0.22:.2f}">' + coin(p, x, y, r_, r_ * 0.45, (-24 + k * 14) * (1 if side else -1)) + "</g>"
            if layer is None:
                b.append(coins[f"coin{side}{k}"])
    b.append(sparks(p, rnd, 10, 30, 420, 260, 1150))
    b.append(sparks(p, rnd, 10, 820, 420, 1050, 1150))
    # quiet ripples rising from the bottom edge
    for i, r_ in enumerate((360, 520, 700)):
        b.append(f'<circle cx="{cx}" cy="{H + 240}" r="{r_}" fill="none" stroke="{p["hi"] if p["dark"] else p["ink"]}" stroke-width="3" stroke-dasharray="{"3 12" if i else "none"}" opacity="{0.35 - i * 0.08:.2f}"/>')
    b.append(dial(p, cx, H + 240, 470, ticks=72, major=6, op=".25", rings=(0,)))
    b.append(f'<ellipse cx="{cx}" cy="{H}" rx="520" ry="260" fill="url(#glowl)" opacity=".45"/>')
    b.append(sparks(p, rnd, 8, 60, 1850, 1020, 2300, avoid=(cx, H + 240, 380)))
    b.append(f'<rect width="{W}" height="{H}" fill="url(#vigp)"/>')
    extra = (f'<linearGradient id="skyt" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{p["sky0"]}"/>'
             f'<stop offset="1" stop-color="{p["sky1"]}"/></linearGradient>'
             f'<radialGradient id="vigp" cx="50%" cy="50%" r="75%"><stop offset="60%" stop-color="{p["sky1"]}" stop-opacity="0"/>'
             f'<stop offset="100%" stop-color="{p["dot"]}" stop-opacity="{".45" if p["dark"] else ".08"}"/></radialGradient>')
    if layer == "bg":
        return svg(W, H, defs(p, extra) + "".join(b[:glow_at]), p["dot"])
    if layer == "fore":
        return svg(W, H, defs(p, extra) + "".join(b[glow_at:]), p["dot"])
    if layer == "glow":
        # The light from the slot, its own layer so the app can breathe and dim it.
        return svg(W, H, defs(p, extra) + slot_glow, p["dot"])
    if layer == "coin":
        # One coin on its own small canvas (80 wide, centre 50,30): the app draws many
        # of them streaming out on "Sent!".
        return svg(100, 64, defs(p, extra) + coin(p, 50, 28, 40, 18), p["dot"])
    return svg(W, H, defs(p, extra) + "".join(b), p["dot"])


# ---------------------------------------------------------------- (h) empty activity: a quiet shelf, a blank ledger
def empty(p):
    W, H = 1080, 440
    rnd = random.Random(41)
    cx = 540
    b = [f'<rect width="{W}" height="{H}" fill="url(#sky)"/>']
    b.append(f'<rect width="{W}" height="{H}" fill="url(#fine)" opacity="{float(p["texop"]) * 0.6:.2f}"/>')
    b.append(dial(p, cx, 330, 330, ticks=72, major=6, op=".25", rings=(0, 40)))
    b.append(f'<circle cx="{cx}" cy="260" r="260" fill="url(#glow)" opacity=".35"/>')
    sy = 340
    # shelf with brackets
    b.append(f'<rect y="{sy + 34}" width="{W}" height="{H - sy - 34}" fill="{p["stone3"]}"/>')
    b.append(f'<rect y="{sy + 34}" width="{W}" height="{H - sy - 34}" fill="url(#dots)" opacity="{p["texop"]}"/>')
    b.append(f'<rect x="60" y="{sy}" width="960" height="34" rx="6" fill="{p["stone2"]}" stroke="{p["ink"]}" stroke-width="5"/>')
    b.append(f'<rect x="60" y="{sy + 18}" width="960" height="16" fill="url(#hatch)" opacity=".4"/>')
    b.append(f'<path d="M74 {sy + 9}H700" stroke="{p["hi"]}" stroke-width="3" opacity=".6" stroke-linecap="round"/>')
    for bx in (150, 930):
        b.append(f'<path d="M{bx - 18} {sy + 34}h36v24l-18 30l-18 -30z" fill="{p["metal"]}" stroke="{p["ink"]}" stroke-width="4" stroke-linejoin="round"/>')
    # a small safe on the left, shut
    x0, w, h = 190, 190, 176
    y0 = sy - h
    b.append(f'<rect x="{x0}" y="{y0}" width="{w}" height="{h}" rx="14" fill="{p["metal"]}" stroke="{p["ink"]}" stroke-width="5"/>')
    b.append(f'<rect x="{x0 + 16}" y="{y0 + 16}" width="{w - 32}" height="{h - 32}" rx="8" fill="{p["stone2"]}" stroke="{p["ink"]}" stroke-width="3.5"/>')
    b.append(f'<rect x="{x0 + w - 70}" y="{y0 + 16}" width="54" height="{h - 32}" fill="url(#hatch)" opacity=".4"/>')
    b.append(f'<circle cx="{x0 + w / 2 - 8}" cy="{y0 + h / 2}" r="46" fill="{p["metal2"]}" stroke="{p["ink"]}" stroke-width="4"/>')
    b.append(dial(p, x0 + w / 2 - 8, y0 + h / 2, 40, ticks=24, major=6, color=p["ink"], op=".9", width=2.5, rings=(0,)))
    b.append(f'<circle cx="{x0 + w / 2 - 8}" cy="{y0 + h / 2}" r="12" fill="{p["brand"]}" stroke="{p["ink"]}" stroke-width="3.5"/>')
    b.append(f'<path d="M{x0 + w - 30} {y0 + 60}v56" stroke="{p["ink"]}" stroke-width="12" stroke-linecap="round"/>')
    b.append(f'<path d="M{x0 + w - 30} {y0 + 60}v56" stroke="{p["metal2"]}" stroke-width="6" stroke-linecap="round"/>')
    for fx in (x0 + 26, x0 + w - 44):
        b.append(f'<rect x="{fx}" y="{sy - 4}" width="18" height="8" fill="{p["ink"]}"/>')
    # the open ledger, blank
    lx, ly = 650, sy - 6
    left = f"M{lx} {ly - 6}C{lx - 60} {ly - 30} {lx - 140} {ly - 24} {lx - 200} {ly}L{lx - 180} {ly - 150}C{lx - 120} {ly - 172} {lx - 50} {ly - 170} {lx} {ly - 148}Z"
    right = f"M{lx} {ly - 6}C{lx + 60} {ly - 30} {lx + 140} {ly - 24} {lx + 200} {ly}L{lx + 180} {ly - 150}C{lx + 120} {ly - 172} {lx + 50} {ly - 170} {lx} {ly - 148}Z"
    b.append(f'<path d="M{lx - 212} {ly + 4}L{lx - 190} {ly - 140}H{lx + 190}L{lx + 212} {ly + 4}Z" fill="{p["brand2"]}" stroke="{p["ink"]}" stroke-width="4" stroke-linejoin="round"/>')
    b.append(f'<path d="{left}" fill="{p["paper"]}" stroke="{p["ink"]}" stroke-width="4" stroke-linejoin="round"/>')
    b.append(f'<path d="{right}" fill="{p["paper"]}" stroke="{p["ink"]}" stroke-width="4" stroke-linejoin="round"/>')
    b.append(f'<path d="{right}" bb="{lx} {ly - 170} {lx + 200} {ly}" fill="url(#hatch2)" opacity=".25"/>')
    rules = []
    for i in range(5):
        y = ly - 128 + i * 24
        rules.append(f"M{lx - 168 + i * 4} {y + 4}Q{lx - 90} {y - 12} {lx - 16} {y + 2}M{lx + 16} {y + 2}Q{lx + 90} {y - 12} {lx + 168 - i * 4} {y + 4}")
    b.append(f'<path d="{"".join(rules)}" fill="none" stroke="{p["light"] if p["dark"] else p["stone3"]}" stroke-width="3"/>')
    b.append(f'<path d="M{lx + 70} {ly - 162}v150l14 -14l14 14v-150" fill="{p["brand"]}" stroke="{p["ink"]}" stroke-width="3.5" stroke-linejoin="round"/>')
    # a single coin resting at the right: the vault is ready, nothing moved yet
    b.append(coin(p, 930, sy - 16, 38, 16, 0))
    b.append(sparks(p, rnd, 5, 820, 120, 1020, 260))
    b.append(sparks(p, rnd, 3, 60, 90, 170, 230))
    b.append(f'<rect width="{W}" height="{H}" fill="url(#vig)"/>')
    return svg(W, H, defs(p) + "".join(b), p["dot"])


SCENES = {"welcome_vault": welcome, "create_stones": create, "join_doorway": join, "key_shards": shards,
          "backup_seal": backup, "restore_key": restore,
          "sent_slot": sent, "empty_ledger": empty}

# Scenes the app animates, and the layers it moves.
LAYERS = {"welcome_vault": ["bg", "boltsL", "boltsR", "door", "glow0", "glow1", "glow4", "locks",
                            "key0", "key1", "key4", "wheel", "glint", "top"],
          "key_shards": ["bg", "piece0", "piece1", "piece2", "piece3", "piece4", "top"],
          "sent_slot": ["bg", "glow", "fore", "coin"]}

def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--out", type=Path, default=ASSETS, help="output directory (default: app/assets/illustrations)")
    ap.add_argument("names", nargs="*", help="scene names to build (default: all)")
    args = ap.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    for name in args.names or SCENES:
        for theme, pal in PALETTES.items():
            path = args.out / f"{name}_{theme}.svg"
            path.write_text(SCENES[name](pal))
            kb = path.stat().st_size / 1024
            print(f"{path}  {kb:.1f} KB" + ("  <-- over the 60 KB budget" if kb > 60 else ""))
            # Layers for the app's animation of this scene.
            for layer in LAYERS.get(name, []):
                lp = args.out / "layers" / f"{name}_{layer}_{theme}.svg"
                lp.parent.mkdir(parents=True, exist_ok=True)
                lp.write_text(SCENES[name](pal, layer))
                print(f"  {lp}  {lp.stat().st_size / 1024:.1f} KB")


if __name__ == "__main__":
    main()
