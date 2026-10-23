# "It sends itself": the background for the app's Sent screen (film/src/LoopCard.tsx).
# The same room. Right of the card, a brass pneumatic tube comes down the wall to a mouth
# over a tray; the window's light lands on it. The sealed payment waits in a capsule in the
# tray. While the card says "Sending…" the capsule lifts, turns upright and rises to the
# mouth; at "Sent!" (the app's frame 47) the tube takes it and the gauge needle kicks. Before
# the loop ends the next sealed capsule drops into the tray.
#   blender --background --factory-startup --python sent.py -- <workdir> still <frame> | anim
import math
import os
import sys

from mathutils import Vector

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import kit  # noqa: E402
import props  # noqa: E402
from kit import ball, disc, empty, lathe, rod, slab  # noqa: E402

kit.LOOP = 132  # the Sent screen's length
ARGS = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
WORK = ARGS[0] if ARGS else os.getcwd()
MODE = ARGS[1] if len(ARGS) > 1 else "still"
TEX = os.path.join(WORK, "tex")
scene = kit.reset()

M = props.materials(TEX)
M["gauge"] = kit.toon("gauge", props.C["paper"], grain=None, decal=os.path.join(TEX, "gauge.png"), decal_uv=True)
M["hole"] = kit.flat("hole", "#163330")
props.room(M)
props.lamp(M, -3.3, 1.3)
props.books(M, -2.2, 1.75)
props.mug(M, -1.5, 0.55, mat=M["verdigris"], rz=-150)
stamp, _ = props.stamp(M, -1.2, -0.75)  # done for today: at rest by the card's left edge
LEAVES = props.plant(M, 3.85, 1.6)
props.board(M, -2.55, 2.6)
props.clock(M, 3.7, 3.15)
props.coins(M, *((0.55, -2.65) if kit.TALL else (2.35, -1.05)), stack=4, loose=((0.45, -0.3),))

# --- the pneumatic tube station (right of the card) ---------------------------------------
TX, TY = (1.75 if kit.TALL else 2.0), props.WALL_Y - 0.32  # the tube's axis, in front of the wall
MOUTH_Z = 1.55
rod("tube", M["brass"], (TX, TY, MOUTH_Z + 0.27), (TX, TY, 8.0), 0.2, verts=48)
for k, z in enumerate((2.3, 3.7)):  # wall brackets
    slab(f"bracket{k}", M["wood"], (TX, TY + 0.17, z), (0.5, 0.3, 0.1), bevel=0.02)
    lathe(f"collar{k}", M["brass"], (TX, TY, z - 0.06), [(0, 0), (0.23, 0), (0.24, 0.02), (0.24, 0.1), (0.23, 0.12), (0, 0.12)])
lathe("mouth", M["brass"], (TX, TY, MOUTH_Z),
      [(0.36, 0.0), (0.37, 0.03), (0.33, 0.07), (0.25, 0.16), (0.21, 0.27), (0.2, 0.3)], caps=(False, False))
disc("mouth_hole", M["hole"], (TX, TY, MOUTH_Z + 0.12), 0.27)
# the gauge, on the tube's front
GZ = 2.95
gauge_rim = lathe("gauge_rim", M["brass"], (TX, TY - 0.2, GZ),
                  [(0, 0), (0.27, 0), (0.29, 0.03), (0.27, 0.07), (0.23, 0.07), (0, 0.07)])
gauge_rim.rotation_euler = (math.radians(90), 0, 0)
disc("gauge_face", M["gauge"], (TX, TY - 0.275, GZ), 0.23, rot=(90, 0, 0))
needle = empty("needle", (TX, TY - 0.285, GZ))
n = slab("needle_bar", M["ink"], (0, 0, 0.08), (0.025, 0.01, 0.17))
n.parent = needle
ball("needle_hub", M["brass"], (TX, TY - 0.29, GZ), 0.03)
# the tray it lands in
CX, CY = TX, 1.95
slab("tray", M["wood"], (CX, CY, 0.025), (1.15, 0.55, 0.05), bevel=0.01)
for k, (dx, dy, sx, sy) in enumerate([(0, 0.255, 1.15, 0.04), (0, -0.255, 1.15, 0.04), (0.555, 0, 0.04, 0.55), (-0.555, 0, 0.04, 0.55)]):
    slab(f"tray_rim{k}", M["wood"], (CX + dx, CY + dy, 0.08), (sx, sy, 0.1), bevel=0.008)
kit.ellipse_shadow("tray_contact", M["contact"], (CX, CY, 0), 0.55, 0.32)


