# The shared world of the site's scenes: one room, its colour script, and the props every
# scene draws from (lamp, books, mug, slips under a magnifier, coins, an envelope with its
# wax, the stamp, the plant, the signers' board, clock, notes) and the window light.
# Each builder takes positions so scenes can rearrange the room; motion stays in the scenes.
import math
import os

import bpy
from mathutils import Vector

import kit
from kit import adopt, ball, blade, disc, ellipse_shadow, empty, lathe, rod, slab, toon

# Colour script: (shadow, mid, light) per material, picked by hand and checked in grey.
# The room sits in shade (mid values); only sunlit things and the window patch reach light.
C = {
    "wall": ("#7DA8A1", "#9DC5BD", "#E8F1E6"),
    "desk": ("#9F8160", "#BC9F78", "#F1E2C2"),
    "paper": ("#C9C8BA", "#E6E1D2", "#FFF8E8"),
    "envelope": ("#C2BFAE", "#DEDACA", "#FBF4E2"),
    "verdigris": ("#0B4C48", "#14716A", "#3FA196"),
    "brass": ("#8A6420", "#C1952F", "#F2CF73"),
    "gold": ("#9A6B16", "#D99F33", "#FFE08A"),
    "wood": ("#4A2C1B", "#7A4A2C", "#B07A4E"),
    "terracotta": ("#97492F", "#CD6C4B", "#F2A27C"),
    "leaf": ("#1D5444", "#2E7F63", "#6DB48C"),
    "cork": ("#8A6440", "#B48757", "#DDB383"),
    "glass": ("#86B3AC", "#BCDCD4", "#EEF8F2"),
    "soil": ("#33241A", "#4A3526", "#6A4C36"),
    "note": ("#B8892A", "#E8B940", "#FFE07A"),
    "book_a": ("#0B4C48", "#14716A", "#3FA196"),
    "book_b": ("#97492F", "#CD6C4B", "#F2A27C"),
    "book_c": ("#6D7F5E", "#93A47F", "#C4D2A8"),
    "coffee": ("#2A1A12", "#3B2418", "#5A3A26"),
}


def materials(tex_dir):
    def tex(name):
        return os.path.join(tex_dir, name + ".png")

    return {
        "wall": toon("wall", C["wall"], grain="plane"),
        "desk": toon("desk", C["desk"], grain="plane"),
        "paper": toon("paper", C["paper"], grain="plane"),
        "slip": toon("slip", C["paper"], grain="plane", decal=tex("slip"), decal_uv=True),
        "lens": toon("lens", C["glass"], grain=None, decal=tex("lens"), decal_uv=True),
        "envelope": toon("envelope", C["envelope"], grain="plane"),
        "verdigris": toon("verdigris", C["verdigris"], gloss=True, rim=True),
        "brass": toon("brass", C["brass"], gloss=True, rim=True),
        "gold": toon("gold", C["gold"], gloss=True, rim=True),
        "wood": toon("wood", C["wood"], gloss=True, rim=True),
        "terracotta": toon("terracotta", C["terracotta"], gloss=True, rim=True),
        "leaf": toon("leaf", C["leaf"]),
        "cork": toon("cork", C["cork"], grain="plane"),
        "soil": toon("soil", C["soil"], grain=None),
        "coffee": toon("coffee", C["coffee"], grain=None),
        "glass": toon("glass", C["glass"], grain=None, gloss=True),
        "clock": toon("clock", C["paper"], grain=None, decal=tex("clock"), decal_uv=True),
        "seal": toon("seal", C["verdigris"], grain=None, decal=tex("seal"), decal_uv=True),
        "maya": toon("maya", C["paper"], grain=None, decal=tex("maya"), decal_uv=True),
        "you": toon("you", C["paper"], grain=None, decal=tex("you"), decal_uv=True),
        "ravi": toon("ravi", C["paper"], grain=None, decal=tex("ravi"), decal_uv=True),
        "note0": toon("note0", C["note"], grain=None, decal=tex("note0"), decal_uv=True),
        "note1": toon("note1", C["note"], grain=None, decal=tex("note1"), decal_uv=True),
        "book_a": toon("book_a", C["book_a"]),
        "book_b": toon("book_b", C["book_b"]),
        "book_c": toon("book_c", C["book_c"]),
        "ink": kit.flat("ink", kit.INK),
        "bulb": kit.flat("bulb", "#FFF6E0"),
        "contact": kit.flat("contact", "#8E7556"),
    }


