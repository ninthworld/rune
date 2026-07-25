# The RUNE environment system — layers, focal-safe geometry, theme family

Design authority for the battlefield **environment** (ADR 0030 layer 1) under
issue #542, consumed by #530 (Runic Vale implementation), by the environment/layout
boundary of #531/#536, and by the raster-plate request in #548.

This document **codifies the approved baselines**. It does not invent a look. Where
the images decide, the numbers below are transcribed from them; where the images are
silent, the choice is marked **[DECISION]** and is normative until amended here.

Binding sources, in precedence order:

| Rank | Source | What it fixes |
| --- | --- | --- |
| 1 | [`../ui-concepts/rune-battlefield-environments.jpg`](../ui-concepts/rune-battlefield-environments.jpg) | seat-count composition (panels 1–5), theme family (panels 6–8), the ambient reservation |
| 2 | [`../ui-concepts/rune-2.5d-interface-baseline.jpg`](../ui-concepts/rune-2.5d-interface-baseline.jpg) | canonical 4p Runic Vale at full fidelity; every sampled palette value |
| 3 | [`../ui-concepts/rune-zones-interaction.jpg`](../ui-concepts/rune-zones-interaction.jpg) | where zone racks and clusters land relative to the scenery |
| 4 | [ADR 0031](../decisions/0031-bundled-asset-policy.md), [`presentation-budgets.md`](presentation-budgets.md) | provenance, formats, byte ceilings |
| 5 | [`layout-model.md`](layout-model.md), `clients/web/src/table/plane/metrics.ts` | the slot rects the focal-safe geometry is derived from |
| 6 | [`visual-system.md`](visual-system.md) §4, [`asset-pipeline.md`](asset-pipeline.md) | style pillars, effect taxonomy |

**The one thing the environment reference proves:** panels 1–5 are the *same*
environment at 2, 3, 4, 5, and 6 players. No seat, plinth, or board footprint is
painted into the illustration. Every rule below exists to keep that literally true.

---

## 1. The layer contract

Four layers, back to front, matching #548's `L0`–`L3` request exactly.

| Layer | Contains | Parallax factor | Treatment | Dropped at |
| --- | --- | --- | --- | --- |
| **L0 — far surround** | water bodies, distant foliage, sky glow, horizon vignette | `0.15` | authored soft; internal local contrast ≤ **1.6:1**; mean luminance ≥ 1 step below L1 | **Lite** → replaced by the token gradient (no fetch) |
| **L1 — arena floor** | the plaza field, radial paving rings, the central rune medallion | `0.35` | authored soft; local contrast ≤ **1.25:1** inside the focal core (§2); no hard edge crosses the core | **never** — L1 is the theme's identity floor. Lite uses the half-resolution variant |
| **L2 — arena edge** | stone rim, grass/ground verge, the raised lips at top and bottom | `0.60` | local contrast ≤ **2.0:1**; permitted inside the focal core **only** as the top/bottom lips (§2.4) | **Lite** → off |
| **L3 — props** | lanterns, crystal plinths, flowers, foliage clumps, columns, theme instruments | `1.00` | local contrast ≤ **2.6:1**, and never above the card-frame contrast floor; **corner/edge anchored only** | **Lite** → off; **Standard** → static (no flicker) |

Z-order is fixed: `L0 → L1 → L2 → L3 → scene plane (ADR 0030 layer 2) → effects → chrome`.
No layer may be reordered, merged, or given a fifth sibling without amending this
document.

### 1.1 Parallax

There is no free camera (ADR 0030): parallax is driven **only** by the staging tween's
plane delta, never by pointer position, device orientation, or scroll.

- Layer offset = `factor × E`, with the factors in the table above.
- `E` is the environment's maximum excursion: **12 logical px** at desktop, **8 px**
  at tablet, **0 px** on phone portrait. **[DECISION]** — the images are static, so
  the magnitude is chosen to sit far below the 44 px hit floor and below one card's
  contact-shadow spread.
- Standard halves `E`; Lite sets `E = 0`; `prefers-reduced-motion` sets `E = 0`.
- Parallax animates on the `staging` motion class (`SCENE_MOTION.staging`, 400 ms).

### 1.2 Blur is banned at runtime

`visual-system.md` §3 forbids runtime blur (cost, legibility, motion sickness). The
"heaviest blur" of #548's L0 row is **authored softness baked into the plate**, not a
CSS/WebGL filter. No environment layer may carry `filter: blur()` at any tier.
Focus dim (`SCENE_FOCUS_DIM`) applies to scene regions, never to the environment.

---

## 2. Focal-safe geometry

### 2.1 Derivation

Overlay every slot rect the plane can occupy at 2, 3, 4, 5, and 6 seats
(`clients/web/src/table/plane/metrics.ts`, `PLANE`), clip to the canvas, and take the
union. All values are fractions of the **composed canvas** (the viewport the scene
plane fills), not of the source plate.

