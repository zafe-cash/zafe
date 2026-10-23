# The illustration kit every site scene is built from: Blender renders an authored scene as
# an editorial drawing. The reasons for each choice: docs/site.md §6.
# - Colour: each material is a hand-picked triplet (shadow, mid, light), not a formula.
#   Shadows lean cool, lights warm; the light band only comes from the key sun.
# - Shading: Diffuse -> Shader to RGB -> 3 constant bands, plus designed gloss and rim shapes
#   on chosen props. Texture (pigment mottle, dry-brush band edges) is in object space so it
#   moves with things instead of swimming.
# - Lines: Line Art (Grease Pencil), heavy silhouettes and light inner lines, tapered, a
#   fixed-seed wobble (no boil), each group tinted from its own fill.
# - Light: a key sun through a window opening (the room is in shade) and a cool fill.
# - Motion: every animated channel is keyed on every frame from a periodic function or an
#   eased pose table, so loops are seamless by construction.
import math
import os
import random

import bmesh
import bpy
from mathutils import Matrix, Vector

FPS = 30
LOOP = 126  # the request card's loop (film/src/Root.tsx); scenes with other cards set their own
INK = "#1E2B2A"
WARM = "#FFF2D8"  # the key light's tint on light bands and highlights
T_MID, T_LIGHT = 0.1, 0.8  # luminance thresholds between the three bands
# ZAFE_TALL=1: the phone cut (9:16). The same world through a longer lens: the card fills
# more of a narrow frame, so the scene keeps about the same room around it.
TALL = os.environ.get("ZAFE_TALL") == "1"
TALL_ZOOM = float(os.environ.get("ZAFE_TALL_ZOOM", "1.9"))


def srgb(hexstr):
    h = hexstr.lstrip("#")
    c = [int(h[i:i + 2], 16) / 255 for i in (0, 2, 4)]
    return (*[x / 12.92 if x <= 0.04045 else ((x + 0.055) / 1.055) ** 2.4 for x in c], 1.0)


def mix(c, d, t):
    return tuple(c[i] * (1 - t) + d[i] * t for i in range(3)) + (1.0,)


def reset(width=1600, height=900, ambient="#7FA6A0", strength=0.1):
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene
    if TALL:
        width, height = 720, 1280
    scene.render.engine = "BLENDER_EEVEE"
    scene.render.resolution_x, scene.render.resolution_y = width, height
    scene.render.fps = FPS
    scene.eevee.taa_render_samples = 16  # flat tones converge at once
    scene.view_settings.view_transform = "Standard"
    scene.view_settings.look = "None"
    world = bpy.data.worlds.new("World")
    scene.world = world
    world.use_nodes = True
    bg = world.node_tree.nodes["Background"]
    bg.inputs["Color"].default_value = srgb(ambient)
    bg.inputs["Strength"].default_value = strength
    return scene


# --- node helpers --------------------------------------------------------------------------

def _math(nt, op, a, b=None):
    n = nt.nodes.new("ShaderNodeMath")
    n.operation = op
    for sock, v in zip(n.inputs, (a, b)):
        if v is None:
            continue
        if isinstance(v, (int, float)):
            sock.default_value = v
        else:
            nt.links.new(v, sock)
    return n.outputs[0]


def _mix(nt, fac, a, b):
    n = nt.nodes.new("ShaderNodeMix")
    n.data_type = "RGBA"
    for sock, v in ((n.inputs["Factor"], fac), (n.inputs["A"], a), (n.inputs["B"], b)):
        if isinstance(v, (int, float)):
            sock.default_value = v
        elif isinstance(v, tuple):
            sock.default_value = v
        else:
            nt.links.new(v, sock)
    return n.outputs["Result"]


def _noise(nt, coord, scale, detail, rough, stretch=None):
    n = nt.nodes.new("ShaderNodeTexNoise")
    n.inputs["Scale"].default_value, n.inputs["Detail"].default_value = scale, detail
    n.inputs["Roughness"].default_value = rough
    if stretch:
        v = nt.nodes.new("ShaderNodeVectorMath")
        v.operation = "MULTIPLY"
        v.inputs[1].default_value = stretch
        nt.links.new(coord, v.inputs[0])
        coord = v.outputs[0]
    nt.links.new(coord, n.inputs["Vector"])
    return _math(nt, "SUBTRACT", n.outputs["Fac"], 0.5)


