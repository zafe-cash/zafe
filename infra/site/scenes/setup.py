# "Start a vault together": the background for the app's key-making screen
# (film/src/LoopCard.tsx). Looking down at a round table in the same room's light: three
# seats (a mug and the same four safety words on each person's card), and a strongbox right
# of the card. Its lid closes while the phones make the key; three keys, one per signer,
# slide into its three locks and turn together at "Vault keys ready" (the app's frame 62).
# Near the end they turn back, come out and the lid opens, so the loop starts open.
#   blender --background --factory-startup --python setup.py -- <workdir> still <frame> | anim
import math
import os
import sys

from mathutils import Vector

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import kit  # noqa: E402
import props  # noqa: E402
from kit import empty, lathe, rod, slab  # noqa: E402

kit.LOOP = 150  # the key-making screen's length
ARGS = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
WORK = ARGS[0] if ARGS else os.getcwd()
MODE = ARGS[1] if len(ARGS) > 1 else "still"
TEX = os.path.join(WORK, "tex")
scene = kit.reset()

M = props.materials(TEX)
M["words"] = kit.toon("words", props.C["paper"], grain="plane", decal=os.path.join(TEX, "words.png"), decal_uv=True)
M["plate"] = kit.toon("plate", props.C["brass"], grain=None, gloss=True, decal=os.path.join(TEX, "plate.png"), decal_uv=True)
M["floor"] = kit.toon("floor", ("#5E6F68", "#7C8F87", "#B9C7BC"), grain="plane")
M["inside"] = kit.flat("inside", "#3A2416")

# --- the room: a floor, a round table ------------------------------------------------------
slab("floor", M["floor"], (0, 0, -0.95), (200, 200, 0.1))  # big: the tall cut sees far past the table
lathe("table", M["desk"], (0, 0.3, -0.12),
      [(0, 0), (4.4, 0), (4.6, 0.04), (4.62, 0.09), (4.58, 0.12), (0, 0.12)], segs=160)
lathe("table_leg", M["wood"], (0, 0.3, -0.95), [(0, 0), (0.9, 0), (0.25, 0.15), (0.2, 0.83), (0, 0.83)])

# --- three seats: a mug and the same safety words on each card ------------------------------
SEATS = [  # (mug, card, card angle, mug colour): Maya, Ravi, you
    ((-3.25, 1.3), (-2.45, 0.55), 18, "terracotta"),
    ((3.35, 2.2), (1.75, 2.55), -14, "gold"),
    ((-2.75, -1.7), (-1.75, -1.35), -8, "verdigris"),
]
for k, ((mx, my), (cx, cy), rz, mat) in enumerate(SEATS):
    props.mug(M, mx, my, mat=M[mat], name=f"mug{k}", rz=-30 + 120 * k)
    slab(f"words{k}", M["words"], (cx, cy, 0.006), (1.1, 0.73, 0.012), rot=(0, 0, rz), uv_top=True)
LEAVES = props.plant(M, -3.6, 3.3)

# --- the strongbox (right of the card) --------------------------------------------------
BX, BY = (0.15, -2.15) if kit.TALL else (2.25, -0.45)  # the tall cut: below the card
W, D, H = 1.3, 0.85, 0.62
slab("box", M["wood"], (BX, BY, H / 2), (W, D, H), bevel=0.02)
slab("box_inside", M["inside"], (BX, BY, H - 0.004), (W - 0.12, D - 0.12, 0.01)).lineart.usage = "EXCLUDE"
for k, (sx, sy) in enumerate([(-1, -1), (1, -1), (-1, 1), (1, 1)]):  # brass corners
    slab(f"corner{k}", M["brass"], (BX + sx * (W / 2 - 0.05), BY + sy * (D / 2 - 0.05), H / 2), (0.14, 0.14, H + 0.02), bevel=0.01)
plate = slab("plate", M["plate"], (BX, BY - D / 2 - 0.012, H * 0.45), (1.0, 0.5, 0.02), rot=(90, 0, 0), uv_top=True)
for k in range(3):  # a few coins inside, seen while the lid is open
    lathe(f"box_coin{k}", M["gold"], (BX - 0.3 + 0.28 * k, BY + 0.05 * (k % 2), H + 0.002 + 0.02 * (k == 1)), props.COIN)
hinge = empty("lid_hinge", (BX, BY + D / 2, H))  # the lid turns about its back edge
lid = slab("lid", M["wood"], (0, -D / 2, 0.06), (W + 0.04, D + 0.04, 0.12), bevel=0.02)
lid.parent = hinge
slab("lid_band", M["brass"], (0, -D - 0.005, 0.06), (W * 0.4, 0.03, 0.13)).parent = hinge
kit.ellipse_shadow("box_contact", M["contact"], (BX, BY, 0), 0.85, 0.6)