| Source rect | 2p | 3p | 4p | 5p | 6p | x | y |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `duelFar` | ● | | | | | 12–88 % | 2–36 % |
| `far` | | ● | ● | ● | ● | 20–80 % | 2–36 % |
| `corridor` | ● | ● | ● | ● | ● | 12–88 % | 36–67 % |
| `receiver` | ● | ● | ● | ● | ● | 12–88 % | 67–100 % |
| wing, single (`0.24 W`, bleed `0.28`) | | ● | ● | | | 0–17.3 % / 82.7–100 % | 12–52 % |
| wing, double (`0.21 W`) rank 0 | | | | ● | ● | 0–15.1 % / 84.9–100 % | 12–37 % |
| wing, double rank 1 | | | | ● | ● | 0–15.1 % / 84.9–100 % | 40–65 % |

Union, then a **2 % guard band** for hand-fan overhang and contact shadows:

```
central column   x 10–90 %,  y   0–100 %      occupied at EVERY seat count
seat flanks      x  0–10 %,  y  10– 67 %      occupied at 3–6 seats, revealed at 2
                 x 90–100 %, y  10– 67 %
```

The complement is four corner pockets. That is the entire budget for illustrated
incident — and it is exactly what the baseline shows: rock and foliage top-left, a
lantern and flowers bottom-left, crystal plinths pinched into the outer ~3 % of width
at mid height.

### 2.2 The normative zones

| Zone | Region (fractions of the composed canvas) | Rule |
| --- | --- | --- |
| **A — focal core** | `x ∈ [10 %, 90 %], y ∈ [0 %, 100 %]` | **No L2, no L3, ever** (one carve-out, §2.4). L1 only. Luminance amplitude ≤ **8 %** within any 10 % × 10 % window; no edge whose local contrast exceeds **1.25:1**. |
| **B — seat flanks** | `x ∈ [0 %,10 %] ∪ [90 %,100 %], y ∈ [10 %, 67 %]` | L2 permitted. L3 permitted only as **low-mass ground cover** (verge, water, small foliage) — never a tall silhouette, never a light source. Content here is covered at 3–6 seats and revealed at 2. |
| **C — prop pockets** | `x ∈ [0 %,10 %] ∪ [90 %,100 %]`, `y ∈ [0 %,10 %] ∪ [67 %,100 %]` — four rects | The full L3 vocabulary. 8.6 % of canvas area; this is where the theme's personality lives. |

**The answer in one line: the focal-safe rectangle is `x 10 %–90 %, y 0 %–100 %` —
the central 80 % of width at full height.**

### 2.3 The medallion sub-zone

The central rune medallion (L1) is centred at `(50 %, 40 %)` with a radius of
**5 % of canvas width**, transcribed from the baseline (`x 47–53 %, y 36–44 %`). It is
the only permitted L1 incident inside Zone A and is capped at **1.4:1** against the
plaza field — one contrast step *above* the plaza's own amplitude, no more. It sits in
the centre corridor, which by construction holds no card, and is the anchor the
tightest crop must preserve (§4.3).

### 2.4 The lip carve-out

The baseline carries its depth read on two broad horizontal L2 edges — the raised
stone lips behind the far side and behind the receiver. They cross Zone A, and they
must. The carve-out:

- L2 may enter Zone A **only** at `y ∈ [0 %, 8 %]` and `y ∈ [92 %, 100 %]`.
- Only as a broad horizontal band: no silhouette taller than **8 % of H**, no vertical
  element, no local contrast above **1.6:1**.
- L3 has **no** carve-out and never enters Zone A at any height.

### 2.5 What #530 must pin

These are pure-geometry assertions and belong in unit tests, not in a browser:

1. Every prop anchor in the theme manifest lies inside Zone C (or Zone B for
   `mass: 'low'` anchors).
2. Zone A ∩ (L2 anchors ∪ L3 anchors) = ∅ except anchors tagged `lip`.
3. For every seat count 2…6, `carveSlots()`'s union of rects is contained in
   Zone A ∪ Zone B.
4. The medallion centre and radius match §2.3 at every supported aspect.

---

## 3. Seat-count composition

### 3.1 Transcription from panels 1–5

Seat positions genuinely differ. The environment does not.

| Panel | Players | Opponent placement | Local | Environment change |
| --- | --- | --- | --- | --- |
| 1 | 2 | one, **top centre**, full width | bottom centre | none — flanks fully revealed |
| 2 | 3 | two, **top left and top right**; top centre left open | bottom centre | none — flanks partly covered |
| 3 | 4 (canonical) | three: top left, **top centre (focused, ringed)**, top right | bottom centre | none |
| 4 | 5 | 4p arrangement **plus a side rank** at mid height | bottom centre | none — Zone B mid-height covered |
| 5 | 6 | 4p arrangement **plus a full side rank pair** | bottom centre | none — Zone B fully covered |

Panels 4 and 5 draw **one and two more portraits than their captions state**; they are
concept renders, not seat manifests. Their binding content is the *pattern* — additional
seats accrete outward and downward along the side arcs, symmetrically, never by
re-composing the centre — not their literal counts. Panels 1–3 are exact.

### 3.2 The placement rule

**[DECISION]** The environment's composition rule, stated so art and layout cannot drift:

