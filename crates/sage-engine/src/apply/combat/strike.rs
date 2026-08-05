//! Dealing combat damage and clearing combat: the one or two damage steps of
//! CR 510.5, the batch that lands simultaneously (CR 510.2), and the end-of-combat
//! removal of CR 511.3.

use super::*;

#[cfg(test)]
mod dies_tests;
#[cfg(test)]
mod double_strike_tests;
#[cfg(test)]
mod tests;

/// Combat-damage step turn-based action: deal all combat damage (CR 510).
///
/// If any creature in combat has first strike there are **two** damage steps
/// (CR 510.5): first-strikers deal in the first, everyone else in the second, and
/// the state-based-actions loop runs *between* them so a creature killed by first
/// strike is gone before it would deal its regular-step damage. Otherwise a single
/// ordinary step is dealt. Each step's assignments are computed
/// ([`combat_damage`]) then applied in one pass, so the batch lands simultaneously
/// (CR 510.2): damage to a player is life loss (feeding CR 704.5a), damage to a
/// creature is marked (CR 120.3) for CR 704.5g, deathtouch damage additionally
/// flags its recipient for CR 704.5h, and lifelink gains life in the same batch
/// (CR 702.15e). The set of blocked attackers is captured up front so a blocked
/// creature whose blockers died to first strike is not re-read as unblocked
/// (CR 509.1h). The pipeline's state-based-actions loop runs again after this.
pub(crate) fn deal_combat_damage(state: &mut GameState, db: &CardDatabase) {
    // CR 509.1h: which attackers are blocked is fixed before any damage is dealt.
    let blocked = blocked_attackers(state);
    if combat_has_first_strike(state, db) {
        apply_combat_batch(
            state,
            combat_damage(state, db, DamageStep::FirstStrike, &blocked),
            db,
        );
        // CR 510.5: SBAs are checked between the two combat-damage steps.
        run_state_based_actions(state, db);
        apply_combat_batch(
            state,
            combat_damage(state, db, DamageStep::Regular, &blocked),
            db,
        );
    } else {
        apply_combat_batch(
            state,
            combat_damage(state, db, DamageStep::Only, &blocked),
            db,
        );
    }
}

/// Apply one combat-damage step's computed batch to `state` (CR 510.2). Life
/// changes and marked damage land together; a deathtouch mark records the
/// recipient for the CR 704.5h state-based action, and lifelink life gain rides
/// the same batch as the damage (CR 702.15e).
pub(crate) fn apply_combat_batch(
    state: &mut GameState,
    batch: Vec<CombatDamage>,
    db: &CardDatabase,
) {
    for assignment in batch {
        match assignment {
            CombatDamage::ToPlayer {
                player,
                amount,
                source_commander,
            } => {
                // Damage to a player is life loss recorded as a `DamageDealt` event
                // (not a bare life change), so a client can report the hit.
                state.deal_damage_to_player(player, amount);
                // CR 903.10a: combat damage from a commander also accrues to the
                // per-designation tally that the 21-damage loss reads. Keyed to the
                // commander's owning player, so it survives the commander's zone
                // changes; non-combat damage never reaches this seam, so it never
                // counts.
                if let Some(commander) = source_commander {
                    state.add_commander_damage(commander, player, amount);
                }
            }
            CombatDamage::ToPermanent {
                permanent,
                amount,
                deathtouch,
            } => {
                // The one damage-to-a-permanent seam: it marks the damage on a
                // creature and removes loyalty from a planeswalker (CR 120.3c/d),
                // recording the `DamageDealt` event either way.
                let marked = state.deal_damage_to_permanent(permanent, amount, db);
                // CR 702.2b / 704.5h: any nonzero damage from a deathtouch source
                // makes the recipient a candidate for destruction.
                if marked
                    && deathtouch
                    && amount > 0
                    && !state.deathtouch_struck.contains(&permanent)
                {
                    state.deathtouch_struck.push(permanent);
                }
            }
            CombatDamage::GainLife { player, amount } => {
                // Lifelink life gain is a non-damage life change (CR 702.15e).
                state.change_life(player, i32::try_from(amount).unwrap_or(i32::MAX));
            }
        }
    }
}

/// End-of-combat turn-based action: remove every creature from combat (CR 511.3)
/// by clearing the attacking flag and blocking assignment on every permanent. The
/// per-turn declaration flags are reset when the next turn begins
/// ([`GameState::begin_next_turn`]), so a fresh combat starts clean.
pub(crate) fn remove_creatures_from_combat(state: &mut GameState) {
    for perm in &mut state.battlefield {
        perm.attacking = None;
        perm.blocking = None;
    }
}
