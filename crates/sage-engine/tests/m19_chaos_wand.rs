//! Chaos Wand: digging through an **opponent's** library, and casting what it finds
//! (issue #787).
//!
//! `{4}, {T}: Target opponent exiles cards from the top of their library until they exile an
//! instant or sorcery card. You may cast that card without paying its mana cost. Then put
//! the exiled cards that weren't cast this way on the bottom of that library in a random
//! order.`
//!
//! Three of its four clauses cross the table, which is what makes it the card this seam was
//! split out for: the exile comes off the *targeted* player's library, the card offered
//! belongs to them while the ability's controller casts it (CR 108.4), and what is not cast
//! goes back on the bottom of *their* library.
//!
//! Every test drives the real [`apply_action`] pipeline and the real [`valid_actions`] offer.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, pending_player_choice, valid_actions, Action, CardDatabase, CardId, CardInstance,
    Color, FunctionalId, GameState, Permanent, PermanentId, PlayerId, Step, Target,
};

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

fn main_phase() -> GameState {
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
    state
}

/// The Wand under seat 0, and seat 1's library **top last**.
fn wand_against(
    db: &CardDatabase,
    library: &[&str],
) -> (GameState, PermanentId, Vec<CardInstance>) {
    let mut state = main_phase();
    let card = cid(db, "chaos_wand");
    let instance = state.new_instance(card).id;
    let id = PermanentId(state.mint_id());
    state.battlefield.push(Permanent {
        id,
        instance,
        printed: card.into(),
        controller: PlayerId(0),
        ..Default::default()
    });
    let cards: Vec<CardInstance> = library
        .iter()
        .map(|slug| state.new_instance(cid(db, slug)))
        .collect();
    state.players[1].library = cards.clone();
    (state, id, cards)
}

/// Activate the Wand at seat 1 and let it resolve.
fn activate(state: &GameState, db: &CardDatabase, wand: PermanentId) -> GameState {
    let state = apply_action(
        state,
        &Action::ActivateAbility {
            permanent: wand,
            index: 0,
            targets: vec![Target::Player(PlayerId(1))],
            payment: Vec::new(),
        },
        db,
    );
    let state = apply_action(&state, &Action::PassPriority, db);
    apply_action(&state, &Action::PassPriority, db)
}

/// It digs until the first instant or sorcery, exiles what it passed, and offers that card.
#[test]
fn issue_787_it_digs_to_the_first_instant_or_sorcery_and_offers_it() {
    let db = db();
    // Top last: the dig sees Forest, then Onakke Ogre, then Shock — and stops there.
    let (state, wand, cards) = wand_against(&db, &["mountain", "shock", "onakke_ogre", "forest"]);
    let state = activate(&state, &db, wand);

    let pending = pending_player_choice(&state).expect("the offer stands");
    let request = pending.question.play_card().expect("it is a play offer");
    // The Shock is the third from the top, so three cards were exiled and it is the one on
    // offer. `cards` is bottom-first, so the Shock is index 1.
    assert_eq!(
        request.card.id, cards[1].id,
        "the card the digging stopped at"
    );
    assert_eq!(
        request.subject,
        PlayerId(0),
        "offered to the Wand's controller"
    );
    assert_eq!(
        state.players[1].exile.len(),
        3,
        "and it exiled what it passed"
    );
    assert_eq!(state.players[1].library.len(), 1, "leaving the rest");
}

