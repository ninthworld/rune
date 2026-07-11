//! State-based actions: the checks the engine applies to a fixed point after
//! every action (CR 704). [`crate::apply_action`] calls
//! [`run_state_based_actions`] as a pipeline stage.

use crate::characteristics::characteristics;
use crate::id::{CardInstance, PermanentId};
use crate::player::LossReason;
use crate::state::{GameState, Permanent};
use crate::CardDatabase;

/// Run state-based actions to a fixed point: keep applying them until a full
/// pass changes nothing (CR 704.3). Pure over the owned state. Takes `db` for
/// the current-toughness read the lethal-damage check needs (CR 704.5g).
///
/// Modeled today:
/// - **CR 704.5a** — a player at 0 or less life loses the game (combat life loss
///   flows in here).
/// - **CR 704.5c** — a player who attempted to draw from an empty library since
///   the last check loses the game (decking); the attempt is flagged on the
///   player by [`crate::Player::draw`] and consumed here.
/// - **CR 704.5g** — a creature with lethal marked damage (damage ≥ its
///   toughness, toughness > 0) is destroyed and put into its owner's graveyard.
/// - **CR 704.5h** — a creature dealt any nonzero damage this turn by a source
///   with deathtouch is destroyed, regardless of whether that damage is lethal by
///   toughness. The struck creatures are recorded in
///   [`GameState::deathtouch_struck`](crate::GameState::deathtouch_struck) when
///   combat damage is applied and consumed (drained) here.
///
/// These run in the same loop so a chain settles in one call: e.g. a creature
/// dying does not itself change a life total today, but keeping the checks in one
/// fixed-point pass is what CR 704.3 requires as more actions land. Consuming a
/// loss into the terminal [`GameResult`](crate::GameResult) — deciding the winner
/// once one player remains (CR 104.2a) — is a pure derivation done on read, not
/// stored here.
pub(crate) fn run_state_based_actions(state: &mut GameState, db: &CardDatabase) {
    loop {
        let mut changed = false;
        // Losing conditions, unified (CR 704.5). Each marks the player as having
        // lost and records why, exactly once.
        for player in &mut state.players {
            // CR 704.5a: a player at 0 or less life loses.
            if player.life <= 0 && !player.has_lost {
                player.has_lost = true;
                player.loss_reason = Some(LossReason::ZeroLife);
                changed = true;
            }
            // CR 704.5c: a player who attempted to draw from an empty library
            // loses. Consume the flag so the pass reaches a fixed point.
            if player.attempted_draw_from_empty {
                player.attempted_draw_from_empty = false;
                if !player.has_lost {
                    player.has_lost = true;
                    player.loss_reason = Some(LossReason::DrewFromEmptyLibrary);
                }
                changed = true;
            }
        }
        // CR 704.5h: a creature dealt damage by a deathtouch source is destroyed.
        // The set is recorded when combat damage is applied (see
        // `apply.rs :: deal_combat_damage`); draining it here consumes the flag so
        // the pass reaches a fixed point, mirroring the empty-library-draw flag.
        let struck = std::mem::take(&mut state.deathtouch_struck);
        if !struck.is_empty() {
            changed = true;
        }
        // CR 704.5g/704.5h: destroy every creature with lethal marked damage or
        // flagged as struck by deathtouch. Collected before mutating so the whole
        // set is judged against one snapshot (the checks are simultaneous,
        // CR 704.3), then each is moved to its owner's graveyard.
        let doomed: Vec<PermanentId> = state
            .battlefield
            .iter()
            .filter(|perm| has_lethal_damage(perm, state, db) || struck.contains(&perm.id))
            .map(|perm| perm.id)
            .collect();
        for id in doomed {
            if let Some(pos) = state.battlefield.iter().position(|p| p.id == id) {
                let perm = state.battlefield.remove(pos);
                // Ownership is approximated by controller until separate ownership
                // tracking lands (mirrors the engine→protocol `owner` shim).
                if let Some(owner) = state.players.get_mut(perm.controller.0) {
                    owner.graveyard.push(CardInstance {
                        id: perm.instance,
                        card: perm.card,
                    });
                }
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}

/// Whether `perm` has lethal marked damage (CR 704.5g): it is a creature with
/// toughness greater than 0 whose marked damage is at least that toughness.
/// Current toughness is read through [`characteristics`], so counters and
/// anthems are folded in. A non-creature (no toughness) is never lethal here.
fn has_lethal_damage(perm: &Permanent, state: &GameState, db: &CardDatabase) -> bool {
    match characteristics(state, perm.id, db).toughness {
        Some(toughness) if toughness > 0 => {
            perm.damage >= u32::try_from(toughness).unwrap_or(u32::MAX)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::actions::Action;
    use crate::apply_action;
    use crate::id::{CardId, PlayerId};
    use crate::state::{CounterKind, Permanent};
    use crate::CardDatabase;

    /// The bundled card database, for tests that need oracle data.
    fn db() -> CardDatabase {
        CardDatabase::bundled().unwrap()
    }

    /// Place a permanent of `card` under `controller` with `damage` marked; return
    /// its fresh id.
    fn place(
        state: &mut GameState,
        card: CardId,
        controller: PlayerId,
        damage: u32,
    ) -> PermanentId {
        let inst = state.new_instance(card);
        let id = PermanentId(state.mint_id());
        state.battlefield.push(Permanent {
            id,
            instance: inst.id,
            card,
            controller,
            tapped: false,
            entered_turn: 0,
            attacking: false,
            blocking: None,
            damage,
            counters: Default::default(),
        });
        id
    }

    #[test]
    fn cr_704_5g_creature_with_lethal_marked_damage_is_destroyed() {
        // CR 704.5g: a creature with damage marked greater than or equal to its
        // toughness is destroyed and put into its owner's graveyard. Thornback
        // Boar is a 3/2; two marked damage is lethal.
        let db = db();
        let mut state = GameState::new_two_player();
        let boar = place(&mut state, CardId(1), PlayerId(0), 2);

        run_state_based_actions(&mut state, &db);

        assert!(
            !state.battlefield.iter().any(|p| p.id == boar),
            "a creature with lethal marked damage leaves the battlefield (CR 704.5g)"
        );
        assert_eq!(
            state.players[0].graveyard.len(),
            1,
            "the destroyed creature is in its owner's graveyard"
        );
    }

    #[test]
    fn cr_704_5g_creature_below_lethal_survives() {
        // CR 704.5g: damage below toughness is not lethal. A 3/2 Boar with one
        // marked damage stays on the battlefield.
        let db = db();
        let mut state = GameState::new_two_player();
        let boar = place(&mut state, CardId(1), PlayerId(0), 1);

        run_state_based_actions(&mut state, &db);

        assert!(state.battlefield.iter().any(|p| p.id == boar));
        assert!(state.players[0].graveyard.is_empty());
    }

    #[test]
    fn cr_704_5g_lethality_reads_current_toughness_with_counters() {
        // CR 704.5g reads *current* toughness (CR 613 layer 7c). A +1/+1 counter
        // makes the 3/2 Boar a 3/3, so two damage is no longer lethal — but three
        // is. This proves the SBA folds counters in, not the printed toughness.
        let db = db();
        let mut state = GameState::new_two_player();
        let boar = place(&mut state, CardId(1), PlayerId(0), 2);
        if let Some(perm) = state.battlefield.iter_mut().find(|p| p.id == boar) {
            perm.counters.insert(CounterKind::PlusOnePlusOne, 1);
        }

        run_state_based_actions(&mut state, &db);
        assert!(
            state.battlefield.iter().any(|p| p.id == boar),
            "2 damage is not lethal to a 3/3 (printed 3/2 + counter)"
        );

        if let Some(perm) = state.battlefield.iter_mut().find(|p| p.id == boar) {
            perm.damage = 3;
        }
        run_state_based_actions(&mut state, &db);
        assert!(
            !state.battlefield.iter().any(|p| p.id == boar),
            "3 damage is lethal to a 3/3 (CR 704.5g)"
        );
    }

    #[test]
    fn cr_704_5h_deathtouch_struck_creature_is_destroyed_below_lethal_damage() {
        // CR 704.5h: a creature flagged as struck by a deathtouch source is
        // destroyed even though its 1 marked damage is far below its toughness. The
        // Basilisk (4/5) survives 1 marked damage by CR 704.5g but not the flag.
        let db = db();
        let mut state = GameState::new_two_player();
        let basilisk = place(&mut state, CardId(4), PlayerId(0), 1); // 1 marked damage
        state.deathtouch_struck.push(basilisk);

        run_state_based_actions(&mut state, &db);

        assert!(
            !state.battlefield.iter().any(|p| p.id == basilisk),
            "a deathtouch-struck creature is destroyed (CR 704.5h)"
        );
        assert_eq!(state.players[0].graveyard.len(), 1);
        assert!(
            state.deathtouch_struck.is_empty(),
            "the deathtouch flag is consumed so the loop settles"
        );
    }

    #[test]
    fn cr_704_5h_stale_deathtouch_flag_settles_without_error() {
        // A struck id whose creature is already gone is drained harmlessly and the
        // loop still reaches a fixed point (no infinite loop, no panic).
        let db = db();
        let mut state = GameState::new_two_player();
        state.deathtouch_struck.push(PermanentId(999));

        run_state_based_actions(&mut state, &db);

        assert!(state.deathtouch_struck.is_empty());
        assert!(state.battlefield.is_empty());
    }

    #[test]
    fn state_based_actions_mark_a_player_at_zero_life_as_lost() {
        let mut state = GameState::new_two_player();
        state.players[1].life = 0;
        let after = apply_action(&state, &Action::PassPriority, &db());
        assert!(after.players[1].has_lost);
        // CR 704.5a: the loss records its reason so the terminal result can name it.
        assert_eq!(after.players[1].loss_reason, Some(LossReason::ZeroLife));
        assert!(!after.players[0].has_lost);
    }

    #[test]
    fn cr_704_5c_attempted_draw_from_empty_library_loses() {
        // CR 704.5c: an attempted draw from an empty library, flagged on the
        // player, is consumed by the SBA loop into a loss and the flag is cleared
        // so the pass reaches a fixed point.
        let db = db();
        let mut state = GameState::new_two_player();
        state.players[0].attempted_draw_from_empty = true;

        run_state_based_actions(&mut state, &db);

        assert!(
            state.players[0].has_lost,
            "decking loses the game (CR 704.5c)"
        );
        assert_eq!(
            state.players[0].loss_reason,
            Some(LossReason::DrewFromEmptyLibrary)
        );
        assert!(
            !state.players[0].attempted_draw_from_empty,
            "the attempt flag is consumed so the loop settles"
        );
    }

    #[test]
    fn state_based_actions_reach_a_fixed_point() {
        // Running SBAs on an already-settled state changes nothing (a second
        // application is idempotent), i.e. the loop terminates at a fixed point.
        let db = db();
        let mut state = GameState::new_two_player();
        state.players[0].life = -3;
        let once = apply_action(&state, &Action::PassPriority, &db);
        let twice = apply_action(&once, &Action::PassPriority, &db);
        assert!(once.players[0].has_lost);
        assert_eq!(once.players[0].has_lost, twice.players[0].has_lost);
    }
}
