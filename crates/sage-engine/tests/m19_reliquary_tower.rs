//! Reliquary Tower, and the first continuous ability in the engine whose subject is a
//! player rather than a permanent.
//!
//! The card is one sentence about *you* and one mana ability, and the sentence is only
//! observable at one moment in the turn: the cleanup step, where a player holding more
//! than seven cards is asked to discard. So every test here walks the real turn structure
//! to that step rather than asserting on a predicate — a maximum hand size nothing ever
//! consults would be a number in a struct, not a rule.
//!
//! Cards are named by their authored `functional_id`, never by an interned handle
//! (ADR 0008 §3).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, maximum_hand_size, over_hand_size, valid_actions, Action, CardDatabase, CardId,
    FunctionalId, GameState, Permanent, PermanentId, PlayerId, Step, MAX_HAND_SIZE,
};

// ----- fixtures -------------------------------------------------------------

/// Enough actions to walk a whole turn; a settle that has not arrived by then is a hang,
/// and failing beats spinning.
const SETTLE_LIMIT: usize = 400;

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// A two-player game at player 0's precombat main, with both libraries stocked so a walk
/// to cleanup never trips the CR 704.5c decking loss.
fn main_phase(db: &CardDatabase) -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    let forest = cid(db, "forest");
    for seat in 0..2 {
        state.players[seat].library = (0..20).map(|_| state.new_instance(forest)).collect();
    }
    state
}

