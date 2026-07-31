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
| `restrictions` | no | Printed combat restrictions; creatures only |
| `abilities` | no | Activated, triggered, or replacement-style ability IR |
| `spell_effects` | no | Resolution effects for instants and sorceries |
| `aura` | no | Aura enchant restriction and static power/toughness, keyword, and/or combat-restriction grant |
| `scripted` | no | Declares behavior implemented in `src/scripted.rs`; defaults to `false` |

Current keyword values are `flying`, `reach`, `vigilance`, `haste`, `defender`, `menace`,
`first_strike`, `trample`, `deathtouch`, `lifelink`, `double_strike`, and `hexproof`.

`defender` (CR 702.3b) removes a creature from the attacker candidate set; `menace`
(CR 702.110b) is checked over the whole declare-blockers selection, since a lone blocker
is illegal precisely because it is alone. Both are read through the computed keywords, so
a granted one restricts exactly as a printed one does.

`hexproof` (CR 702.11b) is not a combat rule at all: it is enforced in `target_is_legal`,
the one predicate both the announcement gate and the CR 608.2b resolution re-check run.
It is controller-relative — an opponent may not target the creature, and its own
controller may.

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

### Combat restrictions (CR 506.3, CR 509.1b)

Restrictions on attacking and blocking that are **not** keyword abilities live in
`restrictions` rather than `keywords`: no card prints them as a keyword, and some carry a
parameter. A unit restriction is its bare name and a parameterized one wraps its payload:

```json
"restrictions": ["cant_be_blocked_by_more_than_one", {"cant_be_blocked_by": "black"}]
```

| Restriction | Meaning | Enforced in |
| --- | --- | --- |
| `cant_attack` | can't be declared as an attacker | the attacker candidate set |
| `cant_block` | can't be declared as a blocker | the blocker candidate set |
| `cant_be_blocked` | no creature may block it | the pairwise block check |
| `cant_be_blocked_by` | no creature of the named colour may block it | the pairwise block check |
| `cant_be_blocked_by_more_than_one` | at most one blocker may be assigned to it | the whole-selection block check |

Printed restrictions belong only on creatures; the loader rejects them elsewhere. They are
read through the computed characteristics at CR 613 layer 6, exactly as keywords are, so a
restriction an Aura or a spell imposes binds identically to a printed one and ends with the
effect that imposed it.

Two of these are facts about the **whole** declaration rather than about one
attacker/blocker pair — `cant_be_blocked_by_more_than_one` and menace — so the engine can
only judge them once the declaration is assembled. Both are therefore stated in the
blocker slot's `prompt` (`docs/protocol.md`) rather than left to a submit that silently
does nothing.

The colour test reads the blocker's **printed** colours: CR 613 layer 5 (colour-changing
effects) is not implemented, so printed colour is current colour, the same way printed
types stand in for current types elsewhere in the engine.

Attack and block *requirements* ("attacks each combat if able") are not modeled: a
declaration can be restricted but never required.

### Imposing restrictions (continuous, CR 613.1f)

The three restriction verbs mirror the keyword-granting ones exactly, and all impose
**until end of turn**:

- `{"kind": "restrict", "target": "any_creature", "restriction": "cant_block"}` — one
  chosen target;
- `{"kind": "restrict_self", "restriction": "cant_be_blocked"}` — the ability's own
  source, which is not a target and never fizzles;
- `{"kind": "restrict_all", "affects": "creatures_without_flying", "restriction": "cant_block"}`
  — a class, whose members are locked in on resolution (CR 611.2c).

An **Aura** imposes for as long as it is attached, via `aura.restrictions`, e.g.
`"aura": {"enchant": "any_creature", "restrictions": ["cant_attack", "cant_block"]}`. The
`power`/`toughness`, `keywords`, and `restrictions` grants are independent; any
combination may be present.

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

### Damage (CR 120.3)

`deal_damage` names **who or what takes the damage** with exactly one of three keys, and
that key decides whether a target is chosen:

```json
{ "kind": "deal_damage", "target": "any_target",      "amount": 2 }
{ "kind": "deal_damage", "player_ref": "each_opponent", "amount": 2 }
{ "kind": "deal_damage", "affects": "each_creature",  "amount": 2 }
```

- `target` is a target spec: one slot, chosen on announcement, re-checked on resolution,
  fizzling if the choice is gone.
