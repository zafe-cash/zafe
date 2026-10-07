# Zafe

**A shared vault for shielded Zcash.** A group of members jointly controls one
shielded address; any member can see the balance, propose a payment and approve it, and
a payment goes out only when **t of the N** members sign.

Zcash's shielded pools have no scripts or contracts, so Zafe uses **re-randomized FROST
threshold signatures**: members' phones jointly produce one ordinary spend signature. On
chain, a Zafe vault looks exactly like a single-user wallet: no owners, no threshold,
nothing that says "multisig".

> **Status: pre-release, testnet/regtest only, not audited.** Do not use with real funds.
> The Zcash Foundation has answered our key-derivation and signing questions (see
> [`upstream-asks.md`](upstream-asks.md)); mainnet waits on an external audit and a hosted relay.

Website: <https://zafe.cash> (the 3D walkthrough is at
[`/showcase`](https://zafe.cash/showcase)).

## How it works

- **Vault keys**: all members run a FROST DKG on their phones, confirm a safety number
  out of band, and sign the vault descriptor. Keys follow ZIP 2005 (quantum-recoverable
  derivation) for the Ironwood pool (NU6.3).
- **No blind signing**: before approving and again before signing, every member's app
  rebuilds and checks the transaction itself: recipients, amounts, memos, change, fee
  and sighash.
- **One-tap approvals**: members pre-publish FROST nonce commitments, so an approval
  already carries the signature share. The member whose approval completes the threshold
  can send automatically; anyone can send once enough shares are in.
- **Blind relay**: a small server forwards signed, HPKE-encrypted envelopes between
  members and sends content-free push notifications. It never sees keys, viewing keys,
  amounts or addresses, and can't spend.
- **Backups**: each member can export an encrypted backup of their key share
  (Argon2id + XChaCha20-Poly1305). Signing nonces are never included.
- **On the phone**: optional Tor for every connection (embedded arti, fail-closed), an
  encrypted wallet database (SQLCipher), an unlock to approve or send, several vaults
  per phone (a fresh identity each), invite links, and viewing keys for auditors.
- **Versioned formats**: every envelope, log entry, event, API body and on-device file
  carries a format version, and older apps are told to update instead of misreading.

The full design is in [`spec.md`](spec.md).

## No telemetry

Zafe has no analytics, no crash reporting and no third-party SDKs that phone home, in the app,
the relay or the site. When something breaks, the app keeps a short error log on the phone
(addresses, keys, links, amounts and vault ids removed before it is written). Settings >
Privacy > Diagnostic report shows it in full; it leaves the phone only if you share it. CI
fails if a telemetry package is added (`scripts/check-no-telemetry.sh`).

## Repository

```
crates/zafe-core    keys (ZIP 2005), DKG, FROST signing, PCZT building and verification,
                    vault log, wallet sync (zcash_client_backend/sqlite), orchestration
crates/zafe-proto   identities, signed/encrypted envelopes, relay API types
crates/zafe-relay   the blind relay (axum + SQLite), push notifications (FCM)
crates/zafe-cli     `zafe`, a headless member used for tests and scripting
app/                Flutter app (Android today; iOS not yet built) with a Rust bridge
                    (flutter_rust_bridge) in app/rust
infra/regtest       Zakura + lightwalletd regtest with NU6.3 active (Docker)
infra/relay         relay Dockerfile and deploy recipes (Fly.io or a VPS); not deployed yet
infra/site          the website (Astro, static, strict CSP): home, /showcase, /join
scripts/            end-to-end scripts, benchmarks, brand and illustration generators
docs/               tracker (what's next), releasing, brand, website and SEO decisions,
                    design references, illustration guide
.claude/skills/     project skills for agents: brand palette, logo design, landing page
```

## Build and test

Rust (stable, see `rust-version` in `Cargo.toml`):

```bash
cargo fmt --all && cargo clippy --workspace --all-targets
cargo test --workspace

# ZIP 2005 key-derivation vectors, checked independently in Python
python3 scripts/check_zip2005_vectors.py

# Live regtest (Docker): an in-process end-to-end spend
cargo test -p zafe-core --test regtest_e2e -- --ignored --nocapture
# Three separate `zafe` CLI members through the relay on regtest
scripts/m0-e2e.sh
```

App (Flutter 3.41.6):

```bash
cd app
flutter analyze
flutter test
flutter build apk --debug --target-platform android-arm64
```

`scripts/app-harness.sh` runs a local relay, regtest and two CLI members so a single phone
or emulator can go through vault creation, payments and approvals. Network endpoints are
build-time `--dart-define`s (`ZAFE_NETWORK`, relay and lightwalletd URLs; defaults point
at a local regtest).

Website (Node 24, as in CI):

```bash
cd infra/site && npm ci && ./build.sh     # → dist/; fails on any inline script or style
```

Pushes to `main` deploy it to Vercel (`.github/workflows/site.yml`); see
[`infra/site/README.md`](infra/site/README.md). Android releases:
[`docs/releasing.md`](docs/releasing.md). Relay deployment:
[`infra/relay/README.md`](infra/relay/README.md).

Contributor notes, invariants and gotchas are in [`AGENTS.md`](AGENTS.md); open work is in
[`docs/tracker.md`](docs/tracker.md).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT), at your option. Exception: code derived from Vizor (listed in
[`app/NOTICE`](app/NOTICE), mainly `app/lib/src/core` and `app/assets/icons`) is
Apache-2.0 only, as upstream. Bundled fonts are under the SIL Open Font License 1.1
(`app/assets/fonts/licenses`).

Unless you explicitly state otherwise, any contribution intentionally submitted for
inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed
as above, without any additional terms or conditions.

## Acknowledgements

The app's architecture and parts of its UI component library are derived from
[Vizor](https://github.com/chainapsis/vizor-wallet) (Apache-2.0); see
[`app/NOTICE`](app/NOTICE). Built on the Zcash Foundation's FROST libraries and the
librustzcash crates.
