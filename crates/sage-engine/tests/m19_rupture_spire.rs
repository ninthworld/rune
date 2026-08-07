//! Rupture Spire, and the `unless you pay` an optional effect could not say (issue #706).
//!
//! `This land enters tapped. When this land enters, sacrifice it unless you pay {1}.
//! {T}: Add one mana of any color.`
//!
//! Every `you may` in the IR meant *and if you don't, nothing happens*. This card's
//! decline has a consequence, which is one field — `otherwise` — and one rule about when
//! the question is worth asking: a player who **cannot** pay is not asked, because there
//! is nothing to decide; the consequence simply happens.
//!
//! "Cannot pay" is asked of the potential pool rather than the floating one, because a
//! player owing this question may still tap lands for mana (CR 605.3a). A toll you could
//! pay by tapping something is a real question.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, pending_player_choice, Action, CardDatabase, CardId, CardInstance, Color,
    FunctionalId, GameState, Permanent, PermanentId, PlayerId, Step,
};

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// Seat 0's main phase with an **empty** pool: what a player could pay is what they can
/// tap for, which is the whole question this card asks.
fn main_phase(db: &CardDatabase) -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    state.players[0].turn_began = state.turn;
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

fn to_hand(state: &mut GameState, db: &CardDatabase, slug: &str, seat: PlayerId) -> CardInstance {
    let instance = state.new_instance(cid(db, slug));
    state.players[seat.0].hand.push(instance);
    instance
}

/// Play the Spire and let its enters trigger resolve as far as it goes.
fn play_spire(state: &GameState, db: &CardDatabase, land: CardInstance) -> GameState {
    let state = apply_action(state, &Action::PlayLand { card: land }, db);
    let state = apply_action(&state, &Action::PassPriority, db);
    apply_action(&state, &Action::PassPriority, db)
}

fn spire_on_battlefield(state: &GameState, db: &CardDatabase) -> bool {
    let spire = cid(db, "rupture_spire");
    state
        .battlefield
        .iter()
        .any(|perm| perm.printed.card() == Some(spire))
}

/// It enters tapped, and the toll is asked of a player who could pay it.
#[test]
fn rupture_spire_asks_for_its_toll_and_keeps_itself_when_paid() {
    let db = db();
    let mut state = main_phase(&db);
    // An untapped Forest is the whole reason this is a question rather than a fact.
    let forest = place(&mut state, &db, "forest", PlayerId(0));
    let land = to_hand(&mut state, &db, "rupture_spire", PlayerId(0));

    let state = play_spire(&state, &db, land);

    assert!(
        state
            .battlefield
            .iter()
            .any(|perm| perm.printed.card() == Some(cid(&db, "rupture_spire")) && perm.tapped),
        "it enters tapped"
    );
    let pending = pending_player_choice(&state).expect("the toll is asked");
    assert_eq!(pending.chooser, PlayerId(0));

    // Tapping for mana while a choice is owed is the one thing that stays legal
    // (CR 605.3a), and it is how this toll gets paid.
    let state = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: forest,
            index: 0,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    let state = apply_action(&state, &Action::AnswerConfirm { accept: true }, &db);

    assert!(spire_on_battlefield(&state, &db), "paid, so it stays");
    assert_eq!(
        state.players[0].mana_pool.green, 0,
        "and the mana went to the toll"
    );
}

/// Declining takes the other branch, which is the half the IR could not say before.
#[test]
fn rupture_spire_sacrifices_itself_when_the_toll_is_declined() {
    let db = db();
    let mut state = main_phase(&db);
    place(&mut state, &db, "forest", PlayerId(0));
    let land = to_hand(&mut state, &db, "rupture_spire", PlayerId(0));

    let state = play_spire(&state, &db, land);
    assert!(pending_player_choice(&state).is_some(), "the toll is asked");
    let state = apply_action(&state, &Action::AnswerConfirm { accept: false }, &db);

    assert!(
        !spire_on_battlefield(&state, &db),
        "declined, so it sacrificed itself"
    );
    assert!(
        state.players[0]
            .graveyard
            .iter()
            .any(|card| card.card == cid(&db, "rupture_spire")),
        "and it is in its owner's graveyard, as a sacrifice goes (CR 701.17)"
    );
}

/// A player with **no way at all** to pay is not asked. There is no decision, so the
/// consequence happens and the resolution moves on.
#[test]
fn a_player_who_cannot_pay_is_not_asked() {
    let db = db();
    let mut state = main_phase(&db);
    let land = to_hand(&mut state, &db, "rupture_spire", PlayerId(0));

    // No other land, an empty pool: nothing anywhere could produce {1}.
    let state = play_spire(&state, &db, land);

    assert!(
        pending_player_choice(&state).is_none(),
        "no question, because there is no answer to give"
    );
    assert!(
        !spire_on_battlefield(&state, &db),
        "the consequence happened without being asked about"
    );
}

/// The tapped land it entered with is not a way to pay: what matters is what could still
/// be tapped, and the Spire itself entered tapped.
#[test]
fn the_spire_cannot_pay_its_own_toll() {
    let db = db();
    let mut state = main_phase(&db);
    let tapped_forest = place(&mut state, &db, "forest", PlayerId(0));
    if let Some(perm) = state
        .battlefield
        .iter_mut()
        .find(|perm| perm.id == tapped_forest)
    {
        perm.tapped = true;
    }
    let land = to_hand(&mut state, &db, "rupture_spire", PlayerId(0));

    let state = play_spire(&state, &db, land);

    assert!(
        pending_player_choice(&state).is_none(),
        "a tapped Forest is not a way to pay, and the Spire is tapped too"
    );
    assert!(!spire_on_battlefield(&state, &db));
}

/// Once it has paid its way in, it is a land that taps for any colour.
#[test]
fn rupture_spire_taps_for_a_colour_it_is_asked_about() {
    let db = db();
    let mut state = main_phase(&db);
    let forest = place(&mut state, &db, "forest", PlayerId(0));
    let land = to_hand(&mut state, &db, "rupture_spire", PlayerId(0));
    let state = play_spire(&state, &db, land);
    let state = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: forest,
            index: 0,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    let mut state = apply_action(&state, &Action::AnswerConfirm { accept: true }, &db);

    // It untaps in the ordinary way; this test is about the ability, not the timing.
    let spire = state
        .battlefield
        .iter()
        .find(|perm| perm.printed.card() == Some(cid(&db, "rupture_spire")))
        .expect("still there")
        .id;
    if let Some(perm) = state.battlefield.iter_mut().find(|perm| perm.id == spire) {
        perm.tapped = false;
    }

    let state = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: spire,
            index: 2,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    let pending = pending_player_choice(&state).expect("which colour?");
    assert!(pending.question.color().is_some());
    let state = apply_action(&state, &Action::AnswerColor { color: Color::Blue }, &db);

    assert_eq!(state.players[0].mana_pool.blue, 1, "one mana, any colour");
}
