//! Cost modification (CR 601.2f): the step between the cost printed on a card and the
//! mana a player actually pays.
//!
//! **A modification is derived, never stored.** Nothing enters
//! [`GameState`](crate::GameState) here and nothing is pruned: a reducer's effect begins
//! the instant its source is on the battlefield and ends the instant it leaves, exactly
//! as an anthem's does (ADR 0005 §1). The whole layer is one pure fold over the
//! battlefield, re-run on every read.
//!
//! **It has to be read in three places, and it is read in one.** `valid_actions` decides
//! whether a seat can afford a cast, `apply_action` charges for it, and the idle
//! predicate ([`crate::priority_has_no_meaningful_action`]) floats a board's potential
//! mana to decide whether a seat has a play at all. A modification applied only at
//! payment time would advertise casts the seat cannot take and auto-pass a seat that had
//! one; a modification applied only at offer time would announce a discount the charge
//! then refused. So every road goes through
//! [`cast_cost`](crate::actions::cast_cost) — the offer, the payment search, the pip
//! enumeration, the legality gate, and the charge — and this function is the one step
//! that road takes. The idle predicate shares it by construction: it asks
//! `valid_actions` of a hypothetical board rather than reading a cost itself.
//!
//! **The arithmetic is CR 601.2f's own**, and it is deliberately not two clamped steps:
//! the total cost is the printed cost *plus* every additional cost and cost increase,
//! *minus* every cost reduction, and "if the mana component of the total cost is reduced
//! to nothing by cost reduction effects, it is considered to be {0}. It can't be reduced
//! to less than {0}." One floor, applied once at the end, is what that sentence says —
//! and it is the same result as applying every increase before every reduction, which is
//! the order the rule's arithmetic implies. Clamping between the two would make a
//! `{1}` spell under a `{2}` reduction and a `{2}` tax cost `{2}`; the rule makes it
//! `{1}`.
//!
//! **Only the generic component moves.** A coloured requirement and a `{C}` are untouched
//! in both directions, which is what every printed reducer in this set says: `{2}` off a
//! `{4}{G}` leaves `{2}{G}`, and a seat with no green source still cannot cast it.

use crate::ability::{Ability, CostModification};
use crate::card::{abilities_of_permanent, CardDatabase};
use crate::id::{CardId, PlayerId};
use crate::mana::ManaCost;
use crate::state::GameState;