# --- materials -----------------------------------------------------------------------------

LINE_TINT = {}  # material name -> its line colour (a dark shade of the fill)


def toon(name, triplet, grain="prop", gloss=False, rim=False, decal=None, decal_uv=None, stripes=None, line=0.55):
    """Three flat tones from a hand-picked (shadow, mid, light) triplet.

    grain: "prop" (mottle + dry-brush band edges), "plane" (mottle only: band-edge jitter on
    big planes reads as grunge), None. decal: an RGBA image painted on top (UV mapped),
    shaded with the surface."""
    shadow, mid, light = (srgb(c) for c in triplet)
    LINE_TINT[name] = mix(shadow, srgb(INK), line)
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    nt = m.node_tree
    nt.nodes.clear()
    out = nt.nodes.new("ShaderNodeOutputMaterial")
    em = nt.nodes.new("ShaderNodeEmission")
    nt.links.new(em.outputs["Emission"], out.inputs["Surface"])
    diffuse = nt.nodes.new("ShaderNodeBsdfDiffuse")
    diffuse.inputs["Color"].default_value = (1, 1, 1, 1)
    to_rgb = nt.nodes.new("ShaderNodeShaderToRGB")
    nt.links.new(diffuse.outputs["BSDF"], to_rgb.inputs["Shader"])
    bw = nt.nodes.new("ShaderNodeRGBToBW")
    nt.links.new(to_rgb.outputs["Color"], bw.inputs["Color"])
    lum = bw.outputs["Val"]
    tc = nt.nodes.new("ShaderNodeTexCoord")
    mottle = None
    if grain:
        mottle = _noise(nt, tc.outputs["Object"], 1.6, 3, 0.6)
    if grain == "prop":
        brush = _noise(nt, tc.outputs["Object"], 9, 4, 0.7, stretch=(1.0, 0.2, 1.0))
        lum = _math(nt, "ADD", lum, _math(nt, "ADD", _math(nt, "MULTIPLY", mottle, 0.16),
                                          _math(nt, "MULTIPLY", brush, 0.2)))
    ramp = nt.nodes.new("ShaderNodeValToRGB")
    ramp.color_ramp.interpolation = "CONSTANT"
    els = ramp.color_ramp.elements
    els[0].position, els[0].color = 0.0, shadow
    els[1].position, els[1].color = T_LIGHT, light
    els.new(T_MID).color = mid
    nt.links.new(lum, ramp.inputs["Fac"])
    col = ramp.outputs["Color"]
    if mottle is not None:  # pigment pooling: a few % of value inside each flat tone
        hsv = nt.nodes.new("ShaderNodeHueSaturation")
        nt.links.new(col, hsv.inputs["Color"])
        nt.links.new(_math(nt, "ADD", _math(nt, "MULTIPLY", mottle, 0.1), 1.0), hsv.inputs["Value"])
        col = hsv.outputs["Color"]
    if stripes:  # tabby markings: wavy bands in object space, a step darker than the fur
        scale, axis, strength = stripes
        wave = nt.nodes.new("ShaderNodeTexWave")
        wave.wave_type, wave.bands_direction = "BANDS", axis
        wave.inputs["Scale"].default_value, wave.inputs["Distortion"].default_value = scale, 5.0
        wave.inputs["Detail"].default_value, wave.inputs["Detail Scale"].default_value = 2.0, 1.5
        nt.links.new(tc.outputs["Object"], wave.inputs["Vector"])
        band = _math(nt, "GREATER_THAN", wave.outputs["Fac"], 0.72)
        col = _mix(nt, _math(nt, "MULTIPLY", band, strength), col, mix(shadow, srgb(INK), 0.12))
    if decal:
        img = nt.nodes.new("ShaderNodeTexImage")
        img.image = bpy.data.images.load(decal, check_existing=True)
        img.extension = "CLIP"
        if decal_uv:
            nt.links.new(tc.outputs["UV"], img.inputs["Vector"])
        # shade the decal with the surface: scale its colour by (tone / mid)
        ratio = nt.nodes.new("ShaderNodeVectorMath")
        ratio.operation = "DIVIDE"
        nt.links.new(col, ratio.inputs[0])
        ratio.inputs[1].default_value = mid[:3]
        shaded = nt.nodes.new("ShaderNodeVectorMath")
        shaded.operation = "MULTIPLY"
        nt.links.new(img.outputs["Color"], shaded.inputs[0])
        nt.links.new(ratio.outputs[0], shaded.inputs[1])
        col = _mix(nt, img.outputs["Alpha"], col, shaded.outputs[0])
    if gloss:  # a highlight shape: a glossy lobe from the key, thresholded
        g = nt.nodes.new("ShaderNodeBsdfGlossy")
        g.inputs["Roughness"].default_value = 0.2
        g_rgb = nt.nodes.new("ShaderNodeShaderToRGB")
        nt.links.new(g.outputs["BSDF"], g_rgb.inputs["Shader"])
        g_bw = nt.nodes.new("ShaderNodeRGBToBW")
        nt.links.new(g_rgb.outputs["Color"], g_bw.inputs["Color"])
        col = _mix(nt, _math(nt, "MULTIPLY", _math(nt, "GREATER_THAN", g_bw.outputs["Val"], 1.6), 0.85),
                   col, srgb(WARM))
    if rim:  # a thin warm rim on the silhouette, on the window's side
        lw = nt.nodes.new("ShaderNodeLayerWeight")
        lw.inputs["Blend"].default_value = 0.3
        geo = nt.nodes.new("ShaderNodeNewGeometry")
        dp = nt.nodes.new("ShaderNodeVectorMath")
        dp.operation = "DOT_PRODUCT"
        nt.links.new(geo.outputs["Normal"], dp.inputs[0])
        dp.inputs[1].default_value = rim if isinstance(rim, tuple) else (-0.75, 0.2, 0.62)
        r = _math(nt, "MULTIPLY", _math(nt, "GREATER_THAN", lw.outputs["Facing"], 0.74),
                  _math(nt, "GREATER_THAN", dp.outputs["Value"], 0.2))
        col = _mix(nt, _math(nt, "MULTIPLY", r, 0.75), col, mix(light, srgb(WARM), 0.4))
    nt.links.new(col, em.inputs["Color"])
    return m


