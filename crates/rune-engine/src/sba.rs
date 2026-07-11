//! State-based actions: the checks the engine applies to a fixed point after
//! every action (CR 704). [`crate::apply_action`] calls
//! [`run_state_based_actions`] as a pipeline stage.

use crate::state::GameState;

/// Run state-based actions to a fixed point: keep applying them until a full
/// pass changes nothing. Pure over the owned state. The only rule modeled today
/// is CR 704.5a — a player at 0 or less life loses the game.
pub(crate) fn run_state_based_actions(state: &mut GameState) {
    loop {
        let mut changed = false;
        for player in &mut state.players {
            if player.life <= 0 && !player.has_lost {
                player.has_lost = true;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::actions::Action;
    use crate::apply_action;
    use crate::CardDatabase;

    /// The bundled card database, for tests that need oracle data.
    fn db() -> CardDatabase {
        CardDatabase::bundled().unwrap()
    }

    #[test]
    fn state_based_actions_mark_a_player_at_zero_life_as_lost() {
        let mut state = GameState::new_two_player();
        state.players[1].life = 0;
        let after = apply_action(&state, &Action::PassPriority, &db());
        assert!(after.players[1].has_lost);
        assert!(!after.players[0].has_lost);
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
