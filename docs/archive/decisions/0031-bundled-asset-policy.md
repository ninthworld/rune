# ADR 0031: Bundled presentation assets — provenance, licensing, and delivery

- Status: accepted
- Date: 2026-07-24

## Context

Until now the client has shipped essentially no presentation assets: cards,
glyphs, and chrome are procedural, and the only bundled binary is the ~14 KB
OFL display face (issue #322). The 2.5D direction (ADR 0029) and its visual
system (issue #469) introduce asset-bearing presentation for the first time:
illustrated environments, portrait/crest art, card backs, effect sprites,
and eventually audio. The project's legal posture
([`../brief.md`](../brief.md) Legal constraints) is deliberately
conservative, ADR 0024 governs the only player-side exception, and the
budgets ([`../design/presentation-budgets.md`](../design/presentation-budgets.md))
cap what may ship. Issue #471 asked for the policy before any production
asset lands.

## Decision

The project may ship **original bundled presentation assets**, under the
following binding rules.

### Provenance classes (exhaustive)

An asset may enter the repository only as one of:

1. **Original commissioned/created work** — made for RUNE, with rights
   granted in writing for distribution under the project's MIT-licensed
   releases.
2. **Original AI-generated work** — already permitted for RUNE's own
   presentation by the blueprint's art policy; the generation tool and the
   essence of the prompt are recorded in the ledger. Prompts and workflows
   **must not** reference Magic: The Gathering, Arena, Wizards of the Coast,
   or any artist's name or protected property as a style target — the
   output must be original in the same sense commissioned work is.
3. **Permissively licensed third-party work** — only CC0, CC-BY, or
   SIL OFL (fonts); attribution obligations recorded in the ledger and
   honored in the client's credits surface. No NC/ND/SA variants, no
   GPL-family asset licenses, no "free for personal use".

Anything outside these classes requires amending this ADR. The standing
prohibitions are unchanged and absolute: no official card images, frames,
symbols, watermarks, branding, or Oracle/flavor text, and no asset
*derived from* them, in the repository, the built client, or the server.
ADR 0024's player-side, device-local card-art pipeline remains the only
exception, unchanged.

### The asset ledger

Every bundled asset has an entry in a machine-readable ledger
(`clients/web/src/assets/ledger.json`, mirrored in a human-readable
`ASSETS.md` beside it): path, title, category, provenance class,
author/tool, license, source URL where applicable, and the AI
prompt/tool record for class 2. **CI fails if a file exists in the asset
tree without a ledger entry** (the gate lands with the first real asset).
An asset without provenance is a build error, not a review comment.

### Delivery and size

- Formats: AVIF or WebP for raster (PNG fallback only where a consumer
  requires it), SVG for line/geometry work, WOFF2 for fonts, packed sprite
  atlases for effect sheets; audio (later) as Opus with an AAC fallback.
- Filenames are content-hashed and served cache-forever with the client
  bundle; there is no separate asset CDN in v1.
- Budget ceilings bind: one environment theme ≤ 1.5 MB compressed, total
  first-match download ≤ 4 MB at default quality, fonts ≤ 60 KB total. A
  match must be fully playable before any optional asset arrives, and card
  play never blocks on art (procedural fallback always renders).
- Working/source files (PSD, project files, raw renders) are **not**
  committed to this repository; the repo carries only the compressed
  shipping form. Total committed presentation assets stay under **12 MB**
  before this ADR must be revisited (repo-clone weight is a project
  concern; `docs/ui-concepts/` reference imagery is separate and stays
  curated).

## Consequences

- Production art can be sourced (commissioned, generated, or adopted from
  CC0/CC-BY) without per-asset policy debates; review checks the ledger
  entry, not the asset's story.
- The effect pipeline and inventory in
  [`../design/asset-pipeline.md`](../design/asset-pipeline.md) build on
  these classes; the credits surface becomes a small client feature owed
  when the first class-3 asset lands.
- The conservative fan-project posture is preserved verbatim; this ADR
  adds original content capability without weakening any prohibition
  (per the brief, weakening would require explicit legal review — none is
  performed or implied here).
