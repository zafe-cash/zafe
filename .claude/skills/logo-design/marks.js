#!/usr/bin/env node
const f = v => +v.toFixed(2);

function rounded(points, r) {
  const k = points.length, radii = Array.isArray(r) ? r : points.map(() => r);
  let d = '';
  for (let i = 0; i < k; i++) {
    const [px, py] = points[(i - 1 + k) % k], [x, y] = points[i], [nx, ny] = points[(i + 1) % k];
    const d0 = Math.hypot(px - x, py - y), d1 = Math.hypot(nx - x, ny - y), rr = Math.min(radii[i], d0 / 2.2, d1 / 2.2);
    const a = [x + (px - x) * rr / d0, y + (py - y) * rr / d0], b = [x + (nx - x) * rr / d1, y + (ny - y) * rr / d1];
    d += (i ? 'L' : 'M') + f(a[0]) + ' ' + f(a[1]) + 'Q' + f(x) + ' ' + f(y) + ' ' + f(b[0]) + ' ' + f(b[1]);
  }
  return d + 'Z';
}

function offsets(pts, w) {
  const h = w / 2, n = pts.length;
  const seg = i => { const [x0, y0] = pts[i], [x1, y1] = pts[i + 1]; const len = Math.hypot(x1 - x0, y1 - y0); return [(y0 - y1) / len, (x1 - x0) / len]; };
  const side = s => pts.map((p, i) => {
    if (i === 0 || i === n - 1) { const [nx, ny] = seg(i === 0 ? 0 : n - 2); return [p[0] + s * nx * h, p[1] + s * ny * h]; }
    const a = seg(i - 1), b = seg(i); let mx = a[0] + b[0], my = a[1] + b[1]; const ml = Math.hypot(mx, my); mx /= ml; my /= ml;
    const k = h / (mx * a[0] + my * a[1]); return [p[0] + s * mx * k, p[1] + s * my * k];
  });
  return { up: side(-1), dn: side(1) };
}

const ZL = 27, ZR = 73, ZT = 26, ZB = 74, BAR = 12.5, DW = 19;
const leftEdge = y => (ZR - DW) - ((ZR - DW) - ZL) * (y - ZT) / (ZB - ZT);
const rightEdge = y => ZR - (ZR - (ZL + DW)) * (y - ZT) / (ZB - ZT);

function zSolid(r = 2) {
  const pts = [[ZL, ZT], [ZR, ZT], [ZR, ZT + BAR], [ZL + DW - 1.5, ZB - BAR], [ZR, ZB - BAR], [ZR, ZB], [ZL, ZB], [ZL, ZB - BAR], [ZR - DW + 1.5, ZT + BAR], [ZL, ZT + BAR]];
  return [{ d: rounded(pts, [r, r, r * .7, r * .3, r, r, r, r * .7, r * .3, r]), tone: 0 }];
}

function zQuorum(r = 2, g = 6) {
  const top = [[ZL, ZT], [leftEdge(ZT) - g, ZT], [leftEdge(ZT + BAR) - g, ZT + BAR], [ZL, ZT + BAR]];
  const bot = [[rightEdge(ZB - BAR) + g, ZB - BAR], [ZR, ZB - BAR], [ZR, ZB], [rightEdge(ZB) + g, ZB]];
  const diag = [[ZR - DW, ZT], [ZR, ZT], [ZL + DW, ZB], [ZL, ZB]];
  return [{ d: rounded(top, [r, r * .6, r * .6, r]), tone: 0 }, { d: rounded(diag, r), tone: 1 }, { d: rounded(bot, [r * .6, r, r, r * .6]), tone: 0 }];
}

function seam(r = 2.6, w = 9.5) {
  const l = 22, rr = 78, t = 22, b = 78, ro = r * 2 + 1.2, ri = Math.max(.4, r * .5);
  const { up: U, dn: D } = offsets([[l - 3, 37], [63, 37], [37, 63], [rr + 3, 63]], w);
  const upper = [[l, t], [rr, t], [rr, U[3][1]], U[2], U[1], [l, U[0][1]]];
  const lower = [[l, D[0][1]], D[1], D[2], [rr, D[3][1]], [rr, b], [l, b]];
  return [
    { d: rounded(upper, [ro, ro, ri, ri, ri, ri]), tone: 0, points: upper },
    { d: rounded(lower, [ri, ri, ri, ri, ro, ro]), tone: 1, points: lower },
  ];
}

function svg(paths, { px = 100, tile = null, shape = 'square', colors = ['#111'], scale = 1 } = {}) {
  const bg = !tile ? '' : shape === 'circle' ? `<circle cx="50" cy="50" r="50" fill="${tile}"/>` : `<rect width="100" height="100" rx="22.4" fill="${tile}"/>`;
  const g = scale === 1 ? '<g>' : `<g transform="translate(${50 - 50 * scale} ${50 - 50 * scale}) scale(${scale})">`;
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${px}" height="${px}" viewBox="0 0 100 100">${bg}${g}${paths.map(p => `<path d="${p.d}" fill="${colors[p.tone] ?? colors[0]}"/>`).join('')}</g></svg>`;
}

module.exports = { rounded, offsets, zSolid, zQuorum, seam, svg };

if (require.main === module) {
  const [cmd, out = 'proof.html'] = process.argv.slice(2);
  if (cmd !== 'proof') { console.log('usage: marks.js proof <out.html>'); process.exit(0); }
  const marks = { Seam: seam(), 'Quorum Z': zQuorum(), 'Solid Z': zSolid() };
  const tile = '#004A46', colors = ['#D9F5F2', '#51DDD2'];
  let html = '<!doctype html><meta charset="utf-8"><body style="margin:0;padding:24px;font:13px sans-serif;background:#e6e6e3">';
  for (const [name, paths] of Object.entries(marks)) {
    html += `<h3>${name}</h3><div style="display:flex;gap:18px;align-items:center;flex-wrap:wrap">`;
    for (const px of [200, 48, 24, 16]) html += svg(paths, { px, tile, colors });
    html += `<div style="background:#111;padding:8px;border-radius:8px">${svg(paths, { px: 24, colors: ['#fff', '#fff'], scale: 1.25 })}</div>`;
    html += svg(paths, { px: 72, tile: '#C9DDF2', shape: 'circle', colors: ['#16324F', '#16324F'], scale: .8 });
    html += '</div>';
  }
  require('fs').writeFileSync(out, html + '</body>');
  console.log('wrote ' + out);
}
