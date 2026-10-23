# "Made to live with": the background for the vault's home screen (film/src/LoopCard.tsx).
# The same room at dusk: the lamp is the light, the window shows the evening, binders for
# the books on a shelf. Nothing is happening, which is the point: a cat sleeps by the card,
# steam leaves the mug, a few stars come and go.
#   blender --background --factory-startup --python live.py -- <workdir> still <frame> | anim
import math
import os
import sys

import bpy

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import kit  # noqa: E402
import props  # noqa: E402
from kit import ball, disc, empty, lathe, rod, slab  # noqa: E402
from mathutils import Euler, Vector  # noqa: E402

ARGS = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
WORK = ARGS[0] if ARGS else os.getcwd()
MODE = ARGS[1] if len(ARGS) > 1 else "still"
TEX = os.path.join(WORK, "tex")
scene = kit.reset(ambient="#2B3E4A", strength=0.12)

# Dusk: the room's big planes drop a step; what the lamp reaches turns warm.
props.C["wall"] = ("#2E4B49", "#3E615D", "#D8B47C")
props.C["desk"] = ("#4E3D2C", "#6C5640", "#E7C48C")
M = props.materials(TEX)
M["sky"] = kit.flat("sky", "#20324A")
M["moon"] = kit.flat("moon", "#F4E9C8")
M["star"] = kit.flat("star", "#E9E2C6")
M["steam"] = kit.flat("steam", "#C9D3CC")
# a ginger cat: legible against the dusk desk (a black one read as one dark mass)
M["cat"] = kit.toon("cat", ("#A9542A", "#DB8344", "#F4B57A"), grain=None, rim=(-0.6, 0.2, 0.75),
                    stripes=(2.1, "X", 0.45), line=0.8)
M["cat_face"] = kit.toon("cat_face", ("#A9542A", "#DB8344", "#F4B57A"), grain=None, rim=(-0.6, 0.2, 0.75), line=0.8)
M["cat_cream"] = kit.toon("cat_cream", ("#C9B08E", "#EFDDBF", "#FFF4E2"), grain=None, line=0.7)
M["cat_ear"] = kit.toon("cat_ear", ("#8A4D52", "#C27A7E", "#E6A7A6"), grain=None)
M["cat_line"] = kit.flat("cat_line", "#4A2716")
M["cat_whisker"] = kit.flat("cat_whisker", "#F3E6D0")
M["cat_nose"] = kit.flat("cat_nose", "#B45A60")
M["cat_toe"] = kit.flat("cat_toe", "#B59C7C")
M["binder_a"] = kit.toon("binder_a", props.C["book_a"])
M["binder_b"] = kit.toon("binder_b", props.C["book_c"])
M["binder_c"] = kit.toon("binder_c", props.C["book_b"])
M["label"] = kit.toon("label", props.C["paper"], grain=None)
props.room(M)
bulb, up = props.lamp(M, -3.1, 1.35, up=(-0.3, 0.22, 0.93), reach=(1.15, -0.35))
props.books(M, -2.0, 1.85)
props.board(M, 2.1, 3.05)
LEAVES = props.plant(M, 3.75, 1.8)

# --- the window: the evening outside, a moon, a few stars --------------------------------
WY = props.WALL_Y - 0.01
wx, wz, ww, wh = -2.35, 2.75, 1.7, 1.7
slab("sky", M["sky"], (wx, WY - 0.01, wz), (ww, 0.01, wh)).lineart.usage = "EXCLUDE"
for k, (dx, dz, sx, sz) in enumerate([(0, wh / 2, ww + 0.16, 0.1), (0, -wh / 2, ww + 0.24, 0.12),
                                     (-ww / 2, 0, 0.1, wh + 0.1), (ww / 2, 0, 0.1, wh + 0.1),
                                     (0, 0, 0.06, wh), (0, 0, ww, 0.06)]):
    slab(f"frame{k}", M["paper"], (wx + dx, WY - 0.05, wz + dz), (sx, 0.08, sz), bevel=0.01)
