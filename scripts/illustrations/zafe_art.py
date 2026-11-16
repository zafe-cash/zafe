import math
import random
import re

PALETTES = {
    "dark": dict(
        sky0="#111515", sky1="#080B0B", stone="#1E2323", stone2="#272E2D",
        stone3="#161B1B", light="#4E5857", hi="#737F7D", ink="#020404",
        brand="#51DDD2", brand2="#006660", brand3="#003633", gold="#F4B728", gold2="#B8841A",
        gold3="#FFE39A", metal="#353E3D", metal2="#4E5857", dot="#000000",
        glow="#F4B728", paper="#9DA9A7", texop="0.35", dark=True,
    ),
    "light": dict(
        sky0="#FFFFFF", sky1="#F1F5F5", stone="#D9DFDF", stone2="#E5EAE9",
        stone3="#C9D0CF", light="#F1F5F5", hi="#FFFFFF", ink="#27302F",
        brand="#00736C", brand2="#00605A", brand3="#9EF4EC", gold="#F4B728", gold2="#C98A10",
        gold3="#FFF1C4", metal="#C9D0CF", metal2="#E5EAE9", dot="#55615F",
        glow="#F4B728", paper="#FFFFFF", texop="0.22", dark=False,
    ),
}


def f(v):
    return str(int(round(v)))


def pts(points):
    return " ".join(f"{f(x)},{f(y)}" for x, y in points)


def line_d(points):
    return "M" + "L".join(f"{f(x)} {f(y)}" for x, y in points)


def defs(p, extra=""):
    return f"""<defs>
<radialGradient id="vig" cx="50%" cy="42%" r="70%"><stop offset="55%" stop-color="{p['sky1']}" stop-opacity="0"/><stop offset="100%" stop-color="{p['dot']}" stop-opacity="{'0.55' if p['dark'] else '0.10'}"/></radialGradient>
<radialGradient id="glow"><stop offset="0%" stop-color="{p['glow']}" stop-opacity="0.75"/><stop offset="45%" stop-color="{p['glow']}" stop-opacity="0.22"/><stop offset="100%" stop-color="{p['glow']}" stop-opacity="0"/></radialGradient>
<radialGradient id="glowl"><stop offset="0%" stop-color="{p['brand']}" stop-opacity="0.55"/><stop offset="100%" stop-color="{p['brand']}" stop-opacity="0"/></radialGradient>
<linearGradient id="sky" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{p['sky0']}"/><stop offset="1" stop-color="{p['sky1']}"/></linearGradient>
<linearGradient id="goldg" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="{p['gold3']}"/><stop offset=".45" stop-color="{p['gold']}"/><stop offset="1" stop-color="{p['gold2']}"/></linearGradient>
{extra}</defs>"""


_TEX_RE = re.compile(r'<(polygon|rect|circle|path)\b([^>]*?)fill="url\(#(hatch|hatch2|dots|fine|grit)\)"([^>]*)/>')
_tex_n = [0]


def _num(attrs, name, default=0.0):
    m = re.search(r'\b' + name + r'="([-\d.]+)"', attrs)
    return float(m.group(1)) if m else default


def _bbox(tag, attrs):
    if tag == "rect":
        x, y = _num(attrs, "x"), _num(attrs, "y")
        return x, y, x + _num(attrs, "width"), y + _num(attrs, "height")
    if tag == "circle":
        cx, cy, r = _num(attrs, "cx"), _num(attrs, "cy"), _num(attrs, "r")
        return cx - r, cy - r, cx + r, cy + r
    if tag == "polygon":
        v = [float(t) for t in re.search(r'points="([^"]*)"', attrs).group(1).replace(",", " ").split()]
        return min(v[0::2]), min(v[1::2]), max(v[0::2]), max(v[1::2])
    v = [float(t) for t in re.search(r'bb="([^"]*)"', attrs).group(1).split()]
    return tuple(v)


