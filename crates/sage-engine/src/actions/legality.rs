//! Action legality validation — checking that actions conform to game rules.

use crate::ability::Ability;
use crate::card::abilities_of;
use crate::combat::{
    attacker_candidates, attackers_needing_damage_order, attacking_defender_of,
    blocker_can_block_attacker, blocker_candidates_for, declared_attackers, defender_candidates,
    pending_blocker_declarer,
};
use crate::id::PermanentId;
use crate::resolve::target_is_legal;
use crate::state::GameState;
use crate::CardDatabase;

use super::definition::{Action, Attack, Block, DamageOrder};
use super::generation::valid_actions;
use super::targeting::action_target_specs;
use super::utilities::{all_unique, tap_cost_is_summoning_sick};

/// Whether `action` — including any targets it carries — is legal against the
/// current `state`. This is the gate [`crate::apply_action`] runs before it
/// applies anything.
///
/// Two independent checks, mirroring ADR 0004 §Enumeration:
/// 1. **Base legality.** The action, with its targets cleared to the requirement
///    form, must be one [`valid_actions`] currently offers.
/// 2. **Target legality.** The carried targets must exactly fill the action's
///    slots, and each must lie in that slot's *freshly computed* legal set. This
///    extends the regenerate-and-check discipline of [`crate::apply_action`] to
///    targets: legality is re-derived from current state, never read back from an
///    exhaustively enumerated list of target combinations.
#[must_use]
pub(crate) fn action_is_legal(state: &GameState, action: &Action, db: &CardDatabase) -> bool {
    // 1. The bare action must be on offer. Comparing the requirement form keeps
    //    this O(number of distinct actions), independent of how many targets each
    //    could take — no combination is ever enumerated here.
    if !valid_actions(state, db).contains(&action.without_targets()) {
        return false;
    }

    // 1a. A mulligan keep validates its bottoming selection (CR 103.5) rather than
    //     the target-slot machinery: exactly one distinct hand card per mulligan
    //     taken (see [`crate::mulligan::keep_bottom_is_legal`]).
    if let Action::Keep { bottom } = action {
        return crate::mulligan::keep_bottom_is_legal(state, bottom);
    }

    // 1b. Combat declarations carry a permanent multi-select rather than
    //     ability targets: validate the selection against the freshly computed
    //     candidate sets (CR 508.1a / 509.1a), the same regenerate-and-check
    //     discipline the target path uses. An empty selection is always legal.
    match action {
        Action::DeclareAttackers { attackers } => {
            return attackers_selection_is_legal(state, db, attackers);
        }
        Action::DeclareBlockers { blocks } => {
            return blocks_selection_is_legal(state, db, blocks);
        }
        Action::OrderCombatDamage { orders } => {
            return damage_orders_are_legal(state, orders);
        }
        _ => {}
    }

    // 1c. Hardening (CR 302.6, issue #454): a `{T}`-cost ability of a summoning-sick
    //     creature is never activatable. Check 1 above already withholds the offer,
    //     so this is a second, independent gate that re-derives the restriction from
    //     current state — a stale or forged action id can never slip a sick creature's
    //     tap ability through [`crate::apply_action`].
    if let Action::ActivateAbility {
        permanent, index, ..
    } = action
    {
        if !activation_clears_summoning_sickness(state, db, *permanent, *index) {
            return false;
        }
    }

    // 2. The carried targets must fill every slot the action declares, each with
    //    a target that is legal *now*. `target_is_legal` is the same predicate the
    //    resolve path re-checks with (CR 608.2b) and the one `legal_targets_for_spec`
    //    filters by, so "in the freshly computed legal set" and "passes the check"
    //    are one and the same — we test membership directly, without building the
    //    set (and certainly without the cartesian product).
    let specs = action_target_specs(state, db, action);
    let chosen = action.targets();
    chosen.len() == specs.len()
        && specs
            .iter()
            .zip(chosen)
            .all(|(&spec, &target)| target_is_legal(spec, target, state, db))
}

