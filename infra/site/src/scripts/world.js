// The Seam Vault: the page's 3D world (docs/site.md, "seventh round").
//
// An isometric diorama on grained paper. The vault door is the Seam mark: its two pieces
// are the door leaves and the Z channel between them is the keyhole. Three members'
// phones stand on pedestals around it; each holds a key shard (half of the Z). Scrolling
// plays one payment: a lone shard can't open the lock; Bob proposes (his phone, dark
// mode, under a warm lamp); a pulse runs through the floor to the others; you approve
// with one tap (your phone, light mode, daylight); two shards seat in the Z and fuse into
// the gold key (it never exists on any phone); the door splits along the Z and gold
// light floods out; coins pour into your phone's sending screen; it's sent and joins a
// stream of identical payments; the door closes back into the logo.
//
// State: everything the scroll timeline moves lives in `S`. The frame loop damps toward
// it, poses the objects, redraws the phone screens only when their state changes
// (story-screens.js), and renders through bloom and grain (postprocessing).

import * as THREE from 'three';
import { RoundedBoxGeometry } from 'three/addons/geometries/RoundedBoxGeometry.js';
import { RoomEnvironment } from 'three/addons/environments/RoomEnvironment.js';
import { SVGLoader } from 'three/addons/loaders/SVGLoader.js';
import { EffectComposer, RenderPass, EffectPass, BloomEffect, NoiseEffect, VignetteEffect, BlendFunction } from 'postprocessing';
import gsap from 'gsap';
import { ScrollTrigger } from 'gsap/ScrollTrigger';
import { W, H, SPOT, loadScreens, banners } from './story-screens.js';
import { MODE, bg, screenMaterial, screenTexture } from './screen-shader.js';

gsap.registerPlugin(ScrollTrigger);

// ---------------------------------------------------------------------------- state

const S = {
  cam: 0, // position along the camera path (0 .. CAM.length - 1)
  frameY: 0.02, // how far down the frame the world sits
  frameX: 0.25, // how far right (the hero's headline is on the left)
  portraitY: 0, // portrait only: extra shift (negative lifts the world above bottom cards)
  ghost: 0, lone: 0, leak: 0.8,
  shardsUp: 0, // shards rise out of the phones
  pulse: 0, // the floor pulse from Bob to the others
  shardA: 0, shardB: 0, fuse: 0, insert: 0, open: 0,
  coins: 0, stream: 0, closeDoor: 0,
  check: 0, // your phone's checks: 0..1 the scan and badges appear, 1..2 they go
  ctaZ: 0, // the call to action
  aPush1: 0, aPush2: 0,
  tapA1: 0, tapA2: 0, tapB1: 0, tapB2: 0,
  banner: 0, sentBanner: 0, caraDim: 0,
  bSheet: 0, bPush: 0, bDone: 0,
};

// ---------------------------------------------------------------------------- helpers

const lerp = (a, b, t) => a + (b - a) * t;
const clamp01 = (v) => Math.min(1, Math.max(0, v));
const smooth = (t) => t * t * (3 - 2 * t);
const damp = (a, b, lambda, dt) => lerp(a, b, 1 - Math.exp(-lambda * dt));

