//! A **triggered** ability that functions while its source is in a graveyard (CR 113.6),
//! built against Spit Flame (M19 #160).
//!
//! The activated case (Reassembling Skeleton, `m19_graveyard_abilities.rs`) proved a card
//! in a zone can be an ability's source. This is the other direction: nobody activates
//! anything, so the ability has to be *found* — the diff-based collector walks the
//! graveyards beside the battlefield and the emblems, and which list reads a given
//! ability is decided by the ability itself. An instant sitting in a graveyard watches
//! for a Dragon; a creature card sitting in the same graveyard watches for nothing,
//! because its trigger works on the battlefield and it is not there.
//!
//! Every test drives the real [`apply_action`] pipeline over the bundled catalog. Cards
//! are named by their authored `functional_id`, never by an interned handle (ADR 0008 §3).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, valid_actions, Action, CardDatabase, CardId, CardInstance, Color, FunctionalId,
    GameState, PlayerId, Step,
};

// ----- fixtures -------------------------------------------------------------

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// A two-player game at player 0's precombat main, each seat floating `mana` of every
/// colour — enough to cast a Dragon and then pay for what its arrival offers.
fn main_phase(db: &CardDatabase, mana: u8) -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    let forest = cid(db, "forest");
    for seat in 0..2 {
        for color in [
            Color::White,
            Color::Blue,
            Color::Black,
            Color::Red,
            Color::Green,
        ] {
            state.players[seat].mana_pool.add(color, mana);
        }
        state.players[seat].mana_pool.add_colorless(mana);
        state.players[seat].library = (0..20).map(|_| state.new_instance(forest)).collect();
    }
    state
}

/// Put a card of `slug` into `seat`'s graveyard and return the instance.
fn bury(state: &mut GameState, db: &CardDatabase, slug: &str, seat: PlayerId) -> CardInstance {
    let instance = state.new_instance(cid(db, slug));
    state.players[seat.0].graveyard.push(instance);
    instance
}

/// Cast a Volcanic Dragon from `seat`'s hand out of the mana they are floating, and let
/// it resolve onto the battlefield. It is `seat`'s turn, since a creature is cast at
/// sorcery speed.
fn land_a_dragon(state: &GameState, db: &CardDatabase, seat: PlayerId) -> GameState {
    let mut state = state.clone();
    state.active_player = seat;
    state.priority = seat;
    let dragon = state.new_instance(cid(db, "volcanic_dragon"));
    state.players[seat.0].hand.push(dragon);
    let cast = Action::CastSpell {
        card: dragon,
        mode: None,
        x: None,
        targets: Vec::new(),
        payment: Vec::new(),
    };
    assert!(
        valid_actions(&state, db).contains(&cast),
        "the Dragon is castable out of the floating mana"
    );
    let state = apply_action(&state, &cast, db);
    let state = apply_action(&state, &Action::PassPriority, db);
    apply_action(&state, &Action::PassPriority, db)
}

// ----- the trigger fires from the graveyard ---------------------------------

#[test]
fn issue_723_a_graveyard_trigger_fires_and_buys_its_card_back() {
    // The whole card: Spit Flame waits in the graveyard, a Dragon arrives, and its
    // controller is asked for {R}. Paying returns the instant to its owner's hand — the
    // same self-referential return an activated graveyard ability makes, reached without
    // anyone activating anything.
    let db = db();
    let mut state = main_phase(&db, 6);
    let flame = bury(&mut state, &db, "spit_flame", PlayerId(0));

    let state = land_a_dragon(&state, &db, PlayerId(0));
    assert_eq!(
        state.stack.len(),
        1,
        "the Dragon's arrival put the graveyard trigger on the stack"
    );

    // Resolving the trigger poses the optional cost rather than applying anything.
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    let pending = sage_engine::pending_player_choice(&state).expect("the `you may pay` question");
    assert_eq!(pending.chooser, PlayerId(0));
    assert!(
        state.players[0].graveyard.iter().any(|c| c.id == flame.id),
        "the card is still in the graveyard while the question is owed"
    );

    let paid = apply_action(&state, &Action::AnswerConfirm { accept: true }, &db);
    assert!(
        paid.players[0].hand.iter().any(|c| c.id == flame.id),
        "paying returns the card to its owner's hand"
    );
    assert!(paid.players[0].graveyard.is_empty());
}