def capsule(name, sealed):
    """A carrier: brass end caps, a glass body; the sent one carries the green seal."""
    c = empty(name, (0, 0, 0))
    cap = [(0, 0), (0.1, 0), (0.15, 0.03), (0.165, 0.08), (0.165, 0.14), (0, 0.14)]
    lathe(f"{name}_cap0", M["brass"], (0, 0, -0.33), cap).parent = c
    top = lathe(f"{name}_cap1", M["brass"], (0, 0, 0.33), cap)
    top.rotation_euler = (math.radians(180), 0, 0)
    top.parent = c
    lathe(f"{name}_body", M["glass"], (0, 0, -0.2), [(0, 0), (0.15, 0), (0.15, 0.4), (0, 0.4)]).parent = c
    lathe(f"{name}_band", M["verdigris"], (0, 0, -0.04), [(0, 0), (0.158, 0), (0.158, 0.08), (0, 0.08)]).parent = c
    if sealed:  # the payment's seal, facing out
        s = lathe(f"{name}_seal", M["verdigris"], (0, -0.15, 0.13), [(0, 0), (0.075, 0), (0.08, 0.01), (0.07, 0.022), (0, 0.022)])
        s.rotation_euler = (math.radians(90), 0, 0)
        s.parent = c
        disc(f"{name}_seal_face", M["seal"], (0, -0.173, 0.13), 0.07, rot=(90, 0, 0)).parent = c
    return c


sent = capsule("capsule", sealed=True)
back = capsule("carrier", sealed=True)  # the next one: identical, so the loop has no seam
for c in (sent, back):  # the hero: big enough to read beside the card
    c.scale = (1.35, 1.35, 1.35)

props.window_light((2.75, props.WALL_Y, 2.6), size=(2.2, 2.4))
kit.camera((0.0, -8.6, 3.3), (0.0, 0.6, 1.2), lens=40)

# --- motion ----------------------------------------------------------------------------------
SENT = 47  # the app's check turns gold
REST = (CX, CY, 0.27)  # lying in the tray
UNDER = (TX, TY, MOUTH_Z - 0.55)  # upright, just under the mouth
GONE = (TX, TY, MOUTH_Z + 1.4)  # inside the tube, out of sight
# (frame, (x, y, z, tilt° from lying to upright, roll°), easing into it)
SEND = [
    (0, (*REST, 0.0, 0.0), "inout"), (10, (*REST, 0.0, 0.0), "inout"),
    (15, (CX - 0.04, CY, 0.31, -8.0, 0.0), "inout"),              # anticipation: tip back
    (27, (TX, TY - 0.05, MOUTH_Z - 0.85, 70.0, 0.0), "inout"),     # rise, turning upright
    (33, (*UNDER, 90.0, 0.0), "out"),
    (40, (TX, TY, MOUTH_Z - 0.5, 90.0, 0.0), "inout"),            # hover under the mouth
    (SENT, (*GONE, 90.0, 0.0), "in"),                               # taken
    (kit.LOOP, (*GONE, 90.0, 0.0), "linear"),
]
RETURN = [
    (0, (*GONE, 90.0, 0.0), "linear"), (106, (*GONE, 90.0, 0.0), "linear"),
    (113, (TX, TY, MOUTH_Z - 0.4, 90.0, 0.0), "in"),                # drops out of the mouth
    (119, (CX, CY - 0.02, 0.27, 8.0, 0.0), "in"),                  # falls over into the tray
    (122, (CX, CY - 0.02, 0.32, -4.0, 0.0), "out"),                 # a small bounce
    (125, (*REST, 0.0, 0.0), "in"),
    (kit.LOOP, (*REST, 0.0, 0.0), "linear"),
]


def place(c, table, start_visible, hide_after=None, show_from=None):
    kit.key_every_frame(c, "location", lambda f: kit.poses(table, f)[:3])
    # lying along x in the tray; tilting turns it upright (its axis to +z)
    kit.key_every_frame(c, "rotation_euler", lambda f: (0, math.radians(90 - kit.poses(table, f)[3]), 0))

    def hidden(f):
        if hide_after is not None and f >= hide_after:
            return True
        if show_from is not None and f < show_from:
            return True
        return not start_visible and show_from is None

    for f in range(kit.LOOP):
        for o in [c, *c.children_recursive]:
            o.hide_render = hidden(f)
            o.keyframe_insert("hide_render", frame=f + 1)


place(sent, SEND, True, hide_after=SENT + 1)
place(back, RETURN, False, show_from=108)
# The gauge: the needle kicks as the tube takes the capsule, overshoots, settles, eases back.
NEEDLE = [(0, (-95.0,), "inout"), (SENT - 3, (-95.0,), "inout"), (SENT + 3, (55.0,), "out"),
          (SENT + 8, (25.0,), "inout"), (SENT + 13, (40.0,), "inout"), (SENT + 20, (32.0,), "inout"),
          (112, (-80.0,), "inout"), (kit.LOOP, (-95.0,), "inout")]
kit.key_every_frame(needle, "rotation_euler", lambda f: (0, math.radians(kit.poses(NEEDLE, f)[0]), 0))
props.breeze(LEAVES)

kit.bake_scale()
kit.lines()

TAG = os.environ.get("ZAFE_TAG", "sent")
if MODE == "anim":
    kit.render(os.path.join(WORK, "out", TAG), anim=True)
else:
    kit.render(os.path.join(WORK, "out", f"{TAG}.png"), frame=int(ARGS[2]) if len(ARGS) > 2 else 0)
