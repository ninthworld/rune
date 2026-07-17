# Bundled display face — "RUNE Display" (issue #322)

The identity display face behind the `--rune-font-display` token (wordmark,
victory/defeat, phase names). Bundled with the client, never fetched over the
network.

| | |
|---|---|
| **Bundled family** | `RUNE Display` (declared in `../tokens.css` via `@font-face`) |
| **Upstream font** | Rajdhani, SemiBold weight |
| **Designer / foundry** | Indian Type Foundry |
| **Source** | https://github.com/google/fonts — `ofl/rajdhani/Rajdhani-SemiBold.ttf` |
| **License** | SIL Open Font License, Version 1.1 — see [`OFL.txt`](./OFL.txt) |
| **Reserved Font Name** | None declared in the upstream copyright notice. |
| **Bundled file** | `rune-display.woff2` (~14 KB) |

## Why Rajdhani

Angular, geometric, and rune-adjacent without the legibility problems of a
novelty face — a natural fit for RUNE's procedural-geometry identity, and a clean
successor to the previous geometric system stack (`Avenir Next` / `Century Gothic`
/ `Futura`), which remains the `--rune-font-display` fallback.

## Modifications (OFL §1–2)

`rune-display.woff2` is a **modified** copy of `Rajdhani-SemiBold.ttf`, produced by
`fontTools`:

1. **Subset** to Basic Latin (U+0020–007E), the Latin-1 Supplement (U+00A0–00FF),
   and a few punctuation marks (en/em dash, curly quotes, ellipsis, `×`) — the
   character set the identity moments need. Raw TTF ~390 KB → WOFF2 ~14 KB.
2. **Renamed** the font family to `RUNE Display` so the subset is never mistaken
   for, or shadow, an installed copy of the upstream Rajdhani family.
3. **Converted** to WOFF2 for the web bundle.

The OFL permits modification and redistribution; the license text travels with the
asset in `OFL.txt`, as the license requires. Rajdhani declares no Reserved Font
Name, so the rename is a courtesy for clarity rather than a strict requirement.

To reproduce: fetch `Rajdhani-SemiBold.ttf` from the source above and re-run the
subset (subset unicodes as listed, rename family to `RUNE Display`, save as WOFF2).
