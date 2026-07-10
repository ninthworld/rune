//! RUNE rules engine — layer 3.
//!
//! Invariants (see AGENTS.md in this crate):
//! - `GameState` is an immutable value type; `apply_action` returns a new state.
//! - No I/O, no async, no globals, no time. Pure functions only.
//! - Everything derivable is computed on demand (pull-based), never cached on objects.

mod id;
mod phase;
mod player;
mod state;
mod zone;

pub use id::{CardId, PermanentId, PlayerId};
pub use phase::Step;
pub use player::{Player, STARTING_LIFE};
pub use state::{GameState, Permanent};
pub use zone::Zone;

/// An action a player may take. The engine generates the legal set with
/// [`valid_actions`] and validates a chosen action against it in [`apply_action`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Yield priority without taking any other action.
    PassPriority,
}

/// A triggered ability that a state transition has caused to trigger.
///
/// Triggers are collected by diffing the state before and after an action (see
/// [`collect_triggers`]) — never via listeners or observers (crate `AGENTS.md`).
/// No abilities trigger yet, so the diff always yields an empty set today; the
/// type and the diff exist so the pipeline stage is real rather than a `TODO`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trigger {
    /// The permanent whose ability triggered.
    pub source: PermanentId,
}

/// Enumerate the actions legal for the player who currently holds priority.
///
/// Pull-based and pure: computed fresh from `state`, never cached on it. Today
/// the only action is [`Action::PassPriority`], offered to the priority holder.
/// A state with no valid priority holder (as in [`GameState::default`]) offers
/// nothing.
#[must_use]
pub fn valid_actions(state: &GameState) -> Vec<Action> {
    if state.priority_holder().is_some() {
        vec![Action::PassPriority]
    } else {
        Vec::new()
    }
}

/// The single entry point of the engine: a pure state transition.
///
/// Pipeline: validate `action` against [`valid_actions`] → clone → apply →
/// replacement effects (scaffold) → state-based-actions loop → collect triggers
/// → return. An action that is not currently legal is rejected as a no-op: the
/// input is returned unchanged (the input is never mutated either way).
#[must_use]
pub fn apply_action(state: &GameState, action: &Action) -> GameState {
    // 1. Validate against the actions actually on offer. An illegal action is a
    //    no-op — return the input unchanged rather than erroring.
    if !valid_actions(state).contains(action) {
        return state.clone();
    }

    // 2. Clone: every mutation below happens on this owned copy.
    let mut next = state.clone();

    // 3. Apply the chosen action.
    match action {
        Action::PassPriority => apply_pass_priority(&mut next),
    }

    // 4. Replacement effects. Scaffold: no replacement effects are modeled yet,
    //    so this is a documented no-op, wired in for later.
    apply_replacements(&mut next);

    // 5. State-based actions, run to a fixed point.
    run_state_based_actions(&mut next);

    // 6. Collect triggers by diffing before/after. None fire yet; the result is
    //    discarded until there is a stack to put them on.
    let _triggers = collect_triggers(state, &next);

    next
}

/// Resolve a pass of priority. Priority moves to the next seat; once every
/// player has passed in unbroken succession the step ends — the turn structure
/// advances ([`GameState::advance`]) and the new active player receives priority.
fn apply_pass_priority(state: &mut GameState) {
    let seats = state.players.len();
    if seats == 0 {
        return;
    }
    state.consecutive_passes += 1;
    if state.consecutive_passes >= seats {
        *state = state.advance();
        state.consecutive_passes = 0;
        state.priority = state.active_player;
    } else {
        state.priority = PlayerId((state.priority.0 + 1) % seats);
    }
}

/// Apply replacement effects. Scaffold: no replacement effects exist yet, so
/// this is intentionally a no-op. It marks where the pipeline stage lives.
fn apply_replacements(_state: &mut GameState) {}

