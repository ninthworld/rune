//! Two creatures that **become** something else, and the allowance that lets one do it
//! only once a turn (CR 205.1b, CR 602.5f, issue #706).
//!
//! `Becomes a Human` is not `is a Knight in addition to its other types`. CR 205.1b sets
//! the creature types where the Equipment's grant adds one, and the difference is exactly
//! what a card like this is for: Chromium stops being an Elder Dragon, which is what makes
//! it survivable rather than merely small.
//!
//! The per-turn allowance is per **ability**, not per permanent — the ledger a planeswalker
//! uses is per permanent (CR 606.3), and a card with two limited abilities would spend one
//! and keep the other.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, characteristics, Action, CardDatabase, CardId, CardInstance, Color, FunctionalId,
    GameState, Keyword, Permanent, PermanentId, PlayerId, Step,
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
        for color in [Color::Green, Color::White, Color::Blue, Color::Black] {
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
        entered_turn: 0,
        ..Default::default()
    });
    id
}

fn to_hand(state: &mut GameState, db: &CardDatabase, slug: &str, seat: PlayerId) -> CardInstance {
    let instance = state.new_instance(cid(db, slug));
    state.players[seat.0].hand.push(instance);
    instance
}

/// Activate ability 0 of `permanent`, carrying `payment` for a cost the player picks
/// (CR 601.2h — the payment rides the action, exactly as a target does).
fn activate_paying(
    state: &GameState,
    db: &CardDatabase,
    permanent: PermanentId,
    payment: Vec<sage_engine::CostPayment>,
) -> GameState {
    let state = apply_action(
        state,
        &Action::ActivateAbility {
            permanent,
            index: 0,
            targets: Vec::new(),
            payment,
        },
        db,
    );
    let state = apply_action(&state, &Action::PassPriority, db);
    apply_action(&state, &Action::PassPriority, db)
}

fn activate(state: &GameState, db: &CardDatabase, permanent: PermanentId) -> GameState {
    activate_paying(state, db, permanent, Vec::new())
}

/// The Champion becomes a Bear Berserker — and stops being a Human.
#[test]
fn ursine_champion_replaces_its_creature_types() {
    let db = db();
    let mut state = main_phase(&db);
    let champion = place(&mut state, &db, "ursine_champion", PlayerId(0));
    let before = characteristics(&state, champion, &db);
    assert!(before.subtypes.iter().any(|kind| kind == "Human"));

    let state = activate(&state, &db, champion);

    let after = characteristics(&state, champion, &db);
    assert!(after.subtypes.iter().any(|kind| kind == "Bear"));
    assert!(after.subtypes.iter().any(|kind| kind == "Berserker"));
    assert!(
        !after.subtypes.iter().any(|kind| kind == "Human"),
        "CR 205.1b: *becomes a Bear Berserker* sets the types rather than adding to them"
    );
    assert_eq!(after.power, Some(5), "and it is +3/+3");
}

/// `Activate only once each turn`: the second activation is not offered.
#[test]
fn the_champion_may_be_activated_only_once_a_turn() {
    let db = db();
    let mut state = main_phase(&db);
    let champion = place(&mut state, &db, "ursine_champion", PlayerId(0));

    let state = activate(&state, &db, champion);
    let mut state = state;
    state.players[0].mana_pool.add(Color::Green, 10);
    state.players[0].mana_pool.add_colorless(10);

    let offered = sage_engine::valid_actions(&state, &db)
        .into_iter()
        .filter(|action| {
            matches!(action, Action::ActivateAbility { permanent, .. } if *permanent == champion)
        })
        .count();
    assert_eq!(offered, 0, "the allowance is spent");

    // And submitting it anyway is refused by the same rule, re-derived.
    let refused = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: champion,
            index: 0,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    assert_eq!(
        characteristics(&refused, champion, &db).power,
        characteristics(&state, champion, &db).power,
        "nothing happened"
    );
}

/// The allowance is per turn: next turn it may be activated again.
#[test]
fn the_allowance_comes_back_next_turn() {
    let db = db();
    let mut state = main_phase(&db);
    let champion = place(&mut state, &db, "ursine_champion", PlayerId(0));
    let state = activate(&state, &db, champion);
    assert!(!state.limited_activations.is_empty(), "spent");

    // `advance` walks one step; the allowance refreshes when the *turn* does, so the walk
    // has to reach it.
    let mut next = state;
    for _ in 0..40 {
        if next.limited_activations.is_empty() {
            break;
        }
        next = next.advance();
    }
    assert!(
        next.limited_activations.is_empty(),
        "the turn boundary clears it, like the loyalty allowance beside it"
    );
}

/// Chromium turns itself into a 1/1 Human with hexproof and no abilities — including the
/// flying it printed.
#[test]
fn chromium_becomes_a_hexproof_human() {
    let db = db();
    let mut state = main_phase(&db);
    let chromium = place(&mut state, &db, "chromium_the_mutable", PlayerId(0));
    let fodder = to_hand(&mut state, &db, "bogstomper", PlayerId(0));
    let before = characteristics(&state, chromium, &db);
    assert_eq!(before.power, Some(7));
    assert!(before.keywords.contains(&Keyword::Flying));

    let state = activate_paying(
        &state,
        &db,
        chromium,
        vec![sage_engine::CostPayment::Discard(fodder.id)],
    );

    let after = characteristics(&state, chromium, &db);
    assert_eq!((after.power, after.toughness), (Some(1), Some(1)));
    assert!(after.subtypes.iter().any(|kind| kind == "Human"));
    assert!(
        !after.subtypes.iter().any(|kind| kind == "Dragon"),
        "it is not an Elder Dragon any more, which is the whole point"
    );
    assert!(after.keywords.contains(&Keyword::Hexproof));
    assert!(
        !after.keywords.contains(&Keyword::Flying),
        "losing all abilities took the printed flying with it"
    );
    assert_eq!(
        state.players[0].graveyard.len(),
        1,
        "and a card was discarded"
    );
}
