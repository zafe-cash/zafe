# Contributing to Zafe

Thanks for helping. Zafe is a shielded multisig for Zcash: a wrong line in the key, signing or
verification code can lose a group's funds. So the rules below are strict where money is at
stake and light everywhere else.

Modelled on [librustzcash's CONTRIBUTING](https://github.com/zcash/librustzcash/blob/main/CONTRIBUTING.md).
Where they differ, this file describes how this repo actually works.

## Before you start

- **Security issues:** don't open a public issue. Email security@zafe.cash
  ([security.txt](https://zafe.cash/.well-known/security.txt)).
- **Questions and ideas:** open a GitHub issue, or post in the
  [forum thread](https://forum.zcashcommunity.com/t/58636).
- **Bugs:** open an issue with the app or CLI version, the network (mainnet, testnet or
  regtest), what you did, what you expected and what happened. The app's report helps
  (Settings → Diagnostic report → Share report). Never paste a backup, its passphrase
  or a PCZT: a PCZT links a payment to the vault.
- **Bigger changes:** open an issue first, so we can agree on the approach before you write
  the code.

Read these first:
- [`spec.md`](spec.md): the protocol (keys §7, signing §9, recovery §10 and §12, security
  model §13).
- [`AGENTS.md`](AGENTS.md): commands, the repo layout, and the **protocol invariants**.
  Breaking an invariant is a security or funds bug.
- [`docs/tracker.md`](docs/tracker.md): what's open, deferred or planned.

## Making a change

### Git

- Work on a branch and open a pull request to `main`. Draft PRs are welcome; mark the PR
  ready once CI passes.
- PRs are merged with a merge commit, so keep the history clean: each commit is one
  discrete change, with no "WIP" or "fix typo from review" commits. Fold review fixes into
  the commit they belong to before merging.
- A bug fix starts with a test that fails on the broken code, ideally in its own commit
  before the fix.
- Commit titles are short (under about 72 characters) and name the area first, as in the
  existing history: `zafe-core: …`, `App: …`, `Relay: …`, `CLI: …`, `Site: …`,
  `Docs: …`. The body says why the change is needed. If you quote a dependency's
  behaviour, cite its pinned source (`file:line`), not memory or a comment.
- Several authors on a commit: add a `Co-Authored-By:` line for each.

### Checks (CI runs all of these)

```bash
cargo fmt --all --check
cargo +stable clippy --workspace --all-targets -- -D warnings
cargo test --workspace
python3 scripts/check_zip2005_vectors.py   # ZIP 2005 vectors, independent check
scripts/check-no-telemetry.sh
python3 scripts/check_docs_sync.py         # docs match the app and the relay
python3 scripts/check_typography.py        # app type scale
python3 scripts/check_theme_pairs.py       # dark/light colour pairs
(cd app && flutter pub get && flutter analyze lib test tool && flutter test test)
```

Some tests are `#[ignore]`d because they need Docker, a local node or another binary: the
regtest end-to-end spend, the bridge flow, the `zcash-sign` cross-check and the frost-zec
interop. `AGENTS.md` ("Commands") shows how to run each one. Run the ones your change
touches. For anything in signing, keys, verification or the vault log, run the regtest
end-to-end tests too.

### Where money is at stake

These parts of `crates/zafe-core` need extra care and two things in every PR: a test that
shows the property holds, and an update to `spec.md` in the same change.

- Key derivation (`keys.rs`, ZIP 2005 `use_qsk`): regenerate the vectors with
  `ZAFE_REGEN_VECTORS=1 cargo test -p zafe-core --test zip2005_vectors` and confirm the
  Python check still passes. Other wallets test against these vectors, so a changed vector
  is a breaking change for them too.
- Key generation and the `sk` agreement (`keygen.rs`, `sas.rs`).
- Verifying a PCZT before signing (`verify.rs`, `tx.rs`): never loosen a check without a
  written reason in the spec.
- FROST signing and nonces (`signing.rs`, `session.rs`, `nonce_store.rs`, the one-tap
  pools in `vault.rs`): a nonce is used at most once, deleted before its share leaves the
  device, and never written to a backup.
- Vault log replay (`vault.rs`): every phone must reach the same state from the same log.
  Behaviour changes go behind a new event version; old logs replay exactly as before.

### Users and docs

- A change to an app label, flow, screen or relay setting updates the user docs
  (`infra/site`, served at zafe.cash/docs) in the same PR. `check_docs_sync.py` catches most
  misses.
- A user-facing change gets a line in the next release notes (`docs/releases/<version>.md`;
  format in [`docs/releases/README.md`](docs/releases/README.md)). Write for users, in plain
  words.
- Update `docs/tracker.md` when you finish, defer or discover something.

### Code style

- Rust: `rustfmt` defaults; clippy clean on stable with `-D warnings`. Typed errors in
  library code for anything that comes from the network, the relay, a file or another member;
  don't `unwrap` it. New public items get rustdoc.
- Secrets (shares, `sk`, nonces, backup passphrases) never reach logs, error messages or the
  clipboard beyond the app's timed copy.
- Flutter: `flutter analyze` clean. Type sizes and colours come from the theme
  (`app_typography.dart`, the colour tokens), as the CI checks enforce.
- Name file and directory names once, in a constant (see `AGENTS.md`), never inline.

## Upstream first

Zafe builds on ZF's FROST crates and librustzcash. If a fix or feature belongs upstream
(FROST, the PCZT roles, test vectors), we send it there as a small PR or a review and use
our own copy only until it lands. When contributing to those projects, follow their own
CONTRIBUTING and merge rules.

## License

Unless you say otherwise, any contribution you submit for inclusion in Zafe is dual
licensed under the [MIT license](LICENSE-MIT) and the
[Apache License, Version 2.0](LICENSE-APACHE), with no additional terms, as described in
the [README](README.md#license).
