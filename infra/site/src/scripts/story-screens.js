// The phones' screens: the app's real renders (public/assets/screens/story_*) and the OS
// notification banners. They go to the GPU once; screen-shader.js composites them the way
// the app moves between them (push, sheet from below) and draws what the OS and the scene
// add on top (taps, coins from the sending screen's slot), so nothing is re-uploaded while
// the page scrolls (a 780 x 1688 upload took ~80 ms on an Intel UHD laptop).

export const W = 780; // render width (390 logical at 2x)
export const H = 1688; // a phone viewport (844 logical)

const SRC = '/assets/screens/';
const NAMES = {
  bobHome: 'story_bob_home_dark',
  bobPropose: 'story_bob_propose_dark',
  bobApproved: 'story_bob_approved_dark',
  meHome: 'story_me_home_light',
  meRequest: 'story_me_request_light',
  meSending: 'story_me_sending_light',
  meDone: 'story_me_done_light',
};

// The app's own notifications (app/lib/src/notifications/vault_updates.dart).
const NEEDS_YOU = 'Ops fund: payment needs your approval';
const NEEDS_YOU_BODY = 'Bob proposed 12.5 ZEC to u1k3v8m .... f6d8s0a.';
const SENT = 'Ops fund: payment sent';
const SENT_BODY = '12.5 ZEC to u1k3v8m .... f6d8s0a';

// Where things are on the renders, as fractions of the viewport (W x H).
export const SPOT = {
  newPayment: [0.264, 0.442], // Home's "New payment" (below the balance card)
  propose: [0.5, 0.758],
  approve: [0.5, 0.887], // the request screen's Approve, pinned at the bottom
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

// The OS notification banner as a texture: the card at BANNER_PAD from the top, with room
// for its shadow. The shader slides it in from the top.
export const BANNER_PAD = 40;
export const BANNER_H = 184 + 2 * BANNER_PAD + 40;
function drawBanner(title, body) {
  const c = document.createElement('canvas');
  c.width = W;
  c.height = BANNER_H;
  const g = c.getContext('2d');
  const y = BANNER_PAD;
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
  return c;
}

export function banners() {
  return { needsYou: drawBanner(NEEDS_YOU, NEEDS_YOU_BODY), sent: drawBanner(SENT, SENT_BODY) };
}