def flat(name, hexcolor):
    """One unlit tone (contact shadows, the lamp bulb)."""
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    nt = m.node_tree
    nt.nodes.clear()
    out = nt.nodes.new("ShaderNodeOutputMaterial")
    em = nt.nodes.new("ShaderNodeEmission")
    em.inputs["Color"].default_value = srgb(hexcolor)
    nt.links.new(em.outputs["Emission"], out.inputs["Surface"])
    return m


# --- geometry ------------------------------------------------------------------------------

def _link(name, me, mat, loc=(0, 0, 0), smooth=True):
    o = bpy.data.objects.new(name, me)
    bpy.context.collection.objects.link(o)
    o.location = loc
    if smooth:
        for poly in me.polygons:
            poly.use_smooth = True
    if mat:
        me.materials.append(mat)
    return o


def lathe(name, mat, loc, prof, segs=72, wobble=0.0, seed=0, caps=(True, True)):
    """A solid of revolution from (radius, z) points; radius 0 is a pole (one vertex).
    `wobble` makes the outline a little hand-made; caps=(False, ..) leaves an end open."""
    rnd = random.Random(seed)
    harm = [(rnd.uniform(0, 6.28), rnd.uniform(0.4, 1.0)) for _ in range(3)]
    me = bpy.data.meshes.new(name)
    bm = bmesh.new()
    rings = []
    for r, z in prof:
        if r == 0:
            rings.append([bm.verts.new((0, 0, z))])
            continue
        ring = []
        for i in range(segs):
            a = 2 * math.pi * i / segs
            w = 1 + wobble * sum(amp * math.sin((k + 2) * a + ph) for k, (ph, amp) in enumerate(harm)) / 3
            ring.append(bm.verts.new((r * w * math.cos(a), r * w * math.sin(a), z)))
        rings.append(ring)
    for a_, b_ in zip(rings, rings[1:]):
        for i in range(segs):
            j = (i + 1) % segs
            if len(a_) == 1:
                bm.faces.new((a_[0], b_[j], b_[i]))
            elif len(b_) == 1:
                bm.faces.new((a_[i], a_[j], b_[0]))
            else:
                bm.faces.new((a_[i], a_[j], b_[j], b_[i]))
    if len(rings[0]) > 1 and caps[0]:
        bm.faces.new(list(reversed(rings[0])))
    if len(rings[-1]) > 1 and caps[1]:
        bm.faces.new(rings[-1])
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    bm.to_mesh(me)
    bm.free()
    o = _link(name, me, mat, loc)
    for poly in me.polygons:  # flat faces (tops, bottoms) stay flat: smooth only the turned sides
        poly.use_smooth = abs(poly.normal.z) < 0.97
    return o


