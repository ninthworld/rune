#![allow(clippy::unwrap_used, unused_imports)]

use crate::id::PlayerId;
use crate::phase::Step;
use crate::player::{LossReason, STARTING_LIFE};

use super::{GameEvent, GameState};

#[test]
fn log_window_is_bounded_but_sequence_numbers_keep_climbing() {
    // The window retains only the most recent 200 entries (dropping the oldest),
    // yet sequence numbers continue monotonically — so a client can tell the
    // window starts partway through the history and never sees a reused number.
    let mut state = GameState::new_two_player();
    for _ in 0..250 {
        state.record_event(GameEvent::Mulligan {
            player: PlayerId(0),
        });
    }
    assert_eq!(state.log.len(), 200, "the window is capped at 200 entries");
    assert_eq!(state.next_log_sequence, 251, "every event took a number");
    assert_eq!(
        state.log.first().unwrap().sequence,
        51,
        "the oldest retained entry is the 51st (entries 1..=50 were dropped)"
    );
    assert_eq!(state.log.last().unwrap().sequence, 250);
    // The retained window is a contiguous run of sequence numbers.
    for pair in state.log.windows(2) {
        assert_eq!(pair[1].sequence, pair[0].sequence + 1);
    }
}

#[test]
fn new_two_player_initial_invariants() {
    let state = GameState::new_two_player();
    assert_eq!(state.turn, 1);
    assert_eq!(state.active_player, PlayerId(0));
    assert_eq!(state.step, Step::Untap);
    assert_eq!(state.players.len(), 2);
    assert!(state.battlefield.is_empty());
    assert!(state.stack.is_empty());
    assert!(!state.land_played);
    // The RNG seed slot defaults to 0 when no seed is injected.
    assert_eq!(state.rng_seed, 0);

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
fn seeded_constructor_records_the_seed_and_changes_nothing_else() {
    // The injected seed is stored verbatim, and the only difference from the
    // default constructor is that one field — the slot is inert for now.
    let seeded = GameState::new_two_player_with_seed(0xDEAD_BEEF);
    assert_eq!(seeded.rng_seed, 0xDEAD_BEEF);

    let mut normalized = seeded.clone();
    normalized.rng_seed = 0;
    assert_eq!(normalized, GameState::new_two_player());
}

#[test]
fn cr_104_2a_result_is_none_while_both_players_remain() {
    // The game is not over while at least two players remain.
    let state = GameState::new_two_player();
    assert!(state.result().is_none());
    assert!(!state.is_over());
}

#[test]
fn cr_104_2a_last_player_standing_wins() {
    // CR 104.2a: when one player remains, the game is over and that player wins.
    let mut state = GameState::new_two_player();
    state.players[1].has_lost = true;
    state.players[1].loss_reason = Some(LossReason::Concede);

    let result = state.result().unwrap();
    assert_eq!(result.winner, Some(PlayerId(0)));
    assert_eq!(result.losers, vec![PlayerId(1)]);
    assert_eq!(result.reason, LossReason::Concede);
    assert!(state.is_over());
}

#[test]
fn cr_104_4a_simultaneous_loss_is_a_draw() {
    // CR 104.4a: if every remaining player loses at once, no one wins (a draw).
    let mut state = GameState::new_two_player();
    for player in &mut state.players {
        player.has_lost = true;
        player.loss_reason = Some(LossReason::ZeroLife);
    }

    let result = state.result().unwrap();
    assert_eq!(result.winner, None, "a simultaneous loss has no winner");
    assert_eq!(result.losers, vec![PlayerId(0), PlayerId(1)]);
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

// ----- Elimination rotation (issue #342) -----

#[test]
fn issue_342_next_living_seat_skips_eliminated_seats() {
    let mut state = GameState::new_multiplayer(4);
    state.players[1].has_lost = true;
    // From seat 0 the next living seat is 2 (1 is out); from 2 it is 3; from 3
    // it wraps past the dead 1 to 0.
    assert_eq!(state.next_living_seat(PlayerId(0)), Some(PlayerId(2)));
    assert_eq!(state.next_living_seat(PlayerId(2)), Some(PlayerId(3)));
    assert_eq!(state.next_living_seat(PlayerId(3)), Some(PlayerId(0)));
    assert_eq!(state.living_player_count(), 3);
}

#[test]
fn issue_342_turn_rotation_skips_an_eliminated_seat_across_full_turns() {
    // In a 3-seat game with seat 1 eliminated, turns walk 0 → 2 → 0 → 2, never
    // handing the eliminated seat a turn.
    let mut state = GameState::new_multiplayer(3);
    state.players[1].has_lost = true;
    state.active_player = PlayerId(0);

    state.begin_next_turn();
    assert_eq!(state.active_player, PlayerId(2), "seat 1 is skipped");
    state.begin_next_turn();
    assert_eq!(state.active_player, PlayerId(0), "wraps past seat 1");
    state.begin_next_turn();
    assert_eq!(state.active_player, PlayerId(2));
}

#[test]
fn issue_342_extra_turn_owed_to_an_eliminated_player_is_discarded() {
    // CR 800.4a: an extra turn queued for a player who has since been eliminated
    // is discarded; the turn goes to the next living seat instead.
    let mut state = GameState::new_multiplayer(3);
    state.active_player = PlayerId(0);
    state.players[1].has_lost = true;
    state.extra_turns.push(PlayerId(1)); // owed to the now-eliminated seat 1

    state.begin_next_turn();
    assert_eq!(
        state.active_player,
        PlayerId(2),
        "the discarded extra turn does not resurrect seat 1; seat 2 acts"
    );
    assert!(state.extra_turns.is_empty());
}