disc("moon", M["moon"], (wx + 0.42, WY - 0.02, wz + 0.42), 0.16, rot=(90, 0, 0)).lineart.usage = "EXCLUDE"
STARS = []
for k, (dx, dz) in enumerate([(-0.55, 0.55), (-0.25, 0.2), (0.3, -0.35), (-0.5, -0.45), (0.55, -0.1)]):
    st = disc(f"star{k}", M["star"], (wx + dx, WY - 0.02, wz + dz), 0.022, rot=(90, 0, 0), verts=8)
    st.lineart.usage = "EXCLUDE"
    STARS.append(st)

# a shooting star, once a loop: a bright head and a tapered tail between the sky and the bars
meteor = empty("meteor", (wx, WY - 0.02, wz), (0, math.radians(147), 0))  # heading down-left
me = bpy.data.meshes.new("meteor_tail")
me.from_pydata([(0, 0, 0.018), (0, 0, -0.018), (-0.55, 0, 0)], [], [(0, 1, 2)])
streak = bpy.data.objects.new("meteor_tail", me)
bpy.context.collection.objects.link(streak)
me.materials.append(M["star"])
streak.lineart.usage = "EXCLUDE"
streak.parent = meteor
spark = disc("meteor_head", M["moon"], (0, 0, 0), 0.032, rot=(90, 0, 0), verts=12)
spark.lineart.usage = "EXCLUDE"
spark.parent = meteor

# --- a shelf of binders: the books, exported --------------------------------------------
sx_, sz_ = 2.1, 1.12
slab("shelf", M["wood"], (sx_, WY - 0.2, sz_), (2.4, 0.4, 0.06), bevel=0.01)
for k, x in enumerate((-0.95, 0.95)):
    slab(f"shelf_bracket{k}", M["wood"], (sx_ + x, WY - 0.1, sz_ - 0.15), (0.05, 0.2, 0.25))
for k in range(6):
    mat = M[("binder_a", "binder_b", "binder_c")[k % 3]]
    lean = -6 if k == 5 else 0
    b = slab(f"binder{k}", mat, (sx_ - 0.9 + k * 0.24 + (0.04 if k == 5 else 0), WY - 0.2, sz_ + 0.03 + 0.34),
             (0.2, 0.34, 0.68), rot=(0, lean, 0), bevel=0.01)
    slab(f"binder_label{k}", M["label"], (b.location.x, WY - 0.375, sz_ + 0.03 + 0.42), (0.13, 0.01, 0.18), rot=(0, lean, 0))

# --- the mug, steaming (left of the card) -----------------------------------------------
MX, MY = -1.35, 0.25
props.mug(M, MX, MY, mat=M["terracotta"], rz=-160)


def ribbon(name):
    """A wisp of steam: a thin S-curved strip, standing up, facing the camera."""
    me = bpy.data.meshes.new(name)
    verts, faces = [], []
    n = 16
    for i in range(n + 1):
        t = i / n
        x = 0.05 * math.sin(t * math.pi * 1.6)
        w = 0.022 * math.sin(math.pi * t) ** 0.6 + 0.002
        verts += [(x - w, 0, t * 0.55), (x + w, 0, t * 0.55)]
        if i:
            faces.append((2 * i - 2, 2 * i - 1, 2 * i + 1, 2 * i))
    me.from_pydata(verts, [], faces)
    o = bpy.data.objects.new(name, me)
    bpy.context.collection.objects.link(o)
    me.materials.append(M["steam"])
    o.lineart.usage = "EXCLUDE"
    return o


WISPS = [ribbon(f"steam{k}") for k in range(3)]

# --- the cat, asleep (right of the card), curled up facing us ------------------------------
# Modelled from blended forms (metaballs, then a mesh): a big haunch, the ribcage curving to
# the shoulder, a head wider than tall with full cheeks resting on its paws; the tail wraps
# round the front and tapers to a soft tip.
CX, CY = (0.45, -2.15) if kit.TALL else (1.98, 0.05)  # the tall cut: on the desk below the card
cat = empty("cat", (CX, CY, 0))