def ball(name, mat, loc, r, segs=32):
    prof = [(r * math.sin(math.pi * k / 12), -r * math.cos(math.pi * k / 12)) for k in range(13)]
    prof[0], prof[-1] = (0, -r), (0, r)
    return lathe(name, mat, loc, prof, segs=segs)


def blade(name, mat, length, width, bend, fold=0.25, n=24):
    """A leaf: a tapered strip bent along its length, folded along the rib."""
    me = bpy.data.meshes.new(name)
    bm = bmesh.new()
    rows = []
    for i in range(n + 1):
        t = i / n
        w = width * (math.sin(math.pi * min(t * 1.08, 1.0)) ** 0.7) * (1 - 0.15 * t)
        y, z = length * t, bend * t * t * length
        rows.append([bm.verts.new((-w, y, z - fold * w)), bm.verts.new((0, y, z)), bm.verts.new((w, y, z - fold * w))])
    for a_, b_ in zip(rows, rows[1:]):
        for k in range(2):
            bm.faces.new((a_[k], a_[k + 1], b_[k + 1], b_[k]))
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-5)
    bm.to_mesh(me)
    bm.free()
    return _link(name, me, mat)


def slab(name, mat, loc, size, rot=(0, 0, 0), bevel=0.0, uv_top=False):
    """A box (size is its full extent). uv_top: the top face gets the whole 0-1 UV square,
    for a printed decal; other faces get a sliver of its margin."""
    me = bpy.data.meshes.new(name)
    bm = bmesh.new()
    bmesh.ops.create_cube(bm, size=1.0)
    bmesh.ops.scale(bm, vec=size, verts=bm.verts)
    uv = bm.loops.layers.uv.new("UVMap")
    for f in bm.faces:
        for lp in f.loops:
            co = lp.vert.co
            if uv_top and f.normal.z > 0.5:
                lp[uv].uv = (co.x / size[0] + 0.5, co.y / size[1] + 0.5)
            else:
                lp[uv].uv = (0.001, 0.001)
    bm.to_mesh(me)
    bm.free()
    o = _link(name, me, mat, loc, smooth=False)
    o.rotation_euler = tuple(math.radians(a) for a in rot)
    if bevel:
        b = o.modifiers.new("bevel", "BEVEL")
        b.width, b.segments, b.limit_method = bevel, 3, "ANGLE"
    return o


def disc(name, mat, loc, r, rot=(0, 0, 0), verts=64):
    """A flat round face with 0-1 UVs (a clock face, the lens view, a portrait pin)."""
    bpy.ops.mesh.primitive_circle_add(vertices=verts, radius=r, fill_type="NGON", location=loc)
    o = bpy.context.object
    o.name = name
    o.rotation_euler = tuple(math.radians(a) for a in rot)
    me = o.data
    uv = me.uv_layers.new(name="UVMap")
    for poly in me.polygons:
        for li in poly.loop_indices:
            co = me.vertices[me.loops[li].vertex_index].co
            uv.data[li].uv = (co.x / (2 * r) + 0.5, co.y / (2 * r) + 0.5)
    me.materials.append(mat)
    return o


