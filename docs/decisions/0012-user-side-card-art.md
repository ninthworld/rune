# ADR 0012: Player-side, opt-in, device-local card art

- Status: accepted
- Date: 2026-07-30

## Context

The card frame is designed around a reserved **art window**: a region that holds a procedural
placeholder, with the name band, cost pips, type line, and P/T pill keeping their positions
whether or not an image ever fills it. Frame and layout work stalls on that window — every
visual decision gets made against a face that will not be the shipped face.

The legal policy (`docs/brief.md`, Legal constraints) is deliberately conservative: no card
images, official frames, symbols, or Wizards branding, and any weakening requires an explicit
decision. What the constraint actually governs is **what the project distributes** — the
repository, the built client, and the server must not contain or serve official imagery. It
says nothing about what a player's own browser fetches from a third party at that player's own
request. That is the model long established by community tools: the application ships
imageless, and users may point it at an image source themselves.

The protocol already carries `CardView.functional_id` (ADR 0008 §7) — a stable presentation
identity a client-local cache can enrich against with no wire change.

## Decision

Card illustrations are a **client-local presentation concern with pluggable, player-selected
sources**, keyed by `functional_id`. Three sources exist, chosen in a settings surface and
stored as a device preference:

1. **Procedural** (default): the vector frame with its placeholder. Nothing downloads; offline
   play is unaffected.
2. **Bundled**: original, project-owned illustrations shipped with the client and gated by a
   manifest. Only art the project may redistribute is ever added here.
3. **Third-party** (opt-in): the player's browser fetches card images directly from an
   external source — currently Scryfall — after an explicit consent step, rate-limited per
   that source's guidelines, and caches them **on the player's device only**. Further sources
   can be added behind the same interface.

The third-party source has two player-selected **presentation styles**:

- **Window** (default): only the bare illustration is downloaded and rendered inside SAGE's
  own procedural frame — SAGE keeps drawing the name band, pips, type line, and keyword strip.
- **Full card**: the entire official card image becomes the face, frame and all. SAGE's
  printed text is suppressed because it is on the image, but the server-computed overlays —
  effective P/T, counters, combat bars, selection and targeting rings, the playable
  affordance, tap state — always render on top. The image is presentation; the overlays remain
  the authoritative values.

The two styles download different images and cache independently, so switching is instant once
both are fetched.

**Alternate printings.** An exact-name lookup returns the source's default printing. An art-map
entry may pin a specific printing (set plus collector number) to select a particular version —
a full-art basic, a specific illustration — instead. Aspect differences need no special
casing: window mode cover-crops any illustration inside its mask, and every card image shares
the physical card aspect (~63:88), which each render tier's footprint matches to within a
fraction of a percent.

Rules the codebase follows:

- The repository, built client, and server never contain or serve official imagery. Downloaded
  art never leaves the player's device and is never re-uploaded, proxied, or shared — not even
  to another client in the same game.
- **Art is cache, never state.** The UI must remain fully reconstructable from one `GameView`
  with the art store empty, so cards render procedurally and clearing the cache is always
  safe.
- The renderer treats art as a looked-up texture keyed by `functional_id`. It never fetches,
  and no game data flows to any art source beyond the card names being resolved.
- Server-computed characteristics are never hidden behind a printed image: in full-card mode
  the effective values still overlay the face, so a buffed 4/4 never reads as its printed 2/2.

## Consequences

- Frame and layout work is unblocked: the art window renders real pixels at the field and hand
  tiers, and the inspector shows an illustration, without waiting on commissioned art.
- The project's distribution posture is unchanged. What ships is exactly as imageless as
  before, and the brief records the player-side carve-out explicitly.
- The client gains a local persistence dependency and a background-loading pipeline — a
  rate-limited queue, a texture registry, a change subscription. All of it is injectable and
  covered by offline tests; no test touches the network.
- Third-party availability is outside the project's control. Failures degrade to the
  procedural face silently, per card: an unavailable illustration can never block play.
