//! `if this creature attacked or blocked this turn`, and the shuffle it pays for
//! (issue #727, CR 608.2 / CR 701.19).
//!
//! Inferno Hellion asks the question at **each** end step and, when the answer is yes,
//! puts itself back in its owner's deck. Both halves are properties of the walk rather
//! than of any one state, so every test here drives the real [`apply_action`] pipeline
//! through a whole combat: the declaration has to actually happen, the end-of-combat
//! turn-based action has to actually clear it (CR 511.3 — which is what makes the
//! recorded declaration the only surviving witness), and the trigger has to actually
//! reach the stack and resolve.
//!
//! The shuffle is the other half. It draws from the seeded stream (ADR 0006), so the same
//! seed puts the card in the same place twice and a different seed does not — which is
//! what separates a shuffle from an append.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, Action, Attack, AttackTarget, Block, CardDatabase, CardId, CardInstanceId,
    FunctionalId, GameState, Permanent, PermanentId, PlayerId, Step,
};

/// Enough actions to walk several whole turns; a walk that has not reached its goal by
/// then is a hang, and failing beats spinning.
const ACTION_CAP: usize = 400;

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// A two-player game at the start of turn 1, both libraries stocked with a card nobody
/// can afford — so the walk never trips the CR 704.5c decking loss, and the library the
/// shuffle acts on is big enough for a reordering to be visible.
fn fresh_game(db: &CardDatabase, seed: u64) -> GameState {
    let mut state = GameState::new_two_player_with_seed(seed);
    for seat in 0..2 {
        let library: Vec<_> = (0..12)
            .map(|_| state.new_instance(cid(db, "colossal_dreadmaw")))
            .collect();
        state.players[seat].library = library;
    }
    state
}

/// Put a permanent of `slug` on the battlefield under `controller`, entered on turn 0 so
/// it is free of summoning sickness from turn 1 (CR 302.6).
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

/// The one action the walk takes on its own: submit an empty combat declaration when one
/// is owed, and otherwise pass. Each test makes its *own* declaration first, which sets
/// the declared flag and leaves this to pass through the rest of combat.
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

/// Walk the game forward under [`walk_action`] until `done` holds.
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

/// Whether the permanent `id` is still on the battlefield.
fn on_battlefield(state: &GameState, id: PermanentId) -> bool {
    state.battlefield.iter().any(|perm| perm.id == id)
}

/// The instance ids in `seat`'s library, top last — the order a shuffle randomizes.
fn library_order(state: &GameState, seat: PlayerId) -> Vec<CardInstanceId> {
    state.players[seat.0]
        .library
        .iter()
        .map(|card| card.id)
        .collect()
}

/// How many copies of `slug` are in `seat`'s library.
fn copies_in_library(state: &GameState, db: &CardDatabase, seat: PlayerId, slug: &str) -> usize {
    let card = cid(db, slug);
    state.players[seat.0]
        .library
        .iter()
        .filter(|instance| instance.card == card)
        .count()
}

/// Seat 0 attacks seat 1 with `attacker` on turn 1, and the game runs on to the point in
/// turn 2 where every one of turn 1's end-step triggers has resolved.
fn attack_and_run_out_the_turn(
    state: &GameState,
    db: &CardDatabase,
    attacker: PermanentId,
) -> GameState {
    let state = walk_until(state, db, |s| {
        s.turn == 1 && s.step == Step::DeclareAttackers
    });
    let state = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: vec![Attack {
                attacker,
                defender: AttackTarget::Player(PlayerId(1)),
            }],
        },
        db,
    );
    walk_until(&state, db, |s| s.turn == 2 && s.step == Step::PrecombatMain)
}

/// The plain yes: a creature that was declared as an attacker this turn answers the
/// condition, and the effect behind it puts the card back in the deck.
#[test]
fn issue_727_a_hellion_that_attacked_is_shuffled_into_its_owners_library() {
    let db = db();
    let mut state = fresh_game(&db, 7);
    let hellion = place(&mut state, &db, "inferno_hellion", PlayerId(0));

    let state = attack_and_run_out_the_turn(&state, &db, hellion);

    assert!(
        !on_battlefield(&state, hellion),
        "the end step's condition saw the attack"
    );
    assert_eq!(
        copies_in_library(&state, &db, PlayerId(0), "inferno_hellion"),
        1,
        "and the card went to its owner's library, not a graveyard"
    );
    assert_eq!(
        state.players[0].graveyard.len(),
        0,
        "a shuffle is not a death"
    );
}

