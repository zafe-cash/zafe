# Section loops

The home page's illustrated loops (`docs/site.md` §6): a Blender scene from `../scenes`
behind one card the app renders itself, composited in Remotion. Each of `sign`, `sent`,
`setup` and `live` comes wide (1600×900) and tall (720×1280).

From the repo root:

```bash
(cd app && flutter test --dart-define=ZAFE_NETWORK=main tool/screens/site_motion_render_test.dart)
BLENDER=/path/to/blender infra/site/scenes/render.sh <workdir>
cd infra/site/film && npm ci && ./prepare.sh <workdir> && ./export.sh   # or ./export.sh sign sent
npm run dev   # Remotion Studio, to step through a loop frame by frame
```

`export.sh` needs `ffmpeg` and a Chrome (`REMOTION_BROWSER`, else the agent-browser one). It
writes `infra/site/public/assets/motion/<loop>[-tall].{mp4,webp}` (video and poster) and
`<loop>[-tall]-still.webp`, shown instead of the video with reduced motion.
