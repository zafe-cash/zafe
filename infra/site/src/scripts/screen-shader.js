// A phone's screen on the GPU (world.js). The app's renders and the banners are textures
// uploaded once; this shader composites them the way the app moves between them and draws
// the motion on top, from uniforms set every frame. It replaces 2D-canvas drawing that
// re-uploaded a 780 x 1688 texture on each change (~80 ms a time on an Intel UHD laptop).
//
// Compositing happens in sRGB, as a 2D canvas does (textures are read raw, constants are
// sRGB), then converts to linear for three's output. Coordinates are canvas pixels with y
// down (W x H), so the numbers match the app's layout (SPOT in story-screens.js).
//
// Layers, bottom to top: the base (one screen; a push from A to B; or B as a sheet over A),
// C fading in over B, the sending screen's coins and spinner, two banners, a dim, two taps.
import * as THREE from 'three';
import { W, H, BANNER_PAD, BANNER_H, SPOT } from './story-screens.js';

export const MODE = { single: 0, push: 1, sheet: 2 };

const vertexShader = /* glsl */ `
#include <fog_pars_vertex>
varying vec2 vUv;
void main() {
  vUv = uv;
  vec4 mvPosition = modelViewMatrix * vec4(position, 1.0);
  gl_Position = projectionMatrix * mvPosition;
  #include <fog_vertex>
}`;