WALL_Y = 2.6


def room(M, desk_size=(16, 8), desk_y=-1.4):
    slab("desk", M["desk"], (0, desk_y, -0.2), (desk_size[0], desk_size[1], 0.4))
    slab("wall", M["wall"], (0, WALL_Y + 0.1, 4), (16, 0.2, 9))


def lamp(M, lx, ly, up=(-0.5, 0.3, 0.81), reach=(1.05, -0.3)):
    """A weighted base, two brass arms, a rolled-rim bell shade. Returns the bulb's place
    and the shade's axis (for a lamp that is on)."""
    lathe("lamp_base", M["verdigris"], (lx, ly, 0),
          [(0, 0), (0.42, 0), (0.44, 0.03), (0.4, 0.09), (0.2, 0.12), (0.08, 0.15), (0, 0.15)])
    ellipse_shadow("lamp_contact", M["contact"], (lx, ly, 0), 0.5, 0.42)
    elbow, head = Vector((lx + 0.18, ly - 0.05, 1.5)), Vector((lx + reach[0], ly + reach[1], 1.95))
    rod("lamp_arm1", M["brass"], (lx, ly, 0.13), elbow, 0.035)
    ball("lamp_joint", M["brass"], elbow, 0.075)
    rod("lamp_arm2", M["brass"], elbow, head, 0.035)
    up = Vector(up).normalized()  # the shade's axis; the opening faces the desk
    shade = lathe("lamp_shade", M["verdigris"], (0, 0, 0),
                  [(0.42, 0.0), (0.41, 0.03), (0.37, 0.07), (0.26, 0.18), (0.16, 0.32), (0.11, 0.42), (0.08, 0.45), (0, 0.46)],
                  caps=(False, True))
    shade.rotation_euler = up.to_track_quat("Z", "Y").to_euler()
    shade.location = head - up * 0.46
    bulb = Vector(shade.location + up * 0.14)
    ball("lamp_bulb", M["bulb"], tuple(bulb), 0.11)
    return bulb, up


def book(M, name, mat, loc, w, d, h, rz):
    """Two boards and a spine around a paper page block, the pages facing the camera."""
    g = empty(name, loc, (0, 0, math.radians(rz)))
    board = 0.025
    for k, z in enumerate((board / 2, h - board / 2)):
        slab(f"{name}_board{k}", mat, (0, 0, z), (w, d, board), bevel=0.008).parent = g
    slab(f"{name}_spine", mat, (0, d / 2 - 0.02, h / 2), (w, 0.04, h), bevel=0.008).parent = g
    slab(f"{name}_pages", M["paper"], (0, -0.015, h / 2), (w - 0.05, d - 0.06, h - 2 * board), bevel=0.004).parent = g
    return g


def books(M, x, y):
    book(M, "book0", M["book_a"], (x, y, 0), 1.45, 1.0, 0.17, 4)
    book(M, "book1", M["book_b"], (x + 0.03, y - 0.02, 0.17), 1.3, 0.92, 0.14, -5)
    book(M, "book2", M["book_c"], (x - 0.03, y + 0.01, 0.31), 1.2, 0.86, 0.13, 7)
    ellipse_shadow("books_contact", M["contact"], (x, y, 0), 0.82, 0.58)


