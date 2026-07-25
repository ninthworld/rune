# The RUNE visual system — 2.5D look and motion grammar

**The design authority for the redesigned client's look, feel, and motion**
(issue #469, under [ADR 0029](../decisions/0029-2-5d-presentation-direction.md)
and [ADR 0030](../decisions/0030-2-5d-presentation-architecture.md), master
issue #464). Anchored on the approved baseline
([`../ui-concepts/rune-2.5d-interface-baseline.jpg`](../ui-concepts/rune-2.5d-interface-baseline.jpg))
for tone, depth, and quality — not pixels.

Relationships to the other authorities:

- [`ui-requirements.md`](ui-requirements.md) stays binding — this system styles
  capabilities, never removes them.
- [`presentation-budgets.md`](presentation-budgets.md) caps everything here:
  every duration below fits its motion class, every treatment respects the
  quality levels, and nothing degrades the scene itself.
- [`ui-design-notes.md`](ui-design-notes.md) remains the shipped client's
  record. Carried forward from it, unchanged in meaning: the card-face
  **information budget**, the **glyph language**, the **combat indicator
  shapes**, the WUBRG **frame accents**, the token-split discipline
  (ADR 0019), and the legal constraints. Superseded by this document: its
  design stance (flat surfaces, ornament rejection as direction) and the
  carved-panel look.
- Layout geometry (region sizes, seat arrangements per player count, mobile
  compositions) belongs to [`layout-model.md`](layout-model.md); this document
  defines what things look like and how they move, not where regions sit.
- The **detail authorities** under this system, each normative in its own
  scope: [`card-representation.md`](card-representation.md) (frames, bands,
  card states), [`seat-identity.md`](seat-identity.md) (the crest cluster),
  [`zone-geography.md`](zone-geography.md) (the zone rack),
  [`stack-and-relationships.md`](stack-and-relationships.md) (the stack stage
  and path grammar), [`environment-system.md`](environment-system.md) (the
  environment layers and themes), and
  [`control-language.md`](control-language.md) (controls, drag, cancel) under
  [ADR 0032](../decisions/0032-contextual-shell-anatomy.md). Where one of them
  refines a row below, it refines it; it never contradicts this document's
  hues, motion classes, or non-color channels.

## 1. Style pillars

1. **Illustrated, not textured.** Broad shapes, clean silhouettes, subtle
   gradients, controlled detail. No photorealism, no dense ornament, no
   engraved panel ware, no heavy gloss. An environment reads in half a
   second and then recedes.
2. **Slightly cartoon-like, professionally finished.** Confident rounded
   forms, slightly exaggerated tactility (cards lift a little higher, land a
   little softer than physical cards would), disciplined saturation. Never
   childish: no outlines-and-goo, no squash-and-stretch on game objects.
3. **Cards are the content.** The environment sits at least one contrast step
   away from, and lower in chroma than, the play surface — the launch themes
   read as a mid-light desaturated plaza under dark-framed cards. Chrome stays
   quiet; effects are brief and purposeful. If a screenshot's most saturated
   pixels aren't cards or a live decision, the register is wrong.
4. **Depth is staged, not modeled.** Perspective, layering, shadow, focus,
   and motion carry all depth (ADR 0030's scene plane). No modeled 3D
   geometry, no simulated physics.
5. **An original identity.** The rune glyph language, the circular life
   medallion, jewel-tone seat accents, and disciplined gold grow into the new
   look. Reference games (e.g. MTG Arena) set the quality bar only — never
   composition, components, or assets.

## 2. Color system

Foundation neutrals (the dark table world, carried and layered):

| Role | Value | Notes |
| --- | --- | --- |
| Ink (deepest chrome, badges) | `#0D0F13` | |
| Environment base | per theme (§4) | one contrast step away from the plane, and lower in chroma |
| Play surface | `#1B212D` → `#151A24` radial | the plane's felt |
| Raised surface / card body | `#23262B` | |
| Line work | `rgba(232,230,225,.06–.14)` | region bounds, dividers |
| Primary text | `#E8E6E1` | |

**Frame accents** (a card's color identity — carried verbatim from the
shipped tokens, tuned for this dark world): W `#CFC7AC`, U `#4E86C1`,
B `#77688C`, R `#C05B4D`, G `#57935F`, multicolor `#C9A84C`, colorless
`#8C949C`, land `#A08A6E`. Frame accents belong to the card and never encode
ownership.

**Interaction accents** — organized as **semantic hue families**: each hue
family owns one meaning-group, and distinct states *within* a family are
separated by the shape channels of §7, never by hue alone:

| Hue family | Value | States in the family | Shape channels (see §7) |
| --- | --- | --- | --- |
| Gold — "you can act" | `#F2C94C` | offered interactions; the priority holder | bottom edge bar (cards); crest glow (priority) |
| Blue — "your attention" | `#7FB2E5` | selection | ring |
| Orange — "threat / intent" | `#E0784A` | targeting; attack and block relationships | ring + drawn path; top/left edge bars |
| Red — "loss moment" | `#D9574A` | damage, destruction | impact flash + badge |
| Green — "gain moment" | `#6FAF78` | healing, growth | soft rise pulse + delta chip |
| Green/red — drop validity, a **distinct pair** | `#8FC49A` valid, `#DA866C` invalid | drag drop regions only | corner ticks vs the no-entry glyph over a dim (never a red wash) |

Gold stays disciplined: it marks **every currently offered interaction**
(`valid_actions` may offer several at once — all of them carry the
treatment) and the priority holder, and is never decorative. Selection keeps
a hue family of its own because it co-occurs with targeting on screen.

Gold is the **entity-level** affordance: it lives on the object itself. The
**single cluster primary** — the one control that advances the current
decision — is a control and not an entity, and wears the blue enamel of
[`control-language.md`](control-language.md) §2.1 (`--rune-primary-face-*`),
which is neither gold nor the selection blue.

The drop-validity pair is **not** the gain/loss families: it belongs to the
drag lifecycle alone, is separated from them by shape, and never co-occurs
with a gain or loss moment on the same object.

**Seat identity accents** — six muted jewel tones assigned deterministically
by seat order: `#4D7EC9` azure, `#B0563F` ember, `#4F8F5C` moss,
`#8B6FB0` amethyst, `#C08B3E` amber, `#4E9A9B` teal.
They appear on region bounds, nameplates, crest rings, and combat/target
references to a player — **never on cards** (frame color is game
information).

## 3. Light, shadow, elevation

One implied key light, high and slightly toward the viewer, from the upper
left, so every shadow falls gently down-screen and a little to the right.
Consistency is the rule; physical accuracy is not.

Elevation ladder (transform + shadow move together; values are the Phase 1
token seed):

| Level | Use | Treatment |
| --- | --- | --- |
| 0 — resting | permanents on the plane | contact shadow, tight and dark |
| 1 — lifted | hover / keyboard focus | rise toward camera (~24 px), shadow softens and spreads |
| 2 — held | selected, dragged, being cast | highest lift + slight tilt toward pointer/travel direction, widest shadow |
| Screen space | hand fan, stack, inspect, overlays | drop shadows against the scene, no plane transform |

**Focus dims, never blurs**: focusing a player or object drops non-relevant
regions to ~60% brightness and slight desaturation. Blur is banned (cost,
legibility, motion-sickness).

## 4. Environment treatment

Four parallax layers, all behind the play surface: **L0 far surround** (sky
glow, water, distant foliage), **L1 arena floor** (the plaza field, paving
rings, the central rune medallion), **L2 arena edge** (rim, verge, the raised
lips), **L3 props** (lanterns, plinths, foliage — corner- and edge-anchored
only). Ambient motion lives in L0 and L3 only, slow and small, per quality
level (Standard drops to L0 at half amplitude, Lite off). Illustrated
incident is confined to the seat flanks and the four corner pockets: the
central 80% of width at full height stays quiet, so the same environment
serves every seat count. The environment never overlaps a game object and
stays at least one contrast step away from the plane, and lower in chroma.

Launch theme family (production specs are in `environment-system.md`):

- **Runic Vale** (default) — warm sand plaza, cool teal water, lantern gold.
- **Verdant Canals** — cooler canals, dense foliage, cyan accents.
- **Sunlit Observatory** — warm ochre and brass under pale gold light.
- **Moonlit Ruins** — cool blue-violet slate, broken architecture, cyan glow.

Every theme passes the same lockstep check, with no per-theme retuning: frame
accents and every hue family ≥ 3:1 against the plaza, primary text ≥ 4.5:1 on
every slot it can land on ([`environment-system.md`](environment-system.md)
§5.4). Cards stay the highest-contrast objects on screen — delivered by dark
card frames over a mid-light desaturated field, not by a dark backdrop.

## 5. Card presentation

The face vocabulary carries the shipped **information budget** per tier
(chip → mini → support → field → hand → inspect) and the **art window**
model: procedural monogram fill by default, plus two device-local sources a
player may select — the project's own bundled illustrations
([ADR 0031](../decisions/0031-bundled-asset-policy.md)) and the opt-in
third-party art of [ADR 0024](../decisions/0024-user-side-card-art.md)
(unchanged). Frame, band, and state detail is
[`card-representation.md`](card-representation.md)'s. What changes here is
presence, not content:

- **On the battlefield** cards lie on the plane, foreshortened by the
  camera, and interact through the elevation ladder. Tap is the carried
  ~25° rotation + dim, footprint pre-reserved, one treatment at every tier.
- **In hand** cards stand in screen space in a curved fan, largest tier;
  hovering/focusing straightens and raises the card above its neighbors.
- **On the stack** an object is a screen-space entry seated in a stack-rail
  slot, expanded to a full card face at the top of the pile and stepping down
  to mini and row tiers with depth; the **slot wrapper** — not the card face —
  wears the controller's seat accent as an edge stripe and the order index, so
  "who controls this entry" reads at a glance while the never-on-cards rule of
  §2 holds without exception.
- **Inspect** is a fixed-size screen-space panel at every geometry
  (budget rule: inspection never depends on battlefield card size).
- ×N stacks render as a slightly splayed physical pile — up to three edges
  behind the top card, stepping down-and-left — with the `×N` tab on the top
  edge; "four Plains" should look like a stack of Plains, not a card wearing
  arithmetic.

## 6. Player identity on the battlefield

Each seat presents as a **crest cluster** at its region's edge, integrated
into the scene (not a dashboard row): the portrait medallion (a bundled
portrait plate in a rimmed aperture, no substitute glyph when it fails), the
circular **life medallion** seated on the portrait's bottom rim (display-face
numerals), and the **hand count** as a compact hexagonal pip. Every other
count — library, graveyard, exile, command — lives on its own pile, and the
commander badge and tax counter live on the command slot
([`zone-geography.md`](zone-geography.md) §4). States:

- **Priority**: gold crest glow plus a slow breathing pulse (reduced motion:
  static double gold ring). Position and the phase plaque corroborate.
- **Active turn**: a fixed turn marker on the crest cluster, distinct in
  shape from priority.
- **Under attack**: targeting-orange ring + incoming paths terminate at the
  crest; an `Attacked ×N` chip counts attackers.
- **Eliminated**: cluster desaturates, crest turns to a rune-marked stone;
  region stays (public zones remain browsable per requirements).
- **Disconnected**: a broken-link glyph over the crest; no color reliance.
  Specified, and dormant until the protocol carries per-seat connection state.

## 7. Non-color state channels

The binding table (budgets: no state is color-only at any quality level):

| State | Color | Non-color channel |
| --- | --- | --- |
| Ownership | seat accent | region position + nameplate + bounds |
| Actionable | gold | bottom **edge bar** (unique shape) |
| Selection | blue | **ring** + elevation level 2 |
| Target candidate | orange | **ring** + steady beacon pulse (RM: static ring) |
| Chosen target | orange | ring + **drawn path** terminating on it |
| Priority | gold | crest glow shape + phase plaque text |
| Tap | — | **rotation** + dim |
| Attacking | orange spectrum | top edge bar + outgoing path + tilt toward defender |
| Blocking | — | left edge bar + doubled-stroke link (carried shape) |
| Damage marked | red | numeric badge (carried) |
| Latent ability | — | marker dot (carried, distinct from gold bar) |
| Illegal / rejected | — | horizontal shake ≤3 px + non-blaming toast |

## 8. Motion grammar

Principles, then the vocabulary. Every duration fits its
[budget class](presentation-budgets.md#animation-budgets), and two distinct
contracts apply to every row:

- **Interruptibility (always, no exceptions):** a newer authoritative view
  retargets or discards any in-flight motion, fast-forward collapses
  everything to the latest state, and no motion ever gates input.
- **Skippability (per class, the default):** motions that complete in
  ≤ 600 ms are **not individually user-skippable** — they are shorter than a
  deliberate skip and remain interruptible as above. Every composition that
  may exceed 600 ms end-to-end is **user-skippable** (input or setting) and
  is explicitly marked *skippable* in its row. No unmarked row may compose
  past 600 ms.

Each row defines a reduced-motion (RM) form — the default is "snap to end
state" unless stated. Motion states **causality**: source → effect →
consequence, in order, so a player who missed the log still reads what
happened.

### Object motions

| Motion | Choreography | Duration | RM form |
| --- | --- | --- | --- |
| Draw | card rises from library pile, arcs to its fan slot, neighbors reflow | 250–350 ms | appears in slot |
| Play land / permanent | card lifts from fan, arcs onto its row, row closes around it, soft contact settle | 300–400 ms | appears in row |
| Cast (goes to stack) | card lifts, shrinks toward the stack rail, stack entry slides in | 300–400 ms | entry appears |
| Resolve | stack entry expands toward its destination while the effect plays — expansion and effect **overlap**, ≤600 ms combined; a multi-part resolution that must compose longer is *skippable* | ≤600 ms total | state applies, badge blink |
| Ability / trigger to stack | a rune chip rises from the source permanent to a stack-rail slot (the synthetic entry), source pulses once | 200–300 ms | entry appears + source badge |
| Countered / fizzle | stack entry crumples (scale + rotate ~5°) and falls out | 250–350 ms | entry vanishes + log emphasis |
| To graveyard (destroy/sacrifice/discard) | card tips flat, slides to the pile, pile count ticks | 300–400 ms | pile count ticks |
| Exile | a violet rune iris opens **at the exile pile**, the card travels into it, the topmost glass pane flashes as it closes | 300–400 ms | pile count ticks, top updates |
| Reveal / look-at | card flips up in place or to a screen-space strip | 200–300 ms | shown immediately |
| Token creation | token scales up from its source with a brief rune circle | 200–300 ms each; a batch uses the budget stagger window and is *skippable* | tokens appear |
| Zone migration (type change moves rows) | eased slide between rows (carried behavior) | 250–350 ms | repositions |

### Feedback motions

| Motion | Choreography | Duration | RM form |
| --- | --- | --- | --- |
| Hover / focus lift | elevation 1 | 80–150 ms | elevation without tween |
| Select | elevation 2 + ring draw-on | 100–150 ms | ring appears |
| Tap / untap | rotation tween ±25° | 150–250 ms | rotates instantly |
| Targeting path | path draws from source to pointer/candidate, dash crawl while pending | draw ≤150 ms | full path, static dash |
| Illegal attempt | ≤3 px horizontal shake, 2 cycles | ≤200 ms | toast only |
| Counters / P/T change | badge pop (scale 1→1.2→1) + delta chip floating up | 200–300 ms | badge updates |
| Healing / growth | green rise pulse (§2 gain family) on the object or life crest + floating delta chip | 200–300 ms | badge/crest updates |

### Combat

| Motion | Choreography | Duration | RM form |
| --- | --- | --- | --- |
| Declare attacker | card tilts ~6° toward its defender, top edge bar ignites, path draws to the defender's crest/permanent | 200–300 ms | indicators appear |
| Declare blocker | blocker steps toward its attacker's lane edge, doubled-stroke link draws | 200–300 ms | indicators appear |
| Combat damage | attacker lunges 8–12 px along its path, impact flash at the defender, damage badges pop; simultaneous exchanges batch per budget stagger | lunge 150 ms, total ≤600 ms | badges + flash frame |
| Lethal / destruction | the graveyard travel, preceded by a ≤150 ms crack flash | within travel budget | pile tick |

### Flow and staging

| Motion | Choreography | Duration | RM form |
| --- | --- | --- | --- |
| Priority passes | gold glow crossfades between crests | 150–250 ms | marker moves |
| Phase / step change | the phase plaque advances with a short wipe; skipped phases compress into one wipe showing the path taken | ≤500 ms total | plaque updates |
| Turn rotation | brief staging beat: table dims 10%, new active crest rises/flashes, turn marker travels | ≤500 ms | markers update |
| Focus / camera change | regions re-stage with eased scene-geometry tween | 300–500 ms | new staging |
| Off-focus activity | a quiet rune ping at the acting player's crest + log entry; never silent | ≤300 ms | static ping badge ≥1 s |

### Session moments

| Motion | Choreography | Duration | RM form |
| --- | --- | --- | --- |
| Game start | environment fades up, regions assemble outward from center, libraries settle, opening hands deal with budgeted stagger | ≤800 ms total window, skippable | scene appears |
| Mulligan | hand sweeps back to library, redraw deals; the composed sweep+deal is *skippable* | within travel budgets | hands swap |
| Reconnect / fast-forward | latest view renders complete, then a single "you are here" pulse on the phase plaque and active crest | rebuild per budget, pulse ≤300 ms | no pulse |
| Concede / defeat | player's region plays the eliminated treatment; for the local player, a quiet full-screen dim into the verdict panel | ≤600 ms | verdict panel |
| Victory | gold rune bloom behind the verdict panel — celebratory, not gaudy | ≤800 ms, skippable | verdict panel |
| Return to lobby | scene recedes (scale down + dim) into the lobby surface | ≤400 ms | cut |

**Implemented (issue #509).** Every row above is shipped on the live and
spectator tables. The budgets, skippability marks, and hue families are data in
`clients/web/src/sceneTokens.ts` (`SCENE_SESSION`, `SCENE_SKIP_THRESHOLD_MS`),
named and classified by `clients/web/src/table/live/sessionMoments.ts`, clocked
by `useSessionMoments`, and rendered as CSS on the shell's `data-moment` /
`data-forced-decision` flags plus the verdict panel's own staging. The mulligan
and verdict halves that belong to the view delta (`mulligan`, `hand_kept`,
`game_over`) emit real presentation intents from `deriveGameViewPresentation` —
they were deliberate state-first stubs before. Two carried notes: the mulligan's
bottomed cards travel as one aggregate hand → library intent because the wire
never names which cards were bottomed (nothing is guessed), and a receiver-less
(spectator) view is told who won without wearing anyone's victory or defeat.

## 9. Sound and haptic hooks

Motion classes above define the **event taxonomy** —
draw/play/tap/cast/resolve/impact/destroy/priority/phase/victory — and every
hook is optional, independently muted, and never load-bearing for
comprehension (the visual + log channels stand alone).

**Status (issue #507): the hook layer is implemented and silent.**
`clients/web/src/table/audio/` maps this taxonomy off the same presentation
intents the scene animates, with master/per-category mute and volume plus an
opt-in Vibration API channel in the display settings. Reduced motion does **not**
silence audio — they are independent channels — and batch events collapse to one
sound per batch window, mirroring the visual stagger budget. Sound assets are
still separate work under ADR 0031; nothing is bundled today, so every category
resolves to silence. The category→intent mapping lives in
[`asset-pipeline.md`](asset-pipeline.md) §The sound and haptic hook layer.

## 10. Carried vs redesigned

**Carried from the shipped system**: the information budget and its tiers;
the glyph language (grown as needed); combat indicator shapes; frame
accents; the tokens discipline (ADR 0019); the OFL display face for identity
moments; select-then-confirm interaction and the one-action-home commitment
([ADR 0032](../decisions/0032-contextual-shell-anatomy.md), whose single
action home is the lower-right control cluster); every
legality/accessibility constraint.

**Redesigned**: the flat carved-panel surface (→ staged scene, §3–4);
dashboard-style player rows (→ crest clusters, §6); the permanent top bar,
bottom dock, and right rail (→ contextual chrome held apart by a tested layer
contract, ADR 0032); the ornament-rejection stance (→ illustrated register,
§1); static state presentation (→ the motion grammar, §8); the effects
ceiling (→ budgeted effects on the WebGL layer, ADR 0030).

## 11. Hand-offs

- **Layouts** — [`layout-model.md`](layout-model.md) stages this system at 2–6
  players, mobile, and stress cases; it owns region geometry and the compact
  compositions.
- **Assets** — [ADR 0031](../decisions/0031-bundled-asset-policy.md) governs
  provenance, licensing, and delivery; [`asset-pipeline.md`](asset-pipeline.md)
  and [`environment-system.md`](environment-system.md) §9 hold the formats and
  byte ceilings. The effect taxonomy of §8–9 is their input.
- **Tokens** — §2–3's values live in `clients/web/src/sceneTokens.ts` under a
  lockstep contrast test, and no CSS module may carry any of them as a literal.
- **Implementation** — the detail authorities listed at the head of this
  document carry the per-surface specifications; this document stays the
  arbiter of hue meaning, motion classes, and non-color channels.
