//! Declaring combat: who attacks whom, who blocks what, and the order an attacker
//! puts its blockers in (CR 508, CR 509, CR 510.1c).

use super::*;

#[cfg(test)]
mod tests;

/// Declare the active player's attackers (CR 508.1): mark each as attacking and
/// tap it (attacking taps, CR 508.1f) unless it has vigilance (CR 702.20b), then
/// record that the declaration is done and open the step's priority round with the
/// active player. An empty selection is a legal "no attackers" declaration
/// (CR 508.1a).
///
/// Only ever reached during the declare-attackers step for the active player (the
/// action is offered nowhere else — see [`crate::valid_actions`]) and only for a
/// selection already validated in [`action_is_legal`].
pub(crate) fn apply_declare_attackers(
    state: &mut GameState,
    attackers: &[Attack],
    db: &CardDatabase,
) {
    for attack in attackers {
        // CR 508.1f / CR 702.20b: whether attacking taps this creature, through the
        // engine's own predicate — the same one the server projects so a client can draw
        // a declaration before it is sent ([`crate::attacking_taps`]). One answer, so a
        // board a player was shown and the board they get cannot disagree. Resolved
        // before the mutable lookup below, since it borrows `state` immutably.
        let taps = attacking_taps(state, attack.attacker, db);
        if let Some(perm) = state
            .battlefield
            .iter_mut()
            .find(|p| p.id == attack.attacker)
        {
            // CR 508.1a: record whom this attacker is attacking, so blocker
            // eligibility and combat damage follow the assignment (issue #341).
            perm.attacking = Some(attack.defender);
            if taps {
                perm.tapped = true;
            }
        }
    }
    state.attackers_declared = true;
    // Record the declaration with each attacker's card identity, so the log can name
    // it even after it has left combat or the battlefield (CR 508.1).
    let declared: Vec<LoggedPermanent> = attackers
        .iter()
        .map(|attack| logged_permanent(state, attack.attacker))
        .collect();
    state.record_event(GameEvent::AttackersDeclared {
        player: state.active_player,
        attackers: declared,
    });
    // The declaration made, the declare-attackers step proceeds to its normal
    // priority round beginning with the active player (CR 508.2).
    state.priority = state.active_player;
    state.consecutive_passes = 0;
}

/// Pair a battlefield permanent's id with the identity that names it in a log
/// event, so the name is projectable later even once the permanent has left the
/// battlefield — including a token, which by then has ceased to exist (CR 111.7) and
/// whose name is therefore the only thing left to record. A missing permanent falls
/// back to a default [`CardId`] — the callers pass ids validated to be on the
/// battlefield, so this is defensive only.
fn logged_permanent(state: &GameState, id: PermanentId) -> LoggedPermanent {
    state.battlefield.iter().find(|p| p.id == id).map_or_else(
        || LoggedPermanent {
            permanent: id,
            identity: LoggedIdentity::Card(CardId::default()),
        },
        LoggedPermanent::of,
    )
}

/// Declare one attacked player's blockers (CR 509.1): record each blocker's
/// assignment to its attacker and either hand the next attacked player their own
/// declaration (multi-defender combat, APNAP order — issue #344) or, once every
/// attacked player has declared, open the step's priority round with the active
/// player (CR 509.4). An empty selection is a legal "no blockers" declaration.
///
/// Only ever reached during the declare-blockers step for the player who currently
/// owes the declaration ([`pending_blocker_declarer`]), and only for a selection
/// already validated in [`action_is_legal`]. Combat damage is computed later, at
/// the combat-damage step, so it is computed once — after every attacked player has
/// declared.
pub(crate) fn apply_declare_blockers(state: &mut GameState, blocks: &[Block]) {
    // The player who owes this declaration, captured before recording changes who
    // owes the next one.
    let declarer = pending_blocker_declarer(state).unwrap_or(state.active_player);
    for block in blocks {
        if let Some(perm) = state.battlefield.iter_mut().find(|p| p.id == block.blocker) {
            perm.blocking = Some(block.attacker);
        }
    }
    // Record the assignments with both creatures' card identity for stable naming.
    let declared: Vec<(LoggedPermanent, LoggedPermanent)> = blocks
        .iter()
        .map(|block| {
            (
                logged_permanent(state, block.blocker),
                logged_permanent(state, block.attacker),
            )
        })
        .collect();
    state.record_event(GameEvent::BlockersDeclared {
        player: declarer,
        blocks: declared,
    });
    // Mark this defender done and decide whether any attacked player still owes a
    // declaration. Two-player combat has a single declarer, so the one declaration
    // completes the step; multi-defender combat tracks each declarer and finishes
    // only once none remain.
    if defending_player(state).is_some() {
        state.blockers_declared = true;
    } else {
        state.blockers_declared_by.push(declarer);
        if pending_blocker_declarer(state).is_none() {
            state.blockers_declared = true;
        }
    }
    state.priority = if state.blockers_declared {
        // Every declaration is in: the step's normal priority round opens with the
        // active player (CR 509.4).
        state.active_player
    } else {
        // The next attacked player (APNAP order) declares before priority is passed.
        pending_blocker_declarer(state).unwrap_or(state.active_player)
    };
    state.consecutive_passes = 0;
}

/// Record the attacking player's combat-damage assignment orders (CR 510.1, issue
/// #346) and open the declare-blockers priority round. Each order is stored on
/// [`GameState::damage_orders`], where [`crate::combat::combat_damage`] reads it to
/// assign lethal-before-next along the chosen sequence; an attacker without a stored
/// order keeps stable battlefield order. Only ever reached for the attacking player
/// once every owed order is supplied (validated in [`action_is_legal`]).
pub(crate) fn apply_order_combat_damage(state: &mut GameState, orders: &[DamageOrder]) {
    for order in orders {
        state
            .damage_orders
            .push((order.attacker, order.blockers.clone()));
    }
    // Every owed order is in; the step's normal priority round opens with the active
    // player before combat damage is dealt (CR 510.1 precedes the damage step).
    state.priority = state.active_player;
    state.consecutive_passes = 0;
}
