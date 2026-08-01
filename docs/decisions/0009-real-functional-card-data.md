# ADR 0009: Real functional card data from a single set

- Status: accepted
- Date: 2026-07-30

## Context

The card model stores only functional data — name, types, mana cost, power/toughness, and an
ability IR — and player-facing rules text is *generated* by the server from that same data
(ADR 0008). Nothing about a real card's name or characteristics is a presentation asset.

That leaves a choice about what the bundled catalog actually contains: invented stand-in cards
with made-up names, or the names and characteristics of real cards. Invented names are the
more conservative option, but they are conservative about the wrong thing — they buy no legal
protection the schema does not already provide, and they cost the project its ability to be
recognized as Magic by anyone playing it.

The legal policy (`docs/brief.md`, Legal constraints) prohibits shipping **presentation
assets**: card images and artwork, official frames, symbols, watermarks, Wizards branding, and
exact Oracle or flavor text. It also states that weakening any constraint requires an explicit
decision. This is that decision, scoped narrowly.

## Decision

The bundled catalog draws its **functional card data** — card *names* and their mechanical
characteristics (types, subtypes, mana cost, colors, power/toughness, and IR-modeled
abilities) — from a single real Magic set, **Core Set 2019 (M19)**. The starter decks in
`data/starter-decks.json` are built from that catalog.

This governs **only** which names and numbers the definitions carry. Every presentation
prohibition stays in force:

- **No Oracle text or flavor text.** Definitions have no rules-prose field, and the
  `deny_unknown_fields` schema rejects one. Player-facing text is generated from the ability
  IR by `crates/sage-server/src/rules_text.rs`, so it is the project's own phrasing, never a
  card's copied wording.
- **No art, frames, symbols, or branding.** Printing records carry only `functional_id`,
  `collector_number`, and `rarity`.
- **No monetization, and no implied affiliation.**

A card enters the catalog only if its function is expressible in the existing ability IR and
rendered by the exhaustive text generator, **and its characteristics match the printing**:
name, mana cost, colors, types, supertypes, subtypes, and power/toughness are copied, not
recalled. A card whose real function the IR cannot say does not enter the catalog in a
weakened form — a definition that is *nearly* the card is worse than no card, because a
player cannot tell which one they are holding.

IR shapes that no clean M19 card exercises — P/T Auras, keyword-only Auras,
`enters_with_counters`, a bare dies-draw trigger, first strike, double strike, `lose_life`,
colorless mana — remain valid vocabulary and keep full test coverage through inline `test_*`
definitions built in tests, not through shipped cards.

### The exception is closed

This decision briefly carried one: `jedit_ojanen`, a non-M19 legendary creature kept as the
commander fixture because the CR 903 flow needs a legend and no M19 legend was expressible at
the time. It was to go "the moment either an expressible M19 legend exists or the single-set
boundary widens", and the first of those has happened.

**Lathliss, Dragon Queen** (M19 #149) is now in the catalog and is the commander fixture, so
`jedit_ojanen` is deleted and **every definition in the catalog is an M19 card**. Two small
IR additions were what it took: a `nontoken` filter on an observed-permanent selector
("whenever another **nontoken** Dragon you control enters") and a subtype on a mass effect
("**Dragons** you control get +1/+0 until end of turn").

The card carried a second lesson worth recording: the definition it was deleted from did not
match the real Jedit Ojanen either — the printed card is a `{4}{W}{W}{U}` white-blue Cat
Warrior, and the catalog held a `{4}{G}{G}` green one. A fixture invented under a real card's
name is exactly the failure this ADR's "copied, not recalled" rule exists to prevent, and it
survived because nothing checked the catalog against the set it claims to come from.

## Consequences

- The shipped catalog and decks read as recognizable Magic, which is the entire point: a
  playtest against invented cards cannot tell you whether the game feels right.
- `docs/brief.md` Legal constraints records the distinction explicitly: card *names and
  functional characteristics* may match real cards; *presentation assets* remain prohibited.
- Engine tests for the handful of IR shapes M19 does not exercise use inline scaffolds, so the
  shipped catalog stays entirely real cards.
- The single-set restriction is a scope boundary, not a legal one. Widening it is ordinary
  work; what it must not do is bring presentation assets along.
- This licenses nothing further. Any weakening of the presentation prohibitions requires its
  own explicit decision.
