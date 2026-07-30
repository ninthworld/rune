# The RUNE card representation bible — frames, surfaces, states, and backs

**The design authority for how a card looks anywhere in the client** (issue #538,
under [ADR 0029](../decisions/0029-2-5d-presentation-direction.md) /
[ADR 0030](../decisions/0030-2-5d-presentation-architecture.md), master issue
#464). It is the card-level companion to [`visual-system.md`](visual-system.md)
(look and motion), [`layout-model.md`](layout-model.md) (where regions sit), and
[`presentation-budgets.md`](presentation-budgets.md) (what everything may cost).

**Binding sources, in precedence order:**

1. The approved baselines in [`../ui-concepts/`](../ui-concepts/) — this document
   transcribes them:
   [`rune-card-system-overview.jpg`](../ui-concepts/rune-card-system-overview.jpg),
   [`rune-card-states.jpg`](../ui-concepts/rune-card-states.jpg),
   [`rune-2.5d-interface-baseline.jpg`](../ui-concepts/rune-2.5d-interface-baseline.jpg).
2. [`presentation-budgets.md`](presentation-budgets.md) — no treatment here may
   exceed a budget.
3. This document, where the baselines are silent. Every such call is marked
   **[D]** in the text and collected in §16.

Issue #538 was written **before** the baselines were approved (commit `e58300b`,
issue #547). Where the issue text and an image disagree, the image wins and the
divergence is recorded in §15.

**Non-negotiables carried in.** Zero game logic in the client — every value below
renders exactly as the server supplied it (`AGENTS.md`). No hex literal in a
component: all colors and sizes flow from
[`clients/web/src/tokens.ts`](../../clients/web/src/tokens.ts) (§5). No official
card images, frames, symbols, or Oracle text are bundled
([ADR 0031](../decisions/0031-bundled-asset-policy.md)); the only player-side
exception is [ADR 0024](../decisions/0024-user-side-card-art.md) (§12).

---

## 1. Measurement method

All ratios are read off `rune-card-states.jpg` (1672×941), which is
orthographic and therefore metrically usable.
`rune-card-system-overview.jpg` and `rune-2.5d-interface-baseline.jpg` are drawn
in the scene's perspective and are used for **channel placement, silhouette, and
color**, never for exact proportions.

Reference measurements (pixels, states sheet):

| Object | Panel | Box (w × h) | Aspect (w/h) |
| --- | --- | --- | --- |
| Battlefield permanent | 1, 3–7 | 191 × 190 | **1.00** |
| Full card (spell on stack) | 9 | 197 × 277 | **0.711** |
| Land resource tile | overview, LANDS row | 130 × 89 | **1.46** |

Every ratio below is expressed as a fraction of the card's **width `W`** (type
and geometry) or its **height `H`** (band stack), so one number serves every
tier. The reference authoring width is `Wref = 190 px`.

---

## 2. Why this frame is Rune's own

The specified frame is original by construction, not by disclaimer. Four
structural properties place it away from any MTG/Arena frame family:

| Property | Rune | Why it is not a reskin |
| --- | --- | --- |
| **Two silhouettes, not one** | A permanent on the battlefield is a **1:1 square plaque**; a card in hand / on the stack is a 0.71 portrait card. The square is the frame with its rules area structurally removed. | Every printed-card frame family has exactly one aspect. Rune's board object is a different *shape*, born from the fact that a battlefield card has no rules to show. |
| **Material: slate plaque + parchment plates** | A dark slate body with visible paper thickness at the bottom edge, a single warm-gold hairline rule inset ~6% of W, and **discrete parchment plates** (title, P/T, glyph) floating on it. | Printed frames are one continuous printed sheet with a beveled art box. Rune's information sits on *separate physical plates* riveted to a slab — the tabletop-object read, not the printed-card read. |
| **Color identity is an edge accent, never a body fill** | The WUBRG accent (`PALETTE`) is a rule/edge tint only; the body stays neutral slate at every color. | Printed frames key the whole card body to color. Rune's board reads as one material at every color, so color never fights the state overlay layer. |
| **State lives in a dedicated band, not the art box** | The bottom **status band** is a permanent structural element that exists on every permanent whether or not it has state. | No printed frame reserves a status channel; overlays there are always additive UI stuck on top of a printed face. |

Additionally: no printed-frame ornament (no scrollwork, set-symbol slot, rarity
gem, collector line, or printed mana-symbol shapes), no printed typography
pairing, and no official color values. The emblem language (rune spiral, §13) is
Rune's own, carried from the shipped glyph set. This section answers #538's
"original frame silhouette/material/color study" acceptance line.

---

## 3. Frame anatomy

### 3.1 Outer silhouette

| Property | Value | Source |
| --- | --- | --- |
| Full-card aspect (w/h) | **0.715** | measured 0.711, panel 9 |
| Permanent aspect (w/h) | **1.00** | measured 0.98–1.01, panels 1–7 |
| Land tile aspect (w/h) | **1.45** | measured 1.46, overview |
| Corner radius | **0.070 · W** | states sheet |
| Plate / art-window radius | **0.035 · W** | states sheet |
| Outer edge (dark slate) | **0.037 · W** on left/right/top | measured 7 px at W=190 |
| Bottom edge (paper thickness) | **0.057 · W** — the bottom edge is thicker and one shade darker | measured 11–13 px, panel 9 |
| Inner gold hairline | weight **0.010 · W**, inset **0.063 · W** from the card edge, follows the outer radius | measured 2 px / 12 px |
| Gold rivet dots | back only (§13) | overview |

The silhouette is identical for hand, battlefield, token, land, stack, and back;
only the aspect and the band stack change. A token adds an arched top (§3.9).

### 3.2 Band stack — full card (hand, stack, inspect)

Fractions of card height `H`, top to bottom. Measured on panel 9.

| # | Band | Height | Notes |
| --- | --- | --- | --- |
| 1 | Top edge | 0.022 · H | slate |
| 2 | Gold rule | 0.007 · H | |
| 3 | **Title bar** | **0.077 · H** | parchment plate, name left, cost disc right |
| 4 | Gold rule | 0.007 · H | |
| 5 | **Art window** | **0.482 · H** | |
| 6 | Gold rule | 0.010 · H | |
| 7 | **Type bar** | **0.077 · H** | parchment plate, type line left |
| 8 | Gold rule | 0.007 · H | |
| 9 | **Rules area** | **0.270 · H** | parchment, server `rules_text` |
| 10 | Gold rule + bottom edge | 0.047 · H | |

A creature's **P/T plate** hangs at the bottom-right of band 9, overhanging the
bottom edge by up to 0.25 of its own height (overview, hand row).

### 3.3 Band stack — battlefield permanent

Fractions of card height `H` (`H = W`). Measured on panel 1.

| # | Band | Height | Notes |
| --- | --- | --- | --- |
| 1 | Top edge | 0.037 · H | slate |
| 2 | Gold rule | 0.016 · H | |
| 3 | **Title bar** | **0.100 · H** | parchment plate, name only — **no cost** |
| 4 | Gold rule | 0.016 · H | |
| 5 | **Art window** | **0.647 · H** | |
| 6 | Gold rule | 0.005 · H | |
| 7 | **Status band** | **0.137 · H** | slate; glyph plates left, P/T plate right |
| 8 | Gold rule + bottom edge | 0.042 · H | |

