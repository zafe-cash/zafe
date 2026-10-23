# Upstream asks: Zcash Foundation (FROST)

**Where to send:** one comment on **frost#1094** (the open issue about external randomizers
and FVK derivation, answered by conradoplg on 2026-09-21), plus a short pointer in `#frost`
on the Zcash R&D Discord. conradoplg co-wrote the Re-Randomized FROST paper, so U5 is his.

**Status (2026-10-01): all three answered.** conradoplg replied on frost#1094
(https://github.com/ZcashFoundation/frost/issues/1094#issuecomment-5932215317): U5 fine, U1 "very unlikely" to change, U3 handled with the move to the
FROST repo. Daira-Emma Hopwood also replied on Discord (under U5) and answered our follow-up the same
day (end of this file): both deviations fine. No upstream blocker left for mainnet.
Ids are the open items in `spec.md` §19. Write each question so it can be answered
without opening links.

---

## U5 (answered 2026-10-01): a builder-chosen randomizer with pre-published commitments

**Context checked:**
- ZIP 312, both the published text and the zips#895 draft (head `be04583`), says the
  Coordinator **MUST** derive the randomizer after round 1: `randomizer_generate()` =
  `HR(random_bytes(Ns) ‖ encode_group_commitment_list(commitment_list))` (the published
  version also hashes the message, which the draft drops because `α` feeds the sighash).
  The rationale calls the commitment binding a hedge: it "prevents the Coordinator from
  fully influencing the randomizer, reducing its trust assumptions".
- Zafe instead uses each spend's PCZT `α` (picked by the builder like `RedDSA.GenRandom`),
  as does frost-tools' `zcash-sign` @ `06c0dbdb` (`collect_randomizers` reads
  `action.spend().alpha()`). So Zafe **deviates from a MUST in ZIP 312** (spec §9.5.1).
- ePrint 2024/436 (checked against the PDF): the unforgeability game allows signatures
  "under a adversarially-chosen randomizer" (Fig. 4, `OSign′` after the honest
  commitments); the extraction works "because this value is public"; §5.1: the party that
  chooses the randomizer "is not trusted with security; even if this party were to act
  maliciously, then the scheme remains secure".
- Zafe could comply if required: `VaultState::assign_commitments` depends only on the pool
  state and the spend count, so a proposer knows each spend's commitments before building
  the PCZT and could set `α = HR(seed ‖ those commitments)`. Unchecked: whether the PCZT
  builder (`create_pczt_from_proposal`) lets us choose `α`; a log race would mean a rebuild.

**Asks:** (a) is unforgeability preserved with a builder-chosen `α`, including several
concurrent signing packages over the same sighash and `α` (one per signer group, disjoint
commitments)? (b) will ZIP 312 allow it (what any PCZT signer gets), or should wallets bind
`α` to the commitments, and would the variant above count?

**Answer (2026-10-01, Daira-Emma Hopwood on Discord):**
> The security proof in https://eprint.iacr.org/2024/436.pdf does not require α to be chosen
> after the commitments (in fact, when considering security against forgery as opposed to
> privacy, it allows the adversary to choose α). However, knowing α allows anyone to link the
> transaction containing rk to the long-term group public key ak.