/// `base` after every cost modification that applies to `caster` casting `card`
/// (CR 601.2f).
///
/// `base` is the cost as it stands *before* this step — the printed mana cost plus the
/// commander tax where one applies (CR 903.8), that tax being an additional cost the
/// rule folds in ahead of any increase or reduction.
///
/// The sources are the permanents `caster` **currently controls** (CR 613 layer 2, so a
/// reducer that has been stolen reduces for its new controller), read through
/// [`abilities_of_permanent`] — which is what makes a reducer that has lost all its
/// abilities stop reducing, with no clause here saying so.
pub(crate) fn modified_cast_cost(
    state: &GameState,
    db: &CardDatabase,
    caster: PlayerId,
    card: CardId,
    base: ManaCost,
) -> ManaCost {
    let mut increase: u32 = 0;
    let mut reduction: u32 = 0;
    for perm in &state.battlefield {
        if crate::characteristics::controller_of(state, perm) != caster {
            continue;
        }
        for ability in abilities_of_permanent(state, db, perm) {
            let Ability::CostModifier {
                spells,
                modification,
            } = ability
            else {
                continue;
            };
            if !crate::card::spell_matches_class(db, card, spells, perm.chosen_color) {
                continue;
            }
            match modification {
                CostModification::Increase { generic } => {
                    increase = increase.saturating_add(u32::from(generic));
                }
                CostModification::Reduce { generic } => {
                    reduction = reduction.saturating_add(u32::from(generic));
                }
            }
        }
    }
    if increase == 0 && reduction == 0 {
        return base;
    }
    let mut cost = base;
    // The single CR 601.2f floor: plus every increase, minus every reduction, never
    // below {0}. `saturating_sub` on the running total *is* that floor.
    let generic = u32::from(cost.generic)
        .saturating_add(increase)
        .saturating_sub(reduction);
    cost.generic = u8::try_from(generic).unwrap_or(u8::MAX);
    cost
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::id::PermanentId;
    use crate::mana::parse_mana_cost;
    use crate::state::Permanent;

    /// A synthetic catalog: one creature spell on each side of a power-4 bound, and
    /// three modifiers whose amounts the bundled catalog has no card for. The arithmetic
    /// of CR 601.2f is what these tests are about, and the bundled cards can only reach
    /// one corner of it.
    fn db() -> CardDatabase {
        CardDatabase::from_json(
            r#"[
            {"schema_version":1,"functional_id":"big_beast","name":"Big Beast",
             "types":["creature"],"mana_cost":"{2}{G}","colors":["green"],
             "power":4,"toughness":4},
            {"schema_version":1,"functional_id":"small_beast","name":"Small Beast",
             "types":["creature"],"mana_cost":"{2}{G}","colors":["green"],
             "power":3,"toughness":3},
            {"schema_version":1,"functional_id":"cheapener","name":"Cheapener",
             "types":["artifact"],"mana_cost":"{1}",
             "abilities":[{"type":"cost_modifier",
                           "spells":{"creature":{"min_power":4}},
                           "modification":{"kind":"reduce","generic":2}}]},
            {"schema_version":1,"functional_id":"big_cheapener","name":"Big Cheapener",
             "types":["artifact"],"mana_cost":"{1}",
             "abilities":[{"type":"cost_modifier",
                           "spells":{"creature":{}},
                           "modification":{"kind":"reduce","generic":5}}]},
            {"schema_version":1,"functional_id":"taxer","name":"Taxer",
             "types":["artifact"],"mana_cost":"{1}",
             "abilities":[{"type":"cost_modifier",
                           "spells":{"creature":{}},
                           "modification":{"kind":"increase","generic":2}}]}
        ]"#,
        )
        .expect("a well-formed synthetic catalog")
    }

    /// A two-player state with `modifiers` on the battlefield under player 0.
    fn board(db: &CardDatabase, modifiers: &[&str]) -> GameState {
        let mut state = GameState::new_two_player();
        for slug in modifiers {
            let card = crate::fixtures::id_in(db, slug);
            let instance = state.new_instance(card).id;
            let id = PermanentId(state.mint_id());
            state.battlefield.push(Permanent {
                id,
                instance,
                printed: card.into(),
                controller: PlayerId(0),
                ..Default::default()
            });
        }
        state
    }

    /// What `slug` costs player 0, given `modifiers` on the battlefield.
    fn cost_of(db: &CardDatabase, modifiers: &[&str], slug: &str) -> ManaCost {
        let state = board(db, modifiers);
        let card = crate::fixtures::id_in(db, slug);
        let printed = parse_mana_cost(&db.card(card).expect("a synthetic card").mana_cost);
        modified_cast_cost(&state, db, PlayerId(0), card, printed)
    }

    #[test]
    fn cr_601_2f_reductions_add_up() {
        let db = db();
        // Two reducers are `{2}` and `{5}` off the same spell; nothing here is
        // first-come-first-served.
        assert_eq!(
            cost_of(&db, &["cheapener"], "big_beast"),
            parse_mana_cost("{G}"),
            "{{2}} off a {{2}}{{G}}"
        );
        assert_eq!(
            cost_of(&db, &["cheapener", "cheapener"], "big_beast").generic,
            0,
            "and two of them cannot take it below {{0}}"
        );
    }

    #[test]
    fn cr_601_2f_a_reduction_cannot_take_a_cost_below_zero() {
        let db = db();
        // {5} off a {2}{G}: the generic half stops at {0} — it does not wrap, and it
        // does not start eating pips.
        assert_eq!(
            cost_of(&db, &["big_cheapener"], "big_beast"),
            parse_mana_cost("{G}")
        );
    }

    #[test]
    fn cr_601_2f_a_coloured_requirement_is_never_reduced() {
        let db = db();
        // A cost that is nothing but pips is unchanged by any reduction, however large.
        let db_cost = {
            let state = board(&db, &["big_cheapener"]);
            let card = crate::fixtures::id_in(&db, "big_beast");
            modified_cast_cost(&state, &db, PlayerId(0), card, parse_mana_cost("{G}{G}{G}"))
        };
        assert_eq!(db_cost, parse_mana_cost("{G}{G}{G}"));
    }

    #[test]
    fn cr_601_2f_increases_and_reductions_are_one_sum_with_one_floor() {
        let db = db();
        // The rule's own arithmetic: the total is the printed cost *plus* every increase,
        // *minus* every reduction, and only the result is held at {0}. So `{2}` printed,
        // `{2}` of tax and `{5}` of reduction is `2 + 2 - 5` held at zero — not a
        // reduction clamped to zero and then taxed back up to `{2}`.
        assert_eq!(
            cost_of(&db, &["taxer", "big_cheapener"], "big_beast").generic,
            0
        );
        // With nothing to floor, the sum is just the sum.
        assert_eq!(
            cost_of(&db, &["taxer", "cheapener"], "big_beast"),
            parse_mana_cost("{2}{G}"),
            "{{2}} on and {{2}} off leaves the printed cost"
        );
        assert_eq!(
            cost_of(&db, &["taxer"], "big_beast"),
            parse_mana_cost("{4}{G}")
        );
    }

    #[test]
    fn a_spell_outside_the_class_is_not_modified() {
        let db = db();
        assert_eq!(
            cost_of(&db, &["cheapener"], "small_beast"),
            parse_mana_cost("{2}{G}"),
            "a 3/3 is below the bound the reducer names"
        );
        assert_eq!(
            cost_of(&db, &["cheapener"], "cheapener"),
            parse_mana_cost("{1}"),
            "and an artifact spell is not a creature spell"
        );
    }

    #[test]
    fn a_modifier_an_opponent_controls_does_not_apply() {
        let db = db();
        let mut state = board(&db, &["cheapener"]);
        for perm in &mut state.battlefield {
            perm.controller = PlayerId(1);
        }
        let card = crate::fixtures::id_in(&db, "big_beast");
        assert_eq!(
            modified_cast_cost(&state, &db, PlayerId(0), card, parse_mana_cost("{2}{G}")),
            parse_mana_cost("{2}{G}"),
            "the ability says `you cast`, and player 0 is not its `you`"
        );
    }
}