**The battlefield face carries no mana cost and no type bar.** Both are absent in
every permanent across all three baselines. Type identity is carried by the
keyword/ability glyph plates in the status band and by inspect.

### 3.4 Color identity treatment

| Element | Treatment |
| --- | --- |
| Gold hairline rule | tinted 35% toward `PALETTE[colorIdentity]`; stays gold-dominant |
| Art-window inner rule | 100% `PALETTE[colorIdentity]` at 0.010 · W |
| Status band | neutral slate at every identity |
| Card body / plates | neutral at every identity |
| Multicolor / colorless / land | `PALETTE.M` / `.C` / `.L`, unchanged from the shipped tokens |

Color identity is **display glue**, produced by
[`table/colorIdentity.ts`](../../clients/web/src/table/colorIdentity.ts) from the
server type line and cost string — never a rules computation. Frame color is game
information and never encodes ownership (`visual-system.md` §2).

### 3.5 Title bar

| Property | Value |
| --- | --- |
| Inset from card edge | 0.063 · W each side |
| Present at | `field` and `support`; at `mini` the plate is the color-identity strip and at `chip` there is no band at all (§8.4) |
| Height (permanent) | 0.100 · H, floored (§9.3) |
| Height (full card) | 0.077 · H, floored |
| Fill | `SURFACES.plate` parchment, `RUNE_GOLD.plateRim` hairline rim |
| Name | left-aligned, `RUNE_TYPE.name = 0.074 · W`, ink `SURFACES.plateInk`, OFL display face |
| Overflow | single line, ellipsis at the cost disc's left edge — never wraps, never shrinks below the floor |
| Cost disc (full card only) | dark disc `SURFACES.costDisc`, diameter **0.14 · W**, numeral 0.085 · W in `SURFACES.nameText`, right-aligned, centre on the title bar's right end, overhanging the frame by ≤ 40% of its diameter |

Colored mana symbols keep the shipped `PIP` swatches at the hand and inspect
tiers; the disc renders the converted numeral plus colored pips where the width
allows, and the numeral alone where it does not.

### 3.6 Art window

| Property | Value |
| --- | --- |
| Inset | 0.063 · W each side; top/bottom bounded by the adjacent gold rules |
| Mask | rounded rect, radius 0.035 · W; illustration **cover-cropped**, focal point centred |
| Inner bevel | 1 px inner shadow at 0.35 alpha, top-left, from the single key light (§3.11) |
| Procedural fill | color-identity field keyed by `functional_id` + the accent monogram (`FRAME.monogramAlpha`), the ADR 0024 default |
| Scrim | `ART.scrimAlpha` card-body scrim behind any glyph or badge that overlaps art |

Focal-safe rule: the central 60% × 60% of the art window must never be covered by
a badge, glyph, ring, path, or plate at any tier.

### 3.7 Type bar

Full card and inspect only. Parchment plate, height 0.077 · H, type line
left-aligned at `RUNE_TYPE.typeLine = 0.056 · W`. A legendary/supertype marker
renders as a small gold crown glyph at the plate's right end **[D]** — the
baselines show a crown on command-zone cards only.

### 3.8 Rules area

Full card and inspect only. Parchment field, `RUNE_TYPE.rules = 0.056 · W`,
ink `SURFACES.plateInk`, left-aligned, 1.35 line height. Renders
`CardView.rules_text` verbatim; the client never formats, abbreviates, or infers
rules. Overflow at the hand tier: clip with a fade and a "…" affordance; inspect
always shows the full string (its budget is "everything the server supplies").

### 3.9 P/T plate and the status band

| Property | Value |
| --- | --- |
| Status band fill | `SURFACES.statusBand` slate, spanning the full inner width |
| P/T plate | parchment, gold rim, right-aligned, height 0.115 · H (permanent), width fits `P/T` + 2 × 0.03 · W padding |
| P/T numerals | `RUNE_TYPE.pt = 0.115 · W`, ink `SURFACES.plateInk`, `/` separator |
| Glyph plates | left-aligned, up to **3** square parchment plates, side 0.105 · H, gap 0.015 · H, black glyph strokes |
| Glyph overflow | the third plate becomes `+N` |
| Loyalty | same plate, numeral only, no `/` |

The P/T plate always shows the **server-computed current** value, never a printed
one. It is the single authoritative characteristic surface and **must remain
visible in every state and every art mode** (ADR 0024).

