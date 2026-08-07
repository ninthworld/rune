//! Counters on a **player** (CR 122.1a) — the first thing in the engine that can hold a
//! counter and is not a permanent.
//!
//! A player's counters and a permanent's are deliberately one mechanism rather than two:
//! the same `BTreeMap<CounterKind, u32>`, the same "absent means zero" reading, and the
//! same single seam that puts them on. What differs is only what is being asked about,
//! which is what lets a prohibition written once cover both.
//!
//! One kind exists, and it is the one the rules define without any card having to:
//! **poison** (CR 704.5d — ten of them and that player loses). Nothing in the bundled
//! catalog gives one out, so every game the engine can currently play has an empty map on
//! every seat and a wire payload byte-for-byte unchanged. The state-based action is here
//! anyway, because a counter kind whose whole rule is unimplemented is a name pretending
//! to be a mechanic.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, Action, CardDatabase, CounterKind, GameState, LossReason, PlayerId, Step,
};

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

/// A settled main phase both seats can pass in.
fn main_phase() -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    state.players[0].turn_began = state.turn;
    state
}

/// Run the state-based-action loop by taking one legal, inconsequential action.
fn settle(state: &GameState, db: &CardDatabase) -> GameState {
    apply_action(state, &Action::PassPriority, db)
}

/// A player starts with no counters at all, and the map says so by being empty rather
/// than by holding zeroes.
#[test]
fn a_fresh_player_holds_no_counters() {
    let state = GameState::new_two_player();
    for player in &state.players {
        assert!(player.counters.is_empty(), "no counters, not zero of them");
    }
}

/// **The crux.** Ten poison counters lose the game (CR 704.5d), and the loss is recorded
/// with its own reason rather than borrowed from the life total.
#[test]
fn cr_704_5d_ten_poison_counters_lose_the_game() {
    let db = db();
    let mut state = main_phase();
    state.players[1].counters.insert(CounterKind::Poison, 10);

    let state = settle(&state, &db);

    assert!(state.players[1].has_lost, "ten is lethal");
    assert_eq!(state.players[1].loss_reason, Some(LossReason::Poison));
    assert!(!state.players[0].has_lost, "and it is not catching");
    assert_eq!(
        state.players[1].life,
        sage_engine::STARTING_LIFE,
        "the player is at full life — poison is its own losing condition"
    );
}

/// Nine is not ten. The threshold is a floor, not a parity, and the counters stay where
/// they are afterwards.
#[test]
fn cr_704_5d_nine_poison_counters_are_survivable() {
    let db = db();
    let mut state = main_phase();
    state.players[1].counters.insert(CounterKind::Poison, 9);

    let state = settle(&state, &db);

    assert!(!state.players[1].has_lost, "nine is not ten");
    assert_eq!(
        state.players[1].counters.get(&CounterKind::Poison),
        Some(&9),
        "and the counters are still there"
    );
}

/// The check reads rather than consumes: the counters remain after the loss is recorded,
/// and the state-based-action loop still reaches a fixed point instead of spinning on a
/// condition that stays true.
#[test]
fn cr_704_5d_the_check_reads_the_counters_and_still_settles() {
    let db = db();
    let mut state = main_phase();
    state.players[1].counters.insert(CounterKind::Poison, 12);

    let state = settle(&state, &db);

    assert_eq!(
        state.players[1].counters.get(&CounterKind::Poison),
        Some(&12),
        "nothing was consumed"
    );
    assert!(state.players[1].has_lost);
    // A second pass over an already-lost player changes nothing, which is what "fixed
    // point" means for a condition that is still true.
    let again = settle(&state, &db);
    assert_eq!(
        again.players[1].loss_reason,
        Some(LossReason::Poison),
        "and the reason is not overwritten by a later pass"
    );
}

/// Counters of different kinds do not pool: a player is not nine-tenths dead from a
/// counter that has nothing to do with poison.
#[test]
fn counter_kinds_on_a_player_are_counted_separately() {
    let db = db();
    let mut state = main_phase();
    state.players[1].counters.insert(CounterKind::Poison, 5);
    state.players[1].counters.insert(CounterKind::Charge, 9);

    let state = settle(&state, &db);

    assert!(
        !state.players[1].has_lost,
        "five poison is five poison, whatever else is on the player"
    );
}
