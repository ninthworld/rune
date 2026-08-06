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
#[cfg(test)]
mod toughness_tests;

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
        apply_step(state, db, DamageStep::FirstStrike, &blocked);
        // CR 510.5: SBAs are checked between the two combat-damage steps.
        run_state_based_actions(state, db);
        apply_step(state, db, DamageStep::Regular, &blocked);
    } else {
        apply_step(state, db, DamageStep::Only, &blocked);
    }
}

/// Compute and apply one combat-damage step: the batch, then the note on each creature
/// that dealt something in it (CR 120, [`Permanent::dealt_damage`](crate::Permanent)).
///
/// The note lands with the batch rather than after the whole combat, so a creature that
/// strikes in the first-strike step has dealt damage by the time the state-based actions
/// between the two steps run — and any continuous ability conditioned on not having dealt
/// damage is already off when they do.
///
/// **What was assigned is not what was dealt** (CR 615.1). The attribution names an
/// assignment by its index, and the note is made only for the assignments that survived
/// the prevention shields — so a creature whose whole strike was prevented has still not
/// dealt damage *yet*, exactly as no life moved and nothing was marked. Every other
/// record the shield erases is erased at [`GameState::deal_damage`]; this is the one that
/// is about the **source**, so it is erased here, where the applier knows what landed.
fn apply_step(state: &mut GameState, db: &CardDatabase, step: DamageStep, blocked: &[PermanentId]) {
    let (batch, dealers) = combat_damage_and_dealers(state, db, step, blocked);
    let landed = apply_combat_batch(state, batch, db);
    for (dealer, assignment) in dealers {
        if landed.get(assignment).copied().unwrap_or(0) > 0 {
            state.note_damage_dealt_by(dealer);
        }
    }
}

/// Apply one combat-damage step's computed batch to `state` (CR 510.2). Life
/// changes and marked damage land together; a deathtouch mark records the
/// recipient for the CR 704.5h state-based action, and lifelink life gain rides
/// the same batch as the damage (CR 702.15e).
///
/// Every assignment goes through [`GameState::deal_damage`], which is where a
/// prevention shield is consulted (CR 615.1) — so combat damage is prevented by exactly
/// the mechanism a burn spell's damage is, and this step contains no clause about it.
/// What comes back is the damage that was actually **dealt**, and it is that number, not
/// the assigned one, that flags a deathtouch recipient, feeds the CR 903.10a commander
/// tally, and gains a lifelink source life: fully prevented damage does none of the
/// three.
///
/// It is also what the caller returns: one amount per entry of `batch`, in the same
/// order, so the fourth reader of "was this really dealt" — the note on the creature that
/// dealt it ([`apply_step`]) — asks the same question off the same answer rather than
/// re-deriving it from the assignment.
pub(crate) fn apply_combat_batch(
    state: &mut GameState,
    batch: Vec<CombatDamage>,
    db: &CardDatabase,
) -> Vec<u32> {
    let mut landed = Vec::with_capacity(batch.len());
    for assignment in batch {
        let dealt = match assignment {
            CombatDamage::ToPlayer {
                player,
                amount,
                source_commander,
                lifelink,
            } => {
                // Damage to a player is life loss recorded as a `DamageDealt` event
                // (not a bare life change), so a client can report the hit.
                let dealt =
                    state.deal_damage(PendingDamage::to_player(player, amount).in_combat(), db);
                // CR 903.10a: combat damage from a commander also accrues to the
                // per-designation tally that the 21-damage loss reads. Keyed to the
                // commander's owning player, so it survives the commander's zone
                // changes; non-combat damage never reaches this seam, so it never
                // counts.
                if let Some(commander) = source_commander {
                    state.add_commander_damage(commander, player, dealt);
                }
                gain_lifelink(state, lifelink, dealt);
                dealt
            }
            CombatDamage::ToPermanent {
                permanent,
                amount,
                deathtouch,
                lifelink,
            } => {
                // The one damage seam: it marks the damage on a creature and removes
                // loyalty from a planeswalker (CR 120.3c/d), recording the
                // `DamageDealt` event either way.
                let dealt = state.deal_damage(
                    PendingDamage::to_permanent(permanent, amount).in_combat(),
                    db,
                );
                // CR 702.2b / 704.5h: any nonzero damage from a deathtouch source
                // makes the recipient a candidate for destruction.
                if dealt > 0 && deathtouch && !state.deathtouch_struck.contains(&permanent) {
                    state.deathtouch_struck.push(permanent);
                }
                gain_lifelink(state, lifelink, dealt);
                dealt
            }
        };
        landed.push(dealt);
    }
    landed
}

/// Gain `dealt` life for a lifelink source's controller (CR 702.15e), if the source had
/// lifelink and any damage was actually dealt.
///
/// A non-damage life change, simultaneous with the damage because it is applied in the
/// same pass over the batch. Damage that was prevented gains nothing: CR 702.15e is
/// about damage *dealt*, and there was none.
fn gain_lifelink(state: &mut GameState, lifelink: Option<PlayerId>, dealt: u32) {
    if let (Some(player), true) = (lifelink, dealt > 0) {
        state.change_life(player, i32::try_from(dealt).unwrap_or(i32::MAX));
    }
}

/// End-of-combat turn-based action: remove every creature from combat (CR 511.3)
/// by clearing the attacking flag and blocking assignments on every permanent. The
/// per-turn declaration flags are reset when the next turn begins
/// ([`GameState::begin_next_turn`]), so a fresh combat starts clean.
pub(crate) fn remove_creatures_from_combat(state: &mut GameState) {
    for perm in &mut state.battlefield {
        perm.attacking = None;
        perm.blocking.clear();
    }
}
