//! Bone Dragon: an ability that functions from a graveyard and charges more than mana
//! (issue #723).
//!
//! `{3}{B}{B}, Exile seven other cards from your graveyard: Return this card from your
//! graveyard to the battlefield tapped.`
//!
//! Two rules meet here and both used to say no. A graveyard ability could charge **mana and
//! nothing else** — a rule whose premise (a card in a zone has nothing to tap, sacrifice, or
//! spend counters from) is true of those three and false of exiling from the very graveyard
//! the card is lying in. And the exile cost had no notion of *other*, so a Dragon could have
//! paid for its own return by exiling itself — exiling the card it was about to bring back,
//! which is a card that reads as recursive and never is.
//!
//! Every test drives the real [`apply_action`] pipeline and the real [`valid_actions`] offer.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, valid_actions, Action, CardDatabase, CardId, CardInstance, Color, CostPayment,
    FunctionalId, GameState, PlayerId, Step,
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

/// A graveyard holding the Dragon and `fodder` other cards, in that order.
fn graveyard(db: &CardDatabase, fodder: usize) -> (GameState, CardInstance) {
    let mut state = main_phase();
    let dragon = state.new_instance(cid(db, "bone_dragon"));
    state.players[0].graveyard.push(dragon);
    for _ in 0..fodder {
        let card = state.new_instance(cid(db, "walking_corpse"));
        state.players[0].graveyard.push(card);
    }
    (state, dragon)
}

/// The activation the engine offers for the Dragon, if it offers one.
fn offered(state: &GameState, db: &CardDatabase, dragon: CardInstance) -> Option<Action> {
    valid_actions(state, db).into_iter().find(
        |action| matches!(action, Action::ActivateAbilityFromGraveyard { card, .. } if card.id == dragon.id),
    )
}

/// The seven cards the cost would take, chosen from the graveyard by the test.
fn seven(state: &GameState, dragon: CardInstance) -> Vec<CostPayment> {
    state.players[0]
        .graveyard
        .iter()
        .filter(|card| card.id != dragon.id)
        .take(7)
        .map(|card| CostPayment::Exile(card.id))
        .collect()
}

/// With seven other cards behind it, the ability is offered and returns the Dragon tapped.
#[test]
fn issue_723_the_dragon_returns_itself_tapped_for_seven_other_cards() {
    let db = db();
    let (state, dragon) = graveyard(&db, 7);
    assert!(
        offered(&state, &db, dragon).is_some(),
        "the ability is offered once the cost can be paid"
    );

    let activation = Action::ActivateAbilityFromGraveyard {
        card: dragon,
        index: 0,
        targets: Vec::new(),
        payment: seven(&state, dragon),
    };
    let state = apply_action(&state, &activation, &db);
    // The cost is paid as the ability is activated (CR 601.2h): seven cards have left the
    // graveyard for exile, and the Dragon has not.
    assert_eq!(state.players[0].exile.len(), 7);
    assert!(
        state.players[0]
            .graveyard
            .iter()
            .any(|card| card.id == dragon.id),
        "the Dragon is still in the graveyard while its ability is on the stack"
    );

    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    let returned = state
        .battlefield
        .iter()
        .find(|perm| perm.instance == dragon.id)
        .expect("the Dragon returned to the battlefield");
    assert!(returned.tapped, "and it returned tapped");
    assert!(
        !state.players[0]
            .graveyard
            .iter()
            .any(|card| card.id == dragon.id),
        "and it is no longer in the graveyard"
    );
}

/// **Other**: the Dragon may not pay for its own return with itself.
#[test]
fn issue_723_the_dragon_is_not_among_the_cards_that_pay_for_it() {
    let db = db();
    // Six others plus the Dragon is seven cards in the graveyard — enough only if the Dragon
    // counts itself, which is exactly what the word "other" forbids.
    let (state, dragon) = graveyard(&db, 6);
    assert!(
        offered(&state, &db, dragon).is_none(),
        "six other cards cannot pay a cost that asks for seven"
    );

    // And naming itself is refused outright rather than quietly accepted.
    let mut payment = seven(&state, dragon);
    payment.push(CostPayment::Exile(dragon.id));
    let forged = Action::ActivateAbilityFromGraveyard {
        card: dragon,
        index: 0,
        targets: Vec::new(),
        payment,
    };
    assert_eq!(
        apply_action(&state, &forged, &db),
        state,
        "a payment naming the source is refused, and nothing is charged"
    );
}

/// A payment that is short, or names a card that is not there, pays nothing at all.
#[test]
fn issue_723_a_wrong_payment_is_refused_rather_than_partly_charged() {
    let db = db();
    let (state, dragon) = graveyard(&db, 7);

    let short = Action::ActivateAbilityFromGraveyard {
        card: dragon,
        index: 0,
        targets: Vec::new(),
        payment: seven(&state, dragon).into_iter().take(6).collect(),
    };
    assert_eq!(
        apply_action(&state, &short, &db),
        state,
        "six cards do not pay a cost of seven, and nothing is exiled"
    );
}

/// The mana half is still owed: a cost is paid all at once or not at all.
#[test]
fn issue_723_the_mana_half_is_still_owed() {
    let db = db();
    let (mut state, dragon) = graveyard(&db, 7);
    state.players[0].mana_pool = sage_engine::ManaPool::default();
    assert!(
        offered(&state, &db, dragon).is_none(),
        "an empty pool cannot pay {{3}}{{B}}{{B}}, however many cards are behind it"
    );
}
