# Release notes and classes

One file per mainnet version, `<version>.md`, required by `.github/workflows/release-mainnet.yml`
(a `mainnet-v<version>` tag without it fails). The first line sets the class; the rest (1-500
characters) is what users read: Play's "What's new" and the top of the GitHub release.

```
class: optional
Short, plain notes about what changed.
```

- `optional`: Play in-app update priority 2. The app downloads it in the background and shows
  "Update ready" on Home; nothing interrupts. Staged rollouts are fine.
- `essential`: priority 5. The app shows a full-screen update on the next launch, unlock or
  resume (held back only while a payment is sending). Use it for a Zcash network upgrade (NU7),
  a vault protocol change that every member needs (an event version gate), or a security fix.
  Roll it out to 100% at once: members of a vault should update together.

Promotion to production: `.github/workflows/play-promote.yml` (see `docs/releasing.md`).
