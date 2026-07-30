# The stack stage and the directional relationship grammar

Normative design specification for issue #541 (parent #464), and the design
authority for the stack/effect portions of #534, #535, #536, and #499.

This document **codifies the approved baseline images** in
[`../ui-concepts/`](../ui-concepts/) into an implementable spec. It decides
where stack objects live, what they look like, and how source, target,
direction, order, and impact are communicated. Numbers live inside
[`presentation-budgets.md`](presentation-budgets.md); hues and shape channels
come from [`visual-system.md`](visual-system.md); slots, focus, and the
degradation ladder come from [`layout-model.md`](layout-model.md).

## 0. Authority, scope, and invariants

### 0.1 Authority chain

| Rank | Source | Decides |
| --- | --- | --- |
| 1 | [`AGENTS.md`](../../AGENTS.md) hard rules, ADR 0030 | architecture; nothing below may violate these |
| 2 | The approved baselines (commit `e58300b`, issue #547) | anatomy, placement, geometry, which channels exist |
| 3 | [`presentation-budgets.md`](presentation-budgets.md) | every duration, count, and cap |
| 4 | [`visual-system.md`](visual-system.md) §2/§7/§8 | hue tokens, non-color channels, motion rows |
| 5 | [`layout-model.md`](layout-model.md) | slots, corridor, focus, rungs |
| 6 | This document | everything the above leave open |

Binding baselines, by panel:

| Image | Panel | What it binds here |
| --- | --- | --- |
| `rune-zones-interaction.jpg` | 9 "THE STACK" | stage position (right of centre, on the focused seat's flank), splayed pile, top entry expanded with art + controller portrait + rules text |
| `rune-zones-interaction.jpg` | 8 "TARGETING" | the arc from the stack entry to the targeted permanent |
| `rune-zones-interaction.jpg` | 10 "RESOLVE" | the resolve/respond control pair sits **below** the stage, not inside it |
| `rune-2.5d-interface-baseline.jpg` | — | the arc rises over the central corridor; source and destination wear different endpoint treatments |
| `rune-card-states.jpg` | 4 "TARGETED" | the circular reticle on the subject, inside its art window |
| `rune-card-states.jpg` | 6 "ATTACHMENT" | attachment is an **elbow bracket pair**, not an arc |
| `rune-card-states.jpg` | 7 "IDENTICAL STACK" | splayed pile + `×N` badge — the shape a folded group takes |
| `rune-card-states.jpg` | 9 "SPELL ON STACK" | stack-card anatomy: title bar, cost pip, art, type strip, rules box |
| `rune-card-system-overview.jpg` | STACK callout | the cast arc terminates **at the stack entry** with an arrowhead |
| `layouts-v1/layout-stackweb-v1.jpg` | — | the deep-stack condensed rail: compact rows, controller stripe on the slot, gold on the next to resolve |
| `layouts-v1/layout-phone-v1.jpg` | — | the compact bottom sheet, proven readable at depth 8 on 390 px |

### 0.2 Invariants (restated, binding on every section)

| # | Invariant | Consequence here |
| --- | --- | --- |
| I1 | Zero game logic in the client | Targets, order, controller, kind, and all descriptive text come from the server. The client never computes what targets what, never infers a trigger from text, never resolves anything. |
| I2 | One `GameView` + pending prompt rebuilds the whole UI | The stage is a pure function of `GameView.stack` and the in-progress `TargetingSession`. A reconnect mid-resolution rebuilds a *settled* stage and skips the impact animation — see §6.4. |
| I3 | Effects are a passive overlay (ADR 0030) | No path, reticle, arrowhead, or impact is ever a hit target. `EffectsLayer.root.eventMode = 'none'` stands. All picking is DOM. |
| I4 | The scene is never degraded | Lite quietens effects; the stage, its order, and its endpoints render identically at every quality level. |
| I5 | Non-color channels everywhere | Every relationship kind is separated by **geometry**, not hue (§4.3). |
| I6 | Input is never gated on animation | Entries are pickable at their final rects at 0 ms. |

### 0.3 Terminology

- **Stage** — the screen-space surface that presents `GameView.stack`.
- **Entry** — one rendered `StackItem`.
- **Slot** — the entry's wrapper. The slot, never the card face, wears the
  controller's seat accent (`visual-system.md` §5).
- **Relationship** — a directed pair the server states: a target, an
  attack, a block, an attachment, a source tether.
- **Path** — the drawn stroke of a relationship.
- **Source end / destination end** — the two ends of a path. Direction is
  always source → destination.

---

## 1. Stack stage placement

### 1.1 Transcription

The zones baseline places the stack **right of centre, on the focused
seat's flank**: outside the central corridor, inboard of the right-hand
zone rack (library / graveyard / exile), above the right command zone, and
with the RESOLVE / RESPOND controls directly beneath it. It is drawn as a
physical splayed pile of screen-space mini-cards with the **top entry
expanded**.

### 1.2 Normative placement

| Property | Value |
| --- | --- |
| Coordinate space | **Screen space.** No plane transform, no perspective foreshortening, drop shadow against the scene (`visual-system.md` §3, "Screen space" elevation row). |
| Anchor | Right-flank anchor `stack:stage`, expressed as a fraction of the viewport: right edge at `0.955 · W`, vertical centre at `0.46 · H`, grown **upward and leftward** from a bottom-right origin so depth never pushes into the receiver's band. |
| Growth direction | New entries enter at the **top-front** of the pile; older entries recede up-and-back. Top-first reading order, matching the shipped `StackPanel` and the `layout-stackweb` mock. |
| Width | `clamp(232px, 0.185 · W, 300px)` at the expanded tier. |
| Empty state | **Absent.** Zero nodes, zero reserved space, no placeholder (issue acceptance: "Empty stack consumes no permanent screen region"). The shipped `StackPanel` already returns `null`; that behaviour is carried. |
| Corridor | The stage's left bound may never cross `0.72 · W`. The centre corridor (`layout-model.md` §The plane and its fixed slots) stays clear for paths and combat webs. |
| Zone racks | The stage's right bound may never overlap the right-hand pile rack. When the rack and the stage would collide the stage steps **inward** (leftward) up to `0.05 · W`, then steps down a density tier (§3), and never covers a pile. |
| Occlusion rule | The stage may never cover: a current target, a current prompt candidate, the local hand fan, any seat's crest cluster, or any pile. It is drawn **under** the inspect surface and **over** the plane. |
| Slide freedom | The stage may slide within a bounded anchor band (`0.86 · W … 0.955 · W` right edge; `0.34 · H … 0.58 · H` centre) to clear the constraints above. It has **no other freedom** — it never crosses to the left flank, because a stable home is worth more than local optimality. |

### 1.3 Why screen space, and why not world space

The stack is not a place on the table; it is a queue the player reads. It
carries prose (`description`), which must stay at a fixed legible size at
every camera geometry — the same argument that put inspect in screen space
(budget rule: "Inspection is independent of battlefield card size"). A
world-space stack would foreshorten its own rules text. ADR 0003 already
places the stack in React DOM for the same reason.

**Consequence for paths:** a stack-anchored path has one screen-space
endpoint and one plane endpoint. Both resolve through the existing
`EffectAnchor` → `rects(ref)` seam, which reports current *screen* rects for
plane cards too, so no new coordinate machinery is required.

### 1.4 Relationship to the RESOLVE / RESPOND controls

Transcribed from panel 10: the resolve/respond pair sits **immediately below
the stage**, in the action dock's alignment, not inside the stage. It is
`valid_actions` chrome (`pass_priority` and friends) and belongs to the one
action home (`layout-model.md`). The stage never renders a button.

---

## 2. Stack entry anatomy

### 2.1 Three density tiers

| Tier | When | Height | Contents |
| --- | --- | --- | --- |
| **Expanded** | the top entry at any depth, or any focused entry | 168 px | full anatomy (§2.2) |
| **Mini** | non-top entries at depth ≤ 5 | 92 px | title bar, cost pip, cropped art band, controller ribbon, order index, target count chip |
| **Row** | every **non-top** entry at depth ≥ 6, and every entry on compact geometry | 48 px (≥ 44 px hit floor) | kind glyph tile, title, `kind · controller` subtitle, order index, controller stripe |

The row tier is the `layout-stackweb` / `layout-phone` transcription. One
entry is expanded at every tier — focus always promotes exactly one entry to
Expanded, drawn over the rail (never reflowing it).

**The top entry stays Expanded at every desktop depth** (maintainer ruling,
issue #534). An earlier wording of this table sent *every* entry to Row at
depth ≥ 6, which contradicted §3.1's own depth table and would have reduced
the next object to resolve — the one thing a player must be able to read — to
a 48 px row. The `layout-stackweb` mock does draw an all-rows rail with only
a gold ring on its top entry, and that mock is layout evidence rather than a
visual-quality target (`layout-model.md`); where the two disagree, §3.1
governs. Compact geometry is unchanged: the bottom sheet is all rows, because
there is no width for anything else.

### 2.2 Expanded anatomy (transcribed from card-states panel 9 + zones panel 9)

Top to bottom, as one screen-space mini Rune card seated in a slot:

| Element | Source | Notes |
| --- | --- | --- |
| Slot edge stripe | controller's seat accent | **on the slot, never the card face** (`visual-system.md` §5). 4 px, full left edge. |
| Order index badge | array position | `n/N`, ink chip, top-left of the slot, ≥ 12 px semibold. |
| "Resolves next" mark | top of stack | gold ring on the slot + the carried `Resolves next` text badge. Gold + ring shape, never gold alone. |
| Title bar | `card.name`, else `description` | 1 line, ellipsis, full text in inspect. |
| Cost pip | `card.mana_cost` | circular pip, upper-right of the title bar. Omitted when absent. |
| Art window | ADR 0024 art if the player opted in; procedural monogram otherwise | **Never a project-shipped image.** |
| Controller portrait thumbnail | seat crest art / monogram | 28 px disc, **bottom-left, overlapping the art window's lower edge**, in a seat-accent ring. Transcribed from zones panel 9. |
| Type strip | `card.type_line` | one line, muted. For abilities this reads `Activated ability` / `Triggered ability` (server-supplied kind, §11). |
| Rules text box | `card.rules_text`, else `description` | up to 3 lines, then a "more" affordance that opens inspect. |
| Target summary | `StackItem.targets` (gap G1) | numbered chips ①②③ matching the destination nodes of §4.5. |

### 2.3 Per-kind entry table

| Kind | Card face? | Distinguishing marks | Notes |
| --- | --- | --- | --- |
| **Spell** | yes — mini Rune card | frame accent from color identity; type strip from `type_line` | Focus expands to the inspect representation. |
| **Activated ability** | **no** — synthetic ability plate | plate substrate (flat raised surface `#23262B`, **square corners**, no card frame, no cost pip well); source-permanent thumbnail (24 px, rounded-square) at the plate's head; type strip reads `Activated ability`; a **source tether** (§4.3 R9) ties it to its permanent | Must not masquerade as a card: the square corners + missing frame accent are the shape channel. |
| **Triggered ability** | **no** — synthetic ability plate | same plate, plus a **trigger caret** glyph (▸) on the source thumbnail and the type strip `Triggered ability` | The server's own concise `description` is the body text. The client never fabricates trigger prose and never renders an invented card image. |
| **Copy** | mirrors its original's kind | a **doubled outline** on the slot (2 px inner + 1 px outer, 2 px apart) and a `Copy` chip | Copies stay separate entries whenever controller or targets differ. |
| **Targetless entry** | as its kind | the target summary row is **replaced by** a muted `No targets` chip; no path is drawn from it; its source disc is still drawn at rest | Absence of targets must be positively stated, never implied by an empty row. |
| **Folded group (`×N`)** | as its kind | splayed pile (2–3 px offsets, card-states panel 7) + `×N` badge | Permitted **only** when every member shares kind, controller, description, and target list, and none is individually pickable or individually orderable. Unfolding is always available (activation or `→`). |

### 2.4 Rules that bind every kind

1. An ability plate never renders a card image, a card frame, or a mana
   cost well. (`AGENTS.md`: no invented card representations; and the issue's
   "It must not masquerade as an ordinary card".)
2. Every entry at every tier carries: controller stripe, order index, kind
   marker, and — when it has targets — a target count. These four never
   degrade away.
3. Text is always the server's. The client concatenates and truncates; it
   never composes rules prose.

---

## 3. The depth ladder — 1, 2, 5, 8, and beyond

### 3.1 Splay geometry

Entries are laid out along a single axis with a constant per-entry offset,
producing the physical pile of the baseline:

| Depth | Layout | Per-entry offset | Tiers | Stage height |
| --- | --- | --- | --- | --- |
| **1** | one Expanded entry, no pile | — | 1 × Expanded | 168 px |
| **2** | Expanded top + 1 Mini receding | `(-10 px, -14 px)`, scale 0.94 | 1 × Expanded, 1 × Mini | ~212 px |
| **3–5** | Expanded top + Mini pile | `(-10 px, -14 px)` per step, scale ×0.94 per step, floor 0.80 | 1 × Expanded, ≤ 4 × Mini | ≤ 320 px |
| **6–8** | **condensed rail** — the pile collapses to a vertical list of Row entries; the top entry stays Expanded above the rail | 4 px gap, no scale change | 1 × Expanded, ≤ 7 × Row | ≤ 168 + 7·52 = 532 px |
| **> 8** | rail scrolls | as above | 1 × Expanded, 7 visible Rows | fixed at the depth-8 height |

The transition at depth 6 is the **collapse point**. It is a change of kind,
not a shrink: recession stops, rows begin. The `layout-stackweb` mock is the
depth-8 reference.

### 3.2 Beyond 8

The rail becomes a scroll container with `overflow-y: auto` and a fixed
height. Rules:

- The **top three** entries are always rendered and never scrolled out
  (sticky head), because "what resolves next" must never require scrolling.
- The **bottom** of the list is sticky too when depth > 10, showing the
  bottom-most entry, so the player can see both ends of the queue.
- A `+K more` divider chip separates the sticky head from the scrolled body
  and states the hidden count.
- Keyboard `Home`/`End` jump to the top/bottom entries; arrow keys scroll
  the focused entry into view. No relationship is lost: an entry scrolled
  out of view keeps its path, whose stage endpoint clamps to the rail's edge
  and grows an **edge indicator** (§10.3).

### 3.3 Order-index readability

The order index is a first-class element, not a decoration:

| Tier | Placement | Size |
| --- | --- | --- |
| Expanded | ink chip, top-left of the slot, `n/N` | 12 px semibold |
| Mini | ink chip, top-left, `n` only; `N` from the stage header | 12 px semibold |
| Row | leading numeral column, fixed width, `n` | 12 px semibold |

The stage header always reads `STACK (N) — TOP RESOLVES FIRST` (transcribed
verbatim from the `layout-stackweb` and `layout-phone` mocks), which supplies
`N` for tiers that show only `n`.

**Ordering source of truth:** `GameView.stack` is bottom-first on the wire.
The stage renders top-first. `n` is `stack.length - i` for wire index `i`.
This is a presentation reversal of a server-supplied array — not derived game
state.

---

## 4. The relationship grammar

This is the core deliverable. A single glowing arc says "these two things
are connected". It does not say who acts on whom, in what order, or what
kind of connection it is. The grammar below adds **direction**, **ownership**,
**order**, **kind**, and **impact** on top of the baseline arc.

### 4.1 The four constituents

Every relationship renders exactly four things:

1. a **source cap** at the acting object,
2. a **path** with a stated geometry,
3. a **direction device** (§4.2),
4. a **destination cap** at the subject (§5).

A relationship missing any of the four is a bug, not a style choice.

### 4.2 The direction device — DECISION

**Direction is carried by three redundant marks, ranked by survivability:**

| Rank | Device | Description | Survives reduced motion | Survives occlusion / bundling |
| --- | --- | --- | --- | --- |
| **D1** | **Endpoint asymmetry** | source = **filled disc**; destination = **open reticle + arrowhead** | yes | only when both ends are visible |
| **D2** | **Monotonic stroke taper** | the stroke widens from source to destination: `1.2 px` at the source, `3.4 px` at the destination, linearly along the sampled polyline | yes | **yes** — readable from any single visible segment |
| **D3** | **Flow** | dash-crawl travels source → destination (existing `dashSegments` phase, already implemented) | **no** | yes |

**Why taper is the primary device.** An arrowhead is a one-pixel-cluster
answer at one end of a curve; at eight-deep stacks and multi-defender combat
the arrowheads bunch inside the same reticles and stop discriminating.
Motion (D3) is the most legible device but is exactly the one that
reduced-motion users lose. A taper is the only device that is *static* and
*locally readable*: any single visible centimetre of the stroke states which
way the effect travels, even when both endpoints are behind a crowded board,
behind the phone sheet, or inside a bundled trunk. D1 and D3 stay because
redundancy is cheap here — the arrowhead is already implemented
(`geometry.ts::arrowHead`) and the taper costs **zero additional draw ops**
(the polyline is already 24 segments; each segment simply gets its own
`width`).

**Rejected alternatives:** colour gradient along the stroke (fails I5 and
`visual-system.md` §7, and the `DrawOp` segment primitive has one flat
colour); chevrons drawn along the path (adds ~12 segment ops per path and
reads as a second dash pattern); moving particles along the path (spends the
particle budget on a persistent effect, which the budgets reserve for
impacts).

### 4.3 Relationship kinds

Nine kinds, each with a **distinct geometry** — no two are separated by hue
alone. Hue is the fourth channel, taken from `visual-system.md` §2.

| # | Kind | Path geometry | Source cap | Destination cap | Hue family | Extra channel |
| --- | --- | --- | --- | --- | --- | --- |
| R1 | **card → card target** | single lifted quadratic bezier over the corridor (`pathCurve`, lift 80 px) | filled disc r 5 | reticle + arrowhead (§5.2) | orange (threat/intent) | subject wears the target-candidate ring |
| R2 | **card → player target** | same bezier, terminating on the crest medallion | filled disc r 5 | **90° arc cap** on the crest ring + arrowhead (§5.3) | orange | crest gains a `Targeted` chip |
| R3 | **card → zone target** | same bezier, terminating on the pile's top card | filled disc r 5 | **square bracket** over the pile (§5.4) — deliberately not a circle | orange | pile lifts 2 px, gains a zone-name chip |
| R4 | **card → stack-object target** | **short bezier with a 24 px lift**, staying inside the stage's bounds | filled disc r 5 on the source entry's outer edge | **inset reticle** in the destination slot + a notch cut in that slot's controller stripe (§5.5) | orange | destination slot gains a `Targeted by ①` chip |
| R5 | **multi-target from one source** | one **trunk** leaves the source, splits at a fan node placed 40 % along the trunk; one branch per target | one filled disc; a hollow fan node at the split | one **numbered** destination node per branch: ①②③ inside the reticle, matching the entry's target order | orange | the entry's target summary chips share the numerals |
| R6 | **multi-defender combat** | one bezier per attacker, bundled per defending seat (§10.2) | filled disc at the attacker | defender's cap (R2 or R7 form) | orange spectrum | attacker wears the top edge bar; defender crest shows `Attacked ×N` |
| R7 | **attacker → defending player** | bezier from the attacker to the defender's crest | filled disc | 90° crest arc cap + arrowhead | orange spectrum | attacker tilts ~6° toward the defender (`visual-system.md` §8) |
| R8 | **blocker → attacker** | **doubled parallel stroke, straight**, no lift, **no arrowhead** (the carried `combatLinks.ts` shape) | filled node at the **blocker** end (existing `nodeRadius: 4`) | none — the stroke simply terminates | combat-warm `#E4572E` | blocker wears the left edge bar. A block is a *bind*, not a directed effect: the absence of an arrowhead is the semantic. |
| R9 | **attachment / source tether** | **elbow bracket pair** — two axis-aligned right-angle connectors, one per direction, drawn in line-work neutral (`rgba(232,230,225,.14)`) | small square terminals at both ends | small square terminals at both ends | neutral line work | Transcribed from card-states panel 6. Static, never dashed, never animated. **This is the shape that says "attached / belongs to", and it must never be confused with a target path** — different colour family, different geometry, symmetric caps. Used for aura/equipment attachment *and* for an ability plate's tether back to its source permanent. |

### 4.4 Path states

Each path is in exactly one state, and the state is legible without motion:

| State | When | Stroke | Dash | Direction devices live | Alpha |
| --- | --- | --- | --- | --- | --- |
| **Pending** | an active `TargetingSession` slot; the pointer/keyboard cursor is over a candidate | tapered | **dashed**, crawling (`dashLen 12 / dashGap 9`, 900 ms period) | D1 D2 D3 | 0.9 |
| **Provisional** | a slot the player has already answered, session not yet submitted | tapered | dashed, **crawl stopped**, phase 0 | D1 D2 | 0.9 |
| **Confirmed** | the object is on the stack with server-stated targets | tapered | **solid** | D1 D2 | 0.9 |
| **Calmed** | confirmed, but another entry is focused, or the board is crowded | tapered | solid | D1 D2 | 0.32 (`COMBAT_LINK.crowdedAlpha`, reused) |
| **Endpoint-only** | > 6 concurrent paths with nothing focused | **no stroke** | — | D1 (both caps) | 0.6 |
| **Resolving** | the entry is resolving | tapered | solid, **retracting** source → destination | D1 D2 | 0.9 → 0 |
| **Retired** | resolved, countered, or an endpoint vanished | not drawn | — | — | — |

`Endpoint-only` is the scalability floor and the issue's "nonfocused paths
reduce emphasis but retain endpoints": the relationship is never silently
lost, but the corridor never fills with strokes. It costs 2 circle ops per
relationship.

### 4.5 Multi-target numbering

Numerals are the ordering channel and they are shared across three surfaces:

1. the entry's target summary chips (①②③),
2. the destination nodes on the board,
3. the accessible name of the entry (§9.2).

Numerals come from the **index in the server's target list** (gap G1) — never
from geometry, never from left-to-right screen order.

### 4.6 What is never drawn

- No path from a targetless entry (§2.3).
- No path for a relationship the server did not state.
- No path is ever a hit target (I3).
- No path is drawn to an endpoint that cannot be resolved to a rect — the
  existing `buildProgram` behaviour of **retiring** rather than drawing stale
  is carried verbatim, with §10.3's edge indicator taking over.

---

## 5. Endpoint treatments

### 5.1 Source cap

| Property | Value |
| --- | --- |
| Form | filled disc, r 5, in the relationship's hue |
| Placement | on the source object's bounding-rect edge, at the point nearest the destination |
| Companion | the source object holds elevation level 2 while its path is pending (`visual-system.md` §3) |
| Ability plate | the disc sits on the **plate's outer edge**; the tether (R9) separately marks the permanent |

### 5.2 Target reticle (card-states panel 4)

| Property | Value |
| --- | --- |
| Form | open ring, r 14, stroke 2 |
| Interior | one inward **chevron** aligned to the path's arrival tangent — the arrowhead lands inside the ring, as in the `layout-stackweb` mock |
| Placement | the centre of the destination's **art window**; degrades to the destination rect's centre when only a rect is known (**DECISION** — see §12) |
| Pending | ring breathes (`micro` class, 120 ms in, steady beacon) |
| Confirmed | ring closed and static |
| Companion | the permanent wears the target-candidate ring from `visual-system.md` §7 — the reticle is the *path's* terminal, the ring is the *object's* state; both are required |

### 5.3 Player endpoint

| Property | Value |
| --- | --- |
| Form | a **90° arc segment** drawn on the seat's crest ring, centred on the path's arrival tangent, stroke 3 |
| Arrowhead | drawn just outside the arc, pointing inward |
| Companion chip | `Targeted` (R2) or `Attacked ×N` (R6/R7), on the crest cluster |
| Anchor | the existing `seat:<id>` ref, which the plane already resolves to the crest cluster or, on compact geometry, the summary tile's mini-crest |
| Why an arc, not a reticle | a crest is a circle already; a concentric reticle would read as decoration. The arc reads as a *hit on the shield*. |

### 5.4 Zone endpoint

| Property | Value |
| --- | --- |
| Form | a **square bracket** (three strokes: two 12 px arms and a 28 px spine) laid over the pile's top card, opening toward the arriving path |
| Arrowhead | at the spine's midpoint, pointing into the pile |
| Companion | the pile lifts 2 px and shows its zone-name chip (`Graveyard` / `Exile` / `Library` / `Command`) |
| Why a bracket | a zone is a container, not a body. The rectilinear form is the non-color channel that separates "into this pile" from "at this object". |

### 5.5 Stack endpoint

| Property | Value |
| --- | --- |
| Form | reticle at r 10 (inset), drawn **inside** the destination slot's bounds |
| Slot mark | a 6 px notch cut in the destination slot's controller stripe, at the arrival height — visible even when the reticle is occluded by the pile splay |
| Chip | `Targeted by ①` on the destination entry |
| Path | never leaves the stage's bounds (R4); a counterspell's arc is a short hop between two entries, not a trip across the arena |

### 5.6 Endpoint conflict rule

When one object is both a source and a destination in the same frame (a
counterspell that is itself countered), the **source disc is drawn on the
outer edge and the reticle in the interior**. They never overlap, and their
forms already differ (filled vs open). The object's accessible name states
both roles.

---

## 6. Impact and resolution

### 6.1 The distinction the grammar must hold

Targeting communicates **intention**; resolution communicates **result**.
They are separated by three simultaneous changes: the path's dash pattern
(dashed → solid → retracting), the entry's presence (arriving → held →
retiring), and the destination's cap (ring → impact transient).

### 6.2 Resolution sequence

When an entry resolves, in one composed motion of **≤ 570 ms** (cap 600 ms):

1. The path **retracts** from the source toward the destination over 300 ms
   (`SCENE_MOTION.resolution` easing), its taper compressing so the whole
   stroke converges on the destination cap.
2. At `t = 120 ms` the destination reticle **collapses** into the impact
   transient — one `TransientInvocation` per destination, category chosen by
   the event, spawned on the existing effects layer.
3. The entry crumples out of the stage (250–350 ms, `visual-system.md` §8
   "Countered / fizzle" shape reused for retirement).
4. The authoritative view has **already** applied the state change (I6).
   The animation decorates a fait accompli.

### 6.3 Terminal forms by event

Categories are the existing `TransientCategory` set; nothing bespoke per card
(`asset-pipeline.md`: data-driven categories keyed to game events).

| Event | Category | Terminal form | Hue family |
| --- | --- | --- | --- |
| Damage | `damage` | impact ring + burst at the subject; damage badge pops | red |
| Destruction / death | `death` | impact ring + downward burst; the card's travel to the graveyard is a separate zone-travel motion | red |
| Exile | `impact` | ring + bracket flash at the exile pile | red |
| Bounce | `impact` | ring at the subject, then a zone-travel to the hand anchor | blue |
| Life change | `damage` / `healing` | ring at the crest + delta chip | red / green |
| Counter added / removed | `counter-change` | ring at the subject + counter badge delta | green |
| Enters the battlefield | `battlefield-entry` | ring at the new permanent | green |
| Draw | `draw` | ring at the library pile | blue |
| **Countered / fizzled** | `counter` | ring at the **stack object**, never at its target | red |

**The fizzle rule is normative and load-bearing.** A countered spell's
terminal is at the stack entry. Its target path retires with a distinct
**release** form: the reticle *opens* (radius grows 14 → 20) and fades over
200 ms without any burst, so "nothing happened to me" is a visible event and
not merely the absence of one.

### 6.4 Reconnect mid-resolution

`GameView` carries no "currently resolving" flag, and must not need one.
On reconnect the client receives whatever the stack is *now* and renders the
settled stage with confirmed paths and no transients. Nothing is lost,
because impacts are decoration over already-applied state (I2, I6). The
reconnect rebuild budget (≤ 50 ms desktop / ≤ 100 ms mobile) applies to the
stage exactly as to the plane.

---

## 7. Motion storyboard and reduced-motion equivalents

### 7.1 Frame timings

`t` is relative to the start of each moment, not to the sequence.
Every row is cross-checked against `presentation-budgets.md` §Animation and
`sceneTokens.ts`.

| # | Moment | `t` | Duration | Motion class (cap) | What moves | Reduced-motion equivalent (same information) |
| --- | --- | --- | --- | --- | --- | --- |
| F0 | **Offered** | 0 | 120 ms | micro (150) | gold edge bar draws on every action-bearing object | gold edge bar present at full opacity, no draw-on |
| F1 | **Source lift** | 0 | 120 ms | micro (150) | source rises to elevation 2, blue selection ring draws | ring present, no lift, no draw-on |
| F2 | **Pending path** | 0 | 150 ms draw-on, then loop | micro (150) + 900 ms dash loop | tapered dashed path draws source → destination; dashes crawl; reticle breathes | **full dashed tapered path rendered instantly at phase 0**, static reticle. D1 + D2 carry direction; D3 is dropped. |
| F3 | **Confirm** | 0 | 120 ms | micro (150) | dashes fuse to solid, reticle snaps closed, numeral appears | solid tapered path + closed reticle + numeral, all at once |
| F4 | **Stack placement** | 0 | 320 ms travel; 400 ms stage re-stage | zoneTravel (400), staging (500) | the card shrinks toward the stage; the stage grows/collapses a tier | entry present at its final rect on the first frame; stage at final geometry; **no travel ghost** |
| F5 | **Priority held** | 0 | 2 s breathing loop | non-blocking cue (500 for the transition) | gold ring on the top entry breathes; phase plaque updates | **static double ring** (the carried `visual-system.md` §6 form, blue-white since #534), phase plaque text |
| F6 | **Resolve travel** | 0 | 300 ms | resolution (600) | path retracts source → destination, taper compressing | path removed in the same frame the state applies |
| F7 | **Impact** | 120 | 450 ms | resolution (600) | reticle collapses; category transient bursts at each destination | one **static** ring held 200 ms (`EFFECT_TIMING.reducedHoldMs`), no particles |
| F8 | **Retirement** | 300 | 270 ms | resolution (600) | entry crumples (scale + ~5° rotate) and falls out; remaining entries re-splay | entry absent on the next frame; remaining entries at their new rects |
| F9 | **Fizzle / counter** | 0 | 350 ms | resolution (600) | terminal at the stack object; target reticle *opens* and fades | entry absent; target reticle absent; the log entry carries the fact |
| F10 | **Off-focus stack activity** | 0 | 300 ms | (existing row) | crest ping at the acting seat (`offFocusPingMs`) | static ping badge held 1000 ms (`offFocusHoldMs`) — already implemented |

**Composed sequence check:** F6 (0–300) ∪ F7 (120–570) ∪ F8 (300–570) spans
**570 ms**, inside the ≤ 600 ms resolution cap and therefore **not
individually skippable** (`SCENE_SKIP_THRESHOLD_MS`). It remains
interruptible by a newer authoritative view, which is a separate contract.

### 7.2 The reduced-motion contract for this grammar

| Information | Full-motion carrier | Reduced-motion carrier | Preserved? |
| --- | --- | --- | --- |
| Which object is the source | source disc + lift + flow origin | source disc (D1) | yes |
| Which object is the subject | reticle + breathing + arrowhead | reticle + arrowhead (D1) | yes |
| **Direction** | flow (D3) + taper (D2) + caps (D1) | **taper (D2) + caps (D1)** | yes |
| Multi-target order | numbered nodes | numbered nodes | yes |
| Stack order | position + index badge | position + index badge | yes |
| Controller | slot stripe + portrait | slot stripe + portrait | yes |
| Pending vs confirmed | crawling vs solid dashes | **static dashed vs solid** | yes |
| Resolution happened | retraction + burst | 200 ms static ring + the applied state + the log entry | yes |
| Fizzle vs impact | opening ring vs burst | opening ring drawn once, held 200 ms | yes |

Every row has a non-motion carrier. This is the acceptance criterion
"Reduced motion preserves source, direction, target, and order".

---

## 8. Cost of the grammar against the binding budgets

### 8.1 Draw-op accounting

Measured in `DrawOp`s per the existing `buildProgram` primitives.

| Element | Ops | Notes |
| --- | --- | --- |
| Dashed path, ~600 px, dash 12 / gap 9 | ~28 `segment` | unchanged from today |
| Solid path (confirmed) | 24 `segment` | the sampled polyline |
| Taper | **0 additional** | per-segment `width`, already a field |
| Arrowhead | 2 `segment` | existing `arrowHead` |
| Source disc | 1 `circle` | new |
| Reticle + chevron | 1 `circle` + 1 `segment` | new |
| Crest arc cap | 1 `circle` (stroked) + 2 `segment` | approximated by a stroked circle clipped by the crest — see §12 D8 |
| Zone bracket | 3 `segment` | new |
| Fan node (multi-target) | 1 `circle` | new |
| Blocker link (R8) | 2 `segment` + 1 `circle` | unchanged |
| Attachment bracket (R9) | 4 `segment` + 2 `circle` | new; static, drawn once |
| **Endpoint-only path** | **2 `circle`** | the crowded-board floor |

Worst declared stress state (`presentation-budgets.md`: eight-deep stack with
a live targeting session and a live animation batch, six seats, multi-defender
combat):

| Concurrent set | Count | Ops | Notes |
| --- | --- | --- | --- |
| Focused entry's paths (full) | 3 | ~96 | focus isolates |
| Other confirmed stack paths | 7 | 14 | endpoint-only |
| Combat links (gang block) | 6 | 18 | calmed at `crowdedAlpha` |
| Attack paths | 4 | ~120 | bundled (§10.2) |
| Attachments | 4 | 24 | static |
| **Total** | — | **~272 ops** | one pooled `Graphics`, one draw call |

For reference, a single 24-sample dashed path already costs ~30 ops today,
and the layer is render-on-demand. ~272 ops in one `Graphics.clear()` +
stroke pass is well inside the frame budget on the measured path; the
disqualified path (full-viewport 2D canvas) is not used.

### 8.2 Particle accounting

| Rule | Value |
| --- | --- |
| Particles spawned by **paths** | **zero** — every path element is a stroke |
| Particles spawned by **caps** | zero |
| Particles spawned by **impacts** | `BURST_BASE 24 × magnitude × DENSITY_SCALE` |
| Standard density (0.4), magnitude 1 | ~10 per impact |
| 8 simultaneous impacts, Standard | ~80 live, against the **150** cap |
| High density, 8 impacts, magnitude 1.5 | ~288 live, against the **400** cap |
| Lite | 0 particles — pulses and edge flashes only, per the quality table |
| Reduced motion | 0 particles |

Persistent effects are deliberately kept in the stroke budget so the particle
pool stays reserved for impacts. This is why chevron-particles were rejected
in §4.2.

### 8.3 Transient accounting

`TRANSIENT_CAP` is 64 / 32 / 8. A resolution emits one transient per
destination. Multi-target caps at the server's target-list length; a
board-wipe batch is already governed by the ≤ 80 ms stagger / ≤ 800 ms window
clamp in `replaceTransients`. Nothing here raises the ceiling.

### 8.4 Idle cost

The stage is DOM and costs nothing when static. The effects layer's
zero-idle contract is preserved: confirmed solid paths and attachment
brackets are **static** and must not mark the layer as animating. This
requires one behavioural rule — `isAnimating()` must treat a *confirmed*
(solid) path the same way it already treats `blocker-link`, i.e. static.
Today every non-`blocker-link` persistent category animates. Recorded as
implementation note IN1 (§13).

---

## 9. Controller identity, ordering, and accessibility

### 9.1 Controller identification (three channels, no colour dependency)

| Channel | Where | Notes |
| --- | --- | --- |
| Seat accent stripe | the **slot's** left edge, 4 px | never on the card face (`visual-system.md` §5) |
| Controller portrait / monogram | 28 px disc on the Expanded tier; 16 px on Mini; the glyph tile's ring on Row | transcribed from zones panel 9 |
| Name text | `player_names[controller]`, else the raw id, in the subtitle (`spell · Sorel`) | the only channel a screen reader needs |

Seat accents are assigned deterministically by `seat_order`, so a stripe means
the same thing on every client and across a reconnect.

### 9.2 Screen-reader model

The stage is a real DOM list, always present in the accessibility tree while
nonempty (the shipped `StackPanel` shape, extended):

```
<section aria-label="Stack, 5 objects, top resolves first">
  <ol>
    <li> … one entry per StackItem, top-first … </li>
```

Each entry's accessible name is assembled **only** from server fields plus
the array position:

> "1 of 5. Resolves next. Spell, Arcane Bolt, controlled by you. Deal 3
> damage to target creature. Targets: 1, Æther Channeler."

Assembly order is fixed: `index` → `top?` → `kind` → `name` → `controller` →
`description` → `targets`. Ability entries insert `source` after `kind`:
"Triggered ability from Dawn Herald".

| Requirement | Mechanism |
| --- | --- |
| Announce stack changes | one `aria-live="polite"` region: "Added to stack: Counterspell, controlled by Sorel, targeting Shock." Text is `description` + `player_names`; nothing is composed by the client. |
| Announce resolution | "Resolved: Arcane Bolt." — emitted from the log, which is already in the DOM. |
| Never announce animation | transients are `aria-hidden`. |
| Relationships are reachable as text | each entry exposes a `Targets` sub-list; each target is a button whose name is the target's server-supplied name and whose activation moves focus/scroll to it. |
| Off-screen targets | the target button's name is suffixed with its seat: "Ridge Wolf, Tam's battlefield". |

### 9.3 Keyboard model

| Key | Action |
| --- | --- |
| `Tab` | enters the stage as one stop (roving tabindex) |
| `↑` / `↓` | move between entries; the focused entry promotes to Expanded and **isolates** its paths (all other paths drop to Calmed/Endpoint-only) |
| `Home` / `End` | top / bottom entry |
| `→` | enter the focused entry's `Targets` sub-list, or unfold a `×N` group |
| `←` | leave the sub-list / refold |
| `Enter` | inspect (the existing inspect handle) |
| `Space` | pick this entry as the answer to the active target slot, when it is a server-listed candidate (the shipped behaviour) |

Focus isolation is the same policy `combatLinks.ts::selectVisibleLinks`
already implements for combat, generalised to every relationship kind.

### 9.4 Non-color summary for relationships

| Relationship | Shape channel (colour-independent) |
| --- | --- |
| Target (any) | tapered stroke + open reticle/arc/bracket + arrowhead |
| Direction | taper (D2) + endpoint asymmetry (D1) |
| Multi-target order | numerals ①②③ |
| Attack | tapered stroke + top edge bar on the attacker + tilt |
| Block | **doubled parallel stroke, no arrowhead** + left edge bar |
| Attachment / tether | **elbow bracket, symmetric square terminals** |
| Stack order | numeric index + list position |
| Resolves next | ring shape + `Resolves next` text |
| Controller | stripe position + portrait + name text |

---

## 10. Routing at six seats and on compact geometry

### 10.1 Corridor discipline

Every path between the stage and a wing seat routes through the **centre
corridor** (`layout-model.md`). The bezier's control point is already lifted
80 px above the higher endpoint; at six seats the lift is raised to
`min(140 px, 0.16 · H)` so the family of paths separates vertically instead
of overlapping across the far side's cards. Paths never cross a crest cluster
and never cross the receiver's hand fan.

### 10.2 Bundling

When ≥ 3 paths share a **destination seat**, they bundle:

1. each path runs individually from its source to a **gather node** placed on
   the corridor side of the destination seat's region, 64 px out along the
   arrival normal;
2. the shared trunk runs from the gather node to the destination cap;
3. the trunk's stroke width is the **sum of member tapers, clamped to 6 px**,
   so a fat trunk visibly means "many";
4. the trunk carries **one** arrowhead; each member keeps its own source disc
   and its own numeral at the gather node.

Bundling never hides a relationship: the member count is on the trunk
(`×N` chip) and every member's endpoints stay drawn.

### 10.3 Off-screen and occluded endpoints

An endpoint that is scrolled out of the stack rail, behind the compact sheet,
or outside the viewport gets an **edge indicator** instead of silently
dropping (the current `buildProgram` drops such effects; §13 IN2):

| Property | Value |
| --- | --- |
| Form | a 20 px chevron on the nearest container edge (rail edge, sheet rail, or viewport edge), pointing along the path's would-be tangent |
| Label | the endpoint's name, truncated, with its numeral |
| Path | the path terminates **at the indicator** with its normal cap, so direction is still readable |
| Accessibility | the indicator is a real button; activating it scrolls/focuses the endpoint |

### 10.4 Compact geometry (phone portrait, and landscape below 1180 px)

Transcribed from the phone mock, which proves depth 8 at 390 px:

| Property | Value |
| --- | --- |
| Kind change | the stage becomes a **bottom sheet**, `STACK (N) — TOP RESOLVES FIRST` header, all entries at the Row tier, scrollable |
| Height | ≤ 0.42 · H, so the focused board and the receiver's dock stay visible |
| Trigger | opens automatically when the stack becomes nonempty and the receiver holds priority; otherwise a collapsed 32 px handle showing `STACK (N)` and the top entry's title |
| Hit targets | every row ≥ 44 px |
| Relationships | **preserved in the scene**, not moved into the sheet. Paths draw above the sheet's scrim; any endpoint the sheet covers gets a §10.3 edge indicator on the sheet's top rail |
| Expanded tier | one entry may promote to Expanded as an overlay card above the sheet; the sheet does not reflow |
| Dismissal | swipe-down or the handle; dismissal never affects game state (I2) |

### 10.5 Tablet landscape

At and above the 1180 px floor, tablet landscape holds desktop staging
(`layout-model.md`), so the stage is the desktop right-flank stage with the
Row tier engaging at depth 6 as usual. Below the floor it takes the compact
sheet branch.

---

## 11. Data mapping — every element to its protocol field

Legend: **OK** = the field exists today; **GAP** = the field does not exist
and the element cannot be rendered correctly without it.

### 11.1 Stack elements

| Element | Source | Status |
| --- | --- | --- |
| Stage presence | `GameView.stack.length > 0` | OK |
| Stack order / index `n/N` | position in `GameView.stack` (bottom-first; reversed for display) | OK |
| "Resolves next" | last element of `GameView.stack` | OK |
| Entry identity | `StackItem.id` | OK |
| Controller stripe, portrait, name | `StackItem.controller` + `GameView.player_names` + `seat_order` (accent assignment) | OK |
| Body text | `StackItem.description` | OK |
| Spell vs ability | `StackItem.kind` (`spell` / `ability`), server-stated (#550) | OK |
| Ability source thumbnail + name | `StackItem.card` (the source's current face), with `StackItem.source` for the tether | OK (both absent once the source has left play — render the C5 plate state) |
| **Activated vs triggered** | `StackItem.kind` (`activated` / `triggered`), server-stated (#579) | OK — the engine records an `AbilityOrigin` at each push site and the projection states it; the caret glyph of §2.3 is a rendering change on top of it |
| **Copy marker** | — | **GAP G3 — deferred** (no copy mechanic exists to project) |
| **Mini card face** (name, `type_line`, `mana_cost`, `rules_text`, frame accent, art identity) | `StackItem.card` (#550) | OK |
| **Target list and order** (chips ①②③, target count, `No targets`) | `StackItem.targets` (#550), ordered | OK |
| **Mode / X / additional-cost summary** | — | **GAP G5 — deferred** (no modal spell or `X` cost exists to project) |
| Inspect content | `onInspect(item.id)` → existing inspect surface | OK |

### 11.2 Relationships

| Relationship | Source | Status |
| --- | --- | --- |
| R1/R2/R3/R4 **pending** path (targeting session) | `ValidAction.requirements[].candidates` + the local `TargetingSession.picks` | OK — this is the only target relationship the client can draw today |
| R1/R2/R3/R4 **confirmed** path (object already on the stack) | `StackItem.targets` (#550) | OK for R1/R2/R4; R3 stays dormant (G7) |
| R5 multi-target fan | `requirements[]` (pending) / `StackItem.targets` (confirmed) | OK |
| R6/R7 attack | `Permanent.attacking`, `Permanent.attacking_player` | OK |
| R8 block | `Permanent.blocking` | OK |
| R9 attachment | `Permanent.attached_to` | OK |
| R9 ability source tether | `StackItem.source` | OK |
| Destination is a **player** vs a **permanent** | `StackTarget.kind` (`player` / `permanent` / `card` / `stack`), typed at the source (#550) | OK for a confirmed path; a *pending* one still classifies `candidates[]` locally |
| Destination is a **zone** | — | **GAP G7 — deferred** (no zone target exists to project) |
| Impact category at resolution | `GameView.log` entries + view diff (the existing presentation adapter) | OK (coarse) |
| Fizzle terminal | log entry | OK (coarse) |
| Seat crest anchor | `seat:<player_id>` ref (existing) | OK |

### 11.3 The gaps, stated as contract changes

Each is a protocol change and therefore must land in `sage-protocol`,
`docs/protocol.md`, and the TypeScript mirror in one PR. All seven are the
`StackItem` contract gaps filed as **#550**; G2 needed an engine change first
and closed with **#579**.

| # | Gap | Minimal shape | Consequence if not closed | Tracked by |
| --- | --- | --- | --- | --- |
| **G1** | `StackItem` carries no targets — targets exist only as prose baked into `description` | `targets?: EntityId[]` on `StackItem`, **ordered**, matching the order the description names them | **Blocking.** No confirmed relationship can be drawn for anything already on the stack. Panel 8 of the zones baseline — an arc from a stack entry to a permanent — is unimplementable. The client must not parse `description` to recover them (I1). | #550 |
| **G2** | No kind discriminator | `kind?: "spell" \| "ability" \| "activated" \| "triggered"` (`copy` with G3) | Triggered and activated abilities are indistinguishable; §2.3's trigger caret cannot be driven. | **Closed** — #550 (protocol half) + #579 (engine half) |
| **G3** | No copy relation | `copy_of?: EntityId` | The `Copy` chip and the doubled outline cannot be driven; copy folding cannot be validated. | #550 |
| **G4** | No card face on a stack object | `card?: CardView` on `StackItem` | The Expanded entry cannot show name, cost pip, type strip, frame accent, or an art window — the baseline's stack card anatomy degrades to a single text line. | #550 |
| **G5** | No mode / X / additional-cost summary | free-form `choices?: string[]` | The issue's "mode/X/additional-cost summary where data exists" cannot be met; the row is simply omitted until it exists. | #550 |
| **G6** | Player vs permanent vs stack-object destinations are not typed | either a `kind` on the target reference, or a documented guarantee that a client may classify by membership in `battlefield` / `seat_order` / `stack` | Endpoint treatment (§5.3 vs §5.2 vs §5.5) is chosen by client-side classification, which is fragile and brushes against I1. | #550 |
| **G7** | Zones are not targetable references | a zone reference form (`{player, zone}`) | R3 (card → zone target) has no data source and is specified but dormant. | #550 |

**Status after #550 and #579.** G1, G4, and G6 were closed by #550: `StackItem`
carries `kind`, an ordered `targets` list typed at the source, and the `card`
face to render (see `docs/protocol.md`, *Permanents and stack objects*). §4's
confirmed states are therefore implementable for R1/R2/R4/R5 as well as combat
(R6–R8) and attachment (R9) — a rendering change, exactly as this document
intended.

**G2 is closed by #579.** #550 landed the discriminator but the engine's
`StackObjectKind::Ability` recorded only that an ability was on the stack — an
activation and a trigger pushed the identical object — so the server could prove
`spell` vs `ability` and no more. #579 added an `AbilityOrigin`
(`Activated` / `Triggered`) set at each of the two push sites, and the projection
states it as `kind: "activated" | "triggered"`. The wire union widened additively:
`ability` remains the coarse value a pre-#579 server sends, and a client that sees
it renders generically rather than picking one. §2.3's trigger caret therefore has
a data source; drawing the glyph is a rendering change on top of it, not a
contract change.

Three remain open, each for a stated reason rather than an oversight:

- **G3** and **G5** are deferred: there is no copy mechanic, no modal spell, and
  no `X` cost to project, so `copy_of` and a choices summary would be fields no
  projection could ever fill. They land with the mechanics that need them.
- **G7** is deferred for the same reason (no zone target exists), so R3 stays
  specified and dormant, as C7 already ruled.

---

## 12. Decisions this document makes (the images did not dictate these)

| # | Decision | Rationale |
| --- | --- | --- |
| D1 | The stage is **screen space**, not world space | rules text must not foreshorten; ADR 0003 already puts the stack in DOM |
| D2 | Right-flank anchor with a bounded slide band; **never** crosses to the left flank | the baseline puts it right of centre; a stable home beats local optimality |
| D3 | The **taper** is the primary direction device, arrowhead and flow are secondary | only the taper is both static and locally readable; see §4.2 |
| D4 | Density ladder collapses to a **row rail at depth 6** (a change of kind, not a shrink) | reconciles the zones baseline (shallow, splayed, expanded) with the `layout-stackweb` mock (deep, condensed) |
| D5 | The top three entries are **sticky** beyond depth 8 | "what resolves next" must never require scrolling |
| D6 | Attachment (R9) uses the **elbow bracket** with symmetric square terminals and is never an arc | transcribed from card-states panel 6 and made a hard separation from target paths |
| D7 | Blocks (R8) get **no arrowhead** | a block is a bind, not a directed effect; the absence is the semantic |
| D8 | The player endpoint is a **90° arc on the crest ring**, not a reticle | a crest is already a circle; a concentric ring reads as decoration |
| D9 | The zone endpoint is a **square bracket**, not a circle | containers get rectilinear caps; bodies get circular ones |
| D10 | The reticle is placed at the destination's **art-window centre**, degrading to rect centre | card-states panel 4 shows it inside the art, offset from the card centre; a rect-only anchor cannot know the art window |
| D11 | **Endpoint-only** is the crowded-board floor (2 ops per relationship) | the issue's "nonfocused paths reduce emphasis but retain endpoints", made concrete and cheap |
| D12 | Multi-target uses a **trunk + fan node + numbered branches**, not N independent arcs | N arcs from one source form a starburst that hides the source; the fan node states "one source, many subjects" |
| D13 | Bundling merges by **destination seat**, splitting within 64 px of the crest | keeps the corridor legible at six seats without hiding membership |
| D14 | Fizzle uses an **opening, fading ring** at the released target | "nothing happened to me" must be a visible event, not an absence |
| D15 | On compact geometry, relationships stay **in the scene**, not in the sheet | moving them into the sheet would destroy the spatial relationship the grammar exists to state |
| D16 | Folding (`×N`) requires identical kind, controller, description **and** target list | anything else risks folding away an individually pickable object |
| D17 | The stage never renders a button; RESOLVE / RESPOND belong to the action dock | one action home (`layout-model.md`, [ADR 0032](../decisions/0032-contextual-shell-anatomy.md) — the commitment is retained from ADR 0023; the dock's location moves to the lower-right cluster) |

---

## 13. Implementation notes (non-normative, for the issues that consume this)

| # | Note |
| --- | --- |
| **IN1** | `EffectsLayer.isAnimating()` currently treats every non-`blocker-link` persistent category as animating. Confirmed (solid) paths and attachment brackets are static and must join `blocker-link` in the static set, or the zero-idle contract breaks whenever anything is on the stack. |
| **IN2** | `buildProgram` retires an effect whose endpoint cannot be resolved. §10.3 needs a third outcome — *clamp to a container edge and emit an indicator* — for endpoints that are occluded rather than gone. The distinction must come from the caller (the endpoint exists in the view but has no rect), not from the layer. |
| **IN3** | `PersistentCategory` needs new members for the grammar: `attachment-bracket`, `source-tether`, and a path **state** field (`pending` / `provisional` / `confirmed` / `calmed` / `endpoint-only` / `resolving`) so the draw program is a pure function of declared state. |
| **IN4** | The `segment` `DrawOp` already carries a per-segment `width`; the taper needs no new primitive. `pathCurve`'s 24 samples are the taper's resolution. |
| **IN5** | `StackPanel.tsx` (143 lines) grows into the stage. Split before it crosses the ceiling: `stack/StackStage.tsx` (placement + ladder), `stack/StackEntry.tsx` (anatomy per kind), `stack/stackOrder.ts` (pure index/tier policy, unit-tested). |
| **IN6** | Relationship selection is a pure function `(GameView, TargetingSession|null, focusId|null) → PersistentEffect[]`, testable GPU-free exactly like `combatLinks.ts`. It must live beside `combatLinks.ts` and subsume it. |
| **IN7** | Everything in this document is assertable through the ADR 0011 structural-snapshot idiom (assert the draw program, not pixels) plus jsdom tests of the DOM stage. Nothing here needs a browser suite, and none may be added (`AGENTS.md`). |

---

## 14. Conflicts and open questions

Recorded here rather than resolved by editing other documents.

| # | Conflict | Detail | Recommendation |
| --- | --- | --- | --- |
| **C1** | **Hue of the target path.** The baselines draw the targeting beam in **cyan/blue** with a **violet** glow on the affected permanent. `visual-system.md` §2 assigns **orange `#E0784A`** to "targeting; attack and block relationships" and **blue `#7FB2E5`** to selection; the `layout-stackweb` mock (also approved) draws the stack-to-blocker target path in **orange**. | The two approved image sets disagree with each other, so the images cannot settle it. | **Resolved by maintainer ruling: blue selection, orange targeting** — `visual-system.md` wins. This document already specified orange (§4.3) and needs no change; the concept art's cyan/blue beam is the **rejected alternative** and is read as illustrative licence. Colour was the weakest channel here anyway: every relationship stays separated by geometry (§4.3). |
| **C2** | **"SELECTED" hue.** `rune-card-states.jpg` panel 3 labels a **violet** outer glow as SELECTED; in the composed scenes (`rune-2.5d-interface-baseline.jpg`, `rune-card-system-overview.jpg`) the violet glow appears on the *targeted* permanent while the *selected* card wears cyan. `visual-system.md` says selection is blue. | Same family as C1. | **Resolved by maintainer ruling: selection is blue `#7FB2E5`.** The sheet's violet is the **rejected alternative**, read as the concept art's rendering of the affected/targeted state. §7.1 F1's blue selection ring stands unchanged. |
| **C3** | **Stack position: pile vs rail.** `rune-zones-interaction.jpg` shows a splayed physical pile right of centre inside the arena; `layouts-v1/layout-stackweb-v1.jpg` shows a flush right-edge vertical rail of compact rows. | Both are approved. | Resolved here as a **density ladder** (§3, D4): the pile is the shallow state, the rail is the deep state, both on the same right-flank anchor. No maintainer action needed unless the reading is rejected. |
| **C4** | **`StackItem.description` already bakes in targets.** The shipped `StackPanel` doc comment states the server bakes chosen targets into `description`. G1 would make the same information available structurally. | Prose and structure would coexist. | Keep `description` authoritative for *text*; add `targets` for *geometry*. The client must never parse one to obtain the other. |
| **C5** | **Ability source may be unresolvable.** `sourceName()` falls back to the raw entity id when an ability's source has left play or is hidden. | The Expanded entry needs a thumbnail and a name. | Specify a `Source no longer on the battlefield` plate state with a neutral glyph and no tether. Consider carrying the source's name on `StackItem` alongside G4. |
| **C6** | **Reticle placement needs an art-window rect.** D10 wants the art-window centre; the effects layer only receives whole-card rects through `rects(ref)`. | Cosmetic, but it is the transcription. | Either expose an `art:<id>` ref alongside the card ref, or accept the rect-centre degradation. **Open.** |
| **C7** | **Zone targeting (R3) is dormant.** Specified in full because the issue requires it, but no protocol field supplies it (G7). | Nothing renders it today. | Leave specified and dormant. Do not invent a client-side zone-target inference. |
| **C8** | **Six-seat corridor lift.** §10.1 raises the bezier lift to `min(140 px, 0.16·H)` at six seats. This number is a design judgement, not a measurement, and no other document pins it. | — | Validate against the six-seat fixture when #536 implements it; adjust here with the measurement. |

---

## 15. Acceptance mapping (issue #541)

| Acceptance criterion | Where met |
| --- | --- |
| A viewer can tell what is on top, who controls it, where it came from, what it targets, and which direction the effect travels | §2.2, §3.3, §4.2, §4.3, §9.1 |
| Spell and ability entries are visually distinct | §2.3 (plate substrate, square corners, no frame accent, source thumbnail); activated/triggered separation has its data source since G2 closed (#579), and the caret glyph is the remaining rendering change |
| Targeting and resolution are distinct moments | §4.4 (path states), §6.1, §7.1 F2–F3 vs F6–F8 |
| Multi-target and crowded states remain traceable | §4.5, §4.4 Calmed / Endpoint-only, §10.2 bundling, §10.3 edge indicators |
| Empty stack consumes no permanent screen region | §1.2 empty state |
| Reduced motion preserves source, direction, target, and order | §7.2 (every row has a non-motion carrier; direction survives via D2 + D1) |
| Deliverable: stage sheet at 1, 2, 5, 8 entries; spell / ability / trigger / copy / targetless | §3.1, §2.3 |
| Deliverable: relationship sheet (card / player / zone / stack / multi-target / multi-defender) | §4.3 R1–R9 |
| Deliverable: direction + impact storyboard and reduced-motion equivalents | §7 |
| Deliverable: controller / ordering / accessibility spec | §9 |
| Deliverable: mobile and 6p routing proof | §10 |
| Maintainer approves stack and relationship sheets | **outstanding — this document is the submission** |
