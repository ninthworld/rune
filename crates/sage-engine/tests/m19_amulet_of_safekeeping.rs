//! Amulet of Safekeeping (issue #706): a card that watches its **controller** rather than
//! itself, and a static ability that names nobody's permanents in particular.
//!
//! `Whenever you become the target of a spell or ability an opponent controls, counter
//! that spell or ability unless its controller pays {1}` is three things the vocabulary
//! could not say. The first is a flag on a condition that already existed — a player is a
//! legal target exactly as a permanent is, so the same stack diff answers it. The second
//! is the trigger *naming* the object it saw, the way a delayed ability names "that
//! spell": by the time the trigger resolves the stack holds it among others, and nothing
//! about the ability would say which. The third is "its controller" — the same "that
//! player" every other sentence means, read off the object the sentence before it named.
//!
//! `Creature tokens get -1/-0` is the first static class that is symmetric — it reaches
//! its own controller's tokens and an opponent's alike — and the first that filters by
//! what a permanent *is* rather than by what it is printed as.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, characteristics, pending_player_choice, Action, CardDatabase, CardId, Color,
    FunctionalId, GameState, Permanent, PermanentId, PlayerId, Step, Target,
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

/// Seat 1 aims a Shock at seat 0; the state comes back at the first thing anyone is
/// asked, which — with an Amulet out — is the toll its trigger poses.
fn shock_the_amulets_controller(state: &GameState, db: &CardDatabase) -> GameState {
    let mut state = state.clone();
    let shock = state.new_instance(cid(db, "shock"));
    state.players[1].hand.push(shock);
    state.priority = PlayerId(1);
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: shock,
            mode: None,
            x: None,
            targets: vec![Target::Player(PlayerId(0))],
            payment: Vec::new(),
        },
        db,
    );
    settle(&state, db)
}

/// Pass priority until somebody is asked something, or the stack empties.
fn settle(state: &GameState, db: &CardDatabase) -> GameState {
    let mut state = state.clone();
    for _ in 0..12 {
        if state.stack.is_empty() || pending_player_choice(&state).is_some() {
            break;
        }
        state = apply_action(&state, &Action::PassPriority, db);
    }
    state
}

/// **The crux.** The Amulet's controller became a target, so the offer is posed — and it
/// is posed to the *spell's* controller, who is the one being asked to pay.
#[test]
fn issue_706_the_amulet_asks_the_spells_controller_to_pay() {
    let db = db();
    let mut state = main_phase(&db);
    place(&mut state, &db, "amulet_of_safekeeping", PlayerId(0));
    let life = state.players[0].life;

    let state = shock_the_amulets_controller(&state, &db);

    let pending = pending_player_choice(&state).expect("the toll is offered");
    assert_eq!(
        pending.chooser,
        PlayerId(1),
        "the spell's controller is the one asked"
    );

    let state = apply_action(&state, &Action::AnswerConfirm { accept: true }, &db);
    let state = settle(&state, &db);
    assert_eq!(
        state.players[0].life,
        life - 2,
        "they paid, so the Shock resolved"
    );
}

/// Declining counters the spell the trigger named — not whatever is on top of the stack,
/// which by then is the trigger itself.
#[test]
fn issue_706_declining_the_toll_counters_that_spell() {
    let db = db();
    let mut state = main_phase(&db);
    place(&mut state, &db, "amulet_of_safekeeping", PlayerId(0));
    let life = state.players[0].life;

    let state = shock_the_amulets_controller(&state, &db);
    let state = apply_action(&state, &Action::AnswerConfirm { accept: false }, &db);
    let state = settle(&state, &db);

    assert_eq!(state.players[0].life, life, "the Shock never resolved");
    assert!(state.stack.is_empty(), "and nothing is left on the stack");
}

/// A spell its **own** controller aims at them is not what the card watches: `an opponent
/// controls` is part of the condition.
#[test]
fn issue_706_your_own_spell_pays_no_toll() {
    let db = db();
    let mut state = main_phase(&db);
    place(&mut state, &db, "amulet_of_safekeeping", PlayerId(0));
    let shock = state.new_instance(cid(&db, "shock"));
    state.players[0].hand.push(shock);

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: shock,
            mode: None,
            x: None,
            targets: vec![Target::Player(PlayerId(0))],
            payment: Vec::new(),
        },
        &db,
    );

    assert!(
        pending_player_choice(&state).is_none(),
        "nothing was asked of your own spell"
    );
}

/// The static half: every creature token is a point smaller, whoever controls it, and a
/// creature that is not a token is untouched.
#[test]
fn issue_706_creature_tokens_are_smaller_on_both_sides_of_the_table() {
    let db = db();
    let mut state = main_phase(&db);
    place(&mut state, &db, "amulet_of_safekeeping", PlayerId(0));
    let card_creature = place(&mut state, &db, "bogstomper", PlayerId(0));
    // Gallant Cavalry brings 2/2 Knight tokens with it.
    let cavalry = state.new_instance(cid(&db, "gallant_cavalry"));
    state.players[0].hand.push(cavalry);
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: cavalry,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    // The creature resolves, then the enters trigger it puts on the stack does.
    let mut state = settle(&state, &db);
    for _ in 0..8 {
        if state.battlefield.iter().any(|perm| perm.printed.is_token()) {
            break;
        }
        state = apply_action(&state, &Action::PassPriority, &db);
    }

    let token = state
        .battlefield
        .iter()
        .find(|perm| perm.printed.is_token())
        .map(|perm| perm.id)
        .expect("the tokens arrived");
    assert_eq!(
        characteristics(&state, token, &db).power,
        Some(1),
        "a 2/2 token is a 1/2"
    );
    assert_eq!(
        characteristics(&state, card_creature, &db).power,
        Some(6),
        "and a creature that is not a token is untouched"
    );
}
