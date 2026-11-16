import random

from zafe_art import defs, f, line_d, svg


def my_scene(p):
    W, H = 1080, 1240
    rnd = random.Random(1)
    cx, cy = 540, 500
    b = [f'<rect width="{W}" height="{H}" fill="url(#sky)"/>']
    b.append(f'<rect width="{W}" height="{H}" fill="url(#fine)" opacity="{p["texop"]}"/>')
    b.append(f'<circle cx="{cx}" cy="{cy}" r="260" fill="url(#glow)"/>')
    b.append(f'<circle cx="{cx}" cy="{cy}" r="140" fill="url(#goldg)" stroke="{p["ink"]}" stroke-width="5"/>')
    b.append(f'<circle cx="{cx}" cy="{cy}" r="140" fill="url(#hatch)" opacity=".3"/>')
    b.append(f'<circle cx="{cx}" cy="{cy - 150}" r="18" fill="{p["brand"]}" stroke="{p["ink"]}" stroke-width="4"/>')
    b.append(f'<path d="{line_d([(cx - 200, cy + 190), (cx, cy + 170), (cx + 200, cy + 190)])}" '
             f'fill="none" stroke="{p["ink"]}" stroke-width="4" stroke-linecap="round"/>')
    for _ in range(12):
        x, y = rnd.randint(80, W - 80), rnd.randint(120, 800)
        b.append(f'<path d="M{x} {y - 8}L{x + 3} {y}L{x} {y + 8}L{x - 3} {y}Z" fill="{p["gold"]}"/>')
    b.append(f'<rect width="{W}" height="{H}" fill="url(#vig)"/>')
    return svg(W, H, defs(p) + "".join(b), p["dot"])
