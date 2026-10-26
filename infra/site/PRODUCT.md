# Product

<!-- impeccable:product-schema 1 -->

Scope: the public website (`infra/site`, zafe.cash). The app's own rules live in
`AGENTS.md`, `docs/app-notes.md` and `docs/brand.md`; the site's design system in
`DESIGN.md`.

## Platform

web

## Users

- **First:** people who already hold Zcash and want to share custody with others: friends, partners, a
  small team. They know shielded ZEC and wallets like Zodl; they may never have used a multisig.
  They arrive from X, the Zcash forum or a link, usually on a phone.
- **Second:** Zcash grant committees, DAOs and teams weighing whether to trust a shared vault. They read
  for the mechanism, the limits and the proof (docs/site.md §3). The docs (`/docs`) serve
  both once they have the app, and anyone running their own server.

## Product Purpose

Zafe is a multisig wallet for shielded Zcash. A group's funds sit in one private vault; a payment leaves
only once enough members (t of n) approve it on their own phones; on-chain it is an ordinary shielded
payment. Success for the site: a Zcash user understands that in one screen, believes it is real, and
installs the app with their co-signers.

## Positioning

The multisig the chain can't see. FROST threshold signatures: the key is split across members' phones
and never put together, not even to sign; enough of them produce one ordinary signature. Each phone
rebuilds and checks the payment itself before signing. Phones talk through a blind server (it sees who
and when, never amounts, addresses or keys). No on-chain multisig and no single-key wallet can say all
three.

## Operating Context

Setting up a vault is a group ritual: one person creates it and shares an invite; everyone compares four
safety words; the phones make the key together. After that a payment is proposed by anyone, approved by
enough members (one tap after a phone unlock), and sent automatically.

## Capabilities and Constraints

- Android: Google Play (`xyz.zafe.zafe`, "Zafe: Multisig for Zcash") and a direct APK from GitHub
  Releases (arm64, Android 7+, a SHA-256 file and a fixed signing certificate). iOS not yet (waitlist).
- Zcash mainnet, **beta**: no external security audit yet ("only use money you can afford to lose").
  A testnet build exists. Members must update together before network upgrades (NU7, 2026-11-05).
- Up to 15 members; any t of n. Losing more phones than the threshold allows, without backups, loses the
  funds for good. Removing a member is not possible yet (make a new vault). Members keep view access.
- Encrypted backups; a lost phone's seat can be moved to a new phone by the co-signers.
- Each vault picks its server (Zafe's or its own) and can move to another one; only Zafe's servers
  send notifications.
- Optional Tor for every connection. No analytics, crash reporting or third-party SDKs, in the app or on
  the site (a CI check enforces it).
- Site: static Astro, strict CSP (`script-src 'self'`, no inline script or style, no third-party hosts).
  The home page's one script pauses the loops; the docs need none; the 3D story lives on `/showcase`.

## Brand Commitments

- Name "Zafe", the Seam mark (`src/components/Mark.astro`), the Verdigris palette: teal = action and
  "needs you", Zcash gold = value only, never green for money (`docs/brand.md`).
- Light paper ground only on the site (user, 2026-10-22).
- Voice: few words, one line per idea, plain, sentence case; jargon only where it's explained.
- Say "multisig", never "treasury".
- The app's real screens and components, never rebuilt fake UI.
- "Server" on the site and in the app, never "relay" (that word stays in code and developer docs).

## Evidence on Hand

- Real app renders (`public/assets/screens/`, from `assets.py`): one payment ("12.50 ZEC, Grant: Q4 audit
  milestone", 2 of 3) from proposal to sent, and the Server and move screens the docs show. The home
  page's loops carry cards the app renders itself (`docs/site.md` §6).
- A real testnet payment from the 2026-09-30 dry run: tx `e6ccd6b7…9eef0237`, block 4,422,289 (linked
  from the home page, shown on `/showcase`).
- Open source app, relay and spec (github.com/zafe-cash/zafe, `spec.md`).
- None of: customers, testimonials, logos, user counts, audits, benchmarks. Never invent them.

## Product Principles

1. Show the mechanism working, don't list it.
2. Say the limits where trust is asked for (beta, unaudited, key loss), not only in an FAQ.
3. Every claim is shipped and checkable.
4. Privacy is the product: nothing on the site tracks anyone.

## Accessibility & Inclusion

WCAG AA contrast; usable one-handed on a phone (44 px targets); works without JavaScript and with reduced
motion (Tor Browser "Safest" is part of the audience).
