// Page motion. One loop: Lenis (smooth scroll) is advanced by GSAP's ticker and tells
// ScrollTrigger about every scroll, so the 3D world (world.js) and the page never drift
// apart. The showcase is the 3D, so the world runs wherever WebGL exists: on a software
// renderer at its lowest quality, with reduced motion as cuts between beats instead of
// camera flights. Only a browser without WebGL (or a failed load) gets the stills.
import Lenis from 'lenis';
import gsap from 'gsap';
import { ScrollTrigger } from 'gsap/ScrollTrigger';
import './ui.js';
import { loadScreens } from './story-screens.js';

gsap.registerPlugin(ScrollTrigger);
ScrollTrigger.config({ ignoreMobileResize: true });

const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches;

if (!reduced) {
  const lenis = new Lenis({ autoRaf: false, anchors: true, lerp: 0.085 });
  lenis.on('scroll', ScrollTrigger.update);
  gsap.ticker.add((time) => lenis.raf(time * 1000));
  gsap.ticker.lagSmoothing(0);
}

// ?world=always runs a software renderer at full quality, as on a GPU (headless checks
// render WebGL in software). ?still is the capture mode for the fallback stills (stills.sh):
// the world at full quality, the camera landing at once, the page's text hidden
// (?still=page keeps it, for the social image).
const params = new URLSearchParams(location.search);
const still = params.has('still');
const forceWorld = still || params.get('world') === 'always';
if (still) document.documentElement.classList.add(params.get('still') === 'page' ? 'still-page' : 'still-capture');

// 'gpu', 'software' (no GPU, or hardware acceleration off: the world starts at its lowest
// quality) or '' (no WebGL at all: the stills).
function webgl() {
  try {
    const c = document.createElement('canvas');
    const gl = c.getContext('webgl2') || c.getContext('webgl');
    if (!gl) return '';
    const info = gl.getExtension('WEBGL_debug_renderer_info');
    const name = info ? String(gl.getParameter(info.UNMASKED_RENDERER_WEBGL)) : '';
    gl.getExtension('WEBGL_lose_context')?.loseContext();
    return /swiftshader|llvmpipe|softpipe|software|basic render/i.test(name) && !forceWorld ? 'software' : 'gpu';
  } catch {
    return '';
  }
}

const canvas = document.querySelector('canvas.world');
const story = document.querySelector('.story');
const hero = document.querySelector('.hero');
const cta = document.querySelector('.cta-world');
const covers = [...document.querySelectorAll('.faq, .foot')];
// html.stills shows the fallback (rendered stills of the world); boot.js hides it on
// every page with JS until this decides, so the world's visitors never fetch them.
const showStills = () => {
  document.documentElement.classList.remove('world-on');
  document.documentElement.classList.add('stills');
};
const gl = canvas && story && cta ? webgl() : '';
if (gl) {
  // Fetch the app screens in parallel with the world's module, not after it.
  const screens = loadScreens();
  import('./world.js')
    .then((m) => m.start({ canvas, story, hero, cta, covers, screens, still, software: gl === 'software', cuts: reduced }))
    .catch((err) => {
      showStills();
      console.warn('world:', err);
    });
} else {
  showStills();
}
