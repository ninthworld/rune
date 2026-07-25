# Per-seat zone geography — the zone rack, its piles, and their interaction

**The design authority for where every seat's library, graveyard, exile, and command
zone live, what each looks like, and how a player touches them** (issue #540, under
[ADR 0029](../decisions/0029-2-5d-presentation-direction.md) /
[ADR 0030](../decisions/0030-2-5d-presentation-architecture.md), master issue #464).
Design authority for the zone portions of #531 (battlefield composition), #532 (seat
clusters), #534 (contextual controls), #535 (effects), and the #536 convergence gate.

This document **codifies the approved baselines**; it does not invent geometry. The
binding image is
[`../ui-concepts/rune-zones-interaction.jpg`](../ui-concepts/rune-zones-interaction.jpg)
(commit `e58300b`, issue #547), which labels Library, Graveyard, Exile, and Command Zone
at all four seats. Supporting:
[`rune-battlefield-environments.jpg`](../ui-concepts/rune-battlefield-environments.jpg)
(rack orientation at 2/3/4/5/6 seats),
[`rune-card-system-overview.jpg`](../ui-concepts/rune-card-system-overview.jpg)
(card back and pile treatment), and
[`rune-player-control-ui.jpg`](../ui-concepts/rune-player-control-ui.jpg) (the identity
cluster this document must not duplicate).

Where the baselines decide something, the baseline wins and this document transcribes it.
Where they are silent, the choice is marked **[D]** and listed in
[§Decisions this document makes](#decisions-this-document-makes). Issue #540's prose was
written before the baselines were approved; where the two disagree, the baseline governs
and the disagreement is recorded in
[§Conflicts and open questions](#conflicts-and-open-questions).

Relationships to the other authorities:

- [`layout-model.md`](layout-model.md) owns **where seats are staged**. This document
  owns **where the rack sits inside a staged seat**. The zones baseline is a zone-anatomy
  sheet drawn at four corner clusters; it is *not* a restaging of the plane's fixed slots
  and does not override the receiver-band / far-side / wings model.
- [`visual-system.md`](visual-system.md) owns look and motion grammar. This document
  refines its zone rows and corrects two of them (see §Conflicts).
- [`presentation-budgets.md`](presentation-budgets.md) caps everything here.
- [`protocol.md`](../protocol.md) is the only source of zone data. Nothing below derives
  legality, cost, or effect.

## 0. Invariants

| # | Invariant |
| --- | --- |
| I1 | Zero game logic. Every offered interaction comes from `valid_actions`, `requirements`, or `prompts`. The rack never computes whether a zone is usable. |
| I2 | Hidden stays hidden. The library's contents and top-card identity are never rendered unless the server explicitly sends them. Activating a library **never** browses. |
| I3 | Public zones (graveyard, exile, command) are fully browsable — `GameView` already ships their ordered contents. |
| I4 | One `GameView` rebuilds the whole rack. No rack state survives a message. Open browser, reveal shelf, and hover are ephemeral, dropped and re-derived exactly like selection and manual focus. |
| I5 | Every count has exactly **one** visual home: its pile. See [§4](#4-count-ownership). |
| I6 | Exactly four zone anchors exist per seat. No generic bucket. See [§11](#11-open-mechanics-readiness). |
| I7 | Every rack surface is ≥ 44 CSS px in every input mode and carries an accessible name plus its count. |

## 1. What the baseline actually shows

Read off `rune-zones-interaction.jpg` (1672 × 941). Four seats, each with a circular
gold-rimmed identity medallion carrying the life numeral (bottom-left disc) and a
color-identity gem (bottom-right). Beside each medallion: a three-pile column and one
separate command slot.

| Seat | Medallion centre | Library | Graveyard | Exile | Command | Command side |
| --- | --- | --- | --- | --- | --- | --- |
| Top-left (life 25) | (110, 124) | (253, 122) | (239, 252) | (221, 372) | (367, 200) | inboard, right of rack |
| Top-right (life 26) | (1390, 150) | (1520, 120) | (1540, 245) | (1545, 350) | (1250, 180) | inboard, left of medallion |
| Bottom-left (life 24) | (190, 600) | (320, 565) | (310, 685) | (285, 815) | (425, 745) | inboard, right of rack |
| Bottom-right (life 23) | (1385, 620) | (1520, 570) | (1520, 700) | (1540, 820) | (1270, 780) | inboard, left of medallion |

Facts the sheet establishes, all four seats agreeing:

1. **Order is fixed and never reverses**: library, graveyard, exile — top to bottom, in
   that order, at every seat. The sheet's `1.` / `2.` / `3.` / `4.` prefixes are
   annotation, not shipped chrome.
2. **Nothing is rotated.** Every pile card, title plate, and badge reads screen-upright
   at every seat. There is no per-seat 90°/180° rotation.
3. **The rack strip sits on the screen-right of the medallion** at all four seats. The
   art is unmirrored; only the command slot changes side.
4. **The command slot is always inboard** — on the side of the cluster facing the table
   centre, and always the innermost element of the cluster.
5. **The command slot is the largest element** — ≈ 1.35× the pile card's width — and is
   the only one with two badges (crown + numeral).
6. **Counts appear on graveyard and command only** in the drawn state. The library and
   exile piles carry no numeral in this sheet; the drawn exile piles hold one card each.
7. **The medallion carries no count pip** in this sheet. The gold hex count pip appears
   only in `rune-player-control-ui.jpg`, exactly one per seat.

`rune-battlefield-environments.jpg` panel 3 (4 players, canonical) adds the orientation
rule the zones sheet cannot show: the top-centre seat's piles run in a **horizontal row**
along its outer edge, and the bottom-centre local seat's piles likewise sit in a
horizontal arrangement beside its medallion. Flank seats keep the vertical column. The
rack therefore **runs along its region's outer edge**, not along a fixed screen axis.

## 2. Rack geometry

### 2.1 Frame and units

- `u` — the rack's **pile card width** at its tier. A pile card is `u × 1.4u` (portrait,
  aspect 0.71, the card silhouette of `rune-card-system-overview.jpg`).
- **along** — the rack's reading axis. Screen top→bottom for a vertical rack, screen
  left→right for a horizontal rack. **Never mirrored, never rotated** (baseline fact 2).
- **rack side** — the perpendicular offset direction from the medallion to the strip:
  screen-right for a vertical rack, screen-down for a horizontal rack (baseline fact 3).
- **inboard** — from the seat's region toward the plane centre (`PlaneSlotFrames.corridor`).

### 2.2 Anchor table (normative)

Offsets from the identity medallion's centre, in `u`:

| Slot | along | perpendicular | Drawn size |
| --- | --- | --- | --- |
| Medallion | 0 | 0 | `1.45u` diameter |
| 1 Library | 0 | `+1.85u` (rack side) | `u × 1.4u` |
| 2 Graveyard | `+1.60u` | `+1.85u` | `u × 1.4u` |
| 3 Exile | `+3.20u` | `+1.85u` | `u × 1.4u` |
| 4 Command | `+1.00u` (± 1.0u tolerance) | rack side inboard ⇒ `+3.35u`; rack side outboard ⇒ `−2.00u` | `1.35u × 1.89u` |

The command rule reads: the command slot always lands on the **inboard flank of the whole
cluster**. When the strip is already inboard of the medallion it clears the strip
(`+3.35u`); when the medallion is inboard of the strip it crosses to the medallion's far
side (`−2.00u`). This reproduces all four baseline seats within tolerance and generalizes
to horizontal racks.

**Decorative lean [D].** The baseline drifts each pile ≈ `0.2u` outboard as the column
descends. Ship it as a *drawn-rect only* lean of ≤ `0.2u` per step. **Hit rects use the
straight column** — a leaning hit rect fails the packing rule below.

### 2.3 Pitch, packing, and the 44 px floor

- Strip pitch is uniform at `1.60u`, gap between slot 3 and slot 4 is `0.60u`.
- Every slot's hit rect is `hitRectFor(rect)` (grown centred to 44 px, drawn footprint
  unchanged — `plane/metrics.ts`).
- **Hit rects in one rack must never overlap [D].** With a 44 px floor this requires the
  along-axis pitch to resolve to ≥ 48 px, i.e. `u ≥ 30 px`. Below that the rack **cannot**
  draw three separable targets and must fall to the digest variant (§6). This is the
  single numeric trigger for digesting a rack; it is independent of the layout model's
  wing digest threshold, and either one firing digests the rack.

### 2.4 Corridor clearance (normative)

The rack lives on the seat's **outer flank**, never in the central interaction corridor.

1. The rack is anchored to its region's outer edge and grows **outward-then-along**; it
   may bleed past the plane edge exactly like a wing slot, and never overlaps another
   region (ADR 0023's by-construction rule).
2. **Clearance rule:** the union of the rack's four hit rects, expanded by a 12 px halo,
   must not intersect `PlaneSlotFrames.corridor`. A rack that cannot satisfy this inside
   its region digests (§6) — it never trims a zone away.
3. The command slot, being the innermost element, is the one this test binds on. It is
   also the one the corridor's targeting paths most often terminate near, so the halo is
   not optional.
4. Racks belonging to two seats never share a clearance zone: each rack stays within its
   own region's slot bounds along the perpendicular axis.

### 2.5 Orientation per region kind

| Region kind (layout-model) | Outer edge | Rack axis | Rack side | Inboard |
| --- | --- | --- | --- | --- |
| Receiver band (bottom, full width) | left flank of the band | vertical, top→bottom | screen-right | right / up |
| Far side (top centre, 3+ players) | top edge of the plane | horizontal, left→right | screen-down | down |
| Far side (duel, full-width top) | top edge of the plane | horizontal, left→right | screen-down | down |
| Wing, left side | left plane edge | vertical, top→bottom | screen-right | right |
| Wing, right side | right plane edge | vertical, top→bottom | screen-right | left |
| Compact summary tile (rung 5) | — | digest rack only (§6) | — | — |

The receiver's crest already stages at `rect.x − crest.w − 64` (`plane/stage.ts`); the
rack strip lands `1.85u` to its right, which is the baseline's bottom-left seat exactly.

## 3. Pile representations

Each pile must be **mechanically distinct at a glance and physically plausible**. No two
piles may read as the same object at any tier. The non-color channel is the pile's
*silhouette and material*, per `presentation-budgets.md` §Accessibility.

| | Library | Graveyard | Exile | Command |
| --- | --- | --- | --- | --- |
| Face | face-down Rune card back | face-up top card | face-up top card | commander face / portrait |
| Material | opaque card stock | opaque card stock, ash-dimmed | **translucent cyan-violet glass slabs** around the card | opaque, heavy gold frame |
| Frame | gold double rule on navy | standard gold card frame | standard gold card frame **inside** a glass pane `0.12u` larger on every side | thicker, brighter gold rule at `1.35u` scale |
| Depth | 1–4 edge slivers stepping down-right | 1–2 offset card layers, each rotated ±2–3° (untidy) | 1–3 glass panes stepping down-right, cyan hairline stroke each | none — a single slot |
| Glow | none | none | outer cyan bloom, `#7FB2E5`→violet, ≤ 8 px | seat-accent rim (`SCENE_SEAT_ACCENTS`) |
| Badges | count | count | count | crown (bottom-left) + tax numeral (bottom-right) |
| Glyph | spiral rune, part of the back | grave rune, etched, empty state only | exile iris rune, etched, empty state only | crown, always |
| Activation | count + server actions only | opens zone browser | opens zone browser | commander popover + server actions |

### 3.1 Library

- The face is the **Rune card back** as drawn in `rune-card-system-overview.jpg`: dark
  navy/slate field, fine gold inner rule with notched corners, a centred gold diamond
  outline enclosing the gold spiral rune. Symmetric and non-orienting — a rotated back
  leaks nothing (issue #548, request 3a).
- **Asset:** the production raster is requested in **#548**. Implementation ships a
  layered **SVG placeholder** behind the same manifest key the production plate will use;
  the key and the manifest contract belong to **#538**. This document binds only: one key,
  one silhouette at every scale (hand card, pile edge, travel ghost, folded pile).
- **Depth ladder** [D], from `library` count:

  | Count | Edge slivers below the top card |
  | --- | --- |
  | 0 | none — empty state (§5) |
  | 1 | 0 |
  | 2–5 | 1 |
  | 6–15 | 2 |
  | 16–39 | 3 |
  | 40+ | 4 |

  Sliver step is `0.02u` down and `0.02u` right, capped at 4. **The library never looks
  empty while count > 0** — at count 1 the full top card still draws.
- **Count badge.** The baseline draws no numeral on the library. Ship one anyway [D],
  in the graveyard's badge style, because I5 makes the pile the *only* home for the
  library count and a deck whose depth ladder saturates at 40 cannot distinguish 45 from
  99. Badge sits at the pile's bottom-outboard corner so it never collides with the
  next slot down.
- **No top-card identity, ever** (I2). The `faceUp` slot exists (`ZonePile.tsx`) and stays
  reserved for an explicit server reveal, which no wire message provides today (§12).

### 3.2 Graveyard

- Face-up top card at pile scale, showing the card's own frame, name band, and art
  window at the pile tier's information budget. Art is dimmed and desaturated toward the
  ash/violet cast of the baseline; the frame accent stays true (frame colour is game
  information and is never restyled).
- Untidy stack: 1–2 layers behind, each offset `0.015u` and rotated ±2–3° [D] (the
  baseline shows irregular offsets; a deterministic per-seat jitter seeded from the seat
  id keeps it stable across renders, since nothing may be load-bearing across messages).
- Count badge: circular, ink fill `#0D0F13`, gold rim, ivory numeral, anchored at the top
  card's bottom-right, overhanging the frame by ~40% of the badge diameter. This is the
  badge form for every pile.
- Activating opens the zone browser (§8) in wire order, top of the pile last.

### 3.3 Exile — must not read as a second graveyard

This is the treatment issue #540 most warns about, and the baseline is explicit. Transcribe
it exactly:

- The top card sits **inside a translucent pale-cyan glass pane** that is `0.12u` larger
  than the card on every side. The pane is a distinct object with its own bright cyan
  hairline stroke (`#7FB2E5` family) and an outer bloom of the same hue trending violet.
- 1–3 further glass panes step down-and-right behind it, each with its own hairline. They
  read as **stacked slabs, not cards** — no gold card frame on a pane, no name band, no
  art. This is what separates exile from the graveyard's card-on-card stack at a glance.
- The card itself carries a cool blue cast; the graveyard's is warm-dark ash. The two
  piles differ in **material, silhouette, and hue family simultaneously**, so neither
  channel alone is load-bearing.
- Count badge in the standard form, on the top card (not on a pane).
- Activating opens the zone browser.

Diagnostic for review: at 60% brightness with hue stripped, a graveyard pile and an exile
pile must still be distinguishable by outline alone — the exile's pane is larger than its
card, the graveyard's is not.

### 3.4 Command zone

- A dedicated slot at `1.35u` width showing the **commander's face** (portrait/art window
  and name band) inside a heavier, brighter gold rule than a normal card frame, with the
  seat's accent (`SCENE_SEAT_ACCENTS`) as a rim — the one place a seat accent touches a
  card-shaped object, justified because the command slot is a *seat fixture*, not a card
  on the plane. The card's own frame accent is not displaced.
- **Crown marker**: gold shield/pentagon plate carrying the crown glyph, straddling the
  bottom-left frame corner. Always present when the slot is present.
- **Tax numeral**: standard circular badge at the bottom-right.
  - **Value shown = `commander_tax[].tax`, rendered `+N`** [D], and the badge is
    suppressed at `tax == 0`. `tax` is what changes the next cast's cost, so it is the
    decision-relevant number.
  - The accessible name carries both: `"… commander, cast 2 times, tax +4"`.
  - The client **never derives** this value. It renders `commander_tax[].tax` as sent
    and animates from the previous server value to the new one; it does not compute a
    tax from `casts`, from a cast count it observed, or from any rules increment.
  - *Baseline note:* the sheet draws bare `3`, `4`, `2`, `1`. Values 3 and 1 are
    impossible for `tax` under CR 903.8 (`tax = 2 × casts`), so the drawn numeral is
    either `casts` or arbitrary art. This reasoning identifies which field the baseline
    drew; it is not a formula the client may evaluate. The badge's **form** is
    transcribed; its **semantics** are decided here.
- Absent entirely in non-Commander formats (§5), never an empty unexplained box.
- Commander actions originate from this slot: it is the source anchor for a commander
  cast and the destination anchor for a commander return.

### 3.5 Pile states

Every pile renders one of these, all reconstructible from one `GameView`:

| State | Library | Graveyard / Exile | Command |
| --- | --- | --- | --- |
| Empty | etched back outline, no slivers, no badge | etched card outline + zone glyph | etched crown outline (Commander formats only) |
| One card | full top face, 0 slivers, badge `1` | top card, no layers, badge `1` | commander face, crown, tax badge if > 0 |
| Deep pile | top face + slivers per ladder | top card + 1–2 layers / 1–3 panes | n/a (command holds ≤ 1 today) |
| Revealed top | **not available** (§12) | already the normal state | already the normal state |
| Actionable | gold bottom **edge bar** (visual-system §7) | gold bottom edge bar | gold bottom edge bar |
| Selected | blue ring + elevation 2 | blue ring + elevation 2 | blue ring + elevation 2 |
| Prompt candidate | orange ring + beacon | orange ring + beacon | orange ring + beacon |
| Prompt-source | pile pulses once and the browser opens in selection mode (§8) | same | same |

## 4. Count ownership

**Rule (I5): a zone's count lives on that zone's pile and nowhere else.**

| Datum | Owner | Source |
| --- | --- | --- |
| Library count | **the library pile** | `me.library_size` / `OpponentView.library_size` |
| Graveyard count | the graveyard pile | `graveyards[].cards.length` |
| Exile count | the exile pile | `exile[].cards.length` |
| Command count | the command slot (implicit — the slot is occupied or etched) | `command[].cards.length` |
| Commander tax | the command slot | `commander_tax[].tax` |
| **Hand count** | **the identity cluster's hex pip** | `my_hand.length` / `OpponentView.hand_size` |
| Life | the identity medallion | `me.life` / `OpponentView.life` |
| Commander damage | the identity cluster (expanded state) | `commander_damage[]` |

### 4.1 Reconciling with the seat-identity spec — the library count

`visual-system.md` §6 currently gives the crest cluster "hand and library counts as
compact pips, and — in commander games — the commander badge with its tax counter." Three
of those four data now belong to the rack. The resolution:

> **The library pile owns the library count. The identity cluster owns the hand count,
> and only the hand count.**

Why the library and not the pip:

1. **The hand has no pile; the library does.** An opponent's hand renders as a face-down
   fan with no count-bearing surface, and the receiver's own hand is a screen-space fan
   *below* the plane entirely. The hand count has no physical home, so it needs the
   identity cluster. The library has a physical home in every variant, down to the digest
   sub-indicator.
2. **The baselines already agree.** `rune-player-control-ui.jpg` draws exactly **one**
   numeral pip per seat beside the medallion at all four states — peripheral, focused,
   local normal, local expanded. There is no second pip to be the library. And
   `rune-zones-interaction.jpg`, where the racks are drawn, gives the medallion no pip at
   all.
3. **Draw motion needs an unambiguous origin.** A draw travels library → hand. If the
   library count sat next to the medallion, the number that ticks down and the object the
   card leaves would be different surfaces. One home makes causality readable.

Consequences, none of which are edits this document makes:

- `visual-system.md` §6 must become "**hand count** as a compact pip" and must drop the
  crest-borne commander badge and tax counter — those are the command slot's. Owner:
  #532 / #539. Recorded in §Conflicts.
- `SummaryTileSlot` already carries both `zones` and `handCount` (`plane/types.ts`) — that
  is correct and needs no change: at rung 5 the tile's four sub-indicators **are** the
  digest rack, not a duplicate of one.
- `PlaneRegion.handCount` stays; `PlaneRegion.zones` stays; neither is rendered twice.

## 5. Empty states

| Case | Treatment |
| --- | --- |
| Public zone empty (graveyard, exile) | A **quiet etched outline** of the card silhouette plus the zone glyph, both at ~6% ink opacity, rising to ~35% on hover / keyboard focus. No count badge. No permanent text label. |
| Library at 0 | The etched back outline. A library at 0 is genuinely empty and must look it — the seat is one draw from losing. |
| Library at ≥ 1 | Never the empty treatment. At minimum the full top card back draws (§3.1). |
| Command slot, Commander format, commander elsewhere | Etched crown outline. The anchor is preserved so a commander return terminates somewhere visible. |
| Command slot, non-Commander format | **Absent.** The slot is not drawn, not reserved, not spaced for. The rack is three slots wide and the clearance test runs against three. |
| Eliminated seat | The whole rack desaturates with the seat's eliminated treatment. Public zones stay browsable (`layout-model.md`, `ui-requirements.md`). |

**Anchor persistence.** An empty rack still publishes its four (or three) anchors so
incoming zone travel has a real destination rect at 0 ms — an animation is never allowed
to invent a target. See §7 and §9.

**Determining "Commander format" is a protocol gap today** — see §12, gap G3. Until it is
closed the client's only honest test is `command`, `commander_tax`, or `commander_damage`
being non-empty, which fails at the start of a Commander game in which every commander is
already on the battlefield. Ship the fallback (no slot) and close the gap.

## 6. Rack variants

Four variants. All four keep the same slot order and the same four anchors.

| Variant | Used by | Tier | Anatomy |
| --- | --- | --- | --- |
| **Local** | receiver band | largest, `u` at field tier | full: face, depth layers, name band on public tops, count badges, crown + tax |
| **Focused** | far side | one tier down | full anatomy, `u` one rung smaller |
| **Wing** | wings at rungs 0–3 | `u` at mini tier | **tightened but distinct**: same four slots, same order, same materials. Name band on public tops suppressed (glyph + count carry identity); depth layers capped at 2 (library) / 1 (graveyard) / 2 (exile); command keeps crown + tax |
| **Digest** | wings at rung 4, summary tiles at rung 5, or any rack failing §2.3 / §2.4 | — | **one rack button** carrying four shaped sub-indicators |

### 6.1 The digest rack

One ≥ 44 px button. Four sub-indicators, each a distinct **shape** so the channel is not
colour (budgets §Accessibility):

| Zone | Sub-indicator shape | Carries |
| --- | --- | --- |
| Library | rounded rectangle (card-back silhouette) | count |
| Graveyard | arched tombstone silhouette | count |
| Exile | diamond / rune-iris outline | count |
| Command | crown pentagon | occupied dot + tax `+N` |

A zone absent from the format (command) contributes no sub-indicator. A zone at 0 shows
its shape as a dashed outline with no numeral — the same etched language as §5.

### 6.2 What expands what

| Action | Result | Does it change focus? |
| --- | --- | --- |
| Activate a **digest rack button** | Expands to the **wing** variant in place, as a popover anchored to the button, inside the region's bounds | **No** |
| Activate a seat's **crest / summary tile** | Re-stages that seat to the far side (`layout-model.md` §Focus model) | Yes |
| Focus promotion (manual or automatic) | Wing rack → focused rack | — |
| Activate **graveyard** or **exile** | Opens the zone browser (§8) | No |
| Activate **library** | Offers the library's server actions if `valid_actions` name it; otherwise inert | No — and **never browses** |
| Activate **command slot** | Opens the commander popover (commander card, `casts`, `tax`) and offers command-zone actions when `valid_actions` name the commander entity | No |
| A prompt names a public zone | The pile enters the prompt's selection state and the browser opens in selection mode (§8) | No |

Expansion is presentation state, dropped on the next view (I4).

## 7. Anchors

Zone travel and effect anchors resolve through the plane's anchor map
(`live/LivePlane.tsx`, `refreshVisualAnchors`). Today the map publishes one `pile:<seat>`
rect for the entire pile cluster. That is insufficient: a motion that must terminate at
the *actual* pile cannot resolve "the pile cluster".

**Normative anchor keys [D]:**

| Key | Rect |
| --- | --- |
| `zone:<seat>:library` | the library slot's hit rect |
| `zone:<seat>:graveyard` | the graveyard slot's hit rect |
| `zone:<seat>:exile` | the exile slot's hit rect |
| `zone:<seat>:command` | the command slot's hit rect (absent when the slot is absent) |
| `zone:<seat>:rack` | the union of the above (digest variant resolves every zone key to the rack button's rect) |
| `pile:<seat>` | retained as an alias of `zone:<seat>:rack` for compatibility |

Resolution order for any anchor reference: exact zone key → `zone:<seat>:rack` →
`seat:<seat>` (the crest, which is staged at every rung and can never degrade away). The
fallback chain guarantees a motion is retargeted rather than retired, matching the
existing off-focus/combat rule in `LivePlane.tsx`.

Anchors exist at their final rects **the moment the scene is built** (budgets: input is
never gated on animation), including for empty zones.

## 8. Interaction contract

### 8.1 Targets and naming

| Requirement | Rule |
| --- | --- |
| Hit size | Every slot ≥ 44 CSS px in both axes, via `hitRectFor`. Drawn footprint never grows to meet it. |
| Packing | Hit rects within one rack never overlap (§2.3). |
| Accessible name — library | `"{player} library, {n} cards"`. `role="img"` when it carries no server action; a `button` named by the action's `label` when it does. |
| Accessible name — graveyard / exile | `"Browse {player} graveyard, {n} cards"` on a `button`. |
| Accessible name — command | `"{player} command zone — {commander name}, cast {casts} times, tax +{tax}"`. |
| Accessible name — empty | `"{player} exile, empty"`. |
| Accessible name — digest rack | `"{player} zones: library {a}, graveyard {b}, exile {c}, command {d}"` on the button; sub-indicators are `aria-hidden` presentation. |
| Keyboard | The rack is **one tab stop** with roving `tabindex` [D]; arrow keys move between slots, Enter/Space activates. Six seats therefore cost six tab stops, not twenty-four. |
| Live regions | Count changes are **not** announced per change (a mill would flood). The log already narrates; the pile's name is re-read on focus. |

### 8.2 Zone browser

`ZoneBrowser.tsx` is today a full-screen modal with a backdrop. Two normative changes:

1. **Anchored, not full-screen** [D]. The browser opens as a panel anchored to the rack's
   inboard side, sized to leave the owning rack and the seat's region visible. The player
   must never lose spatial context — the point of a physical rack is defeated if opening
   it hides where it was.
2. **Selection mode.** When a `requirement` or a `select_from_zone` prompt names cards in
   the opened zone, the browser renders those entries as **candidates** (orange ring +
   beacon, the shipped target language) and every other entry as inert. Confirming
   submits the ids as the slot's answer; it does not send a separate message. Candidate
   membership comes only from `candidates[]` — the browser never filters by rule.
3. Order is wire order, top of the pile last, unchanged. Each entry opens the shared
   inspect popover, unchanged.
4. **Windowing** [D]: above 120 entries the list virtualizes, so a 400-card exile does not
   spend the scene's DOM budget.
5. At most one browser is open. It is ephemeral (I4): a new `GameView` re-derives it from
   the pending prompt, or closes it.

### 8.3 Drag and drop

- A pile is a **drop target only when a server action or prompt names that zone.** No
  client-side inference, ever.
- **Today no `valid_action` names a destination zone** (`ValidAction` has `subject`,
  never a zone) and `select_from_zone` is only ever emitted with `zone: "hand"`. So
  today **no pile is a drop target**, and every drag onto one rejects. This is correct
  behaviour, not a missing feature — see §12 gap G4.
- Rejection: snap-back along the drag path plus the shipped illegal treatment (≤ 3 px
  horizontal shake, 2 cycles, non-blaming toast). Never a red X that blames the player.
- A valid drop target wears the gold bottom edge bar like any other offered interaction.

### 8.4 Reconnect

The rack is a pure function of one `GameView`:

```
rack(seat) = f(library_size, graveyards[seat], exile[seat], command[seat],
               commander_tax[seat], valid_actions, prompt, region geometry)
```

Order, top card, counts, tax, actionability, and every anchor rebuild from that alone.
Nothing is accumulated. A reconnect mid-animation lands on the true state and plays no
catch-up motion beyond the shipped "you are here" pulse. Rebuild cost is counted against
the ≤ 50 ms desktop / ≤ 100 ms mobile scene-rebuild budget.

## 9. Motion map

Every row **terminates at the actual visible pile** — the anchor rect from §7, at the
tier the pile is actually drawn at, including the digest rack button. A motion whose
endpoint cannot be resolved falls back through the §7 chain to the seat crest; it is never
allowed to fade into nothing. Durations are the zone-travel class (250–400 ms) unless
stated. Reduced motion snaps every row to its end state with zero layout difference.

| Event | From anchor | To anchor | Choreography | RM form |
| --- | --- | --- | --- | --- |
| **Draw** | `zone:s:library` | `hand:s` fan slot | card rises off the pile face, arcs to its fan slot, neighbours reflow; slivers settle down one rung | card appears in slot, badge ticks |
| **Mill** | `zone:s:library` | `zone:s:graveyard` | per card, staggered inside the batch window; each lands face-up on the graveyard and becomes the new top | counts tick, top updates |
| **Discard** | `hand:s` (or the fan slot) | `zone:s:graveyard` | card tips flat, slides to the pile, pile settles ±2° | count ticks, top updates |
| **Destroy** | permanent's rect | `zone:s:graveyard` | ≤ 150 ms crack flash, then the graveyard travel | count ticks |
| **Sacrifice** | permanent's rect | `zone:s:graveyard` | brief down-press (no crack flash — sacrifice is a cost, not a loss moment), then the travel | count ticks |
| **Exile** | source rect | `zone:s:exile` | the violet/cyan rune iris **opens at the exile pile**, the card travels into it and the topmost glass pane flashes as it closes | count ticks, top updates |
| **Return** (gy/exile → hand, battlefield, library) | `zone:s:graveyard` / `zone:s:exile` | destination | the top card lifts clear of the pile, the pile below re-settles to its new top, then the normal travel for the destination | new top appears, counts tick |
| **Shuffle** | `zone:s:library` | itself | in-place riffle: slivers fan ≤ `0.15u` and re-square, ≤ 300 ms; no card leaves the pile | one settle frame |
| **Commander return** | battlefield / graveyard / exile | `zone:s:command` | card travels to the command slot; the crown marker strikes once as it lands | commander face appears in slot |
| **Commander cast** | `zone:s:command` | stack rail slot | the standard cast motion originating **at the command slot**; the tax badge ticks from its previous value to the new authoritative `commander_tax` after the card leaves | entry appears, tax badge updates |
| **Reveal / search staging** | `zone:s:library` | reveal shelf (§10) | cards rise from the library and stage above it before entering the browser or prompt | cards appear on the shelf |

Two corrections to `visual-system.md` §8, recorded in §Conflicts:

- **Exile** is specified there as "card lifts and fades through a violet rune iris",
  RM form "vanishes + pile tick". A fade-and-vanish does not terminate at the pile.
  The row above replaces it.
- **Mulligan** already routes hand → library as one aggregate intent; that is correct and
  should terminate at `zone:s:library`, which the anchor keys now make possible.

## 10. Zone travel staging — the reveal shelf

Cards revealed from a library or produced by a search may **stage above the library rack**
before entering the browser or a prompt.

- The shelf is a screen-space strip anchored to `zone:<seat>:library`, on the outboard
  side, offset `0.3u` along the negative rack axis. It never enters the corridor and never
  overlaps another region.
- Capacity **5 cards** [D]; beyond that the shelf collapses to the first four plus a
  `+N` chip and the browser opens immediately.
- The shelf is strictly a **staging surface**: it holds cards the server has already
  revealed. It is not a zone, has no count badge, and publishes no anchor of its own —
  travel from the shelf resolves to `zone:<seat>:library`.
- It is ephemeral (I4). A new `GameView` that no longer carries the reveal drops it.
- **Not implementable today** — no wire message reveals library cards (§12, gaps G1/G2).
  The geometry is reserved so the feature is a data change, not a layout change.

## 11. Open-mechanics readiness

- Exactly **four** anchors are reserved per seat: library, graveyard, exile, command. No
  fifth slot, no generic unlabeled bucket, no "other zones" tray.
- A card that moves to a zone the client has no anchor for is a **bug in the protocol
  contract, not a rendering fallback**: the motion resolves to the seat crest and the log
  narrates it, and the gap is filed.
- Introducing a new zone (a sideboard, a companion area, an "outside the game" surface)
  requires, in one PR: an explicit design decision extending this document, a
  `rune-protocol` shape, the TypeScript mirror, `docs/protocol.md`, and a rack variant that
  still passes §2.3 and §2.4. The rack's slot capacity is a design constraint, deliberately.
- The digest rack's four shapes are likewise a closed set. A fifth shape would defeat the
  glance-legibility the shapes exist for.

## 12. Protocol dependencies and gaps

Everything the rack draws today comes from `GameView`:

| Rack datum | Wire source | Status |
| --- | --- | --- |
| Library count | `me.library_size`, `OpponentView.library_size` | available |
| Graveyard contents + count + top | `graveyards[].cards` (top last) | available |
| Exile contents + count + top | `exile[].cards` (top last) | available |
| Command contents | `command[].cards` (omitted when empty) | available, with G3 |
| Commander tax / casts | `commander_tax[]` (`casts`/`tax` omitted when 0) | available |
| Actionability | `valid_actions[].subject` | available |
| Prompt candidacy | `requirements[].candidates`, `prompts[select_from_zone].candidates` | available, with G1 |

Gaps, each blocking a specific section above:

| # | Gap | Blocks | Notes | Tracked by |
| --- | --- | --- | --- | --- |
| **G1** | `select_from_zone` is only ever emitted with `zone: "hand"` (`view/actions.rs`, `view/requirements.rs`). No prompt names library, graveyard, exile, or command. | §8.2 selection mode for public zones | The client-side contract is zone-agnostic already; the server never exercises it. | #552 |
| **G2** | No library reveal channel. `GameView` carries no `revealed` field and no library card list, so a candidate id from a library prompt resolves to **no `CardView`** — the client cannot render it. | §3.1 revealed top, §10 reveal shelf | Needs a protocol addition (`revealed: ZonePile[]`, or candidate `CardView`s carried on the prompt). Contract change: Rust + TS mirror + `protocol.md`. | #552 |
| **G3** | No format signal in `GameView`. `command`, `commander_tax`, and `commander_damage` are all "omitted when empty", so a Commander game in which every commander is on the battlefield and no tax or damage exists is indistinguishable from a non-Commander game. `CatalogFormat.commander` exists only in the **lobby** catalog. | §5 command-slot presence | The rack must choose between "absent" and "etched crown" with no authoritative answer. Ships as "absent". | #553 |
| **G4** | No action names a destination zone. `ValidAction` carries `subject` (entities) and no zone field. | §8.3 drag/drop onto a pile | Until closed, no pile is a drop target — which is the correct fail-closed behaviour, not a stub. | #552 |
| **G5** | `OpponentView` has `graveyard_size` but no `exile_size`. | nothing — exile contents are public and complete, so the count is `cards.length` | Recorded for completeness; redundant field, not a gap to close. | n/a — nothing to close |

None of these are this document's to fix. G1, G2, and G4 are tracked by **#552**
(action destinations and zone prompts); G3 by **#553** (game and seat state).
Each closes through a protocol PR that updates `rune-protocol`, the TypeScript
mirror, and `docs/protocol.md` together.

## 13. Stress proof — DOM node budget

`presentation-budgets.md` caps the scene at **≤ 15 000 nodes total** and **≤ 12 nodes per
card face at battlefield tiers**. Element counts for the rack anatomy above (elements, not
text nodes):

| Slot | Local / focused | Nodes | Wing | Nodes |
| --- | --- | --- | --- | --- |
| Library | root, frame, 4 slivers, back, badge | 8 | root, frame, 2 slivers, back, badge | 6 |
| Graveyard | root, frame, 2 layers, top card, name band, art window, badge | 8 | root, frame, 1 layer, top card, art window, badge | 6 |
| Exile | root, frame, 3 panes, top card, name band, art window, badge | 9 | root, frame, 2 panes, top card, art window, badge | 7 |
| Command | root, frame, art window, name band, crown, tax badge | 6 | root, frame, art window, crown, tax badge | 5 |
| Rack container | 1 | 1 | 1 | 1 |
| **Rack total** | | **32** | | **25** |

Digest rack: button + 4 sub-indicators = **5 nodes**.

Every per-slot count is **≤ 12**, so no pile ever breaches the per-card-face ceiling. Exile
at 9 is the worst case and the one to watch if the treatment gains detail.

Multiplied out:

| Scenario | Composition | Rack nodes |
| --- | --- | --- |
| 2 players (duel) | 1 local + 1 focused | 32 × 2 = **64** |
| 3 players | local + focused + 1 wing | 32 + 32 + 25 = **89** |
| 4 players (primary) | local + focused + 2 wings | 32 + 32 + 50 = **114** |
| 5 players | local + focused + 3 digest wings | 32 + 32 + 15 = **79** |
| 6 players | local + focused + 4 digest wings | 32 + 32 + 20 = **84** |
| **6 seats, all racks fully drawn** (hypothetical — staging never does this) | 6 × 32 | **192** |
| 6 seats, phone compact (rung 5) | local 32 + focused 32 + 4 tiles × 5 | **84** |

**Worst case is 192 nodes — 1.28% of the 15 000-node scene budget.** Every populated-zone
stress case in `presentation-budgets.md` §Performance (four-player Commander at ~120
permanents; 240-permanent degenerate board; six visible players) leaves the rack budget
untouched, because rack cost is a function of seat count only, never of zone depth: a
99-card library and a 40-card graveyard cost exactly the same nodes as a 1-card one.

The one variable cost is an open zone browser: 3 nodes per row, so a 100-card graveyard
costs ~300 transient nodes. §8.2's windowing at 120 rows caps that at ~360 nodes for any
zone depth, and at most one browser is open.

*Not verified here.* These are counted from the specified anatomy, not measured in a
browser. Real-hardware DOM measurement is outstanding and belongs to the maintainer
(`presentation-budgets.md` §Real-hardware validation).

## 14. Decisions this document makes

The baselines did not dictate these; each is decided here and is open to revision.

- **[D1] Rack mirroring.** The rack strip sits screen-right of the medallion at every
  vertical rack (transcribed from the baseline, which is unmirrored) and screen-below at
  every horizontal rack. Only the command slot changes side, always landing inboard.
- **[D2] Rack orientation follows the region's outer edge** — vertical for flank seats and
  the receiver band, horizontal for the top-centre far side. Supported by the environments
  sheet, not stated by the zones sheet.
- **[D3] Decorative lean is drawn-only.** Hit rects use a straight column.
- **[D4] `u ≥ 30 px` (48 px pitch) is the digest trigger** for a rack that cannot keep
  three non-overlapping 44 px targets.
- **[D5] Corridor clearance** = hit-rect union + 12 px halo must not intersect the corridor.
- **[D6] The library carries a count badge**, though the baseline draws none — required by
  the one-home rule since the depth ladder saturates at 40.
- **[D7] Library depth ladder** thresholds (0 / 1 / 2–5 / 6–15 / 16–39 / 40+ → 0–4 slivers).
- **[D8] Graveyard untidiness is seeded from the seat id**, so it is deterministic and
  never load-bearing across messages.
- **[D9] The command badge shows `tax` as `+N`**, suppressed at 0, with `casts` in the
  accessible name. The baseline's bare numerals are ambiguous and cannot be `tax`.
- **[D10] The command slot wears the seat accent** as a rim — the one seat-accent-on-a-card
  exception, justified because the slot is a seat fixture.
- **[D11] Per-zone anchor keys** `zone:<seat>:<zone>`, with `pile:<seat>` retained as an
  alias for the rack union.
- **[D12] The zone browser is anchored, not full-screen**, so spatial context survives a
  prompt.
- **[D13] Browser windowing above 120 entries.**
- **[D14] The rack is one tab stop with roving `tabindex`.**
- **[D15] Digest sub-indicator shapes**: rounded rect / tombstone arch / rune-iris diamond
  / crown pentagon.
- **[D16] Digest expansion is in place and does not change focus.**
- **[D17] Reveal-shelf capacity of 5**, collapsing to 4 + `+N`.
- **[D18] Sacrifice drops the crack flash** that destruction uses, so cost and loss read
  differently.

## Conflicts and open questions

Recorded, not edited. Each needs its owning issue to resolve.

| # | Conflict | Resolution proposed here | Owner |
| --- | --- | --- | --- |
| C1 | `visual-system.md` §6 gives the crest cluster "hand **and library** counts as compact pips". | Drop `library`. The pile owns it (§4.1). | #532 / #539 |
| C2 | `visual-system.md` §6 gives the crest cluster "the commander badge with its tax counter". | Drop both. The command slot owns them (§3.4). | #532 / #539 |
| C3 | `visual-system.md` §8 Exile row: "card lifts and fades through a violet rune iris", RM "vanishes + pile tick". | Replace with the §9 row — the iris opens **at the exile pile** and the card lands there. | #535 |
| C4 | Issue #540 says "the rack rotates/orients with the seat, so its order reads consistently from the owning player's perspective". | The baseline draws **no rotation at any seat**; every face reads screen-upright. Orientation follows the region's outer edge (D2), order never reverses. Baseline wins. | this doc |
| C5 | Issue #540 specifies "2–3 physical edge layers" on the library. | The baseline draws 4–5. Ship the count-driven ladder D7, capped at 4 slivers. | this doc |
| C6 | Issue #540 describes exile as "a distinct open/violet rune frame or offset tray". | The baseline draws **translucent cyan-violet glass panes larger than the card**, stacked. Transcribed literally in §3.3 — the pane being larger than its card is the load-bearing difference from the graveyard. | this doc |
| C7 | The zones baseline arranges four seats as four corner clusters; `layout-model.md` stages receiver-band / far-side / wings. | The zones sheet is a **zone-anatomy** sheet, not a seat-staging sheet. `layout-model.md` continues to own seat staging. | this doc |
| C8 | `ZonePile.tsx` renders a permanent lowercase zone name under every pile. | §5 forbids a large permanent label; the name moves to the accessible name and the hover/focus etched glyph. | #534 |
| C9 | `ZoneBrowser.tsx` is a full-screen modal with `aria-modal="true"`. | Anchored panel (D12). The `aria-modal` semantics need re-deciding with the anchored form. | #534 |
| C10 | `LivePlane.tsx` publishes one `pile:<seat>` anchor for the whole cluster. | Per-zone keys (D11), or §9's "terminates at the actual pile" cannot hold. | #531 / #535 |
| C11 | `PLANE.pile = { w: 44, h: 62 }` describes a single 44 px pile *cluster*, not a four-slot rack. | The rack needs its own metrics block derived from `u`; §2 supplies the proportions. | #531 |

Open questions for the maintainer:

1. **G3 (format signal).** Should `GameView` carry an explicit format or a `commander:
   boolean`, or should `command` stop being omitted when empty in a Commander game? The
   second is the smaller change and closes the gap exactly.
2. **Library count badge (D6)** — the baseline deliberately shows a clean, unnumbered deck.
   Is the one-home rule worth the badge, or should the library's count live only in the
   accessible name and on hover?
3. **Command tax semantics (D9)** — `+N` tax, or the drawn bare `casts` numeral?
4. **Reveal shelf (§10)** — worth a protocol addition now (G2), or deferred until a card
   in the pool actually searches a library?

Browser verification of the rack — real layout at six seats, actual hit-target separation
at the 44 px floor, and true DOM node counts — is the maintainer's. Automated coverage for
this specification stops at jsdom/Vitest geometry assertions over `stagePlane` output.