#[test]
fn issue_723_declining_the_cost_leaves_the_card_where_it_was() {
    // The decline path, which is the one that has to leave the graveyard untouched: the
    // trigger resolved, nothing was paid, and the card is exactly where it started —
    // available to trigger again the next time a Dragon arrives.
    let db = db();
    let mut state = main_phase(&db, 6);
    let flame = bury(&mut state, &db, "spit_flame", PlayerId(0));

    let state = land_a_dragon(&state, &db, PlayerId(0));
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    let declined = apply_action(&state, &Action::AnswerConfirm { accept: false }, &db);

    assert!(
        declined.players[0]
            .graveyard
            .iter()
            .any(|c| c.id == flame.id),
        "a decline leaves the card in the graveyard"
    );
    assert!(declined.players[0].hand.iter().all(|c| c.id != flame.id));
}

#[test]
fn issue_723_the_trigger_reads_you_as_the_graveyards_own_seat() {
    // A card in a zone has no controller of its own (CR 108.4), so the "you" of `a Dragon
    // you control` is the player whose graveyard it is. An opponent's Dragon is not one
    // this seat controls and fires nothing.
    let db = db();
    let mut state = main_phase(&db, 6);
    bury(&mut state, &db, "spit_flame", PlayerId(0));

    let theirs = land_a_dragon(&state, &db, PlayerId(1));
    assert!(
        theirs.stack.is_empty(),
        "the opponent's Dragon does not fire a trigger from this seat's graveyard"
    );
}

#[test]
fn issue_723_a_trigger_that_works_on_the_battlefield_does_not_fire_from_a_graveyard() {
    // The gate that keeps the third source list honest. Lathliss watches for exactly the
    // same event, but its trigger functions on the battlefield — a card of it lying in a
    // graveyard is not a permanent and its ability is not watching anything. Without the
    // "where does this ability function" comparison, every dead permanent would keep
    // triggering from the pile.
    let db = db();
    let mut state = main_phase(&db, 6);
    bury(&mut state, &db, "lathliss_dragon_queen", PlayerId(0));

    let after = land_a_dragon(&state, &db, PlayerId(0));
    assert!(
        after.stack.is_empty(),
        "a battlefield trigger does not fire from a graveyard"
    );
    assert_eq!(
        after.battlefield.len(),
        1,
        "and no Dragon token was created"
    );
}

#[test]
fn issue_723_a_graveyard_trigger_does_not_also_fire_from_the_battlefield() {
    // The mirror of the gate above, on the one card that could hit it: a permanent whose
    // trigger returns its own card from a graveyard would otherwise be read off both
    // lists at once. Authored here rather than taken from the catalog because M19 prints
    // the shape only on an instant, which is never on the battlefield to be caught.
    const DEFINITIONS: &str = r#"[
        {"schema_version":1,"functional_id":"test_recursive_watcher",
         "name":"Test Recursive Watcher","types":["creature"],"mana_cost":"",
         "power":1,"toughness":1,
         "abilities":[{"type":"triggered",
                       "event":{"permanent_enters":{"scope":"any_creature","except_this":true}},
                       "effects":[{"kind":"return_self_from_graveyard","destination":"hand"}]}]},
        {"schema_version":1,"functional_id":"test_bear","name":"Test Bear",
         "types":["creature"],"mana_cost":"","power":2,"toughness":2}
    ]"#;
    let db = CardDatabase::from_json(DEFINITIONS).expect("a well-formed test catalog");
    let watcher = cid(&db, "test_recursive_watcher");

    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    let instance = state.new_instance(watcher);
    let id = sage_engine::PermanentId(state.mint_id());
    state.battlefield.push(sage_engine::Permanent {
        id,
        instance: instance.id,
        printed: watcher.into(),
        controller: PlayerId(0),
        ..Default::default()
    });

    let bear = state.new_instance(cid(&db, "test_bear"));
    state.players[0].hand.push(bear);
    let cast = Action::CastSpell {
        card: bear,
        mode: None,
        x: None,
        targets: Vec::new(),
        payment: Vec::new(),
    };
    let state = apply_action(&state, &cast, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    assert!(
        state.stack.is_empty(),
        "the permanent's graveyard trigger does not fire while it is on the battlefield"
    );
}