**Token variant.** A token has no title bar. Its top edge is an **arch** (dome
rising 0.09 · H above the card's top line), and a dark `TOKEN` tab plate straddles
that arch centred. The art window extends into the space the title bar would have
used. Status band, glyph plates, and P/T plate are unchanged. At `mini` and
`chip` the arch alone carries the token read; the word tab is dropped.

### 3.10 State overlay layer

One layer above art and plates, below nothing. Ordering, back to front:

1. Tap rotation and dim (transform + opacity on the root — not a layer)
2. Splay/pile edges (box-shadow, behind the face)
3. Edge bars: actionable (bottom), attacking (top), blocking (left)
4. Rings and glows: selection, target candidate, chosen target
5. Badge rail (counters, damage), top tab (`TOKEN` / `×N`)
6. Drawn paths and combat links (scene layer, above all faces)

Rule: **nothing in this layer may cover the title text, the cost disc, the art
window's focal-safe centre, or the P/T plate.** Badges may overhang the frame
edge outward.

### 3.11 Light model

One implied key light, high and slightly toward the viewer
(`visual-system.md` §3), unchanged. Card-specific consequences:

| Feature | Treatment |
| --- | --- |
| Bottom edge | thicker and darker than the other three — the paper-thickness read |
| Plates | 1 px light top rim, 1 px dark bottom rim |
| Gold rule | lit on the top/left run, `RUNE_GOLD.ruleShade` on the bottom/right run |
| Art window | inner shadow top-left |
| Elevation | `PROVISIONAL.lift` / `.shadow` in `card/dom/theme.ts` — rest / lifted / held, unchanged |

Selection and focus **lift and straighten**; they never brighten the body.

### 3.12 Frame plates — the material the frame is made of

Issue #570. §3.11 describes a light model the frame could not actually carry:
every feature above was approximated in CSS — the edge with a border, the bevel
with a box-shadow, the printed surface with a flat fill — so a card could only
ever be a light rectangle with lines drawn on it, which is why it read as
imported from a different product than the chrome around it. This section is
the material that was missing. #529 owns the frame's *geometry and information
hierarchy*; this owns what the frame is *made of*, and lands first so the
hierarchy is cut against plates rather than re-tuned twice.

Seven plates ship, generated and bundled under the same ledger as every other
asset set (`clients/web/public/assets/frames/`, ADR 0031):

| Plate | Surface | Treatment |
| --- | --- | --- |
| `frameEdge` | the card's outer edge | outer contour, lit slate bevel, the thicker bottom paper edge, and the gold hairline **engraved** rather than stroked |
| `artSeam` | the art window's surround | a recessed lip — shadowed on the light side, catching light on the far side — so the window sits *in* the card |
| `headerField` | the title band | a raised printed field the name and cost sit **on** |
| `infoStrip` | the type bar and rules area | a recessed printed strip the text sits **in** |
| `statusStrip` | the permanent's status band | a slate channel cut a shade deeper, so its glyph and P/T plates read as objects lying in it |
| `ptPlate` | the P/T plate | its own asset, tighter radius, stronger bevel — §3.9's authoritative surface as a distinct object |
| `identityWeave` | the identity strip and the procedural art field | a seamless material tile the accent is tinted **through** |

Five properties bind, and each answers a constraint the frame's position in the
client imposes:

1. **Every plate is an alpha light map.** A plate carries highlight, shadow,
   grain, and — on `frameEdge` only — the structural gold hairline. It carries
   no body colour: every fill still arrives from `tokens.ts` and shows through
   the plate. That keeps ADR 0019 intact, serves both environment themes and a
   future light mode from one set, and is what lets **one** 64 px tile give all
   eight colour identities a material instead of an asset per colour. §3.4's
   rule is unchanged and now enforceable — an identity is an accent over a
   surface, never the flat colour block a tinted plate would have become.
2. **Nine-slice, banded on a ratio of `W`.** Each plate declares the fraction of
   the card width its band occupies; the face resolves it per tier into
   `border-image-width`. One asset therefore survives the hand fan, the inspect
   panel, and the battlefield chip, in all three silhouettes of §3.1 — the
   authored bevel is the same *proportion* at every tier, exactly as every
   other length in this document is. `frameEdge`'s band is
   `ruleInset + rule` by construction, so the plate hands off to the card body
   with no seam.
3. **Zero nodes.** Plates are `border-image` and `background-image` on boxes the
   face already renders, so the ≤ 12-node battlefield ceiling (§9) is untouched
   at every tier and for every input.
4. **Never load-bearing.** Every rule that composes a plate keeps its token
   treatment underneath — the hairline is still a real border, the parchment
   still a real background, the rims still box-shadows. A plate that 404s, or a
   browser that declines `border-image`, leaves the frame rendering exactly as
   it did before this set landed. Nothing is awaited and no layout depends on a
   plate arriving.
5. **Generated by a committed script, not by a model.** Frame material is
   measured geometry, so the tool is arithmetic:
   `clients/web/scripts/generateFramePlates.js` (`npm run frames`) synthesises
   the pixels, content-hashes them, and rewrites both the manifest section and
   the ledger entries. Provenance is ADR 0031 class 1, and a re-run reproduces
   the same bytes — **checked**, not claimed: the suite re-encodes every plate
   and compares it byte-for-byte with the committed file, so regeneration is a
   verified no-op and any drift fails CI instead of silently rewriting seven
   assets. That check is why the plates ship as PNG from an in-process encoder
   rather than WebP through an external converter whose version would decide
   the bytes — and why DEFLATE is implemented in the generator rather than taken
   from `zlib`: CI produced different bytes for the one RGBA plate on the first
   run, and a content hash that depends on the machine's zlib is not a content
   hash. ADR 0031 allows PNG "where a consumer requires it"; the consumer here
   is the ledger's own reproducibility claim, and the price is about a fifth of
   zlib's ratio. The whole set is ~78 KB — the frame is on every card, so it is
   the most budget-sensitive set in the project.

The material reaches the face as **one custom property per surface**, each
already carrying its slice and its band (the band as a `calc()` on the
`--face-w` the tier publishes anyway). That is a budget decision, not a style
one: the face's style attribute is what the plane reconciler rewrites on every
view, and publishing source, slice, and band separately cost ~29% of the
reconnect rebuild budget on a 120-permanent board. Collapsed, the set costs
about half that, and it is the only ongoing cost the plates impose.

Card backs (§13) need nothing further: the production raster skins shipped with
#555 and already carry the same silhouette, radius, and edge treatment.

---

## 4. Surface contract

One decided representation per surface. No implementation PR chooses ad hoc.

| Surface | Silhouette | Bands present | Cost | Rules | P/T | Distinctive |
| --- | --- | --- | --- | --- | --- | --- |
| **Hand** | 0.715 full card | title, art, type, rules | yes (disc) | yes | plate, overhanging | fan member, screen space |
| **Selected hand** | same, elevation `held` | same | yes | yes | yes | blue ring + glow; straightens, lifts, grows one tier; actions appear in the dock, never over the card |
| **Battlefield creature** | 1.00 permanent | title, art, status | **no** | **no** | plate | glyph plates carry ability identity |
| **Battlefield noncreature** | 1.00 permanent | title, art, status | **no** | **no** | none (band keeps glyph plates) | attachments cluster with host (§7.6) |
| **Battlefield land** | 1.45 **resource tile** | art only, framed | no | no | no | no title bar; tap state and the mana glyph plate (bottom-left) only; expands to the full card on focus/inspect; a nonbasic or actionable land **never** collapses to an anonymous chip |
| **Token** | 1.00 permanent + arch | arch tab, art, status | no | no | plate | `TOKEN` tab; no cost reservation; strong ×N pile |
| **Spell on stack** | 0.715 full card | title, art, type, rules | yes | yes | plate if creature | screen-space stack-rail slot; the **slot** wears the controller's seat accent and the order index **[D]** |
| **Ability on stack** | 0.715 **ability plate** | source thumbnail, description field | no | server description | no | not a fake card: a slate plate with a 0.30 · W circular source thumbnail top-left, the server `description` as body text, controller ribbon and order index on the slot, and an anchor line back to the source permanent **[D]** |
| **Inspect** | 0.715 full card, fixed screen tier | all, at reading size | yes | full | plate | the card brought forward — the same `CardFace`, at the `inspect` tier (#569), never a second renderer re-listing the fields. An **annex** beside it carries what a printed face has no home for: spelled-out keyword names, counters, damage, attachments, linked objects, and the art-source entry point. Never depends on battlefield card size |
| **Dragged hand card** | 0.715 full card, elevation `held` | as hand | yes | yes | plate | the real face is the drag proxy (#569, `control-language.md` §6.2 stage 2); the origin slot is held open, not left looking occupied |
| **Card back** | matches the surface's silhouette | §13 | — | — | — | one skin per device, applied to every hidden card |
| **Folded identical stack** | top card at full fidelity | as its kind | as its kind | as its kind | as its kind | up to 3 splayed edges **down-and-left**; `×N` tab on the top edge |

---

## 5. Token additions

New values are token additions to
[`clients/web/src/tokens.ts`](../../clients/web/src/tokens.ts), named here so the
implementing PR adds exactly these and no literals. Ratios are unitless fractions
of the card width `W` or height `H`; the face resolves them to px per tier.

| Block | Key | Value | Meaning |
| --- | --- | --- | --- |
| `RUNE_FRAME` | `aspectFull` | `0.715` | full-card w/h |
| | `aspectPermanent` | `1.00` | battlefield permanent w/h |
| | `aspectLandTile` | `1.45` | land resource tile w/h |
| | `radius` | `0.070` | outer corner radius ÷ W |
| | `plateRadius` | `0.035` | plate / art-window radius ÷ W |
| | `edge` | `0.037` | slate edge ÷ W |
| | `edgeBottom` | `0.057` | bottom paper-thickness edge ÷ W |
| | `rule` | `0.010` | gold hairline weight ÷ W |
| | `ruleInset` | `0.063` | hairline inset ÷ W |
| | `archRise` | `0.090` | token arch rise ÷ H |
| `RUNE_BANDS_FULL` | `title` `art` `type` `rules` | `0.077` `0.482` `0.077` `0.270` | ÷ H, §3.2 |
| `RUNE_BANDS_PERM` | `title` `art` `status` | `0.100` `0.647` `0.137` | ÷ H, §3.3 |
| `RUNE_TYPE` | `name` `typeLine` `rules` `pt` `cost` `tab` `badge` | `0.074` `0.056` `0.056` `0.115` `0.085` `0.075` `0.070` | font size ÷ W |
| | `floorName` `floorValue` | `11` `12` | px floors (budgets §Accessibility) |
| `SURFACES` (extend) | `frameEdge` | `#2E343A` | slate edge |
| | `frameEdgeShade` | `#1B2024` | bottom edge / shadow side |
| | `plate` | `#DED8CB` | parchment plates |
| | `plateInk` | `#191C20` | plate text |
| | `statusBand` | `#2F3438` | status band slate |
| | `costDisc` | `#20262B` | cost disc fill |
| | `tokenTab` | `#20262B` | `TOKEN` / `×N` tab fill |
| `RUNE_GOLD` | `rule` | `#C7A46A` | lit gold hairline |
| | `ruleShade` | `#8A7042` | shadow side |
| | `plateRim` | `#B9955E` | plate rim |
| `INDICATORS` (extend) | `selectRing` | `#7FB2E5` | selection core — canonical `SURFACES.selection` (`visual-system.md` §2) |
| | `selectGlow` | `#7FB2E5` | selection outer bloom — same hue at bloom alpha; spread, not hue, separates it from the ring |
| | `targetPath` | `#E0784A` | drawn targeting path — canonical `SURFACES.targeting` |
| | `targetReticle` | `#E0784A` | reticle on the chosen target — same hue; geometry separates it from the path |
| | `counterBg` / `counterText` | `#2A5436` / `#EAF3E9` | counter badge (measured green) |
| | `damageBg` / `damageText` | `#8E3A2A` / `#F6E7E4` | damage badge (measured red) |
| `SPLAY` (amend) | `stepX` `stepY` | `0.055` `0.030` | ÷ W — down-and-**left**, replacing the current 2 px up-and-right |
| | `maxLayers` | `3` | unchanged |
| `CARD_BACK` | `field` `emblem` `rivet` | `#2B3340` `#C7A46A` `#C7A46A` | §13 |

`PALETTE`, `PT_TEXT`, `PIP`, `AFFORDANCE`, `TAP`, `ART`, `COMBAT_LINK`, and
`BADGE` are carried unchanged.

---

## 6. States sheet

### 6.1 The nine panels of `rune-card-states.jpg` (transcribed)

| # | State | Channel(s) | Spec |
| --- | --- | --- | --- |
| 1 | **Normal** | — | Resting: contact shadow, full opacity, no overlay. |
| 2 | **Tapped** | rotation + dim | `TAP.angle` (25°) clockwise, `FRAME.tappedAlpha`. Footprint pre-reserved. One treatment at every tier and for every seat. A declared attacker keeps full opacity while tapped. |
| 3 | **Selected** | **blue ring** + elevation | 0.021 · W ring in `INDICATORS.selectRing` following the outer radius, plus a soft outer bloom in `selectGlow` at ~0.05 · W spread; elevation `held`; the card straightens. The sheet draws this ring and bloom **violet**; the maintainer's ruling assigns selection blue and supersedes the sheet (§15.1). Geometry is transcribed unchanged. |
| 4 | **Targeted** (chosen target) | **orange path + reticle** | A drawn path in `targetPath` terminates in a circular reticle (`targetReticle`, diameter 0.24 · W, 0.016 · W stroke) centred on the target's art window. Path and reticle live on the scene layer, above every face. The sheet draws both **blue**; the ruling assigns targeting orange and supersedes the sheet (§15.1). Geometry is transcribed unchanged. |
| 5 | **Counters + damage** | shaped badges | Counter badge: rounded rect, `counterBg`, two lines (`kind` over `×N`), docked **lower-left** of the art window. Damage badge: **torn/cracked silhouette** — deliberately not a rounded rect — `damageBg`, single numeral, docked **lower-right** of the art window. Both seat wholly inside the art window so the status band, glyph plates, and P/T plate stay visible (**[D]**, see §15.2). |
| 6 | **Attachment** | physical cluster | Attached permanents render as cards at **0.70 × the host's size**, offset down-and-right by `(0.42 · W, 0.30 · H)` from the host's box, stacked behind one another at `(0.06 · W, 0.06 · H)` per additional attachment, drawn **below** the host in z-order. A thin light connector runs host → attachment. The cluster moves, taps, and folds as one unit. |
| 7 | **Identical stack** | splayed pile + count | Top card at full fidelity; up to 3 card edges behind it, offset **down-and-left** per `SPLAY`; `×N` tab on the top edge (§7.4, **[D]** — the sheet also shows bottom-right; rejected because bottom-right is the P/T channel). |
| 8 | **Token group** | arch + tab + pile | Token silhouette (§3.9) plus the identical-stack pile treatment. The top tab reads `TOKEN ×N` as one plate when both apply. |
| 9 | **Spell on stack** | full card in a rail slot | Full 0.715 card, all four bands, in a screen-space stack-rail slot. The slot — not the face — carries the controller's seat accent stripe and the order index (`visual-system.md` §5). |

### 6.2 States the baselines do not show — all decided here **[D]**

| State | Color | Non-color channel | Spec |
| --- | --- | --- | --- |
| **Hover / keyboard focus** | — | elevation | Elevation `lifted` (`PROVISIONAL.lift.lifted`), 80–150 ms. No ring, no tint — hover must never be mistaken for selection. |
| **Actionable** | gold `AFFORDANCE.actionable` | **bottom edge bar** | Carried unchanged from the shipped client and `visual-system.md` §7: a solid bar of `AFFORDANCE.edgeHeight` across the bottom edge, riding the status band's lower rule. Driven only by `valid_actions[]` containing an action for this entity. |
| **Target candidate** | orange `SURFACES.targeting` | ring + steady beacon pulse | Full-perimeter ring at 0.016 · W; reduced motion renders the ring static. Distinct from the blue selection ring by hue *and* weight, and from the chosen target — which shares the orange targeting hue — by the absence of a path and reticle. |
| **Attacking** | `INDICATORS.attackingBar` | **top edge bar** + outgoing path + 6° tilt | Carried from #332. Full opacity retained while tapped. |
| **Blocking** | `INDICATORS.blockingBar` | **left edge bar** + doubled-stroke link | Carried from #332/#339. |
| **Unavailable / ineligible** | — | dim | `FRAME.dimmedAlpha` multiplicative, non-interactive, during an active prompt only. |
| **Summoning sick** | — | marker glyph plate | A dedicated glyph plate in the status band's left group (not a dim), so it survives at every tier and never competes with tap. Replaces `FRAME.sickAlpha` for the new frame. |
| **Latent activated ability** | `INDICATORS.abilityMarker` | marker dot on the title bar's right end — the color-identity strip's right end where the strip replaces it (§8.4) | Carried; distinct from the gold bar (latent vs live). |
| **Commander** | — | gold crown plate, status band left group | Requires a wire field — see §14 gap G7. |
| **Face-down permanent** | — | card back in the permanent silhouette | Requires a wire field — see §14 gap G8. |

Non-color rule (budgets §Accessibility): **no card state is color-only at any
quality level.** Every row above names a shape, position, or transform channel.

---

## 7. Counters, badges, glyphs, and placement channels

### 7.1 The five channels

| Channel | Position | Occupants | Cap |
| --- | --- | --- | --- |
| **Top tab** | top edge, centred, overhanging | `TOKEN`, `×N`, `TOKEN ×N` | 1 plate |
| **Title-right** | title bar, right end | cost disc (full card), latent-ability dot (permanent) | 1 |
| **Art lower-left** | inside art window | counter badges | 2, then `+N` |
| **Art lower-right** | inside art window | damage badge | 1 |
| **Status band** | full width below art | glyph plates (left, ≤3), P/T plate (right) | fixed |

Edge bars (actionable/attacking/blocking) and rings are **not** badges: they ride
the frame edge and consume no channel.

### 7.2 Counter badges

- Up to **two** counter kinds render as shaped badges in the art lower-left,
  stacked upward; further kinds collapse into a single `+N` badge that opens the
  full list on focus/inspect.
- `+1/+1` and `-1/-1` are always among the two shown when present.
- Badge shape is a rounded rect at every kind; the **kind text** is the
  information channel, never the color alone.
- At `mini` and `support` the badge degrades to a **compact circular badge** on
  the card's top-right, carrying the count only (the form the interface baseline
  shows) — the kind then lives in inspect.

### 7.3 Damage badge

Torn/cracked silhouette in `damageBg`, art lower-right, numeral only. It is
**never** merged into the P/T plate: the plate shows the server's current
toughness, damage is a separate marked value (`Permanent.damage`, CR 120.3).

### 7.4 The `×N` tab

Dark `tokenTab` plate on the top edge, centred, overhanging by ~50% of its own
height, `RUNE_TYPE.tab` text in `SURFACES.nameText`. Carries the exact count at
any N; the splayed pile depth is capped at `SPLAY.maxLayers` and never scales
with N. Zero DOM nodes for the pile itself (box-shadow layers, carried).

### 7.5 Ability and keyword glyphs

The shipped glyph language (`chrome/glyphs.ts`) is carried unchanged. Glyphs
render as black strokes on parchment plates in the status band's left group,
capped at 3 plates with `+N` overflow. Text names appear in inspect only.
Activated-ability **availability** (the gold bar) is always visually distinct
from activated-ability **existence** (the latent marker dot).

### 7.6 Attachments

Attachments are a spatial cluster, not a badge (§6.1 panel 6). The host's own
badges and plates are never displaced by an attachment; if a cluster would push
past its region's slot, the cluster tightens its offsets before anything else
degrades.

---

## 8. Dimensions and type scale

### 8.1 Canonical 4-player, 1280 × 720

| Surface | Tier | Footprint (w × h) | Name | Type | Rules | P/T | Nodes (§9) |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Receiver permanent | `field` | 96 × 96 | 11 | — | — | 13 | 10 |
| Receiver land tile | `field` land | 96 × 66 | — | — | — | — | 4 |
| Focused opponent permanent | `support` | 78 × 78 | 11 | — | — | 12 | 9 |
| Focused opponent land tile | `support` land | 78 × 54 | — | — | — | — | 4 |
| Wing permanent | `mini` | 62 × 62 | — (strip, §8.4) | — | — | 12 | 8 |
| Digest chip | `chip` | 48 × 48 | — | — | — | 12 | 4 |
| Hand card | `hand` | 116 × 162 | 13 | 11 | 11 | 14 | exempt |
| Stack entry | `stack` | 104 × 145 | 12 | 11 | 11 | 13 | exempt |
| Inspect | `inspect` | 260 × 364 | 18 | 13 | 13 | 20 | exempt |

### 8.2 Canonical 4-player, 1680 × 945

Battlefield tiers scale by **1.31**, screen-space tiers by **1.20** (screen-space
text is already at reading size; it grows more slowly than the plane).

| Surface | Tier | Footprint | Name | Type | Rules | P/T |
| --- | --- | --- | --- | --- | --- | --- |
| Receiver permanent | `field` | 126 × 126 | 12 | — | — | 15 |
| Receiver land tile | `field` land | 126 × 87 | — | — | — | — |
| Focused opponent permanent | `support` | 102 × 102 | 11 | — | — | 13 |
| Wing permanent | `mini` | 81 × 81 | — (strip, §8.4) | — | — | 12 |
| Hand card | `hand` | 139 × 194 | 15 | 12 | 12 | 17 |
| Stack entry | `stack` | 125 × 175 | 14 | 12 | 12 | 15 |
| Inspect | `inspect` | 312 × 437 | 20 | 14 | 14 | 22 |

### 8.3 Six-player and phone degradation

| Geometry | Receiver | Far side | Wings | Hand | Notes |
| --- | --- | --- | --- | --- | --- |
| 6p @ 1280 × 720 | `field` 96 | `mini` 62 | **digest** — chips 48, category counts | 116 × 162 | wings digest by the `layout-model.md` 0.225 · W threshold; candidates pierce the rung at `support` 78 |
| 6p @ 1680 × 945 | `field` 126 | `support` 78 | `mini` 62 | 139 × 194 | one rung better throughout |
| Phone 390 × 844 | `support` 78 | `mini` 62 | summary tiles | 88 × 123 | hand fan pages at the 44 px floor; stack opens as a sheet at `stack` 96 × 134 |

### 8.4 The floor rule

Ratios are authored at `Wref = 190`. Below that, type is **clamped, and the band
that holds it grows**:

```
nameSize   = max(RUNE_TYPE.floorName,  RUNE_TYPE.name * W)
valueSize  = max(RUNE_TYPE.floorValue, RUNE_TYPE.pt   * W)
titleH     = max(RUNE_BANDS_PERM.title  * H, nameSize  * 1.35)
statusH    = max(RUNE_BANDS_PERM.status * H, valueSize * 1.35)
artH       = H - edges - rules - titleH - statusH      // art absorbs the difference
```

Consequences, stated so no implementation re-derives them:

- At `field` (W = 96) the authored name size would be 7.1 px; it clamps to 11 px
  and the title bar grows from 9.6 px to 15 px.
- At `mini` (W = 62) the clamp turns the tier inside out: an 11 px name grows the
  title bar to 14.9 px and a 12 px P/T grows the status band to 16.2 px, so
  **half** the card's height is type and the art window falls to 0.357 · H
  against an authored 0.647. The `chip` treatment therefore applies one rung
  earlier — the title bar is **replaced** by the color-identity strip, the same
  band box at its authored 0.100 · H, carrying no text and so taking no floor.
  The art window absorbs the difference (0.496 · H) and identity moves to the
  accent + glyph + inspect path. The strip keeps the title band's two overlay
  channels, the attacking top bar and the latent-ability marker dot, so no
  server-computed channel is lost with the name. Every tier above `mini` keeps
  the parchment name plate at the 11 px floor.
- At `chip` (W = 48) the name cannot reach the 11 px floor inside the band at
  all, and the band no longer earns even a strip: the title bar is **dropped**,
  the art window's own 100% identity rule (§3.4) carries the accent alongside the
  basic-land or type glyph, and identity moves to the glyph + inspect path
  (budgets §Accessibility, carried).
- Every battlefield tier keeps a ≥ 44 px hit target regardless of drawn size.
- Text scaling to 125% grows the floors; the art window absorbs it. Critical
  values never clip.

---

## 9. DOM node budget

Binding ceiling: **≤ 12 nodes per card face at battlefield tiers**
(`presentation-budgets.md` §Performance). Zero-node channels are carried from the
shipped `CardFace`: rings and pile splay are box-shadows; edge bars, monogram,
and the ability dot are pseudo-elements; tap, dim, and elevation are
transform/opacity.

| Tier | Nodes | Composition |
| --- | --- | --- |
| `chip` | **4** | root, inner, art/glyph, consolidated badge |
| `mini` | **8** | root, inner, art, **color-identity strip** (§8.4), status band, glyph `<svg>` + path, P/T |
| `support` | **9** | + consolidated badge row |
| `field` | **10** | + top tab (`TOKEN` / `×N`) |
| `field` land tile | **4** | root, inner, art, badge |
| `field` + full-card art mode | **10** | full-card `<img>` replaces art + title + band content; overlays keep their nodes |
| `hand` / `stack` / `inspect` | exempt | screen space; per-symbol pips and per-badge spans return |

Headroom at `field` is **2 nodes**. Rules for staying inside it:

- No element may scale with its input. The glyph strip is one `<svg>` with
  combined paths and a capped `+N`; the badge row consolidates to one node at
  every battlefield tier; the pile is box-shadow only.
- Attachments are **separate faces**, each with its own budget — a cluster of
  three is three faces, not one 30-node face.
- The status band is one node that contains the glyph `<svg>` and the P/T plate;
  it is not a wrapper per plate.

---

## 10. Compression and degradation ladder

Engaged per region, independently. The order is binding (issue #538, aligned with
`layout-model.md` §The degradation ladder):

| # | Step | Effect on the card |
| --- | --- | --- |
| 1 | **Gaps** | row/column gaps shrink to the tap-swept minimum; no face changes |
| 2 | **Overlap** | faces overlap by ≤ 0.25 · W along the row; the title bar, P/T plate, and status band of every card stay exposed |
| 3 | **Fold identicals** | equal `cardVisualSignature` permanents fold into one splayed pile + `×N` tab |
| 4 | **Shorten battlefield rules** | (full-card art mode only) the printed rules region is cropped out of the drawn face; Rune's own battlefield face has no rules area to shorten |
| 5 | **Simplify secondary glyphs** | glyph plates cap 3 → 2 → 1 + `+N`; counter badges take the compact circular form (§7.2) |
| 6 | **Reduce peripheral tier** | wing regions step down a rung, then to the digest |

Guarantees, at every rung:

- Local and focused objects, and **every current prompt candidate**, degrade
  **last** and always render individually and pickably (`layout-model.md`).
- Combat participants, attachment clusters, and the current selection never fold.
- A stressed representation **returns to the full baseline tier** when density
  clears — degradation is a function of the current view, never sticky state.
- Steps 1–3 change no glyph or text; steps 5–6 never remove the P/T plate.

---

## 11. Motion

Card motion is specified by `visual-system.md` §8 and capped by
`presentation-budgets.md` §Animation. Only the frame-specific rows are added
here; each reduced-motion form is "snap to end state".

| Motion | Choreography | Duration |
| --- | --- | --- |
| Permanent ↔ full card (focus/inspect) | the square permanent grows to 0.715; type bar and rules area **wipe in** from the art window's bottom rule | 200–300 ms |
| Land tile ↔ full card | the 1.45 tile grows to 0.715; title bar wipes in from the top rule | 200–300 ms |
| Fold / unfold `×N` | pile edges fan out to individual faces along the row | 250–350 ms, batch-staggered |
| `TOKEN` arch on creation | arch and tab scale up with the token's rune circle | within the token-creation budget |

---

## 12. Art modes

Three modes, per [ADR 0024](../decisions/0024-user-side-card-art.md). The mode is
a device-local preference; art is cache, never state.

| Mode | Availability | What draws | Bundling |
| --- | --- | --- | --- |
| **Rune window** (default) | always | Rune's frame; art window holds project-owned illustration if bundled, else the procedural color-identity field keyed by `functional_id` | bundled originals only, ADR 0031 class 1–3 |
| **Scryfall window** (opt-in) | after explicit consent | identical frame; `art_crop` fills the art window, cover-cropped | never bundled, proxied, or served — device-local IndexedDB only |
| **Scryfall full-card** (opt-in) | after explicit consent | the `normal` image replaces the drawn face at every full-face tier | as above |

**Full-card mode invariants.** Rune's printed-text elements (name, cost disc,
type bar, rules) are suppressed because the image carries them. Everything below
**must still render on top, unchanged**:

| Overlay | Why |
| --- | --- |
| P/T plate (current, server-computed) | a buffed 4/4 must never read as its printed 2/2 |
| Counter and damage badges | not on the printed image |
| Tap rotation and dim | game state |
| Selection ring, target candidate ring, chosen-target reticle and path | interaction state |
| Actionable / attacking / blocking edge bars | `valid_actions` and combat |
| `TOKEN` / `×N` tab, attachment cluster | board structure |

Aspect: the printed image is ~0.72, which matches `RUNE_FRAME.aspectFull`
(0.715) to within 1%. On the **battlefield**, where Rune's silhouette is square,
full-card mode renders the image **letterboxed into the square footprint with the
top 72% of the printed card shown** (name band + art), and Rune's status band
draws over the bottom **[D]** — the printed rules are unreadable at that size
anyway and inspect carries them. Chips and land tiles stay procedural in every
mode (carried).

Failure of any art source degrades silently, per card, to the procedural face and
never blocks play.

---

## 13. Card backs

### 13.1 Default anatomy

Transcribed from `rune-card-system-overview.jpg`, upper-left `CARD BACK` callout.

| Element | Spec |
| --- | --- |
| Silhouette | identical to the face at the same surface: same aspect, `RUNE_FRAME.radius`, same slate edge and bottom paper thickness |
| Field | `CARD_BACK.field` `#2B3340` navy-slate with a low-contrast marbled grain |
| Inset rule | single gold hairline (`RUNE_GOLD.rule`), weight 0.008 · W, inset 0.075 · W, following the outer radius |
| Corner rivets | four small gold dots on the rule's corners, radius 0.012 · W |
| Emblem | centred gold rune-spiral inside a pointed-oval frame with four cardinal barbs; overall diameter **0.52 · W**, centred at (0.5 · W, 0.5 · H) |
| Text | none, at any tier |
| States | a back-facing card carries tap rotation, selection, target, and actionable channels exactly as a face does |

Hidden-information safety is a hard requirement: the back must be **the same for
every hidden card on the device**, and no feature of it may vary with, or be
inferred from, the card it hides — including its rotation. The emblem's outer
frame is 2-fold rotationally symmetric so a rotated back is indistinguishable
from an unrotated one apart from the public tap state.

### 13.2 Skin manifest contract

```jsonc
// clients/web/public/card-backs/manifest.json
[
  { "id": "rune-default", "name": "Rune Spiral",  "file": "rune-default.svg",  "default": true },
  { "id": "rune-ember",   "name": "Ember Sigil",  "file": "rune-ember.svg" }
]
```

| Rule | Statement |
| --- | --- |
| Selection scope | **device-local presentation** (`localStorage` key `rune.cardBackSkin`, the ADR 0024 / ADR 0027 idiom). It applies to every hidden card on that client and claims nothing about any other player's cosmetic. |
| Protocol | none. Per-player backs visible to opponents require an explicit future wire field and are **never inferred**. |
| Invariants every skin must hold | identical silhouette and radius; the same contrast band against the play surface; hidden-information neutrality; no face-like composition; no encoding of card identity, color, or type |
| Fallback | a missing, malformed, or failed skin falls back to `rune-default` **with no layout change** |
| Provenance | every file needs an ADR 0031 ledger entry; CI fails on an unledgered asset |
| Formats | SVG for geometric backs, AVIF/WebP for raster plates (ADR 0031 §Delivery) |

### 13.3 Placeholder status

Production raster plates for the default back and one alternate skin are
requested in **issue #548 §Request 3**. Implementation of this spec ships an
**SVG placeholder on the production manifest key** `rune-default` — same id, same
file slot, same aspect and byte budget — so replacement is a file swap plus a
ledger entry, not a rework. #548's alternate skin satisfies §13.2's "at least one
skin proof".

---

## 14. Data mapping

Every visual element and the `GameView` field or client type that supplies it.
The client renders these verbatim; it derives no characteristic.

| Visual element | Source | Notes |
| --- | --- | --- |
| Card name (title bar) | `CardView.name` | |
| Type line (type bar) | `CardView.type_line` | |
| Cost disc | `CardView.mana_cost` → `parseManaCost` | display formatting only |
| Rules area | `CardView.rules_text` | verbatim; never formatted or inferred |
| Keyword glyph plates | `CardView.keywords` → `keywordGlyphName` | unmapped keywords dropped |
| P/T plate | `CardView.power` / `.toughness` | strings, so `*` round-trips |
| Loyalty plate | `Permanent.counters[kind = "loyalty"]` | no dedicated field — see G2 |
| Color identity accent | `table/colorIdentity.ts` from `type_line` + `mana_cost` | display glue, not game logic |
| Art window content | ADR 0024 art store keyed by `CardView.functional_id` | client-local cache |
| Tap rotation + dim | `Permanent.tapped` | |
| Counter badges | `Permanent.counters[]` | rendered verbatim, never summed into P/T |
| Damage badge | `Permanent.damage` | |
| Attacking bar / tilt / path | `Permanent.attacking`, `.attacking_player` | |
| Blocking bar / link | `Permanent.blocking` | |
| `blocked ×N` badge | count of permanents whose `blocking` names this one | a count of server references |
| Attachment cluster | `Permanent.attached_to` | |
| Controller ribbon (stack slot) | `StackItem.controller` / `Permanent.controller` | seat accent on the slot, never on the card |
| Stack entry body | `StackItem.description` | |
| Ability source thumbnail / anchor | `StackItem.source` | |
| Stack order index | index within `GameView.stack` (bottom first) | |
| Actionable edge bar | `GameView.valid_actions[]` naming the entity | never client legality |
| Selection / target candidate / chosen target | ephemeral prompt state derived from `valid_actions` | not load-bearing across messages |
| `×N` fold | `cardVisualSignature` equality | a presentational fold key only |
| Card back skin | device-local manifest (§13.2) | never on the wire |
| Elevation, hover, motion | client presentation state | |

### 14.1 Gaps — visual elements with **no data source**

| # | Element | Missing | Impact | Disposition | Tracked by |
| --- | --- | --- | --- | --- | --- |
| G1 | Token identity (`TOKEN` tab, arch silhouette) | no `is_token` on `Permanent` / `CardView` | the token surface in §4 cannot be rendered correctly; type-line guessing would be client game logic | **blocking for the token surface** — needs a protocol field | #551 |
| G2 | Loyalty plate | no `loyalty` field | works via the `loyalty` counter, but "current loyalty" is a characteristic, not a counter | acceptable v1; flag for protocol review | not filed |
| G3 | Summoning sickness | no wire field; `CardDisplayData.summoningSick` has no supplier | the §6.2 sick glyph plate cannot be driven | needs a protocol field | #551 |
| G4 | Latent activated ability | no `has_activated_ability` field; today a rules-text heuristic (`ui-design-notes.md`) | the marker dot rides a heuristic | known swap point; unchanged | #551 |
| G5 | Current vs printed values in inspect | wire sends current only | inspect cannot show "4/4 (printed 2/2)" | out of scope for v1; record on inspect | not filed |
| G6 | Stack entry targets | `StackItem` has no `targets` | §4's "spell on stack … and targets" is unimplementable | needs a protocol field | #550 |
| G7 | Commander marker | `commander_damage` / `commander_tax` exist but no per-permanent commander flag | the §6.2 crown plate cannot be driven | needs a protocol field | #551 |
| G8 | Face-down permanent | no face-down flag on `Permanent` | a card back can never render on the battlefield | needs a protocol field | #551 |
| G9 | Attachment kind (aura vs equipment) | `attached_to` has no kind | the cluster connector cannot differentiate | cosmetic; acceptable v1 | #551 |
| G10 | Basic-land glyph | derived by the client from `type_line` | already shipped as display glue | acceptable | n/a — no gap |

Every "needs a protocol field" row is a contract change:
`docs/protocol.md`, `sage-protocol`, and the TypeScript mirror in the same PR
(`AGENTS.md`). **None of them may be worked around by client-side inference.**

---

## 15. Conflicts and open questions

Recorded here rather than fixed in the other documents, per this issue's scope.

### 15.1 Selection and targeting hues — **resolved by maintainer ruling**

**Ruling: selection is blue `#7FB2E5`; targeting is orange `#E0784A`**, per
`visual-system.md` §2/§7. This document now follows the ruling throughout
(§4, §5, §6.1, §6.2, and decision 9 of §16). No edit to `visual-system.md` is
owed.

The conflict, kept for the record: `visual-system.md` assigned **blue**
`#7FB2E5` to selection and **orange** `#E0784A` to targeting, while all three
approved baselines show **violet** selection rings (states panel 3; Stonehide
Behemoth in the overview and the interface baseline) and **blue** targeting
paths and reticles (states panel 4; the cast arc in both scene images). This
document originally followed the images and kept orange for *target candidate*
only. **Rejected alternative:** the baselines' violet selection / blue
targeting. The sheets' violet and blue are now read as illustrative licence;
their geometry — ring weight, bloom spread, reticle diameter, path routing —
is transcribed unchanged, because only the hue assignment moved.

Consequence to hold in implementation: *target candidate* and *chosen target*
now share the orange targeting family, exactly as `visual-system.md` §7
intends. They are separated by shape alone — candidate = ring + steady beacon
pulse; chosen = ring + drawn path terminating in a reticle. Selection keeps the
blue family to itself on the card face.

### 15.2 Panel 5 badges occlude the P/T plate

`rune-card-states.jpg` panel 5 draws the counter and damage badges across the
whole status band, hiding both the glyph plates and the P/T plate. That
contradicts issue #538 ("badges … never cover title, cost, art focal center,
P/T") and ADR 0024's rule that authoritative values always remain visible.
**Decision:** the badges' shape, color, and left/right channel are transcribed
exactly; their vertical placement is raised so they seat wholly inside the art
window and the status band stays visible.

### 15.3 Splay direction is inconsistent between panels

States panel 7 splays the identical stack **down-and-right**; panel 8 and the
overview's token pile splay **down-and-left**. **Decision:** down-and-left (two
of three depictions, and it keeps the right edge clear for the P/T plate and the
badge rail). The shipped `SPLAY` token currently offsets **up-and-right at 2 px**
and must change (§5).

