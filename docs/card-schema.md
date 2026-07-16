# Card schema

How a card is authored. The model is [ADR 0018](decisions/0018-scalable-functional-card-definitions.md)
(functional definitions, `FunctionalId`, `schema_version`) on top of
[ADR 0013](decisions/0013-card-identity-and-set-model.md)'s oracle-vs-printing split.
The types are `CardData` and `Printing` in `crates/rune-engine/src/card.rs`.

## Where cards live

- **`crates/rune-engine/data/catalog/<functional_id>.json`** — one **functional
  definition** per file: the printing-independent rules object for one card, no matter
  how many sets print it. The file name *is* the card's identity, and the build rejects
  a file whose `functional_id` disagrees with it (ADR 0018 §4).
- **`crates/rune-engine/data/sets/<SET>.json`** — an array of **printing records**: the
  bibliographic appearances of those cards in a set. A printing carries no rules, so a
  reprint is one new record here and zero changes anywhere else. The set code comes from
  the file name.

One card per file is what makes the catalog scale: **adding a card edits zero existing
lines**, in data or in Rust, so two people authoring different cards never touch the
same file.

Nothing enumerates these files by hand. `crates/rune-engine/build.rs` walks both
directories at compile time, validates every file, interns a `CardId` for each
definition, and generates the `include_str!` manifest that `card.rs` includes. The
engine still does **no I/O at runtime** — the build script runs on the machine doing the
building, and all the shipped binary sees is `&'static str` constants, exactly as
[ADR 0006](decisions/0006-serde-in-engine.md) sanctioned. Only *who writes the
`include_str!` list* changed: a build script, not a human.

## A functional definition

```json
{
  "schema_version": 1,
  "functional_id": "verdant_scout",
  "name": "Verdant Scout",
  "types": ["creature"],
  "subtypes": ["Elf", "Scout"],
  "mana_cost": "{G}",
  "colors": ["green"],
  "power": 1,
  "toughness": 1,
  "abilities": [
    {
      "type": "triggered",
      "event": "self_enters_battlefield",
      "effects": [{ "kind": "draw_card", "count": 1 }]
    }
  ]
}
```

| Field | Required | What it is |
|---|---|---|
| `schema_version` | yes | The schema this definition is authored against. Must equal `rune_engine::SCHEMA_VERSION` (currently `1`); anything else fails the load. |
| `functional_id` | yes | The card's authored, stable identity: a lowercase `snake_case` slug, unique across the catalog, conventionally the slug of its name (`"Thornback Boar"` → `thornback_boar`), and **matching the file name**. Assign it once; never reuse or renumber it. |
| `name` | yes | The card's name. |
| `types` | yes | Printed card types (`creature`, `land`, `instant`, …). At least one. |
| `supertypes` | no | Printed supertypes (`basic`, `legendary`). Empty by default. |
| `subtypes` | no | Printed subtypes, as printed (`"Elf"`, `"Aura"`). Empty by default. |
| `mana_cost` | yes | Curly-brace notation (`"{2}{G}"`); empty for a card with no mana cost. |
| `colors` | no | The card's colors (CR 105.2), authored explicitly — never re-derived from the cost's pips at runtime, so a colorless-cost-but-colored card is representable. Empty (colorless) by default. |
| `power` / `toughness` | no | Printed P/T, for creatures. Absent for non-creatures. |
| `keywords` | no | Printed keyword abilities (CR 702): `flying`, `reach`, `vigilance`, `haste`, `first_strike`, `trample`, `deathtouch`, `lifelink`. |
| `abilities` | no | The ability IR (ADR 0007): `activated`, `triggered`, `enters_tapped`, `enters_with_counters`. |
| `spell_effects` | no | What an instant/sorcery does on resolution (CR 608.2c), in the same effect IR. |
| `aura` | no | An Aura's enchant restriction and static P/T grant (CR 303.4). Only on a card whose `subtypes` include `"Aura"`. |
| `scripted` | no | `false` by default. `true` declares that this card's behavior is (also) defined in code, in `crates/rune-engine/src/scripted.rs` — the ADR 0007 escape hatch. No bundled card is scripted today. |

The `abilities`, `spell_effects`, and `aura` shapes are the engine's IR and are
documented where they are defined: `crates/rune-engine/src/ability.rs`. Mana
production is authored as an effect: `add_mana` (one of the five colors, e.g.
`{ "kind": "add_mana", "color": "green", "amount": 1 }`) or `add_colorless_mana`
for a mana rock's `{C}` (e.g. `{ "kind": "add_colorless_mana", "amount": 1 }` —
colorless is not a `Color`, so it is a distinct effect).

## What a definition may not contain

