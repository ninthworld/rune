//! Playing a card from **exile**, under a permission granted for the turn (issue #723).
//!
//! Dark-Dweller Oracle is the smallest card that proves the seam: `{1}, Sacrifice a creature:
//! Exile the top card of your library. Until end of turn, you may play that card.`
//!
//! The permission names **instances**, not a class, and that is the whole point of the shape.
//! *That card* is the one this activation exiled; a card that reached exile any other way is
//! not offered, and the permission dies with the turn.
//!
//! Every test drives the real [`apply_action`] pipeline and the real [`valid_actions`] offer.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, valid_actions, Action, CardDatabase, CardId, CardInstance, Color, CostPayment,
    FunctionalId, GameState, Permanent, PermanentId, PlayerId, Step,
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

/// Seat 0's library, **top first**.
fn library_of(state: &mut GameState, db: &CardDatabase, slugs: &[&str]) -> Vec<CardInstance> {
    let instances: Vec<_> = slugs
        .iter()
        .map(|slug| state.new_instance(cid(db, slug)))
        .collect();
    state.players[0].library = instances.iter().copied().rev().collect();
    instances
}

/// A board with the Oracle out, a creature to feed it, and a library whose top card is
/// `top`. Returns the state with the Oracle's ability already activated and resolved.
fn oracle_activated(db: &CardDatabase, top: &str) -> (GameState, CardInstance) {
    let mut state = main_phase();
    let oracle = place(&mut state, db, "dark_dweller_oracle", PlayerId(0));
    let food = place(&mut state, db, "onakke_ogre", PlayerId(0));
    let library = library_of(&mut state, db, &[top, "forest"]);

    // The offer carries no payment — a sacrifice is chosen by the player — so the
    // creature being fed to it is named here, the way the server's prompt would.
    assert!(
        valid_actions(&state, db)
            .iter()
            .any(|action| matches!(action, Action::ActivateAbility { permanent, .. } if *permanent == oracle)),
        "the Oracle's ability is offered"
    );
    let activation = Action::ActivateAbility {
        permanent: oracle,
        index: 0,
        targets: Vec::new(),
        payment: vec![CostPayment::Sacrifice(food)],
    };
    let state = apply_action(&state, &activation, db);
    let state = apply_action(&state, &Action::PassPriority, db);
    let state = apply_action(&state, &Action::PassPriority, db);
    (state, library[0])
}

/// The exiled card is offered, and it is offered as an ordinary cast.
#[test]
fn issue_723_a_card_exiled_this_way_may_be_cast_this_turn() {
    let db = db();
    let (state, exiled) = oracle_activated(&db, "shock");

    assert!(
        state.players[0].exile.iter().any(|c| c.id == exiled.id),
        "the top card was exiled"
    );
    let offered = valid_actions(&state, &db);
    assert!(
        offered
            .iter()
            .any(|action| matches!(action, Action::CastSpell { card, .. } if card.id == exiled.id)),
        "the exiled card is offered as a cast: {offered:#?}"
    );

    // And taking the offer works: it leaves exile for the stack, exactly as a hand cast
    // does. Offering a cast the apply path then refused would be the worst of both. The
    // offer advertises the *requirement* form with empty targets, so the target is filled
    // in here the way the server's target slot fills it.
    let cast = Action::CastSpell {
        card: exiled,
        mode: None,
        x: None,
        targets: vec![sage_engine::Target::Player(PlayerId(1))],
        payment: Vec::new(),
    };
    let after = apply_action(&state, &cast, &db);
    assert!(
        !after.players[0].exile.iter().any(|c| c.id == exiled.id),
        "the card left exile for the stack"
    );
    assert!(
        after
            .stack
            .iter()
            .any(|object| object.controller == PlayerId(0)),
        "the spell is on the stack"
    );
}

/// A land among them is **played**, not cast (CR 116.2a), and it costs the land drop.
#[test]
fn issue_723_a_land_exiled_this_way_is_played_and_costs_the_land_drop() {
    let db = db();
    let (state, exiled) = oracle_activated(&db, "mountain");

    let offered = valid_actions(&state, &db);
    let play = offered
        .iter()
        .find(|action| matches!(action, Action::PlayLand { card } if card.id == exiled.id))
        .cloned()
        .expect("the exiled land is offered as a play");

    let after = apply_action(&state, &play, &db);
    assert!(
        after
            .battlefield
            .iter()
            .any(|perm| perm.instance == exiled.id),
        "the land reached the battlefield"
    );
    assert!(
        after.land_played,
        "a play from exile spends the turn's land drop like any other"
    );
}

/// The permission names the cards it exiled and nothing else in the zone.
#[test]
fn issue_723_a_card_exiled_any_other_way_is_not_offered() {
    let db = db();
    let (mut state, _exiled) = oracle_activated(&db, "shock");

    // A second castable card in the same zone, put there by nobody's permission.
    let intruder = state.new_instance(cid(&db, "lightning_strike"));
    state.players[0].exile.push(intruder);

    let offered = valid_actions(&state, &db);
    assert!(
        !offered.iter().any(
            |action| matches!(action, Action::CastSpell { card, .. } if card.id == intruder.id)
        ),
        "only the cards the permission named are playable"
    );
}

/// It lapses with the turn, and the card stays in exile.
#[test]
fn issue_723_the_permission_does_not_survive_the_turn() {
    let db = db();
    let (state, exiled) = oracle_activated(&db, "shock");
    assert!(
        !state.exile_playing.is_empty(),
        "the permission was granted"
    );

    // The turn boundary drops every per-turn permission. Reached by advancing the game
    // rather than by clearing the field, so this tests the boundary and not the setup.
    let mut later = state.clone();
    while later.turn == state.turn {
        later = later.advance();
    }
    assert!(
        later.exile_playing.is_empty(),
        "the permission lapsed with the turn"
    );
    assert!(
        later.players[0].exile.iter().any(|c| c.id == exiled.id),
        "the card is still in exile — it was never going back"
    );
}

/// An empty library exiles nothing and grants nothing, rather than stalling.
#[test]
fn issue_723_an_empty_library_grants_no_permission() {
    let db = db();
    let mut state = main_phase();
    let oracle = place(&mut state, &db, "dark_dweller_oracle", PlayerId(0));
    let food = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    state.players[0].library.clear();

    let activation = Action::ActivateAbility {
        permanent: oracle,
        index: 0,
        targets: Vec::new(),
        payment: vec![CostPayment::Sacrifice(food)],
    };
    let state = apply_action(&state, &activation, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    assert!(state.players[0].exile.is_empty());
    assert!(
        state.exile_playing.is_empty(),
        "no cards exiled, so no permission to play any"
    );
}