/// The other half of the same question, and the half a snapshot could never answer: a
/// creature that blocked is no longer blocking anything by the end step (CR 511.3), so
/// only the recorded declaration still says it did.
#[test]
fn issue_727_a_hellion_that_blocked_is_shuffled_into_its_owners_library() {
    let db = db();
    let mut state = fresh_game(&db, 11);
    // A 1/1 attacker, so the 7/3 blocker survives the block and lives to be asked about.
    let scoundrel = place(&mut state, &db, "grasping_scoundrel", PlayerId(0));
    let hellion = place(&mut state, &db, "inferno_hellion", PlayerId(1));

    let state = walk_until(&state, &db, |s| {
        s.turn == 1 && s.step == Step::DeclareAttackers
    });
    let state = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: vec![Attack {
                attacker: scoundrel,
                defender: AttackTarget::Player(PlayerId(1)),
            }],
        },
        &db,
    );
    let state = walk_until(&state, &db, |s| s.step == Step::DeclareBlockers);
    let state = apply_action(
        &state,
        &Action::DeclareBlockers {
            blocks: vec![Block {
                blocker: hellion,
                attacker: scoundrel,
            }],
        },
        &db,
    );
    let state = walk_until(&state, &db, |s| {
        s.turn == 2 && s.step == Step::PrecombatMain
    });

    assert!(
        !on_battlefield(&state, hellion),
        "blocking answers the condition exactly as attacking does"
    );
    assert_eq!(
        copies_in_library(&state, &db, PlayerId(1), "inferno_hellion"),
        1,
        "and it goes to the blocker's own library"
    );
}

/// The no. The trigger fires at every end step whatever happened, so a creature that
/// stayed home is the case that proves the *condition* is doing the work rather than the
/// trigger — two of its own end steps come and go and it is still there.
#[test]
fn issue_727_a_hellion_that_did_neither_stays_on_the_battlefield() {
    let db = db();
    let mut state = fresh_game(&db, 3);
    let hellion = place(&mut state, &db, "inferno_hellion", PlayerId(0));

    let state = walk_until(&state, &db, |s| {
        s.turn == 4 && s.step == Step::PrecombatMain
    });

    assert!(
        on_battlefield(&state, hellion),
        "three turns of end steps asked, and every answer was no"
    );
    assert_eq!(
        copies_in_library(&state, &db, PlayerId(0), "inferno_hellion"),
        0
    );
}

/// A shuffle, not an append (CR 701.19): the card really is somewhere in the deck, and
/// the deck is not in the order it was in.
#[test]
fn issue_727_the_library_is_shuffled_rather_than_appended_to() {
    let db = db();
    let mut state = fresh_game(&db, 7);
    let hellion = place(&mut state, &db, "inferno_hellion", PlayerId(0));
    let before = library_order(&state, PlayerId(0));

    let state = attack_and_run_out_the_turn(&state, &db, hellion);
    let after = library_order(&state, PlayerId(0));

    assert_eq!(
        after.len(),
        before.len() + 1,
        "one card joined the library and none left it"
    );
    let mut appended = before.clone();
    appended.push(
        *after
            .iter()
            .find(|id| !before.contains(id))
            .expect("the hellion's instance is in the library"),
    );
    assert_ne!(
        after, appended,
        "putting it on top would have left the rest of the deck in order"
    );
    let mut sorted_after: Vec<u64> = after.iter().map(|id| id.0).collect();
    let mut sorted_appended: Vec<u64> = appended.iter().map(|id| id.0).collect();
    sorted_after.sort_unstable();
    sorted_appended.sort_unstable();
    assert_eq!(
        sorted_after, sorted_appended,
        "a shuffle is a permutation: the same cards, reordered"
    );
}

/// ADR 0006: the shuffle draws from the injected seed and nothing else, so the same game
/// replays to the same deck order — and a different seed does not.
#[test]
fn issue_727_the_shuffle_is_deterministic_for_a_seed() {
    let db = db();
    let run = |seed: u64| {
        let mut state = fresh_game(&db, seed);
        let hellion = place(&mut state, &db, "inferno_hellion", PlayerId(0));
        let state = attack_and_run_out_the_turn(&state, &db, hellion);
        library_order(&state, PlayerId(0))
    };

    assert_eq!(run(7), run(7), "the same seed shuffles the same way");
    assert_ne!(
        run(7),
        run(8),
        "and a different seed does not — the order is the stream's, not the code's"
    );
}