// The Seam mark in its own 0..100 space (public/assets/zafe.svg).
const MARK_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
<path d="M22 26.66Q22 22 26.66 22L71.6 22Q78 22 78 28.4L78 56.95Q78 58.25 76.7 58.25L49.77 58.25Q48.47 58.25 49.39 57.33L73.55 33.17Q74.47 32.25 73.17 32.25L23.3 32.25Q22 32.25 22 30.95Z"/>
<path d="M22 43.05Q22 41.75 23.3 41.75L50.23 41.75Q51.53 41.75 50.61 42.67L26.45 66.83Q25.53 67.75 26.83 67.75L76.7 67.75Q78 67.75 78 69.05L78 73.34Q78 78 73.34 78L28.4 78Q22 78 22 71.6Z"/>
</svg>`;
// The Z channel's centreline, in mark space: top bar, diagonal, bottom bar.
const Z = [
  [22, 37],
  [62, 37],
  [38, 63],
  [78, 63],
];

const DOOR = 2.7; // the door frame's side, world units
const K = DOOR / 100; // mark units to world
const FRAME_DEPTH = 0.5;
const LEAF_DEPTH = 0.22;

// Mark space to door-local (x right, y up, z out of the door).
const markToDoor = (mx, my, z = 0) => new THREE.Vector3((mx - 50) * K, (50 - my) * K, z);

// A paper texture: warm off-white with grain and a few fibres.
function paperTexture() {
  const c = document.createElement('canvas');
  c.width = c.height = 512;
  const g = c.getContext('2d');
  g.fillStyle = '#E4EAE8';
  g.fillRect(0, 0, 512, 512);
  const img = g.getImageData(0, 0, 512, 512);
  for (let i = 0; i < img.data.length; i += 4) {
    const n = (Math.random() - 0.5) * 18;
    img.data[i] += n;
    img.data[i + 1] += n;
    img.data[i + 2] += n;
  }
  g.putImageData(img, 0, 0);
  g.strokeStyle = 'rgba(120,140,136,0.06)';
  g.lineWidth = 1;
  for (let i = 0; i < 90; i++) {
    const x = Math.random() * 512;
    const y = Math.random() * 512;
    const a = Math.random() * Math.PI;
    g.beginPath();
    g.moveTo(x, y);
    g.lineTo(x + Math.cos(a) * 24, y + Math.sin(a) * 24);
    g.stroke();
  }
  const t = new THREE.CanvasTexture(c);
  t.wrapS = t.wrapT = THREE.RepeatWrapping;
  t.repeat.set(10, 10);
  t.colorSpace = THREE.SRGBColorSpace;
  return t;
}

// A plane with rounded corners and 0..1 UVs (the phone screens).
function roundedPlane(w, h, r) {
  const s = new THREE.Shape();
  const x = -w / 2;
  const y = -h / 2;
  s.moveTo(x + r, y);
  s.lineTo(x + w - r, y);
  s.quadraticCurveTo(x + w, y, x + w, y + r);
  s.lineTo(x + w, y + h - r);
  s.quadraticCurveTo(x + w, y + h, x + w - r, y + h);
  s.lineTo(x + r, y + h);
  s.quadraticCurveTo(x, y + h, x, y + h - r);
  s.lineTo(x, y + r);
  s.quadraticCurveTo(x, y, x + r, y);
  const geo = new THREE.ShapeGeometry(s, 24);
  const pos = geo.attributes.position;
  const uv = new Float32Array(pos.count * 2);
  for (let i = 0; i < pos.count; i++) {
    uv[i * 2] = (pos.getX(i) - x) / w;
    uv[i * 2 + 1] = (pos.getY(i) - y) / h;
  }
  geo.setAttribute('uv', new THREE.BufferAttribute(uv, 2));
  return geo;
}

// A key shard: bars along part of the Z centreline, in door-local space.
function zBars(segments, material) {
  const group = new THREE.Group();
  for (const [[x1, y1], [x2, y2]] of segments) {
    const a = markToDoor(x1, y1);
    const b = markToDoor(x2, y2);
    const len = a.distanceTo(b) + 8 * K;
    const bar = new THREE.Mesh(new RoundedBoxGeometry(len, 8 * K, 0.12, 3, 0.03), material);
    bar.position.copy(a).lerp(b, 0.5);
    bar.rotation.z = Math.atan2(b.y - a.y, b.x - a.x);
    bar.castShadow = true;
    group.add(bar);
  }
  return group;
}

// ---------------------------------------------------------------------------- scene

export async function start({ canvas, story, hero, cta, covers, screens, still = false, software = false, cuts = false }) {
  // story.js starts fetching the screens while this module downloads.
  const images = await (screens ?? loadScreens());

  const renderer = new THREE.WebGLRenderer({ canvas, antialias: false, powerPreference: 'high-performance', stencil: false });
  // Reading a program's info log waits for its compile to finish; skip it outside checks.
  renderer.debug.checkShaderErrors = still || location.hostname === 'localhost' || location.hostname === '127.0.0.1';
  // The screens and banners go to the GPU now, once (screen-shader.js).
  const shots = Object.fromEntries(Object.entries(images).map(([k, img]) => [k, screenTexture(img, renderer)]));
  const bannerCanvases = banners();
  const bannerTex = { needsYou: screenTexture(bannerCanvases.needsYou, renderer), sent: screenTexture(bannerCanvases.sent, renderer) };
  const mobile = window.matchMedia('(max-width: 760px)').matches;
  // Quality levels, best first. The page starts at the device's level and steps down
  // (never back up, so it can't flicker) while frames are slow; see adapt().
  const dpr = window.devicePixelRatio || 1;
  // Resolution is the main lever: on an Intel UHD laptop (dpr 1.25, 165 Hz) the full look
  // ran at 18 ms at its own dpr and 12 ms at dpr 1, as fast as without post-processing.
  // So the look (bloom, noise, vignette) stays down to dpr 1; only the last levels drop it.
  const LEVELS = [
    { dpr: Math.min(dpr, 2), msaa: 4, post: true },
    { dpr: Math.min(dpr, 2), msaa: 2, post: true }, // phones start here: sharp on 3x screens
    { dpr: Math.min(dpr, 1.5), msaa: 2, post: true },
    { dpr: Math.min(dpr, 1.5), msaa: 0, post: true },
    { dpr: Math.min(dpr, 1), msaa: 0, post: true },
    { dpr: Math.min(dpr, 1), msaa: 0, post: false },
    { dpr: 0.75, msaa: 0, post: false },
  ];
  // What a level costs relative to another (pixels, MSAA samples, post), so a drop skips
  // levels that are the same here (on a dpr 1.25 screen, dpr 2 and 1.5 are both 1.25).
  const weight = (l) => LEVELS[l].dpr ** 2 * Math.max(1, LEVELS[l].msaa) * (LEVELS[l].post ? 1.6 : 1);
  const cheaper = (l) => {
    let n = l + 1;
    while (n < LEVELS.length - 1 && weight(n) > 0.8 * weight(l)) n++;
    return Math.min(n, LEVELS.length - 1);
  };
  // Start where the window can afford it: the best level whose render (pixels times MSAA
  // samples) fits a budget, so a large or high-dpr laptop window doesn't open on 4x MSAA
  // at dpr 2 (24M samples a frame on a 1500x1000 window) and stutter while it adapts.
  const BUDGET = 9e6; // samples per frame
  const cost = (l) => window.innerWidth * window.innerHeight * LEVELS[l].dpr ** 2 * Math.max(1, LEVELS[l].msaa);
  let level = still ? 0 : mobile ? 1 : 0;
  while (!still && level < 3 && cost(level) > BUDGET) level++;
  if (software) level = LEVELS.length - 1; // no GPU: the cheapest look from the start
  // The level this device settled on last time (same GPU and screen density, a similar
  // window, within a week), so a return visit starts there instead of stepping down again:
  // each step reallocates the buffers, a visible hitch. A per-viewer convenience in local
  // storage (never sent anywhere); without storage it adapts as on a first visit.
  const LEVEL_KEY = 'zafe-world-level';
  const gl = renderer.getContext();
  const gpuInfo = gl.getExtension('WEBGL_debug_renderer_info');
  const device = `${gpuInfo ? gl.getParameter(gpuInfo.UNMASKED_RENDERER_WEBGL) : ''}|${dpr}`;
  const area = () => window.innerWidth * window.innerHeight;
  const remember = () => {
    try {
      localStorage.setItem(LEVEL_KEY, JSON.stringify({ device, area: area(), level, at: Date.now() }));
    } catch {}
  };
  if (!still && !software) {
    try {
      const v = JSON.parse(localStorage.getItem(LEVEL_KEY));
      const recent = Date.now() - v.at < 7 * 864e5;
      if (v.device === device && recent && Math.abs(v.area / area() - 1) < 0.25 && v.level >= 0 && v.level < LEVELS.length) level = v.level;
    } catch {}
  }
  canvas.dataset.quality = String(level);
  let settleUntil = performance.now() + 2000; // adapt() ignores frames until then
  renderer.setPixelRatio(LEVELS[level].dpr);
  renderer.outputColorSpace = THREE.SRGBColorSpace;
  renderer.toneMapping = THREE.NoToneMapping;
  // Shadows are rendered once, at load, and baked into a texture (bakeShadows below); after
  // that the shadow map is off. Things that move (the phones) get painted
  // contact shadows (blob()).
  renderer.shadowMap.enabled = true;
  renderer.shadowMap.type = THREE.VSMShadowMap;
  renderer.shadowMap.autoUpdate = false;

  const scene = new THREE.Scene();
  const PAPER = new THREE.Color('#F1F5F5');
  scene.background = PAPER;
  scene.fog = new THREE.Fog(PAPER, 30, 60);
  const pmrem = new THREE.PMREMGenerator(renderer);
  scene.environment = pmrem.fromScene(new RoomEnvironment(), 0.04).texture;
  scene.environmentIntensity = 0.35;

  // Light: a soft key from the upper left with shadows, sky and ground fill.
  const key = new THREE.DirectionalLight('#FFF3E2', 2.6);
  key.position.set(-6, 12, 8);
  key.castShadow = true;
  key.shadow.mapSize.setScalar(2048);
  key.shadow.camera.left = -9;
  key.shadow.camera.right = 9;
  key.shadow.camera.top = 9;
  key.shadow.camera.bottom = -9;
  key.shadow.radius = 10;
  key.shadow.blurSamples = 16;
  key.shadow.bias = -0.0005;
  scene.add(key, new THREE.HemisphereLight('#FFFFFF', '#B9C7C4', 0.45));

  // Floor.
  const floor = new THREE.Mesh(
    new THREE.PlaneGeometry(90, 90),
    new THREE.MeshStandardMaterial({ map: paperTexture(), roughness: 1, metalness: 0 }),
  );
  floor.rotation.x = -Math.PI / 2;
  floor.receiveShadow = true;
  scene.add(floor);

  const clay = (color, extra = {}) => new THREE.MeshStandardMaterial({ color, roughness: 0.78, metalness: 0.02, ...extra });

  // ---------------------------------------------------------------- the vault
  const VAULT_POS = new THREE.Vector3(0, 0, -0.6);
  const vault = new THREE.Group();
  vault.position.copy(VAULT_POS);
  scene.add(vault);
  const plinth = new THREE.Mesh(new RoundedBoxGeometry(3.6, 0.4, 1.5, 4, 0.08), clay('#CFD9D7'));
  plinth.position.y = 0.2;
  plinth.castShadow = plinth.receiveShadow = true;
  vault.add(plinth);
  const door = new THREE.Group(); // door-local: origin at the door's centre, +z out
  door.position.set(0, 0.4 + DOOR / 2, 0);
  vault.add(door);
  const doorFrame = new THREE.Mesh(new RoundedBoxGeometry(DOOR, DOOR, FRAME_DEPTH, 6, 0.36), clay('#004A46', { roughness: 0.55 }));
  doorFrame.castShadow = doorFrame.receiveShadow = true;
  door.add(doorFrame);
  const FRONT = FRAME_DEPTH / 2;
  // Gold light behind the leaves: only the Z channel lets it out.
  const glowCanvas = document.createElement('canvas');
  glowCanvas.width = glowCanvas.height = 256;
  {
    const g = glowCanvas.getContext('2d');
    const grad = g.createRadialGradient(128, 128, 10, 128, 128, 180);
    grad.addColorStop(0, '#FFF1C4');
    grad.addColorStop(0.5, '#F3BA3C');
    grad.addColorStop(1, '#8A5E08');
    g.fillStyle = grad;
    g.fillRect(0, 0, 256, 256);
  }
  const glowTex = new THREE.CanvasTexture(glowCanvas);
  glowTex.colorSpace = THREE.SRGBColorSpace;
  const glowMat = new THREE.MeshBasicMaterial({ map: glowTex, color: new THREE.Color('#FFFFFF'), transparent: true, toneMapped: false });
  const glow = new THREE.Mesh(new THREE.PlaneGeometry(58 * K, 58 * K), glowMat);
  glow.position.z = FRONT + 0.004;
  door.add(glow);
  const vaultLight = new THREE.PointLight('#F6C35A', 0, 9, 1.4);
  vaultLight.position.set(0, 0, FRONT - 0.2);
  door.add(vaultLight);
  // Inside: a gold-lit recess with stacks of coins and rays, seen when the door opens.
  const inside = new THREE.Group();
  inside.position.z = FRONT - 0.02;
  door.add(inside);
  const recess = new THREE.Mesh(
    new THREE.BoxGeometry(58 * K, 58 * K, 0.6),
    new THREE.MeshStandardMaterial({ color: '#3A2A08', roughness: 0.9, side: THREE.BackSide }),
  );
  recess.position.z = -0.3;
  inside.add(recess);
  const stackMat = new THREE.MeshStandardMaterial({ color: '#F3BA3C', metalness: 1, roughness: 0.25, emissive: '#A5750F', emissiveIntensity: 0.5 });
  for (let i = 0; i < 7; i++) {
    const n = 3 + ((i * 5) % 6);
    const stack = new THREE.Mesh(new THREE.CylinderGeometry(0.11, 0.11, 0.028 * n, 32), stackMat);
    stack.position.set(-0.55 + (i % 4) * 0.36 + (i > 3 ? 0.18 : 0), -0.62 + 0.014 * n, -0.45 + (i > 3 ? 0.2 : 0));
    inside.add(stack);
  }
  const haloCanvas = document.createElement('canvas');
  haloCanvas.width = haloCanvas.height = 256;
  {
    const g = haloCanvas.getContext('2d');
    const grad = g.createRadialGradient(128, 128, 0, 128, 128, 128);
    grad.addColorStop(0, 'rgba(255,226,150,0.9)');
    grad.addColorStop(0.35, 'rgba(243,186,60,0.35)');
    grad.addColorStop(1, 'rgba(243,186,60,0)');
    g.fillStyle = grad;
    g.fillRect(0, 0, 256, 256);
  }
  const rayMat = new THREE.SpriteMaterial({ map: new THREE.CanvasTexture(haloCanvas), transparent: true, opacity: 0, depthWrite: false, blending: THREE.AdditiveBlending, toneMapped: false });
  const halo = new THREE.Sprite(rayMat);
  halo.scale.setScalar(4.2);
  halo.position.z = 0.9;
  inside.add(halo);

  // The leaves: the mark's two pieces, extruded.
  const svg = new SVGLoader().parse(MARK_SVG);
  const leafMats = [clay('#D9F5F2', { roughness: 0.6 }), clay('#51DDD2', { roughness: 0.6 })];
  const leaves = svg.paths.map((path, i) => {
    const geo = new THREE.ExtrudeGeometry(path.toShapes(true), {
      depth: LEAF_DEPTH / K,
      bevelEnabled: true,
      bevelThickness: 0.8,
      bevelSize: 0.6,
      bevelSegments: 3,
      curveSegments: 12,
    });
    geo.translate(-50, -50, 0);
    geo.rotateX(Math.PI); // mark y is down: turn over so the front faces +z
    geo.scale(K, K, K);
    geo.translate(0, 0, FRONT + LEAF_DEPTH + 0.01);
    const leaf = new THREE.Mesh(geo, leafMats[i]);
    leaf.castShadow = true;
    const pivot = new THREE.Group();
    pivot.add(leaf);
    door.add(pivot);
    return pivot;
  });
  const LEAF_FRONT = FRONT + LEAF_DEPTH + 0.02;
  // The ghost keyhole: a teal outline of the Z.
  const ghostCurve = new THREE.CurvePath();
  for (let i = 0; i < Z.length - 1; i++) {
    ghostCurve.add(new THREE.LineCurve3(markToDoor(...Z[i], LEAF_FRONT + 0.02), markToDoor(...Z[i + 1], LEAF_FRONT + 0.02)));
  }
  const ghostMat = new THREE.MeshBasicMaterial({ color: new THREE.Color('#51DDD2').multiplyScalar(2.2), transparent: true, opacity: 0, toneMapped: false });
  const ghost = new THREE.Mesh(new THREE.TubeGeometry(ghostCurve, 120, 0.035, 8, false), ghostMat);
  door.add(ghost);

  // ---------------------------------------------------------------- key shards
  const shardMat = () =>
    new THREE.MeshStandardMaterial({ color: '#51DDD2', emissive: new THREE.Color('#51DDD2'), emissiveIntensity: 1.8, roughness: 0.25, metalness: 0.1 });
  const MID = [50, 50];
  const halfA = [[Z[0], Z[1]], [Z[1], MID]]; // top bar + upper diagonal
  const halfB = [[MID, Z[2]], [Z[2], Z[3]]]; // lower diagonal + bottom bar
  const shards = {
    bob: zBars(halfA, shardMat()),
    me: zBars(halfB, shardMat()),
    cara: zBars(halfB, shardMat()),
  };
  for (const sh of Object.values(shards)) scene.add(sh);
  const goldKey = new THREE.MeshStandardMaterial({ color: '#F3BA3C', metalness: 1, roughness: 0.22, emissive: new THREE.Color('#F3BA3C'), emissiveIntensity: 0 });

  // ---------------------------------------------------------------- phones
  const SCREEN_W = 0.96;
  const SCREEN_H = (SCREEN_W * H) / W;
  const BODY_W = SCREEN_W + 0.1;
  const BODY_H = SCREEN_H + 0.1;
  const PHONE_DEPTH = 0.1;
  const PEDESTAL_H = 0.3;
  // Painted light and shade, the way lightweight 3D sites fake them: a soft radial blob
  // on the surface below a thing, instead of a real light (evaluated for every pixel) or a
  // shadow re-rendered every frame. `shade` darkens (contact shadow), otherwise it glows.
  function blob(color, size, opacity, shade = false) {
    const c = document.createElement('canvas');
    c.width = c.height = 128;
    const g = c.getContext('2d');
    const grad = g.createRadialGradient(64, 64, 0, 64, 64, 64);
    grad.addColorStop(0, color);
    grad.addColorStop(1, shade ? 'rgba(255,255,255,0)' : 'rgba(0,0,0,0)');
    g.fillStyle = grad;
    g.fillRect(0, 0, 128, 128);
    const t = new THREE.CanvasTexture(c);
    t.colorSpace = THREE.SRGBColorSpace;
    const m = new THREE.Mesh(
      new THREE.PlaneGeometry(size[0], size[1]),
      new THREE.MeshBasicMaterial({
        map: t, transparent: true, opacity, depthWrite: false, toneMapped: false,
        blending: shade ? THREE.NormalBlending : THREE.AdditiveBlending,
      }),
    );
    m.rotation.x = -Math.PI / 2;
    return m;
  }

  function makePhone(frameColor, lampColor, lampIntensity, screenBg) {
    const root = new THREE.Group();
    const pedestal = new THREE.Mesh(new THREE.CylinderGeometry(0.7, 0.78, PEDESTAL_H, 48), clay('#CFD9D7'));
    pedestal.position.y = PEDESTAL_H / 2;
    pedestal.castShadow = pedestal.receiveShadow = true;
    root.add(pedestal);
    const phone = new THREE.Group();
    phone.position.y = PEDESTAL_H + BODY_H / 2 + 0.06;
    root.add(phone);
    const body = new THREE.Mesh(
      new RoundedBoxGeometry(BODY_W, BODY_H, PHONE_DEPTH, 8, 0.14),
      new THREE.MeshStandardMaterial({ color: frameColor, metalness: 0.7, roughness: 0.3 }),
    );
    phone.add(body);
    // It turns and floats every frame: a contact shadow on the pedestal rather than a cast
    // shadow, so the shadow map doesn't have to be redrawn while the camera moves.
    const contact = blob('rgba(40,60,58,0.5)', [BODY_W * 1.1, 0.5], 1, true);
    contact.position.y = PEDESTAL_H + 0.003;
    root.add(contact);
    const screenMat = screenMaterial(shots.meHome);
    screenMat.uniforms.uBg.value.copy(bg(screenBg));
    const screen = new THREE.Mesh(roundedPlane(SCREEN_W, SCREEN_H, 0.11), screenMat);
    screen.position.z = PHONE_DEPTH / 2 + 0.002;
    phone.add(screen);
    // The phone's lamp as a pool of light on the floor around its pedestal (was a point light).
    if (lampIntensity > 0) {
      const pool = blob(lampColor, [3.4, 3.4], Math.min(0.5, lampIntensity * 0.08));
      pool.position.set(0.3, 0.004, 0.5);
      root.add(pool);
    }
    scene.add(root);
    return { root, phone, u: screenMat.uniforms };
  }
  const bob = makePhone('#2B3131', '#FFB36B', 5, '#080B0B');
  const me = makePhone('#D5DCDB', '#D8ECFF', 3, '#F1F5F5');
  const cara = makePhone('#C8B79A', '#FFFFFF', 0, '#F1F5F5');
  bob.u.uDarkUi.value = 1;
  bob.u.uMode.value = MODE.push;
  bob.u.uBan1.value = bannerTex.sent;
  me.u.uBan1.value = bannerTex.needsYou;
  me.u.uBan2.value = bannerTex.sent;
  me.u.uC.value = shots.meDone;
  cara.u.uA.value = shots.meHome;
  cara.u.uBan1.value = bannerTex.needsYou;
  cara.u.uBan2.value = bannerTex.sent;
  const BOB_SCREENS = [shots.bobHome, shots.bobPropose, shots.bobApproved];
  const PHONES = {
    bob: new THREE.Vector3(-3.4, 0, 1.2),
    me: new THREE.Vector3(3.4, 0, 1.2),
    cara: new THREE.Vector3(2.4, 0, -3.3),
  };
  bob.root.position.copy(PHONES.bob);
  me.root.position.copy(PHONES.me);
  cara.root.position.copy(PHONES.cara);

  // A point on a phone's screen (fractions of the viewport) in world space.
  const screenPoint = (p, fx, fy, out = new THREE.Vector3()) =>
    p.phone.localToWorld(out.set((fx - 0.5) * SCREEN_W, (0.5 - fy) * SCREEN_H, PHONE_DEPTH / 2 + 0.03));
  const phoneTop = (p, out = new THREE.Vector3()) => p.phone.localToWorld(out.set(0, BODY_H / 2 + 0.95, 0));

  // ---------------------------------------------------------------- floor channels and the pulse
  // Z-shaped grooves from Bob's pedestal to the other two.
  const channelMat = clay('#CBD6D4', { roughness: 0.9 });
  const pulseMat = new THREE.MeshBasicMaterial({ color: new THREE.Color('#51DDD2').multiplyScalar(3), toneMapped: false });
  function zPath(a, b) {
    const mid1 = new THREE.Vector3(lerp(a.x, b.x, 0.25), 0.012, a.z + 1.2);
    const mid2 = new THREE.Vector3(lerp(a.x, b.x, 0.75), 0.012, b.z + 1.2);
    const p = new THREE.CurvePath();
    const pts = [new THREE.Vector3(a.x, 0.012, a.z), mid1, mid2, new THREE.Vector3(b.x, 0.012, b.z)];
    for (let i = 0; i < 3; i++) p.add(new THREE.LineCurve3(pts[i], pts[i + 1]));
    return p;
  }
  const paths = [zPath(PHONES.bob, PHONES.me), zPath(PHONES.bob, PHONES.cara)];
  for (const p of paths) {
    const groove = new THREE.Mesh(new THREE.TubeGeometry(p, 80, 0.07, 6, false), channelMat);
    groove.scale.y = 0.25;
    groove.receiveShadow = true;
    scene.add(groove);
  }
  const pulses = paths.map(() => {
    const m = new THREE.Mesh(new THREE.SphereGeometry(0.11, 16, 16), pulseMat);
    scene.add(m);
    return m;
  });

  // ---------------------------------------------------------------- coins and the stream
  const coinGeo = new THREE.CylinderGeometry(0.1, 0.1, 0.025, 40);
  const COINS = 18;
  const coins = new THREE.InstancedMesh(coinGeo, new THREE.MeshStandardMaterial({ color: '#F3BA3C', metalness: 1, roughness: 0.2, emissive: '#6B4A00', emissiveIntensity: 0.4 }), COINS);
  coins.castShadow = true;
  scene.add(coins);
  const STREAM = 44;
  const stream = new THREE.InstancedMesh(coinGeo, clay('#C3CCCB', { roughness: 0.5, metalness: 0.4 }), STREAM);
  scene.add(stream);
  const tmp = new THREE.Object3D();

  // ---------------------------------------------------------------- your phone's checks
  // While the payment request is open on your phone, a scan line runs down its screen and
  // what it checked appears beside it.
  // A label in a white pill with a teal check (what your phone checked).
  function badge(text) {
    const c = document.createElement('canvas');
    c.width = 512;
    c.height = 128;
    const g = c.getContext('2d');
    g.fillStyle = '#FFFFFF';
    g.beginPath();
    g.roundRect(8, 8, 496, 112, 56);
    g.fill();
    g.fillStyle = '#E0F3F1';
    g.beginPath();
    g.arc(64, 64, 34, 0, Math.PI * 2);
    g.fill();
    g.strokeStyle = '#00736C';
    g.lineWidth = 9;
    g.lineCap = 'round';
    g.lineJoin = 'round';
    g.beginPath();
    g.moveTo(48, 66);
    g.lineTo(60, 78);
    g.lineTo(82, 52);
    g.stroke();
    g.fillStyle = '#090E0E';
    g.font = '500 52px "DM Sans", sans-serif';
    g.fillText(text, 116, 82);
    const t = new THREE.CanvasTexture(c);
    t.colorSpace = THREE.SRGBColorSpace;
    const sp = new THREE.Sprite(new THREE.SpriteMaterial({ map: t, transparent: true, toneMapped: false }));
    sp.scale.set(0.96, 0.24, 1);
    scene.add(sp);
    return sp;
  }

  const beam = new THREE.Mesh(
    new THREE.PlaneGeometry(SCREEN_W, 0.12),
    new THREE.MeshBasicMaterial({ color: new THREE.Color('#51DDD2').multiplyScalar(2), transparent: true, opacity: 0, toneMapped: false, depthWrite: false, blending: THREE.AdditiveBlending }),
  );
  beam.position.z = PHONE_DEPTH / 2 + 0.01;
  me.phone.add(beam);
  const badges = ['Recipient', 'Amount', 'Fee'].map(badge);

  // The big Z in the floor around the vault, lit for the call to action (read from above).
  const bigZ = new THREE.CurvePath();
  {
    const z = [new THREE.Vector3(-5.6, 0.015, -4.6), new THREE.Vector3(5.6, 0.015, -4.6), new THREE.Vector3(-5.6, 0.015, 4.8), new THREE.Vector3(5.6, 0.015, 4.8)];
    for (let i = 0; i < 3; i++) bigZ.add(new THREE.LineCurve3(z[i], z[i + 1]));
  }
  const bigZMat = new THREE.MeshBasicMaterial({ color: new THREE.Color('#51DDD2').multiplyScalar(1.8), transparent: true, opacity: 0, toneMapped: false });
  const bigZMesh = new THREE.Mesh(new THREE.TubeGeometry(bigZ, 200, 0.16, 8, false), bigZMat);
  bigZMesh.scale.y = 0.2;
  scene.add(bigZMesh);

  // ---------------------------------------------------------------- camera
  const camera = new THREE.PerspectiveCamera(20, 1, 0.5, 200);
  // One continuous take: positions and targets per beat, joined by curves.
  const iso = (target, dist, yaw = 0.55, pitch = 0.5) =>
    [target.clone().add(new THREE.Vector3(Math.sin(yaw) * Math.cos(pitch), Math.sin(pitch), Math.cos(yaw) * Math.cos(pitch)).multiplyScalar(dist)), target];
  const doorCentre = VAULT_POS.clone().add(new THREE.Vector3(0, 0.4 + DOOR / 2, 0));
  const bobFace = PHONES.bob.clone().add(new THREE.Vector3(0, 1.45, 0));
  const meFace = PHONES.me.clone().add(new THREE.Vector3(0, 1.45, 0));
  // In the order the story visits them, so every move goes to the next point on the path.
  const CAM = [
    iso(new THREE.Vector3(0, 1.2, 0), 25, 0.62, 0.4), // 0 hero: the whole diorama
    iso(new THREE.Vector3(0, 1.3, -0.2), 24, 0.4), // 1 shares
    iso(doorCentre.clone(), 17, 0.25, 0.35), // 2 alone: the lock
    iso(bobFace.clone(), 9.5, -0.62, 0.2), // 3 Bob proposes
    iso(new THREE.Vector3(0, 0.8, 0.4), 26, 0.3, 0.75), // 4 the pulse, from above
    // 5 on the way down to the lock (proposing was Bob's approval: nothing more on his phone)
    iso(doorCentre.clone().add(new THREE.Vector3(0, 0.3, 0.6)), 19, 0.12, 0.48),
    iso(doorCentre.clone(), 13, -0.1, 0.25), // 6 his shard seats
    iso(meFace.clone(), 9.5, 0.72, 0.2), // 7 you approve
    iso(doorCentre.clone(), 12, 0.18, 0.22), // 8 yours seats: the key fuses
    iso(doorCentre.clone(), 7.5, 0.12, 0.1), // 9 the door opens: in close
    iso(meFace.clone(), 10.5, 0.78, 0.28), // 10 sending
    iso(new THREE.Vector3(0, 1.1, 0), 30), // 11 sent: back to the diorama
    [new THREE.Vector3(0.8, 44, 12), new THREE.Vector3(0, 0, 0.6)], // 12 from above: the Seam
  ];
  // Portrait screens: the wide shots pull back so the side phones stay in frame.
  let camPos = null;
  let camTarget = null;
  function buildCamera(portrait) {
    // Portrait pulls the wide shots back just enough for the diorama to fit the width
    // (1.55 left it at half the screen on phones, 1.3 cut the edge phones).
    const pos = CAM.map(([p, t]) => (portrait && p.distanceTo(t) > 14 ? t.clone().add(p.clone().sub(t).multiplyScalar(1.42)) : p));
    camPos = new THREE.CatmullRomCurve3(pos, false, 'centripetal');
    camTarget = new THREE.CatmullRomCurve3(CAM.map((c) => c[1]), false, 'centripetal');
  }
  buildCamera(false);
  const cur = { pos: CAM[0][0].clone(), target: CAM[0][1].clone(), frameY: S.frameY, frameX: S.frameX };

  // ---------------------------------------------------------------- baked shadows
  // The way light WebGL sites do it: the key light and everything resting on the floor are
  // still, so their shadows are rendered once, from above, into a texture, and the
  // receivers (floor, plinth, pedestals, channels) darken by it. No
  // shadow map is drawn or sampled per frame (live VSM shadows cost ~12 ms a frame on an
  // Intel UHD laptop). Moving things (key shards, coins) cast nothing. The light's shadow
  // camera only reaches the vault (+-9 units), so the bake covers +-BAKE_R around it.
  const BAKE_R = 13;
  function bakeShadows() {
    const size = mobile ? 1024 : 2048;
    const target = new THREE.WebGLRenderTarget(size, size);
    const top = new THREE.OrthographicCamera(-BAKE_R, BAKE_R, BAKE_R, -BAKE_R, 1, 100);
    top.position.set(0, 50, 0);
    top.up.set(0, 0, -1); // image up is -z: v = (BAKE_R - z) / 2 BAKE_R
    top.lookAt(0, 0, 0);
    const moving = new Set([shards.bob, shards.me, coins]);
    const isMoving = (o) => {
      for (let p = o; p; p = p.parent) if (moving.has(p)) return true;
      return false;
    };
    const hidden = [];
    const receivers = [];
    scene.traverse((o) => {
      if (!o.isMesh) return;
      const keep = (o.castShadow || o.receiveShadow) && !isMoving(o);
      if (!keep && o.visible) {
        o.visible = false;
        hidden.push(o);
      }
      if (o.receiveShadow && o !== doorFrame) receivers.push(o);
    });
    // The view from above, lit, with shadows and without: their ratio is how much light
    // each spot keeps in shade (the paper's texture and the ambient light cancel out).
    // Half-float, so light brighter than white isn't clipped before the division; no fog.
    const views = [0, 1].map(() => new THREE.WebGLRenderTarget(size, size, { type: THREE.HalfFloatType }));
    const background = scene.background;
    const fog = scene.fog;
    scene.background = null;
    scene.fog = null;
    renderer.shadowMap.needsUpdate = true;
    renderer.setRenderTarget(views[0]);
    renderer.render(scene, top);
    key.castShadow = false; // the same view, unshadowed
    renderer.setRenderTarget(views[1]);
    renderer.render(scene, top);
    const ratio = new THREE.Mesh(
      new THREE.PlaneGeometry(2, 2),
      new THREE.ShaderMaterial({
        uniforms: { uLit: { value: views[0].texture }, uFull: { value: views[1].texture } },
        vertexShader: 'varying vec2 vUv; void main() { vUv = uv; gl_Position = vec4(position.xy, 0.0, 1.0); }',
        fragmentShader: `uniform sampler2D uLit, uFull; varying vec2 vUv;
          void main() {
            vec3 w = vec3(0.2126, 0.7152, 0.0722);
            float full = dot(texture2D(uFull, vUv).rgb, w);
            float lit = full > 1e-4 ? dot(texture2D(uLit, vUv).rgb, w) / full : 1.0;
            gl_FragColor = vec4(vec3(clamp(lit, 0.0, 1.0)), 1.0);
          }`,
      }),
    );
    renderer.setRenderTarget(target);
    renderer.render(ratio, top);
    renderer.setRenderTarget(null);
    scene.background = background;
    scene.fog = fog;
    for (const o of hidden) o.visible = true;
    for (const v of views) v.dispose();
    ratio.geometry.dispose();
    ratio.material.dispose();
    // No shadow map from here on.
    renderer.shadowMap.enabled = false;
    key.shadow.dispose();
    const bake = { uBake: { value: target.texture }, uBakeR: { value: BAKE_R } };
    const patched = new Set();
    for (const o of receivers) {
      const m = o.material;
      if (patched.has(m)) continue;
      patched.add(m);
      m.onBeforeCompile = (sh) => {
        Object.assign(sh.uniforms, bake);
        sh.vertexShader = 'uniform float uBakeR;\nvarying vec2 vBakeUv;\n' + sh.vertexShader.replace(
          '#include <begin_vertex>',
          '#include <begin_vertex>\n{ vec4 bw = modelMatrix * vec4(transformed, 1.0); vBakeUv = vec2(bw.x + uBakeR, uBakeR - bw.z) / (2.0 * uBakeR); }',
        );
        sh.fragmentShader = 'uniform sampler2D uBake;\nvarying vec2 vBakeUv;\n' + sh.fragmentShader.replace(
          '#include <opaque_fragment>',
          `{
            vec2 b = vBakeUv;
            float inside = step(0.0, b.x) * step(b.x, 1.0) * step(0.0, b.y) * step(b.y, 1.0);
            outgoingLight *= mix(1.0, texture2D(uBake, clamp(b, 0.0, 1.0)).r, inside);
          }
          #include <opaque_fragment>`,
        );
      };
      m.customProgramCacheKey = () => 'baked-shadow';
      m.needsUpdate = true;
    }
  }
  bakeShadows();

  // ---------------------------------------------------------------- post
  const composer = new EffectComposer(renderer, { multisampling: LEVELS[level].msaa, frameBufferType: THREE.HalfFloatType });
  composer.addPass(new RenderPass(scene, camera));
  const bloom = new BloomEffect({ luminanceThreshold: 0.92, luminanceSmoothing: 0.2, intensity: 1.1, mipmapBlur: true, radius: 0.7 });
  const noise = new NoiseEffect({ blendFunction: BlendFunction.OVERLAY });
  noise.blendMode.opacity.value = 0.12;
  const vignette = new VignetteEffect({ offset: 0.35, darkness: 0.28 });
  composer.addPass(new EffectPass(camera, bloom, noise, vignette));

  // ---------------------------------------------------------------- layout
  let portrait = false;
  function resize() {
    const w = window.innerWidth;
    const h = window.innerHeight;
    renderer.setSize(w, h, false);
    composer.setSize(w, h);
    camera.aspect = w / h;
    portrait = w / h < 0.8;
    camera.fov = portrait ? 30 : 20;
    if (camPos) buildCamera(portrait);
    camera.updateProjectionMatrix();
    settleUntil = performance.now() + 2000;
  }
  resize();
  window.addEventListener('resize', resize);

  // ---------------------------------------------------------------- pointer
  let px = 0;
  let py = 0;
  window.addEventListener('pointermove', (e) => {
    px = e.clientX / window.innerWidth - 0.5;
    py = e.clientY / window.innerHeight - 0.5;
  });
  const tilt = { x: 0, y: 0 };

  // ---------------------------------------------------------------- frame
  const vA = new THREE.Vector3();
  const vB = new THREE.Vector3();
  const vC = new THREE.Vector3();
  const qTmp = new THREE.Quaternion();
  let last = performance.now();
  const t0 = last;
  let running = true;

  function shardPose(shard, owner, progress, lone, time, seatOffset) {
    // Rest: floating above the phone. Flight: an arc to the door. Seated: in the Z.
    const rise = smooth(clamp01(S.shardsUp));
    phoneTop(owner, vA);
    vA.y += Math.sin(time * 1.4 + seatOffset) * 0.06 - (1 - rise) * 0.9;
    // The seat, in world space (door-local origin, pushed in by `insert`).
    door.localToWorld(vB.set(0, 0, 0.62 - 0.24 * S.insert));
    const k = smooth(clamp01(progress));
    shard.position.lerpVectors(vA, vB, k);
    shard.position.y += Math.sin(k * Math.PI) * 1.4;
    if (lone > 0) {
      // A lone shard: flies at the lock, bounces off, drops back.
      const up = Math.sin(clamp01(lone) * Math.PI);
      door.localToWorld(vC.set(0, 0, 0.9));
      shard.position.lerp(vC, up * 0.92);
      shard.position.x += Math.sin(clamp01(lone) * Math.PI * 3) * 0.12 * up;
    }
    // Orientation: spinning gently at rest, squared up with the door when seated.
    door.getWorldQuaternion(qTmp);
    const spin = new THREE.Quaternion().setFromEuler(new THREE.Euler(0.25 * Math.sin(time + seatOffset), time * 0.6 + seatOffset, 0.15));
    shard.quaternion.copy(spin).slerp(qTmp, Math.max(k, Math.sin(clamp01(lone) * Math.PI)));
    const s = rise * (0.82 + 0.18 * k);
    shard.scale.setScalar(Math.max(0.0001, s));
  }

  // Adaptive quality: the median frame time over ~1 s windows (one hitch, such as a shader
  // compiling after a drop or a texture upload, doesn't count); a window under ~57 fps
  // drops to the next cheaper level (TARGET_MS; a 60 Hz screen's 16.7 ms stays). Ignores frames for 2 s after start or a resize and 1 s after a
  // drop, or after the world or the tab was hidden. It never gives up: the showcase is the
  // 3D, so at worst it stays at the lowest level.
  // Two drops in a row that don't make frames faster mean the browser or the screen caps
  // the frame rate (battery saver, 30 fps phones, a 50 Hz screen), not the GPU: go back to
  // where it started and stop adapting, or a capped fast device would sink to the
  // blurriest level for nothing. One unhelpful drop isn't enough: on an Intel UHD laptop
  // fewer MSAA samples changed nothing but a lower resolution did. Very slow windows
  // (under ~30 fps) drop two steps.
  let winMs = 0;
  let winTimes = [];
  const TARGET_MS = 17.5;
  let dropped = null; // { from, avg, misses }: since the last drop that made frames faster
  let capped = false;
  document.addEventListener('visibilitychange', () => {
    settleUntil = performance.now() + 1000;
  });
  function adapt(now, ms) {
    if (still) return; // capture mode keeps the best quality however slow
    if (now < settleUntil) {
      winMs = 0;
      winTimes = [];
      return;
    }
    winMs += Math.min(ms, 1000);
    winTimes.push(ms);
    if (winMs < 1000) return;
    winTimes.sort((a, b) => a - b);
    const avg = winTimes[winTimes.length >> 1]; // the median frame
    winMs = 0;
    winTimes = [];
    if (capped) return;
    if (dropped) {
      if (avg > dropped.avg * 0.9) {
        if (++dropped.misses >= 2) {
          capped = true;
          setLevel(dropped.from, now);
          return;
        }
      } else {
        dropped = null;
      }
    }
    if (avg <= TARGET_MS || level === LEVELS.length - 1) return;
    if (!dropped) dropped = { from: level, avg, misses: 0 };
    setLevel(avg > 33 ? cheaper(cheaper(level)) : cheaper(level), now);
  }

  function setLevel(next, now) {
    level = next;
    renderer.setPixelRatio(LEVELS[level].dpr);
    composer.multisampling = LEVELS[level].msaa;
    composer.setSize(window.innerWidth, window.innerHeight);
    renderer.setSize(window.innerWidth, window.innerHeight, false);
    canvas.dataset.quality = String(level); // for perf checks
    settleUntil = now + 1000;
    remember();
  }

  function frame() {
    if (!running) return;
    const now = performance.now();
    adapt(now, now - last);
    const dt = Math.min(0.05, (now - last) / 1000);
    const camDt = still || cuts ? 1 : dt; // capture mode, reduced motion: the camera lands at once
    last = now;
    const time = (now - t0) / 1000;

    // Camera along the take, damped, with pointer tilt.
    const u = clamp01(S.cam / (CAM.length - 1));
    camPos.getPoint(u, vA);
    camTarget.getPoint(u, vB);
    cur.pos.x = damp(cur.pos.x, vA.x, 5, camDt);
    cur.pos.y = damp(cur.pos.y, vA.y, 5, camDt);
    cur.pos.z = damp(cur.pos.z, vA.z, 5, camDt);
    cur.target.x = damp(cur.target.x, vB.x, 5, camDt);
    cur.target.y = damp(cur.target.y, vB.y, 5, camDt);
    cur.target.z = damp(cur.target.z, vB.z, 5, camDt);
    tilt.x = damp(tilt.x, px, 3, dt);
    tilt.y = damp(tilt.y, py, 3, dt);
    camera.position.copy(cur.pos);
    camera.position.x += tilt.x * 0.9;
    camera.position.y -= tilt.y * 0.6;
    camera.lookAt(cur.target);
    // The hero leaves the top of the screen to the headline: shift the frame down.
    cur.frameY = damp(cur.frameY, portrait ? S.frameY + 0.26 * S.frameX / 0.2 + S.portraitY : S.frameY, 4, camDt);
    cur.frameX = damp(cur.frameX, portrait ? 0 : S.frameX, 4, camDt);
    const fw = renderer.domElement.clientWidth;
    const fh = renderer.domElement.clientHeight;
    camera.setViewOffset(fw, fh, -fw * cur.frameX, -fh * cur.frameY, fw, fh);
    // Haze relative to what the camera looks at, so far framings (from above) aren't fogged out.
    const look = cur.pos.distanceTo(cur.target);
    scene.fog.near = Math.max(30, look + 5);
    scene.fog.far = scene.fog.near + 30;

    // Phones face the camera (yaw only), float a little.
    for (const [i, p] of [bob, me, cara].entries()) {
      p.root.getWorldPosition(vC);
      const yaw = Math.atan2(camera.position.x - vC.x, camera.position.z - vC.z);
      p.phone.rotation.y = damp(p.phone.rotation.y, yaw, 4, dt);
      p.phone.position.y = PEDESTAL_H + BODY_H / 2 + 0.06 + Math.sin(time * 0.9 + i * 2) * 0.03;
      p.phone.rotation.z = Math.sin(time * 0.6 + i) * 0.015;
    }
    bob.root.updateMatrixWorld(true);
    me.root.updateMatrixWorld(true);
    cara.root.updateMatrixWorld(true);

    // Screens: only uniforms change (screen-shader.js), nothing is uploaded.
    {
      // Bob: Home, review with "Propose payment", his proposal awaiting one more approval.
      const k = Math.min(1.999, S.aPush1 + S.aPush2);
      const i = Math.floor(k);
      const u = bob.u;
      u.uA.value = BOB_SCREENS[i];
      u.uB.value = BOB_SCREENS[i + 1];
      u.uT.value = S.aPush1 + S.aPush2 >= 2 ? 1 : k - i;
      u.uBan1B.value = S.sentBanner;
      u.uTap1.value.set(SPOT.newPayment[0], SPOT.newPayment[1], S.tapA1, 0);
      u.uTap2.value.set(SPOT.propose[0], SPOT.propose[1], S.tapA2, 0);
    }
    {
      // You: Home, the banner, the payment request sheet, sending, sent.
      const u = me.u;
      const pushing = S.bPush > 0;
      u.uMode.value = pushing ? MODE.push : MODE.sheet;
      u.uA.value = pushing ? shots.meRequest : shots.meHome;
      u.uB.value = pushing ? shots.meSending : shots.meRequest;
      u.uT.value = pushing ? S.bPush : S.bSheet;
      u.uCmix.value = pushing ? S.bDone : 0;
      const sending = clamp01((S.bPush - 0.5) * 2);
      u.uSending.value = sending;
      u.uCoins.value = sending * (1 - 0.6 * S.bDone);
      u.uDone.value = S.bPush > 0.5 ? S.bDone : 0;
      u.uTime.value = time;
      u.uBan1B.value = S.banner;
      u.uBan2B.value = S.sentBanner * (1 - S.bDone);
      u.uTap1.value.set(SPOT.banner[0], SPOT.banner[1], S.tapB1, 0);
      u.uTap2.value.set(SPOT.approve[0], SPOT.approve[1], S.tapB2, 0);
    }
    // Cara: Home with the same banners. She isn't needed this time.
    cara.u.uBan1B.value = S.banner;
    cara.u.uBan2B.value = S.sentBanner;
    cara.u.uDim.value = S.caraDim;

    // Vault: the leak, the ghost Z, the door opening along the Z and closing again.
    const open = smooth(clamp01(S.open)) * (1 - smooth(clamp01(S.closeDoor)));
    leaves[0].position.set(0.95 * open, 0.75 * open, 0.25 * open);
    leaves[0].rotation.set(-0.2 * open, 0.35 * open, -0.12 * open);
    leaves[1].position.set(-0.95 * open, -0.5 * open, 0.35 * open);
    leaves[1].rotation.set(0.15 * open, -0.35 * open, 0.1 * open);
    const leak = S.leak * (0.85 + 0.15 * Math.sin(time * 2)) + 5 * open + 1.5 * S.fuse;
    glowMat.color.setScalar(0.55 + 0.35 * leak);
    glowMat.opacity = 1 - 0.85 * open;
    rayMat.opacity = 0.75 * open * (0.85 + 0.15 * Math.sin(time * 2));
    vaultLight.intensity = 14 * open + 1.5 * S.fuse;
    ghostMat.opacity = clamp01(S.ghost) * (0.6 + 0.4 * Math.sin(time * 3)) * (1 - clamp01(S.fuse));

    // Shards.
    shardPose(shards.bob, bob, S.shardA, 0, time, 0);
    shardPose(shards.me, me, S.shardB, 0, time, 2.1);
    shardPose(shards.cara, cara, 0, S.lone, time, 4.2);
    // Fused: gold, seated, pushed in; hidden when the door opens.
    const gold = clamp01(S.fuse);
    for (const sh of [shards.bob, shards.me]) {
      sh.traverse((m) => {
        if (!m.isMesh) return;
        if (gold > 0.5 && m.material !== goldKey) m.material = goldKey;
        if (gold <= 0.5 && m.material === goldKey) m.material = sh === shards.bob ? shards.bobMat : shards.meMat;
      });
      sh.visible = open < 0.35;
    }
    goldKey.emissiveIntensity = 0.3 + 2.2 * Math.sin(clamp01(S.fuse) * Math.PI);

    // Floor pulse from Bob to the others.
    const pu = clamp01(S.pulse);
    pulses.forEach((m, i) => {
      m.visible = pu > 0 && pu < 1;
      if (m.visible) paths[i].getPointAt(pu, m.position);
    });

    // Coins: from the open vault into your phone's slot.
    coins.visible = S.coins > 0 && S.coins < 1;
    if (coins.visible) {
      door.localToWorld(vA.set(0, 0, 0.6));
      screenPoint(me, SPOT.slot[0], SPOT.slot[1], vB);
      for (let i = 0; i < COINS; i++) {
        const k = clamp01(S.coins * 1.7 - (i / COINS) * 0.7);
        const e = smooth(k);
        tmp.position.lerpVectors(vA, vB, e);
        tmp.position.y += Math.sin(e * Math.PI) * (1.3 + (i % 4) * 0.2);
        tmp.position.x += Math.sin(i * 12.9) * 0.25 * Math.sin(e * Math.PI);
        tmp.rotation.set(Math.PI / 2 + time * 4 + i, time * 3 + i, 0);
        tmp.scale.setScalar(k > 0 && k < 1 ? 1 - 0.5 * e : 0.0001);
        tmp.updateMatrix();
        coins.setMatrixAt(i, tmp.matrix);
      }
      coins.instanceMatrix.needsUpdate = true;
    }

    // The stream: identical payments crossing behind the diorama; yours joins it.
    stream.visible = S.stream > 0;
    if (stream.visible) {
      for (let i = 0; i < STREAM; i++) {
        const k = (time * 0.035 + i / STREAM) % 1;
        tmp.position.set(-16 + 32 * k, 2.6 + Math.sin(i * 7.1) * 0.5, -7.5 + Math.sin(i * 3.3) * 0.8);
        tmp.rotation.set(Math.PI / 2, 0, time + i);
        tmp.scale.setScalar(clamp01(S.stream) * (0.9 + 0.2 * Math.sin(i)));
        tmp.updateMatrix();
        stream.setMatrixAt(i, tmp.matrix);
      }
      stream.instanceMatrix.needsUpdate = true;
    }

    // Your phone's checks: the scan line while the request is open, then the badges, which
    // stay until you've approved.
    const ck = clamp01(S.check);
    beam.material.opacity = 0.7 * Math.sin(Math.PI * ck);
    beam.visible = ck > 0 && ck < 1;
    beam.position.y = (0.5 - ((time * 0.35) % 1)) * SCREEN_H;
    const gone = smooth(clamp01(S.check - 1));
    badges.forEach((b, i) => {
      const k = smooth(clamp01(ck * 3 - i * 0.6)) * (1 - gone);
      // Beside the phone; in portrait (no room beside it) over the empty middle of its
      // screen, between Details and Approve, overhanging its right edge.
      if (portrait) me.phone.localToWorld(vA.set(0.45, -0.1 - i * 0.19, 0.25));
      else me.phone.localToWorld(vA.set(0.95, 0.42 - i * 0.34, 0.25));
      b.position.copy(vA);
      b.position.y += Math.sin(time * 1.3 + i) * 0.04;
      const size = portrait ? 0.8 : 1;
      b.scale.set(0.96 * size * k, 0.24 * size * k, 1);
      b.visible = k > 0.01;
    });
    bigZMat.opacity = clamp01(S.ctaZ) * (0.75 + 0.25 * Math.sin(time * 2));

    if (LEVELS[level].post) composer.render(dt);
    else renderer.render(scene, camera); // the lowest levels draw straight to the screen
  }

  // ---------------------------------------------------------------- the timeline
  shards.bobMat = shards.bob.children[0].material;
  shards.meMat = shards.me.children[0].material;

  const tl = gsap.timeline({
    defaults: { ease: 'power2.inOut' },
    scrollTrigger: { trigger: story, start: 'top bottom', end: 'bottom bottom', scrub: true },
  });
  const at = (vars, pos, duration = 3, ease) => tl.to(S, { ...vars, duration, ...(ease ? { ease } : {}) }, pos);
  // Beats (timeline units; captions switch at BEATS below).
  at({ cam: 1, frameX: 0 }, 0, 8); // hero -> shares
  at({ shardsUp: 1, ghost: 1 }, 3, 5, 'back.out(1.4)');
  at({ cam: 2 }, 9, 6); // the lock
  at({ lone: 1 }, 11, 6, 'none'); // a lone shard bounces off
  at({ cam: 3 }, 17, 6); // Bob
  at({ tapA1: 1 }, 23, 2, 'none');
  at({ aPush1: 1 }, 24.5, 2.5);
  at({ tapA2: 1 }, 28, 2, 'none');
  at({ aPush2: 1 }, 29.5, 2.5);
  at({ cam: 4 }, 32, 5); // from above: the pulse
  at({ pulse: 1 }, 33, 5, 'power1.inOut');
  at({ banner: 1 }, 37, 2, 'back.out(1.4)');
  at({ cam: 5 }, 38, 4); // down toward the lock: his share is already on its way
  at({ shardA: 1, cam: 6 }, 46, 6); // his shard seats; watch it at the door
  at({ cam: 7 }, 53, 6); // you
  at({ tapB1: 1 }, 59, 2, 'none');
  at({ banner: 0 }, 60.5, 1.5);
  at({ bSheet: 1 }, 60.5, 3, 'power3.out');
  at({ check: 1 }, 63.5, 4, 'none'); // your phone checks it: scan, then the badges
  at({ tapB2: 1 }, 71.5, 2, 'none');
  at({ caraDim: 1 }, 73, 3);
  at({ check: 2 }, 73, 2); // approved: the badges go
  at({ cam: 8 }, 73, 5); // your shard seats: 2 of 3
  at({ shardB: 1 }, 73.5, 5);
  at({ fuse: 1 }, 79, 3, 'none'); // the key, gold
  at({ insert: 1 }, 81, 2);
  at({ cam: 9, open: 1 }, 83, 6, 'power3.inOut'); // the door splits; dive
  at({ cam: 10 }, 90, 6); // your phone
  at({ bPush: 1 }, 90, 3);
  at({ coins: 1 }, 90, 8, 'none');
  at({ bDone: 1, sentBanner: 1 }, 99, 3);
  at({ cam: 11, stream: 1, closeDoor: 1 }, 102, 8);

  // Captions: shares, the lock, Bob, notified, his share, your checks, you approve, the key,
  // the vault opens, sending, sent (each where its moment starts).
  const BEATS = [0, 9, 17, 32, 46, 58, 69, 79, 83, 90, 99];
  let beat = -1;
  tl.eventCallback('onUpdate', () => {
    const t = tl.time();
    let n = 0;
    for (let i = 0; i < BEATS.length; i++) if (t >= BEATS[i]) n = i;
    if (n !== beat) {
      beat = n;
      story.dataset.beat = String(n);
    }
  });
  story.dataset.beat = '0';

  // The call to action: from the sent payment up above the vault; the floor Z lights;
  // then a hold. Its own scrubbed section (220vh): 10.8 units of move, 3.6 of hold.
  const ct = gsap.timeline({
    defaults: { ease: 'power2.inOut', immediateRender: false },
    scrollTrigger: { trigger: cta, start: 'top bottom', end: 'bottom bottom', scrub: true },
  });
  ct.fromTo(S, { cam: 11, frameY: 0.02, portraitY: 0, ctaZ: 0 }, { cam: 12, frameX: 0, frameY: -0.1, portraitY: -0.06, ctaZ: 1, duration: 10.8 }, 0);
  ct.to({}, { duration: 3.6 }, 10.8);

  // Render unless an opaque section (FAQ, footer) covers the whole screen: from its top at
  // the top of the screen to its bottom at the bottom. ScrollTrigger caches the positions
  // (recomputed on resize), so this reads no layout per frame.
  const coverTriggers = covers.map((el) => ScrollTrigger.create({ trigger: el, start: 'top top', end: 'bottom bottom' }));
  const covered = () => coverTriggers.some((t) => t.isActive);
  const loop = () => {
    const was = running;
    running = !covered();
    if (running && !was) settleUntil = performance.now() + 1000;
    frame();
  };
  // Warm up: compile every material now (hidden ones too, and the gold the key shards turn
  // into mid-story), off the main thread where the browser can, so nothing compiles while
  // the page scrolls (a first-use compile was an 80-100 ms hitch on an Intel UHD laptop).
  const warm = new THREE.Mesh(new THREE.BoxGeometry(0.01, 0.01, 0.01), goldKey);
  warm.visible = false;
  scene.add(warm);
  await renderer.compileAsync(scene, camera);
  scene.remove(warm);
  warm.geometry.dispose();
  // And every texture goes to the GPU now, not when its object first appears (a sprite's
  // first draw was a 55 ms hitch).
  scene.traverse((o) => {
    for (const m of [o.material].flat()) {
      if (m) for (const v of Object.values(m)) if (v?.isTexture) renderer.initTexture(v);
    }
  });
  gsap.ticker.add(loop);
  document.documentElement.classList.add('world-on');
  ScrollTrigger.refresh();

  return { S, hero };
}
