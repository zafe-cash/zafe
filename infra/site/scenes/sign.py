import math
import os
import sys


sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import kit
import props
from kit import LOOP

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
T = kit.TALL
props.slips_and_magnifier(M, *((-0.95, -3.05) if T else (-1.45, -1.2)))
props.coins(M, *((1.05, -2.95) if T else (2.3, -1.25)))
ex, ey, erz = (-0.05, -2.35, -8) if T else (1.45, -0.45, -8)
_, blob, seal = props.envelope(M, ex, ey, erz)
a = math.radians(erz)
wx, wy = ex + 0.02 * math.sin(a), ey - 0.02 * math.cos(a)
tx, ty = (0.85, -1.7) if T else (1.95, 0.75)
stamp, stamp_shadow = props.stamp(M, tx, ty)
LEAVES = props.plant(M, 3.35, 1.75)
props.board(M, 2.55, 2.55)
props.clock(M, -2.85, 2.85)
props.notes(M, [(3.4, 2.02, 6), (4.05, 2.75, -7)])
props.window_light((-2.95, props.WALL_Y, 2.2))

kit.camera((0.0, -8.6, 3.3), (0.0, 0.6, 1.2), lens=40)

PRESS = 66


R0, OV = (tx, ty), (wx, wy)
MID = ((tx + wx) / 2 + 0.1, (ty + wy) / 2)
STAMP = [
    (0, (*R0, 0.0, 0.0), "inout"), (40, (*R0, 0.0, 0.0), "inout"),
    (46, (*R0, 0.06, -6.0), "inout"),
    (54, (*MID, 0.78, 9.0), "inout"),
    (60, (*OV, 0.38, 3.0), "out"),
    (63, (*OV, 0.32, 0.0), "inout"),
    (PRESS, (*OV, 0.018, 0.0), "in"),
    (72, (*OV, 0.018, 0.0), "linear"),
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


def shadow_size(f):
    k = max(0.0, 1 - kit.poses(STAMP, f)[2] / 0.25)
    return (max(0.25 * k, 0.001), max(0.22 * k, 0.001), 1)


kit.key_every_frame(stamp_shadow, "scale", shadow_size)
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
