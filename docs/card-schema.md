# Card schema

How a card is authored. The model is [ADR 0018](decisions/0018-scalable-functional-card-definitions.md)
(functional definitions, `FunctionalId`, `schema_version`) on top of
[ADR 0013](decisions/0013-card-identity-and-set-model.md)'s oracle-vs-printing split.
The types are `CardData` and `Printing` in `crates/rune-engine/src/card.rs`.

## The two files

- **`crates/rune-engine/data/oracle.json`** — an array of **functional definitions**:
  the printing-independent rules object for each card, one per distinct card no matter
  how many sets print it. (ADR 0018 §4 shards this into one file per card; that is
  issue #193, and it changes the layout, not the schema below.)
- **`crates/rune-engine/data/sets/<SET>.json`** — an array of **printing records**: the
  bibliographic appearances of those cards in a set. A printing carries no rules, so a
  reprint is one new record here and zero changes anywhere else.

Both are embedded at compile time with `include_str!` and parsed in memory. The engine
does no I/O at runtime — see [ADR 0006](decisions/0006-serde-in-engine.md).

## A functional definition

```json
{
  "schema_version": 1,
  "id": 6,
  "functional_id": "verdant_scout",
  "name": "Verdant Scout",
  "types": ["creature"],
  "subtypes": ["Elf", "Scout"],
  "mana_cost": "{G}",
  "colors": ["green"],
  "oracle_text": "",
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
| `functional_id` | yes | The card's authored, stable identity: a lowercase `snake_case` slug, unique across the catalog, conventionally the slug of its name (`"Thornback Boar"` → `thornback_boar`). Assign it once; never reuse or renumber it. |
| `id` | yes | **Transitional.** The integer handle the loader interns this definition under. It is not authored card data, and it goes away when `build.rs` assigns handles (issue #193). Nothing outside the engine may depend on its value. |
| `name` | yes | The card's name. |
| `types` | yes | Printed card types (`creature`, `land`, `instant`, …). At least one. |
| `supertypes` | no | Printed supertypes (`basic`, `legendary`). Empty by default. |
| `subtypes` | no | Printed subtypes, as printed (`"Elf"`, `"Aura"`). Empty by default. |
| `mana_cost` | yes | Curly-brace notation (`"{2}{G}"`); empty for a card with no mana cost. |
| `colors` | no | The card's colors (CR 105.2), authored explicitly — never re-derived from the cost's pips at runtime, so a colorless-cost-but-colored card is representable. Empty (colorless) by default. |
| `oracle_text` | yes | **Transitional.** Hand-authored prose. Deleted once the server generates fallback rules text from the IR below (ADR 0018 §7, issue #194); no functional definition holds prose after that. |
| `power` / `toughness` | no | Printed P/T, for creatures. Absent for non-creatures. |
| `keywords` | no | Printed keyword abilities (CR 702): `flying`, `reach`, `vigilance`, `haste`, `first_strike`, `trample`, `deathtouch`, `lifelink`. |
| `abilities` | no | The ability IR (ADR 0007): `activated`, `triggered`, `enters_tapped`, `enters_with_counters`. |
| `spell_effects` | no | What an instant/sorcery does on resolution (CR 608.2c), in the same effect IR. |
| `aura` | no | An Aura's enchant restriction and static P/T grant (CR 303.4). Only on a card whose `subtypes` include `"Aura"`. |
| `scripted` | no | `false` by default. `true` declares that this card's behavior is (also) defined in code, in `crates/rune-engine/src/scripted.rs` — the ADR 0007 escape hatch. No bundled card is scripted today. |

The `abilities`, `spell_effects`, and `aura` shapes are the engine's IR and are
documented where they are defined: `crates/rune-engine/src/ability.rs`.

## What a definition may not contain

The schema is **closed**: `deny_unknown_fields` rejects any field not listed above, so
the load fails rather than silently ignoring it. That is how the legal posture is
enforced structurally rather than by review — no exact Oracle text, flavor text, image
URI or asset path, official symbol, frame, watermark, or artist credit can enter the
catalog, by accident or otherwise (`docs/brief.md` Legal Considerations, ADR 0018 §2).

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
| Interned handle | `CardId` (aliased `OracleId`) | the catalog loader | one build |
| Printing | set code + collector number | the set file | forever |
| Per-game instance | `CardInstanceId`, `PermanentId` | the engine, at runtime | one game |

Reference a card from a printing (or, later, a decklist) by its `FunctionalId`. The
`CardId` is a handle the engine keys rules reads on; it is stable only within a build,
and a catalog change may reassign it.

## Adding a card

1. Add one entry to `oracle.json`: a fresh `functional_id` and a fresh `id`, with
   `schema_version: 1`.
2. Add a printing record to a set file if the card should be printed somewhere.
3. Add tests for the behavior in the same PR (`crates/rune-engine/AGENTS.md`).
4. Run `make check`.

Every failure mode above — an unknown field, an unrecognized `schema_version`, a
duplicated identity, a malformed slug, a printing that references nothing — is a
`CatalogError` at load, with a message naming the offender.

## Versioning

`schema_version` is a whole-catalog forcing function. A breaking change to the shape of
a definition — a renamed field, a restructured `abilities` encoding — bumps
`rune_engine::SCHEMA_VERSION` and migrates every definition in the same change. A
definition declaring a version the engine does not recognize is a hard error, never a
silent skip, so a half-migrated catalog cannot boot.
