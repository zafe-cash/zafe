#!/usr/bin/env node
const l2s = x => x <= 0.0031308 ? 12.92 * x : 1.055 * Math.pow(x, 1 / 2.4) - 0.055;
const s2l = x => x <= 0.04045 ? x / 12.92 : Math.pow((x + 0.055) / 1.055, 2.4);
function labToLin(L, a, b) {
  const l = (L + 0.3963377774 * a + 0.2158037573 * b) ** 3, m = (L - 0.1055613458 * a - 0.0638541728 * b) ** 3, s = (L - 0.0894841775 * a - 1.2914855480 * b) ** 3;
  return [4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s, -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s, -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s];
}
const inGamut = (L, C, h) => { const r = h * Math.PI / 180; return labToLin(L, C * Math.cos(r), C * Math.sin(r)).every(v => v >= -1e-4 && v <= 1 + 1e-4); };
function maxC(L, h) { let lo = 0, hi = 0.4; for (let i = 0; i < 30; i++) { const m = (lo + hi) / 2; inGamut(L, m, h) ? lo = m : hi = m; } return lo; }
function oklch(L, C, h) {
  C = Math.min(C, maxC(L, h)); const r = h * Math.PI / 180;
  return '#' + labToLin(L, C * Math.cos(r), C * Math.sin(r)).map(v => Math.round(Math.min(1, Math.max(0, l2s(v))) * 255).toString(16).padStart(2, '0')).join('').toUpperCase();
}
function hexToOklch(hex) {
  const [r, g, b] = [1, 3, 5].map(i => s2l(parseInt(hex.slice(i, i + 2), 16) / 255));
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b), m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b), s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  const L = 0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s, a = 1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s, bb = 0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s;
  return { L, C: Math.hypot(a, bb), h: (Math.atan2(bb, a) * 180 / Math.PI + 360) % 360 };
}
const lum = hex => { const [r, g, b] = [1, 3, 5].map(i => s2l(parseInt(hex.slice(i, i + 2), 16) / 255)); return 0.2126 * r + 0.7152 * g + 0.0722 * b; };
const contrast = (a, b) => { const x = lum(a), y = lum(b); return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05); };

const GOLD = { d: oklch(0.82, 0.15, 83), l: oklch(0.5, 0.11, 78) };

function derive(o) {
  o = { chroma: 0.12, fillL: 0.82, deepL: 0.5, lightFill: 'deep', lightL: 0.84, tint: 0.01, sem: { pos: 'value', warn: 50, bad: 350 }, ...o };
  const h = o.hue, t = o.tint, n = (L, c) => oklch(L, c, h), s = o.sem;
  const d = { win: n(0.145, t * .6), ground: n(0.191, t * .7), raised: n(0.252, t * .8), line: n(0.293, t), muted: n(0.585, t * 1.4), sec: n(0.724, t * 1.3), fg: n(0.967, t * .5) };
  const l = { win: n(0.967, t * .5), ground: '#FFFFFF', raised: n(0.933, t * .6), line: n(0.9, t * .7), muted: n(0.62, t * 1.3), sec: n(0.481, t * 1.4), fg: n(0.16, t * .8) };
  d.acc = oklch(o.fillL, o.chroma, h); l.acc = oklch(o.deepL, o.chroma + .04, h);
  d.fill = d.acc; d.label = oklch(0.2, 0.04, h);
  const deep = o.lightFill === 'deep';
  l.fill = deep ? l.acc : oklch(o.lightL, o.chroma, h); l.label = deep ? '#FFFFFF' : oklch(0.2, 0.045, h);
  d.val = GOLD.d; l.val = GOLD.l;
  if (s.pos === 'value') { d.pos = d.val; l.pos = l.val; } else { d.pos = oklch(0.8, 0.15, s.pos); l.pos = oklch(0.52, 0.14, s.pos); }
  d.warn = oklch(0.82, 0.13, s.warn); l.warn = oklch(0.53, 0.13, s.warn);
  d.bad = oklch(0.7, 0.17, s.bad); l.bad = oklch(0.5, 0.18, s.bad);
  return { d, l, ink: n(0.178, t * 1.6), cardLight: n(0.95, t * 2) };
}

function checks(p) {
  const pairs = {
    'label on fill (dark)': [p.d.label, p.d.fill], 'label on fill (light)': [p.l.label, p.l.fill],
    'brand on window (dark)': [p.d.acc, p.d.win], 'brand on window (light)': [p.l.acc, p.l.win],
    'received (light)': [p.l.pos, p.l.win], 'warning (light)': [p.l.warn, p.l.win], 'error (light)': [p.l.bad, p.l.win],
    'secondary text (dark)': [p.d.sec, p.d.win], 'secondary text (light)': [p.l.sec, p.l.win],
    'value on dark card': [p.d.val, p.ink], 'value on light card': [p.l.val, p.cardLight], 'brand on light card': [p.l.acc, p.cardLight],
  };
  return Object.fromEntries(Object.entries(pairs).map(([k, [a, b]]) => [k, +contrast(a, b).toFixed(2)]));
}

module.exports = { oklch, hexToOklch, contrast, maxC, derive, checks };

if (require.main === module) {
  const argv = process.argv.slice(2), cmd = argv.shift();
  const flag = k => { const i = argv.indexOf(k); return i >= 0 ? argv.splice(i, 2)[1] : undefined; };
  if (cmd === 'derive') {
    const pos = flag('--pos'), warn = flag('--warn'), bad = flag('--bad');
    const [hue, chroma, fillL, deepL, lightFill] = argv;
    const o = { hue: +hue, sem: { pos: pos === undefined || pos === 'value' ? 'value' : +pos, warn: warn ? +warn : 50, bad: bad ? +bad : 350 } };
    if (chroma) o.chroma = +chroma;
    if (fillL) o.fillL = +fillL;
    if (deepL) o.deepL = +deepL;
    if (lightFill) o.lightFill = lightFill;
    const p = derive(o);
    console.log(JSON.stringify(p, null, 2));
    for (const [k, v] of Object.entries(checks(p))) console.log(`${v < 4.5 ? 'FAIL' : 'ok  '} ${v.toFixed(2)}:1  ${k}`);
  } else if (cmd === 'hue') {
    for (const hex of argv) { const { L, C, h } = hexToOklch(hex); console.log(`${hex}  L ${L.toFixed(3)}  C ${C.toFixed(3)}  h ${h.toFixed(1)}`); }
  } else if (cmd === 'contrast') {
    console.log(contrast(argv[0], argv[1]).toFixed(2) + ':1');
  } else {
    console.log('usage: oklch.js derive <hue> [chroma fillL deepL lightFill] [--pos h|value --warn h --bad h] | hue <#hex...> | contrast <#fg> <#bg>');
  }
}
