# Rules coverage

The authoritative record of which *Magic: The Gathering* Comprehensive Rules
(CR) the RUNE engine actually implements today. Every row points at real engine
code and a real test that exercises it.

## Scope

This document lists **implemented** CR rules only. It is a coverage map, **not a
TODO list** of unimplemented rules — a rule that RUNE does not model yet simply
has no row here. When a rule is only *partially* modeled, its row is marked
`partial` and the gap is named explicitly, but the deferred remainder is not
enumerated as future work. For the shape of the engine and where these pieces
live, see [`brief.md`](brief.md) and the ADRs under [`decisions/`](decisions/).

Rule numbers follow the CR citation convention in
[`coding-standards.md`](coding-standards.md): engine behavior that implements a
CR rule cites `CR NNN.Nx` in its doc comment, and any PR that adds or changes
rule behavior updates this table in the same PR. The table is seeded by auditing
those citations:

```
rg 'CR \d+' crates/rune-engine/src
```

Code and test anchors are given as `path :: item`. Paths are relative to the
repository root; all code is in `crates/rune-engine/src/`.

## Coverage

| CR rule | Summary | Status | Code anchor | Test anchor |
| --- | --- | --- | --- | --- |
| CR 115 / 115.1 | A spell or ability targets objects/players as defined by a target spec; legality is derived on demand from current state. | partial — only `AnyPlayer`, `AnyPermanent`, `AnyCreature` specs exist; printed types are authoritative (no type-changing layer). | `ability.rs :: TargetSpec`, `resolve.rs :: target_is_legal` | `resolve.rs :: target_legality_tracks_current_state` |
| CR 116.2a | Playing a land is a special action that does not use the stack. | implemented | `apply.rs :: apply_play_land` | `apply.rs :: issue_card_effects_etb_draw_end_to_end` |
| CR 122 | Counters on a permanent (`+1/+1`, `-1/-1`) are stored per kind and counted on demand. | partial — only the two P/T counter kinds are modeled; loyalty/charge/etc. deferred. | `state.rs :: CounterKind`, `state.rs :: Permanent::counter_count` | `characteristics.rs :: counter_count_defaults_to_zero_and_reports_stored_counts` |
| CR 205.4 | A card's supertypes are a closed, structured set (never a parsed string). | implemented | `card_type.rs :: Supertype` | `card.rs :: type_line_renders_supertypes_types_and_subtypes` |
| CR 300 | A card's card types are a closed, structured set. | implemented | `card_type.rs :: CardType` | `card.rs :: has_type_and_has_subtype_query_structured_types` |
| CR 601.2c | The targets chosen for a spell/ability are recorded on the stack object as it is put on the stack. | implemented | `apply.rs :: apply_activate_ability`, `ability.rs :: Target` | `actions.rs :: a_legal_target_is_accepted_and_carried_onto_the_stack` |
| CR 605.1a | A mana ability is an activated ability whose every effect adds mana (and is not a targeted/loyalty ability). | partial — simplified: the "could add mana" / non-targeting nuances beyond "all effects add mana" are not modeled. | `ability.rs :: is_mana_ability` | `ability.rs :: activated_mana_ability_round_trips` |
| CR 605.3 | A mana ability resolves immediately without using the stack and does not change who has priority. | implemented | `apply.rs :: apply_activate_ability` | `apply.rs :: forest_mana_ability_adds_green_without_using_the_stack` |
| CR 608.2b | On resolution, an object's chosen targets are re-checked; if every target is now illegal the object is removed from the stack without effect ("fizzle"). | implemented | `resolve.rs :: resolve_stack_object`, `resolve.rs :: target_is_legal` | `resolve.rs :: an_object_whose_target_became_illegal_fizzles` |
| CR 608.2c | An individually illegal target is skipped on resolution while the object's still-legal targets take effect. | partial — no effect with more than one target slot exists yet, so only the single-target skip path is exercised. | `resolve.rs :: resolve_stack_object` | `resolve.rs :: an_object_whose_target_became_illegal_fizzles` |
| CR 608.2m | A resolving instant/sorcery is put into its owner's graveyard rather than creating a permanent. | partial — owner is approximated by controller (no separate ownership tracking yet). | `resolve.rs :: resolve_stack_object` | `resolve.rs :: issue_47_non_permanent_spell_resolves_to_graveyard_not_battlefield` |
| CR 608.3 | A resolving spell is routed by its card types: a permanent spell enters the battlefield, a non-permanent spell does not. | implemented | `resolve.rs :: resolve_stack_object`, `card.rs :: CardData::is_permanent` | `resolve.rs :: resolving_a_creature_spell_puts_it_on_the_battlefield` |
| CR 613.7 | Within a layer, continuous effects apply in timestamp order. | implemented (for the modeled layer) | `characteristics.rs :: ordered_pt_modifiers` | `characteristics.rs :: two_static_modifiers_apply_in_timestamp_order_and_sum` |
| CR 613.7c | Layer 7c: `+1/+1` / `-1/-1` counters, then static `+X/+Y` modifiers, adjust a permanent's power/toughness. | partial — only layer 7c is computed; layers 1–6 and set-P/T (CDAs) are deferred behind the same read path. | `characteristics.rs :: characteristics`, `characteristics.rs :: pt_counter_delta`, `characteristics.rs :: static_pt_delta` | `characteristics.rs :: single_static_modifier_stacks_on_printed_pt_and_counters` |
| CR 704 | State-based actions are checked and applied to a fixed point after every action. | partial — the SBA loop exists but only CR 704.5a is modeled (see below). | `sba.rs :: run_state_based_actions` | `sba.rs :: state_based_actions_reach_a_fixed_point` |
| CR 704.5a | A player with 0 or less life loses the game. | implemented | `sba.rs :: run_state_based_actions` | `sba.rs :: state_based_actions_mark_a_player_at_zero_life_as_lost` |

## Keeping this current

When you add or change engine behavior that implements a CR rule:

1. Cite the rule as `CR NNN.Nx` in the doc comment of the code that implements
   it (the citation convention in [`coding-standards.md`](coding-standards.md)).
2. Add or update the corresponding row here — rule number, one-line summary,
   status, code anchor, and test anchor — in the **same PR**. Mark anything
   incomplete `partial` and name the gap.

This is part of the definition of done; see
[`agents/workflow.md`](agents/workflow.md).