def rod(name, mat, p0, p1, r, verts=24):
    p0, p1 = Vector(p0), Vector(p1)
    o = lathe(name, mat, (0, 0, 0), [(0, 0), (r, 0), (r, (p1 - p0).length), (0, (p1 - p0).length)], segs=verts)
    o.location = p0
    o.rotation_euler = (p1 - p0).to_track_quat("Z", "Y").to_euler()
    return o


def ellipse_shadow(name, mat, loc, rx, ry, rot=0):
    """A contact shadow: a crisp flat ellipse just above the surface (no lines)."""
    o = disc(name, mat, (loc[0], loc[1], loc[2] + 0.002), 1.0, rot=(0, 0, rot))
    o.scale = (rx, ry, 1)
    o.lineart.usage = "EXCLUDE"
    return o


def empty(name, loc=(0, 0, 0), rot=(0, 0, 0)):
    e = bpy.data.objects.new(name, None)
    bpy.context.collection.objects.link(e)
    e.location, e.rotation_euler = loc, rot
    return e


def adopt(parent, *children):
    """Parent without moving: `parent` must not be rotated or scaled."""
    for c in children:
        c.parent = parent
        c.location = Vector(c.location) - Vector(parent.location)


def bake_scale():
    """Object coordinates in true units (so grain is the same size everywhere)."""
    for o in bpy.data.objects:
        if o.type == "MESH" and tuple(o.scale) != (1.0, 1.0, 1.0) and not o.animation_data:
            sc = o.scale.copy()
            o.data = o.data.copy()
            o.data.transform(Matrix.Diagonal((*sc, 1.0)))
            o.scale = (1, 1, 1)


# --- light and camera ----------------------------------------------------------------------

def sun(direction, energy, color=(1.0, 0.96, 0.88), shadow=True):
    """A sun shining along `direction`; shadowed suns get crisp, graphic shadow edges."""
    bpy.ops.object.light_add(type="SUN")
    s = bpy.context.object
    s.rotation_euler = Vector(direction).normalized().to_track_quat("-Z", "Y").to_euler()
    s.data.energy, s.data.color = energy, color
    s.data.use_shadow = shadow
    if shadow:
        s.data.angle = 0.0
        s.data.shadow_filter_radius = 0.0
        s.data.use_shadow_jitter = False
        s.data.shadow_maximum_resolution = 0.0005
    return s


def camera(loc, target, lens=45, tall_target=None):
    """A camera tracking `target`. In the tall cut: a longer lens (TALL_ZOOM) fitted to the
    frame's width, aimed at `tall_target` when the scene gives one."""
    if TALL and tall_target is not None:
        target = tall_target
    bpy.ops.object.camera_add(location=loc)
    cam = bpy.context.object
    bpy.context.scene.camera = cam
    cam.data.lens = lens
    if TALL:
        cam.data.sensor_fit = "HORIZONTAL"
        cam.data.lens = lens * TALL_ZOOM
    t = empty("cam_target", target)
    c = cam.constraints.new("TRACK_TO")
    c.target, c.track_axis, c.up_axis = t, "TRACK_NEGATIVE_Z", "UP_Y"
    return cam


# --- lines ---------------------------------------------------------------------------------