def mug(M, x, y, mat=None, name="mug", rz=-12):
    mat = mat or M["terracotta"]
    lathe(name, mat, (x, y, 0),
          [(0, 0), (0.18, 0), (0.2, 0.02), (0.205, 0.38), (0.215, 0.41), (0.2, 0.42), (0.185, 0.4), (0.18, 0.05), (0, 0.05)],
          wobble=0.006, seed=2)
    disc(f"{name}_coffee", M["coffee"], (x, y, 0.33), 0.18)
    a = math.radians(rz)
    bpy.ops.mesh.primitive_torus_add(location=(x + 0.2 * math.cos(a), y + 0.2 * math.sin(a), 0.22),
                                     major_radius=0.1, minor_radius=0.028,
                                     rotation=(math.radians(90), 0, a))
    handle = bpy.context.object
    handle.name = f"{name}_handle"
    handle.data.materials.append(mat)
    ellipse_shadow(f"{name}_contact", M["contact"], (x, y, 0), 0.24, 0.2)


def slips_and_magnifier(M, sx, sy):
    """Payment slips; the magnifier lies over the top one's address and shows it enlarged."""
    for i, (dx, dy, rz) in enumerate([(0.12, 0.06, 15), (-0.05, 0.02, 4)]):
        slab(f"slip{i}", M["paper"], (sx + dx, sy + dy, 0.006 + i * 0.012), (1.5, 1.03, 0.01), rot=(0, 0, rz))
    top = 0.006 + 2 * 0.012
    slab("slip_top", M["slip"], (sx, sy, top), (1.5, 1.03, 0.01), rot=(0, 0, 9), uv_top=True)
    lens_c = Vector((sx + 0.05, sy - 0.05, top + 0.05))
    bpy.ops.mesh.primitive_torus_add(location=lens_c, major_radius=0.44, minor_radius=0.045,
                                     major_segments=72, minor_segments=16)
    ring = bpy.context.object
    ring.name = "loupe_ring"
    ring.data.materials.append(M["brass"])
    disc("loupe_lens", M["lens"], tuple(lens_c + Vector((0, 0, 0.005))), 0.42, rot=(0, 0, 9))
    hd = Vector((-0.62, -0.78, 0)).normalized()  # the handle points to the bottom-left corner
    axis = hd.to_track_quat("Z", "Y").to_euler()
    ferrule = lathe("loupe_ferrule", M["brass"], (0, 0, 0), [(0, 0), (0.05, 0), (0.06, 0.04), (0.06, 0.14), (0.05, 0.16), (0, 0.16)])
    ferrule.location, ferrule.rotation_euler = lens_c + hd * 0.47, axis
    grip = lathe("loupe_handle", M["wood"], (0, 0, 0),
                 [(0, 0), (0.05, 0), (0.07, 0.08), (0.075, 0.45), (0.085, 0.7), (0.07, 0.86), (0.04, 0.9), (0, 0.9)])
    grip.location, grip.rotation_euler = lens_c + hd * 0.63, axis


COIN = [(0, 0), (0.2, 0), (0.22, 0.008), (0.22, 0.042), (0.2, 0.05), (0.17, 0.05), (0.165, 0.044), (0, 0.044)]


def coins(M, cx, cy, stack=5, loose=((0.5, 0.18), (0.36, -0.34))):
    for i in range(stack):
        j = math.sin(i * 2.1) * 0.025
        c = lathe(f"coin{i}", M["gold"], (cx + j + i * 0.01, cy + j * 0.5, i * 0.052), COIN)
        c.rotation_euler = (math.radians(1.5 * math.sin(i)), math.radians(1.5 * math.cos(i)), 0)
    for i, (dx, dy) in enumerate(loose):
        lathe(f"loose{i}", M["gold"], (cx + dx, cy + dy, 0), COIN)
    ellipse_shadow("coins_contact", M["contact"], (cx, cy, 0), 0.27, 0.23)


