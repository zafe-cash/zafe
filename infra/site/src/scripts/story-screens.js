// The phones' screens: the app's real renders (public/assets/screens/story_*), composited
// into a 2D canvas per phone the way the app moves between them (push, sheet from below,
// scroll to the button), plus what the OS and the scene add on top: taps, the
// notification banner, coins falling from the sending screen's slot. The canvases are
// textures on the 3D phones (story-scene.js).

export const W = 780; // render width (390 logical at 2x)
export const H = 1688; // a phone viewport (844 logical)

const SRC = '/assets/screens/';
const NAMES = {
  bobHome: 'story_bob_home_dark',
  bobPropose: 'story_bob_propose_dark',
  bobApprove: 'story_bob_approve_dark',
  bobApproved: 'story_bob_approved_dark',
  meHome: 'story_me_home_light',
  meReview: 'story_me_review_light',
  meSending: 'story_me_sending_light',
  meDone: 'story_me_done_light',
};

const NEEDS_YOU = 'Ops fund: payment needs your approval';
const SENT = 'Ops fund: payment sent';
const SENT_BODY = '12.50 ZEC, approved by Bob and you.';

// Where things are on the renders, as fractions of the viewport (W x H).
export const SPOT = {
  newPayment: [0.265, 0.366],
  propose: [0.5, 0.758],
  approve: [0.5, 0.834], // on the approve screens once scrolled to the bottom
  banner: [0.5, 0.075],
  slot: [0.5, 0.127], // the sending screen's coin slot
  status: [0.5, 0.377], // the sending screen's spinner / check
};

// The Seam mark (public/assets/zafe.svg), drawn as paths so the canvas stays untainted.
const MARK_UPPER = new Path2D('M22 26.66Q22 22 26.66 22L71.6 22Q78 22 78 28.4L78 56.95Q78 58.25 76.7 58.25L49.77 58.25Q48.47 58.25 49.39 57.33L73.55 33.17Q74.47 32.25 73.17 32.25L23.3 32.25Q22 32.25 22 30.95Z');
const MARK_LOWER = new Path2D('M22 43.05Q22 41.75 23.3 41.75L50.23 41.75Q51.53 41.75 50.61 42.67L26.45 66.83Q25.53 67.75 26.83 67.75L76.7 67.75Q78 67.75 78 69.05L78 73.34Q78 78 73.34 78L28.4 78Q22 78 22 71.6Z');

export function drawMark(g, x, y, size) {
  g.save();
  g.translate(x, y);
  g.scale(size / 100, size / 100);
  roundRect(g, 0, 0, 100, 100, 22);
  g.fillStyle = '#004A46';
  g.fill();
  g.fillStyle = '#D9F5F2';
  g.fill(MARK_UPPER);
  g.fillStyle = '#51DDD2';
  g.fill(MARK_LOWER);
  g.restore();
}

export function roundRect(g, x, y, w, h, r) {
  g.beginPath();
  g.moveTo(x + r, y);
  g.arcTo(x + w, y, x + w, y + h, r);
  g.arcTo(x + w, y + h, x, y + h, r);
  g.arcTo(x, y + h, x, y, r);
  g.arcTo(x, y, x + w, y, r);
  g.closePath();
}

export async function loadScreens() {
  const images = {};
  await Promise.all(
    Object.entries(NAMES).map(
      ([key, name]) =>
        new Promise((resolve, reject) => {
          const img = new Image();
          img.decoding = 'async';
          img.onload = () => resolve();
          img.onerror = reject;
          img.src = `${SRC}${name}.webp`;
          images[key] = img;
        }),
    ),
  );
  if (document.fonts) {
    await Promise.all([
      document.fonts.load('500 30px "DM Sans"'),
      document.fonts.load('400 28px "DM Sans"'),
    ]);
  }
  return images;
}

export function makeCanvas() {
  const c = document.createElement('canvas');
  c.width = W;
  c.height = H;
  return c;
}

const clamp01 = (v) => Math.min(1, Math.max(0, v));

// An image at full width, scrolled `scroll` (0..1) of the way to its bottom.
function drawFull(g, img, x = 0, y = 0, scroll = 0) {
  const h = (img.height * W) / img.width;
  g.drawImage(img, x, y - scroll * Math.max(0, h - H), W, h);
}

