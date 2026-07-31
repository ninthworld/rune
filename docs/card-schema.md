# Card schema

SAGE cards are versioned functional definitions: structured, printing-independent data the
engine executes. The model is defined by [ADR 0008](decisions/0008-functional-card-definitions.md),
including the split between a functional definition and the printings that reference it.

The Rust authorities are `CardData` and `Printing` in
`crates/sage-engine/src/card.rs`; validators live in `src/catalog.rs`.

## File layout

- `crates/sage-engine/data/catalog/<functional_id>.json` contains one functional
  definition. The file stem must equal its `functional_id`.
- `crates/sage-engine/data/sets/<SET>.json` contains that set’s printing records. A
  printing refers to a functional definition and carries no rules.

`build.rs` discovers, validates, sorts, and embeds both directories at compile time. The
running engine performs no filesystem I/O.

## Functional definition

The bundled catalog's functional definitions are sourced from a real set (Core Set 2019);
see [ADR 0009](decisions/0009-real-functional-card-data.md). Only functional data is stored —
no Oracle text, flavor, art, or branding.

```json
{
  "schema_version": 1,
  "functional_id": "skyscanner",
  "name": "Skyscanner",
  "types": ["artifact", "creature"],
  "subtypes": ["Thopter"],
  "mana_cost": "{3}",
  "colors": [],
  "power": 1,
  "toughness": 1,
  "keywords": ["flying"],
  "abilities": [
    {
      "type": "triggered",
      "event": "self_enters_battlefield",
      "effects": [{ "kind": "draw_card", "count": 1 }]
    }
  ]
}
```

### Fields

| Field | Required | Meaning |
| --- | --- | --- |
| `schema_version` | yes | Must equal `sage_engine::SCHEMA_VERSION`, currently `1` |
| `functional_id` | yes | Stable lowercase `snake_case` identity; must match the file name |
| `name` | yes | Display name |
| `types` | yes | One or more card types such as `creature`, `land`, or `instant` |
| `supertypes` | no | Values such as `basic` or `legendary` |
| `subtypes` | no | Printed subtype names such as `Elf` or `Aura` |
| `mana_cost` | yes | Curly-brace notation; empty when the card has no mana cost |
| `colors` | no | Explicit card colors; empty means colorless |
| `power`, `toughness` | conditional | Both required for creatures and forbidden for non-creatures |
| `keywords` | no | Supported keyword abilities |
| `abilities` | no | Activated, triggered, or replacement-style ability IR |
| `spell_effects` | no | Resolution effects for instants and sorceries |
| `aura` | no | Aura enchant restriction and static power/toughness and/or keyword grant |
| `scripted` | no | Declares behavior implemented in `src/scripted.rs`; defaults to `false` |

Current keyword values are `flying`, `reach`, `vigilance`, `haste`, `defender`, `menace`,
`first_strike`, `trample`, `deathtouch`, `lifelink`, and `double_strike`.

`defender` (CR 702.3b) removes a creature from the attacker candidate set; `menace`
(CR 702.110b) is checked over the whole declare-blockers selection, since a lone blocker
is illegal precisely because it is alone. Both are read through the computed keywords, so
a granted one restricts exactly as a printed one does.

### Granting keywords (continuous, CR 613.1f)

Effects can grant a keyword ability, applied at the layer system's ability-adding
layer 6 so a granted keyword is indistinguishable from a printed one everywhere
keywords are read (combat legality, evasion, damage, view projection, generated text):

- An **Aura** (or other static grant) grants for as long as it is attached: list the
  keywords under `aura.keywords`, e.g. an Aura granting flying is
  `"aura": {"enchant": "any_creature", "keywords": ["flying"]}`. The `power`/`toughness`
  and `keywords` grants are independent; either or both may be present.
- A **spell or ability** grants **until end of turn** with the `grant_keyword` effect,
  e.g. `{"kind": "grant_keyword", "target": "any_creature", "keyword": "trample"}`. The
  grant expires in the cleanup step (CR 514.2). Duplicate grants are redundant, not
  additive. Keyword *removal* and conditional grants are out of scope.

### Targets (CR 115.1)

A targeting effect names a **class** with `target`, not an object; the player chooses one
member as the spell or ability is announced (CR 601.2c) and the choice is re-checked on
resolution (CR 608.2b). The classes are `any_player`, `any_opponent`, `any_permanent`,
`any_nonland_permanent`, `any_creature`, `any_creature_you_control`,
`any_creature_an_opponent_controls`, `any_creature_with_flying`, `any_tapped_creature`,
`any_artifact`, `any_enchantment`, `any_artifact_or_enchantment`, `any_land`,
`spell_on_stack`, `creature_spell_on_stack`, and `any_target`.

Every class is evaluated **relative to the choosing object's controller**, which is what
lets one authored card mean "you" from either seat. Classes read through the computed
characteristics where they can, so `any_creature_with_flying` accepts a creature that was
*granted* flying and stops accepting it when the grant ends (CR 613.1f).

An effect fills exactly one target slot, and a card's slots are consumed in the order its
effects are written.

### Player references

An effect whose subject is a player carries a `player_ref` rather than a `target`. The
reference itself decides whether a target is chosen:

| `player_ref` | Meaning | Chooses a target? |
| --- | --- | --- |
| `controller` | "you" | no |
| `each_opponent` | every opponent still in the game | no |
| `target_player` | one chosen player | yes |
| `target_opponent` | one chosen opponent | yes |

