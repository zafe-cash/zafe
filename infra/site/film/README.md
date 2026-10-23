# Section loops

The home page's illustrated loops (docs/site.md §6): a scene from `../scenes` behind one app
component, timed together. Wide (1600x900) and tall (720x1280) for each of `sign`, `sent`,
`setup`, `live`.

```bash
cd app && flutter test --dart-define=ZAFE_NETWORK=main tool/screens/site_motion_render_test.dart
BLENDER=/path/to/blender infra/site/scenes/render.sh <workdir>
cd infra/site/film && npm ci && ./prepare.sh <workdir> && ./export.sh
npm run dev   # Remotion Studio, to look at a loop frame by frame
```

`export.sh` writes `infra/site/public/assets/motion/<loop>[-tall].{mp4,webp}` and
`<loop>[-tall]-still.webp` (shown instead of the video with reduced motion).