def envelope(M, ex, ey, rz=-8, name="envelope", sealed=None):
    """An envelope with its flap; returns (wax blob, seal) at its centre. sealed=True shows
    only the pressed seal, False only the blob, None both (the scene animates them)."""
    g = empty(name, (ex, ey, 0), (0, 0, math.radians(rz)))
    slab(f"{name}_body", M["envelope"], (0, 0, 0.008), (1.1, 0.72, 0.016)).parent = g
    me = bpy.data.meshes.new(f"{name}_flap")
    me.from_pydata([(-0.55, 0.36, 0.0175), (0.55, 0.36, 0.0175), (0.0, -0.06, 0.0175)], [], [(0, 1, 2)])
    flap = bpy.data.objects.new(f"{name}_flap", me)
    bpy.context.collection.objects.link(flap)
    me.materials.append(M["envelope"])
    flap.parent = g
    blob = lathe(f"{name}_wax", M["verdigris"], (0, -0.02, 0.016),
                 [(0, 0), (0.15, 0), (0.16, 0.015), (0.13, 0.045), (0.08, 0.06), (0, 0.062)], wobble=0.1, seed=7)
    blob.parent = g
    seal = empty(f"{name}_seal", (0, -0.02, 0.016))
    seal.parent = g
    lathe(f"{name}_seal_body", M["verdigris"], (0, 0, 0),
          [(0, 0), (0.19, 0), (0.2, 0.01), (0.19, 0.026), (0, 0.026)], wobble=0.08, seed=9).parent = seal
    disc(f"{name}_seal_face", M["seal"], (0, 0, 0.027), 0.185).parent = seal
    if sealed is True:
        blob.hide_render = True
    elif sealed is False:
        seal.scale = (0.001,) * 3
    return g, blob, seal


def stamp(M, tx, ty):
    """The wax stamp at rest; returns (its empty, its contact shadow)."""
    s = empty("stamp", (tx, ty, 0))
    adopt(s,
          lathe("stamp_base", M["brass"], (tx, ty, 0),
                [(0, 0), (0.2, 0), (0.21, 0.02), (0.21, 0.09), (0.14, 0.13), (0.09, 0.2), (0, 0.2)]),
          lathe("stamp_handle", M["wood"], (tx, ty, 0),
                [(0, 0.19), (0.085, 0.19), (0.06, 0.32), (0.07, 0.45), (0.12, 0.58), (0.15, 0.68), (0.13, 0.76), (0.07, 0.8), (0, 0.81)]))
    return s, ellipse_shadow("stamp_contact", M["contact"], (tx, ty, 0), 0.25, 0.22)


def plant(M, px, py, z=0.0):
    """A flared terracotta pot with folded leaves on pivots; returns the leaves for a breeze."""
    lathe("pot", M["terracotta"], (px, py, z),
          [(0, 0), (0.3, 0), (0.33, 0.03), (0.4, 0.56), (0.47, 0.58), (0.48, 0.74), (0.43, 0.75), (0.41, 0.68), (0, 0.68)],
          wobble=0.01, seed=4)
    disc("soil", M["soil"], (px, py, z + 0.69), 0.4)
    ellipse_shadow("pot_contact", M["contact"], (px, py, z), 0.42, 0.36)
    leaves = []
    for i in range(11):
        ang = i * (360 / 11) + (i % 2) * 11
        tilt = 32 + (i % 3) * 13
        length = 0.55 + (i % 4) * 0.08
        pivot = empty(f"stem{i}", (px, py, z + 0.7))
        blade(f"leaf{i}", M["leaf"], length * 1.9, 0.11 + 0.02 * (i % 3), bend=-0.18 - 0.05 * (i % 3)).parent = pivot
        leaves.append((pivot, tilt, ang, length))
    return leaves


def breeze(leaves):
    """One breeze with two harmonics; longer leaves lag and move more. Seamless."""
    for pivot, tilt, ang, length in leaves:
        lag, amp = 3 + 10 * (length - 0.55), 1.4 + 6 * (length - 0.55)

        def rot(f, tilt=tilt, ang=ang, lag=lag, amp=amp):
            p = 2 * math.pi * (f - lag) / kit.LOOP
            b = math.sin(p) + 0.35 * math.sin(2 * p + 1.1)
            return (math.radians(tilt + amp * b), 0, math.radians(ang + 0.6 * amp * b))

        kit.key_every_frame(pivot, "rotation_euler", rot)