/// Run state-based actions to a fixed point: keep applying them until a full
/// pass changes nothing. Pure over the owned state. The only rule modeled today
/// is CR 704.5a — a player at 0 or less life loses the game.
fn run_state_based_actions(state: &mut GameState) {
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

/// Collect the triggers that should now exist by diffing `before` against
/// `after`. No abilities trigger yet, so this always returns an empty set; the
/// diff is a pure function of the two states, with no listeners (crate
/// `AGENTS.md`).
#[must_use]
pub fn collect_triggers(_before: &GameState, _after: &GameState) -> Vec<Trigger> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn apply_action_does_not_mutate_input() {
        // PassPriority now changes the state, so the input and output differ —
        // what must hold is that the *input* is untouched (purity).
        let before = GameState::new_two_player();
        let snapshot = before.clone();
        let _after = apply_action(&before, &Action::PassPriority);
        assert_eq!(before, snapshot);
    }

    #[test]
    fn valid_actions_offers_pass_priority_to_the_priority_holder() {
        let state = GameState::new_two_player();
        assert_eq!(valid_actions(&state), vec![Action::PassPriority]);
    }

    #[test]
    fn valid_actions_on_seatless_state_is_empty() {
        // Default has no players, so no one holds priority and nothing is legal.
        assert!(valid_actions(&GameState::default()).is_empty());
    }

    #[test]
    fn illegal_action_is_a_no_op() {
        // On a seatless state PassPriority is not on offer; applying it must
        // leave the state unchanged.
        let state = GameState::default();
        let after = apply_action(&state, &Action::PassPriority);
        assert_eq!(after, state);
    }

    #[test]
    fn passing_priority_rotates_before_the_step_ends() {
        // First pass hands priority to the other seat without ending the step.
        let state = GameState::new_two_player();
        let after = apply_action(&state, &Action::PassPriority);
        assert_eq!(after.priority, PlayerId(1));
        assert_eq!(after.consecutive_passes, 1);
        assert_eq!(after.step, Step::Untap);
        assert_eq!(after.active_player, PlayerId(0));
    }

    #[test]
    fn a_full_round_of_passes_advances_the_step() {
        // Both players pass in succession: the step advances and priority
        // returns to the active player with the pass count reset.
        let state = GameState::new_two_player();
        let state = apply_action(&state, &Action::PassPriority);
        let state = apply_action(&state, &Action::PassPriority);
        assert_eq!(state.step, Step::Upkeep);
        assert_eq!(state.turn, 1);
        assert_eq!(state.active_player, PlayerId(0));
        assert_eq!(state.priority, PlayerId(0));
        assert_eq!(state.consecutive_passes, 0);
    }

    #[test]
    fn state_based_actions_mark_a_player_at_zero_life_as_lost() {
        let mut state = GameState::new_two_player();
        state.players[1].life = 0;
        let after = apply_action(&state, &Action::PassPriority);
        assert!(after.players[1].has_lost);
        assert!(!after.players[0].has_lost);
    }

    #[test]
    fn state_based_actions_reach_a_fixed_point() {
        // Running SBAs on an already-settled state changes nothing (a second
        // application is idempotent), i.e. the loop terminates at a fixed point.
        let mut state = GameState::new_two_player();
        state.players[0].life = -3;
        let once = apply_action(&state, &Action::PassPriority);
        let twice = apply_action(&once, &Action::PassPriority);
        assert!(once.players[0].has_lost);
        assert_eq!(once.players[0].has_lost, twice.players[0].has_lost);
    }

    #[test]
    fn trigger_diff_yields_nothing_for_a_plain_transition() {
        let before = GameState::new_two_player();
        let after = before.advance();
        assert!(collect_triggers(&before, &after).is_empty());
    }

    #[test]
    fn new_two_player_initial_invariants() {
        let state = GameState::new_two_player();
        assert_eq!(state.turn, 1);
        assert_eq!(state.active_player, PlayerId(0));
        assert_eq!(state.step, Step::Untap);
        assert_eq!(state.players.len(), 2);
        assert!(state.battlefield.is_empty());

        for player in &state.players {
            assert_eq!(player.life, STARTING_LIFE);
            assert!(player.library.is_empty());
            assert!(player.hand.is_empty());
            assert!(player.graveyard.is_empty());
            assert!(player.exile.is_empty());
        }

        // The active player resolves to an actual seat.
        let active = state.active_player().unwrap();
        assert_eq!(active.life, STARTING_LIFE);
    }

    #[test]
    fn default_state_is_empty() {
        let state = GameState::default();
        assert_eq!(state.turn, 0);
        assert_eq!(state.step, Step::Untap);
        assert!(state.players.is_empty());
        // No seats, so there is no active player to borrow.
        assert!(state.active_player().is_none());
    }

    #[test]
    fn step_next_cycles_through_the_turn() {
        // Twelve steps, wrapping back to Untap.
        let mut step = Step::Untap;
        let sequence = [
            Step::Upkeep,
            Step::Draw,
            Step::PrecombatMain,
            Step::BeginCombat,
            Step::DeclareAttackers,
            Step::DeclareBlockers,
            Step::CombatDamage,
            Step::EndCombat,
            Step::PostcombatMain,
            Step::End,
            Step::Cleanup,
            Step::Untap,
        ];
        for expected in sequence {
            step = step.next();
            assert_eq!(step, expected);
        }
    }

    #[test]
    fn advance_walks_one_full_turn_without_rotating() {
        // From Untap, eleven advances reach Cleanup, all within turn 1 for the
        // same active player — no rotation happens mid-turn.
        let mut state = GameState::new_two_player();
        let sequence = [
            Step::Upkeep,
            Step::Draw,
            Step::PrecombatMain,
            Step::BeginCombat,
            Step::DeclareAttackers,
            Step::DeclareBlockers,
            Step::CombatDamage,
            Step::EndCombat,
            Step::PostcombatMain,
            Step::End,
            Step::Cleanup,
        ];
        for expected in sequence {
            state = state.advance();
            assert_eq!(state.step, expected);
            assert_eq!(state.turn, 1);
            assert_eq!(state.active_player, PlayerId(0));
        }
    }

    #[test]
    fn advance_past_cleanup_starts_next_players_turn() {
        let mut state = GameState::new_two_player();
        state.step = Step::Cleanup;

        let next = state.advance();
        assert_eq!(next.turn, 2);
        assert_eq!(next.active_player, PlayerId(1));
        assert_eq!(next.step, Step::Untap);
    }

    #[test]
    fn two_turns_cycle_back_to_the_first_player() {
        // Player 0 (turn 1) -> player 1 (turn 2) -> player 0 (turn 3).
        let mut state = GameState::new_two_player();
        state.step = Step::Cleanup;
        let state = state.advance();
        assert_eq!(state.active_player, PlayerId(1));

        let mut state = state;
        state.step = Step::Cleanup;
        let state = state.advance();
        assert_eq!(state.turn, 3);
        assert_eq!(state.active_player, PlayerId(0));
    }

    #[test]
    fn extra_turn_is_taken_before_normal_rotation() {
        // Active player 0 has an extra turn queued; ending the turn hands the
        // turn back to player 0 rather than rotating to player 1.
        let mut state = GameState::new_two_player().with_extra_turn(PlayerId(0));
        state.step = Step::Cleanup;

        let next = state.advance();
        assert_eq!(next.turn, 2);
        assert_eq!(next.active_player, PlayerId(0));
        assert_eq!(next.step, Step::Untap);
        assert!(next.extra_turns.is_empty());
    }

    #[test]
    fn extra_turns_are_taken_last_in_first_out() {
        // Grant player 1's extra turn, then player 0's: player 0 goes first.
        let mut state = GameState::new_two_player()
            .with_extra_turn(PlayerId(1))
            .with_extra_turn(PlayerId(0));

        state.step = Step::Cleanup;
        let state = state.advance();
        assert_eq!(state.active_player, PlayerId(0));

        let mut state = state;
        state.step = Step::Cleanup;
        let state = state.advance();
        assert_eq!(state.active_player, PlayerId(1));

        // With the queue drained, rotation resumes normally.
        let mut state = state;
        state.step = Step::Cleanup;
        let state = state.advance();
        assert_eq!(state.active_player, PlayerId(0));
    }

    #[test]
    fn extra_step_is_visited_before_the_natural_sequence() {
        // An additional precombat main phase inserted after the postcombat main.
        let mut state = GameState::new_two_player();
        state.step = Step::PostcombatMain;
        let state = state.with_extra_step(Step::PrecombatMain);

        let next = state.advance();
        assert_eq!(next.step, Step::PrecombatMain);
        assert_eq!(next.turn, 1);
        assert_eq!(next.active_player, PlayerId(0));
        assert!(next.extra_steps.is_empty());

        // Once the extra step is consumed, the sequence resumes from it.
        assert_eq!(next.advance().step, Step::BeginCombat);
    }

    #[test]
    fn advance_does_not_mutate_input() {
        let before = GameState::new_two_player();
        let _ = before.advance();
        assert_eq!(before.step, Step::Untap);
        assert_eq!(before.turn, 1);
    }

    #[test]
    fn advance_on_seatless_state_does_not_panic() {
        // Default state has no players; ending its turn must not divide by zero.
        let state = GameState {
            step: Step::Cleanup,
            ..GameState::default()
        };
        let next = state.advance();
        assert_eq!(next.turn, 0);
        assert_eq!(next.step, Step::Cleanup);
    }

    #[test]
    fn player_zone_accessor_matches_fields() {
        let mut player = Player::new();
        player.hand.push(CardId(7));
        player.graveyard.push(CardId(9));
        assert_eq!(player.zone(Zone::Hand), &vec![CardId(7)]);
        assert_eq!(player.zone(Zone::Graveyard), &vec![CardId(9)]);
        assert!(player.zone(Zone::Library).is_empty());
        assert!(player.zone(Zone::Exile).is_empty());
    }
}
