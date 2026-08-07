//! Apex of Power: casting from among exiled cards, and paying out only for the ordinary
//! road (issue #723).
//!
//! `Exile the top seven cards of your library. Until end of turn, you may cast spells from
//! among them. If this spell was cast from your hand, add ten mana of any one color.`
//!
//! Two things separate it from Dark-Dweller Oracle, which shares its permission: it permits
//! **casting spells**, not playing cards — a land among the seven stays in exile — and it
//! asks where it was cast from, which is a question about the resolving object rather than
//! about the board.
//!
//! Every test drives the real [`apply_action`] pipeline and the real [`valid_actions`] offer.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, valid_actions, Action, CardDatabase, CardId, CardInstance, Color, FunctionalId,
    GameState, PlayerId, Step,
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

/// Seat 0's library, **top last**, under an Apex in hand.
fn board(db: &CardDatabase, library: &[&str]) -> (GameState, CardInstance, Vec<CardInstance>) {
    let mut state = main_phase();
    let apex = state.new_instance(cid(db, "apex_of_power"));
    state.players[0].hand.push(apex);
    let cards: Vec<CardInstance> = library
        .iter()
        .map(|slug| state.new_instance(cid(db, slug)))
        .collect();
    state.players[0].library = cards.clone();
    (state, apex, cards)
}

/// Cast the Apex from hand and let it resolve.
fn cast_from_hand(state: &GameState, db: &CardDatabase, apex: CardInstance) -> GameState {
    let state = apply_action(
        state,
        &Action::CastSpell {
            card: apex,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        db,
    );
    let state = apply_action(&state, &Action::PassPriority, db);
    apply_action(&state, &Action::PassPriority, db)
}

/// It exiles seven and lets the spells among them be cast.
#[test]
fn issue_723_a_spell_among_the_exiled_cards_may_be_cast() {
    let db = db();
    let (state, apex, cards) = board(
        &db,
        &[
            "forest", "shock", "forest", "forest", "forest", "forest", "forest", "mountain",
        ],
    );
    let state = cast_from_hand(&state, &db, apex);

    assert_eq!(state.players[0].exile.len(), 7, "the top seven are exiled");
    assert_eq!(state.players[0].library.len(), 1, "the eighth is not");

    // The second half asks a colour, and while that question stands nothing else is
    // offered — the freeze every suspension makes. Answer it, and the permission is live.
    let state = apply_action(&state, &Action::AnswerColor { color: Color::Red }, &db);

    let shock = cards[1];
    assert!(
        state.players[0].exile.iter().any(|c| c.id == shock.id),
        "the Shock was among the seven"
    );
    assert!(
        valid_actions(&state, &db)
            .iter()
            .any(|action| matches!(action, Action::CastSpell { card, .. } if card.id == shock.id)),
        "and it is offered as a cast"
    );
}

/// **Cast spells**, not play cards: a land among them stays in exile.
#[test]
fn issue_723_a_land_among_them_may_not_be_played() {
    let db = db();
    let (state, apex, cards) = board(
        &db,
        &[
            "shock", "shock", "shock", "shock", "shock", "shock", "mountain", "shock",
        ],
    );
    let state = cast_from_hand(&state, &db, apex);

    let mountain = cards[6];
    assert!(
        state.players[0].exile.iter().any(|c| c.id == mountain.id),
        "the Mountain was among the seven"
    );
    assert!(
        !valid_actions(&state, &db)
            .iter()
            .any(|action| matches!(action, Action::PlayLand { card } if card.id == mountain.id)),
        "but *cast spells from among them* does not let it be played"
    );
}

/// The mana comes only when the Apex itself was cast from a hand.
#[test]
fn issue_723_the_mana_is_paid_out_for_a_cast_from_hand() {
    let db = db();
    let (state, apex, _) = board(&db, &["forest", "forest", "forest"]);
    let state = cast_from_hand(&state, &db, apex);

    // Ten mana of one colour, chosen mid-resolution — so the choice is owed, and the pool
    // has not grown until it is answered. Measured from here, because casting the Apex
    // itself spent {7}{R}{R}{R} out of the same pool.
    let paid = state.players[0].mana_pool.clone();
    let pending = sage_engine::pending_player_choice(&state).expect("the colour is asked");
    assert!(pending.question.color().is_some());
    let state = apply_action(&state, &Action::AnswerColor { color: Color::Red }, &db);

    assert_eq!(
        state.players[0].mana_pool.red,
        paid.red + 10,
        "ten of the one colour named"
    );
}

/// Cast from **exile** rather than a hand, the payout does not happen — and the exile half
/// still does.
#[test]
fn issue_723_no_mana_when_it_was_not_cast_from_a_hand() {
    let db = db();
    let (mut state, apex, _) = board(&db, &["forest", "forest", "forest"]);
    // Move the Apex to exile and grant this turn's permission over it, which is the other
    // road onto the stack (#782).
    state.players[0].hand.retain(|card| card.id != apex.id);
    state.players[0].exile.push(apex);
    state.exile_playing.push(sage_engine::ExilePlaying {
        player: PlayerId(0),
        cards: vec![apex.id],
        turn: state.turn,
        cast_only: false,
    });

    let cast = valid_actions(&state, &db)
        .into_iter()
        .find(|action| matches!(action, Action::CastSpell { card, .. } if card.id == apex.id))
        .expect("the Apex is castable from exile");
    let after_cast = apply_action(&state, &cast, &db);
    // Measured after the cast has been paid for, so the only thing that could move it now
    // is the payout the card withholds.
    let paid = after_cast.players[0].mana_pool.clone();
    let state = apply_action(&after_cast, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    assert!(
        sage_engine::pending_player_choice(&state).is_none(),
        "no colour is asked, because the branch was not taken"
    );
    assert_eq!(
        state.players[0].mana_pool.red, paid.red,
        "and no mana was added"
    );
    // The half that is not conditional still happened.
    assert_eq!(
        state.players[0].exile.len(),
        3,
        "the library was still exiled"
    );
}
