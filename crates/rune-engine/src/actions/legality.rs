//! Action legality validation — checking that actions conform to game rules.

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
use super::utilities::all_unique;

/// Whether `action` — including any targets it carries — is legal against the
/// current `state`. This is the gate [`crate::apply_action`] runs before it
/// applies anything.
///
/// Two independent checks, mirroring ADR 0009 §Enumeration:
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
