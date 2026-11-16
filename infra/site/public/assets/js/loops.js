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