- `player_ref` is the same reference `gain_life` and `mill` take, with the same rule —
  `each_opponent` chooses nothing and hits every opponent, `target_player` fills a slot.
- `affects` is the same class `pump_all` takes, and never targets.

Both class forms enumerate their subjects **on resolution** (CR 611.2c), so a creature
that arrived after the spell was cast is included and one that has died is not.

Damage is not life loss. Damage to a creature is *marked* on it and drives the
lethal-damage state-based action (CR 704.5g); damage to a player is life loss
(CR 120.3a). A card that says "loses life" is authored with `lose_life`, and one that
says "deals damage" cannot be approximated by it. Damage prevention and deathtouch on
non-combat damage are out of scope.

### Activation costs

`cost` entries are `{"kind":"tap"}` (the `{T}` symbol) and
`{"kind":"mana","mana":"{1}{R}"}`, written in the same curly-brace notation a card's
`mana_cost` uses. Mana is paid from the activating player's pool through the same seam a
cast uses, and the whole cost is paid all or nothing — a failed mana payment never leaves
the source tapped. CR 302.6 still forbids a summoning-sick creature paying `{T}`,
including for a mana ability.

### Effects on the ability's own source

`pump_self` and `put_counters_on_self` act on the permanent whose ability is resolving.
The source is not a *target* (CR 115.1), so these choose nothing, fill no slot, and never
fizzle; a source that has left the battlefield is simply not there to modify. They are
meaningless on a spell, which has no source permanent.

### Mass, non-targeting modifications

`pump_all` and `grant_keyword_all` modify a **class** until end of turn rather than a
target, so they choose nothing and never fizzle:

```json
{ "kind": "pump_all", "affects": "creatures_you_control", "power": 2, "toughness": 1 }
```

The classes are `creatures_you_control`, `each_creature`,
`creatures_your_opponents_control`, and `creatures_without_flying`. The first three are read
relative to the effect's controller so one authored card means "you" from either seat; the
last reads flying through the computed keywords, so a *granted* flying excludes a creature
exactly as a printed one does. `deal_damage` takes the same set.

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

### Effects that ask a player to choose cards

