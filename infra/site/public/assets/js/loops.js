// The home page's section loops (Loop.astro): a pause button on each (WCAG 2.2.2: moving
// content over 5 s must be pausable), and loops off screen don't play. A paused loop stays
// paused until its button is pressed again.
for (const fig of document.querySelectorAll('[data-loop]')) {
  const video = fig.querySelector('video');
  const button = fig.querySelector('.loop-toggle');
  if (!video || !button) continue;
  let paused = false;
  let visible = true;
  const sync = () => {
    button.setAttribute('aria-pressed', String(paused));
    button.setAttribute('aria-label', paused ? 'Play animation' : 'Pause animation');
    if (paused || !visible) video.pause();
    else video.play().catch(() => {});
  };
  button.hidden = false;
  button.addEventListener('click', () => {
    paused = !paused;
    sync();
  });
  new IntersectionObserver(([entry]) => {
    visible = entry.isIntersecting;
    sync();
  }).observe(video);
}
