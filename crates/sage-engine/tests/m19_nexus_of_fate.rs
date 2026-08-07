//! Nexus of Fate (issue #706): the card that will not stay in a graveyard.
//!
//! `If Nexus of Fate would be put into a graveyard from anywhere, reveal it and shuffle
//! it into its owner's library instead` is the first ability in the vocabulary that
//! functions in **every** zone. Each road to a graveyard is its own seam — a spell that
//! resolved, a spell that was countered, a discard, a mill, a permanent that died — and
//! "from anywhere" would be five facts if it were applied at each of them.
//!
//! It is applied at the one place all five end instead, which is what these tests are
//! about: the same card, arriving at the same graveyard by four different roads, and
//! never getting there.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, Action, CardDatabase, CardId, CardInstance, Color, FunctionalId, GameState,
    PlayerId, Step, Target,
};

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

fn main_phase(db: &CardDatabase) -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    state.players[0].turn_began = state.turn;
    for player in &mut state.players {
        for color in [
            Color::White,
            Color::Blue,
            Color::Black,
            Color::Red,
            Color::Green,
        ] {
            player.mana_pool.add(color, 10);
        }
        player.mana_pool.add_colorless(10);
    }
    let filler = cid(db, "bogstomper");
    for seat in 0..2 {
        let library: Vec<_> = (0..12).map(|_| state.new_instance(filler)).collect();
        state.players[seat].library = library;
    }
    state
}

/// Put a Nexus in seat 0's hand and hand back its instance.
fn nexus_in_hand(state: &mut GameState, db: &CardDatabase) -> CardInstance {
    let card = state.new_instance(cid(db, "nexus_of_fate"));
    state.players[0].hand.push(card);
    card
}

/// Whether `card` is in seat `seat`'s graveyard.
fn in_graveyard(state: &GameState, seat: PlayerId, card: CardInstance) -> bool {
    state.players[seat.0]
        .graveyard
        .iter()
        .any(|held| held.id == card.id)
}

/// Whether `card` is in seat `seat`'s library.
fn in_library(state: &GameState, seat: PlayerId, card: CardInstance) -> bool {
    state.players[seat.0]
        .library
        .iter()
        .any(|held| held.id == card.id)
}

/// **The crux.** The spell resolves, takes its extra turn, and goes back into the library
/// rather than to the graveyard every other instant reaches (CR 608.3).
#[test]
fn issue_706_a_resolved_nexus_shuffles_itself_back_in() {
    let db = db();
    let mut state = main_phase(&db);
    let nexus = nexus_in_hand(&mut state, &db);

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: nexus,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    assert!(
        !in_graveyard(&state, PlayerId(0), nexus),
        "it never reached the graveyard"
    );
    assert!(
        in_library(&state, PlayerId(0), nexus),
        "and it is back in the library"
    );
    assert_eq!(state.extra_turns.len(), 1, "the extra turn still happened");
}

/// Countered is the same answer by a different road (CR 701.5a).
#[test]
fn issue_706_a_countered_nexus_shuffles_itself_back_in() {
    let db = db();
    let mut state = main_phase(&db);
    let nexus = nexus_in_hand(&mut state, &db);
    let cancel = state.new_instance(cid(&db, "cancel"));
    state.players[1].hand.push(cancel);

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: nexus,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    let spell = state.stack.last().expect("the Nexus is on the stack").id;
    let mut state = state;
    state.priority = PlayerId(1);
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: cancel,
            mode: None,
            x: None,
            targets: vec![Target::Spell(spell)],
            payment: Vec::new(),
        },
        &db,
    );
    let mut state = state;
    for _ in 0..6 {
        if state.stack.is_empty() {
            break;
        }
        state = apply_action(&state, &Action::PassPriority, &db);
    }

    assert!(
        !in_graveyard(&state, PlayerId(0), nexus),
        "a countered Nexus is not in a graveyard either"
    );
    assert!(in_library(&state, PlayerId(0), nexus));
    assert!(
        state.extra_turns.is_empty(),
        "and being countered, it took no turn"
    );
}

/// Discarded, from the hand it was sitting in (CR 701.8).
#[test]
fn issue_706_a_discarded_nexus_shuffles_itself_back_in() {
    let db = db();
    let mut state = main_phase(&db);
    let nexus = nexus_in_hand(&mut state, &db);
    // A hand of the Nexus and the sorcery that will make its owner discard it — seat 0's
    // own turn, because a sorcery is cast on one (CR 307.1).
    let mind_rot = state.new_instance(cid(&db, "mind_rot"));
    state.players[0].hand = vec![nexus, mind_rot];

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: mind_rot,
            mode: None,
            x: None,
            targets: vec![Target::Player(PlayerId(0))],
            payment: Vec::new(),
        },
        &db,
    );
    let mut state = apply_action(&state, &Action::PassPriority, &db);
    for _ in 0..8 {
        if sage_engine::pending_player_choice(&state).is_some() {
            break;
        }
        state = apply_action(&state, &Action::PassPriority, &db);
    }
    // Answer the discard with the Nexus itself.
    let state = apply_action(
        &state,
        &Action::AnswerChoice {
            chosen: vec![nexus.id],
        },
        &db,
    );

    assert!(
        !state.players[0].hand.iter().any(|c| c.id == nexus.id),
        "it really left the hand"
    );
    assert!(
        !in_graveyard(&state, PlayerId(0), nexus),
        "a discarded Nexus is not in a graveyard"
    );
    assert!(in_library(&state, PlayerId(0), nexus));
}

/// And milled, off the top of the library it will be going straight back into
/// (CR 701.13).
#[test]
fn issue_706_a_milled_nexus_shuffles_itself_back_in() {
    let db = db();
    let mut state = main_phase(&db);
    let nexus = state.new_instance(cid(&db, "nexus_of_fate"));
    // The top of the library is the end of the vector, so this is the next card milled.
    state.players[0].library.push(nexus);
    let before = state.players[0].library.len();
    let millstone = {
        let card = cid(&db, "millstone");
        let instance = state.new_instance(card).id;
        let id = sage_engine::PermanentId(state.mint_id());
        state.battlefield.push(sage_engine::Permanent {
            id,
            instance,
            printed: card.into(),
            controller: PlayerId(0),
            ..Default::default()
        });
        id
    };

    let state = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: millstone,
            index: 0,
            targets: vec![Target::Player(PlayerId(0))],
            payment: Vec::new(),
        },
        &db,
    );
    let mut state = state;
    for _ in 0..6 {
        if state.stack.is_empty() {
            break;
        }
        state = apply_action(&state, &Action::PassPriority, &db);
    }

    assert!(
        !in_graveyard(&state, PlayerId(0), nexus),
        "the mill moved it, but not into the graveyard"
    );
    assert!(in_library(&state, PlayerId(0), nexus));
    assert_eq!(
        state.players[0].library.len(),
        before - 1,
        "the other card it milled did reach the graveyard"
    );
}