### 15.4 `×N` badge placement is inconsistent

States panels 7–8 place `×N` at the pile's bottom-right; the interface baseline
places it on the top edge. **Decision:** top edge (§7.4) — bottom-right is the
P/T channel.

### 15.5 Counter badge placement is inconsistent

States panel 5 places counters at the lower-left of the art window; the interface
baseline places small circular counter badges at the top-right. **Decision:**
both, by tier — the labeled lower-left badge at `field` and above, the compact
top-right circular badge at `support` and `mini` (§7.2).

### 15.6 The interface baseline's cyan permanent rims are unexplained

The local battlefield row in `rune-2.5d-interface-baseline.jpg` shows a cyan/teal
rim on four of five permanents with no legend. It is not selection (blue ring +
elevation), not targeting (orange), and not the gold actionable bar. Under the
§15.1 ruling the rim now sits in the **same hue family as selection**, which
raises the cost of adopting it: any future meaning for the cyan rim must be
separated from selection by shape, not hue. **This spec does not
adopt it**; actionable keeps the gold bottom edge bar of `visual-system.md` §7 and
the shipped `AFFORDANCE` token. **Open question for the maintainer:** what does
the cyan rim mean, and does it replace or accompany the gold bar?

### 15.7 `rune-zones-interaction.jpg` draws a different frame

