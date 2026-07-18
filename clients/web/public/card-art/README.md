# Bundled card art

Project-owned card illustrations for the **bundled** art source (ADR 0024). Each
file is named `<functional_id>.jpg` after its catalog card
(`crates/rune-engine/data/catalog/`), and `manifest.json` lists the functional
ids that have art — the client only requests files the manifest names.

Rules for anything added here:

- **Project-owned originals only** — art RUNE generated or commissioned and may
  redistribute under this repository's terms. Never official card images,
  frames, symbols, or any Wizards of the Coast asset (`docs/brief.md`, Legal
  constraints).
- Landscape crops around 626×457 or larger render best in the card frame's art
  window; the renderer cover-crops to fit.
- Add the functional id to `manifest.json` in the same change that adds the
  image.

The directory currently ships empty: the manifest is `[]` and every card renders
its procedural face until the RUNE-generated set lands.
