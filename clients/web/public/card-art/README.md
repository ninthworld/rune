# Bundled card art

Project-owned card illustrations for the **bundled** art source (ADR 0024). Each
WebP file has a content-hashed name, and `manifest.json` maps a catalog card's
stable `functional_id` to that filename. The client only requests paths the
manifest names.

Rules for anything added here:

- **Project-owned originals only** — art RUNE generated or commissioned and may
  redistribute under this repository's terms. Never official card images,
  frames, symbols, or any Wizards of the Coast asset (`docs/brief.md`, Legal
  constraints).
- Landscape crops around 626×457 or larger render best in the card frame's art
  window; the renderer cover-crops to fit.
- Add the functional id and content-hashed filename to `manifest.json` in the
  same change that adds the image.
- Record every image in `src/assets/ledger.json` and its human-readable mirror.

The first bundled set is Ember Onslaught's eight unique cards. Cards outside
that set continue to use the procedural art-window treatment.