> Seats are placed on an **arc around a fixed plaza**. The plaza, its medallion, its
> lips, and its props never move, scale, or re-key with the seat count. Adding a seat
> only *covers more of Zone B*; removing one only *reveals more of Zone B*.

Consequences that bind the illustration:

- **No seat plinth, no board footprint, no six-way radial division may be painted into
  L1.** The paving pattern must be rotationally plausible at every count — concentric
  rings and a soft radial fan, never a fixed N-way split.
- The plaza's bright field must extend to the full Zone A ∪ Zone B envelope, so a
  2-player table does not show empty painted seats and a 6-player table does not run
  a board off the paved area onto grass.
- Wing bleed (`PLANE.wing.bleed = 0.28`) means a wing's outer 28 % is off-canvas.
  The art must therefore stay legible when its outer flank is *partly* covered, which
  is the case at every count from 3 up.

### 3.3 Boundary with the layout model

This document owns the environment; [`layout-model.md`](layout-model.md) owns seat
staging. Where they meet:

- Layout may move a seat anywhere inside Zone A ∪ Zone B without consulting the art.
- Layout may **not** place drawn content inside Zone C without amending §2.2.
- A layout change that widens the occupied union changes the focal-safe zones here,
  in the same PR.

---

## 4. Aspect handling

### 4.1 Authoring canvas

L0, L1, and L2 are authored as one continuous plate at **21:9** (`2.333`), with the
**16:9 safe crop marked**. L3 is *not* a plate (§4.4).

The 16:9 crop is centred and uses `1.778 / 2.333 = 76.2 %` of the source width:
source `x ∈ [11.9 %, 88.1 %]`, full height.

Ultrawide therefore **reveals** the outer 23.8 % of the plate. It never stretches.

### 4.2 Crop anchors

| Target | Aspect | Source width used | Horizontal anchor | Vertical anchor | Layers |
| --- | --- | --- | --- | --- | --- |
| Ultrawide | 21:9 (2.333) | 100 % | centre | full height | L0–L3 |
| Desktop | 16:9 (1.778) | 76.2 % | centre | full height | L0–L3 |
| Desktop | 16:10 (1.600) | 68.6 % | centre | full height | L0–L3 |
| Desktop | 3:2 (1.500) | 64.3 % | centre | full height | L0–L3 |
| Tablet landscape floor | 1180×820 (1.439) | 61.7 % | centre | full height | L0–L3 |
| Tablet | 4:3 (1.333) | **57.1 %** | centre | full height | L0–L3 |
| Phone portrait | 390×844 (0.462) | 100 %, cover-fit | centre | §4.5 | L0, L1, L2 re-anchored |

Horizontal anchor is always **centre**: the plaza's centre of mass is the source
centre, so every landscape crop is symmetric and no anchor table is needed per theme.

### 4.3 What must survive the tightest crop

The tightest landscape crop is 4:3, using source `x ∈ [21.4 %, 78.6 %]`. Everything
below must be **fully contained** in that rect:

| Must survive | Why |
| --- | --- |
| the whole plaza field (L1) edge-to-edge of the crop | otherwise a tablet shows unpaved ground under cards |
| the central rune medallion, complete | it is the theme's single readable identity mark |
| both raised lips (L2), spanning the crop width | they carry the entire depth read |
| the top and bottom paving rings | they frame the receiver and far bands |

Everything outside source `x ∈ [21.4 %, 78.6 %]` is **reveal-only surround**: water,
distant foliage, canal banks, ruin architecture. Nothing load-bearing may live there.

### 4.4 Props do not crop

**[DECISION]** L3 is delivered as **discrete corner/edge-anchored sprites with alpha**,
not painted into the wide plate.

If props were baked at fixed source coordinates, the 16:9 crop would discard every prop
outside source `x ∈ [11.9 %, 88.1 %]` — i.e. exactly the prop pockets — and 16:9 would
have no visible scenery at all. Anchoring solves it and matches #548's "corner-anchored
only" instruction:

- Each prop has a manifest entry: `{ key, anchor, offset, scale, mass, region }`.
- `anchor ∈ { top-left, top-right, bottom-left, bottom-right, left-mid, right-mid }`.
- `offset` is in fractions of the **composed canvas**, so a prop sits the same distance
  from the corner at 16:9 and at 21:9.
- `region` must be `C` (or `B` when `mass: 'low'`); validated by the §2.5 test.
- `left-mid` / `right-mid` anchors are limited to `mass: 'low'` and to `x < 4 %` /
  `x > 96 %` — the pinch the baseline's crystal plinths sit in.

Ultrawide therefore reveals more *surround*, and the props stay where the eye expects
them. This is the mechanism that makes one source set serve 16:9 and 21:9 without a
second composition.

### 4.5 Phone portrait

**[DECISION]** Portrait is a recomposition, not a crop — a 0.462 aspect would use 20 %
of the plate's width and show nothing recognisable.

- L1 is **cover-fit** to the canvas width at scale ≥ 1, anchored so the medallion
  centre sits at `(50 %, 40 %)` of the viewport — the same place it sits on desktop.