**Answer (conradoplg, frost#1094):** *"That is fine. That procedure is really to offer an
additional layer of protection against a possible weak RNG by the coordinator. But you need
to trust the coordinator (and all participants) to keep alpha secret anyway."* So the ZIP
312 deviation is accepted by ZF too.

So (a) is yes, for both interactive and one-tap signing. The caveat is privacy: `α` must stay
among the members. It does today (PCZT only in the encrypted log, HPKE signing requests and
members' devices; not in the broadcast tx, backups or the CSV export; members hold the FVK
anyway), and spec §9.5.1 now requires any future PCZT export to strip it. (b), ZIP 312's
MUST, was then answered by conradoplg (above): fine.

## U1 (answered 2026-10-01): are vaults created today recoverable?

**Answer (conradoplg, frost#1094):** *"There is always some risk of things changing, but I
feel like it is very unlikely. Most work pending on zips#895 is regarding on how
participants agree on sk; we plan to use the COCKTAIL-DKG protocol to do the DKG while
simultaneously agree on sk. But if you have your own system that will also work."* So
vaults derived per § 4.2.3 from Zafe's own `sk` agreement are expected to stay recoverable.
Vectors: conradoplg preferred frost-tools; issue frost-tools#609, PR frost-tools#610 (2026-10-04).

**Context checked:**
- ZIP 2005 (status Proposed; changes since 2026-07 are wording) has a normative "Usage with
  FROST": participants "MUST privately agree on a value `sk`", then derive `nk`, `qsk`,
  `qk`, `rivk_ext` per § 4.2.3 with `use_qsk = true`; all must get the same `ak`, `nk`,
  `rivk_ext`; `qsk` kept as securely as `ask`. No `sk`-agreement method is required. The
  Recovery Protocol checks the derivations, a `SoK^qsk` and a RedDSA signature under `ak`.
- Zafe's constants match § 4.2.3 exactly (`0x07`, `0x0C`, `0x0D`, BLAKE3
  "Zcash ZIP 2005 qk-derivation v1"); FVK via `FullViewingKey::from_bytes`, never the
  `from_sk_ak_incompatible…` constructor (orchard#475, still open).
- conradoplg on frost#1094: FROST FVK derivation is "blocked on zips#895 being finished and
  integrated with ZIP 2005, and then implemented". ZIP 2005 already has the FROST section,
  so it's unclear whether that still applies to wallets deriving keys themselves.
- No official `use_qsk` vectors: zcash-test-vectors#129 (open) adds only QR note
  commitments. Zafe has its own (`crates/zafe-core/test-vectors/zip2005_use_qsk.json`, from
  `tests/zip2005_vectors.rs`, checked independently by `scripts/check_zip2005_vectors.py`).

**Reply so far (2026-10-01):** *"If you derive FROST keys only from sk then there is
necessarily a single party with all of the key material, which slightly defeats the point
of using FROST IMHO. But it sounds like that's what you want, if you want total
recoverability from seed."* That reads our question as deriving `ask` from `sk`. Zafe
doesn't: `ak` and the `ask` shares come from the DKG and `ask` never exists; `sk` gives
only `nk`, `qsk`, `rivk_ext` (ZIP 2005 "Usage with FROST"). Every member does hold `qsk`,
so against a quantum adversary one member could steal (ZIP 2005 says so too; spec §2.3).
Still unanswered: does the Recovery Protocol accept such vaults as they are.

**Asks:** does "blocked on zips#895" apply to keys derived this way, or will such vaults pass
the Recovery Protocol as they are? Is anything in § 4.2.3 still expected to change? Offer:
PR our vectors to zcash-test-vectors.

## U3 (answered 2026-10-01): reddsa 0.5.x until the ciphersuite moves

**Answer (conradoplg, frost#1094):** *"They will be moved to the FROST repo. I expect that to
happen before any need of a security fix, but in the unlikely case that happens, we will
work it out. The move will keep serialization and everything else as is."* Switch crates
when the move ships; stored `KeyPackage`s stay readable.

reddsa 0.6.0/0.6.1 (2026-09-25) dropped the FROST ciphersuites; frost#963 (move them to the
`frost` repo) is open and `frost` has no redpallas crate. Zafe stays on reddsa 0.5.2 +
frost-core 3.0.0 (orchard 0.15.5 used reddsa 0.5; since the NU7 move Zafe pins orchard 0.16,
which uses reddsa 0.6 without FROST, so the tree has both). **Ask:** security fixes for 0.5.x until
the move, and will it keep the frost-core 3.x `KeyPackage` serialization?

---

## Not asking (re-checked 2026-10-01)

- **Old Q2, external-randomizer API:** answered on frost#1094 ("there will always a
  mechanism for feeding an external randomizer"; `sign()` may be un-deprecated). We keep
  the one `#[allow(deprecated)]` wrapper.
- **Old Q4, DKG and `sk` agreement:** the zips#895 draft specifies it ("Contributory
  Generation of sk": each member's `sk_i = random_bytes(32)` as a COCKTAIL-DKG payload,
  `sk = H(n ‖ len(payload_1) ‖ payload_1 ‖ …)`). Zafe's own scheme differs; recoverability
  doesn't depend on it (U1). Align when COCKTAIL-DKG ships with Pallas (frost#1033 open;
  the WIP frost#1032 has no Pallas ciphersuite; C2SP#216 closed for C2SP#299).
- **Old Q5, FROST book:** `book/src/zcash/technical-details.md` still suggests throwing
  `sk` away and says the key share is the only secret, against ZIP 2005. Send a docs PR
  rather than a question.
- **Old Q6, frostd store-and-forward/push and repair (frost-tools#311):** not a question;
  revisit when Zafe implements repair.

## Discord pointer (paste after the comment is up)

> hi FROST folks 👋 I'm building Zafe, a mobile shielded multisig on Ironwood with
> re-randomized FROST (frost-core 3.0, reddsa 0.5.2, PCZT). Two questions block our
> mainnet, written up on frost#1094: https://github.com/ZcashFoundation/frost/issues/1094#issuecomment-5928093098
> 1. our randomizer is the PCZT's `α`, chosen before round 1 (like `zcash-sign`), which
>    ZIP 312 says MUST come after. OK for pre-published commitments?
> 2. ZIP 2005 keys derived from an agreed `sk` per § 4.2.3: recoverable as is?
> thanks 🙏


## Follow-up to Daira (Discord, sent 2026-10-01; answered the same day)

> Thanks Daira! Two follow-ups:
>
> **1.** In Zafe α never leaves the vault members (it's only in the PCZT, sent encrypted;
> never broadcast), and they all hold the FVK anyway. Given that, is it OK to deviate from
> ZIP 312's "derive the randomizer from the commitments after round 1"? We can't for
> one-tap signing: commitments are published before any proposal exists, and α must be
> fixed before the sighash.
>
> **2.** We don't derive the FROST keys from sk: ak and the ask shares come from the DKG, so
> ask never exists. sk only gives nk, qsk and rivk_ext (ZIP 2005 "Usage with FROST"). Does a
> vault built this way pass the Recovery Protocol as is?

**Answer (Daira-Emma Hopwood, 2026-10-01):**
> Yes this is fine.
> Yes that's the intended usage with use_qsk = true, as in the Usage with FROST diagram. With
> use_qsk = false it does not conform to ZIP 2005 and the funds will not be quantum-recoverable.
>
> If this wasn't clear, how could the ZIP be changed to make it more clear? Would an explicit
> statement that use_qsk MUST be true whenever using FROST with a DKG help?

So: (1) builder-chosen `α` before round 1 is fine given `α` stays among members; (2) our
vault shape (`ak` from the DKG, `sk` → `nk`/`qsk`/`rivk_ext`, `use_qsk = true`) passes the
Recovery Protocol. Our reply to her ZIP question (suggestions only, `docs/tracker.md`):
put the MUST in § 4.2.3 itself, say which Recovery Statement branch a FROST wallet uses
(qk branch; `ask` never exists), and say `sk`/`qsk` must be kept (the FROST book says to
discard `sk`).

---

# Upstream ask: zcashlabs/thus-spoke-zakura (local regtest environment)

**Where to send:** an issue on `zcashlabs/thus-spoke-zakura`.

**Finding (2026-09-29, `ths` at `b24101f`, node `zakuracore/zakura:1.4.0`):** the environment runs a **pre-Ironwood** regtest chain:
- `crates/tsz-server/src/main.rs` `zakura_config()` sets only `"NU6" = 1` under `[network.testnet_parameters.activation_heights]`.
- `crates/tsz-server/src/wallet.rs` `regtest_network()` and `db.rs` `local_network()` hardcode `nu6_1`, `nu6_2` and `nu6_3` to `None`.

The **Zakura node itself supports NU6.3 on regtest**. Started with `"NU5"`, `"NU6"`, `"NU6.1"`, `"NU6.2"` and `"NU6.3"` all set to `1`, `getblockchaininfo` reports every upgrade through NU6.3 at height 1, with next-block consensus branch `37a5165b` (NU6.3).

**Ask:** activate every NU6.x (including NU6.3) at height 1 by default, or behind a flag (e.g. `ths start --ironwood`). Make the faucet and "send" route through the Ironwood pool, since Orchard stops accepting deposits after NU6.3. That would make `ths` usable for testing Ironwood apps like Zafe. Zebra's own regtest default already activates every NU6.x at height 1.

**Zafe's workaround until then:** run `zakuracore/zakura:1.4.0` and the `ths` lightwalletd image directly with our own NU6.3 config (`infra/regtest/`).

**Resolved (2026-10-02):** our PR #124 merged and shipped in `ths` v0.3.0 (Zakura 1.6.0, NU6.1-6.3 at height 1, Ironwood wallet). Since 2026-10-04 `infra/regtest/` runs `ths` itself (`ths start`, faucet, `ths mine`); our own Zakura config is gone. Zafe now pins `ths` 0.4.0 (NU5 to NU7 at height 1).

**Filed (2026-10-04):** issue [#145](https://github.com/zcashlabs/thus-spoke-zakura/issues/145), fix in PR [#146](https://github.com/zcashlabs/thus-spoke-zakura/pull/146). In 0.3.0, `ths --name X stop` from another shell deletes the environment but leaves the foreground `ths start` launcher running (it only exits on a signal). `infra/regtest/down.sh` sends SIGINT instead. **Resolved:** merged 2026-10-11, shipped in v0.4.0 (Zafe pins it since 2026-10-19).

**Asked (2026-10-04):** an opt-in `ths start --detach` for scripts and CI ([#147](https://github.com/zcashlabs/thus-spoke-zakura/issues/147)); offered to send the PR. PR [#308](https://github.com/zcashlabs/thus-spoke-zakura/pull/308) open since 2026-10-14 (conflicts with v0.4.0 resolved 2026-10-19), awaiting review.