const fragmentShader = /* glsl */ `
#include <fog_pars_fragment>
uniform sampler2D uA, uB, uC, uBan1, uBan2;
uniform int uMode;
uniform float uT, uCmix, uDarkUi, uBan1B, uBan2B, uTime, uSending, uDone, uCoins, uDim;
uniform vec4 uTap1, uTap2; // x, y (fractions), t (0..1), unused
uniform vec3 uBg; // sRGB
uniform vec2 uSlot, uStatus;
varying vec2 vUv;

const vec2 S = vec2(${W}.0, ${H}.0);
const vec3 INK = vec3(9.0, 14.0, 14.0) / 255.0;

// Coverage of a signed distance (negative inside), antialiased over one screen pixel.
float cover(float d) { return clamp(0.5 - d / max(fwidth(d), 1e-4), 0.0, 1.0); }
void over(inout vec3 c, vec3 col, float a) { c = mix(c, col, clamp(a, 0.0, 1.0)); }
// A screen render at canvas point p (y down); outside it, the screen's background.
vec3 shot(sampler2D t, vec2 p) {
  vec2 uv = vec2(p.x / S.x, 1.0 - p.y / S.y);
  vec3 c = texture2D(t, clamp(uv, 0.0, 1.0)).rgb;
  float inside = step(0.0, uv.x) * step(uv.x, 1.0) * step(0.0, uv.y) * step(uv.y, 1.0);
  return mix(uBg, c, inside);
}
float roundBox(vec2 v, vec2 half_, float r) {
  vec2 q = abs(v) - half_ + r;
  return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
}
void banner(inout vec3 c, sampler2D t, float b, vec2 p) {
  float top = 22.0 - (1.0 - b) * 260.0 - ${BANNER_PAD}.0;
  vec2 uv = vec2(p.x / S.x, 1.0 - (p.y - top) / ${BANNER_H}.0);
  vec4 k = texture2D(t, clamp(uv, 0.0, 1.0)); // premultiplied
  float a = step(0.0, uv.y) * step(uv.y, 1.0) * clamp(b * 1.5, 0.0, 1.0) * step(0.0001, b);
  c = c * (1.0 - k.a * a) + k.rgb * a;
}
// A fingertip: a pressed disc and a ring that spreads (story beats tapA*, tapB*).
void tap(inout vec3 c, vec4 tp, vec2 p) {
  float t = tp.z;
  float on = step(0.0001, t) * step(t, 0.9999);
  float d = length(p - tp.xy * S);
  float press = t < 0.35 ? t / 0.35 : 1.0;
  float fade = t < 0.75 ? 1.0 : 1.0 - (t - 0.75) / 0.25;
  float R = 46.0 - 8.0 * press;
  vec3 fill = uDarkUi > 0.5 ? vec3(1.0) : INK;
  over(c, fill, on * fade * (uDarkUi > 0.5 ? 0.22 : 0.16) * cover(d - R));
  over(c, vec3(1.0), on * fade * (uDarkUi > 0.5 ? 0.8 : 0.95) * cover(abs(d - R) - 2.0));
  float r = clamp((t - 0.3) / 0.7, 0.0, 1.0);
  vec3 teal = uDarkUi > 0.5 ? vec3(81.0, 221.0, 210.0) / 255.0 : vec3(0.0, 115.0, 108.0) / 255.0;
  over(c, teal, on * step(0.3, t) * (1.0 - r) * 0.6 * cover(abs(d - (40.0 + 140.0 * r)) - 3.0));
}

void main() {
  vec2 p = vec2(vUv.x, 1.0 - vUv.y) * S;
  vec3 c;
  if (uMode == 1) {
    // Navigation push: A slides a little left and darkens, B comes in from the right with
    // a soft shadow on its left edge.
    float t = uT;
    c = shot(uA, p + vec2(0.28 * S.x * t, 0.0));
    over(c, uDarkUi > 0.5 ? vec3(0.0) : INK, (uDarkUi > 0.5 ? 0.5 : 0.18) * t);
    float edge = S.x * (1.0 - t);
    over(c, vec3(0.0), 0.35 * (1.0 - smoothstep(-40.0, 40.0, edge - 10.0 - p.x)) * step(0.0001, t) * step(p.x, edge));
    vec2 q = p - vec2(edge, 0.0);
    vec3 b = mix(shot(uB, q), shot(uC, q), uCmix);
    c = mix(c, b, cover(edge - p.x) * step(0.0001, t));
  } else if (uMode == 2) {
    // A sheet from below: B over a dimmed A, top corners rounded while it moves.
    c = shot(uA, p);
    over(c, INK, 0.35 * uT);
    float y0 = S.y * (1.0 - uT);
    float r = uT < 1.0 ? 56.0 : 0.0;
    vec2 half_ = vec2(S.x * 0.5, (S.y + 80.0) * 0.5);
    float d = roundBox(p - vec2(S.x * 0.5, y0 + half_.y), half_, r);
    c = mix(c, shot(uB, p - vec2(0.0, y0)), cover(d) * step(0.0001, uT));
  } else {
    c = mix(shot(uA, p), shot(uC, p), uCmix);
  }

  // Sending: coins leave the slot along the two dotted trails (top half only), a spinning
  // arc round the status, then the success ripple.
  if (uCoins > 0.0) {
    for (int i = 0; i < 10; i++) {
      float fi = float(i);
      float k = fract(uTime * 0.45 + fi / 10.0);
      vec2 e = mod(fi, 2.0) < 0.5 ? vec2(0.08, 0.36) : vec2(0.92, 0.33);
      vec2 ctr = vec2(uSlot.x + (e.x - uSlot.x) * k, uSlot.y + (e.y - uSlot.y) * k + 0.05 * sin(k * 3.14159265)) * S;
      float spin = abs(cos(uTime * 5.0 + fi));
      vec2 v = p - ctr;
      float cs = cos(0.3), sn = sin(0.3);
      v = vec2(cs * v.x + sn * v.y, -sn * v.x + cs * v.y);
      vec2 rad = vec2(18.0 * (0.35 + 0.65 * spin), 18.0);
      float d = (length(v / rad) - 1.0) * min(rad.x, rad.y);
      float a = uCoins * sin(k * 3.14159265) * step(p.y, S.y * 0.5);
      over(c, vec3(243.0, 186.0, 60.0) / 255.0, a * cover(d));
      over(c, vec3(184.0, 131.0, 15.0) / 255.0, a * cover(abs(d) - 1.5));
    }
  }
  vec2 st = uStatus * S;
  float sd = length(p - st);
  if (uSending > 0.0 && uDone < 1.0) {
    float a0 = mod(uTime * 4.0, 6.28318531);
    float ang = mod(atan(p.y - st.y, p.x - st.x) - a0, 6.28318531);
    float arc = ang < 1.4 ? abs(sd - 84.0) - 3.5 : 1e4;
    vec2 e0 = st + 84.0 * vec2(cos(a0), sin(a0));
    vec2 e1 = st + 84.0 * vec2(cos(a0 + 1.4), sin(a0 + 1.4));
    arc = min(arc, min(length(p - e0), length(p - e1)) - 3.5);
    over(c, vec3(81.0, 221.0, 210.0) / 255.0, uSending * (1.0 - uDone) * cover(arc));
  }
  if (uDone > 0.0 && uDone < 1.0) {
    for (int j = 0; j < 2; j++) {
      float r = clamp((uDone - float(j) * 0.25) / 0.75, 0.0, 1.0);
      over(c, vec3(201.0, 143.0, 20.0) / 255.0, (1.0 - r) * 0.55 * step(0.0001, r) * cover(abs(sd - (66.0 + 150.0 * r)) - 3.0));
    }
  }

  banner(c, uBan1, uBan1B, p);
  banner(c, uBan2, uBan2B, p);
  over(c, INK, 0.45 * uDim);
  tap(c, uTap1, p);
  tap(c, uTap2, p);

  vec3 lin = mix(c / 12.92, pow((c + 0.055) / 1.055, vec3(2.4)), step(0.04045, c));
  gl_FragColor = vec4(lin, 1.0);
  #include <colorspace_fragment>
  #include <fog_fragment>
}`;