- L0 is the theme's token gradient extended above and below the plate; no fetch.
- L2 renders **only** its two lips, re-anchored to canvas top and bottom rather than
  to source coordinates.
- L3 is off. `E = 0`, so no parallax.
- The compact change-of-kind (layout rung 5) already collapses the layout; the
  environment matches by simplifying rather than by shrinking.

---

## 5. The theme family

### 5.1 Theme-invariant (this is what lets one layout serve all four)

| Invariant | Value |
| --- | --- |
| Layer count, names, z-order | L0–L3, §1 |
| Parallax factors and `E` ladder | §1.1 |
| Focal-safe zones A/B/C | §2.2 — byte-identical rects across themes |
| Plaza footprint | the L1 field fills Zone A ∪ Zone B at every crop |
| Medallion centre and radius | `(50 %, 40 %)`, `r = 5 % W` |
| Lip positions | `y ∈ [0 %, 8 %]` and `y ∈ [92 %, 100 %]` |
| Prop anchors | the six anchor names of §4.4; a theme may leave one empty, never add one |
| Light direction | one implied key, **high and slightly toward the viewer, from upper-left**; shadows fall down-screen and slightly right (`visual-system.md` §3) |
| Contrast ceilings | §1's per-layer caps; cards remain the highest-contrast objects |
| Authoring canvas and crop anchors | §4 |
| Ambient reservation | §6 |
| Manifest keys, load classes, byte budget | §9 |
| Quality/fallback behaviour | §8 |

### 5.2 Theme-variant

Hue of every palette slot; prop *identity*; water/ground material; the glow accent;
the character of ambient motion (within the §7.1 amplitude cap). Nothing else.

### 5.3 The four themes

Palette keys are **sampled from the approved images** (the baseline for Runic Vale,
panels 6–8 for the rest) and rounded. They are the values #530 must land in
`sceneTokens.ts`; no CSS module may carry any of them as a literal.

| | **Runic Vale** (canonical, default) | **Verdant Canals** | **Sunlit Observatory** | **Moonlit Ruins** |
| --- | --- | --- | --- | --- |
| Source | baseline, panel 3 | panel 6 | panel 7 | panel 8 |
| Key | warm sand plaza, cool teal water, warm lantern gold | same plaza, deeper foliage, bright cyan canals | warm ochre/terracotta, pale gold light, brass | cool blue-violet, grey slate, cyan rune glow |
| `plazaCore` | `#B4A379` | `#8A7F66` | `#907B5F` | `#52575E` |
| `plazaEdge` | `#A89B72` | `#A08A64` | `#A68861` | `#5B6069` |
| `paving` | `#B2AB7A` | `#96895F` | `#9B815F` | `#61666E` |
| `medallion` | `#9FA991` | `#8F9478` | `#B18E54` | `#6E7B8C` |
| `rim` | `#54534C` | `#6F6858` | `#836C57` | `#384457` |
| `verge` | `#585C4B` | `#535A49` | `#8A7356` | `#2F4251` |
| `water` | `#5F7674` | `#414432` | `#6F5A46` | `#243A59` |
| `surroundBase` | `#565D3C` | `#323723` | `#514638` | `#2C3238` |
| `propWarm` | `#9D7C58` | `#837451` | `#B18E54` | `#7A6A4E` |
| `propCool` | `#36ABBC` | `#3FC2E0` | `#8FA6B0` | `#3A6A9C` |
| `glow` | `#4E9A9B` | `#3FC2E0` | `#C9A45E` | `#5379A8` |
| Light | high, upper-left, warm neutral | same, cooler fill | same, warmer and brighter | same, cooler and dimmer |
| Prop vocabulary | stone lanterns on posts, teal and amethyst crystal plinths, flowering shrubs, moss, low walls | brass lantern, crystal plinths, dense foliage, canal stonework, reeds | armillary sphere, brass orrery, sundial, telescope mount, potted cypress | broken arches, fallen columns, one lantern, glowing rune veins, rubble |
| Surround material | streams and small waterfalls both sides | broad canals both sides | shallow reflecting pools, ochre terraces | luminous cyan streams, ruined arcades |

### 5.4 Token slots

The current `EnvironmentTheme` interface in `clients/web/src/sceneTokens.ts`
(`skyTop / skyHorizon / skyBase / ground / arena / glow`) predates these baselines and
cannot express a light plaza over a dark surround. #530 replaces it with:

```
label, surroundTop, surroundBase, water,
plazaCore, plazaEdge, paving, medallion,
rim, verge, propWarm, propCool, glow
```

`surroundTop` is the plate-free gradient stop above the surround; the remaining twelve
map 1:1 onto §5.3. The lockstep contrast test in `sceneTokens.test.ts` extends to the
new slots and must assert, for **every** theme with **no per-theme retuning**:

- card frame accents ≥ **3:1** against `plazaCore` (cards stay the highest-contrast
  objects);