def _texture(kind, box, ink, rnd):
    x0, y0, x1, y1 = box
    if kind in ("hatch", "hatch2"):
        step, sw, sgn = (10, 2.2, 1) if kind == "hatch" else (8, 1.6, -1)
        h = y1 - y0
        d = []
        c = x0 - h - step
        while c < x1 + h + step:
            if sgn > 0:
                d.append(f"M{f(c)} {f(y1)}L{f(c+h)} {f(y0)}")
            else:
                d.append(f"M{f(c)} {f(y0)}L{f(c+h)} {f(y1)}")
            c += step
        return f'<path d="{"".join(d)}" stroke="{ink}" stroke-width="{sw}"/>'
    area = (x1 - x0) * (y1 - y0)
    per = {"dots": 1000, "fine": 1500, "grit": 1500}[kind]
    n = int(area / per)
    sizes = [2.4, 3.2, 4.2] if kind != "grit" else [2, 3, 4.6]
    buckets = {w: [] for w in sizes}
    for _ in range(n):
        x, y = rnd.uniform(x0, x1), rnd.uniform(y0, y1)
        buckets[rnd.choice(sizes)].append(f"M{x:.0f} {y:.0f}h.1")
    out = "".join(f'<path d="{"".join(v)}" stroke="{ink}" stroke-width="{w:g}" stroke-linecap="round"/>' for w, v in buckets.items() if v)
    if kind == "grit":
        d = [f"M{rnd.uniform(x0, x1):.0f} {rnd.uniform(y0, y1):.0f}l{rnd.randint(4,10)} {rnd.randint(-3,3)}" for _ in range(n // 10)]
        out += f'<path d="{"".join(d)}" stroke="{ink}" stroke-width="1.6"/>'
    return out


def detexture(body, ink):
    rnd = random.Random(1234)
    _tex_n[0] = 0

    def sub(m):
        tag, a, kind, b = m.groups()
        attrs = a + b
        box = _bbox(tag, attrs)
        op = re.search(r'opacity="([^"]*)"', attrs)
        shape_attrs = re.sub(r'\s(opacity|bb|stroke[-\w]*)="[^"]*"', "", attrs)
        _tex_n[0] += 1
        cid = f"t{_tex_n[0]}"
        full = tag == "rect" and "x=" not in attrs and "y=" not in attrs
        clip = "" if full else f'<clipPath id="{cid}"><{tag}{shape_attrs}/></clipPath>'
        g_attrs = (f' clip-path="url(#{cid})"' if not full else "") + (f' opacity="{op.group(1)}"' if op else "")
        return f'{clip}<g{g_attrs}>{_texture(kind, box, ink, rnd)}</g>'

    return _TEX_RE.sub(sub, body)


def svg(w, h, body, ink='#000'):
    body = detexture(body, ink)
    i = body.index("</defs>")
    body = body[:i] + f'<clipPath id="cv"><rect width="{w}" height="{h}"/></clipPath>' + body[i:i + 7] + '<g clip-path="url(#cv)">' + body[i + 7:] + "</g>"
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}">'
            + body + "</svg>")


def stone_wall(p, rnd, x0, y0, x1, y1, row_h=62, blk=(120, 210), stroke=3, hole=None):
    fills = {k: [] for k in ("stone", "stone2", "stone3")}
    shade, hil, cracks = [], [], []
    y = y0
    while y < y1:
        h = row_h + rnd.randint(-8, 8)
        x = x0 - rnd.randint(0, 120)
        while x < x1:
            w = rnd.randint(*blk)
            j = lambda: rnd.randint(-4, 4)
            poly = [(x + 4 + j(), y + 4 + j()), (x + w - 4 + j(), y + 4 + j()),
                    (x + w - 4 + j(), y + h - 4 + j()), (x + 4 + j(), y + h - 4 + j())]
            fill = rnd.choice(["stone", "stone", "stone2", "stone3"])
            crack = rnd.random() < 0.3
            ccx = x + rnd.randint(20, max(21, w - 30))
            cj = (rnd.randint(-8, 8), rnd.randint(-10, 10))
            if hole and all(math.hypot(px - hole[0], py - hole[1]) < hole[2] for px, py in poly):
                x += w
                continue
            fills[fill].append("M" + "L".join(f"{f(px)} {f(py)}" for px, py in poly) + "Z")
            shade.append(f"M{f(x+4)} {f(y+h-16)}L{f(x+w-4)} {f(y+h-12)}V{f(y+h-4)}H{f(x+4)}Z")
            hil.append(f"M{f(x+10)} {f(y+10)}H{f(x+w*0.55)}")
            if crack:
                cracks.append(f"M{f(ccx)} {f(y+6)}l{cj[0]} 14l{cj[1]} 12")
            x += w
        y += h
    out = [f'<g stroke="{p["ink"]}" stroke-width="{stroke}" stroke-linejoin="round">']
    out += [f'<path d="{"".join(v)}" fill="{p[k]}"/>' for k, v in fills.items() if v]
    out.append("</g>")
    out.append(f'<path d="{"".join(shade)}" fill="{p["ink"]}" opacity=".28"/>')
    out.append(f'<path d="{"".join(hil)}" stroke="{p["hi"]}" stroke-width="2" opacity=".5" stroke-linecap="round"/>')
    out.append(f'<path d="{"".join(cracks)}" fill="none" stroke="{p["ink"]}" stroke-width="2"/>')
    return "".join(out)