// Textures read raw (sRGB values; the shader composites in sRGB), mipmapped and sharp at
// a glancing angle. Uploaded once, by `renderer.initTexture` at load. Canvases (the
// banners, with transparent shadows) are premultiplied so filtering keeps clean edges.
export function screenTexture(source, renderer) {
  const canvas = source instanceof HTMLCanvasElement;
  const t = canvas ? new THREE.CanvasTexture(source) : new THREE.Texture(source);
  t.premultiplyAlpha = canvas;
  t.colorSpace = THREE.NoColorSpace;
  t.anisotropy = Math.min(8, renderer.capabilities.getMaxAnisotropy());
  t.needsUpdate = true;
  renderer.initTexture(t);
  return t;
}

// The sRGB background behind a screen (seen while one slides).
export const bg = (hex) => new THREE.Vector3(...[1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255));

export function screenMaterial(blank) {
  const uniforms = THREE.UniformsUtils.merge([THREE.UniformsLib.fog]);
  Object.assign(uniforms, {
    uA: { value: blank },
    uB: { value: blank },
    uC: { value: blank },
    uBan1: { value: blank },
    uBan2: { value: blank },
    uMode: { value: MODE.single },
    uT: { value: 0 },
    uCmix: { value: 0 },
    uDarkUi: { value: 0 },
    uBan1B: { value: 0 },
    uBan2B: { value: 0 },
    uTime: { value: 0 },
    uSending: { value: 0 },
    uDone: { value: 0 },
    uCoins: { value: 0 },
    uDim: { value: 0 },
    uTap1: { value: new THREE.Vector4() },
    uTap2: { value: new THREE.Vector4() },
    uBg: { value: new THREE.Vector3() },
    uSlot: { value: new THREE.Vector2(...SPOT.slot) },
    uStatus: { value: new THREE.Vector2(...SPOT.status) },
  });
  return new THREE.ShaderMaterial({ uniforms, vertexShader, fragmentShader, fog: true, toneMapped: false });
}