/// Whether activating ability `index` of `permanent` clears the CR 302.6
/// summoning-sickness restriction (issue #454): `false` exactly when the ability's
/// cost contains `{T}` and its source is a creature that has not been under its
/// controller's control since their most recent turn began (haste, CR 702.10b,
/// exempts it). A mana ability is gated like any other (CR 605.3a).
///
/// `false` for a permanent that is not on the battlefield — a stale id names no
/// source to pay a cost with. `true` for an index that is not an activated ability:
/// there is no `{T}` cost to restrict, and check 1 of [`action_is_legal`] has
/// already rejected the action on its own terms.
fn activation_clears_summoning_sickness(
    state: &GameState,
    db: &CardDatabase,
    permanent: PermanentId,
    index: usize,
) -> bool {
    let Some(perm) = state.battlefield.iter().find(|p| p.id == permanent) else {
        return false;
    };
    match abilities_of(db, perm.card).get(index) {
        Some(Ability::Activated { cost, .. }) => !tap_cost_is_summoning_sick(state, perm, cost, db),
        _ => true,
    }
}

/// Whether a declared attacker selection is legal (CR 508.1a): every named
/// permanent is a current attacker candidate ([`attacker_candidates`]), no
/// permanent is named twice, and every attacker's defender is a legal defender
/// candidate ([`defender_candidates`]) — an opponent still in the game, never the
/// active player and never an eliminated one. An empty selection is legal
/// (declaring no attackers).
pub(crate) fn attackers_selection_is_legal(
    state: &GameState,
    db: &CardDatabase,
    attackers: &[Attack],
) -> bool {
    let candidates = attacker_candidates(state, db);
    let defenders = defender_candidates(state);
    let ids: Vec<PermanentId> = attackers.iter().map(|a| a.attacker).collect();
    all_unique(&ids)
        && attackers
            .iter()
            .all(|a| candidates.contains(&a.attacker) && defenders.contains(&a.defender))
}

/// Whether a declared blocker selection is legal (CR 509.1a): every blocker is a
/// current blocker candidate of the player who owes this declaration
/// ([`blocker_candidates_for`] the [`pending_blocker_declarer`]), every named
/// attacker is currently attacking ([`declared_attackers`]) *and attacking that
/// player* (CR 509.1a — a player blocks only attackers attacking them), no creature
/// is declared as a blocker more than once, and each blocker can legally block the
/// attacker it is assigned to given evasion keywords — a flyer can be blocked only
/// by flying or reach (CR 702.9c, 702.17b, via [`blocker_can_block_attacker`]). An
/// empty selection is legal (declaring no blockers).
///
/// Scoping to the current declarer is what makes the multi-defender flow (issue
/// #344) safe: each attacked player's declaration is validated against exactly
/// their own creatures and the attackers attacking them. Two-player games are
/// unchanged — the sole opponent is the one declarer.
///
/// Evasion is checked per assignment rather than by trimming the candidate set, so
/// a partial block of a mix of flying and ground attackers stays expressible: a
/// ground creature may still block the ground attacker in the same declaration
/// that a flyer blocks the flyer.
fn blocks_selection_is_legal(state: &GameState, db: &CardDatabase, blocks: &[Block]) -> bool {
    let Some(declarer) = pending_blocker_declarer(state) else {
        // No declaration is owed: only the empty selection is vacuously legal.
        return blocks.is_empty();
    };
    let blockers = blocker_candidates_for(state, declarer, db);
    let attackers = declared_attackers(state);
    let assigned: Vec<PermanentId> = blocks.iter().map(|b| b.blocker).collect();
    all_unique(&assigned)
        && blocks.iter().all(|b| {
            blockers.contains(&b.blocker)
                && attackers.contains(&b.attacker)
                // CR 509.1a: the declaring player may block only attackers attacking
                // *them*, so the attacker's chosen defender must be this declarer.
                && attacking_defender_of(state, b.attacker) == Some(declarer)
                && blocker_can_block_attacker(state, b.attacker, b.blocker, db)
        })
}

