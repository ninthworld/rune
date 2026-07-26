# Seat identity and player-state policy

**The design authority for the seat cluster and for which player-specific
information is always visible, conditionally visible, or opened on demand**
(issue #539, parent #464). It is the binding component/data policy for #532 and
for the player-state portions of #534/#536 — those issues implement this
document and may not decide player-state visibility ad hoc.

**Design authority.** The approved baselines in
[`../ui-concepts/`](../ui-concepts/) (commit `e58300b`, issue #547) are binding
for anatomy, silhouette, and proportion:

| Sheet | What it fixes here |
| --- | --- |
| [`rune-player-control-ui.jpg`](../ui-concepts/rune-player-control-ui.jpg) panels 1–5 | cluster anatomy, the four attested variants, the expanded tray, the mana reservoir |
| [`rune-2.5d-interface-baseline.jpg`](../ui-concepts/rune-2.5d-interface-baseline.jpg) | clusters in a live composition; mirroring per side |
| [`rune-zones-interaction.jpg`](../ui-concepts/rune-zones-interaction.jpg) | the minimal rung at four seats |
| [`rune-battlefield-environments.jpg`](../ui-concepts/rune-battlefield-environments.jpg) panels 1–5 | cluster anchors and orientation at 2/3/4/5/6 seats |

Where a baseline decides something, this document transcribes it. Where the
baselines are silent, this document decides and marks the row **[D]**. Every
**[D]** row is collected in §12.

**Relationships.** [`visual-system.md`](visual-system.md) owns look and motion
(this document is the normative detail under its §6 and §7);
[`layout-model.md`](layout-model.md) owns region geometry and the degradation
ladder (this document only fixes the cluster's anchor and orientation inside a
slot); [`presentation-budgets.md`](presentation-budgets.md) caps every number
here; [`ui-requirements.md`](ui-requirements.md) stays binding.

**Normative language.** MUST / MUST NOT / SHOULD / MAY as usual. Two project
hard rules govern everything below and are repeated because they are load
bearing:

- **Zero game logic in the client.** Every value is displayed exactly as the
  server computed it. The client never derives legality, cost, effect,
  terminality, or a rules threshold.
- **The whole cluster MUST be reconstructable from one `GameView` (or one
  `SpectatorView`) plus the pending prompt.** No cluster state survives a
  message. Disclosure state (§3) is ephemeral presentation state and is
  re-derived, exactly like selection.

---

## 1. Cluster anatomy

### 1.1 The scale unit

Every dimension is expressed in **D**, the portrait medallion's outer rim
diameter. One cluster has one D; nothing inside it scales independently.

| Rung | D | Where | Touch target |
| --- | --- | --- | --- |
| Local expanded | 112 px | receiver, disclosure open | cluster ≥ 112 px |
| Local normal | 112 px | receiver's band | cluster ≥ 112 px |
| Focused | 96 px | far side | cluster ≥ 96 px |
| Wing (full board) | 76 px | 3–4 seats | cluster ≥ 76 px |
| Wing (digest) / compact | 60 px | 5–6 seats | cluster ≥ 60 px |
| Minimal | 48 px | phone summary tile, deep digest | cluster ≥ 48 px |

**[D]** The ladder itself. The baselines draw four sizes without labelling
them; these values place every rung above the 44 px floor of
`presentation-budgets.md` §Accessibility and step by a readable ~1.25×.

### 1.2 Elements

| # | Element | Geometry (in D) | Notes |
| --- | --- | --- | --- |
| 1 | **Portrait medallion** | outer Ø `1.00`; rim band `0.065` total, as a `0.012` gold hairline + `0.041` brushed-gold band + `0.012` inner hairline; aperture Ø `0.87` | circular mask; portrait art is clipped to the aperture, never to the rim |
| 2 | **Life medallion** | Ø `0.55` (±0.03); ring `0.055` gold, ink `#0D0F13` face; centre on the portrait's vertical axis at `cy_portrait + 0.50` — i.e. **on the portrait's bottom rim** | ~50 % of its height (`0.275 D`) overlaps the portrait; it renders **above** the portrait and **below** nothing |
| 3 | **Keystone rivet** | gold trapezoid `0.06` wide × `0.025` tall at 12 o'clock of the life ring | the join mark; purely decorative, transcribed from panels 1/2/4 |
| 4 | **Nameplate** | height `0.35`; length `1.05`–`2.40`; dark slate fill, double gold rule (`0.010` outer, `0.006` inner, `0.02` inset) | outboard end terminates in a **pointed chevron**, run `0.16` (0.45 × plate height); inboard end butts the identity gem |
| 5 | **Name text** | display serif, small caps, letterspaced `+0.04 em`, `#E8E6E1`, optical centre of the plate, inset `0.19` from the chevron tip | ≥ 12 px semibold at every rung (budget floor) |
| 6 | **Identity gem** | rotated square `0.19` across the diagonal, gold bevel `0.02`, faceted fill with one specular streak | centred on the nameplate's vertical centre, at the plate's inboard end |
| 7 | **Hand pip** | vertical hexagon (points top and bottom, vertical left/right sides) `0.38` wide × `0.42` tall; gold hairline rim, ink face, keystone nub at 12 o'clock | centre at `cy_life`, `cx_portrait ± 0.65` on the **outboard** side; clear gap `0.20` from the life ring |
| 8 | **Library pip** **[D]** | same hexagon at `0.85` scale (`0.32` × `0.36`), fill `#151A24`, no keystone nub, a two-line "spine" mark on the leading edge | mirrors the hand pip: `cx_portrait ∓ 0.65`, **inboard** side, same `cy` |
| 9 | **Under-slung tab** | shield pentagon `0.48` wide × `0.52` tall, flat top, vertical sides, point down; gold hairline rim | the compact substitute for pip 7 — centre at `cx_portrait`, `cy_life + 0.25`, ~55 % occluded by the life medallion |
| 10 | **Status rail** **[D]** | arc of `0.28`-diameter round medallions on a `1.15 D` radius from the portrait centre, stepping `26°`, growing from the inboard shoulder (2 o'clock for a left-side cluster, 10 o'clock for a right-side one) | conditional only; §5.6 |
| 11 | **Disclosure chevron** | circular button Ø `0.40`, gold hairline rim, ink face, `^`/`v` chevron glyph | at `cy_life`, `cx_portrait + 1.20` outboard of the hand pip; local cluster only |

**Stacking order within a cluster (back to front):** state rings (§6) →
expanded tray (§3) → nameplate → portrait medallion → identity gems → life
medallion → count pips → status rail → disclosure chevron.

### 1.3 Portrait art

The portrait medallion carries one of the bundled raster plates in the
**same aperture mask** and with the same rim at every rung.

- **Raster plate** — the portrait set delivered by issue #548 §Request 2:
  square source art, head-and-shoulders, no rim, no ring, no numbers. The
  client applies mask, rim, and every state ring. The local player's plate is
  the deliberately faceless hooded figure of panels 3–4.

The former procedural rune/monogram placeholder is removed when the portrait
consumer lands. While a plate is loading or if it fails, the aperture keeps its
token background and accessible player name but draws no substitute glyph.

There is **no protocol field selecting a portrait**. Plate assignment is
presentation, keyed by the seat's index in `seat_order` (§10).

---

## 2. Cluster variants

Four variants are attested in the baseline; three more are required by the
client and are decided here.

| Variant | Source | D | Nameplate | Life | Hand pip | Library pip | Status rail | Chevron |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **Peripheral / compact** | panel 1 | 60 | yes, name only | yes | as under-slung tab (9) | pile badge only **[D]** | ≤ 1 + `+N` | no |
| **Focused / active** | panel 2 | 96 | yes, name only | yes | hexagon (7) | hexagon (8) **[D]** | ≤ 2 + `+N` | no |
| **Local normal** | panel 3 | 112 | yes, `You` or name | yes | hexagon (7) | hexagon (8) **[D]** | ≤ 2 + `+N` | yes (`v`) |
| **Local expanded** | panel 4 | 112 | yes | yes | hexagon (7) | inside the tray **[D]** | ≤ 2 + `+N` | yes (`^`) |
| **Wing (full)** | derived | 76 | yes, name only | yes | hexagon (7) | hexagon (8) | ≤ 1 + `+N` | no |
| **Minimal** | zones sheet | 48 | no | Ø `0.42` at 7–8 o'clock | no | no | `+N` only | no |
| **Eliminated** **[D]** | — | rung's D | yes, struck through | replaced (§6.5) | no | no | none | no |
| **Disconnected** **[D]** | — | rung's D | yes | yes, dimmed | yes | yes | plus link glyph | as rung |
| **Spectator (self)** **[D]** | — | — | — | — | — | — | — | — |

**Eliminated [D].** The portrait aperture desaturates to 0 % and drops to 45 %
luminance; the rim turns from gold to `#5A5F66` stone; the life medallion's
numeral is replaced by a struck rune glyph (a circle with a single diagonal
bar) at `0.30 D`; count pips and the status rail are removed; the nameplate
keeps the name with a `0.01 D` strike rule through it. The cluster stays in its
slot, stays keyboard reachable, and its public piles stay browsable
(`layout-model.md`). Shape (stone rim + struck rune) carries the state; colour
never alone.

**Disconnected [D].** A broken-link glyph — two `0.10 D` chain links with a
`0.03 D` gap, drawn in `#8C949C` — is pinned at the portrait's 10-o'clock rim
(inboard shoulder), overlapping the rim band. The portrait dims to 70 %
luminance; nothing is removed, because the seat is still in the game. This is
**specified but unimplementable today** (§11).

**Spectator (self) [D].** A spectator has no seat and therefore no local
cluster: `SpectatorView` carries no `you`, `me`, `my_hand`, or `mana_pool`. The
receiver's band renders **no** cluster, **no** hand pip, **no** disclosure
chevron, and **no** mana reservoir. Every seat — including the one a viewer
might think of as "theirs" — renders in the **focused** or **wing** variant
from `SpectatorView.players[]`. The band instead carries the spectator's
watch-mode label. Any variant that would display a private value MUST be
unreachable in spectator mode by construction, not by a runtime check.

---

## 3. The expanded local state

### 3.1 What panel 4 shows

A **fan tray** unfolds downward and outward from behind the local cluster:

| Property | Value |
| --- | --- |
| Silhouette | a shallow wing: top edge is an inverted-V peaking under the life medallion; bottom edge is a downward arc; both ends taper to points |
| Width | `3.8 D` (≈ 426 px at D = 112), centred on the portrait axis |
| Height | `1.15 D` at the centre, `0.55 D` at the tips |
| Top edge | tucks behind the life medallion and the nameplate's inboard end |
| Fill | `#1B212D` at 92 % with the standard double gold rule on the outer contour |
| Dividers | radial gold hairlines emanating from the portrait centre, at `±22°` from vertical |
| Segments | left = **Poison**, centre = **Commander damage**, right = **Counters** |
| Section labels | display serif small caps, gold `#C9A84C`, `0.11 D` cap height |
| Poison value | bare numeral, `0.30 D`, toxic green `#8FBF5A` — no chip |
| Counters value | hexagon `0.30 D` wide with a brass fill `#8A6A38` and gold rim, numeral in `#F3EBDC` |
| Commander-damage chips | circular portrait medallions Ø `0.40 D`, pitch `0.52 D`, each with a value hexagon `0.27 D` wide slung under it, ~50 % overlapped |
| Zero rows | shown (panel 4 draws a `0`) — see §5.4 |

The tray renders **behind** the life medallion, the hand pip, and the
disclosure chevron; those three stay fully legible while it is open.

### 3.2 Disclosure interaction **[D]**

| Aspect | Rule |
| --- | --- |
| Trigger | the disclosure chevron (element 11), or activating the life medallion, or `Enter`/`Space` on the focused cluster |
| Affordance | the chevron rotates `180°` in ≤ 150 ms (feedback class); reduced motion swaps the glyph |
| Scope | **the local cluster only.** An opponent's detail opens as a **popover** anchored to their cluster, never as an in-place tray — a tray at a wing rung would overlap a neighbouring region, which `layout-model.md` forbids |
| Persistence | ephemeral. Dropped and re-derived on the next view, like selection and manual focus |
| Auto-open | never. The tray MUST NOT open itself on a threshold, a warning, or a prompt |
| Dismiss | the chevron, `Escape`, activating any other cluster, or any pointer/touch outside the tray |
| Hit target | chevron ≥ 44 px at every rung (Ø `0.40 D` = 45 px at D = 112; below that the chevron is not offered and the popover path is used) |
| Empty tray | if every segment would be empty **and** the game is not a Commander game, the chevron is not rendered at all |

### 3.3 Tray contents by seat count

The tray's width is fixed; the commander-damage segment is what varies.

| Seats | Commander-damage chips | Behaviour |
| --- | --- | --- |
| 2 | 1 | one chip, centred |
| 3 | 2 | pitch `0.52 D` |
| 4 | 3 | as panel 4 |
| 5 | 4 | pitch compresses to `0.46 D` |
| 6 | 5 | pitch compresses to `0.42 D`; chips drop to Ø `0.36 D` |

Chips are ordered by `seat_order`, starting from the seat clockwise of the
receiver, so a chip's position is stable for the whole game. Eliminated
commanders keep their chip (their damage is still lethal history) with the
eliminated portrait treatment. A non-Commander game renders no
commander-damage segment and the tray narrows to `2.4 D`.

---

## 4. Information matrix

| Value | Always | When nonzero / applicable | On demand | Surface |
| --- | --- | --- | --- | --- |
| Player name (or seat label) | ● | | | nameplate |
| Life | ● | | | life medallion |
| Hand count | ● | | | hand pip |
| Library count | ● | | | library pip (pile badge at compact/minimal) |
| Active turn | ● | | | turn pennant (§6.2) |
| Priority | ● | | | priority ring (§6.1) |
| Elimination | ● | | | eliminated variant (§2) |
| Connection state | ● | | | link glyph (§6.4) — **blocked, §11** |
| Identity gems | ● | | | gems 6 |
| Commander presence | | ● | | command badge on the pile; cluster shows a `0.14 D` crown mark on the portrait rim at 5 o'clock |
| Commander tax | | ● | ● | **on the command-zone pile**; the cluster duplicates it only when that pile is collapsed out of the rung |
| Commander damage (worst incoming) | | ● | | shield chip on the status rail |
| Full commander-damage matrix | | | ● | expanded tray / popover |
| Poison | | ● | | tray + rail chip — **blocked, §11** |
| Other player counters | | ● | ● | tray "Counters" segment — **blocked, §11** |
| Named statuses (≤ 2) | | ● | | status rail |
| Named statuses (all, with text) | | | ● | status popover — **rules text blocked, §11** |
| Attacked marker + count | | ● | | attacked ring + count chip (§6.3) |
| Decision deadline | | ● | | deadline ring (§6.6), receiver only |
| Auto-passed notice | | ● | | transient chip at the local cluster |
| Local unspent mana | | ● | | mana reservoir (§5.2) |
| Opponent mana | — | — | — | **never rendered** (§5.2) |
| Graveyard / exile counts | | | ● | pile badges, not the cluster |
| Turn number, phase | ● | | | phase chrome, not the cluster |

---

## 5. Value specifications

### 5.1 Life

- The life medallion carries the number **verbatim** from the server. The
  client never adds, subtracts, or clamps it.
- Numerals: display face, `0.34 D` cap height for 1–2 digits, `0.28 D` for 3
  digits, `0.24 D` for 4 glyphs (`-99`). The medallion never resizes; the
  numeral does. Minimum rendered size stays ≥ 12 px semibold at every rung.
- **Gain/loss** animates per `visual-system.md` §8 (green rise pulse / red
  impact flash + floating delta chip). The animation is driven by the log
  events; the **displayed number** always comes from the life field, never from
  a running client total.
- **Urgency is a shape, never a colour alone** **[D]**:

| Condition | Shape channel |
| --- | --- |
| `life ≤ 5` | the life ring gains a `0.02 D` notched outer collar (8 notches) |
| `life ≤ 0` | the collar closes into a solid double ring and the numeral sets in the loss hue |

  Both are thresholds on a *displayed number*, not a rules judgement: the
  client MUST NOT declare a loss. Terminality comes from `result` alone.

### 5.2 Mana pool — local only

The protocol exposes unspent mana for the **receiving player only**
(`GameView.mana_pool`). Therefore:

- The reservoir renders **only** on the local cluster, and **only** from
  `mana_pool`.
- The client MUST NOT render, infer, estimate, or hint at an opponent's mana —
  **including from tapped lands**, from the battlefield, or from the log. There
  is no "probably has `{U}` up" affordance at any rung.
- A spectator sees no reservoir at all (`SpectatorView` has no `mana_pool`).
- **Hidden when empty.** `mana_pool.length === 0` renders nothing — no empty
  frame, no zero row, no reserved space.

Transcribed from panel 5:

| Property | Value (G = gem width = `0.48 D_local`; 54 px at D = 112) |
| --- | --- |
| Banner | the same chevron plate as the nameplate, height `0.35 D`, label `MANA`, left-aligned above the row |
| Gem silhouette | vertical faceted cabochon: clipped upper corners, widest at 40 % height, rounded taper to a bottom point; `G` wide × `1.08 G` tall; gold bevel rim `0.05 G` |
| Gem glyphs | `{W}` sun-rosette on ivory-gold, `{U}` droplet on cyan, `{B}` skull on violet-black, `{R}` flame on ember, `{G}` leaf/tree on green, generic/colourless faceted diamond on silver |
| Count hexagon | the vertical hexagon of element 7 at `0.60 G` × `0.66 G`, centred `(+0.26 G, +0.45 G)` from the gem centre — i.e. slung on the gem's lower-right, ~50 % overlapped |
| Row pitch | `1.35 G`, single row, left-aligned |
| Anchor | attached to the local cluster / action area, inboard of the nameplate |

**Grouping rule** (transcribed + made exact **[D]**): identical pip strings
group into **one** gem carrying the count. Gems are ordered
`{W} {U} {B} {R} {G}` then everything else; a gem whose count is zero is not
rendered. A pip string the client has no gem for renders on the generic gem
with the raw pip string as its accessible label — never dropped, never
guessed. Hybrid or Phyrexian pip strings are treated as their own group, keyed
by the exact string; the client MUST NOT split `{W/U}` into two gems, because
that would be an interpretation of the cost.

### 5.3 Commander tax

Tax lives with the **command-zone pile**, where the recast action originates.
The cluster carries a commander-presence crown mark only. The cluster displays
the tax value **only** when the command pile is not drawn at the current rung
(digest, minimal, phone summary tile); in that case a `0.28 D` chip reading
`+{N}` attaches to the status rail. Both readings come from `commander_tax`,
never from a cost string.

### 5.4 Commander damage

`commander_damage` is authoritative. Two surfaces:

| Surface | Content | Rule |
| --- | --- | --- |
| **Cluster (warning)** | the single **highest nonzero** incoming value | shown only when ≥ 1; `0` tallies are never drawn here |
| **Tray / popover (matrix)** | one chip per opposing commander in `seat_order`, **including zeros** | the opened matrix is the complete picture; a missing row would read as "no data", not "no damage" |

Selecting the maximum of a server-provided list is presentation ranking, not
game logic: no legality, cost, effect, or terminality is computed, and 21 stays
the server's call via `result.reason == "commander_damage"`.

Escalation is **shape first** **[D]**, thresholds from CR 903.10a:

| Incoming | Treatment |
| --- | --- |
| 1–14 | plain shield chip with the source seat's accent as a `0.02 D` edge stripe |
| 15–17 | the shield gains a doubled outline |
| 18–20 | plus a near-lethal notch ring (3 notches at the chip's top arc) |
| 21+ | terminal treatment; the seat is expected to carry the eliminated variant from `eliminated` / `result` in the same or a following view. The client MUST NOT eliminate the seat itself |

Activating the chip opens the matrix (tray for the local seat, popover for an
opponent).

### 5.5 Commander name

The nameplate's primary line is the **player** name. In a Commander game the
commander's name renders as a subordinate second line at `0.22 D` cap height in
`#B8B4AC`, only at the local, focused, and wing-full rungs. It is read from the
player's command-zone pile and therefore **disappears whenever the commander is
not in the command zone** — see §11.

### 5.6 Named statuses

- Rail medallions are `0.28 D` circles with a glyph and no text at the rung;
  the label rides the accessible name and the popover.
- **At most two** are drawn, plus a `+N` overflow medallion. Ordering is the
  server's array order — the client MUST NOT rank, prioritise, or interpret.
- Activating a rail medallion (or the `+N`) opens the status popover listing
  every status by its server-provided string.
- `statuses` is **free-form display text**. The client MUST NOT parse a number
  out of it, MUST NOT match it against a mechanic table to derive behaviour,
  and MUST NOT infer a threshold from it. An unrecognised string renders on the
  generic status glyph with the raw string as its label.
- Single-holder statuses (monarch, initiative) MAY additionally light a scene
  prop, per `visual-system.md` §4 — the cluster badge is still required.

---

## 6. State channels

Every state gets a **distinct shape or placement**; colour is never the only
carrier (`presentation-budgets.md` §Accessibility).

### 6.1 Priority — glowing double ring (transcribed, panel 2)

| Layer | Geometry |
| --- | --- |
| Gap | `0.030 D` of untouched background outside the gold rim |
| Bright core ring | `0.045 D` stroke, near-white core `#DCEBFF` |
| Outer bloom | `0.050 D` soft falloff in `#7FB2E5`, additive |

Motion: a slow breathe, 1.4 s period, ±8 % bloom radius. Reduced motion holds
the static double ring. The ring is **concentric and outside** the medallion —
no other state uses that placement.

### 6.2 Active turn — turn pennant **[D]**

A pennant tab: `0.30 D` wide × `0.22 D` tall, a rectangle with a notched
(swallow-tail) trailing edge, in the seat accent with a gold hairline, pinned
to the portrait rim at **12 o'clock**, half overlapping it. Distinct from
priority in both shape (tab vs ring) and placement (top vs perimeter), so a
seat that is active *and* holds priority reads as both at once — the common
case.

### 6.3 Attacked — incoming ring + count chip **[D]**

- A **dashed** ring in targeting orange `#E0784A`, `0.03 D` stroke, drawn at
  `0.52 D` radius from the portrait centre — **inside** the priority ring's
  band, so the two never collide.
- Combat paths terminate on the portrait centre (`layout-model.md`: paths
  terminate at defender crests regardless of focus).
- A count chip `Attacked ×N` at the rail's first slot, `0.28 D`, with the
  attacker count. `N` is the number of `battlefield` entries whose
  `attacking_player` names this seat — a filter over server state, not a combat
  computation.

### 6.4 Disconnected — broken-link glyph **[D]**

Per §2. Placement (inboard shoulder) and shape (broken chain) are unique.
**Blocked, §11.**

### 6.5 Eliminated — stone crest **[D]**

Per §2: stone rim, desaturated aperture, life numeral replaced by the struck
rune. The life *number* is removed rather than shown as `0`, because a live
seat may legitimately sit at `0` for an instant before state-based actions.

### 6.6 Decision deadline **[D]**

When `action_deadline` is present (receiver only), the priority ring's bloom
layer is overdrawn by a **depleting arc** starting at 12 o'clock and sweeping
clockwise, `0.045 D` stroke, in gold. It never replaces the priority ring; it
rides on top of it. The client displays the countdown and never enforces it.

### 6.7 Auto-passed and rejected action **[D]**

`auto_passed` renders a transient `0.28 D` chip reading `Passed for you` at the
local cluster's rail, auto-dismissing in ≤ 3 s. `action_rejected` is a toast,
not a cluster state. Neither is load bearing.

### 6.8 Channel summary

| State | Colour | Shape channel | Placement |
| --- | --- | --- | --- |
| Priority | blue-white | double glow ring | concentric, outside the rim |
| Active turn | seat accent | swallow-tail pennant | 12 o'clock, on the rim |
| Attacked | orange | dashed ring + count chip | inside the rim + rail slot 1 |
| Decision deadline | gold | depleting arc | over the priority band |
| Disconnected | neutral | broken chain glyph | 10 o'clock shoulder |
| Eliminated | stone | struck rune replaces life | the life medallion itself |
| Commander presence | gold | crown mark | 5 o'clock, on the rim |
| Commander damage | orange | shield chip (+ notches) | status rail |
| Poison | green | drop/skull chip | status rail + tray left segment |
| Named status | neutral | round glyph medallion | status rail arc |
| Targetable seat | orange | ring + drawn path | per `visual-system.md` §7 |
| Selected seat | blue | ring + elevation | per `visual-system.md` §7 |

---

## 7. Stress states

| Case | Rule |
| --- | --- |
| **Long name** | the nameplate grows to `2.40 D`, then the text truncates with a middle ellipsis (`Verynamed…player`), keeping the first 8 and last 4 graphemes. The full name rides the accessible name and the popover. The plate never wraps and never pushes the portrait. |
| **No name** | `player_names` has no entry → the label is `Seat N` from the seat's index in `seat_order` (1-based). Never blank, never a raw `p{N}` id. |
| **Life `999`** | numeral steps to `0.28 D`; medallion unchanged. |
| **Life `-99`** | numeral steps to `0.24 D`; the minus sign is a `0.10 D` en-dash-weight bar so it does not read as part of the digit. |
| **Life ≥ 4 glyphs** | numeral holds `0.24 D` and the medallion clips nothing; beyond 5 glyphs the value truncates to `999+` / `-99−` **only in the pip**, with the exact value in the accessible name and the tray. |
| **20-card hand** | the hand pip shows `20` at `0.20 D`; no shape change. Hand *fan* paging is `layout-model.md`'s concern, not the cluster's. |
| **3-digit library** | the library pip's numeral steps to `0.18 D`; the hexagon does not grow. |
| **Library `0`** | rendered as `0` — a zero library is load-bearing information (CR 704.5c), never hidden. |
| **5 commander-damage sources** | tray pitch compresses per §3.3; the cluster still shows exactly one worst-value shield. |
| **6 status kinds** | rail draws 2 + `+4`; the popover lists all six. At the wing rung it draws 1 + `+5`; at minimal, `+6` alone. |
| **Empty mana** | reservoir absent entirely (§5.2). |
| **Nonempty mana, 6 kinds** | six gems in one row at pitch `1.35 G`; if the row would exceed the receiver band's inner width it wraps to a second row rather than shrinking a gem below the 44 px hit floor. |
| **8 seats** | the rung ladder bottoms out at minimal (48 px); `ui-requirements.md` requires 2–8 tiles without moving the receiver. Cluster anatomy does not change below minimal — clusters are removed from no rung. |
| **Every state at once** | priority ring + turn pennant + attacked ring + deadline arc + crown + full rail can co-occur. They occupy five disjoint placements plus the rail, by construction. |

---

## 8. Placement and orientation

`layout-model.md` owns the slots. This section fixes only where the cluster
sits **inside** its slot and which way it faces, transcribed from
`rune-battlefield-environments.jpg` panels 1–5.

**Orientation rule (transcribed from the 2.5D baseline).** The **portrait sits
nearest the table centre and the nameplate points outboard**, away from the
corridor. Consequences:

| Slot | Nameplate | Identity gems | Hand pip | Library pip | Under-slung tab |
| --- | --- | --- | --- | --- | --- |
| Left wing | extends left | left of portrait | left (outboard) | right | centred below |
| Right wing | extends right | right of portrait | right (outboard) | left | centred below |
| Far side (top centre) | extends **left** **[D]** | left | right | left | centred below |
| Local (bottom centre) | extends **left** | left | right | left | centred below |

For centre-anchored clusters "outboard" is ambiguous; the baseline draws the
nameplate to the left in both cases, and that is fixed here as the default.
Clusters MUST NOT rotate — text stays horizontal at every seat, at every count.

**Anchors per seat count** (panel = `rune-battlefield-environments.jpg`):

| Seats | Panel | Local | Far side | Wings |
| --- | --- | --- | --- | --- |
| 2 | 1 | bottom centre, on the band's outer edge | top centre-left, on the opponent's board edge | — |
| 3 | 2 | bottom centre | — (both opponents read as upper seats) | one upper-left, one upper-right, each outboard of its board strip |
| 4 | 3 | bottom centre | top centre, above its board, priority ring shown | one mid-left, one mid-right, outboard of their strips |
| 5 | 4 | bottom centre | top centre | upper-left, upper-right, mid-right — stacked along the ellipse, outboard |
| 6 | 5 | bottom centre | top centre | two per side, stepping down the left and right arcs, outboard |

The local cluster is **always bottom centre**, straddling the receiver band's
outer edge, with the library pile to its left and the graveyard/exile piles to
its right (panels 3–5). The focused cluster is **always top centre**. Wing
clusters step down the ellipse in `seat_order`, alternating sides, and never
cross the centre corridor.

**"Straddling" is literal (issue #582).** The portrait medallion's centre sits
*on* the band's outer edge, so half the cluster hangs below the board and half
overlaps it — which is exactly what the focused cluster already does at the top
(`cy = slot.y`). The first implementation put the centre `0.62 D` *inside* the
band, so 62 % of a 112 px medallion plus all of its priority bloom, life ring,
and hand pip were drawn over the player's own creatures, and the board's
reservation for it (`layout-model.md`) cost 129 px of a 195 px band instead of
68 px. The clamp that keeps the lower half on the plane is the only thing that
may move it.

> The 5-seat panel stages wings as **one left, two right**, while
> `layout-model.md` §Staging per player count specifies **2 left, 1 right**.
> See §13.

---

## 9. Interaction

| Verb | Behaviour |
| --- | --- |
| **Focus** | activating any cluster (pointer, touch, `Enter`/`Space` on the focused element) re-stages that seat to the far side. Ephemeral; re-derived each view. The local cluster is not focusable in this sense — it never leaves the band. |
| **Player targeting** | when a prompt lists a seat as a candidate, the cluster becomes the pick surface: targeting ring + drawn path, hit rect ≥ 44 px, `aria-label` `Target player {name}`. Non-candidate clusters dim. Candidates pierce every rung (`layout-model.md`) — a digest or minimal cluster is still individually pickable. |
| **Defender choice** | during attack declaration, each legal defending seat's cluster is a candidate by the same contract; the chosen defender additionally wears the attacked ring and terminates the drawn path. The set of legal defenders comes from `valid_actions`; the client never computes it. |
| **Disclosure** | §3.2 — local tray, opponent popover. |
| **Status details** | activating a rail medallion opens a popover anchored to the medallion, dismissible by `Escape` or outside press, focus-trapped, listing every status verbatim. |
| **Commander-damage details** | activating the shield chip opens the same surface as disclosure, scrolled to the commander-damage segment. |
| **Keyboard** | the cluster is one tab stop; the disclosure chevron, the shield chip, and the rail are a roving-tabindex group inside it. `Escape` closes any open popover and returns focus to its anchor. |
| **Touch** | every activatable sub-element keeps a ≥ 44 px hit rect even where its painted size is smaller; hit rects MUST NOT overlap. Where a rung cannot fit two non-overlapping 44 px rects, the sub-element is not offered and its content moves into the cluster-level popover. |
| **Screen readers** | the cluster exposes one accessible name of the form `{name}, {life} life, {hand} in hand, {library} in library` plus the applicable states, so the whole seat reads without opening anything. |

---

## 10. Data-source table

**This is the binding column.** Every visible value maps to a concrete field
that exists in `crates/rune-protocol` today, or is marked **GAP**. Field paths
are verified against `crates/rune-protocol/src/{view,card,result,spectator}.rs`
and their TypeScript mirrors in `clients/web/src/protocol/`.

### 10.1 Identity and ordering

| Displayed value | Seated source | Spectator source | Status |
| --- | --- | --- | --- |
| Seat set and order | `GameView.seat_order` (falls back to `you` + `opponents[].player_id`) | `SpectatorView.seat_order` | exists |
| Receiver's seat id | `GameView.you` | — (no receiver) | exists |
| Player name | `GameView.player_names[id]` | `SpectatorView.player_names[id]` | exists; absent key → `Seat N` |
| Seat accent colour | derived from the index in `seat_order` (`identityAccents.ts`) | same | derived, deterministic |
| Portrait plate | — | — | **GAP (asset, not protocol)**: assignment is by seat index; production plates are #548 §2b |
| Identity gems (colours) | `deriveColorIdentity()` over `GameView.command[].cards[].mana_cost` for `player_id == id` | `SpectatorView.command[]` | **GAP**: #553 — only while the commander is in the command zone; no per-player colour-identity field |
| Commander name | `GameView.command[].cards[].name` | `SpectatorView.command[]` | **GAP**: #553 — disappears when the commander is on the battlefield or in a graveyard |

### 10.2 Counts and life

| Displayed value | Local (receiver) | Opponent | Spectator | Status |
| --- | --- | --- | --- | --- |
| Life | `me.life` | `opponents[].life` | `players[].life` | exists |
| Hand count | `my_hand.length` | `opponents[].hand_size` | `players[].hand_size` | exists |
| Library count | `me.library_size` | `opponents[].library_size` | `players[].library_size` | exists |
| Graveyard count | `graveyards[].cards.length` | `opponents[].graveyard_size` **or** `graveyards[].cards.length` | `graveyards[].cards.length` | exists (two equivalent sources; prefer the pile for the receiver, the count for opponents) |
| Exile count | `exile[].cards.length` | same | same | exists |
| Command-zone count / presence | `command[].cards.length` | same | same | exists; elided in a non-Commander game |

### 10.3 Turn, priority, combat

| Displayed value | Source | Status |
| --- | --- | --- |
| Priority holder | `priority_player` (`Option<PlayerId>`) | exists; both views |
| Active player | `active_player` | exists; both views |
| Turn number | `turn` | exists |
| Phase / step | `phase` | exists |
| Attacked ring on a seat | any `battlefield[].attacking_player === id` | exists |
| Attacker count `×N` | count of the above | exists (filter, not computation) |
| Two-player fallback | `battlefield[].attacking === true` with the sole opponent as defender | exists (documented fallback for older servers) |
| Decision deadline | `action_deadline` | exists; **receiver-only by construction** |
| Auto-passed chip | `auto_passed` | exists; transient |
| Rejected action | `action_rejected` | exists; toast, not cluster state |

### 10.4 Commander

| Displayed value | Source | Status |
| --- | --- | --- |
| Incoming commander damage, per source | `commander_damage[]` filtered `damaged === id` | exists |
| Worst incoming value | max `amount` over that filter | exists (ranking of server values) |
| Damage source's seat identity | `commander_damage[].commander` (a `PlayerId`) | exists |
| Lethal threshold (21) | CR 903.10a constant, display only | constant |
| Actual elimination by commander damage | `result.reason === "commander_damage"` and/or `opponents[].eliminated` | exists |
| Commander tax | `commander_tax[].tax` where `commander === id` | exists |
| Commander casts | `commander_tax[].casts` | exists |
| Commander presence mark | presence of a `commander_tax` entry, or a non-empty `command` pile | exists |

### 10.5 Mana

| Displayed value | Source | Status |
| --- | --- | --- |
| Local unspent mana, grouped | `GameView.mana_pool: string[]` (pip strings) | exists |
| Opponent mana | **none — and none is wanted** | forbidden; adding it is a protocol decision, never an inference |
| Spectator mana | **none** — `SpectatorView` has no `mana_pool` | by construction |

### 10.6 Player state

| Displayed value | Source | Status |
| --- | --- | --- |
| Named statuses — opponent | `opponents[].statuses: string[]` | exists; **free-form display text only** |
| Named statuses — spectator | `players[].statuses` | exists |
| Named statuses — local | — | **GAP**: #544 — `SelfView` has only `life` and `library_size` |
| Status rules text / label | — | **GAP**: #544 `PlayerStatus.label` |
| Poison count | — | **GAP**: #544 `PlayerCounter { kind: "poison" }` |
| Poison lethal threshold | — | **GAP**: #544 `PlayerCounter.lethal_at` |
| Other player counters | — | **GAP**: #544 `PlayerCounter[]` |
| Eliminated — opponent | `opponents[].eliminated` | exists |
| Eliminated — spectator | `players[].eliminated` | exists |
| Eliminated — local | — | **GAP**: #553 — no `SelfView.eliminated`. `result.losers` only arrives at game over; a `log[]` `player_eliminated` entry is a bounded window and is not reconstructable, so it MUST NOT be the state source |
| Disconnected — any seat | — | **GAP**: #553 — the server holds a seat open across a disconnect (`room/broadcast.rs`) but publishes no per-seat connection field |
| AI-controlled seat marker | — | **GAP**: #553 — `SeatView.ai` exists in the **lobby** only and is not carried into `GameView` |
| Teams / format designations | — | **GAP**: #553 — not modelled; `ui-requirements.md` lists them as "when supported" |

### 10.7 Presentation-only inputs (never the displayed value)

| Use | Source |
| --- | --- |
| Life gain/loss animation | `log[]` `life_changed`, `damage_dealt` |
| Off-focus activity ping | `log[]` credited to a seat |
| Elimination moment staging | `log[]` `player_eliminated` |
| Verdict panel | `result` |

---

## 11. Blocked on protocol

The elements below are **fully specified and deliberately dormant**. Each ships
only when the named field exists. Until then the client renders nothing in that
slot — no placeholder, no zero, no "unknown".

| Specified element | Where | Needs | Issue |
| --- | --- | --- | --- |
| Poison chip and the tray's **Poison** segment (panel 4) | §3.1, §5.6, §6.8 | a structured numeric player counter, e.g. `PlayerCounter { kind, count, lethal_at? }` on self/opponent/spectator views | #544 |
| **Counters** segment and its brass hexagon (panel 4) | §3.1 | the same structured counter list, for non-poison kinds | #544 |
| Poison near-lethal warning shape | §5.1-style shape escalation | `PlayerCounter.lethal_at` — a **server-owned** threshold. The client MUST NOT hard-code 10 | #544 |
| Local player's named statuses | §5.6 | `statuses` (or #544's `PlayerStatus[]`) on `SelfView` | #544 |
| Status labels and descriptions in the popover | §5.6, §9 | `PlayerStatus.label` — today `statuses` is a bare string with no authored label | #544 |
| Disconnected link glyph | §2, §6.4 | a per-seat connection flag on `OpponentView`/`SelfView`/`SpectatorView.players` | #553 |
| Eliminated treatment on the **local** cluster | §2, §6.5 | `eliminated` on `SelfView` | #553 |
| Stable identity gems | §1.2 el. 6, §10.1 | a per-player colour identity (or commander identity) that survives the commander leaving the command zone | #553 |
| Commander name on the nameplate | §5.5 | as above | #553 |
| AI-seat marker | §10.6 | `ai` carried from `SeatView` into the in-game views | #553 |

**Standing prohibition.** Until a structured field exists, the client MUST NOT:

1. parse a number out of `OpponentView.statuses` (`"poison:3"`, `"poison 3"`,
   or any other shape);
2. count `log[]` entries to derive a counter, a poison total, or a life total;
3. infer a threshold, a lethality, or an elimination from any of the above;
4. show a zero or an "unknown" chip in a dormant slot.

The design is reserved so that landing #544 (structured player counters and
statuses) and #553 (game and seat state) is a data wire-up, not a redesign: the
geometry, placement, hit targets, and escalation shapes above do not change.

---

## 12. Decisions the baselines did not dictate

Every **[D]** in this document, collected:

1. **The rung ladder** — five D values (112 / 96 / 76 / 60 / 48 px) and which
   variant sits at each. The sheets show four sizes without labelling them.
2. **The library pip** — shape (the hand hexagon at 0.85 scale with a spine
   mark), placement (mirrored inboard), and its demotion to a pile badge at the
   compact and minimal rungs. The baselines draw only one count pip per
   cluster.
3. **Hand vs library assignment** — the free-standing hexagon and the
   under-slung shield tab are two attachment modes of the **hand** pip (the
   values in the 2.5D baseline track visible hand fans, not deck depth); the
   library pip is the new inboard hexagon.
4. **The eliminated variant** — stone rim, desaturated aperture, struck-rune
   glyph replacing the life numeral, struck nameplate.
5. **The disconnected variant** — broken-chain glyph at the inboard shoulder,
   70 % portrait luminance, nothing removed.
6. **The spectator case** — no local cluster at all; every seat renders in the
   focused/wing variant.
7. **The status rail** — its existence as an arc, its radius, its step angle,
   the 2 + `+N` cap, and server-array ordering.
8. **The active-turn pennant** — swallow-tail tab at 12 o'clock.
9. **The attacked treatment** — dashed orange ring at 0.52 D radius plus a
   count chip in rail slot 1.
10. **The decision-deadline arc** and its layering over the priority ring.
11. **The auto-passed chip.**
12. **Disclosure mechanics** — trigger set, local-tray-vs-opponent-popover
    split, ephemerality, no auto-open, dismissal, and the "no chevron when the
    tray would be empty" rule.
13. **Tray content by seat count** — chip pitch compression at 5 and 6 seats
    and stable `seat_order` chip positions.
14. **Zero rows in the opened matrix are shown** (reconciling panel 4's `0`
    chip with the issue's "do not render every zero tally": the prohibition
    applies to the collapsed cluster warning, the matrix is the full picture).
15. **Life urgency shapes** — the notched collar at ≤ 5 and the closed double
    ring at ≤ 0.
16. **Numeric overflow** — per-digit numeral step-downs and the `999+` pip
    truncation with the exact value in the accessible name.
17. **Name truncation** — 2.40 D max plate, middle ellipsis at 8 + 4 graphemes,
    `Seat N` fallback.
18. **Identity gem stacking** — gems stack along the nameplate's inboard end at
    1.05× pitch, capped at five.
19. **Commander-damage escalation shapes** at 15+, 18–20, 21+.
20. **Commander tax duplication rule** — the cluster shows the value only when
    the command pile is not drawn at the rung.
21. **Mana grouping exactness** — WUBRG-then-generic ordering, unknown pip
    strings on the generic gem, hybrid/Phyrexian kept as their own group.
22. **Mana reservoir wrap** rather than gem shrink at six kinds.
23. **Centre-anchored nameplate direction** — left, for the far side and the
    local cluster.
24. **Keyboard and touch model** — one tab stop per cluster with a roving group
    inside; the "no 44 px rect, no sub-element" rule.
25. **The accessible-name sentence** for the whole cluster.
26. **Portrait load-failure specification** — token aperture with no substitute
    glyph; the accessible player name and every non-art identity channel remain.

---

## 13. Conflicts and open questions

Recorded here rather than edited into the other documents.

1. **Life crest shape.** [`visual-system.md`](visual-system.md) §1.5 and §6 and
   [`ui-design-notes.md`](ui-design-notes.md) §Identity describe a **hexagonal**
   life crest. The approved baselines draw a **circular** life medallion, and
   use the vertical hexagon for the *count pips* instead. This document follows
   the baselines. `visual-system.md` §6 and `ui-design-notes.md` should be
   reconciled to "circular life medallion; hexagonal count pips".

2. **Seat accent palettes disagree three ways.** `visual-system.md` §2 lists six
   jewel tones (`#4D7EC9`, `#B0563F`, `#4F8F5C`, `#8B6FB0`, `#C08B3E`,
   `#4E9A9B`); `clients/web/src/table/identityAccents.ts` ships eight different
   values (teal, rose, periwinkle, olive, slate cyan, clay, sage, heather); the
   baselines draw the identity gems in what read as **card colours** (green,
   blue, purple), not seat accents. This document treats the gem as **colour
   identity** (per the issue's anatomy list) and the rim/nameplate accent as
   **seat identity** — two separate channels — but the palette source of truth
   needs one owner.

3. **Five-seat wing split.** `layout-model.md` §Staging specifies 2 left / 1
   right; `rune-battlefield-environments.jpg` panel 4 draws 1 left / 2 right.
   The mirror is cosmetically arbitrary but must be pinned in one place —
   `plane-slots.test.ts` currently encodes the layout-model form.

4. **Seat counts drawn in the environment sheet exceed the label.** Panels 4–5
   draw more clusters than "5 players" / "6 players" implies. The sheet is
   read here as an *anchor pattern* (local bottom centre, focused top centre,
   wings stepping outward down each arc), not a seat census.

5. **Zero commander-damage rows.** §5.4 reconciles the issue's "do not render
   every zero tally" with panel 4's visible `0` chip by scoping the prohibition
   to the collapsed cluster. If the maintainer prefers the matrix to hide zeros
   too, only §3.1 and §5.4 change.

6. **`graveyard_size` vs the graveyard pile.** Both exist and can disagree
   during a partial update. This document prefers the pile for the receiver and
   the count for opponents; a protocol note stating which is canonical would
   remove the ambiguity.

7. **Commander identity is only knowable while the commander is in the command
   zone.** Both the identity gems and the nameplate's commander line flicker
   when the commander is cast. A per-player commander/colour-identity field
   would fix both; tracked by #553.

8. **Local player state is systematically thinner than opponent state.**
   `SelfView` carries `life` and `library_size` only, while `OpponentView`
   carries `hand_size`, `graveyard_size`, `statuses`, and `eliminated`. #544
   covers statuses/counters; `eliminated` on `SelfView` is covered by #553.

9. **Browser verification.** Nothing in this document has been validated in a
   real browser. Ring bloom against the environment plates, the 44 px rects at
   the minimal rung, and the tray's overlap with the hand fan at the 1280×800
   floor are all pixel questions that belong to the maintainer.
