# ADR 0026: Real functional card data from a single set

- Status: accepted
- Date: 2026-07-19
- Issue: N/A (product direction)

## Context

Since [ADR 0013](0013-card-identity-and-set-model.md) and
[ADR 0018](0018-scalable-functional-card-definitions.md) the bundled catalog has shipped an
*invented* starter set: functional stand-in cards with made-up names (Thornback Boar,
Quickfire Bolt, …). The invented names were a conservative choice, not a requirement of the
data model — the schema stores only functional data (name, types, mana cost, power/toughness,
and an ability IR), and player-facing rules text is *generated* by the server
([ADR 0018 §7](0018-scalable-functional-card-definitions.md)); nothing about a real card's
name or characteristics is a presentation asset.

The project's legal policy ([`docs/brief.md`](../brief.md) "Legal constraints") prohibits
shipping **presentation assets**: card images and artwork, official frames/symbols/watermarks
or WotC branding, and *exact Oracle text or flavor text*. It also states that any weakening of
those constraints "requires an explicit legal review and architectural decision." This ADR is
that decision, scoped narrowly.

## Decision

The bundled catalog draws its **functional card data** — card *names* and their mechanical
characteristics (types, subtypes, mana cost, colors, power/toughness, and IR-modeled
abilities) — from a single real Magic set, **Core Set 2019 (M19)**, rather than from invented
names. The starter decks ([`clients/web/src/starter-decks.json`](../../clients/web/src/starter-decks.json))
are built from that catalog.

This changes **only** which names and numbers the functional definitions carry. Every existing
presentation prohibition stays in force and is unchanged:

- **No Oracle text or flavor text.** Definitions still have no rules-prose field; the closed
  `deny_unknown_fields` schema still rejects one. Player-facing rules text is still *generated*
  from the ability IR by `crates/rune-server/src/rules_text.rs`, so it is the engine's own
  phrasing, never a card's copied Oracle wording.
- **No art, frames, symbols, or branding.** Printing records still carry only
  `functional_id` / `collector_number` / `rarity`; the opt-in, device-only art pipeline of
  [ADR 0024](0024-user-side-card-art.md) is the only path to real images and is unchanged
  (with real card names, a card now resolves its art by its own name, so the stand-in map is
  empty by default).
- **No monetization, no implied affiliation.**

A card is chosen for the catalog only if its function is expressible in the existing ability
IR (`crates/rune-engine/src/ability.rs`) and rendered by the exhaustive text generator. IR
shapes that no clean M19 card uses (P/T Auras, `enters_with_counters`, a bare dies-draw
trigger, first strike, deathtouch, `lose_life`, colorless mana) remain valid vocabulary and
keep full coverage through **inline `from_json` test scaffolds** (test-only definitions named
`test_*`), not through shipped cards.

## Consequences

- The shipped catalog and decks read as recognizable Magic, which is the point.
- The invented-starter-set language in [ADR 0013 §5](0013-card-identity-and-set-model.md) and
  the "invented" framing in [ADR 0018](0018-scalable-functional-card-definitions.md) /
  [ADR 0024](0024-user-side-card-art.md) is superseded by this ADR for the bundled catalog.
- `docs/brief.md` "Legal constraints" is clarified: card *names and functional
  characteristics* may match real cards; *presentation assets* (Oracle/flavor text, art,
  frames, branding) remain prohibited.
- Engine feature tests that previously leaned on purpose-built keyword/ability fixtures now
  use inline `test_*` scaffolds for the handful of IR shapes M19 does not exercise; the
  shipped catalog stays 100% real cards.
- This does not license copying Oracle text, art, or branding. Any further weakening of the
  presentation prohibitions still requires its own explicit decision.