A non-targeting reference can never fizzle and, in a game of three or more, really does
name every opponent. `gain_life`, `lose_life`, and `mill` all take a reference, so both
shapes exist for each without any of them restating the fizzle rule.

### Activation costs

`cost` entries are `{"kind":"tap"}` (the `{T}` symbol) and
`{"kind":"mana","mana":"{1}{R}"}`, written in the same curly-brace notation a card's
`mana_cost` uses. Mana is paid from the activating player's pool through the same seam a
cast uses, and the whole cost is paid all or nothing — a failed mana payment never leaves
the source tapped. CR 302.6 still forbids a summoning-sick creature paying `{T}`,
including for a mana ability.

### Mass, non-targeting modifications

`pump_all` and `grant_keyword_all` modify a **class** until end of turn rather than a
target, so they choose nothing and never fizzle:

```json
{ "kind": "pump_all", "affects": "creatures_you_control", "power": 2, "toughness": 1 }
```

The affected set is locked in on resolution (CR 611.2c) — a creature that arrives later in
the turn is untouched. That is the whole difference between one of these and an
`Ability::Static` anthem, which is re-derived on every read.

### Static abilities (continuous, CR 604.3)

A printed static ability applies continuously for as long as its source is on the
battlefield, with nothing ever put on the stack — an anthem or a lord:

```json
{
  "type": "static",
  "affects": { "scope": "creatures_you_control", "subtype": "Elf", "except_this": true },
  "modification": { "kind": "power_toughness", "power": 1, "toughness": 1 }
}
```

- `subtype` restricts the class ("other **Elves** you control"); omit it for every
  creature its controller controls.
- `except_this` excludes the source itself — the "other" in a lord's wording. It compares
  the *permanent*, not the card, so two copies of one lord do pump each other.
- `modification` is either `power_toughness` (layer 7c, folded after counters in timestamp
  order) or `grant_keyword` (layer 6, idempotent).

The effect is **derived from the battlefield on every read, never stored**: it starts and
stops with its source, so a static ability cannot outlive the permanent that printed it.
Rules text is composed from the same selector the engine applies, so the sentence and the
scope cannot disagree.

### Trigger conditions

`event` is `self_enters_battlefield`, `self_dies`, or `self_attacks` — each observed by
diffing the state before and after an action, never by a listener. Every condition is about
the ability's **own source**; a trigger watching another object is not yet expressible.

A triggered ability reaches the stack with **no targets**, and nothing prompts its
controller to choose any, so a trigger whose effect targets fizzles on resolution. Author
triggers with non-targeting effects until that gap closes.

The full `abilities`, `spell_effects`, target, cost, and Aura shapes are the enums in
`crates/sage-engine/src/ability.rs`. Those Rust types are authoritative; do not reproduce
the IR in a second documentation schema that can drift.

## Closed schema and generated text

`CardData` uses `deny_unknown_fields`. A definition cannot contain exact Oracle text,
flavor text, image paths or URLs, official symbols or frames, artist credit, watermarks,
or arbitrary presentation fields. Unknown fields fail parsing.

Definitions contain no rules-prose field. `crates/sage-server/src/rules_text.rs` generates
`CardView.rules_text` from the same structured behavior the engine executes. Formatter
matches are exhaustive, so adding an IR variant without display support fails compilation.

A scripted card is the exception because its Rust behavior cannot be inspected by the
formatter. It must provide its own non-Oracle explanatory text beside the code in
`src/scripted.rs`. Loader validation requires the catalog’s `scripted` flag and the code
registration to agree in both directions.

## Printing record

```json
{
  "functional_id": "skyscanner",
  "collector_number": "19",
  "rarity": "common"
}
```

All three fields are required. `rarity` is one of `common`, `uncommon`, `rare`, or
`mythic`. The set code comes from the file name. A printing must resolve to an existing
functional definition, and collector numbers must be unique within a set.

## Identity model

| Layer | Type | Assigned by | Lifetime |
| --- | --- | --- | --- |
| Functional card | `FunctionalId` | Card author | Stable across builds |
| Engine handle | `CardId` (`OracleId` alias) | Build script | One catalog build |
| Printing | set code + collector number | Set file | Stable bibliography |
| Game object | `CardInstanceId`, `PermanentId` | Engine | One game or battlefield stay |

Never persist or hand-author a `CardId`. Adding a definition can change sorted interning and
renumber handles. Printings, decklists, tests, and scripted code use `FunctionalId` and
resolve it through `CardDatabase::card_id` when a handle is needed.

## Adding a card

1. Add `data/catalog/<functional_id>.json` with schema version `1` and a matching id.
2. Add or update a set file if the card needs a printing record.
3. Add behavior tests using the card’s `functional_id`.
4. Run `make check`.

Adding a functional definition creates one catalog file; adding a printing may also edit its
set file.

## Validation

The build and loader reject:

- unknown fields or malformed JSON;
- unsupported schema versions;
- malformed, duplicate, or file-mismatched functional ids;
- missing types or invalid creature power/toughness;
- an Aura grant on a non-Aura;
- unresolved printing references or duplicate collector numbers; and
- disagreement between a scripted definition and `src/scripted.rs`.

Breaking schema changes increment `SCHEMA_VERSION` and migrate the entire catalog in the
same change. Unsupported versions fail rather than being skipped.