def key(name, bow_mat):
    """A key along its local +y (the insert direction): a ring bow, a shaft, two bits."""
    k = empty(name)
    bow = lathe(f"{name}_bow", bow_mat, (0, -0.32, 0),
                     [(0.05, -0.025), (0.12, -0.025), (0.13, 0), (0.12, 0.025), (0.05, 0.025), (0.04, 0)], segs=48)
    bow.rotation_euler = (0, math.radians(90), 0)  # the bow's plane holds the key's axis
    bow.parent = k
    rod(f"{name}_shaft", M["brass"], (0, -0.2, 0), (0, 0.32, 0), 0.025, verts=16).parent = k
    slab(f"{name}_bit0", M["brass"], (0, 0.26, -0.05), (0.03, 0.06, 0.09)).parent = k
    slab(f"{name}_bit1", M["brass"], (0, 0.16, -0.04), (0.03, 0.05, 0.07)).parent = k
    return k


KEYS = [key("key_maya", M["terracotta"]), key("key_you", M["verdigris"]), key("key_ravi", M["gold"])]
HOLES = [Vector((BX + dx, BY - D / 2 - 0.03, H * 0.45 + 0.04)) for dx in (-0.25, 0.0, 0.25)]
# where each key lies before (on the table in front of the box, fanned)
LIE = [(BX - 0.75, BY - 1.25, 0.03, 35), (BX + 0.05, BY - 1.45, 0.03, -10), (BX + 0.8, BY - 1.2, 0.03, -40)]

props.window_light((-2.3, 0.7, 0.0), direction=(0.55, 0.35, -0.75), size=(2.4, 2.2))
kit.camera((0.0, -6.9, 6.4), (0.0, 0.35, 0.0), lens=36)

# --- motion ----------------------------------------------------------------------------------
READY = 62  # the app shows "Vault keys ready"
OPEN = -105.0
LID = [(0, (OPEN,), "inout"), (22, (OPEN,), "inout"), (44, (2.0,), "in"), (47, (-3.0,), "out"), (50, (0.0,), "inout"),
       (126, (0.0,), "inout"), (144, (OPEN,), "inout"), (kit.LOOP, (OPEN,), "linear")]
kit.key_every_frame(hinge, "rotation_euler", lambda f: (math.radians(kit.poses(LID, f)[0]), 0, 0))
for i, (k, hole, (lx, ly, lz, lrz)) in enumerate(zip(KEYS, HOLES, LIE)):
    d = i * 2  # a little stagger: three people, not a machine
    ready = (hole.x, hole.y - 0.5, hole.z)
    inside = (hole.x, hole.y - 0.2, hole.z)
    table = [  # (frame, (x, y, z, spin about its axis°, heading°), easing); lying flat = spin 90
        (0, (lx, ly, lz, 90.0, lrz), "inout"), (40 + d, (lx, ly, lz, 90.0, lrz), "inout"),
        (50 + d, (*ready, 0.0, 0.0), "inout"),                      # lift and face the lock
        (57 + d // 2, (*inside, 0.0, 0.0), "inout"),                # insert
        (READY - 1, (*inside, 0.0, 0.0), "linear"),
        (READY + 3, (*inside, 100.0, 0.0), "in"),                   # turn, together
        (READY + 6, (*inside, 90.0, 0.0), "out"),
        (118, (*inside, 90.0, 0.0), "inout"),
        (122, (*inside, 0.0, 0.0), "inout"),                        # turn back
        (128 + d, (*ready, 0.0, 0.0), "inout"),                     # out
        (140 + d, (lx, ly, lz, 90.0, lrz), "inout"),                # back on the table
        (kit.LOOP, (lx, ly, lz, 90.0, lrz), "linear"),
    ]

    def loc(f, table=table):
        return kit.poses(table, f)[:3]

    def rot(f, table=table):
        _, _, _, spin, heading = kit.poses(table, f)
        return (0, math.radians(spin), math.radians(heading))

    kit.key_every_frame(k, "location", loc)
    kit.key_every_frame(k, "rotation_euler", rot)
props.breeze(LEAVES)

kit.bake_scale()
kit.lines()

TAG = os.environ.get("ZAFE_TAG", "setup")
if MODE == "anim":
    kit.render(os.path.join(WORK, "out", TAG), anim=True)
else:
    kit.render(os.path.join(WORK, "out", f"{TAG}.png"), frame=int(ARGS[2]) if len(ARGS) > 2 else 0)