- `SCENE_NEUTRALS.text` ≥ **4.5:1** against every slot it can land on;
- every hue family of `SCENE_HUES` ≥ **3:1** against `plazaCore` and `plazaEdge`;
- `plazaCore` luminance within `[0.28, 0.42]` for warm themes and `[0.12, 0.30]` for
  cool themes — the band the sampled values occupy.

**Conflict:** the shipped tokens name *Ember Reach* and *Pale Court*; the approved
images name *Verdant Canals*, *Sunlit Observatory*, and *Moonlit Ruins*. See §11.

---

## 6. `AMBIENT SPACE — FUTURE`

Panels 6–8 mark a dashed region at lower left in every theme. Measured against each
panel's inner frame it is consistently `x ≈ 1.5 %–22 %, y ≈ 69 %–97.5 %`, with the
upper-right corner chamfered.

**Normative reservation:** `x ∈ [0 %, 20 %], y ∈ [69 %, 97 %]` of the composed canvas,
with a chamfer taking the inboard edge to `x = 12 %` above `y = 80 %`.

### 6.1 What it is reserved for

**[DECISION]** — the images name the region but not its purpose. It is reserved as
**the one place the client may later grow a new non-load-bearing presentation surface**
without re-authoring any theme. Candidates: an ambient log or flavour ticker, a
spectator-presence indicator, a table-mood/weather affordance, a match timer. It is
**not** reserved for anything that carries rules information — that would violate the
one-view reconstruction invariant and the noninteractive contract of §7.

### 6.2 The rule that keeps it quiet

Binding in every theme:

1. **At most one L3 prop** may be anchored in the region, and it must be an
   independently addressable sprite with its own manifest key, so claiming the region
   hides exactly one thing. (Panels 6 and 8 each place a single lantern here; that is
   the intended density.)
2. No prop in the region exceeds **12 % of H** in silhouette height.
3. No local contrast above **1.8:1**, and no glow bloom crossing `x = 22 %`.
4. Mean luminance stays at or below `plazaCore`.
5. L1 paving and L2 verge may pass through, unchanged.

### 6.3 Occupancy is seat-count dependent

The region overlaps the left seat flank at 5–6 seats (wing rank 1 reaches `y = 65 %`)
and the receiver band's left margin. It is **compositionally** reserved always;
**occupancy** is not guaranteed. The subregion never contested by any seat at any count
is `x ∈ [0 %, 10 %], y ∈ [69 %, 100 %]` — the bottom-left prop pocket, which today also
carries the in-match `RUNE` wordmark. Any future occupant must be occlusion-tolerant
and must yield to screen-space chrome and to the hand fan.

---

## 7. The noninteractive scene contract

**The environment is a backdrop. It is never part of the game.**

Absolute, at every quality level, in every theme, on every device:

1. Every environment layer carries `pointer-events: none`. No layer is ever a hit
   target, a drop target, a focus stop, or a tab stop.
2. No environment element ever appears in `valid_actions[]`, in a prompt's candidate
   list, or in any hit-test result. The client computes no legality (`AGENTS.md`).
3. The environment carries **no state that survives a view**. It is a pure function of
   `(theme preference, viewport, quality, reduced-motion)`. Reconnect renders it
   identically with animation suppressed.
4. The environment never occludes a game object. It is strictly behind the scene plane
   and never overlaps a card, crest, pile, or path.
5. No environment reaction may be the **sole** channel for any information. Every hook
   below corroborates something already stated by the crest, the phase pill, the log,
   or a card's own treatment.
6. No hidden hotspots, no prebuilt clickable decorations. A future interactive
   decoration is a new issue and a new ADR consequence, not a flag flip.
7. The environment never gates input, never delays a scene build, and never blocks a
   match on an asset.

### 7.1 Ambient motion

| Property | Cap |
| --- | --- |
| Period | ≥ 4 s |
| Amplitude | ≤ 6 % opacity **or** ≤ 2 logical px translation — never both, never scale |
| Layers permitted | L0 (water shimmer, glow breathe) and L3 (lantern flicker) only |
| High | both |
| Standard | L0 only, amplitude halved |
| Lite | off |
| `prefers-reduced-motion` | **off at every level**, including High |

### 7.2 Passive reaction hooks

Keyed to the existing effect taxonomy (`asset-pipeline.md` §The generic effect
taxonomy) — no new protocol channel, no new log event.

| Hook | Trigger (intent / log) | Treatment | Layer | Cap | Corroborated by |
| --- | --- | --- | --- | --- | --- |
| `env.priority-pulse` | `priority` | warm the nearest lantern ~8 % | L3 | `turnFlow` 500 ms | crest gold glow, phase pill |
| `env.turn-tint` | `turn` | medallion tints toward the active seat accent | L1 | `turnFlow` 500 ms | crest position, phase pill, log |
| `env.impact-ripple` | `impact`, `damage` | one water ring in the nearest L0 body | L0 | `resolution` 600 ms | damage badge, impact flash |
| `env.loss-dim` | `death`, `PlayerEliminated` | L0 + L2 dim 4 % | L0/L2 | `turnFlow` 500 ms | eliminated treatment, log |
| `env.victory-bloom` | `GameOver` | medallion gold bloom | L1 | `SCENE_SESSION.victory` 800 ms | verdict panel |