// Navigation push: `from` slides a little left and darkens, `to` comes in from the right.
function push(g, drawFrom, drawTo, t, dark) {
  if (t <= 0) return drawFrom(0);
  if (t >= 1) return drawTo(0);
  drawFrom(-0.28 * W * t);
  g.fillStyle = dark ? `rgba(0,0,0,${0.5 * t})` : `rgba(9,14,14,${0.18 * t})`;
  g.fillRect(0, 0, W, H);
  g.save();
  g.shadowColor = 'rgba(0,0,0,0.35)';
  g.shadowBlur = 40;
  g.shadowOffsetX = -10;
  drawTo(W * (1 - t));
  g.restore();
}

// A fingertip: a pressed disc and a ring that spreads, over t = 0..1.
function tap(g, [fx, fy], t, dark) {
  if (t <= 0 || t >= 1) return;
  const x = fx * W;
  const y = fy * H;
  const press = t < 0.35 ? t / 0.35 : 1;
  const fade = t < 0.75 ? 1 : 1 - (t - 0.75) / 0.25;
  g.save();
  g.globalAlpha = fade;
  g.fillStyle = dark ? 'rgba(255,255,255,0.22)' : 'rgba(9,14,14,0.16)';
  g.beginPath();
  g.arc(x, y, 46 - 8 * press, 0, Math.PI * 2);
  g.fill();
  g.lineWidth = 4;
  g.strokeStyle = dark ? 'rgba(255,255,255,0.8)' : 'rgba(255,255,255,0.95)';
  g.stroke();
  if (t > 0.3) {
    const r = (t - 0.3) / 0.7;
    g.globalAlpha = (1 - r) * 0.6;
    g.lineWidth = 6;
    g.strokeStyle = dark ? '#51DDD2' : '#00736C';
    g.beginPath();
    g.arc(x, y, 40 + 140 * r, 0, Math.PI * 2);
    g.stroke();
  }
  g.restore();
}

// The OS notification banner, dropping in from the top (b = 0..1).
function banner(g, b, title, body) {
  if (b <= 0) return;
  const y = 22 - (1 - b) * 260;
  g.save();
  g.globalAlpha = clamp01(b * 1.5);
  g.shadowColor = 'rgba(0,40,38,0.35)';
  g.shadowBlur = 40;
  g.shadowOffsetY = 14;
  roundRect(g, 20, y, W - 40, 184, 40);
  g.fillStyle = 'rgba(255,255,255,0.98)';
  g.fill();
  g.shadowColor = 'transparent';
  drawMark(g, 48, y + 26, 40);
  g.fillStyle = '#55615F';
  g.font = '400 24px "DM Sans", sans-serif';
  g.fillText('Zafe', 102, y + 55);
  g.textAlign = 'right';
  g.fillText('now', W - 52, y + 55);
  g.textAlign = 'left';
  g.fillStyle = '#090E0E';
  g.font = '500 30px "DM Sans", sans-serif';
  g.fillText(title, 48, y + 112);
  g.fillStyle = '#55615F';
  g.font = '400 28px "DM Sans", sans-serif';
  g.fillText(body, 48, y + 152);
  g.restore();
}

// Coins leaving the sending screen's slot along its two dotted trails (time-based).
function slotCoins(g, time, amount) {
  if (amount <= 0) return;
  const [sx, sy] = SPOT.slot;
  const trails = [
    [0.08, 0.36],
    [0.92, 0.33],
  ];
  g.save();
  g.beginPath();
  g.rect(0, 0, W, H * 0.5);
  g.clip();
  for (let i = 0; i < 10; i++) {
    const k = (time * 0.45 + i / 10) % 1;
    const [ex, ey] = trails[i % 2];
    const x = (sx + (ex - sx) * k) * W;
    const y = (sy + (ey - sy) * k + 0.05 * Math.sin(k * Math.PI)) * H;
    const spin = Math.abs(Math.cos(time * 5 + i));
    g.globalAlpha = amount * Math.sin(k * Math.PI);
    g.fillStyle = '#F3BA3C';
    g.strokeStyle = '#B8830F';
    g.lineWidth = 3;
    g.beginPath();
    g.ellipse(x, y, 18 * (0.35 + 0.65 * spin), 18, 0.3, 0, Math.PI * 2);
    g.fill();
    g.stroke();
  }
  g.restore();
}