That sheet (not in this issue's binding set) uses a **dark** title bar and a
heavier gold rule, where both binding card sheets use a **light parchment** title
bar. Its stack-entry composition (controller portrait thumbnail at the entry's
lower-left) is adopted as corroboration for §4's ability plate; its frame
treatment is not. **Open question:** is the zones sheet's frame superseded?

### 15.8 Battlefield tiers are far below the authoring reference

The baselines draw permanents at ~190 px; the shipped `TIER` table tops out at
84 px on the battlefield. Every type ratio therefore clamps (§8.4), which
consumes art-window height. §8 proposes slightly larger permanent widths
(96 / 78 / 62 / 48) made affordable by the square silhouette. **Open question:**
approve the new widths, or accept a smaller art window at the current widths.

### 15.9 Land tiles and the accessibility floor

A land resource tile carries no name at any battlefield tier. `ui-requirements`
and the budgets allow the glyph + inspect path to carry identity, but #538 also
says nonbasic/actionable lands "cannot collapse to anonymous glyph chips".
**Decision:** a nonbasic or actionable land tile renders a name strip across the
tile's bottom at `field` and above (floored at 11 px), and is excluded from the
chip rung entirely; a basic land may chip. **Open question:** confirm.

---

## 16. Decision register

Everything below is a call this document makes because the baselines are silent
or inconsistent. This is the maintainer's review list.

1. **Permanent aspect is 1.00 and full-card aspect is 0.715** as two members of one
   frame family (the images show both; naming them a family is the decision).
2. **The battlefield face carries no mana cost and no type bar** — transcribed from
   the images, but stated as a normative rule.
3. **Land resource tile at 1.45 aspect** with no title bar, derived from a
   perspective measurement rather than an orthographic one.
4. **Token silhouette gets an arched top** at every tier; the `TOKEN` word tab
   drops below `support`.
5. **Top edge is the tab channel**; `TOKEN`, `×N`, and `TOKEN ×N` share it
   (§7.4, §15.4).
6. **Splay is down-and-left** at `0.055 · W` / `0.030 · H` (§15.3).
7. **Counter/damage badges seat inside the art window** so the P/T plate survives
   (§15.2).
8. **Counter badges degrade to compact top-right circles** at `support`/`mini`
   (§15.5).
9. **Selection = blue ring + bloom; chosen target = orange path + reticle; target
   candidate = orange ring** (§15.1, maintainer ruling — the baselines' violet
   selection and blue targeting are the rejected alternative).
10. **Hover/focus is elevation only** — no ring, no tint.
11. **Actionable stays the gold bottom edge bar**; the baseline's cyan rim is not
    adopted (§15.6).
12. **Summoning sickness becomes a glyph plate**, replacing the alpha dim.
13. **Attachment cluster geometry**: 0.70 scale, `(0.42 · W, 0.30 · H)` offset,
    `(0.06, 0.06)` per extra, drawn below the host.
14. **Ability stack entries are a synthetic slate plate**, not a card face, with a
    0.30 · W circular source thumbnail.
15. **Stack slot carries the controller accent and order index**, not the card.
16. **Full-card art mode on the battlefield letterboxes the top 72% of the printed
    card** into the square footprint, with Rune's status band drawn over the bottom.
17. **The floor rule** (§8.4): ratios clamp at 11 px / 12 px and the holding band
    grows, with the art window absorbing the difference.
18. **Proposed permanent tier widths 96 / 78 / 62 / 48** (§15.8).
19. **Nonbasic and actionable land tiles get a bottom name strip** and never chip
    (§15.9).
20. **Legendary/supertype marker is a gold crown at the type bar's right end.**
21. **Card back emblem sizing and rivets** (0.52 · W emblem, 0.012 · W rivets) —
    the image shows the composition, the numbers are read and rounded here.
22. **Card-back skin manifest shape and the `rune.cardBackSkin` key** (§13.2).
23. **Node counts per tier** (§9) and the two-node headroom rule at `field`.
24. **Degradation step 4 is a no-op for Rune's own battlefield face** (there are no
    battlefield rules to shorten); it applies only in full-card art mode.
