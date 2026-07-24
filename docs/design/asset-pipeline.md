# Asset and effects pipeline — groundwork

The pipeline groundwork for the 2.5D client (issue #471, the last Phase 0
issue of #464). Policy — provenance classes, the ledger, delivery rules —
is decided in [ADR 0031](../decisions/0031-bundled-asset-policy.md); this
document is the working reference: what assets exist, what formats and
budgets they must fit, how generic effects are keyed to game events, and
what falls back when data is missing. Style authority is
[`visual-system.md`](visual-system.md); size and quality ceilings are
[`presentation-budgets.md`](presentation-budgets.md).

## Asset inventory

| Category | Consumer | Form | Budget/notes |
| --- | --- | --- | --- |
| Battlefield environments | environment layer (ADR 0030 L1) | 2–4 layered rasters per theme (sky, far ground, arena edge) | ≤ 1.5 MB/theme; three launch theme concepts in visual-system §4 |
| Player/commander portrait treatment | crest clusters | raster portraits in the crest mask, procedural monogram fallback | player-selected art stays device-local per ADR 0024; bundled portraits are class-1/2 originals |
| Card backs | piles, hidden cards, travel ghosts | one raster + procedural fallback | shared across themes |
| Badges, icons, cursors, selection treatments | scene + chrome | SVG / the existing glyph language, extended | glyphs stay single-source (`chrome/glyphs/geometry.ts` model) |
| Effect sprites/sheets | effects layer (WebGL) | packed atlases | pooled; caps per quality level (High ≤ 400 live particles, Standard ≤ 150, Lite ≤ 40) |
| Audio (later) | audio hooks | Opus + AAC fallback | event taxonomy below; independently muted; never load-bearing |
| Fonts | identity moments | WOFF2 | ≤ 60 KB total (today ~14 KB) |

Everything above ships with the client bundle, content-hashed and cached
forever; a match is fully playable before any optional asset arrives, and
every category has a procedural fallback so a missing asset degrades to
the shipped client's procedural language, never to a hole.

## Provenance, licensing, attribution

See ADR 0031 for the binding rules. Working summary: three provenance
classes (original commissioned, original AI-generated with recorded
prompt/tool, or CC0/CC-BY/OFL third-party), a machine-readable ledger
beside the assets that CI enforces once the first asset lands, and the
unchanged absolute prohibitions on official imagery or anything derived
from it. Attribution for class-3 assets surfaces in a client credits
screen (owed with the first such asset).

## The generic effect taxonomy

Effects are **data-driven categories keyed to game events, never bespoke
per card** (#464 non-goal: no per-card animation). An effect invocation is
a category plus parameters the client already has: source rect, target
rect(s), seat accent, frame color, and magnitude. Cards without any effect
metadata get their category's default — the fallback is always defined.

Two trigger channels exist today, and the taxonomy is honest about which
feeds which:

**Channel A — `GameLogEvent` entries** (`rune-protocol/src/log.rs`,
ADR 0021): the authoritative event stream.

| Effect category | Log event(s) | Grammar class (visual-system §8) |
| --- | --- | --- |
| cast-departure | `SpellCast` | Cast (goes to stack) |
| resolution | `SpellResolved` | Resolve |
| counter/fizzle | `SpellCountered`, `SpellFizzled` | Countered / fizzle |
| combat-declaration | `AttackersDeclared`, `BlockersDeclared` | Declare attacker / blocker |
| impact | `DamageDealt` | Combat damage / loss moment |
| life-delta | `LifeChanged` | Healing/growth (gain) or loss moment |
| death | `PermanentDied` | Lethal / destruction |
| draw | `CardsDrawn` | Draw |
| flow | `StepChanged` | Phase / turn transitions |
| elimination | `PlayerEliminated` | Concede / defeat treatment |
| command-zone-return | `CommanderReturnedToCommandZone` | Zone travel variant |
| verdict | `GameOver` | Victory / defeat |
| pregame | `Mulligan`, `HandKept` | Mulligan |

**Channel B — view diffs**: tap/untap, counters and P/T deltas, token
appearance, attachment changes, and general zone movement have **no log
event today** — the shipped client animates them from successive
`GameView`s (the reconciler's diff), and the effect layer keys on the same
diffs. This works without any protocol change and is the v1 mechanism.

**The protocol gap, stated for later:** if Phase 2+ wants effect fidelity
that a diff cannot supply (e.g. *why* a permanent left — destroyed vs
sacrificed vs exiled — to pick a category), that is a protocol change
(new log events), to be proposed then under the normal contract rules —
not assumed here.

Audio and haptic hooks share this exact taxonomy: one event category → an
optional sound/haptic id, independently muted, never load-bearing
(the visual + log channels always stand alone).

## Delivery, caching, versioning

Per ADR 0031 and the budgets: content-hashed filenames, cache-forever,
lazy-loaded beyond the first-match set (default theme + UI assets ≤ 4 MB),
no separate CDN in v1 — assets version with the client bundle, so a
deployed client and its assets are always coherent and reconnect never
races an asset version. Quality levels select variants at load where a
category provides them (e.g. reduced-resolution environment set for Lite);
the procedural floor needs no assets at all.

## Out of scope / open items

- Production sourcing (who creates the launch themes and portraits) and
  the credits surface — owed when the first assets land, tracked in
  Phase 1+ issues.
- The bundled RUNE card-art set (original illustrations filling the
  bundled source's manifest) remains the existing roadmap follow-up under
  ADR 0024's framework, unchanged by this pipeline.
- Channel-B protocol enrichment (zone-change reasons) — a possible Phase 2+
  contract change, listed above, not decided.
- The ledger CI gate lands with the first real asset (a gate with nothing
  to check would be dead weight today).