def board(M, bx, bz):
    """The three signers pinned on a cork board, joined by a string: one vault."""
    wy = WALL_Y - 0.01
    slab("board_frame", M["wood"], (bx, wy, bz), (2.3, 0.06, 1.45), bevel=0.02)
    slab("board", M["cork"], (bx, wy - 0.03, bz), (2.15, 0.02, 1.3))
    pins = []
    for dx, who, rot, dz in [(-0.68, "maya", -4, 0.0), (0.0, "you", 2, 0.06), (0.68, "ravi", 5, -0.03)]:
        x, z = bx + dx, bz - 0.05 + dz
        slab(f"portrait_{who}", M[who], (x, wy - 0.05, z), (0.52, 0.65, 0.012), rot=(90, rot, 0), uv_top=True)
        pin = Vector((x, wy - 0.07, z + 0.27))
        ball(f"pin_{who}", M["terracotta"], pin, 0.04)
        pins.append(pin)
    for k in range(2):
        rod(f"string{k}", M["ink"], pins[k], pins[k + 1], 0.007, verts=8)


def clock(M, x, z):
    wy = WALL_Y - 0.01
    rim = lathe("clock_rim", M["verdigris"], (x, wy, z),
                [(0, 0), (0.42, 0), (0.44, 0.03), (0.42, 0.08), (0.36, 0.08), (0, 0.08)])
    rim.rotation_euler = (math.radians(90), 0, 0)
    disc("clock_face", M["clock"], (x, wy - 0.085, z), 0.36, rot=(90, 0, 0))
    for nm, length, ang, w in (("clock_hour", 0.2, 300, 0.035), ("clock_min", 0.3, 50, 0.025)):
        h = slab(nm, M["ink"], (0, 0, 0), (w, 0.01, length))
        h.rotation_euler = (0, math.radians(ang), 0)
        h.location = (x + math.sin(math.radians(ang)) * length / 2, wy - 0.095, z + math.cos(math.radians(ang)) * length / 2)


def notes(M, places):
    wy = WALL_Y - 0.01
    for k, (x, z, r) in enumerate(places):
        slab(f"note{k}", M[f"note{k % 2}"], (x, wy - 0.07, z), (0.4, 0.4, 0.01), rot=(90, r, 0), uv_top=True)


def window_light(target, direction=(0.5, 0.66, -0.56), size=(2.1, 2.3), key=6.0, fill=1.1, drift=True):
    """The room in shade; a key sun through a window opening (out of shot) lands on `target`;
    a cool shadowless fill keeps shaded forms in two tones. The window drifts slowly."""
    d = Vector(direction).normalized()
    kit.sun(d, key)
    kit.sun((-0.45, 0.85, -0.35), fill, color=(0.82, 0.92, 1.0), shadow=False)
    window = empty("window", tuple(Vector(target) - d * 7), d.to_track_quat("Z", "Y").to_euler())
    w, h = size
    parts = [((-(w / 2 + 6), 0), (12, 30)), (((w / 2 + 6), 0), (12, 30)), ((0, h / 2 + 6), (w, 12)),
             ((0, -(h / 2 + 6)), (w, 12)), ((0, 0), (0.09, h)), ((0, 0), (w, 0.09))]
    for k, ((x, y), (pw, ph)) in enumerate(parts):
        o = slab(f"occluder{k}", None, (x, y, 0), (pw, ph, 0.05))
        o.parent = window
        o.visible_camera = False
        o.lineart.usage = "EXCLUDE"  # Line Art ignores ray visibility: it would hide every line
    if drift:  # a slow continuous drift, never easing to a stop
        w0 = Vector(window.location)
        kit.key_every_frame(window, "location", lambda f: tuple(
            w0 + Vector((0.1 * math.sin(2 * math.pi * f / kit.LOOP), 0, 0.04 * math.sin(2 * math.pi * f / kit.LOOP + 1.3)))))
    return window