25. **The color-identity strip replaces the title bar at `mini`, not only at
    `chip`** (§8.4): at W = 62 the 11 px name floor and the 12 px P/T floor
    together claimed half the card's height and left the art window at
    0.357 · H against an authored 0.647. **Rejected alternative:** keep the name
    plate and accept a tier that is mostly type. The strip is the `chip`
    treatment applied one rung earlier, not a new device — and `chip` itself
    drops the band entirely, one rung further down the same ladder.

---

## 17. Hand-offs

| Consumer | What this document decides for it |
| --- | --- |
| **#529** (card face implementation) | §3, §5, §8, §9, §12 — the frame, the tokens, the tiers, the budget |
| **#533 / #535** | the surface contract (§4) and the states sheet (§6); neither may finalize card visuals before this closes |
| **#531 / #534** (card portions) | stack and inspect surfaces (§4), badge channels (§7) |
| **#569 / #584** (card interaction surfaces) | §4's inspect, dragged-card, and browsable-pile rows: every one of them draws this frame, at a tier from §8.1, through the one renderer. A surface that shows cards and does not use `CardFace` is a defect against this document |
| **#548** | §13.3 — the card-back placeholder key and swap contract |
| **#536** (convergence gate) | §15's open questions must be answered and §16 approved before convergence is declared |
| **Protocol work** | §14.1 G1, G3, G6, G7, G8 are contract changes, each needing `docs/protocol.md` + `sage-protocol` + the TS mirror in one PR. Filed as **#551** (card state flags: G1, G3, G4, G7, G8, G9) and **#550** (`StackItem` contract: G6). G2 and G5 are not filed. |
