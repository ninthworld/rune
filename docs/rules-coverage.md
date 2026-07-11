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
| CR 103.5 | The London mulligan: after opening hands, each player may keep or mulligan; a mulligan shuffles the hand into the library and redraws a full opening hand, and a player who keeps after N mulligans puts N cards on the bottom of their library (a single multi-select requirement slot). Turn 1 begins only once every player has kept. | partial — turn-order/APNAP nuance is modeled as a simpler seat-by-seat sequential decision (the simultaneous-decision UI treatment is out of scope, issue #111); a first-hand keep works over the wire, while projecting the bottoming requirement rides the same engine→wire `requirements` wiring pending for targeting (ADR 0009 follow-up #73). | `mulligan.rs :: MulliganState`, `apply.rs :: apply_mulligan`, `apply.rs :: apply_keep`, `mulligan.rs :: bottom_requirement` | `mulligan.rs :: cr_103_5_nth_mulligan_requires_bottoming_n_cards`, `mulligan.rs :: cr_103_5_turn_one_waits_until_all_players_keep` |
| CR 115 / 115.1 | A spell or ability targets objects/players as defined by a target spec; legality is derived on demand from current state. | partial — only `AnyPlayer`, `AnyPermanent`, `AnyCreature` specs exist; printed types are authoritative (no type-changing layer). | `ability.rs :: TargetSpec`, `resolve.rs :: target_is_legal` | `resolve.rs :: target_legality_tracks_current_state` |
| CR 116.2a | Playing a land is a special action that does not use the stack. | implemented | `apply.rs :: apply_play_land` | `apply.rs :: issue_card_effects_etb_draw_end_to_end` |
| CR 122 | Counters on a permanent (`+1/+1`, `-1/-1`) are stored per kind and counted on demand. | partial — only the two P/T counter kinds are modeled; loyalty/charge/etc. deferred. | `state.rs :: CounterKind`, `state.rs :: Permanent::counter_count` | `characteristics.rs :: counter_count_defaults_to_zero_and_reports_stored_counts` |
| CR 205.4 | A card's supertypes are a closed, structured set (never a parsed string). | implemented | `card_type.rs :: Supertype` | `card.rs :: type_line_renders_supertypes_types_and_subtypes` |
| CR 300 | A card's card types are a closed, structured set. | implemented | `card_type.rs :: CardType` | `card.rs :: has_type_and_has_subtype_query_structured_types` |
| CR 302.6 | Summoning sickness: a creature can't attack (or use `{T}`/`{Q}` abilities) unless its controller has controlled it continuously since their most recent turn began. | partial — the attack restriction is enforced; the tap/untap-ability restriction is not modeled yet. | `combat.rs :: has_summoning_sickness`, `combat.rs :: attacker_candidates` | `combat.rs :: summoning_sickness_is_by_entry_turn_cr_302_6`, `apply.rs :: issue_117_summoning_sick_creature_cannot_attack_cr_302_6` |
| CR 508.1 / 508.1a / 508.1f | Declare attackers: the active player declares as attackers untapped creatures controlled since the turn began; declaring none is legal; attacking taps the attacker. | partial — declaration only; single attack target (the opponent), no vigilance/defender, and combat damage is issue #118. | `combat.rs :: attacker_candidates`, `actions.rs :: attackers_selection_is_legal`, `apply.rs :: apply_declare_attackers` | `apply.rs :: issue_117_declare_attackers_taps_and_marks_attackers_cr_508_1`, `apply.rs :: issue_117_empty_attacker_declaration_is_legal_cr_508_1a`, `apply.rs :: issue_117_tapped_creature_cannot_attack_cr_508_1a` |
| CR 509.1 / 509.1a | Declare blockers: the defending player assigns each blocker to one attacking creature; multiple blockers per attacker are allowed; a tapped creature can't block. | partial — declaration only (no blocking restrictions beyond untapped, no damage assignment order); combat damage is issue #118. | `combat.rs :: blocker_candidates`, `combat.rs :: declared_attackers`, `actions.rs :: blocks_selection_is_legal`, `apply.rs :: apply_declare_blockers` | `apply.rs :: issue_117_defender_declares_blockers_multiple_per_attacker_cr_509_1a`, `apply.rs :: issue_117_tapped_creature_cannot_block_cr_509_1a`, `apply.rs :: issue_117_a_creature_cannot_be_declared_as_two_blocks_cr_509_1a` |
| CR 511.3 | At end of combat, all creatures and planeswalkers are removed from combat. | partial — creatures are removed (attacking/blocking cleared); planeswalkers/battles are not modeled. | `apply.rs :: remove_creatures_from_combat` | `apply.rs :: issue_117_end_of_combat_removes_creatures_from_combat_cr_511_3` |
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