Every hook: cosmetic only, no hit target, no layout change, suppressed entirely under
reduced motion and at Lite, `env.impact-ripple` additionally suppressed at Standard.
Durations come from `sceneTokens.ts`; none may be a literal in a CSS module.

---

## 8. Quality, fallback, and loading

### 8.1 Quality matrix

| | High | Standard | Lite |
| --- | --- | --- | --- |
| L0 | raster plate 1× | raster plate 1× | **token gradient** (no fetch) |
| L1 | raster plate 1× | raster plate 1× | raster plate **0.5×** |
| L2 | raster plate 1× | raster plate 1× | off |
| L3 | full sprite set + flicker | full sprite set, static | off |
| Parallax `E` | 12 px | 6 px | 0 |
| Ambient motion | L0 + L3 | L0, halved | off |
| Passive hooks | all | all but `env.impact-ripple` | none |

Lite retains the illustrated identity (the L1 plate) rather than collapsing to a
gradient — the requirement from #542 and the reason L1 is never dropped.

### 8.2 Loading states

| State | What renders | Playable? |
| --- | --- | --- |
| **T0 — pre-asset** (always the first frame) | the theme's token composition: radial `surroundTop → surroundBase`, plaza ellipse in `plazaCore → plazaEdge`, medallion mark, `glow` accent. Zero bytes, same frame as the first scene. | **yes, fully** |
| **T1 — placeholder resolved** | layered SVG in the same four slots (§10) | yes |
| **T2 — plates resolved** | raster L0–L3 at the tier's variants | yes |

The match is fully interactive at T0. No transition between states may move a hit
target, change a rect, or interrupt an animation; each layer cross-fades in on the
`staging` class (reduced motion: snap).

### 8.3 Failure