// A spinning arc around the sending spinner, and the success ripple after.
function status(g, time, sending, done) {
  const [fx, fy] = SPOT.status;
  const x = fx * W;
  const y = fy * H;
  if (sending > 0 && done < 1) {
    g.save();
    g.globalAlpha = sending * (1 - done);
    g.lineWidth = 7;
    g.lineCap = 'round';
    g.strokeStyle = '#51DDD2';
    const a = time * 4;
    g.beginPath();
    g.arc(x, y, 84, a, a + 1.4);
    g.stroke();
    g.restore();
  }
  if (done > 0 && done < 1) {
    g.save();
    for (const lag of [0, 0.25]) {
      const r = clamp01((done - lag) / 0.75);
      g.globalAlpha = (1 - r) * 0.55;
      g.lineWidth = 6;
      g.strokeStyle = '#C98F14';
      g.beginPath();
      g.arc(x, y, 66 + 150 * r, 0, Math.PI * 2);
      g.stroke();
    }
    g.restore();
  }
}

// Bob's phone (dark): Home, review with "Propose payment", his proposal, approved.
export function drawBob(g, img, s) {
  g.clearRect(0, 0, W, H);
  g.fillStyle = '#080B0B';
  g.fillRect(0, 0, W, H);
  const layers = [
    (x) => drawFull(g, img.bobHome, x),
    (x) => drawFull(g, img.bobPropose, x),
    (x) => drawFull(g, img.bobApprove, x, 0, s.aScroll),
    (x) => drawFull(g, img.bobApproved, x),
  ];
  const k = Math.min(2.999, s.aPush1 + s.aPush2 + s.aPush3);
  const i = Math.floor(k);
  push(g, layers[i], layers[i + 1], s.aPush1 + s.aPush2 + s.aPush3 >= 3 ? 1 : k - i, true);
  banner(g, s.sentBanner, SENT, SENT_BODY);
  tap(g, SPOT.newPayment, s.tapA1, true);
  tap(g, SPOT.propose, s.tapA2, true);
  tap(g, SPOT.approve, s.tapA3, true);
}

// Your phone (light): Home, the banner, the review sheet, sending, sent.
export function drawMe(g, img, s, time) {
  g.clearRect(0, 0, W, H);
  g.fillStyle = '#F1F5F5';
  g.fillRect(0, 0, W, H);
  const review = (x) => drawFull(g, img.meReview, x, 0, s.bScroll);
  const sending = (x) => {
    drawFull(g, img.meSending, x);
    if (s.bDone > 0) {
      g.save();
      g.globalAlpha = s.bDone;
      drawFull(g, img.meDone, x);
      g.restore();
    }
  };
  if (s.bPush > 0) {
    push(g, review, sending, s.bPush, false);
  } else {
    drawFull(g, img.meHome);
    if (s.bSheet > 0) {
      g.fillStyle = `rgba(9,14,14,${0.35 * s.bSheet})`;
      g.fillRect(0, 0, W, H);
      g.save();
      const y = H * (1 - s.bSheet);
      g.shadowColor = 'rgba(0,0,0,0.3)';
      g.shadowBlur = 50;
      roundRect(g, 0, y, W, H + 80, s.bSheet < 1 ? 56 : 0);
      g.clip();
      g.translate(0, y);
      review(0);
      g.restore();
    }
  }
  if (s.bPush > 0.5) {
    slotCoins(g, time, clamp01((s.bPush - 0.5) * 2) * (1 - 0.6 * s.bDone));
    status(g, time, clamp01((s.bPush - 0.5) * 2), s.bDone);
  }
  banner(g, s.banner, NEEDS_YOU, 'Bob proposed 12.50 ZEC.');
  banner(g, s.sentBanner * (1 - s.bDone), SENT, SENT_BODY);
  tap(g, SPOT.banner, s.tapB1, false);
  tap(g, SPOT.approve, s.tapB2, false);
}

// Cara's phone (light): Home with the same banners. She isn't needed this time.
export function drawCara(g, img, s) {
  g.clearRect(0, 0, W, H);
  drawFull(g, img.meHome);
  banner(g, s.banner, NEEDS_YOU, 'Bob proposed 12.50 ZEC.');
  banner(g, s.sentBanner, SENT, SENT_BODY);
  if (s.caraDim > 0) {
    g.fillStyle = `rgba(9,14,14,${0.45 * s.caraDim})`;
    g.fillRect(0, 0, W, H);
  }
}
