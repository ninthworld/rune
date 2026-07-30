# Archive

Superseded documentation from the RUNE era, retained as history.

**Nothing here is authoritative.** Do not cite it as a requirement, do not implement from it,
and do not treat an open issue that references it as authorization. Current guidance lives in
[`../brief.md`](../brief.md), [`../../AGENTS.md`](../../AGENTS.md), and
[`../decisions/`](../decisions/).

## What's here and why it was archived

- **`design/`** — ~7,000 lines of UI and visual specification (control language, card
  representation, stack and relationships, seat identity, environment system, zone geography,
  layout model, presentation budgets, visual system, 2.5D findings, front door and lobby, UI
  blueprint and requirements). Written largely ahead of the code it described, for a client
  that was rebuilt three times and has now been deleted. It is the clearest artifact of the
  design-doc-first failure the restart exists to correct.

- **`decisions/`** — 17 ADRs that no longer bind. Mostly presentation-shaped (the DOM/canvas
  split, chrome styling, fixed and contextual shell anatomy, the 2.5D direction and
  architecture, bundled asset policy, direct entity activation), plus decisions superseded by
  later ones (0013 by 0018) or scoped out of the current milestone (spectator view model, deck
  persistence, AI opponents).

- **`roadmap.md`** — the milestone document. It marked M4 "Readable games" and M5 "More than
  two" as *Complete* for a product that was never playable or fun, which is precisely how
  "complete" drifted from "good." SAGE deliberately has no roadmap document.

Two ADRs that the pre-restart rules had sidelined are **live** in `../decisions/` rather than
archived here: 0011 (browser e2e, whose suite was removed three times and is now a required
gate again) and 0024 (player-side, opt-in, device-local card art).