| Failure | Behaviour |
| --- | --- |
| One layer fails to load | that layer falls back to its T0 token treatment; every other layer keeps its resolved form |
| L1 fails | T0 plaza composition; the theme still reads |
| Whole theme fails | fall back to **`runicVale` at T0**, not to a dark dashboard gradient (#542), and surface nothing modal |
| Manifest missing or malformed | T0 for the selected theme; log once, never retry-loop |
| Theme key unknown (stale preference) | `DEFAULT_SCENE_THEME`, and rewrite the stored preference |

There is no state in which the environment is a hole, a flat black field, or a blocker.

---

## 9. Asset budget

### 9.1 Per-theme ledger (≤ 1.5 MB, ADR 0031)

| Manifest key | Layer / variant | Nominal size | Format | Budget | Load class |
| --- | --- | --- | --- | --- | --- |
| `env/<theme>/l0` | far surround | 1920×823 | AVIF | **290 KB** | first-match |
| `env/<theme>/l1-half` | arena floor, 0.5× | 1680×720 | AVIF | **150 KB** | first-match |
| `env/<theme>/l1` | arena floor, 1× | 3360×1440 | AVIF | **600 KB** | `lazy/` upgrade |
| `env/<theme>/l2` | arena edge, alpha | 3360×1440 | AVIF + alpha | **250 KB** | `lazy/` upgrade |
| `env/<theme>/l3` | prop atlas, alpha | packed | AVIF + alpha | **130 KB** | `lazy/` upgrade |
| `env/<theme>/manifest.json` | keys, anchors, crop guide, focal-safe rects | — | JSON | **4 KB** | first-match |
| | | | **Total** | **1 424 KB** | 76 KB / 5.1 % headroom |

L0 is authored at half linear resolution and upscaled: it is the softest layer and
upscaling costs nothing legible. L1 ships two variants because Lite needs the identity
without the bytes.

### 9.2 First-match cross-check (≤ 4 MB, `presentation-budgets.md`)

| Item | Size |
| --- | --- |
| Interactive code bundle (measured at `e0598dc`) | 299 KB |
| Fonts | 15 KB |
| Default theme, first-match class (`l0` + `l1-half` + manifest) | 444 KB |
| Portraits (8 × ~40 KB, #548 request 2) | 320 KB |
| Card backs (2, #548 request 3) | 50 KB |
| Effect atlas | 200 KB |
| **First-match total** | **≈ 1.33 MB** — 33 % of the 4 MB ceiling |

The default theme's `lazy/` upgrade (980 KB) arrives after the match is interactive, so
"lobby → match presentation ready ≤ 2 s" is met by the T0/T1 path and never waits on a
plate.

### 9.3 Repository cross-check (< 12 MB committed, ADR 0031)

4 themes × 1 424 KB = **5.70 MB**, plus portraits/backs/effects ≈ 0.6 MB = **≈ 6.3 MB**.
Room for a fifth theme before the ADR must be revisited. `docs/ui-concepts/` reference
imagery is counted separately per ADR 0031.

### 9.4 Load classification

- **First-match class** ships in `dist/` root and is counted by
  `clients/web/scripts/checkLoadBudget.js` as `asset`.
- **`lazy/` class**: the 1× L1, L2, L3 of *every* theme, and *all* assets of every
  non-default theme. The gate's `deferred` classification is the mechanism
  (`presentation-budgets.md` §Enforcement); putting a theme there is the explicit act
  that keeps it out of the first-match set.
- The ≤ 1.5 MB per-theme budget counts **all** classes for that theme and needs its own
  check; #530 adds it to the load-budget script.

### 9.5 Provenance

Every plate is ADR 0031 **class 2** (original AI-generated): tool and prompt essence
recorded in `clients/web/src/assets/ledger.json` and `ASSETS.md`. No prompt references
Magic: The Gathering, Arena, Wizards of the Coast, or any artist's name. CI fails on a
file in the asset tree without a ledger entry. Working files are not committed.

---

## 10. The SVG placeholder contract

Production plates are requested in #548 and do not exist. #530 must ship a complete,
shippable environment **now**, such that the raster drop is a file swap plus a ledger
entry — not a rework. This section is the mechanism.

### 10.1 What ships

**[DECISION]** The placeholder is **inline SVG rendered by a React component per layer**,
generated from `SCENE_THEMES`, not four committed `.svg` files.

Reasons, all binding: an `.svg` file cannot import `sceneTokens.ts`, so committing one
would put literal hex outside the token layer; a committed file needs an ADR 0031
provenance story for art that is really code; and a component costs bundle bytes, not
asset bytes, so the per-theme 1.5 MB ceiling stays measured against real plates only.

### 10.2 The slot identity that must be preserved

| Property | Placeholder must match the future plate |
| --- | --- |
| Layer count and names | L0, L1, L2, L3 — four components, four DOM nodes, one per layer |
| Manifest keys | `env/<theme>/l0`, `l1-half`, `l1`, `l2`, `l3` — the key resolves to a *renderer*, later to a URL |
| `viewBox` / aspect | `0 0 2333 1000` (21:9) for L0–L2; L3 sprites in their anchor boxes |
| Crop anchors | §4.2, applied by the same code path for SVG and raster |
| Focal-safe geometry | §2.2, identical rects, asserted by the same tests |
| Prop anchors | the six anchors of §4.4, same manifest shape |
| Parallax factors | §1.1, applied to the same four nodes |
| Load classes | placeholder for a `lazy/` key still resolves only when that tier asks for it |
| `pointer-events: none` | on every node, both forms |

### 10.3 What each placeholder layer draws

Not a flat rectangle — the composition must be recognisable so layout review is real:

| Layer | SVG content |
| --- | --- |
| **L0** | radial gradient `surroundTop → surroundBase`, two soft `water` ellipses at the left and right margins, one `glow` bloom at the horizon |
| **L1** | plaza ellipse filled `plazaCore → plazaEdge`, three concentric `paving` rings, radial fan strokes at ≤ 1.25:1, medallion mark at `(50 %, 40 %)`, `r = 5 % W`, drawn from the existing glyph geometry model (`chrome/glyphs/geometry.ts`) |
| **L2** | plaza rim stroke in `rim`, two lip bands at `y ∈ [0 %, 8 %]` and `[92 %, 100 %]`, verge fill in `verge` outside the rim |
| **L3** | four corner marks — two lantern silhouettes in `propWarm`, two crystal-plinth silhouettes in `propCool` — placed on the same anchors the plate's props will use |

### 10.4 Budget envelope

The placeholder renderer counts against the **interactive code bundle** (≤ 1.0 MB
gzipped, 701 KB headroom today), not against any asset budget.

- Cap: **≤ 12 KB gzipped** for all four layer components combined, across all themes
  (themes are data; the renderer is one).
- Zero asset bytes, zero ledger entries, zero network requests at T1.
- The per-theme 1.5 MB line reads **0 KB used** until the first plate lands.

### 10.5 The swap procedure

1. Drop the plate files at the manifest paths of §9.1.
2. Add ledger entries (`ledger.json` + `ASSETS.md`) with tool and prompt essence.
3. Flip `source: 'procedural'` → `source: 'raster'` for those keys in the theme
   manifest.
4. Done. No component change, no CSS change, no layout change, no test change — the
   geometry tests of §2.5 read the manifest, not the art, so they keep passing
   unmodified and prove the swap preserved the contract.

The placeholder renderer stays in the tree after the swap: it is the T0/failure
fallback of §8.3 and the Lite L0 treatment, both of which are permanent.

---

## 11. Theme selection

**Device-local presentation preference**, in the same idiom as the card-art source
(ADR 0024), saved decks (ADR 0027), and the quality/density/motion settings
(`clients/web/src/table/settings/presentationSettings.ts`, issue #505).

| Property | Value |
| --- | --- |
| Storage | `localStorage`, key `rune.presentation.environmentTheme`, guarded exactly as the shipped keys are |
| Type | `SceneThemeName` |
| Default | `DEFAULT_SCENE_THEME` (`runicVale`) |
| Surface | the existing presentation-settings panel, reachable from the front door and the in-match game menu |
| Protocol | **none** — never a `GameView` field, never a lobby field, never sent to the server |
| Scope | per device, per client. Two players in one match may see different themes; a spectator may see a third |
| Mid-match change | permitted; the manifest re-resolves and layers cross-fade on the `staging` class (reduced motion snaps) |
| Reconstruction | not load-bearing. The whole UI still rebuilds from one `GameView` + pending prompt; the theme only selects palette and plates |
| Unknown value | falls back to the default and rewrites the stored key (§8.3) |

Non-default themes are entirely `lazy/`: selecting one downloads it once, content-hashed
and cached forever, and the match stays playable throughout (T0 → T1 → T2).

---

## 12. Conflicts and open questions

Recorded here rather than by editing other documents.

| # | Conflict | Detail | Disposition |
| --- | --- | --- | --- |
| 1 | **Theme names** | `visual-system.md` §4 and `sceneTokens.ts` name *Runic Vale, Ember Reach, Pale Court*. The approved images name *Runic Vale, Verdant Canals, Sunlit Observatory, Moonlit Ruins*. #542's body still lists the old three. | The images win. #530 renames the token keys and updates `visual-system.md` §4 in the same PR. #542's body should be amended. |
| 2 | **Environment luminance direction** | `visual-system.md` §1 pillar 3 and §4 require the environment to sit *darker* than the play surface. The approved baseline has a **light warm plaza** (`#B4A379`, L ≈ 0.37) under **dark-framed cards**. | The images win. "Cards are the highest-contrast objects" survives — it is now delivered by dark card frames on a mid-light desaturated field (measured ≈ 3.7:1). `visual-system.md` §1/§4 needs rewording from "darker" to "at least one contrast step away from, and lower in chroma than, the play surface". |
| 3 | **`EnvironmentTheme` token slots** | The shipped six slots (`skyTop/skyHorizon/skyBase/ground/arena/glow`) cannot express a light plaza over a dark surround. | Replace with the thirteen slots of §5.4 in #530; extend the `sceneTokens.test.ts` lockstep contrast gate. |
| 4 | **Three-player composition** | Panel 2 places two opponents symmetrically at top-left and top-right with top-centre open. `layout-model.md` stages 3p as focused far side (top centre) + one wing. | Not resolved here — this is #531's call. Either reading fits the environment: both produce occupied regions inside Zone A ∪ Zone B, so the art is unaffected. Flagged for the layout owner. |
| 5 | **Panels 4 and 5 over-count seats** | Panel 4 draws six portraits for "5 Players"; panel 5 draws eight for "6 Players". | Treated as concept-render artefacts. Only the *placement pattern* is binding (§3.1). Panels 1–3 are exact. |
| 6 | **`AMBIENT SPACE` purpose** | Neither #542 nor #548 states what it is for. | §6.1 decides it: reserved for a future non-load-bearing presentation surface. Open for the maintainer to narrow. |
| 7 | **Focal-safe percentage** | #548 and #542 say "the central ~55 %" / "55–65 %". The derived union is **80 % of width at full height** (§2). At 55 % a prop could sit at `x = 22.5 %`, which the receiver band and the far side both occupy. | **#548 and #542 should be amended to the §2.2 zones.** The L0–L3 breakdown itself needs no change. |
| 8 | **Per-theme budget is unenforced** | `checkLoadBudget.js` covers the code, font, and first-match budgets, not the ≤ 1.5 MB per-theme line. | #530 adds the per-theme check when the first plate (or manifest) lands. |
| 9 | **Ledger CI gate does not exist yet** | ADR 0031 says the gate lands with the first real asset. The SVG placeholder is code, so it does not trigger it. | The gate is owed by #548's first delivery, not by #530. |

---

## 13. Decisions this document makes

Items the approved images did not dictate, decided here and normative until amended:

1. Parallax magnitude ladder and the `E` excursion cap (§1.1).
2. Per-layer local-contrast ceilings (§1).
3. The 2 % guard band applied to the derived slot union (§2.1).
4. The three-zone model A/B/C and the 10 %/67 % pocket boundaries (§2.2).
5. The lip carve-out that lets L2 cross the focal core (§2.4).
6. The arc placement rule and the ban on baked seat geometry in L1 (§3.2).
7. Centre horizontal anchoring for every landscape crop (§4.2).
8. The 4:3 "must survive" containment rule (§4.3).
9. **L3 as anchored sprites rather than a baked plate** — the mechanism that makes one
   source serve 16:9 and 21:9 (§4.4).
10. Phone portrait as recomposition, with the medallion pinned to `(50 %, 40 %)` (§4.5).
11. The invariant/variant split (§5.1–5.2) and the thirteen-slot token interface (§5.4).
12. The `AMBIENT SPACE` purpose, its normative rect, and the one-prop rule (§6).
13. The five passive reaction hooks and their caps (§7.2).
14. The T0/T1/T2 loading model and the per-layer failure fallback (§8.2–8.3).
15. The per-layer byte allocation and the first-match / `lazy/` split (§9).
16. **Inline-SVG-from-tokens as the placeholder form**, its 12 KB code-bundle cap, and
    the four-step swap procedure (§10).
17. Theme selection as a device-local key with mid-match cross-fade (§11).
