//! Playing a card **as part of a resolution** (CR 608.2f, issue #787).
//!
//! Djinn of Wishes: `{2}{U}{U}, Remove a wish counter: Reveal the top card of your library.
//! You may play that card without paying its mana cost. If you don't, exile it.`
//!
//! This is the one question in the engine whose answer is an **action taken on the game**
//! rather than an answer to a question. Everything under test is that seam: what is offered
//! while it stands, that the offer is a real cast with real targets, that the price reaches
//! that card and no other, and that both branches of *if you don't* are distinguishable.
//!
//! Every test drives the real [`apply_action`] pipeline and the real [`valid_actions`] offer.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, pending_player_choice, total_cast_cost, valid_actions, Action, CardDatabase,
    CardId, CardInstance, Color, CounterKind, FunctionalId, GameState, Permanent, PermanentId,
    PlayerId, Step, Target,
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

/// The Djinn on the battlefield with its counters, and `top` on top of the library.
fn djinn_with_top(db: &CardDatabase, top: &str) -> (GameState, PermanentId, CardInstance) {
    let mut state = main_phase();
    let card = cid(db, "djinn_of_wishes");
    let instance = state.new_instance(card).id;
    let id = PermanentId(state.mint_id());
    let mut djinn = Permanent {
        id,
        instance,
        printed: card.into(),
        controller: PlayerId(0),
        ..Default::default()
    };
    djinn.counters.insert(CounterKind::Wish, 3);
    state.battlefield.push(djinn);

    // Two cards, so the one under the top stays put whatever happens above it.
    let under = state.new_instance(cid(db, "forest"));
    let revealed = state.new_instance(cid(db, top));
    state.players[0].library = vec![under, revealed];
    (state, id, revealed)
}

/// Activate the Djinn and let the ability resolve, so the offer is standing.
fn offer(state: &GameState, db: &CardDatabase, djinn: PermanentId) -> GameState {
    let activation = valid_actions(state, db)
        .into_iter()
        .find(|action| matches!(action, Action::ActivateAbility { permanent, .. } if *permanent == djinn))
        .expect("the Djinn's ability is offered");
    let state = apply_action(state, &activation, db);
    let state = apply_action(&state, &Action::PassPriority, db);
    apply_action(&state, &Action::PassPriority, db)
}

/// While the offer stands, the player is offered that card's play and **nothing else**.
#[test]
fn issue_787_the_offer_is_the_card_s_own_play_and_a_decline() {
    let db = db();
    let (state, djinn, revealed) = djinn_with_top(&db, "shock");
    let state = offer(&state, &db, djinn);

    assert!(
        pending_player_choice(&state).is_some_and(|p| p.question.play_card().is_some()),
        "the resolution suspended on the offer"
    );

    let offered = valid_actions(&state, &db);
    assert!(
        offered.iter().any(
            |action| matches!(action, Action::CastSpell { card, .. } if card.id == revealed.id)
        ),
        "the revealed card's own cast is offered: {offered:#?}"
    );
    assert!(
        offered
            .iter()
            .any(|action| matches!(action, Action::AnswerConfirm { accept: false })),
        "and so is the decline"
    );
    // Nothing else: no pass, no other cast, no activation. The game is frozen on this
    // question exactly as it is on every other one.
    assert!(
        !offered.iter().any(|action| matches!(
            action,
            Action::PassPriority | Action::PlayLand { .. } | Action::ActivateAbility { .. }
        )),
        "no other action is offered while the question stands: {offered:#?}"
    );
}