def lines(outer=0.0068, inner=0.0032, wobble=0.05):
    """Line Art per material group: heavy contours, light creases / borders / intersections,
    each tinted from its fill; tapered ends and a fixed wobble that doesn't boil."""
    scene = bpy.context.scene
    groups = {}
    for o in list(bpy.data.objects):
        hidden = o.hide_render and not o.animation_data  # animated visibility still gets lines
        if o.type != "MESH" or not o.data.materials or hidden or o.lineart.usage == "EXCLUDE":
            continue
        nm = o.data.materials[0].name
        if nm not in LINE_TINT:
            continue
        if nm not in groups:
            groups[nm] = bpy.data.collections.new("L_" + nm)
            scene.collection.children.link(groups[nm])
        groups[nm].objects.link(o)
    bpy.ops.object.grease_pencil_add(type="LINEART_SCENE", location=(0, 0, 0))
    gp = bpy.context.object
    gp.name = "lines"
    first = gp.modifiers[0]
    layer = first.target_layer
    k = 0
    for nm, col in groups.items():
        mt = bpy.data.materials.new("line_" + nm)
        bpy.data.materials.create_gpencil_data(mt)
        mt.grease_pencil.color = (*LINE_TINT[nm][:3], 1.0)
        gp.data.materials.append(mt)
        for heavy in (True, False):
            mo = first if k == 0 else gp.modifiers.new(f"la{k}", "LINEART")
            mo.source_type, mo.source_collection = "COLLECTION", col
            mo.target_layer, mo.target_material = layer, mt
            mo.use_contour = heavy
            mo.use_crease = mo.use_material = mo.use_intersection = not heavy
            mo.use_loose = mo.use_edge_mark = False
            mo.crease_threshold = math.radians(115)
            mo.radius = outer if heavy else inner
            mo.opacity = 1.0 if heavy else 0.8
            mo.chaining_image_threshold = 0.004
            mo.smooth_tolerance = 0.1
            mo.use_cache = k > 0
            k += 1
    th = gp.modifiers.new("taper", "GREASE_PENCIL_THICKNESS")
    th.use_uniform_thickness = False
    th.thickness_factor = 0.25
    th.use_custom_curve = True
    pts = th.custom_curve.curves[0].points
    pts[0].location, pts[1].location = (0.0, 1.0), (1.0, 1.0)
    pts.new(0.12, 0.0)
    pts.new(0.88, 0.0)
    th.custom_curve.update()
    nz = gp.modifiers.new("wobble", "GREASE_PENCIL_NOISE")
    nz.factor, nz.factor_thickness, nz.factor_strength = wobble, 0.5, 0.0
    nz.noise_scale, nz.use_random, nz.seed = 1.0, False, 3
    sm = gp.modifiers.new("smooth", "GREASE_PENCIL_SMOOTH")
    sm.factor, sm.step = 0.6, 2
    scene.grease_pencil_settings.antialias_threshold_render = 0.5
    return gp


# --- motion --------------------------------------------------------------------------------

def ease(t, kind="inout"):
    t = max(0.0, min(1.0, t))
    if kind == "in":
        return t * t * t
    if kind == "out":
        return 1 - (1 - t) ** 3
    if kind == "linear":
        return t
    return t * t * (3 - 2 * t)


def poses(table, f):
    """Evaluate a pose table [(frame, value, easing into it), ...] at frame f.
    Values are tuples; segments interpolate with the easing of their end key."""
    if f <= table[0][0]:
        return table[0][1]
    for (f0, v0, _), (f1, v1, kind) in zip(table, table[1:]):
        if f0 <= f <= f1:
            t = ease((f - f0) / (f1 - f0), kind)
            return tuple(a + (b - a) * t for a, b in zip(v0, v1))
    return table[-1][1]


def key_every_frame(o, path, fn):
    """Key `path` on frames 1..LOOP from fn(loop frame 0..LOOP-1): seamless when fn is
    periodic (fn(LOOP) == fn(0))."""
    for f in range(LOOP):
        setattr(o, path, fn(f))
        o.keyframe_insert(path, frame=f + 1)


def render(path_or_dir, anim=False, frame=0):
    scene = bpy.context.scene
    scene.frame_start, scene.frame_end = 1, LOOP
    scene.render.image_settings.file_format = "PNG"
    if anim:
        os.makedirs(path_or_dir, exist_ok=True)
        scene.render.filepath = os.path.join(path_or_dir, "f_")
        scene.render.use_overwrite = False  # resume: frames already on disk are kept
        scene.render.use_placeholder = True  # parallel processes share the frames between them
        bpy.ops.render.render(animation=True)
    else:
        os.makedirs(os.path.dirname(path_or_dir), exist_ok=True)
        scene.frame_set(frame + 1)
        scene.render.filepath = path_or_dir
        bpy.ops.render.render(write_still=True)