/// Put a permanent of `slug` onto the battlefield under `controller`, and return its
/// battlefield identity.
fn place(
    state: &mut GameState,
    db: &CardDatabase,
    slug: &str,
    controller: PlayerId,
) -> PermanentId {
    let card = cid(db, slug);
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

/// Give `seat` exactly `count` cards in hand.
fn fill_hand(state: &mut GameState, db: &CardDatabase, seat: PlayerId, count: usize) {
    let forest = cid(db, "forest");
    state.players[seat.0].hand.clear();
    for _ in 0..count {
        let instance = state.new_instance(forest);
        state.players[seat.0].hand.push(instance);
    }
}

/// Walk the game forward one legal action at a time until `done` holds, passing where
/// passing is offered and otherwise taking the first non-concede action there is.
///
/// Deliberately **not** discard-aware: if the walk reaches a cleanup step that owes a
/// discard, the first non-concede action is one, and the hand shrinks. That is what makes
/// "the hand still has nine cards" a real assertion rather than an artifact of the walk
/// choosing not to discard.
fn settle_until(
    state: &GameState,
    db: &CardDatabase,
    done: impl Fn(&GameState) -> bool,
) -> GameState {
    let mut state = state.clone();
    for _ in 0..SETTLE_LIMIT {
        if done(&state) {
            return state;
        }
        let offered = valid_actions(&state, db);
        let action = if offered.contains(&Action::PassPriority) {
            Action::PassPriority
        } else {
            offered
                .into_iter()
                .find(|action| action != &Action::Concede)
                .expect("some action is always available")
        };
        state = apply_action(&state, &action, db);
    }
    panic!("the game never reached the state under test");
}

/// The number of cards in `seat`'s hand.
fn hand(state: &GameState, seat: PlayerId) -> usize {
    state.players[seat.0].hand.len()
}

// ----- the card -------------------------------------------------------------

#[test]
fn the_default_maximum_is_seven_and_a_full_hand_is_discarded_down_to_it() {
    // The baseline the card is an exception to. Without it, nine cards at cleanup means
    // two discards — asserted here so the test below is a comparison rather than a claim.
    let db = db();
    let mut state = main_phase(&db);
    fill_hand(&mut state, &db, PlayerId(0), 9);

    assert_eq!(
        maximum_hand_size(&state, PlayerId(0), &db),
        Some(MAX_HAND_SIZE)
    );
    assert!(over_hand_size(&state, PlayerId(0), &db));

    let next_turn = settle_until(&state, &db, |s| s.turn == 2);
    assert_eq!(
        hand(&next_turn, PlayerId(0)),
        MAX_HAND_SIZE,
        "the cleanup step took the two cards over the limit"
    );
}

#[test]
fn reliquary_tower_removes_the_maximum_and_the_cleanup_discard_with_it() {
    // The whole card. Nine cards go into the cleanup step and nine come out, because the
    // step never pauses to ask: no maximum, nothing to discard down to.
    let db = db();
    let mut state = main_phase(&db);
    place(&mut state, &db, "reliquary_tower", PlayerId(0));
    fill_hand(&mut state, &db, PlayerId(0), 9);

    assert_eq!(maximum_hand_size(&state, PlayerId(0), &db), None);
    assert!(!over_hand_size(&state, PlayerId(0), &db));

    let next_turn = settle_until(&state, &db, |s| s.turn == 2);
    assert_eq!(
        hand(&next_turn, PlayerId(0)),
        9,
        "no maximum, so the cleanup step asked for nothing"
    );
}

#[test]
fn the_discard_is_never_offered_while_the_tower_is_out() {
    // The step gate and the action generator have to agree: a cleanup that does not pause
    // must also be a cleanup with nothing to offer. Asserted at the step itself, because
    // a disagreement between the two would either stall the turn or discard silently.
    let db = db();
    let mut state = main_phase(&db);
    place(&mut state, &db, "reliquary_tower", PlayerId(0));
    fill_hand(&mut state, &db, PlayerId(0), 9);

    let mut at_cleanup = state.clone();
    at_cleanup.step = Step::Cleanup;
    at_cleanup.priority = PlayerId(0);
    let offered = valid_actions(&at_cleanup, &db);
    assert!(
        !offered
            .iter()
            .any(|action| matches!(action, Action::Discard { .. })),
        "no discard is on offer to a player with no maximum hand size"
    );
}

#[test]
fn the_maximum_returns_the_instant_the_tower_leaves() {
    // Derived on every read, never stored (ADR 0005 §1): the ability starts and stops with
    // the permanent, so a Tower destroyed in the same turn takes its effect with it and
    // the cleanup step asks after all. Nothing has to be pruned for that to be true.
    let db = db();
    let mut state = main_phase(&db);
    let tower = place(&mut state, &db, "reliquary_tower", PlayerId(0));
    fill_hand(&mut state, &db, PlayerId(0), 9);
    assert_eq!(maximum_hand_size(&state, PlayerId(0), &db), None);

    state.battlefield.retain(|perm| perm.id != tower);
    assert_eq!(
        maximum_hand_size(&state, PlayerId(0), &db),
        Some(MAX_HAND_SIZE)
    );

    let next_turn = settle_until(&state, &db, |s| s.turn == 2);
    assert_eq!(
        hand(&next_turn, PlayerId(0)),
        MAX_HAND_SIZE,
        "with the Tower gone the ordinary rule applies again"
    );
}

#[test]
fn the_tower_says_nothing_about_the_other_seat() {
    // "You" is the source's controller, and the ability has no other subject. An opponent
    // holding nine cards discards on their own turn exactly as they always did.
    let db = db();
    let mut state = main_phase(&db);
    place(&mut state, &db, "reliquary_tower", PlayerId(0));
    fill_hand(&mut state, &db, PlayerId(1), 9);

    assert_eq!(maximum_hand_size(&state, PlayerId(0), &db), None);
    assert_eq!(
        maximum_hand_size(&state, PlayerId(1), &db),
        Some(MAX_HAND_SIZE),
        "the Tower is not on their battlefield"
    );

    // Turn 2 is seat 1's, and its cleanup is the one that asks them.
    let turn_three = settle_until(&state, &db, |s| s.turn == 3);
    assert_eq!(hand(&turn_three, PlayerId(1)), MAX_HAND_SIZE);
}

#[test]
fn the_tower_taps_for_colourless_like_any_other_land() {
    // The second half of the card, and the half that makes it playable. A land whose only
    // tested behaviour was a rule about hand size would be a rule, not a card.
    let db = db();
    let mut state = main_phase(&db);
    let tower = place(&mut state, &db, "reliquary_tower", PlayerId(0));

    let after = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: tower,
            index: 1,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    assert_eq!(after.players[0].mana_pool.colorless, 1);
    assert!(
        after.battlefield.iter().any(|p| p.id == tower && p.tapped),
        "the {{T}} in the cost was paid"
    );
}