Four effects stop mid-resolution and hand one named player a decision (issue #604). The
game does not proceed until it is answered, and the answer is validated against the zone
as it is *at that moment* — see `docs/decisions/0013-mid-resolution-player-choices.md`.

| `kind` | Asks | Aftermath |
| --- | --- | --- |
| `discard` | `count` cards of `player_ref`'s hand | they go to that player's graveyard |
| `scry` | any number of the top `count` cards | the chosen go to the bottom, in the chosen order |
| `look_at_top` | up to `take` of the top `count` | the chosen go to `destination`, the rest to the bottom in a random order |
| `search_library` | up to `take` cards of the library | the chosen go to `destination`, then the library is shuffled |

```json
{ "kind": "discard", "player_ref": "target_opponent", "count": 1,
  "chosen_by": "controller", "filter": { "kind": "noncreature_nonland" } }
```

- `player_ref` decides whether a discard **targets**, exactly as it does for `mill` — so
  both "target player discards two cards" and "each opponent discards a card" are
  writable, and neither restates the fizzle rule. `scry`, `look_at_top`, and
  `search_library` act on the controller's own library and never target.
- `chosen_by` is `owner` (the default — the discarding player picks) or `controller` (the
  spell's controller picks, the hand-attack shape). The chooser is also the only seat the
  cards are shown to.
- `filter` narrows which cards may be picked: `any` (the default), `land`,
  `creature` with an optional `max_power`, `noncreature_nonland`, or
  `same_name_as_source` ("a card named *this card*", matched on printed identity so two
  copies of one printing find each other).
- `destination` is `hand` (the default), `battlefield`, or `battlefield_tapped`. A card
  entering the battlefield this way goes through the same seam a resolving permanent
  spell uses, so its "enters tapped"/"enters with counters" replacements and its ETB
  triggers all fire.

A question with **no legal answer is never asked**: an empty hand, an empty library, or a
look that turns up nothing matching applies the effect with an empty selection and
resolves — including the aftermath, so a look that whiffs still bottoms what it looked at
and a search that finds nothing still shuffles (CR 701.19c).

Two orderings are deliberately **not** modeled and are listed in the exclusions: the
cards a scry keeps on top stay in their printed order, and the cards a `look_at_top`
bottoms go there at random rather than in an order the player picks.

### Effects a player may decline

`may` wraps other effects in a yes-or-no its controller answers mid-resolution (issue
#610). It rides the same queue, routing, and never-stall rules as the card choices above
— see `docs/decisions/0014-optional-effects.md`.

```json
{ "kind": "may", "effects": [{ "kind": "draw_card", "count": 1 }] }
{ "kind": "may", "cost": "{1}",
  "effects": [{ "kind": "draw_card", "count": 1 }] }
```

- The first reads "you may draw a card"; the second, "you may pay {1}. If you do, draw a
  card". Rules text is composed from the wrapped effects, so the printed sentence and the
  question the player is asked are the same words.
- **The controller answers**, whoever else the surrounding ability names and whoever holds
  priority. A trigger that goes on the stack during an opponent's turn still asks its own
  controller.
- `cost` is a mana cost in the same `{...}` notation an activation cost uses, paid from
  the controller's pool. While the question is owed they may activate **mana abilities**
  and nothing else (CR 605.3a), so a cost is payable if the board could still make the
  mana. A cost no amount of tapping could pay is never asked at all — it is declined, and
  recorded as declined.
- **Declining is not a fizzle.** The wrapped effects are skipped; every other effect of
  the same ability, and the spell's own trip to its final zone (CR 608.3), happen exactly
  as if the `may` were not there.
- **The wrapped effects may not target.** One effect declares at most one target slot, so
  a wrapper cannot speak for what it wraps; the catalog validator rejects it at build
  time rather than letting the card silently do nothing. Choosing an optional cost at
  *announcement* (kicker) is a different mechanism and is still excluded.

### Trigger conditions

Conditions about the ability's **own source** are authored as bare strings:
`self_enters_battlefield`, `self_dies`, `self_attacks`, `you_gain_life`. A condition that
carries a selector wraps it:

```json
{ "type": "triggered",
  "event": { "permanent_dies": { "scope": "any_creature", "except_this": true } },
  "effects": [{ "kind": "lose_life", "player_ref": "each_opponent", "amount": 1 }] }
```

`permanent_enters` and `permanent_dies` take an observed-permanent selector — `scope` is
`creatures_you_control` or `any_creature`, with an optional `subtype` and an `except_this`
that means "another". `you_cast_spell` takes `enchantment` or `instant_or_sorcery`.

`beginning_of_step` is about the turn rather than about an object:

```json
{ "type": "triggered",
  "event": { "beginning_of_step": { "step": "upkeep", "whose_turn": "yours" } },
  "effects": [{ "kind": "gain_life", "player_ref": "controller", "amount": 1 }] }
```

`step` is `upkeep`, `draw`, `begin_combat`, or `end_step` — the four steps printed cards
trigger at, and deliberately not every step of the turn: all four grant priority, so a
trigger owed at one is answered in the step it belongs to. `whose_turn` is `yours` (only
the controller's own turn) or `each` (every turn), and that choice is most of what such an
ability means: "each upkeep" fires twice as often as "your upkeep" and is otherwise the
same card.

Every condition is observed by diffing the state before and after an action, never by a
listener. A condition about an **event** rather than a board position (life gain, casting,
a step beginning) is read from the events that transition recorded, because gaining and
losing the same life leaves every total unchanged and still triggered — and because one
pass of priority can walk through several steps at once, so comparing the step before with
the step after would miss every crossing but the last.

A watching condition reports **how many times** it was met, not whether: two creatures
dying at once trigger a death-watcher twice. A watching ability must still be on the
battlefield afterwards, except a death-watcher, which observes a creature that died
alongside it.

A triggered ability reaches the stack **unaimed** and its controller is then asked to
choose its targets (CR 603.3d), so a trigger whose effect targets works exactly as a
spell's does — including the resolution-time re-check that fizzles it if its target has
gone (CR 608.2b). A trigger with no legal choice for one of its slots is never put on the
stack at all (CR 603.3c), so authoring one costs nothing when the board cannot answer it.

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
- printed `restrictions` on a card that is not a creature;
- unresolved printing references or duplicate collector numbers; and
- disagreement between a scripted definition and `src/scripted.rs`.

Breaking schema changes increment `SCHEMA_VERSION` and migrate the entire catalog in the
same change. Unsupported versions fail rather than being skipped.