def blobs(name, mat, parts, parent=None, origin=(0, 0, 0)):
    """One organic mesh from blended ellipsoids: parts = [(centre, radius, (sx, sy, sz))],
    optionally with a fourth item, a turn about z in degrees."""
    mb = bpy.data.metaballs.new(name)
    mb.resolution, mb.render_resolution, mb.threshold = 0.02, 0.012, 0.6
    for co, r, size, *turn in parts:
        el = mb.elements.new(type="ELLIPSOID")
        el.co, el.radius = Vector(co) - Vector(origin), r
        el.size_x, el.size_y, el.size_z = size
        if turn:
            el.rotation = Euler((0, 0, math.radians(turn[0]))).to_quaternion()
        el.stiffness = 2.0
    o = bpy.data.objects.new(name, mb)
    bpy.context.collection.objects.link(o)
    o.location = origin
    bpy.context.view_layer.objects.active = o
    for other in bpy.context.selected_objects:
        other.select_set(False)
    o.select_set(True)
    bpy.ops.object.convert(target="MESH")
    o = bpy.context.view_layer.objects.active
    o.name = name
    o.data.materials.clear()
    o.data.materials.append(mat)
    for poly in o.data.polygons:
        poly.use_smooth = True
    if parent is not None:
        o.parent = parent
    return o


body = blobs("cat_body", M["cat"], [
    ((0.36, 0.16, 0.27), 0.58, (1.0, 0.95, 0.8)),     # the haunch, the biggest mass
    ((0.04, 0.04, 0.25), 0.54, (1.12, 0.9, 0.74)),    # the ribcage
    ((-0.22, -0.14, 0.23), 0.42, (1.0, 1.0, 0.85)),   # the shoulder, turning toward us
    ((-0.32, -0.42, 0.12), 0.34, (1.15, 1.0, 0.55)),  # the chest, flowing under the chin
], parent=cat, origin=(0.1, 0.05, 0.0))
# only the paws show: small, side by side and turned in a little, under the muzzle
# (two meshes, so they touch without melting into one shape)
blobs("cat_paw0", M["cat_cream"], [((-0.41, -0.655, 0.052), 0.125, (1.0, 1.2, 0.6), 14)], parent=cat)
blobs("cat_paw1", M["cat_cream"], [((-0.235, -0.675, 0.052), 0.125, (1.0, 1.2, 0.6), -14)], parent=cat)
HEAD = (-0.32, -0.4, 0.25)
head = empty("cat_head", HEAD)
head.parent = cat
blobs("cat_skull", M["cat_face"], [
    ((0, 0, 0), 0.3, (1.15, 0.92, 0.88)),             # the cranium, wider than tall
    ((-0.12, -0.08, -0.07), 0.16, (1.0, 0.9, 0.85)),  # cheeks
    ((0.12, -0.08, -0.07), 0.16, (1.0, 0.9, 0.85)),
], parent=head)
blobs("cat_cream", M["cat_cream"], [
    ((0, -0.17, -0.085), 0.13, (1.25, 0.75, 0.7)),    # the muzzle
], parent=head)
nose = ball("cat_nose", M["cat_nose"], (0, -0.243, -0.03), 1.0)  # small and flat, under the eyes
nose.scale = (0.024, 0.012, 0.014)
nose.parent = head
for k, px_ in enumerate((-0.41, -0.235)):  # toe lines on each paw
    for t in (-0.022, 0.022):
        toe = rod(f"cat_toe{k}_{t}", M["cat_toe"], (px_ + t, -0.755, 0.075), (px_ + t, -0.772, 0.05), 0.0045, verts=6)
        toe.lineart.usage = "EXCLUDE"
        toe.parent = cat