/// The offered card belongs to the opponent, and casting it is the controller's (CR 108.4).
#[test]
fn issue_787_the_controller_casts_a_card_the_opponent_owns() {
    let db = db();
    let (state, wand, cards) = wand_against(&db, &["mountain", "shock"]);
    let state = activate(&state, &db, wand);
    let shock = cards[1];

    let offered = valid_actions(&state, &db);
    assert!(
        offered
            .iter()
            .any(|action| matches!(action, Action::CastSpell { card, .. } if card.id == shock.id)),
        "the opponent's card is offered to this seat: {offered:#?}"
    );

    let cast = Action::CastSpell {
        card: shock,
        mode: None,
        x: None,
        targets: vec![Target::Player(PlayerId(1))],
        payment: Vec::new(),
    };
    let after = apply_action(&state, &cast, &db);
    assert!(
        !after.players[1].exile.iter().any(|c| c.id == shock.id),
        "it left the owner's exile for the stack"
    );
    assert_eq!(
        after.stack.first().map(|object| object.controller),
        Some(PlayerId(0)),
        "and the spell is the caster's, not its owner's"
    );

    // It resolves against what its caster aimed it at.
    let after = apply_action(&after, &Action::PassPriority, &db);
    let after = apply_action(&after, &Action::PassPriority, &db);
    assert_eq!(after.players[1].life, 18, "the Shock hit the opponent");
}

/// What was not cast goes back on the bottom of **that** library.
#[test]
fn issue_787_the_rest_go_to_the_bottom_of_the_owner_s_library() {
    let db = db();
    let (state, wand, cards) = wand_against(&db, &["mountain", "shock", "onakke_ogre", "forest"]);
    let state = activate(&state, &db, wand);
    let shock = cards[1];

    let cast = Action::CastSpell {
        card: shock,
        mode: None,
        x: None,
        targets: vec![Target::Player(PlayerId(1))],
        payment: Vec::new(),
    };
    let after = apply_action(&state, &cast, &db);

    // Three were exiled and one was cast, so two go back — under the card that was never
    // dug to, and into the owner's library rather than the caster's.
    assert!(
        after.players[1].exile.is_empty(),
        "nothing is left in exile"
    );
    assert_eq!(after.players[1].library.len(), 3);
    assert!(after.players[0].library.is_empty(), "not into the caster's");
    assert_eq!(
        after.players[1].library.last().map(|card| card.id),
        Some(cards[0].id),
        "the card the dig never reached is still on top"
    );
}

/// Declining puts **everything** back, the offered card included.
#[test]
fn issue_787_declining_bottoms_the_offered_card_too() {
    let db = db();
    let (state, wand, _) = wand_against(&db, &["mountain", "shock", "onakke_ogre", "forest"]);
    let state = activate(&state, &db, wand);

    let after = apply_action(&state, &Action::AnswerConfirm { accept: false }, &db);
    assert!(
        pending_player_choice(&after).is_none(),
        "the question is settled"
    );
    assert!(
        after.players[1].exile.is_empty(),
        "nothing stays exiled: it was not cast this way, so it goes back"
    );
    assert_eq!(
        after.players[1].library.len(),
        4,
        "all four are in the library"
    );
}

/// A library with no instant or sorcery is dug through, and nothing is offered.
#[test]
fn issue_787_a_library_with_nothing_to_find_is_dug_through_and_offers_nothing() {
    let db = db();
    let (state, wand, _) = wand_against(&db, &["forest", "mountain"]);
    let state = activate(&state, &db, wand);

    assert!(
        pending_player_choice(&state).is_none(),
        "there is nothing to offer"
    );
    assert_eq!(
        state.players[1].exile.len(),
        2,
        "and what it turned over stays exiled — the sentence that puts cards back is the \
         one that follows the offer, and there was no offer"
    );
    assert!(state.players[1].library.is_empty());
    assert!(
        !state.players[1].has_lost,
        "running a library out is not a loss"
    );
}

/// The bottoming is seeded, so the same game replays the same way.
#[test]
fn issue_787_the_bottom_order_is_drawn_from_the_seeded_stream() {
    let db = db();
    let order = |seed: u64| {
        let (mut state, wand, _) = wand_against(
            &db,
            &["mountain", "shock", "onakke_ogre", "forest", "island"],
        );
        state.rng_seed = seed;
        let state = activate(&state, &db, wand);
        let state = apply_action(&state, &Action::AnswerConfirm { accept: false }, &db);
        state.players[1]
            .library
            .iter()
            .map(|card| card.id)
            .collect::<Vec<_>>()
    };
    assert_eq!(order(7), order(7), "one seed, one order");
}