/// Whether a combat-damage assignment order selection is legal (CR 510.1, issue
/// #346): it names exactly the attackers that owe an order
/// ([`attackers_needing_damage_order`]), each with a permutation of that attacker's
/// own blockers — no missing, extra, duplicated, or foreign blocker. An empty
/// selection is legal only when no attacker owes an order (the choice-free case).
fn damage_orders_are_legal(state: &GameState, orders: &[DamageOrder]) -> bool {
    let mut owed = attackers_needing_damage_order(state);
    // Exactly the owed attackers, once each.
    let named: Vec<PermanentId> = orders.iter().map(|o| o.attacker).collect();
    if !all_unique(&named) {
        return false;
    }
    let mut named_sorted = named.clone();
    named_sorted.sort_by_key(|id| id.0);
    owed.sort_by_key(|id| id.0);
    if named_sorted != owed {
        return false;
    }
    // Each order is a permutation of exactly that attacker's blockers.
    orders.iter().all(|order| {
        let mut declared: Vec<PermanentId> = state
            .battlefield
            .iter()
            .filter(|p| p.blocking == Some(order.attacker))
            .map(|p| p.id)
            .collect();
        let mut chosen = order.blockers.clone();
        declared.sort_by_key(|id| id.0);
        chosen.sort_by_key(|id| id.0);
        all_unique(&order.blockers) && chosen == declared
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]

    use super::*;
    use crate::apply_action;
    use crate::fixtures::fixture;
    use crate::id::PlayerId;
    use crate::phase::Step;
    use crate::state::Permanent;

    /// A two-player game at player 0's precombat main on turn 3 with a Llanowar
    /// Elves that entered on `entered_turn`, and the action that activates its
    /// `{T}: Add {G}`.
    fn elves_state(entered_turn: u32) -> (GameState, CardDatabase, Action) {
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        while state.turn < 3 {
            state = state.advance_to_next_turn();
        }
        state.step = Step::PrecombatMain;
        let card = fixture("llanowar_elves");
        let inst = state.new_instance(card);
        let id = PermanentId(state.mint_id());
        state.battlefield.push(Permanent {
            id,
            instance: inst.id,
            card,
            controller: PlayerId(0),
            tapped: false,
            entered_turn,
            attacking: None,
            blocking: None,
            damage: 0,
            counters: Default::default(),
            attached_to: None,
        });
        let action = Action::ActivateAbility {
            permanent: id,
            index: 0,
            targets: Vec::new(),
        };
        (state, db, action)
    }

    #[test]
    fn issue_454_apply_rejects_a_summoning_sick_tap_ability_handed_directly() {
        // CR 302.6: even handed the action directly — a stale or forged action id
        // that `valid_actions` never offered — the apply path refuses it, so
        // `apply_action` is a no-op (no mana floated, the creature left untapped).
        let (state, db, action) = elves_state(3);
        assert!(!action_is_legal(&state, &action, &db));
        let after = apply_action(&state, &action, &db);
        assert_eq!(after, state, "an illegal activation changes nothing");
        assert_eq!(after.players[0].mana_pool.green, 0);
        assert!(!after.battlefield[0].tapped);
    }

    #[test]
    fn issue_454_the_hardening_gate_is_independent_of_the_offer_check() {
        // The gate re-derives CR 302.6 from current state rather than trusting the
        // offer list, so it rejects on its own — not only because check 1 would.
        let (sick, db, action) = elves_state(3);
        let Action::ActivateAbility {
            permanent, index, ..
        } = action
        else {
            panic!("the fixture builds an activation");
        };
        assert!(!activation_clears_summoning_sickness(
            &sick, &db, permanent, index
        ));

        // The same creature, in play since an earlier turn, clears the gate.
        let (seasoned, db, action) = elves_state(1);
        assert!(activation_clears_summoning_sickness(
            &seasoned, &db, permanent, index
        ));
        assert!(action_is_legal(&seasoned, &action, &db));

        // A permanent id that names nothing on the battlefield clears nothing.
        assert!(!activation_clears_summoning_sickness(
            &seasoned,
            &db,
            PermanentId(9999),
            0
        ));
    }

    #[test]
    fn issue_454_the_gate_holds_while_the_controller_is_not_the_active_player() {
        // The activation the gate protects against is submittable at instant speed
        // during someone *else's* turn, so the gate is measured from the
        // controller's most recent turn. Player 0's Elves entered on their turn 3;
        // through player 1's turn 4 it is still restricted, and only player 0's
        // turn 5 clears it. The turns are walked so the rotation is the engine's.
        let (state, db, action) = elves_state(3);
        let Action::ActivateAbility {
            permanent, index, ..
        } = action
        else {
            panic!("the fixture builds an activation");
        };

        let opponents_turn = state.advance_to_next_turn();
        assert_eq!(
            (opponents_turn.turn, opponents_turn.active_player),
            (4, PlayerId(1))
        );
        assert!(
            !activation_clears_summoning_sickness(&opponents_turn, &db, permanent, index),
            "still restricted during the opponent's turn"
        );
        assert!(!action_is_legal(&opponents_turn, &action, &db));

        let own_turn = opponents_turn.advance_to_next_turn();
        assert_eq!((own_turn.turn, own_turn.active_player), (5, PlayerId(0)));
        assert!(activation_clears_summoning_sickness(
            &own_turn, &db, permanent, index
        ));
    }
}