for k, side in enumerate((-1, 1)):
    # closed, sleeping eyes: a soft curve, a little lower at the outer corner
    pts = [Vector((side * 0.17, -0.19, 0.01)), Vector((side * 0.12, -0.215, 0.022)), Vector((side * 0.065, -0.215, 0.018))]
    for j in range(2):
        rod(f"cat_eye{k}_{j}", M["cat_line"], pts[j], pts[j + 1], 0.009, verts=8).parent = head
    for j, dz in enumerate((0.0, -0.025)):  # whiskers
        w = rod(f"cat_whisker{k}_{j}", M["cat_whisker"], (side * 0.1, -0.23, -0.07 + dz),
                (side * 0.32, -0.2, -0.05 + dz * 2.2), 0.0035, verts=6)
        w.lineart.usage = "EXCLUDE"
        w.parent = head
EARS = []
EAR = [(0, 0), (0.11, 0), (0.095, 0.05), (0.06, 0.12), (0.025, 0.165), (0.008, 0.18), (0, 0.182)]
for k, side in enumerate((-1, 1)):
    # asleep, the ears relax: sunk into the skull, laid back and a little out
    ear = lathe(f"cat_ear{k}", M["cat_face"], (side * 0.15, 0.03, 0.045), EAR, segs=24)
    ear.scale = (1.0, 0.42, 0.9)  # flat, like a real ear
    ear.rotation_euler = (math.radians(24), math.radians(side * 34), 0)
    ear.parent = head
    inner = lathe(f"cat_ear_in{k}", M["cat_ear"], (side * 0.15, 0.0, 0.07), [(0, 0), (0.065, 0), (0.03, 0.09), (0, 0.11)], segs=20)
    inner.scale = (1.0, 0.3, 1.0)
    inner.rotation_euler = ear.rotation_euler
    inner.parent = head
    EARS.append(ear)
# the tail: wraps round the front from the haunch and tapers to a soft tip; it plays
TAIL = [(0.72, 0.18, 0.1, 1.0), (0.86, -0.12, 0.08, 0.95), (0.7, -0.42, 0.07, 0.88),
        (0.42, -0.56, 0.07, 0.78), (0.15, -0.6, 0.08, 0.64), (-0.03, -0.58, 0.1, 0.45), (-0.1, -0.56, 0.11, 0.08)]
tail_curve = bpy.data.curves.new("cat_tail", "CURVE")
tail_curve.dimensions = "3D"
tail_curve.bevel_depth, tail_curve.bevel_resolution, tail_curve.resolution_u = 0.075, 6, 16
tail_curve.use_fill_caps = True
spline = tail_curve.splines.new("NURBS")
spline.points.add(len(TAIL) - 1)
for pt, (x, y, z, r) in zip(spline.points, TAIL):
    pt.co, pt.radius = (x, y, z, 1.0), r
spline.order_u, spline.use_endpoint_u = 4, True
tail = bpy.data.objects.new("cat_tail", tail_curve)
bpy.context.collection.objects.link(tail)
tail_curve.materials.append(M["cat"])
tail.parent = cat

# --- light: the lamp, warm; a cool evening fill -------------------------------------------
bpy.data.objects["lamp_bulb"].visible_shadow = False  # the light sits inside it
bpy.ops.object.light_add(type="SPOT", location=tuple(bulb))
spot = bpy.context.object
spot.rotation_euler = (-up).to_track_quat("-Z", "Y").to_euler()
spot.data.energy, spot.data.color = 1500, (1.0, 0.84, 0.6)
spot.data.spot_size, spot.data.spot_blend = math.radians(72), 0.3
spot.data.shadow_soft_size = 0.0
kit.sun((-0.45, 0.85, -0.35), 0.7, color=(0.62, 0.74, 1.0), shadow=False)
kit.camera((0.0, -8.6, 3.3), (0.0, 0.6, 1.2), lens=40)

# --- motion: breathing, steam, stars --------------------------------------------------------
L = kit.LOOP


def ph(f, n=1, off=0.0):
    return 2 * math.pi * n * f / L + off


# two breaths per loop: the body swells a little, the head rides it
kit.key_every_frame(body, "scale", lambda f: (1 + 0.01 * math.sin(ph(f, 2)), 1 + 0.025 * math.sin(ph(f, 2)),
                                               1 + 0.04 * math.sin(ph(f, 2))))
