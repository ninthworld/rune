# The RUNE control and direct-manipulation language

**The design authority for match controls, buttons, drag/drop, prompt placement,
and cancel semantics** (issue #543, under master issue #464). It codifies the
approved baselines committed in `e58300b` (issue #547) into an implementable,
normative specification. Where the baselines decide, this document transcribes;
where they are silent, it decides and marks the decision in §14.

Binding inputs, in precedence order:

1. [`AGENTS.md`](../../AGENTS.md) hard rules — `valid_actions[]` is the only
   source of interactivity; the client computes no legality, cost, or effect;
   the whole UI rebuilds from one `GameView` + pending prompt.
2. The approved baselines:
   [`rune-player-control-ui.jpg`](../ui-concepts/rune-player-control-ui.jpg)
   (panels 6, 6b, 7),
   [`rune-zones-interaction.jpg`](../ui-concepts/rune-zones-interaction.jpg)
   (panels 5, 6, 7, 10),
   [`rune-2.5d-interface-baseline.jpg`](../ui-concepts/rune-2.5d-interface-baseline.jpg)
   (cluster in situ, lower right).
3. [ADR 0004](../decisions/0004-subject-owned-actions.md) (actions carry
   subjects), [ADR 0025](../decisions/0025-direct-entity-activation.md)
   (direct entity activation), [ADR 0020](../decisions/0020-priority-automation.md)
   (priority automation and `set_stops`).
4. [`ui-requirements.md`](ui-requirements.md), [`visual-system.md`](visual-system.md),
   [`layout-model.md`](layout-model.md), [`presentation-budgets.md`](presentation-budgets.md).

Conflicts between these inputs are recorded in §15 and resolved there, not by
editing the other documents.

---

## 1. Reading the baselines: frame and scale anchor

Every baseline is a 1672 × 941 render of a 16:9 landscape table. Measurements
below are in **baseline pixels** read off those files.

**Scale anchor (decision D1).** The smallest control in the cluster — the
circular icon button — measures 44 × 44 baseline px, exactly the 44 CSS-px
touch floor. The control language therefore adopts **1 baseline px = 1 CSS px**
and treats chrome control sizes as fixed CSS values (as `chrome/tokens.css`
already does), not viewport fractions. Board geometry keeps scaling per
[`layout-model.md`](layout-model.md); controls do not.

Raw measurements, panel by panel:

| Element | Panel | Measured (w × h) | Notes |
| --- | --- | --- | --- |
| Primary pill "PASS PRIORITY" | control-ui 6 | 267 × 58 (blue face 253 × 54) | stadium; frame 3 px |
| Primary pill "CAST SPELL" | 2.5D in situ | 195 × 50 | same form, tighter cluster |
| Icon button (rune glyph) | control-ui 6 | 44 ⌀ | circle, gold ring |
| "UNDO" pill | control-ui 6 | 118 × 35 | chamfered rect, gold frame |
| Phase plaque | control-ui 6 | 270 × 68 | hexagonal, points ≈ 22 px deep |
| Phase plaque (compact) | control-ui 6b | 190 × 62 | same form, one text line |
| Step pips | control-ui 6 | 11 ⌀, 21 px pitch | 4 drawn (3 filled) |
| Forward chevron | control-ui 6 | 18 × 20 glyph | blue, right end of plaque |
| Decision plaque "CHOOSE ATTACKERS" | control-ui 7 | 251 × ~72 | dark plate, gold frame |
| CONFIRM / CANCEL | control-ui 7 | 106 × 31 each, 10 px gap | chamfered rect |
| RESOLVE (filled) | zones 10 | 117 × 33 | chamfered rect, blue face |
| RESPOND (outline) | zones 10 | 117 × 32, 12 px below | chamfered rect, dark face |
| Drop region plate | zones 6 | 133 × 151 | card footprint + padding |
| No-entry glyph | zones 6 (invalid) | 52 ⌀, 6 px stroke | centered |

Cluster geometry: column width **268 px**, right margin **28 px** from the
viewport edge, row gap **12 px**, icon↔pill gap **12 px**.

**Touch correction (decision D2).** Four drawn controls fall below the 44 px
floor (UNDO 35, CONFIRM/CANCEL 31, RESOLVE/RESPOND 33). The drawn **plate**
heights are preserved as the visual form; the **hit box** is padded to 44 px
with transparent inset padding. Plate and hit box are separate values in §2.

---

## 2. Token additions

New custom properties for `clients/web/src/chrome/tokens.css`. Existing tokens
are reused wherever they already fit and are named as such.

### 2.1 Control color

| Token | Value | Source |
| --- | --- | --- |
| `--rune-primary-face-top` | `#315DBB` | control-ui 6, sampled |
| `--rune-primary-face-bottom` | `#2755B1` | control-ui 6, sampled |
| `--rune-primary-frame` | `#1C3977` | primary bevel, sampled |
| `--rune-primary-rim` | `rgba(232, 240, 255, 0.45)` | inner top highlight |
| `--rune-primary-glow` | `rgba(36, 133, 240, 0.40)` | outer bloom |
| `--rune-confirm-face` | `#2D7A4C` | control-ui 7 CONFIRM |
| `--rune-confirm-frame` | `#1C4E31` | derived shade |
| `--rune-danger-face` | `#82412F` | control-ui 7 CANCEL |
| `--rune-danger-frame` | `#5A2C20` | derived shade |
| `--rune-control-plate` | `#1B2430` | UNDO / RESPOND / plaque plate |
| `--rune-control-frame` | `#8F8844` | gold frame midtone, sampled |
| `--rune-control-frame-hi` | `#C6B472` | frame highlight (top-left) |
| `--rune-control-frame-lo` | `#5E5A31` | frame shade (bottom-right) |
| `--rune-pip-on` | `#2C8CFA` | filled step pip, sampled |
| `--rune-pip-off` | `rgba(232, 230, 225, 0.22)` | hollow pip ring |
| `--rune-chevron` | `#275BBE` | plaque chevron, sampled |
| `--rune-drag-glow` | `#2485F0` | zones 5 lifted-card rim |
| `--rune-origin-dash` | `#5E90C5` | zones 5 origin slot dash |
| `--rune-drop-valid` | `#8FC49A` | zones 6 valid stroke, sampled |
| `--rune-drop-valid-fill` | `rgba(143, 196, 154, 0.16)` | translucent tint |
| `--rune-drop-invalid` | `#DA866C` | zones 6 invalid stroke, sampled |
| `--rune-drop-invalid-fill` | `rgba(0, 0, 0, 0.28)` | **dim**, not a red wash |

Carried unchanged: `--rune-selection` `#7fb2e5`, `--rune-targeting` `#e0784a`,
`--rune-accent-gold` `#f2c94c`, the surface tiers, `--rune-touch` `44px`,
`--rune-transition` `140ms ease`, `--rune-elevation-1/2`.

### 2.2 Control geometry and type

| Token | Value | Use |
| --- | --- | --- |
| `--rune-control-h-primary` | `56px` | primary pill plate height |
| `--rune-control-h` | `36px` | pill / confirm / cancel / resolve plate |
| `--rune-control-hit` | `44px` | every control's minimum hit box |
| `--rune-control-w-cluster` | `268px` | control-cluster column width |
| `--rune-control-w-pair` | `118px` | paired button (UNDO, CONFIRM, RESOLVE) |
| `--rune-control-chamfer` | `8px` | 45° corner cut on the chamfered family |
| `--rune-plaque-h` | `68px` | phase plaque plate height |
| `--rune-plaque-point` | `22px` | hexagonal point depth |
| `--rune-control-frame-w` | `2px` | gold frame stroke |
| `--rune-primary-frame-w` | `3px` | primary bevel frame stroke |
| `--rune-pip-size` | `11px` | step pip diameter |
| `--rune-pip-pitch` | `21px` | pip center-to-center |
| `--rune-type-action` | `24px` | primary label (new) |
| `--rune-drop-radius` | `12px` | drop-region corner radius (= `--rune-radius-lg`) |
| `--rune-drop-tick` | `14px` | corner-tick arm length |
| `--rune-drop-stroke` | `2px` | drop-region stroke (3px when hovered) |

Type reuses the existing scale: primary `--rune-type-action` 24px, plaque title
`--rune-type-title` 20px, plaque sub-line `--rune-type-caption` 12px, decision
title `--rune-type-heading` 16px, pill labels `--rune-type-body-lg` 14px,
compact primary (RESOLVE) `--rune-type-heading` 16px. Every control label uses
`--rune-font-display` in small caps with `0.06em` letter-spacing, as drawn.

---

## 3. The button family

Two shapes only, as drawn. Everything else is a fill, a frame, or a size.

- **Stadium pill** — the large primary alone. Radius = plate height ÷ 2
  (`--rune-radius-pill`).
- **Chamfered rect** — every other control: secondary outline, confirm,
  cancel, utility pill, compact primary. 8 px 45° corner cuts, gold frame.
- The **circle** (icon button) and the **hexagonal plaque** are the two
  non-button surfaces in the family.

### 3.1 Component sheet

| Component | Plate (w × h) | Hit box | Shape / radius | Fill | Frame | Type | Baseline |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Primary (large) | 268 × 56 | full plate | stadium, `999px` | vertical `--rune-primary-face-top` → `-bottom` | 3 px `--rune-primary-frame` + 1 px `--rune-primary-rim` inner top | 24 px display caps, `#F4F7FF` | control-ui 6 |
| Primary (compact) | 118 × 36 | 118 × 44 | chamfer 8 px | same gradient | 2 px gold | 16 px | zones 10 RESOLVE |
| Confirm | 118 × 36 | 118 × 44 | chamfer 8 px | the primary's blue enamel (see below) | 2 px gold | 14 px | control-ui 7 |
| Cancel / destructive | 118 × 36 | 118 × 44 | chamfer 8 px | `--rune-danger-face` | 2 px gold | 14 px | control-ui 7 |
| Secondary outline | 118 × 36 | 118 × 44 | chamfer 8 px | `--rune-control-plate` | 2 px gold | 14 px | zones 10 RESPOND |
| Utility pill | 118 × 36 | 118 × 44 | chamfer 8 px | `--rune-control-plate` | 2 px gold | 14 px | control-ui 6 UNDO |
| Icon button | 44 ⌀ | 44 ⌀ | circle | `--rune-control-plate` | 2 px gold ring | glyph 20 px | control-ui 6 |
| Phase plaque | 268 × 68 | see §5 | hexagon, 22 px points | `--rune-control-plate` | 2 px gold | 20 px + 12 px | control-ui 6 |
| Decision plaque | ≥251 × content | container | rect, `--rune-radius-md` | `--rune-overlay-bg` | 2 px gold | title 16 px | control-ui 7 |

Frame construction (all gold-framed components): 2 px stroke, a 135° gradient
`--rune-control-frame-hi` → `--rune-control-frame` → `--rune-control-frame-lo`,
plus a 1 px `rgba(0,0,0,0.5)` inner line. The stroke is drawn as two clipped
boxes, and the inner box's outline is the outer's **offset inward by the stroke**,
not the same outline re-resolved against a smaller box — a shared polygon puts
2.83 px of gold in a chamfer's corners and thins the plaque's trim to 1.68 px at
its leading point (issue #571). The offset is derived in
`table/controls/plaqueGeometry.ts` and shipped as `--rune-control-chamfer-face`,
`--rune-plaque-face-point`, and `--rune-plaque-face-tip`. The primary's bevel is the same
construction in the blue family, with the pale inner rim at the top edge only —
"restrained bevel, pale inner highlight, dark outer edge", exactly as drawn.

**There is one primary colour, and no green control** (issue #586). Control-ui
panel 7 draws CONFIRM green, and it shipped that way. Drawn beside the blue
enamel that every other primary action in the product wears — `PASS`, `CONNECT`,
`CREATE GAME`, `SUBMIT DECK` — that is a second primary hue, and §4.1 allows
exactly one primary treatment at a time. §4.2 rule 1 already resolves it: while a
decision is open the cluster's blue slot is empty *because* the decision's
CONFIRM carries the advance. The confirm therefore **is** the primary and wears
the compact primary's enamel, and `--rune-confirm-face` is retired. Red survives
on cancel/destructive alone, where it means exactly one thing.

Several **named choices** offered on equal terms are the tie of rules 4 and 7,
not a row of primaries: they render as equal-weight secondaries. Promoting one
would be the client ranking the server's own options.

Every control ships an accessible name. Icon buttons carry `aria-label` and a
tooltip; there are no unlabeled glyphs (issue #543 requirement, carried).

**Two icon buttons may not draw the same picture** (issue #583). The family has
one circular silhouette, so an icon button's *only* distinguishing mark at 44 px
is its glyph, and the match shipped two — the activity badge at the top right and
the game-menu handle at the bottom right — as `≡` and `☰`: different codepoints,
one picture. The accessible names were correct and invisible; a sighted player
had two identical controls in opposite corners. The drawn glyphs are declared
together in `table/controls/iconGlyphs.ts` (the badge keeps the rule stack, the
menu handle takes a gear, which is also what its drawer holds), so a future
collision is a failing assertion rather than something a reviewer has to notice.

### 3.2 State matrix

Applies to every component above. Deltas are additive to the rest state.

| State | Visual delta | Non-color cue | Motion | Reduced motion |
| --- | --- | --- | --- | --- |
| Rest | as §3.1 | — | — | — |
| Hover | face lightens 6 %; frame highlight to `--rune-control-frame-hi`; lift 2 px | 2 px lift | 80–150 ms (`hover/focus lift` class) | no tween |
| Active / pressed | face darkens 8 %; inset shadow; lift → 0 | plate depresses | ≤100 ms | instant |
| Focus (keyboard / controller) | 2 px `--rune-selection` ring, 2 px offset, drawn **outside** the frame | ring, always visible, never suppressed | ring draw ≤100 ms | ring appears |
| Selected / toggled | `aria-pressed="true"`; face gains the `--rune-gold-fill` wash; frame → `--rune-accent-gold` | filled inner bar along the bottom edge | 100–150 ms | appears |
| Disabled | face at 40 % opacity; frame flattens to `--rune-border` | label prefixed with the server-stated reason chip | none | none |
| Pending-server | face at 70 %; a 2 px `--rune-primary-rim` hairline sweeps the bottom edge | hairline (a shape, not a hue) + `aria-busy="true"` | ≤600 ms loop | static hairline |
| Deadline warning | frame → `--rune-targeting`; countdown chip appended | chip text ("0:07") | pulse 1 Hz | static chip |
| Rejection recovery | ≤3 px horizontal shake ×2 | non-blaming toast | ≤200 ms (visual-system §7) | toast only |

**Disabled is rare by construction.** An action the server has not offered is
not rendered at all. A control renders disabled only where the server states
the reason — today the single case is `PromptOption.requires` not yet satisfied
(protocol §Targets and prompts), which the client reports verbatim as
"needs: <slot prompt>". Any other "greyed out" control is a bug (see GAP-4).

### 3.3 Grouping and hierarchy

- The **control cluster** occupies the lower-right column, 268 px wide, 28 px
  from the viewport edge, stacked: primary → (icon button, utility pill) →
  phase plaque, 12 px gaps. This is the geometry of control-ui panel 6 and the
  2.5D baseline in situ.
- **Compact cluster** (control-ui 6b) = phase plaque + icon button only. It is
  what the cluster degrades to when no primary and no utility control is
  offered — i.e. when `valid_actions` is empty. Nothing is hidden that was
  actionable.
- Secondary controls never equal the primary's weight: they are half height,
  half width, and never blue-filled except the compact primary.
- **Concede/leave never appears in the cluster.** It lives behind the game menu
  (the icon button, D5) with a two-step confirmation and the danger treatment,
  physically separated from the ordinary primary.
- **The bar never enumerates per-card actions** (ADR 0004). The cluster holds
  subject-less actions plus the contextual echo of the current selection, and
  nothing else, at any board size.
- **A surface opens at the control that discloses it** (issue #583). The cluster
  is bottom-anchored, so everything it discloses opens **upward** from its own
  top edge, clamped to the viewport by a max-height built from the cluster's
  band height and margins — the plaque's step list, and the game menu's drawer.
  A disclosed surface may never land on a control in another corner. The drawer
  shipped at `top: 52px`, the height of the top bar ADR 0032 removed, so
  pressing the handle in one corner made a panel appear over the activity badge
  in the other.

### 3.4 The menu rung — the language's one viewport term (issue #566)

D1's scale anchor (§1) is right for the **match**: the arena is full of cards,
the cluster is deliberately small edge furniture, and chrome that grew with the
viewport would eat the board. It is wrong for the **menus**, where the arena is
empty. The pregame sized itself from §3.1's fixed widths and the fixed type ramp
with no viewport term anywhere, and the maintainer read the result as a UI shrunk
into the corner of a picture: the `RUNE` lockup, the status lines, and `CONNECT`
all too small to be the screen's one action.

So the family gains a **rung**: a restatement of §3.1's own values, fluid
between each value and a ceiling. Each rung is one `clamp()` per token —

    clamp(§3.1 value, §3.1 value / basis × 100vmin, §3.1 value × cap)

| Rung | basis | cap | Where |
| --- | --- | --- | --- |
| (none) | — | — | the match — §3.1 exactly, untouched |
| open | 620 | 1.6 | front door, server lobby, create-table |
| dense | 900 | 1.25 | the ready room's seating ring |

The rules the rung follows:

- **It restates the component sheet; it never states a new number.** Every
  length is derived from the §3.1 value it scales, so §3.1 still decides the
  proportions. At a rung's floor a menu control is bit-for-bit a match control,
  which is what keeps this a scale pass and not a second button vocabulary.
- **The floor is the §3.1 value.** A rung never draws a control *smaller* than
  the match does, so the 44 px anchor D1 pins can never be undercut by a small
  or zoomed viewport.
- **Trim is not on a rung.** The chamfer and the frame stroke are fixed: the
  face-outline offsets of §3.1 (issue #571) are an exact derivation that only
  holds at the drawn values, and a 45° cut reads as the same cut at any plate
  height.
- **`vmin`, never `vw`.** An ultrawide's surplus width is gutter — the plane
  spends it the same way (`layout-model.md`) — and growing controls into it
  would push an arena's content off the top and bottom of a short window.
- **Two rungs, because there are two kinds of menu arena.** The *open* rung
  serves a place that puts one decision on an empty plaza. The *dense* rung
  serves the ready room, which puts a seat per player plus the table's own
  plaque inside one box whose height must clear half a seat plus half the
  centre; seats grow with the rung and the arena does not, so the open rung
  would reintroduce the collision #546 fixed.

**How a surface takes a rung.** It re-points §3.1's own tokens at that rung's
set — the idiom the pregame already uses for `--rune-control-w-pair`. Nothing in
`table/controls` knows a rung exists: every rule there reads exactly one token,
so a control on a menu and the same control in a match are one component drawn
at two sizes, and the plaque behind it and the text beside it move with it
because they read the same tokens.

**Why not a single scale multiplier.** The obvious shape is one unitless
`clamp(1, calc(100vmin / 620), 1.6)` that every length multiplies by. It is
invalid CSS — `100vmin / 620` is a *length*, so the clamp mixes a length with
two numbers, the declaration is dropped at computed-value time, and every
property that multiplied by it is dropped with it. There is no portable way to
derive a unitless ratio from the viewport (`calc(100vmin / 620px)` is
`<length>/<length>`, which Firefox does not resolve), so the fluid term is
carried by each length. A second trap sits behind the first: a *derived* custom
property is substituted where it is declared, so `:root { --w: calc(var(--base)
* var(--scale)) }` cannot see a `--scale` a descendant overrides. Re-pointing
avoids both, because the tokens are read by ordinary properties on descendants.

`chrome/tokens.css` declares both rungs, `table/controls/controlTokens.ts`
mirrors `basis` and `cap` as `MENU_RUNG`, `controlTokens.test.ts` recomputes
every clamp from the token it restates, and `pregame/menuRung.test.ts` resolves
the stylesheets under CSS's own substitution model and checks the number each
scope ends up laying out from.

The rung is stated here rather than in the pregame because #580 is the same
fixed-width-versus-viewport failure in the match and should spend the same term
rather than inventing a second one.

---

## 4. One blue primary per state, and how its label is derived

### 4.1 The rule

At most **one** blue primary control exists on screen at any time. It is the
control that advances the current decision. It is never invented: the slot
holds a control only when the derivation below resolves to an entry in
`valid_actions[]`, and its label is that entry's server-supplied `label`,
rendered verbatim in display caps.

### 4.2 Derivation

Evaluated top to bottom; **first match wins**. Inputs are the current
`GameView.valid_actions`, the client's ephemeral selection, and any open
targeting/multi-select session. Every test is a *count* or a *membership* test
over the server's own list — no legality, cost, or effect is computed.

| # | Condition | Primary slot | Label |
| --- | --- | --- | --- |
| 1 | A multi-select session is open | empty (blue); the decision plaque's **CONFIRM** carries the advance | `MultiSelectControls.confirm.label` (from `action.label`) |
| 2 | A targeting session is open | empty; the plaque holds **CANCEL** only; the last pick submits | — |
| 3 | Exactly one entity is selected **and** it has exactly one action whose `subject` contains it | that action | `action.label` — e.g. `CAST SPELL`, `PLAY LAND`, `TAP FOR MANA` |
| 4 | An entity is selected with ≥ 2 subject-actions | **empty** | — (all render as equal-weight secondaries in the echo) |
| 5 | No selection, and `valid_actions` contains an entry with `type === "pass_priority"` | that entry | `action.label` — e.g. `PASS PRIORITY` |
| 6 | No selection, no `pass_priority`, and exactly one entry has an empty `subject` (excluding `concede`) | that entry | `action.label` — e.g. `CONFIRM ATTACKERS` |
| 7 | No selection, no `pass_priority`, and ≥ 2 subject-less entries (excluding `concede`) | **empty** | — (all render as secondaries) |
| 8 | `valid_actions` is empty | **empty**; cluster degrades to compact (6b) | plaque reads "Waiting" |

Invariants of the derivation:

- **`concede` is never eligible** for the primary slot (rule 6/7 exclude it).
- **Rule 4 and 7 deliberately refuse to pick a "best" action.** Choosing among
  several offered actions would be a client judgment; ties render flat.
- **`RESOLVE` is a server label, not a client rewrite.** The zones baseline's
  `RESOLVE` is rule 5 with the server labelling `pass_priority` contextually
  while the stack is non-empty. The client must not swap `PASS PRIORITY` for
  `RESOLVE` itself — deciding that a pass resolves the top of the stack is a
  rules judgment (ADR 0020 makes exactly this point). Contextual labelling of
  `pass_priority` is **GAP-2**.
- Rules 3 and 5 are the two labels the baselines actually draw
  (`CAST SPELL` with a hand card selected; `PASS PRIORITY` with nothing
  selected). Rules 1, 2, 4, 6, 7, 8 are decision **D8**.

### 4.3 The RESPOND secondary

Zones panel 10 pairs `RESOLVE` (blue, filled) with `RESPOND` (outline).
`RESPOND` is **not an action** — no such entry exists in `valid_actions`, and
inventing one is forbidden. It is specified (**D6**) as a pure navigation
control: activating it clears focus from the primary and moves focus into the
hand fan, so the player can select something to cast instead of passing. It
sends nothing, changes no game state, and is `aria-label`led "Respond instead
of passing". It renders whenever the primary is `pass_priority` and the stack
is non-empty (`view.stack.length > 0`) — a view field, not a derivation.

### 4.4 Cluster placement of the pair

The zones baseline draws RESOLVE/RESPOND beside the stack; the control-ui
baseline draws the primary in the lower-right cluster. These are the same
control (**D7**): the stack rail is the right-hand column and the control
cluster sits at its foot ([`layout-model.md`](layout-model.md)), so a
stack-adjacent pair *is* the cluster. The cluster does not move. What changes
is the primary's **form**: whenever the RESOLVE/RESPOND **pair** renders, the
cluster uses the compact primary + secondary pair (118 × 36 each) instead of
the full-width stadium pill, so the pair reads against the rail above it.

**The form follows the pair, not the stack** (maintainer ruling). An earlier
wording gated the form switch on `view.stack.length > 0` alone, while §4.3
gates RESPOND on a non-empty stack **and** a `pass_priority` primary. Those
come apart: a `CAST SPELL` primary over a non-empty stack drew a lone 118 px
pill with nothing beside it — half of a pair the baselines only ever draw
whole. The compact form therefore engages exactly when RESPOND renders; in
every other state the primary keeps the stadium.

---

## 5. The phase plaque

Transcribed from control-ui panels 6 and 6b and the 2.5D in-situ cluster.

```
┌ ◄ ─────────────────────────────────────────── ► ┐
│  MAIN PHASE                                  ›  │   268 × 68, hexagonal
│  YOUR TURN        ● ● ● ○                       │   points 22 px deep
└ ────────────────────────────────────────────────┘
```

| Part | Content | Source | Type |
| --- | --- | --- | --- |
| Title line | current step name | `STEP_NAME[view.phase]` (shipped map) | 20 px display caps |
| Ownership line | `Your turn` / `<name>'s turn` / `Priority` | `view.active_player` vs `view.you`, `view.priority_player`, `view.player_names` | 12 px caps, `--rune-selection` |
| Step pips | phase-group progress | `view.phase` classified into `PHASE_GROUPS` | 11 ⌀, 21 px pitch |
| Forward chevron | step-list disclosure | — | 18 × 20 glyph, 44 px hit box |
| Auto-passed badge | transient "Auto-passed" | `view.auto_passed` | 11 px |

Two rules the drawn plaque needs and the transcription did not state (issue
#586; the frame/face construction itself is unchanged):

- **Nothing overlaps a point.** The plaque's ends are 22 px diagonals, not
  edges, and the gaps around it are measured between bounding boxes — so panel
  6b's menu icon, 12 px from the plaque's box and centred on the same midline,
  lands 12 px from the *tip* and reads as sitting on it. A control beside the
  plaque clears the point depth as well as the gap, and the compact plaque gives
  the same term back out of its width so the row still measures one 268 px
  column.
- **The Auto-passed badge is a tab, not a toast.** It shares the plate's top
  edge and its material — no gap, no free-floating capsule on the overlay
  background — and is held inboard of the trailing point. It is still transient,
  still driven by `view.auto_passed` alone, and still outside the plate because
  6b has only one text line.

**The plaque says the phase; a decision says the question.** When a decision's
action label is the same words as `STEP_NAME[view.phase]` — "Declare Attackers"
is both — the decision surface drops its drawn heading rather than printing the
phrase twice in two treatments a few hundred pixels apart. Its accessible name
still carries the title; no server word is ever rewritten, only not repeated.

### 5.1 Step pips

The baseline draws four pips (three filled) in panel 6 and three (two filled)
in situ — the counts are illustrative, not a model. **D3:** pips render the
**five shipped phase groups** (`beginning`, `main`, `combat`, `main`, `ending`)
already implemented in `PhaseIndicator.tsx`, in turn order:

| Group state | Fill | Non-color cue |
| --- | --- | --- |
| Passed | `--rune-pip-on`, 100 % | solid disc |
| Current | `--rune-pip-on` + 2 px `--rune-selection` ring | disc **plus ring** (distinct shape) |
| Upcoming | transparent, 1.5 px `--rune-pip-off` ring | hollow ring |

Five pips at 21 px pitch = 95 px, which fits the plaque beside the ownership
line. The pip row is `aria-hidden`; the semantic step sequence is the expanded
`<ol>` behind the chevron, exactly as shipped.

### 5.2 The forward chevron, ADR 0020, and `set_stops`

**D4:** the chevron is a **disclosure control**, not a game action. It toggles
the full twelve-step list (the shipped `indicator-steps` panel), where every
step carries its stop toggle and answers with `set_stops`. It:

- carries `aria-expanded` / `aria-controls`, rotates 90° when open (reduced
  motion: swaps glyph, no tween);
- opens the list **above** the plaque (the cluster is bottom-anchored),
  viewport-clamped, per `ui-requirements.md` (phase expansions must render
  entirely within the viewport);
- commits nothing to the game. The only message it can produce is `set_stops`,
  whose full new preference is sent on each toggle and whose sole source of
  truth is `view.stops` / `view.own_turn_stops` (nothing is stored
  client-side).

Each step's toggle has **three** settings, not two — `Auto` → `Your turn` →
`Always`, cycling — because the server keeps two stop lists per seat (issue
#455): one that fires on any turn, and one that fires only while the seat is the
active player. The second is where the server seeds the *human default* (your
own main phases), so a two-state toggle could not draw a seat that already
carries stops it never set, nor let it clear them. The two set states differ by
their word and by frame shape (solid vs dashed), never by hue alone; every click
sends both whole lists.

The chevron's forward-pointing form reads as "where the turn is going next",
which is what the step list shows. It must **not** be wired to a
"skip ahead"/"pass until my next stop" action: no such action exists
(**GAP-3**). The escape hatch from an auto-pass chain is a stop, exactly as
ADR 0020 specifies, and the chevron is the door to setting one.

When `view.auto_passed` is set, the plaque shows the transient "Auto-passed"
badge (shipped behaviour) and the chevron's affordance gets a one-shot gold
outline for ≤ 1 s to point at the fix — display-only, dropped on the next view.
`view.auto_passed_steps` says *where* the settle acted for this seat — an ordered
path of turn-and-step positions, which the plaque reports in two places because
neither alone can carry all of it:

- the **badge's accessible name** reads the whole path in order, grouped into
  per-turn runs ("Auto-passed for you at End Step on turn 1, then Upkeep, Draw
  and End Step on turn 2"). This is where a settle that crossed a turn is
  actually reported, since a twelve-row list cannot show a step belonging to two
  turns at once. Every occurrence is spoken; nothing is de-duplicated.
- the **step list** marks the positions belonging to `view.turn` only, with its
  own glyph (`↷`, distinct from the turn trail's `✓` — "you were passed here" is
  a stronger claim than "the turn went through here", and the two are never both
  drawn on one step). A step the settle visited twice this turn carries a count
  (`↷×2`), because one row and two visits are different quantities.

Marking a row from a *previous* turn's entry would claim a skip at a step this
turn has not reached, so the filter is not an optimisation — it is the
correctness condition. Neither mark animates, so the reduced-motion form carries
exactly the same information.

---

## 6. Direct manipulation

Drag is an enhancement over the same server-authoritative model. Everything it
can do is reachable without it (§7). Nothing is sent until a drop lands on a
server-named destination.

### 6.1 Drop regions are derived only from server-named targets

For a subject entity `E` and the offered actions whose `subject` contains `E`:

| Action shape | Drop regions | Effect of the drop |
| --- | --- | --- |
| No `requirements`, no `prompts`, `type ∈ {play_land, cast_spell}` | one **commit area**: the receiver's own battlefield band ∪ the stack rail, treated as a single semantic region | submit `ChooseAction{id, token}` |
| No `requirements`, any other `type` | none — drag is not offered; the action fires from the entity or the echo | — |
| Has `requirements` | the rendered rect of every id in the **active** requirement slot's `candidates`, **plus** the commit area | candidate → record the pick for the active slot; commit area → open the decision plaque at slot 1 |
| Has `prompts` (option / select_from_zone / order) | commit area only | open the decision plaque; drag never answers a prompt slot |

Notes that follow from the hard rules:

- The commit area is **one region** so the client never has to decide whether a
  spell is a permanent. The server decides where the object lands; final
  placement is canonical layout, never free placement (**D10**).
- Candidates of a *non-active* slot are inert. Drag cannot skip or invent a
  required choice.
- A zone pile reacts only when the server named it — i.e. when a candidate id
  in the active slot resolves to that pile's contents. Piles are not
  drop-reactive by virtue of being piles.
- Drop regions are recomputed from each fresh `GameView`. A newer view during
  a drag ends the drag (§6.3, stage 9).

**When a region lights, stated once** (issue #586's third defect was that it was
not): *a region highlight belongs to the drag gesture and marks the commit area;
everything picked by name lights on the card.* Three consequences, and they cover
every state the shell can be in:

| State | Region lit? | Cards lit? |
| --- | --- | --- |
| Land drop / permanent cast — a drag whose action carries no requirements | **yes**, the receiver's band | — |
| A drag whose action carries requirements | **no** | the active slot's candidates |
| Targeting or a combat declaration — no drag in progress | **no** | the active slot's candidates |

The rule is "the region lights exactly when dropping on it commits". Lighting a
band that a drop would not answer is the inconsistency the maintainer saw between
a Main Phase 1 land drag (full-band highlight) and a Declare Attackers
declaration (none): the same amount of chrome for two states that are not the
same gesture. Note the second row is narrower than the §6.1 table's third
column: the commit area is *derivable* there, but no shipped path routes a drop
on it into the decision surface, so it is not drawn. Drawing an inert region is
the defect; wiring that route is a separate change.

Treatment is §6.2 stages 3–5 and §11 — a lit surface with a falloff and corner
ticks, never a flat uniform stroke. Gold is the selection and actionable-edge
accent; a drop region wears `--rune-drop-*` and nothing else.

### 6.2 Drag lifecycle

Visual treatments are transcribed from zones panels 5 and 6.

| # | Stage | Trigger | Treatment | Duration | Reduced motion |
| --- | --- | --- | --- | --- | --- |
| 0 | Idle | — | actionable cards wear the gold edge bar (visual-system §7) | — | — |
| 1 | Armed | pointer down on an entity with ≥ 1 offered action | card lifts to elevation 1 | 80–150 ms | lift, no tween |
| 2 | Dragging | pointer moved ≥ **8 px** (pointer) / ≥ **10 px** (touch) — **D11** | card at elevation 2, tilt ≤ 6° toward travel; 3 px `--rune-drag-glow` rim + 10 px bloom; follows the pointer | 100–150 ms | glow appears instantly, no tilt |
| 2a | Origin slot | on entering stage 2 | the hand slot is **held open**: 3 px dashed `--rune-origin-dash` (8/6 dash), radius 12 px, dimmed fill, centered upward arrow glyph | — | static |
| 3 | Regions lit | on entering stage 2 | every derived drop region paints: `--rune-drop-valid-fill`, 2 px `--rune-drop-valid` stroke, radius 12 px, L-shaped corner ticks (14 px arms, 6 px inset) | ≤150 ms | appear |
| 4 | Hover valid | pointer inside a region | stroke → 3 px, fill → 24 % opacity, ticks extend 4 px, a tether draws source → region center | draw ≤150 ms | full path, static dash |
| 5 | Hover invalid | pointer over any **named** region that is not a drop region | that region shows `--rune-drop-invalid-fill` (a **dim**, not a red wash), 2 px `--rune-drop-invalid` stroke, and the centered 52 ⌀ no-entry glyph | ≤150 ms | appears |
| 6 | Drop, valid | pointer up inside a region | region flashes once; per §6.1 either submit (→ stage 7) or open the decision plaque at the next slot | ≤200 ms | flash omitted |
| 7 | Pending server | submission sent | the card holds in the corridor at elevation 2, 70 % opacity; the primary enters pending-server (§3.2); all `valid_actions` interactivity is inert | ≤5 s ceiling | hairline static |
| 8 | Server accept | next `GameView` without `action_rejected` | the scene reconciles per the motion grammar ("Play land / permanent", "Cast") | 300–400 ms | appears in row |
| 9 | Server reject | next `GameView` with `action_rejected: true` | the card is already in hand (the client never moved game state); ≤3 px shake ×2 + non-blaming toast | ≤200 ms | toast only |
| 10 | Drop, invalid or outside | pointer up anywhere else | eased arc snap-back to the origin slot; **nothing is sent** | 250–350 ms | card reappears in slot |
| 11 | Interrupt | newer view, `Escape`, right-click, blur, or pointer cancel | drag ends, origin restored, nothing sent | ≤150 ms | instant |

**Shipped state (issue #569).** Stages 2 and 2a are implemented as specified in
substance and differ in two stated numbers. The proxy is the **real card face** —
`card/dom/CardFace` at the `hand` tier, elevation `held`, `aria-hidden` and
`pointer-events: none` so it is invisible to assistive technology and to the
`elementFromPoint` walk that resolves the drop — and the origin slot is held open
by keeping the fan's button in the tree and marking it `data-vacated`, so a
cancelled drag restores nothing because nothing was removed. The two differences:
the shipped arming threshold is **6 px** for both pointer and touch rather than
D11's 8 px / 10 px, and the tilt is a fixed −4° rather than one that follows
travel. Both are the pre-#569 behaviour, deliberately preserved; changing them is
a separate decision, not a side effect of drawing the card properly.

The **pending-server** lock (stage 7) is local, non-load-bearing presentation:
it is released by any inbound view or after 5 s, whichever is first, and a
fresh mount never reproduces it (**D13**). The protocol carries no per-submission
acknowledgement (**GAP-5**), so the lock can never be authoritative.

### 6.3 What drag may never do

- Never compute legality, cost, or effect; never light a region the server did
  not name.
- Never skip a required choice: an action carrying `prompts`, or more than one
  requirement slot, always routes through the decision plaque.
- Never place a card freely: the drop's only output is an `action_id` (+ picks).
- Never be the only path to any action (§7).
- Never depend on hover: touch reaches every region through §7's tap model.
- Auto-scroll / auto-pan during drag is out of scope for v1; candidates pierce
  every rung ([`layout-model.md`](layout-model.md)) so a candidate is always
  rendered and pickable in place without travel.

---

## 7. Input parity

Every drag interaction has a click, a keyboard, and a touch path. This table is
normative and complete: an interaction missing from a column is a defect.

| Interaction | Pointer — drag | Pointer — click | Keyboard | Touch |
| --- | --- | --- | --- | --- |
| Make an entity current | press + move ≥ 8 px | click the entity | arrows to focus, `Enter`/`Space` | tap |
| Fire the sole offered action | drop on the commit area | click primary, or click the selected entity again (ADR 0025) | `Enter` on the selected entity, or `Enter` on the primary | tap the selected entity again, or tap the primary |
| Choose among several actions | drop on the commit area, then plaque | click one echo button | arrows to the echo, `Enter` | tap one echo button |
| Pass priority | — | click primary | `P`, or `Enter` on the primary | tap primary |
| Pick a target | drop on a candidate | click the candidate | arrows traverse **only** the active slot's candidates, `Enter` picks | tap the candidate |
| Toggle a multi-select candidate | — (drag does not toggle) | click to toggle | `Enter` toggles | tap toggles |
| Advance to the next slot | — | click "Next" on the plaque | `Enter` on "Next" | tap "Next" |
| Confirm a declaration | — | click CONFIRM | `Enter` on CONFIRM | tap CONFIRM |
| Reorder an `order` prompt | — (v1: no drag reorder) | click ↑ / ↓ per row | arrows to the row control, `Enter` | tap ↑ / ↓ |
| Cancel the current step | release outside any region | click CANCEL | `Escape` | tap CANCEL |
| Tap a land for mana | — | select, then click the echo or the land again (ADR 0025, #463) | `Enter` twice on the land | tap twice |
| Declare attackers/blockers entry | — | click a combat candidate (ADR 0025 rule 1) | `Enter` on the candidate | tap the candidate |
| Inspect | — | hover dwell peek; right-click pins | `I` on the focused entity | long-press ≥ 500 ms peeks; tap the pin control |
| Set a stop | — | chevron → step toggle | `Enter` on the chevron, arrows to the step, `Enter` | tap chevron → tap toggle |
| Open the game menu | — | click the icon button | `Enter` on the icon button | tap icon button |

### 7.1 Keyboard model

Rides the shipped spatial focus engine (`table/focus.ts`) — focus moves
*between regions* and *within a region's items*; `Tab` stays native and
untrapped.

| Key | Verb | Behaviour |
| --- | --- | --- |
| ↑ ↓ ← → | move focus | `nextFocus()`; along-axis walks a region, cross-axis jumps regions |
| `Enter` / `Space` | select / confirm | the single universal activation (ADR 0025: same event for first and second activation) |
| `Escape` | back / cancel | one level per press (§8) |
| `P` | pass | fires `pass_priority` when offered (shipped binding) |
| `I` | inspect | pins the focused entity's panel |
| `?` | help | shortcut reference |

**During targeting or multi-select the focus ring is filtered**: arrows
traverse only the active slot's `candidates` plus the decision plaque's
controls. ADR 0025's direct-activation vocabulary is suspended, exactly as the
ADR states — the only interaction is picking candidates. Focus is always
visible (§3.2), and the focused candidate is announced with its slot progress
("Target 1 of 2: target creature").

### 7.2 Touch model

- **Tap** = select. **Tap again** on the selected entity = fire its sole action
  (ADR 0025 rule 3). Mana abilities require this deliberate two-tap path (#463).
- **Long-press** (≥ 500 ms with < 10 px movement) = inspect peek. It is never
  drag initiation (**D12**), so the two gestures cannot collide: movement
  ≥ 10 px before 500 ms starts a drag; stillness at 500 ms starts a peek.
- **Drag** requires the 10 px threshold, so an accidental tap can never play a
  card.
- Long-press and hover peeks are suppressed mid-pick (carried, #321); pinning
  stays reachable.
- No multi-finger or edge gestures are defined. Every touch target ≥ 44 px.

---

## 8. Cancel semantics

**The guarantee: a player can always back out of a partially-built interaction
without sending anything.** Nothing reaches the wire until the interaction is
complete, so every cancel below is purely local.

| Stage | What cancel means | Control | `Escape` | Sends |
| --- | --- | --- | --- | --- |
| Nothing selected | no-op | — | no-op | — |
| Entity selected | clear the selection, restore the neutral cluster | "Cancel selection" in the echo | yes | nothing |
| Drag in flight | snap back to the origin slot | release outside any region | yes | nothing |
| Targeting, slot 1 unanswered | abandon the action | plaque CANCEL | yes | nothing |
| Targeting, slot *n* answered | retract the **last pick** and return to slot *n* (this is what the baseline's UNDO pill does when it renders) | UNDO pill | `Escape` abandons the whole session; UNDO retracts one step | nothing |
| Multi-select building | abandon the whole declaration, restoring the neutral state | plaque CANCEL | yes | nothing |
| Multi-select the view **forces** (mulligan, cleanup discard) | **no cancel is offered** — there is no neutral state to return to (shipped behaviour, #451) | — | no-op | nothing |
| Option picker open | close the picker, keep any slot picks | plaque CANCEL | yes | nothing |
| Submitted, pending server | **not cancellable** — the message is gone | — | no-op | — |
| Concede confirmation open | dismiss the confirmation | CANCEL (danger pair) | yes | nothing |

Additional rules:

- A newer authoritative view **interrupts** any open selection, drag, targeting,
  or multi-select immediately and silently. The client re-derives everything
  from the new view; no in-progress state survives a message.
- Right-click and the browser's context menu are suppressed over interactive
  entities and mapped to cancel-one-level during a drag or pick.
- Cancel restores the *current authoritative view*, never a remembered earlier
  one.
- **There is no post-submission undo.** The UNDO pill is only ever the
  retract-one-local-step control above. It does not render in the neutral state
  the baseline draws it in — an unavailable action does not render (issue #543's
  own rule). A true takeback is **GAP-1**.

---

## 9. Interaction storyboards

Each storyboard is the canonical sequence; the drag path is the enhancement and
the click path is stated where it differs. Every step names the server shape it
rides.

**1 — Play a land.** Hand card carries `{type:"play_land", subject:[card]}` with
no requirements. (1) Select or pick up the card → gold bar, elevation 2.
(2) Commit area lights (own band ∪ stack rail). (3) Drop, or press the primary
labelled from `action.label`. (4) `ChooseAction{id, token}`. (5) Pending, then
the "Play land / permanent" motion on the next view.

**2 — Play a permanent.** Identical shape (`cast_spell`, no requirements). The
card travels to the stack first and then to its server-determined row on the
following view; the client stages both from the views it receives and never
predicts the row.

**3 — Cast an instant/sorcery with no targets.** Identical to 2. The only
difference is where the server puts the object.

**4 — Cast a targeted instant/sorcery.** Action carries one `requirements`
slot. (1) Pick up. (2) Both the commit area and the slot's `candidates` light.
(3a) Drop on a candidate → the pick is recorded; the slot list is complete and
there are no `prompts`, so `ChooseAction{id, token, targets:[{slot, chosen}]}`
submits (the single-target shortcut). (3b) Drop on the commit area → the
decision plaque opens at slot 1, prompt strip reads
"Choose target: <requirement.prompt>". (4) `Escape` at any point abandons; the
card returns to hand and nothing is sent.

**5 — Activate a targeted ability.** The permanent carries
`{type:"activate_ability", subject:[perm]}` with requirements. Select the
permanent first (ADR 0025: a permanent is not draggable-to-target until it is
the current selection). Then dragging from the selected source to a lit
candidate follows storyboard 4 step 3a. A permanent with several offered
actions never fires on drag: the drop opens the echo so the player chooses
which action, then targeting begins.

**6 — Multi-target selection.** Action carries ≥ 2 requirement slots. Drag can
answer at most slot 1; the plaque then walks the remaining slots. The prompt
strip shows "Target *n* of *N*"; CONFIRM is absent (targeting auto-submits on
the last pick) and CANCEL abandons. Only the **active** slot's candidates are
pickable at any moment.

**7 — Declare attackers.** `{type:"declare_attackers"}` with the `attackers`
slot, and — in multiplayer — one `defend_<id>` slot per attacker candidate.
(1) Activating any eligible creature enters the declaration with that creature
toggled (ADR 0025 rule 1); or press the subject-less action from the cluster.
(2) The decision plaque appears titled from `action.label` ("CHOOSE ATTACKERS"
in the baseline) with green CONFIRM and red CANCEL. (3) Toggle further
candidates. (4) Each declared attacker's `defend_<id>` slot is walked (its
candidates are defending players, picked on their crests). (5) CONFIRM submits
one atomic `targets[]`; the empty subset legally declares none. Drag plays no
part — toggling is not a drag gesture.

**8 — Declare blockers.** `{type:"declare_blockers"}` with one requirement slot
per attacker. Same plaque; the prompt names the attacker per slot; "Next"
advances. CONFIRM submits every in-play slot atomically.

**9 — Choose a zone/card from a browser.** `select_from_zone` prompt with
`zone`, `count`, `candidates`. If the zone is on the board (hand), candidates
highlight in place and are tapped there. If it is not (graveyard, library,
exile), the decision sheet opens the row list. CONFIRM is disabled until
exactly `count` ids are chosen — the one server-stated disablement (§3.2).

**10 — Invalid drop.** Pointer over a named region the server did not offer →
that region shows the dim + red stroke + no-entry glyph (zones panel 6). It
never accepts. Release there = storyboard "cancel mid-drag": eased snap-back,
nothing sent, no toast (nothing was refused — nothing was attempted).

**11 — Cancel mid-interaction.** At every stage of §8, one `Escape` (or the
plaque's CANCEL) restores the authoritative view. The origin slot closes, the
card returns, and `valid_actions` interactivity is fully restored on the same
frame.

**12 — Server rejection after an optimistic pickup.** The client never moves
game state optimistically, so "optimistic" here means only that the card was
lifted and the pending lock was taken. On `action_rejected: true`: release the
lock, shake ≤ 3 px ×2, show the non-blaming toast ("the game moved on"), and
render the re-sent view — which already carries the true `valid_actions`. The
tone is informational: a `valid_actions`-driven client can only be rejected by
a stale-view race, never by user error (protocol §`action_rejected`).

---

## 10. Prompt and decision placement

> **Superseded in part by issue #567 (see C11).** §10.1's anchoring walk and its
> split of "the strip carries the sentence, the plaque carries the controls" no
> longer describe the shipped client. There is now **one** decision surface — the
> **decision area** (`table/decision/DecisionArea.tsx`) — at the head of the
> lower-right action column, carrying the question, the numeric control of a
> `number` slot (issue #554), *and* the controls, and
> `PromptStrip`, `DecisionSheet`, and the anchoring module are removed. §10.1's
> constraint below survives and is what the new placement satisfies by
> construction; the algorithm that used to satisfy it does not. §10.2's phone
> bottom-sheet row survives as the area's compact form.

The decision plaque (control-ui panel 7) is a titled surface with the action's
label as its title and its controls beneath. It **must not occlude the subject
of the decision or any candidate**.

### 10.1 Anchoring algorithm (D17)

Inputs: the subject's rect, the active slot's candidate rects, and the layout
regions — all already known to the layout function. Placement is deterministic
for a given view + viewport (`ui-requirements.md` §Performance and determinism).

1. Prefer **below** the subject if the subject sits in the top half of the
   board; **above** if it sits in the bottom half.
2. Reject any position whose rect intersects the subject rect or any candidate
   rect.
3. If rejected, slide along the perpendicular axis in 16 px steps until clear.
4. If no clear position exists, **dock at the cluster slot** (lower right),
   which the layout model guarantees is outside the board and outside the
   center corridor.
5. Always clamp to the viewport with a ≥ 16 px gutter. Elevation
   `--rune-elevation-2`.

**There is no opposite-side retry, deliberately** (maintainer ruling). A
plaque whose preferred side is blocked slides, and if sliding never clears it
docks — it does not flip above/below first. The player then learns exactly two
positions for a decision, its subject's preferred side and the cluster, rather
than three. This costs proximity to the subject on a crowded board, which is
the accepted trade.

The prompt **strip** (the words: question, progress, count, deadline) keeps its
shipped fixed home on the hand panel's top edge and never moves. The plaque
carries the controls; the strip carries the sentence. Together they satisfy
`ui-requirements.md` §Prompt system.

### 10.2 Placement per geometry

| Geometry | Subject location | Plaque placement |
| --- | --- | --- |
| 2 seats, desktop | receiver band | above the subject, in the center corridor |
| 2 seats, desktop | far side | below the subject, in the corridor |
| 3–4 seats, desktop | far side or receiver band | corridor, per §10.1 |
| 3–4 seats, desktop | a wing board | **never inside a wing slot** — dock at the cluster |
| 5–6 seats, desktop | any | **always dock at the cluster**; the corridor is dense with combat paths and the wings are digested |
| Tablet landscape ≥ 1180 px | any | as desktop |
| Tablet portrait / phone portrait | any | **bottom sheet** above the hand fan, full width, ≤ 40 % viewport height; the board re-stages so the subject stays visible above it; the receiver's band is never covered |

At every geometry the plaque is dismissible, viewport-clamped, and pointer-
transparent where it hosts no control (shipped `data-pointer-through` behaviour,
#451) so candidates underneath stay tappable.

---

## 11. Non-color status cues

Required by `AGENTS.md` and `visual-system.md` §7. Every control state in §3.2
already carries one. The interaction-specific additions:

| State | Color | Non-color channel |
| --- | --- | --- |
| Drop region valid | `--rune-drop-valid` | **corner ticks** (unique shape) + translucent fill |
| Drop region invalid | `--rune-drop-invalid` | **no-entry glyph** + dimming (fill goes *darker*, not redder) |
| Drag origin | `--rune-origin-dash` | **dashed** outline + upward arrow glyph |
| Dragged card | `--rune-drag-glow` | elevation 2 + tilt |
| Hovered destination | — | stroke weight 2 → 3 px + **drawn tether** |
| Primary vs secondary | blue vs slate | **height** (56 vs 36) and **shape** (stadium vs chamfer) |
| Confirm vs cancel | blue enamel vs red | **order** (confirm always leading) + labels |
| Eligible vs chosen candidate | one accent | eligible = **thin dashed** ring, chosen = **solid doubled** ring + **check mark** |
| Current step pip | `--rune-pip-on` | disc **plus ring** |
| Pending-server | — | sweeping **hairline** + `aria-busy` |
| Deadline warning | `--rune-targeting` | **countdown chip** with digits |

## 12. Reduced motion

Every animated affordance has an equivalent, listed in-line in §3.2 and §6.2.
Summary of the drag-specific forms under `prefers-reduced-motion: reduce`:

| Animated affordance | Reduced-motion equivalent |
| --- | --- |
| Card lift / tilt on pick-up | card jumps to elevation 2, no tilt, no tween |
| Drag glow bloom | static rim, no pulse |
| Region light-up | regions appear at full treatment instantly |
| Tether draw | full path drawn at once, static dash |
| Snap-back arc | card reappears in the origin slot |
| Drop flash | omitted |
| Pending hairline sweep | static hairline |
| Rejection shake | omitted; toast only |
| Chevron rotation | glyph swap |
| Plaque enter/exit | appears/disappears |

No reduced-motion path changes what is sent or what is reachable.

---

## 13. Interaction state → server shape mapping

The normative mapping. Every interactive state must appear here with a concrete
`valid_actions` / prompt / view shape, or be listed as a GAP.

| Interaction state | Driven by | Client answer |
| --- | --- | --- |
| Entity is actionable (gold bar) | any `ValidAction` whose `subject` contains the entity id | — |
| Entity selection + contextual echo | that entity's subject-actions (ADR 0004) | — |
| Primary "PASS PRIORITY" | `ValidAction{type:"pass_priority", subject:[]}` | `ChooseAction{id, token}` |
| Primary "CAST SPELL" / "PLAY LAND" | selected entity's single action, `type:"cast_spell"` / `"play_land"` | `ChooseAction{id, token}` |
| Primary "TAP FOR MANA" | `ValidAction{mana_ability:true}` on the selected land | `ChooseAction{id, token}` |
| Commit-area drop region | offered action with no `requirements`/`prompts`, `type ∈ {play_land, cast_spell}` | `ChooseAction{id, token}` |
| Candidate drop region / target ring | active `requirements[i].candidates` | pick recorded locally |
| Targeting progress "Target n of N" | `requirements.length`, `requirements[i].prompt` | — |
| Targeting submit | all slots filled | `ChooseAction{..., targets:[{slot, chosen}]}` |
| Declare attackers plaque | `ValidAction{type:"declare_attackers"}`, slot `attackers` | `targets:[{slot:"attackers", chosen:[…]}]` |
| Per-attacker defender pick | requirement slot `defend_<permId>` | `targets:[{slot:"defend_…", chosen:[player]}]` |
| Declare blockers plaque | `ValidAction{type:"declare_blockers"}`, one slot per attacker | `targets:[…]` per in-play slot |
| Combat damage order | `order` prompt on `order_combat_damage` | `targets:[{slot, chosen:[permutation]}]` |
| Zone-browser pick | `select_from_zone` prompt (`zone`, `count`, `candidates`) | `targets:[{slot, chosen}]` |
| Option picker (keep / mulligan) | `option` prompt with `options[{id,label,requires}]` | `targets:[{slot, chosen:[optionId]}]` |
| Option disabled with a reason | `PromptOption.requires` not yet satisfied | — |
| CONFIRM enabled | every in-play slot's cardinality satisfied (server-stated `count`) | — |
| RESPOND secondary | `view.stack.length > 0` **and** the primary is `pass_priority` | **nothing** (navigation only) |
| Plaque title line | `view.phase` | — |
| Plaque ownership line | `view.active_player`, `view.priority_player`, `view.you`, `view.player_names` | — |
| Step pips | `view.phase` → `PHASE_GROUPS` | — |
| Chevron → step list | `PHASES` (static) | — |
| Per-step stop toggle (tri-state) | `view.stops`, `view.own_turn_stops` | `SetStops{stops:[Phase…], own_turn:[Phase…]}` |
| Auto-passed badge | `view.auto_passed` | — |
| Per-step "passed for you here" mark (counted) | `view.auto_passed_steps` entries whose `turn` is `view.turn` | — |
| Auto-passed badge's spoken path | all of `view.auto_passed_steps`, grouped into per-turn runs | — |
| Deadline chip / warning frame | `view.action_deadline` | — |
| Rejection shake + toast | `view.action_rejected` | — |
| "Waiting" / compact cluster | `valid_actions` empty | — |
| Concede (behind menu, confirmed) | `ValidAction{type:"concede"}` | `ChooseAction{id, token}` |
| Game-over verdict, no controls | `view.result` present, `valid_actions` empty | — |

### 13.1 Gaps — interactions with no server representation

These are **protocol issues**. No client-side logic may paper over any of them;
until each has a server shape, the affordance does not render.

| Gap | Interaction the baselines or the issue imply | Why it has no representation | Disposition | Tracked by |
| --- | --- | --- | --- | --- |
| **GAP-1** | **UNDO in the neutral state** (control-ui panel 6 draws the pill with nothing selected) | No `undo`/takeback exists in `valid_actions`, and there is no client→server message for one. `ChooseAction` is final. | The pill renders **only** as the local retract-one-step control (§8). A real takeback needs an engine + protocol decision. | #554 (recorded there as noted, not proposed) |
| **GAP-2** | **Contextual primary label "RESOLVE"** (zones panel 10) | `ValidAction.label` for `pass_priority` is fixed server-side; deciding that a pass resolves the stack top is a rules judgment the client may not make. | Filed as a protocol/server issue: label `pass_priority` contextually. Client renders `label` verbatim meanwhile. | #554 |
| **GAP-3** | **"Advance / skip to my next stop"** (a forward chevron reads as an advance) | No `pass_until`, `advance_phase`, or hold-priority action exists; ADR 0020 defers auto-yield/hold to M6. | Chevron is a disclosure only (D4). Do not wire it to a game action. | #554 |
| **GAP-4** | **"Disabled controls explain why"** (issue #543) | Only `PromptOption.requires` carries a server-stated reason. There is no general `ValidAction` unavailability reason, because unavailable actions are simply absent. | Only the `requires` case renders disabled. Everything else does not render. | #554 (recorded there as noted, not proposed) |
| **GAP-5** | **Pending-server / in-flight acknowledgement** | `ChooseAction` has no correlation id and the server sends no ack — only the next full view. A client cannot tell "my action landed" from "someone else's broadcast". | The pending lock is local, ≤ 5 s, non-load-bearing (D13). A real in-flight state needs a protocol ack. | #554 |
| **GAP-6** | **Numeric value prompts (X, divided damage, pile splits)** | The prompt kinds are `option`, `select_from_zone`, `order` only. | No control is designed. Adding one is a protocol change (`ui-requirements.md` §Prompt system). | #554 |
| **GAP-7** | **Alternative cost / mana payment choice** | No prompt kind carries a cost choice; ADR 0025 leaves server-computed payment plans open on the roadmap. | Mana stays the deliberate select-then-activate path per land. | #554 |
| **GAP-8** | **Hold priority / full control / auto-yield toggles** | ADR 0020 defers these to M6; only `set_stops` exists. | Only per-step stops render. | not filed — ADR 0020 owns the deferral |
| **GAP-9** | **Takeback, draw offer, simultaneous multiplayer decisions** | No server or protocol support (`ui-requirements.md` §Session and game lifecycle). | Out of scope; no control. | #554 for takeback and draw offer (noted, not proposed); simultaneous decisions not filed |

---

## 14. Decisions this document makes (the baselines were silent)

| # | Decision |
| --- | --- |
| D1 | Scale anchor: 1 baseline px = 1 CSS px, pinned by the 44 px icon button; control sizes are fixed CSS values, not viewport fractions. |
| D2 | Plate heights are kept as drawn (36 px) but hit boxes are padded to 44 px. The drawn 31–35 px controls are below the touch floor. |
| D3 | Step pips render the five shipped `PHASE_GROUPS` (passed / current+ring / upcoming), not the 4 and 3 pips the two panels happen to draw. |
| D4 | The plaque's forward chevron is a **disclosure** for the twelve-step list and its `set_stops` toggles — never a game action. |
| D5 | The circular rune icon button is the **game menu handle** (settings, display, shortcuts, log, concede-with-confirmation). This is what makes the compact cluster (panel 6b = plaque + icon) coherent: it keeps the two things that are always available. |
| D6 | `RESPOND` is a navigation control that sends nothing; it renders when the primary is `pass_priority` and `view.stack` is non-empty. |
| D7 | The zones baseline's stack-adjacent pair and the control-ui cluster are the same surface. The cluster does not move; its primary switches from the stadium pill to the compact pair while the stack is non-empty. |
| D8 | The eight-rule primary derivation of §4.2, including the two "empty primary on a tie" rules (4 and 7) — the client never picks a "best" action. |
| D9 | `concede` is never eligible for the primary slot. |
| D10 | Drop-region model: one **commit area** (own band ∪ stack rail) for requirement-less actions, plus the **active** slot's candidate rects. This avoids any permanent-vs-spell classification in the client. |
| D11 | Drag thresholds: 8 px pointer, 10 px touch. |
| D12 | Long-press (≥ 500 ms, < 10 px) is inspect, never drag initiation; the thresholds make the two gestures mutually exclusive. |
| D13 | Pending-server is a local, ≤ 5 s, non-load-bearing lock released by any inbound view. |
| D14 | Disabled controls render only for the one server-stated reason (`PromptOption.requires`); everything else is absent rather than greyed. |
| D15 | Focus ring: 2 px `--rune-selection`, 2 px offset, drawn outside the frame, never suppressed (the baselines show no keyboard state). |
| D16 | Invalid-drop feedback dims rather than reddens the fill; the no-entry glyph carries the meaning, so the state survives a color-blind or Lite quality path. |
| D17 | Decision-plaque anchoring precedence (below/above the subject → slide → dock at the cluster), and the phone bottom-sheet form. **Superseded by #567 (C11): the decision area has a fixed home at the head of the action column, so there is no anchoring; the bottom-sheet form survives as its compact form.** |
| D18 | Concede lives behind the menu with a two-step confirmation and the danger treatment, never adjacent to the ordinary primary. |
| D19 | The cancel taxonomy of §8, including "no cancel is offered for a view-forced decision" and "no post-submission undo". |
| D20 | Drop-validity green/red are a **distinct semantic pair** from the gain/loss hue families of `visual-system.md` §2; they are disambiguated by shape (corner ticks vs no-entry glyph) and by never co-occurring with a gain/loss moment on the same object. See §15 C3. |
| D21 | Type mapping of the drawn small-caps display face onto the existing scale, adding exactly one token (`--rune-type-action`, 24 px). |
| D22 | `order` prompts are reordered with ↑/↓ row controls in v1; no drag-to-reorder (parity cost outweighs the polish, and the shipped surface already works). |

---

## 15. Conflicts and open questions

Recorded, not resolved by editing other documents. Each needs a maintainer
decision or a follow-up PR to the named authority.

**C1 — Blue primary vs. gold "you can act".** `visual-system.md` §2 assigns
gold `#F2C94C` to *every currently offered interaction*. Both baselines draw the
cluster's primary in blue enamel. Proposed resolution: gold remains the
**entity-level** offered affordance (the card's bottom edge bar); blue is
reserved for the **single cluster primary**, which is a control, not an entity.
`visual-system.md` §2 would gain that sentence.

**C2 — Targeting is drawn blue, not orange. Resolved by maintainer ruling:
blue selection, orange targeting.** Zones panels 7/8 draw the
selected/targeted creature with a blue ring and a blue tether; `visual-system.md`
§7 assigns orange `#E0784A` to target candidate / chosen target and blue
`#7FB2E5` to selection. The visual system wins; the baselines' blue tether is
the **rejected alternative**, read as the *source selection's* accent. This spec
already kept the visual system (orange for candidates and the chosen-target
path, blue for selection and the drag rim) and needs no change. `visual-system.md`
§7 stands as written.

Consequence: the drag rim (`--rune-drag-glow` `#2485F0`) and the origin-slot
dash (`--rune-origin-dash` `#5E90C5`) sit in the **same blue family as
selection** (`--rune-selection` `#7FB2E5`), and a dragged card is normally also
the selected one. They are separated by shape and placement, never by hue — the
selection ring is a stroke on the card's own outline plus elevation 2; the drag
rim is the 3 px rim + 10 px bloom of a card that tilts toward travel and follows
the pointer (§6.2 stage 2); the origin dash is a dashed outline with an upward
arrow glyph left in the *vacated slot* (§6.2 stage 2a). No third hue is introduced.

**C3 — Drop green/red vs. the gain/loss hue families.** `visual-system.md` §2
owns green for "gain moment" and red for "loss moment". The zones baseline uses
both for drop validity. D20 separates them by shape and by non-co-occurrence,
but the hue families table should be amended to name the drop pair explicitly.

**C4 — Drawn control heights below the 44 px floor.** UNDO (35), CONFIRM/CANCEL
(31), RESOLVE/RESPOND (33) are all under the `AGENTS.md` / `ui-requirements.md`
44 px minimum. D2 pads the hit box. If the maintainer prefers the plates
themselves to grow, the cluster's vertical rhythm changes and panel 7's
proportions shift.

**C5 — Where the one action home lives. Resolved by
[ADR 0032](../decisions/0032-contextual-shell-anatomy.md).** ADR 0023 and
[`ui-blueprint.md`](ui-blueprint.md) put the action dock **beside the hand**;
`ActionDock.tsx` implements that. Both baselines put the primary, the utility
pill, and the plaque in a **lower-right control cluster**. This spec follows the
later, approved baselines: the *commitment* ("one action home") is preserved,
its *location* moves.

ADR 0032 supersedes ADR 0023 and performs that move, so this is no longer an
open conflict. It went further than the location, because the conflict was
wider than this section could see: ADR 0023's anatomy is *permanent* regions,
and every contextual surface this document specifies contradicts it. ADR 0032
removes the permanent top bar, bottom dock, and right rail, and replaces
"regions never overlap by construction" with a tested layer contract — *a layer
may only be covered by a layer the player explicitly invoked and can dismiss
without answering it.* The refactor of `ActionDock.tsx` is #534's.

**C6 — The phase indicator's position.** `PhaseIndicator.tsx` renders
top-center; the baseline plaque sits at the foot of the control cluster. If the
plaque is adopted, the top-center indicator is retired or demoted to a
turn/priority marker. Both baselines show only the cluster plaque.

**C7 — The utility rail.** The 2.5D baseline shows a five-icon row (`?`, `»`,
`▶`, `☰`, `⚙`) below the plaque that the control-ui sheet does not include. Its
composition is outside this spec. Note the potential duplication between that
row's `☰` and D5's menu icon button — one of them should go.

**C8 — UNDO drawn in the neutral state.** Panel 6 draws the pill with nothing
selected, where GAP-1 says nothing can be undone. This spec does not render it
there. If the maintainer wants the pill always present, it must be disabled with
a server-known reason — which does not exist (GAP-4).

**C9 — Three paths to the same action.** ADR 0025's second-activation, the
cluster primary, and now drag all fire the same single offered action. That is
intentional redundancy for input parity, but it means an implementation must
route all three through one code path so they cannot diverge.

**C11 — One decision surface, or a sentence and a plaque. Resolved by issue
#567 (maintainer-authored).** §10.1 gave a decision two surfaces — the strip at a
fixed home for the sentence, the plaque next to the subject for the controls —
and §10.1/D17 specified the anchoring walk that kept the plaque off its subject.
Shipped, that produced three surfaces, not two: `PromptStrip` drew the sentence,
`DecisionSheet` drew it again above its option buttons, and `DecisionPlaque` drew
a third copy of the title — control-less for a forced mulligan, whose confirm was
absent (named options answer it) and whose cancel was absent (§8/D19). The
anchoring never ran on real inputs either: the shell had no subject, candidate, or
wing rects to give it, so every landscape ≥ 1180 px call docked at the cluster and
every other call took the sheet form with no receiver band.

#567 asks for "one coherent lower-right action area for authoritative decisions,
phase/priority controls, and the player's current mana" and "one 2.5D-native
choice surface **adjacent to the primary action**". That is the resolution: the
**decision area** carries the question, the progress, the count, the deadline, the
rows, the **numeric control**, the named choices, and the controls, at the head of
the control cluster's column. `PromptStrip` and `DecisionSheet` are deleted; the
anchoring module is deleted with them, because a surface with a fixed home has
nothing to anchor.

The numeric control is `NumberPromptSurface`, the answer to a server-posed `number`
slot (issue #554 — the value of X, a count of counters, one share of a divided
effect). It landed in `DecisionSheet` while that surface still existed, and moved
here with the rest of the question rather than being deleted with its host: a
`number` slot is the one slot kind with **no candidates at all**, so neither the
board nor a row list can answer it and it must bring its own control. It is
submitted by the area's own Confirm, like every other slot — the sheet had a
second one — and the slot opens pre-filled at the server's minimum, so confirming
without touching it is a legal answer. Every bound is the server's.
§10's own constraint — never occlude the subject or a candidate — is now met by
geometry rather than by search: the area stands on the cluster's published height
on the full composition and on the hand band's top edge on the compact one, so the
cards a mulligan is asking about are never under it.

Two things §10 decided that this does **not** change: a decision the view forces
still offers no cancel (§8/D19), and mulligan bottoming is still picked on the
cards rather than listed in the surface.

**C10 — Pip semantics were guessed.** D3 picks the five phase groups. If the
baseline's pips were meant as "seats yet to pass priority" or "steps within the
current phase", the plaque's second line and pip row need redesign — and the
former would be a new derived value the client may not compute.

---

## 16. Hand-offs

- **#533 / #534 / #535 / #499** implement against this document. Implementation
  must not invent direct-manipulation behaviour per component (issue #543's
  closure gate).
- **Protocol issue** filed from §13.1: **#554** (action and prompt contract),
  covering GAP-2 (contextual `pass_priority` label), GAP-3 (advance/skip),
  GAP-5 (submission ack), GAP-6 (numeric prompts), and GAP-7 (cost choice) as
  proposals, and recording GAP-1/GAP-9 (takeback, draw offer) and GAP-4
  (general unavailability reason) as noted-not-proposed. GAP-8 stays with
  ADR 0020's M6 deferral.
- **ADR 0032** ([contextual shell anatomy](../decisions/0032-contextual-shell-anatomy.md))
  supersedes ADR 0023 and settles C5. #534 performs the dock's move and the
  removal of the permanent top bar, bottom dock, and right rail.
- **Browser verification** of real pointer/touch thresholds, hit-box overlap at
  the tablet floor, and the drag snap-back arc belongs to the maintainer;
  automated coverage for this spec stops at jsdom/Vitest (derivation tables,
  parity paths, placement geometry, state mapping).