The schema is **closed**: `deny_unknown_fields` rejects any field not listed above, so
the load fails rather than silently ignoring it. That is how the legal posture is
enforced structurally rather than by review — no exact Oracle text, flavor text, image
URI or asset path, official symbol, frame, watermark, or artist credit can enter the
catalog, by accident or otherwise (`docs/brief.md` Legal Considerations, ADR 0018 §2).

**No rules prose either — not even your own.** A definition has no text field at all.
The words a player reads are *generated* from `abilities`, `spell_effects`, `aura`, and
`keywords` by the server's formatter (`crates/rune-server/src/rules_text.rs`, ADR 0018
§7) and projected as `CardView.rules_text`. A card's displayed text therefore cannot
drift from its behavior: there is nothing to keep in sync. Author the IR correctly and
the text follows.

The one exception is a `scripted` card, whose behavior is Rust the formatter cannot
read: it authors its own text beside its code, in `crates/rune-engine/src/scripted.rs`.
The loader enforces that pairing in **both** directions — a definition claiming
`scripted: true` with no code arm fails to load, and so does a card with a code arm
whose definition does not declare it (ADR 0018 §5).

## A printing record

```json
{ "functional_id": "verdant_scout", "collector_number": "12", "rarity": "rare" }
```

Three fields, all required: the `functional_id` of the card being printed, the
collector number within the set, and the rarity (`common`, `uncommon`, `rare`,
`mythic`). `deny_unknown_fields` applies here too. The loader resolves `functional_id`
to that build's interned handle, so a reference to a card that does not exist fails the
load — it can never surface as a missing card mid-game.

## Identity: which id is which

Four layers, only two of them authored by hand (ADR 0018 §3, and the module docs in
`crates/rune-engine/src/id.rs`):

| Layer | Type | Assigned by | Stable for |
|---|---|---|---|
| Functional | `FunctionalId` | the card's author | forever |
| Interned handle | `CardId` (aliased `OracleId`) | `build.rs` | one build |
| Printing | set code + collector number | the set file | forever |
| Per-game instance | `CardInstanceId`, `PermanentId` | the engine, at runtime | one game |

**Never write a `CardId` down.** `build.rs` sorts every `FunctionalId` by byte value and
interns `CardId(0)`, `CardId(1)`, … in that order, so authoring one new card renumbers
its neighbours — an integer that means Thornback Boar today means something else
tomorrow. Two authors adding cards in the same PR wave therefore cannot collide on an
id, because nobody assigns one. Reference a card from a printing, a decklist, or a test
by its `FunctionalId` and resolve the handle with `CardDatabase::card_id`. This is also
why `scripted.rs` keys its escape-hatch arms on `FunctionalId` rather than `CardId`.

## Adding a card

1. Create `data/catalog/<functional_id>.json` with `schema_version: 1` and a
   `functional_id` matching the file name. No `id` — the handle is interned for you.
2. Add a printing record to a set file if the card should be printed somewhere.
3. Add tests for the behavior in the same PR (`crates/rune-engine/AGENTS.md`).
4. Run `make check`.

You will not edit an existing line to do this.

## Where each rule is enforced

Every rule below is checked, and the check runs where it can fail earliest. The
validators themselves live in one file, `crates/rune-engine/src/catalog.rs`, which
`build.rs` and the engine both compile — so a rule cannot pass at build time and fail at
load time, or vice versa.

| Rule | Enforced by |
|---|---|
| Unknown field (a presentation asset, or a hand-written `id`) | `deny_unknown_fields`, at parse |
| An effect that needs a `TargetSpec` and has none | the type system — `target` is a required field, so it is unrepresentable |
| Unrecognized `schema_version` | `build.rs`, and the loader |
| Malformed slug; `functional_id` ≠ file name; duplicate identity | `build.rs`, and the loader |
| A `Creature` without power/toughness (or a non-creature with them) | `build.rs`, and the loader |
| An `aura` grant on a card that is not an Aura | `build.rs`, and the loader |
| A printing referencing a card that does not exist; two printings sharing a collector number | `build.rs`, and the loader |
| A `scripted` flag that disagrees with `scripted.rs`, **in either direction** | the loader only |

The last row is the one exception, and it is a structural one: the code tier is compiled
Rust, which does not exist yet when `build.rs` runs, so the build script cannot see it.
`CardDatabase::bundled()` owns that check instead — which means a mismatch fails
`cargo test` and the server's startup, never a game already in progress.

A build-time failure names the file and the problem
(`data/catalog/no_pt.json: no_pt is a Creature with no power/toughness`). A load-time
failure is a `CatalogError` naming the offender.

## Versioning

`schema_version` is a whole-catalog forcing function. A breaking change to the shape of
a definition — a renamed field, a restructured `abilities` encoding — bumps
`rune_engine::SCHEMA_VERSION` and migrates every definition in the same change. A
definition declaring a version the engine does not recognize is a hard error, never a
silent skip, so a half-migrated catalog cannot boot.
