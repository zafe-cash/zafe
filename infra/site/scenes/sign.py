# "Your phone checks every payment": the background for the request card (film/src/Loop.tsx).
# A desk in a room in shade; light comes through a window up-left and lands on the wall and
# the payment slip. The card floats in the middle; the darkest accents sit at its edges (the
# stamp on the right, the magnifier over the slip at bottom-left), the corners stay quiet.
# The story: the magnifier shows the slip's address (the same as the card's), the three
# signers are pinned on the wall, and when the card shows "Approved" the stamp presses the
# wax seal on the envelope.
#   python3 tex.py <workdir>/tex
#   blender --background --factory-startup --python sign.py -- <workdir> still <frame> | anim
import math
import os
import sys


sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import kit  # noqa: E402
import props  # noqa: E402
from kit import LOOP  # noqa: E402

ARGS = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
WORK = ARGS[0] if ARGS else os.getcwd()
MODE = ARGS[1] if len(ARGS) > 1 else "still"
TEX = os.path.join(WORK, "tex")
scene = kit.reset()


M = props.materials(TEX)
props.room(M)
props.lamp(M, -3.25, 1.35)
props.books(M, -2.15, 1.75)
props.mug(M, -1.55, 0.6)
# The tall cut (kit.TALL) stages the action on the desk below the card.
T = kit.TALL
props.slips_and_magnifier(M, *((-0.95, -3.05) if T else (-1.45, -1.2)))
props.coins(M, *((1.05, -2.95) if T else (2.3, -1.25)))
ex, ey, erz = (-0.05, -2.35, -8) if T else (1.45, -0.45, -8)
_, blob, seal = props.envelope(M, ex, ey, erz)
a = math.radians(erz)
wx, wy = ex + 0.02 * math.sin(a), ey - 0.02 * math.cos(a)  # the wax, in the room
tx, ty = (0.85, -1.7) if T else (1.95, 0.75)  # the stamp's rest, behind the envelope
stamp, stamp_shadow = props.stamp(M, tx, ty)
LEAVES = props.plant(M, 3.35, 1.75)
props.board(M, 2.55, 2.55)
props.clock(M, -2.85, 2.85)
props.notes(M, [(3.4, 2.02, 6), (4.05, 2.75, -7)])
props.window_light((-2.95, props.WALL_Y, 2.2))

# --- camera ----------------------------------------------------------------------------------
kit.camera((0.0, -8.6, 3.3), (0.0, 0.6, 1.2), lens=40)

# --- motion: the stamp acts, the plant breathes, the light drifts -------------------------
PRESS = 66  # the card shows "Approved" on its frame 66


R0, OV = (tx, ty), (wx, wy)
MID = ((tx + wx) / 2 + 0.1, (ty + wy) / 2)
# (frame, (x, y, z, tilt°), easing into it); tilt leans the handle toward -y (the travel)
STAMP = [
    (0, (*R0, 0.0, 0.0), "inout"), (40, (*R0, 0.0, 0.0), "inout"),
    (46, (*R0, 0.06, -6.0), "inout"),            # anticipation: lift a little, lean back
    (54, (*MID, 0.78, 9.0), "inout"),            # travel on an arc, leaning into it
    (60, (*OV, 0.38, 3.0), "out"),
    (63, (*OV, 0.32, 0.0), "inout"),             # aim
    (PRESS, (*OV, 0.018, 0.0), "in"),            # strike
    (72, (*OV, 0.018, 0.0), "linear"),           # contact hold
    (80, (*OV, 0.42, -3.0), "out"),
    (90, (*MID, 0.72, -7.0), "inout"),
    (98, (*R0, 0.04, 2.0), "inout"),
    (101, (*R0, 0.0, 3.0), "in"),
    (105, (*R0, 0.0, 0.0), "out"),
    (LOOP, (*R0, 0.0, 0.0), "linear"),
]
kit.key_every_frame(stamp, "location", lambda f: kit.poses(STAMP, f)[:3])
kit.key_every_frame(stamp, "rotation_euler", lambda f: (math.radians(kit.poses(STAMP, f)[3]), 0, 0))


def shadow_pose(f):
    x, y, z, _ = kit.poses(STAMP, f)
    return (x, y, 0.002)


kit.key_every_frame(stamp_shadow, "location", shadow_pose)


def shadow_size(f):  # full on the desk, gone once the stamp is a hand's width up
    k = max(0.0, 1 - kit.poses(STAMP, f)[2] / 0.25)
    return (max(0.25 * k, 0.001), max(0.22 * k, 0.001), 1)


kit.key_every_frame(stamp_shadow, "scale", shadow_size)
# The wax: a soft blob until the strike, then a flat seal that squashes and settles; the
# blob comes back while the card resets (frames 104-126), so the loop starts unsealed.
RESET = 114


def seal_scale(f):
    if f < PRESS or f >= RESET:
        return (0.01, 0.01, 0.01)
    t = (f - PRESS) / 8
    sq = kit.ease(t, "out")
    return (1.25 - 0.25 * sq, 1.25 - 0.25 * sq, 0.6 + 0.4 * sq)


kit.key_every_frame(seal, "scale", seal_scale)
kit.key_every_frame(blob, "scale", lambda f: (0.01,) * 3 if PRESS <= f < RESET else
                    ((0.85 + 0.15 * kit.ease((f - RESET) / 8, "out"),) * 3 if f >= RESET else (1, 1, 1)))
props.breeze(LEAVES)

kit.bake_scale()
kit.lines()

TAG = os.environ.get("ZAFE_TAG", "sign")
if MODE == "anim":
    kit.render(os.path.join(WORK, "out", TAG), anim=True)
else:
    kit.render(os.path.join(WORK, "out", f"{TAG}.png"), frame=int(ARGS[2]) if len(ARGS) > 2 else 0)
