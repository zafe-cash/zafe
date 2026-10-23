// Pointer and text motion (docs/site.md, research notes on cursors and text).
//
// Pointer: the cursor itself is CSS (site.css, drawn by the OS so it never lags). On fine
// pointers without reduced motion a "Get Zafe" pill leans toward the pointer.
//
// Text: headings rise word by word from a mask when they appear; two "unshield"
// scrambles (the proposal caption, the encrypted chain fields). All of it sets
// transforms and text through the DOM APIs, which the CSP allows (no inline styles in
// markup, no <style> elements).
import gsap from 'gsap';
import { ScrollTrigger } from 'gsap/ScrollTrigger';

gsap.registerPlugin(ScrollTrigger);

const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
// ?pointer=fine forces the pill lean on (headless browsers report no fine pointer).
const fine = window.matchMedia('(pointer: fine)').matches || new URLSearchParams(location.search).get('pointer') === 'fine';

// ------------------------------------------------------------------ text: word rise

function splitWords(el) {
  if (el.dataset.split) return el.querySelectorAll('.wi');
  el.dataset.split = '1';
  const nodes = [...el.childNodes];
  el.textContent = '';
  for (const node of nodes) {
    if (node.nodeType !== Node.TEXT_NODE) {
      el.appendChild(node);
      continue;
    }
    const parts = node.textContent.split(/(\s+)/);
    for (const part of parts) {
      if (!part) continue;
      if (/^\s+$/.test(part)) {
        el.appendChild(document.createTextNode(' '));
        continue;
      }
      const w = document.createElement('span');
      w.className = 'w';
      const inner = document.createElement('span');
      inner.className = 'wi';
      inner.textContent = part;
      w.appendChild(inner);
      el.appendChild(w);
    }
  }
  return el.querySelectorAll('.wi');
}

function rise(el) {
  if (!el || reduced) return;
  const words = splitWords(el);
  gsap.fromTo(words, { yPercent: 115 }, { yPercent: 0, duration: 0.9, ease: 'power4.out', stagger: 0.06, overwrite: true });
}

// ------------------------------------------------------------------ text: unshield

const GLYPHS = 'abcdef0123456789';
function scramble(el, duration = 0.8) {
  if (!el || reduced) return;
  const final = el.dataset.text || el.textContent;
  el.dataset.text = final;
  const state = { p: 0 };
  gsap.fromTo(
    state,
    { p: 0 },
    {
      p: 1,
      duration,
      ease: 'none',
      overwrite: true,
      onUpdate() {
        const n = Math.floor(final.length * state.p);
        let out = final.slice(0, n);
        for (let i = n; i < final.length; i++) out += final[i] === ' ' ? ' ' : GLYPHS[(Math.random() * GLYPHS.length) | 0];
        el.textContent = out;
      },
      onComplete() {
        el.textContent = final;
      },
    },
  );
}

// Story: the proposal caption decrypts into place; at "Sent" the chain fields unshield.
const story = document.querySelector('.story');
if (story) {
  new MutationObserver(() => {
    if (story.dataset.beat === '2') scramble(story.querySelector('.k2'), 0.7);
    if (story.dataset.beat === '10') story.querySelectorAll('.chain-mini dd').forEach((dd, i) => gsap.delayedCall(i * 0.08, () => scramble(dd, 0.9)));
  }).observe(story, { attributes: true, attributeFilter: ['data-beat'] });
}

// Other headings rise once as they enter.
for (const el of document.querySelectorAll('.cta-card h2, .faq h2')) {
  ScrollTrigger.create({ trigger: el, start: 'top 85%', once: true, onEnter: () => rise(el) });
}

// ------------------------------------------------------------------ pills lean

if (fine && !reduced) {
  for (const p of document.querySelectorAll('.pill')) {
    p.addEventListener('pointermove', (e) => {
      const r = p.getBoundingClientRect();
      const x = (e.clientX - (r.left + r.width / 2)) * 0.22;
      const y = (e.clientY - (r.top + r.height / 2)) * 0.3;
      gsap.to(p, { x, y, duration: 0.4, ease: 'power3.out', overwrite: 'auto' });
    });
    p.addEventListener('pointerleave', () => gsap.to(p, { x: 0, y: 0, duration: 0.6, ease: 'elastic.out(1, 0.5)' }));
  }
}
