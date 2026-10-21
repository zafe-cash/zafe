# Why ZCG declined recent multisig grants

Zcash Community Grants (ZCG) meeting minutes from June 2025 to September 2026 were
searched on 2026-10-02. The quotes below are copied word for word from the minutes. Four
multisig, FROST wallet or FROST custody proposals were declined. FROST bridges and DEXes
(Zecboat, ZPrivDEX) are left out.

## 1. Threshold Shielded Signing Kit (TSSK), $25,000: rejected

[Forum thread](https://forum.zcashcommunity.com/t/52937). Minutes:
[11/10/25](https://forum.zcashcommunity.com/t/53062) (still open) and
[11/24/25](https://forum.zcashcommunity.com/t/53482) (rejected).

The proposal: a FROST Rust SDK, a relay and a Vault CLI.

- Artkor (11/10): "Based on expert feedback, the practical need and the adoption path of
  these toolkits is unclear."
- Jason (11/24): "We took feedback from the community and ecosystem engineers, and it isn't
  clear that what this provides isn't already in some of the Zcash Foundation toolkits for
  FROST."
- Decision: "Reject based on feedback from ZF and community on usefulness."

## 2. Zenith Full-node Wallet 2026, $122,600: declined

[Minutes 7/6/2026](https://forum.zcashcommunity.com/t/56577).

The proposal: FROST multisig through a DKG, dynamic fees and Ironwood migration for a
full-node wallet.

- Artkor: "First, the committee generally does not want to encourage separate grant
  funding for support of mandatory protocol upgrades, such as the upcoming Ironwood work,
  because that type of development is part of maintaining the normal functionality of a
  wallet. Second, it seems that we are all expecting a significant architectural shift
  with Tachyon. That raises questions about whether the remaining useful lifecycle of
  today's full-node wallet model is strong enough to justify allocating community funds to
  this work at this time. Personally, I am interested in supporting more advanced
  functionality such as FROST, as well as other experimental wallet features, but only
  when the work is clearly reusable and broadly applicable across the Zcash ecosystem."
- Zerodartz: "My main concern is that this wallet doesn't have many users as far as we
  know. And yeah, full wallet nodes are not the most popular for most of the community.
  There's just not enough value right now for us to fund it sadly."
- Hanh: "I think that the amount requested here is disproportionate compared to the
  amount of work and the value offered. The only thing that is different from the other
  wallets is the support for FROST and it seems that their implementation is not
  necessarily better than what is out there. Also it would tie the users to their
  platform. It's mainly a problem related to value delivered and cost that I decline."

## 3. FROST Shielded Multi-Sig SDK, $22,000: declined

[Forum thread](https://forum.zcashcommunity.com/t/56608).
[Minutes 7/20/2026](https://forum.zcashcommunity.com/t/56763).

The proposal: a wallet-ready Rust API wrapping ZF's FROST stack.

- Gguy: "Because of the complexity of using FROST and the protocol I think it's too
  optimistic to believe an API can meet the specific needs of many different Zcash
  applications. I think this type of work needs to come from the application developers
  who are focused on their specific use cases and UX. My concern is that without perfect
  fit with developer needs this work would have low adoption and low impact."
- Paul: "I think we just need to understand that a solution like this is actually going
  to have a need to fulfill in users and demand."
- Gguy: "FROST can provide many interesting use cases but until we get to that point in
  the adoption phase I believe it's too hard to predict the exact API flows that we're
  going to need."
- Hanh: "This proposal skips over the hard problems of bringing FROST to Zcash at scale.
  What's the secure p2p messaging mechanism? How do participants get the tx they need to
  sign (securely)? How do they backup the shares? How do we recover when a participant
  disappears? Etc. I decline."

## 4. Threshold (FROST) custody for shielded ZEC, $47,000: declined

[Forum thread](https://forum.zcashcommunity.com/t/56660) (aryaethn).
[Minutes 8/3/2026](https://forum.zcashcommunity.com/t/56909).

The proposal: org-grade custody with COCKTAIL-DKG (ZIP 2005), `qsk` backup, re-randomized
FROST over PCZT on Ironwood, a coordinator service and Keystone support, from a solo
developer. Of the four, this is the closest to Zafe.

- Gguy: "Without an end-to-end solution or application in mind, it can be hard to tie the
  exact needs for technologies like this in with the needs of the applications. At the
  risk of this being something that isn't adopted across the community, I'm leaning
  towards rejecting this one."
- Artkor: "I very much want to see FROST continue to develop and gain broader practical
  adoption, so I remain open to retroactively supporting any concrete contribution that is
  recognized as useful by experts in this field and by the community."
- Paul: "I have concerns about adoption and ongoing maintenance and having one person
  working on this effort. I think it really needs a team. So, too many risks and
  outstanding questions for me to approve."
- Zerodartz: "FROST is interesting technology and a useful tool. Zcash ecosystem needs it
  to be used way more but for that to happen there has to be one standard every wallet can
  use. This grant in some ways tries to fix that, but I think it would make more sense if
  there was more collaboration with core teams and a proper security audit by multiple
  auditors. Custody infrastructure needs highest level of trust. I'd also like to see some
  demand signal from exchanges who would probably need this the most if they were to hold
  shielded ZEC. Right now I don't see the need to rush so I'm rejecting."
- Hanh: "For Information, zkool provides end to end decentralized FROST DKG and Signing
  using Zcash memo as secure transport. Works with Ironwood. So the gap would be org-grade
  and KS support. Unfortunately, that's also the part where I don't see much appetite for
  standardization and integration. There is too much risk with upstream dependencies for
  me to approve this, but I see no support for ecosystem shareholders that would lead me
  to believe otherwise. I decline."

## Where Zafe stands against these concerns (assessment, 2026-10-02)

### Concerns Zafe answers with shipped code

| Concern | Who raised it | What Zafe has |
|---|---|---|
| Secure messaging between signers | Hanh (#3) | Blind relay, signed envelopes, HPKE, encrypted vault log |
| Getting the transaction to sign securely | Hanh (#3) | PCZT in the encrypted log; every member checks it on their own device before approving and again before signing (`verify::verify_pczt`) |
| Backing up shares | Hanh (#3) | Encrypted `ZAFEBAK` backups, plus backup-health attestations in the log |
| A signer disappears | Hanh (#3) | Seat moves with share repair (RTS), and retrying a stalled repair |
| No application in mind, only an SDK | Gguy (#3, #4) | Zafe is an end-to-end app (Android, CLI, relay) |
| Talking with core teams | Zerodartz (#4) | U1, U3 and U5 were cleared with ZF (frost#1094) and Daira-Emma Hopwood |
| The UX | Gguy (#3) | One-tap signing, notifications, live updates |

### Concerns Zafe has not answered

- **Demand and users.** Zafe had no users and no hosted relay (the testnet and mainnet beta relays went live later in October), so there is no demand signal from DAOs or exchanges yet. This came up in every decision and is the biggest gap.
- **Team and maintenance.** Paul's "one person … it really needs a team" applies to Zafe just as it did to #4.
- **Audit.** None yet; it is the M2 gate. Zerodartz asked for "multiple auditors".
- **One standard every wallet can use.** Zafe's `sk` agreement differs from the COCKTAIL-DKG draft (zips#895), and its relay format is Zafe's own. Hanh's Zenith concern, that it "would tie the users to their platform", applies. `docs/stack-plan.md` addresses this, but none of it has been done yet.
- **Overlap with zkool.** Hanh maintains zkool and says it already does FROST DKG and signing on Ironwood. The difference needs a clear, honest statement: org-grade UX, phones, one-tap signing, recovery and backup health. It should not be claimed as being first.
- **Upstream risk.** Zafe pins `reddsa` 0.5.2 while the FROST ciphersuite moves out of it (frost#963), and Tachyon will change wallet architecture. Hanh used upstream risk to decline #4.
- **Scale.** Hanh raised FROST's n² messaging cost on another grant (Zecboat, 4/13/2026). One-tap signing falls back to interactive signing above 64 signer groups.

### Implication

A forward grant would likely be declined for the same reasons as #4: solo developer, no
users, no audit. The committee's stated opening is **retroactive** support "recognized as
useful by experts in this field and by the community". The stronger path is to ship to
real testers (a hosted relay, a few DAOs using it on testnet), get the audit and publish
the stack-plan standardization. Then apply retroactively, with demand evidence and a
comparison table against zkool.