/// It is free, and the price reaches that card and no other.
#[test]
fn issue_787_the_revealed_card_is_free_and_nothing_else_is() {
    let db = db();
    let (mut state, djinn, revealed) = djinn_with_top(&db, "shock");
    // A second Shock in hand, to prove the free price is about the offered instance rather
    // than about the card or the player.
    let in_hand = state.new_instance(cid(&db, "shock"));
    state.players[0].hand.push(in_hand);

    let state = offer(&state, &db, djinn);
    assert_eq!(
        total_cast_cost(&state, &db, revealed),
        Some(sage_engine::ManaCost::default()),
        "the offered card costs nothing"
    );
    assert_ne!(
        total_cast_cost(&state, &db, in_hand),
        Some(sage_engine::ManaCost::default()),
        "the copy in hand still costs what it prints"
    );
}

/// Taking the offer casts it — with its own target — and the resolution finishes.
#[test]
fn issue_787_casting_the_revealed_card_announces_its_targets_and_resumes() {
    let db = db();
    let (mut state, djinn, revealed) = djinn_with_top(&db, "shock");
    let victim = {
        let card = cid(&db, "onakke_ogre");
        let instance = state.new_instance(card).id;
        let id = PermanentId(state.mint_id());
        state.battlefield.push(Permanent {
            id,
            instance,
            printed: card.into(),
            controller: PlayerId(1),
            ..Default::default()
        });
        id
    };
    let state = offer(&state, &db, djinn);

    // The offer is the *requirement* form; the target is named here, exactly as for a cast
    // from hand. A free cast is still an announcement.
    let cast = Action::CastSpell {
        card: revealed,
        mode: None,
        x: None,
        targets: vec![Target::Permanent(victim)],
        payment: Vec::new(),
    };
    let state = apply_action(&state, &cast, &db);

    assert!(
        pending_player_choice(&state).is_none(),
        "playing the card answered the question"
    );
    assert!(
        !state.players[0]
            .library
            .iter()
            .any(|card| card.id == revealed.id),
        "the card left the library"
    );
    assert!(!state.stack.is_empty(), "and it is on the stack");

    // It resolves like any other spell, against the target it named.
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    assert!(
        !state.battlefield.iter().any(|perm| perm.id == victim),
        "the Shock it cast killed what it was aimed at"
    );
}

/// Declining exiles it, and the card underneath is untouched.
#[test]
fn issue_787_declining_exiles_the_revealed_card() {
    let db = db();
    let (state, djinn, revealed) = djinn_with_top(&db, "shock");
    let state = offer(&state, &db, djinn);

    let state = apply_action(&state, &Action::AnswerConfirm { accept: false }, &db);
    assert!(
        pending_player_choice(&state).is_none(),
        "declining answered the question"
    );
    assert!(
        state.players[0]
            .exile
            .iter()
            .any(|card| card.id == revealed.id),
        "the card the player did not play is exiled"
    );
    assert_eq!(
        state.players[0].library.len(),
        1,
        "and the card under it stayed where it was"
    );
}

/// A revealed **land** is offered as a play, and spends the turn's land drop.
#[test]
fn issue_787_a_revealed_land_is_played_under_the_ordinary_allowance() {
    let db = db();
    let (state, djinn, revealed) = djinn_with_top(&db, "mountain");
    let state = offer(&state, &db, djinn);

    let play = valid_actions(&state, &db)
        .into_iter()
        .find(|action| matches!(action, Action::PlayLand { card } if card.id == revealed.id))
        .expect("the revealed land is offered as a play");
    let state = apply_action(&state, &play, &db);

    assert!(
        state
            .battlefield
            .iter()
            .any(|perm| perm.instance == revealed.id),
        "the land reached the battlefield"
    );
    assert!(state.land_played, "and it spent the turn's land drop");
    assert!(pending_player_choice(&state).is_none());
}

/// An empty library asks nothing rather than stalling.
#[test]
fn issue_787_an_empty_library_asks_nothing() {
    let db = db();
    let (mut state, djinn, _) = djinn_with_top(&db, "shock");
    state.players[0].library.clear();
    let state = offer(&state, &db, djinn);
    assert!(pending_player_choice(&state).is_none());
    assert!(state.stack.is_empty(), "the ability finished resolving");
}