kit.key_every_frame(head, "location", lambda f: (HEAD[0], HEAD[1], HEAD[2] + 0.008 * math.sin(ph(f, 2) - 0.4)))
# one ear flick, late in the loop
kit.key_every_frame(EARS[1], "rotation_euler", lambda f: (math.radians(24 - 18 * math.exp(-((f - 92) / 3.0) ** 2)),
                                                           math.radians(34), 0))
# the tail sweeps lazily along the desk: a wave runs from the base to the tip, so the tip
# lags and swings widest; late in the loop the tip twitches once
n = len(TAIL)
perp = []
for i in range(n):
    a_, b_ = TAIL[max(i - 1, 0)], TAIL[min(i + 1, n - 1)]
    d = Vector((b_[0] - a_[0], b_[1] - a_[1], 0)).normalized()
    perp.append(Vector((-d.y, d.x, 0)))
for f in range(L):
    for i, pt in enumerate(spline.points):
        x, y, z, r = TAIL[i]
        u = i / (n - 1)
        w = u ** 1.4  # the base stays, the tip moves most
        sweep = 0.13 * w * math.sin(ph(f) - 2.2 * u)
        twitch = 0.045 * u ** 3 * math.sin(ph(f, 6)) * math.exp(-((f - 100) / 7.0) ** 2)
        off = perp[i] * (sweep + twitch)
        pt.co = (x + off.x, y + off.y, z + 0.03 * u ** 2 * (0.5 + 0.5 * math.sin(ph(f) - 2.2 * u - 0.8)), 1.0)
        pt.keyframe_insert("co", frame=f + 1)
# steam: three wisps rise, sway and shrink away, a third of a loop apart
for k, w in enumerate(WISPS):
    def wisp_loc(f, k=k):
        t = ((f / L) * 2 + k / 3) % 1.0
        return (MX - 0.02 + 0.06 * math.sin(ph(f, 2, k)), MY - 0.05, 0.42 + t * 0.55)

    def wisp_scale(f, k=k):
        t = ((f / L) * 2 + k / 3) % 1.0
        s = math.sin(math.pi * t) ** 0.8
        return (max(s, 0.001), 1, max(0.6 + 0.4 * s, 0.001))

    kit.key_every_frame(w, "location", wisp_loc)
    kit.key_every_frame(w, "scale", wisp_scale)
for k, st in enumerate(STARS):  # stars twinkle out of step
    kit.key_every_frame(st, "scale", lambda f, k=k: (0.6 + 0.4 * (0.5 + 0.5 * math.sin(ph(f, 1 + k % 2, k * 1.7))),) * 3)
# the shooting star crosses both top panes, right to left above the moon, in about a second
M0, M1 = Vector((wx + 0.3, WY - 0.02, wz + 0.74)), Vector((wx - 0.68, WY - 0.02, wz + 0.1))
S0, S1 = 26, 58


def meteor_loc(f):
    t = kit.ease((f - S0) / (S1 - S0), "linear") if S0 <= f <= S1 else (0.0 if f < S0 else 1.0)
    return tuple(M0.lerp(M1, t))


def meteor_scale(f):
    if not S0 <= f <= S1:
        return (0.001,) * 3
    t = (f - S0) / (S1 - S0)
    grow = min(1.0, t / 0.35) * min(1.0, (1 - t) / 0.3)  # the tail stretches out, then fades
    return (max(grow, 0.001), 1, max(0.4 + 0.6 * grow, 0.001))


kit.key_every_frame(meteor, "location", meteor_loc)
kit.key_every_frame(meteor, "scale", meteor_scale)
props.breeze(LEAVES)

kit.bake_scale()
kit.lines()

TAG = os.environ.get("ZAFE_TAG", "live")
if MODE == "anim":
    kit.render(os.path.join(WORK, "out", TAG), anim=True)
else:
    kit.render(os.path.join(WORK, "out", f"{TAG}.png"), frame=int(ARGS[2]) if len(ARGS) > 2 else 0)
