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
  compositions) is issue #470's deliverable; this document defines what
  things look like and how they move, not where regions sit.

## 1. Style pillars

1. **Illustrated, not textured.** Broad shapes, clean silhouettes, subtle
   gradients, controlled detail. No photorealism, no dense ornament, no
   engraved panel ware, no heavy gloss. An environment reads in half a
   second and then recedes.
2. **Slightly cartoon-like, professionally finished.** Confident rounded
   forms, slightly exaggerated tactility (cards lift a little higher, land a
   little softer than physical cards would), disciplined saturation. Never
   childish: no outlines-and-goo, no squash-and-stretch on game objects.
3. **Cards are the content.** The environment sits darker and
   lower-contrast than the play surface; chrome stays quiet; effects are
   brief and purposeful. If a screenshot's most saturated pixels aren't
   cards or a live decision, the register is wrong.
4. **Depth is staged, not modeled.** Perspective, layering, shadow, focus,
   and motion carry all depth (ADR 0030's scene plane). No modeled 3D
   geometry, no simulated physics.
5. **An original identity.** The rune glyph language, the hexagonal life
   crest, jewel-tone seat accents, and disciplined gold grow into the new
   look. Reference games (e.g. MTG Arena) set the quality bar only — never
   composition, components, or assets.

## 2. Color system

Foundation neutrals (the dark table world, carried and layered):

| Role | Value | Notes |
| --- | --- | --- |
| Ink (deepest chrome, badges) | `#0D0F13` | |
| Environment base | per theme (§4) | always darker than the plane |
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

Gold stays disciplined: it marks **every currently offered interaction**
(`valid_actions` may offer several at once — all of them carry the
treatment) and the priority holder, and is never decorative. Selection keeps
a hue family of its own because it co-occurs with targeting on screen.

**Seat identity accents** — six muted jewel tones (extending the shipped
four) assigned deterministically by seat order: `#4D7EC9` azure, `#B0563F`
ember, `#4F8F5C` moss, `#8B6FB0` amethyst, `#C08B3E` amber, `#4E9A9B` teal.
They appear on region bounds, nameplates, crest rings, and combat/target
references to a player — **never on cards** (frame color is game
information).

## 3. Light, shadow, elevation

One implied key light, high and slightly toward the viewer, so every shadow
falls gently down-screen. Consistency is the rule; physical accuracy is not.

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

Three parallax groups, all behind the play surface: **sky** (gradient +
large soft shapes), **far ground** (silhouetted landforms), **arena edge**
(the surface's surround). Ambient motion lives in sky and far ground only,
slow and small, per quality level (Standard reduced, Lite static). The
environment never overlaps a game object and stays at least one contrast
step below the plane.

Launch theme concepts (concept level here; production specs are #471's):

- **Runic Vale** (default) — indigo sky, slate arena, cool teal glow.
- **Ember Reach** — deep umber sky, basalt arena, warm ember accents.
- **Pale Court** — blue-gray dawn, weathered marble arena, faint gilt.

Every theme must pass the same check: cards, accents, and text hit their
contrast budgets against it with no per-theme retuning.

## 5. Card presentation

The face vocabulary carries the shipped **information budget** per tier
(chip → mini → support → field → hand → inspect) and the **art window**
model: procedural monogram fill by default, illustration when a source
provides one (ADR 0024 unchanged). What changes is presence, not content:

- **On the battlefield** cards lie on the plane, foreshortened by the
  camera, and interact through the elevation ladder. Tap is the carried
  ~25° rotation + dim, footprint pre-reserved, one treatment at every tier.
- **In hand** cards stand in screen space in a curved fan, largest tier;
  hovering/focusing straightens and raises the card above its neighbors.
- **On the stack** an object is a screen-space mini card seated in a
  stack-rail slot; the **slot wrapper** — not the card face — wears the
  controller's seat accent as an edge stripe, so "who controls this entry"
  reads at a glance while the never-on-cards rule of §2 holds without
  exception.
- **Inspect** is a fixed-size screen-space panel at every geometry
  (budget rule: inspection never depends on battlefield card size).
- ×N stacks render as a slightly splayed physical pile (2–3 px offsets) with
  the count badge — "four Plains" should look like a stack of Plains, not a
  card wearing arithmetic.

## 6. Player identity on the battlefield

Each seat presents as a **crest cluster** at its region's edge, integrated
into the scene (not a dashboard row): the crest (portrait art or monogram in
a seat-accent ring), the hexagonal **life crest** (carried; display-face
numerals), hand and library counts as compact pips, and — in commander games
— the commander badge with its tax counter. States:

- **Priority**: gold crest glow plus a slow breathing pulse (reduced motion:
  static double gold ring). Position and the phase pill corroborate.
- **Active turn**: a fixed turn marker on the crest cluster, distinct in
  shape from priority.
- **Under attack**: targeting-orange ring + incoming paths terminate at the
  crest; an `Attacked ×N` chip counts attackers.
- **Eliminated**: cluster desaturates, crest turns to a rune-marked stone;
  region stays (public zones remain browsable per requirements).
- **Disconnected**: a broken-link glyph over the crest; no color reliance.

## 7. Non-color state channels

The binding table (budgets: no state is color-only at any quality level):

| State | Color | Non-color channel |
| --- | --- | --- |
| Ownership | seat accent | region position + nameplate + bounds |
| Actionable | gold | bottom **edge bar** (unique shape) |
| Selection | blue | **ring** + elevation level 2 |
| Target candidate | orange | **ring** + steady beacon pulse (RM: static ring) |
| Chosen target | orange | ring + **drawn path** terminating on it |
| Priority | gold | crest glow shape + phase pill text |
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
| Exile | card lifts and fades through a violet rune iris | 300–400 ms | vanishes + pile tick |
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
| Phase / step change | phase pill advances with a short wipe; skipped phases compress into one wipe showing the path taken | ≤500 ms total | pill updates |
| Turn rotation | brief staging beat: table dims 10%, new active crest rises/flashes, turn marker travels | ≤500 ms | markers update |
| Focus / camera change | regions re-stage with eased scene-geometry tween | 300–500 ms | new staging |
| Off-focus activity | a quiet rune ping at the acting player's crest + log entry; never silent | ≤300 ms | static ping badge ≥1 s |

### Session moments

| Motion | Choreography | Duration | RM form |
| --- | --- | --- | --- |
| Game start | environment fades up, regions assemble outward from center, libraries settle, opening hands deal with budgeted stagger | ≤800 ms total window, skippable | scene appears |
| Mulligan | hand sweeps back to library, redraw deals; the composed sweep+deal is *skippable* | within travel budgets | hands swap |
| Reconnect / fast-forward | latest view renders complete, then a single "you are here" pulse on the phase pill and active crest | rebuild per budget, pulse ≤300 ms | no pulse |
| Concede / defeat | player's region plays the eliminated treatment; for the local player, a quiet full-screen dim into the verdict panel | ≤600 ms | verdict panel |
| Victory | gold rune bloom behind the verdict panel — celebratory, not gaudy | ≤800 ms, skippable | verdict panel |
| Return to lobby | scene recedes (scale down + dim) into the lobby surface | ≤400 ms | cut |

## 9. Sound and haptic hooks

Concept-level only (production is separate work; delivery via #471's
pipeline): motion classes above define the **event taxonomy** —
draw/play/tap/cast/resolve/impact/destroy/priority/phase/victory — and every
hook is optional, independently muted, and never load-bearing for
comprehension (the visual + log channels stand alone).

## 10. Carried vs redesigned

**Carried from the shipped system**: the information budget and its tiers;
the glyph language (grown as needed); combat indicator shapes; frame
accents; the tokens discipline (ADR 0019); the OFL display face for identity
moments; select-then-confirm interaction and the one-action-home commitment
(ADR 0023); every legality/accessibility constraint.

**Redesigned**: the flat carved-panel surface (→ staged scene, §3–4);
dashboard-style player rows (→ crest clusters, §6); the ornament-rejection
stance (→ illustrated register, §1); static state presentation (→ the
motion grammar, §8); the effects ceiling (→ budgeted effects on the WebGL
layer, ADR 0030).

## 11. Hand-offs

- **#470 (layouts)**: stages this system at 2–6 players, mobile, and stress
  cases; owns region geometry and the compact compositions.
- **#471 (assets)**: production specs, formats, and licensing for
  environments, crest art, and effect sprites inside the budget ceilings;
  the effect taxonomy of §8–9 is its input.
- **Phase 1 implementation**: turns §2–3 values into tokens, builds the
  fixture-driven battlefield to this document, and validates §8 against the
  budget caps in CI-runnable form.
