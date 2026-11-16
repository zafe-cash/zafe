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

const params = new URLSearchParams(location.search);
const still = params.has('still');
const forceWorld = still || params.get('world') === 'always';
if (still) document.documentElement.classList.add(params.get('still') === 'page' ? 'still-page' : 'still-capture');

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
const showStills = () => {
  document.documentElement.classList.remove('world-on');
  document.documentElement.classList.add('stills');
};
const gl = canvas && story && cta ? webgl() : '';
if (gl) {
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
