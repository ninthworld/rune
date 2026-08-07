//! Ajani's Last Stand (issue #706): a card that pays with itself, and one that triggers
//! from a **hand**.
//!
//! Three things the vocabulary could not say, and the third is the interesting one.
//! `Whenever a creature or planeswalker you control dies` widens a class that only ever
//! named creatures. `You may sacrifice this enchantment` is the one optional cost that
//! asks nothing further — the permanent is named by the sentence rather than picked.
//!
//! `When a spell or ability an opponent controls causes you to discard this card` is an
//! ability that functions in a zone nobody can see. It is read by diffing the hand it was
//! in — the card was there and is not — and narrowed by **who caused it**, which the
//! recorded event now carries: a hand one card lighter says nothing about why it is
//! lighter, so a cleanup discard and an opponent's Mind Rot would otherwise be the same
//! event.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, pending_player_choice, Action, CardDatabase, CardId, Color, FunctionalId,
    GameState, Permanent, PermanentId, PlayerId, Step, Target,
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

/// How many 4/4 Avatar tokens are on the battlefield.
fn avatars(state: &GameState) -> usize {
    state
        .battlefield
        .iter()
        .filter(|perm| perm.printed.is_token())
        .count()
}

/// Kill `victim` with a Murder aimed at it, and settle.
fn murder(state: &GameState, db: &CardDatabase, victim: PermanentId) -> GameState {
    let mut state = state.clone();
    let murder = state.new_instance(cid(db, "murder"));
    state.players[0].hand.push(murder);
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: murder,
            mode: None,
            x: None,
            targets: vec![Target::Permanent(victim)],
            payment: Vec::new(),
        },
        db,
    );
    settle(&state, db)
}

/// **The crux of the first ability.** A creature dying offers the trade, and accepting
/// pays with the enchantment itself — which is gone afterwards, and the Avatar is not.
#[test]
fn issue_706_the_enchantment_trades_itself_for_an_avatar() {
    let db = db();
    let mut state = main_phase(&db);
    let stand = place(&mut state, &db, "ajani_s_last_stand", PlayerId(0));
    let bear = place(&mut state, &db, "bogstomper", PlayerId(0));

    let state = murder(&state, &db, bear);
    assert!(
        pending_player_choice(&state).is_some(),
        "the trade is offered"
    );
    let state = apply_action(&state, &Action::AnswerConfirm { accept: true }, &db);
    let state = settle(&state, &db);

    assert!(
        !state.battlefield.iter().any(|perm| perm.id == stand),
        "the enchantment paid for it"
    );
    assert_eq!(avatars(&state), 1, "and the Avatar arrived");
}

/// Declining keeps the enchantment and makes nothing — the cost is the whole of what buys
/// the token.
#[test]
fn issue_706_declining_keeps_the_enchantment_and_makes_nothing() {
    let db = db();
    let mut state = main_phase(&db);
    let stand = place(&mut state, &db, "ajani_s_last_stand", PlayerId(0));
    let bear = place(&mut state, &db, "bogstomper", PlayerId(0));

    let state = murder(&state, &db, bear);
    let state = apply_action(&state, &Action::AnswerConfirm { accept: false }, &db);
    let state = settle(&state, &db);

    assert!(
        state.battlefield.iter().any(|perm| perm.id == stand),
        "the enchantment is still there"
    );
    assert_eq!(avatars(&state), 0, "and nothing was made");
}

/// The class is wider than creatures: a **planeswalker** dying offers the same trade.
#[test]
fn issue_706_a_dying_planeswalker_offers_the_trade_too() {
    let db = db();
    let mut state = main_phase(&db);
    place(&mut state, &db, "ajani_s_last_stand", PlayerId(0));
    let walker = place(&mut state, &db, "vivien_reid", PlayerId(0));
    // Three loyalty, and three damage takes all of it (CR 120.3c, CR 704.5i) — a
    // planeswalker is not a legal target for a creature-killing spell.
    if let Some(perm) = state.battlefield.iter_mut().find(|perm| perm.id == walker) {
        perm.counters.insert(sage_engine::CounterKind::Loyalty, 3);
    }
    let strike = state.new_instance(cid(&db, "lightning_strike"));
    state.players[0].hand.push(strike);
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: strike,
            mode: None,
            x: None,
            targets: vec![Target::Permanent(walker)],
            payment: Vec::new(),
        },
        &db,
    );
    let state = settle(&state, &db);

    assert!(
        pending_player_choice(&state).is_some(),
        "a planeswalker is in the class the card names"
    );
}

/// **The crux of the second ability.** An opponent's spell makes you discard it, and the
/// card triggers from the hand it was in — with a Plains out, that is an Avatar.
#[test]
fn issue_706_discarding_it_to_an_opponents_spell_makes_an_avatar() {
    let db = db();
    let mut state = main_phase(&db);
    place(&mut state, &db, "plains", PlayerId(0));
    let stand = state.new_instance(cid(&db, "ajani_s_last_stand"));
    let mind_rot = state.new_instance(cid(&db, "mind_rot"));
    state.players[0].hand = vec![stand];
    state.players[1].hand = vec![mind_rot];
    // Seat 1's turn, so their sorcery is castable (CR 307.1).
    state.active_player = PlayerId(1);
    state.priority = PlayerId(1);
    state.players[1].turn_began = state.turn;

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
    let state = settle(&state, &db);
    let state = apply_action(
        &state,
        &Action::AnswerChoice {
            chosen: vec![stand.id],
        },
        &db,
    );
    let state = settle(&state, &db);

    assert_eq!(
        avatars(&state),
        1,
        "the discarded card triggered from the hand it was in"
    );
}

/// With no Plains, the trigger still fires and its intervening if answers no.
#[test]
fn issue_706_without_a_plains_the_discard_makes_nothing() {
    let db = db();
    let mut state = main_phase(&db);
    let stand = state.new_instance(cid(&db, "ajani_s_last_stand"));
    let mind_rot = state.new_instance(cid(&db, "mind_rot"));
    state.players[0].hand = vec![stand];
    state.players[1].hand = vec![mind_rot];
    state.active_player = PlayerId(1);
    state.priority = PlayerId(1);
    state.players[1].turn_began = state.turn;

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
    let state = settle(&state, &db);
    let state = apply_action(
        &state,
        &Action::AnswerChoice {
            chosen: vec![stand.id],
        },
        &db,
    );
    let state = settle(&state, &db);

    assert_eq!(avatars(&state), 0, "no Plains, no Avatar");
    assert_eq!(
        state.players[0].graveyard.len(),
        1,
        "and the card really was discarded"
    );
}

/// A discard the card's **own** controller caused is not what it watches: `an opponent
/// controls` is part of the condition, and a card pitched to your own cost was your doing.
#[test]
fn issue_706_pitching_it_yourself_triggers_nothing() {
    let db = db();
    let mut state = main_phase(&db);
    place(&mut state, &db, "plains", PlayerId(0));
    let stand = state.new_instance(cid(&db, "ajani_s_last_stand"));
    let mind_rot = state.new_instance(cid(&db, "mind_rot"));
    state.players[0].hand = vec![stand, mind_rot];

    // Seat 0 casts the discard spell at themselves.
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
    let state = settle(&state, &db);
    let state = apply_action(
        &state,
        &Action::AnswerChoice {
            chosen: vec![stand.id],
        },
        &db,
    );
    let state = settle(&state, &db);

    assert_eq!(avatars(&state), 0, "your own spell is not an opponent's");
}