def coin(p, cx, cy, rx, ry, rot=0):
    return (f'<g transform="rotate({rot} {f(cx)} {f(cy)})">'
            f'<ellipse cx="{f(cx)}" cy="{f(cy+4)}" rx="{f(rx)}" ry="{f(ry)}" fill="{p["gold2"]}" stroke="{p["ink"]}" stroke-width="3"/>'
            f'<ellipse cx="{f(cx)}" cy="{f(cy)}" rx="{f(rx)}" ry="{f(ry)}" fill="url(#goldg)" stroke="{p["ink"]}" stroke-width="3"/>'
            f'<ellipse cx="{f(cx)}" cy="{f(cy)}" rx="{f(rx*0.68)}" ry="{f(ry*0.68)}" fill="none" stroke="{p["gold2"]}" stroke-width="2.5"/>'
            "</g>")




def dial(p, cx, cy, r, ticks=60, major=5, color=None, width=2.5, op=".5", rings=(0, 34)):
    c = color or (p["hi"] if p["dark"] else p["ink"])
    out = [f'<g fill="none" stroke="{c}" opacity="{op}">']
    for k in rings:
        out.append(f'<circle cx="{f(cx)}" cy="{f(cy)}" r="{f(r - k)}" stroke-width="{width}"/>')
    d = []
    for i in range(ticks):
        a = math.radians(i * 360 / ticks - 90)
        l = 22 if i % major == 0 else 11
        d.append(f"M{f(cx + (r - 6) * math.cos(a))} {f(cy + (r - 6) * math.sin(a))}"
                 f"L{f(cx + (r - 6 - l) * math.cos(a))} {f(cy + (r - 6 - l) * math.sin(a))}")
    out.append(f'<path d="{"".join(d)}" stroke-width="{width}" stroke-linecap="round"/></g>')
    return "".join(out)


def key(p, x, y, s=1.0, rot=0, gem=True, ghost=False):
    ink = p["ink"]
    if ghost:
        fill, extra, sw = "none", ' stroke-dasharray="7 9"', 3
        ink = ghost if isinstance(ghost, str) else (p["hi"] if p["dark"] else p["ink"])
    else:
        fill, extra, sw = "url(#goldg)", "", 4
    g = [f'<g transform="translate({f(x)} {f(y)}) rotate({rot}) scale({s})" stroke-linejoin="round">',
         f'<path d="M24 -9H176V9H164V34H146V9H132V26H116V9H24Z" fill="{fill}" stroke="{ink}" stroke-width="{sw}"{extra}/>',
         f'<circle r="38" fill="{fill}" stroke="{ink}" stroke-width="{sw}"{extra}/>']
    if not ghost:
        g.append(f'<path d="M-24 -14a28 28 0 0 1 18 -18M40 -3H170" fill="none" stroke="{p["gold3"]}" stroke-width="4" stroke-linecap="round"/>')
        g.append(f'<path d="M26 4H174" stroke="{p["gold2"]}" stroke-width="3" opacity=".8"/>')
        if gem:
            g.append(f'<circle r="15" fill="{p["brand"]}" stroke="{ink}" stroke-width="4"/>')
            g.append(f'<path d="M-7 -4a8 8 0 0 1 7 -6" fill="none" stroke="{p["hi"] if p["dark"] else "#FFFFFF"}" stroke-width="3" stroke-linecap="round"/>')
        else:
            g.append(f'<circle r="14" fill="{p["sky1"]}" stroke="{ink}" stroke-width="4"/>')
    g.append("</g>")
    return "".join(g)


def sparks(p, rnd, n, x0, y0, x1, y1, lo=4, hi_=10, color=None, avoid=None):
    d = []
    while n > 0:
        x, y = rnd.uniform(x0, x1), rnd.uniform(y0, y1)
        if avoid and math.hypot(x - avoid[0], y - avoid[1]) < avoid[2]:
            continue
        s = rnd.randint(lo, hi_)
        d.append(f"M{f(x)} {f(y - s)}L{f(x + s / 3)} {f(y)}L{f(x)} {f(y + s)}L{f(x - s / 3)} {f(y)}Z")
        n -= 1
    return f'<path d="{"".join(d)}" fill="{color or p["gold"]}"/>'
