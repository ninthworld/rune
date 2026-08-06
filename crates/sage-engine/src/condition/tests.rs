//! The window `attacked or blocked this turn` reads, at its own boundary (issue #727).
//!
//! Inferno Hellion cannot show this on its own: the card that asks the question also
//! removes itself the moment the answer is yes, so the same permanent is never asked
//! twice. The property is nonetheless the whole of "this **turn**", so it is asserted
//! here, against declarations and turn boundaries the real
//! [`apply_action`](crate::apply_action) produced rather than events written by hand.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::ability::Condition;
use crate::actions::{Action, Attack};
use crate::apply::apply_action;
use crate::combat::AttackTarget;
use crate::fixtures::{bundled, fixture};
use crate::id::PermanentId;
use crate::phase::Step;
use crate::state::Permanent;

/// Enough actions to cross a turn boundary and reach the next turn's combat.
const ACTION_CAP: usize = 200;

/// A two-player game at the start of turn 1 with one creature per seat, each entered on
/// turn 0 so neither is summoning-sick (CR 302.6), and libraries deep enough that the
/// walk never decks anybody (CR 704.5c).
fn game_with_a_creature_each() -> (GameState, PermanentId, PermanentId) {
    let mut state = GameState::new_two_player();
    for seat in 0..2 {
        let library: Vec<_> = (0..12)
            .map(|_| state.new_instance(fixture("colossal_dreadmaw")))
            .collect();
        state.players[seat].library = library;
    }
    let mine = place(&mut state, PlayerId(0));
    let theirs = place(&mut state, PlayerId(1));
    (state, mine, theirs)
}

fn place(state: &mut GameState, controller: PlayerId) -> PermanentId {
    let card = fixture("onakke_ogre");
    let instance = state.new_instance(card).id;
    let id = PermanentId(state.mint_id());
    state.battlefield.push(Permanent {
        id,
        instance,
        printed: card.into(),
        controller,
        ..Default::default()
    });
    id
}

/// Submit an empty combat declaration when one is owed, and otherwise pass.
fn walk_action(state: &GameState) -> Action {
    match state.step {
        Step::DeclareAttackers if !state.attackers_declared => Action::DeclareAttackers {
            attackers: Vec::new(),
        },
        Step::DeclareBlockers if !state.blockers_declared => {
            Action::DeclareBlockers { blocks: Vec::new() }
        }
        _ => Action::PassPriority,
    }
}

fn walk_until(
    state: &GameState,
    db: &CardDatabase,
    done: impl Fn(&GameState) -> bool,
) -> GameState {
    let mut state = state.clone();
    for _ in 0..ACTION_CAP {
        if done(&state) {
            return state;
        }
        let next = apply_action(&state, &walk_action(&state), db);
        assert_ne!(next, state, "the walk made no progress");
        state = next;
    }
    panic!("the walk ran past its cap without reaching the goal");
}

/// Ask the condition about `source`, in the frame an ability of that permanent resolves
/// in. The resolution window is the log's head, which this condition never reads — its
/// window is the turn.
fn asked_about(state: &GameState, db: &CardDatabase, source: PermanentId) -> bool {
    condition_holds(
        state,
        &Condition::AttackedOrBlockedThisTurn,
        PlayerId(0),
        Some(source),
        state.next_log_sequence,
        db,
    )
}

/// The condition is about **one** permanent, and it turns on at the declaration and off
/// at the turn boundary — with no event of its own in either direction, because combat
/// has already forgotten (CR 511.3) by the time anything asks.
#[test]
fn issue_727_the_attacked_window_closes_at_the_turn_boundary() {
    let db = bundled();
    let (state, mine, theirs) = game_with_a_creature_each();

    let state = walk_until(&state, db, |s| {
        s.turn == 1 && s.step == Step::DeclareAttackers
    });
    assert!(
        !asked_about(&state, db, mine),
        "nothing has been declared yet"
    );

    let state = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: vec![Attack {
                attacker: mine,
                defender: AttackTarget::Player(PlayerId(1)),
            }],
        },
        db,
    );
    assert!(asked_about(&state, db, mine), "it was just declared");
    assert!(
        !asked_about(&state, db, theirs),
        "and the creature that stayed home is a different permanent"
    );

    // Past end of combat, where CR 511.3 clears `attacking` — the board no longer says
    // anything about the declaration, and the condition still does.
    let state = walk_until(&state, db, |s| s.step == Step::End);
    assert!(
        state
            .battlefield
            .iter()
            .all(|perm| perm.attacking.is_none() && perm.blocking.is_empty()),
        "combat is over and nothing is attacking"
    );
    assert!(
        asked_about(&state, db, mine),
        "the declaration the turn produced outlives the combat that produced it"
    );

    // The next turn. Same permanent, same board, no declaration in it.
    let state = walk_until(&state, db, |s| s.turn == 2 && s.step == Step::PrecombatMain);
    assert!(
        !asked_about(&state, db, mine),
        "the window is the turn, and this is a different one"
    );
}

/// A source that is not a permanent — a spell, an emblem's ability — has nothing that
/// could have attacked, so the answer is no rather than a panic.
#[test]
fn issue_727_a_condition_about_a_source_that_is_not_a_permanent_is_false() {
    let db = bundled();
    let (state, _mine, _theirs) = game_with_a_creature_each();
    assert!(!condition_holds(
        &state,
        &Condition::AttackedOrBlockedThisTurn,
        PlayerId(0),
        None,
        state.next_log_sequence,
        db,
    ));
}
