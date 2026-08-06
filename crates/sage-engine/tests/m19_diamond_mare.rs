//! Diamond Mare (M19 #231): a colour named **as the permanent enters** (CR 614.12), kept
//! on the permanent, and read back by the permanent's own trigger for as long as it is on
//! the battlefield.
//!
//! Every test drives the **real** [`apply_action`] pipeline over the bundled catalog. The
//! thing under test is not that the definition parses — it is that the choice happens at
//! the right moment, that the answer sticks, and that a later ability sees it. Cards are
//! named by their authored `functional_id`, never by an interned handle (ADR 0008 §3).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, pending_player_choice, valid_actions, Action, CardDatabase, CardId, CardInstance,
    Color, FunctionalId, GameState, PermanentId, PlayerId, Step,
};

// ----- fixtures -------------------------------------------------------------

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// A two-player game parked at player 0's precombat main with both pools stocked, so
/// payability never decides a test that is about a choice.
fn main_phase() -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    for player in &mut state.players {
        for color in Color::ALL {
            player.mana_pool.add(color, 10);
        }
        player.mana_pool.add_colorless(10);
    }
    state
}

fn to_hand(state: &mut GameState, db: &CardDatabase, slug: &str, seat: PlayerId) -> CardInstance {
    let instance = state.new_instance(cid(db, slug));
    state.players[seat.0].hand.push(instance);
    instance
}

/// Cast `slug` from `seat`'s hand and pass it down to resolution. The state comes back
/// wherever resolution left it — which for a card that chooses as it enters is *before*
/// the permanent exists.
fn cast(state: &GameState, db: &CardDatabase, slug: &str, seat: PlayerId) -> GameState {
    let mut state = state.clone();
    state.priority = seat;
    state.consecutive_passes = 0;
    let instance = to_hand(&mut state, db, slug, seat);
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: instance,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        db,
    );
    assert!(
        state.stack.iter().any(|obj| obj.controller == seat),
        "{slug} did not reach the stack"
    );
    let state = apply_action(&state, &Action::PassPriority, db);
    apply_action(&state, &Action::PassPriority, db)
}

/// Cast Diamond Mare under `seat` and answer the entry choice with `color`, returning the
/// resulting state and the permanent that arrived.
fn mare(
    state: &GameState,
    db: &CardDatabase,
    seat: PlayerId,
    color: Color,
) -> (GameState, PermanentId) {
    let before = cast(state, db, "diamond_mare", seat);
    let after = apply_action(&before, &Action::AnswerColor { color }, db);
    let id = after
        .battlefield
        .iter()
        .find(|perm| !before.battlefield.iter().any(|old| old.id == perm.id))
        .expect("the Mare arrived once its colour was named")
        .id;
    (after, id)
}

fn chosen(state: &GameState, id: PermanentId) -> Option<Color> {
    state
        .battlefield
        .iter()
        .find(|perm| perm.id == id)
        .expect("the permanent is on the battlefield")
        .chosen_color
}

fn life(state: &GameState, seat: PlayerId) -> i32 {
    state.players[seat.0].life
}

// ----- the choice is part of entering ---------------------------------------

/// CR 614.12: the colour is named *as* the permanent enters, so there is no instant at
/// which Diamond Mare is on the battlefield without one. The spell has left the stack and
/// the card is in no zone at all — exactly where a spell's card sits while a
/// mid-resolution choice is owed — and the only thing anyone may do is answer.
#[test]
fn the_colour_is_named_before_the_permanent_exists() {
    let db = db();
    let state = cast(&main_phase(), &db, "diamond_mare", PlayerId(0));

    assert!(
        state.battlefield.is_empty(),
        "the permanent must not arrive before its colour is named"
    );
    assert!(state.stack.is_empty(), "the spell has finished resolving");
    assert!(
        state.players.iter().all(|player| {
            player.hand.is_empty() && player.graveyard.is_empty() && player.exile.is_empty()
        }),
        "the card waits in no zone, as a suspended spell's card does"
    );

    let pending = pending_player_choice(&state).expect("a colour is owed");
    assert_eq!(pending.chooser, PlayerId(0), "its controller answers");
    assert_eq!(
        state.priority,
        PlayerId(0),
        "priority goes to the chooser and nobody else acts"
    );
    assert!(
        pending.resume.is_none(),
        "nothing was suspended: the entry is the last step of the resolution, not one \
         of its effects"
    );
}

/// The never-stall rule of ADR 0013 in its simplest form: a colour question always has
/// five legal answers (CR 105.1), so it is always posed and always answerable.
#[test]
fn all_five_colours_are_offered_and_only_the_chooser_may_act() {
    let db = db();
    let state = cast(&main_phase(), &db, "diamond_mare", PlayerId(0));

    let offered = valid_actions(&state, &db);
    assert!(
        offered
            .iter()
            .all(|action| matches!(action, Action::AnswerColor { .. } | Action::Concede)),
        "nothing else is legal while a choice is owed: {offered:?}"
    );
    // The offer is the bare question, and every one of the five answers it accepts is
    // accepted for real — no colour is a special case and none can stall the game.
    for color in Color::ALL {
        let answered = apply_action(&state, &Action::AnswerColor { color }, &db);
        assert_ne!(&answered, &state, "{color:?} must be a legal answer");
        assert_eq!(answered.battlefield.len(), 1);
        assert_eq!(answered.battlefield[0].chosen_color, Some(color));
    }
}

/// Answering completes the entry that was waiting: one permanent arrives, carrying the
/// colour, and priority goes back to the seat that had it.
#[test]
fn answering_puts_the_permanent_onto_the_battlefield_with_its_colour() {
    let db = db();
    let (state, mare) = mare(&main_phase(), &db, PlayerId(0), Color::Red);

    assert_eq!(state.battlefield.len(), 1);
    assert_eq!(chosen(&state, mare), Some(Color::Red));
    assert!(pending_player_choice(&state).is_none());
    assert!(
        state.interrupted_priority.is_none(),
        "the interrupted holder was restored"
    );
}

/// Two copies choose independently, and a copy that leaves and comes back is a new
/// object that chooses again (CR 400.7) — nothing about the first colour survives.
#[test]
fn each_permanent_records_its_own_colour_and_a_returning_card_chooses_again() {
    let db = db();
    let (state, first) = mare(&main_phase(), &db, PlayerId(0), Color::Red);
    let (state, second) = mare(&state, &db, PlayerId(0), Color::Green);

    assert_eq!(chosen(&state, first), Some(Color::Red));
    assert_eq!(chosen(&state, second), Some(Color::Green));

    // Send the first one home and cast it again: a fresh permanent, freshly asked.
    let mut state = state;
    let instance = state
        .battlefield
        .iter()
        .find(|perm| perm.id == first)
        .expect("still there")
        .instance;
    state.battlefield.retain(|perm| perm.id != first);
    state.players[0].hand.push(CardInstance {
        id: instance,
        card: cid(&db, "diamond_mare"),
    });
    let recast = apply_action(
        &state,
        &Action::CastSpell {
            card: CardInstance {
                id: instance,
                card: cid(&db, "diamond_mare"),
            },
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    let recast = apply_action(&recast, &Action::PassPriority, &db);
    let recast = apply_action(&recast, &Action::PassPriority, &db);
    assert!(
        pending_player_choice(&recast).is_some(),
        "the returning card is a new object and is asked again"
    );
    let recast = apply_action(
        &recast,
        &Action::AnswerColor {
            color: Color::White,
        },
        &db,
    );
    let reborn = recast
        .battlefield
        .iter()
        .find(|perm| perm.instance == instance)
        .expect("back on the battlefield");
    assert_eq!(reborn.chosen_color, Some(Color::White));
    assert_ne!(reborn.id, first, "a fresh PermanentId on every entry");
}

// ----- reading the answer back ----------------------------------------------

/// The recorded colour is what the cast trigger watches: a red spell gains its
/// controller a life, and a spell of any other colour gains nothing.
#[test]
fn the_trigger_fires_for_the_chosen_colour_and_for_no_other() {
    let db = db();
    let (state, _) = mare(&main_phase(), &db, PlayerId(0), Color::Red);
    let opening = life(&state, PlayerId(0));

    // Shock is red — the chosen colour.
    let mut red = state.clone();
    let shock = to_hand(&mut red, &db, "shock", PlayerId(0));
    let red = apply_action(
        &red,
        &Action::CastSpell {
            card: shock,
            mode: None,
            x: None,
            targets: vec![sage_engine::Target::Player(PlayerId(1))],
            payment: Vec::new(),
        },
        &db,
    );
    let red = apply_action(&red, &Action::PassPriority, &db);
    let red = apply_action(&red, &Action::PassPriority, &db);
    assert_eq!(
        life(&red, PlayerId(0)),
        opening + 1,
        "casting a spell of the chosen colour gains 1 life"
    );

    // Divination is blue — not the chosen colour.
    let mut blue = state.clone();
    let divination = to_hand(&mut blue, &db, "divination", PlayerId(0));
    let blue = apply_action(
        &blue,
        &Action::CastSpell {
            card: divination,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    let blue = apply_action(&blue, &Action::PassPriority, &db);
    let blue = apply_action(&blue, &Action::PassPriority, &db);
    assert_eq!(
        life(&blue, PlayerId(0)),
        opening,
        "a spell of another colour is not of the chosen one"
    );
}

/// The same printed card watches a different class on two boards — which is the whole
/// point of recording the answer rather than printing it.
#[test]
fn the_same_card_watches_whatever_each_copy_chose() {
    let db = db();
    let (state, _) = mare(&main_phase(), &db, PlayerId(0), Color::Blue);
    let opening = life(&state, PlayerId(0));

    let mut state = state;
    let divination = to_hand(&mut state, &db, "divination", PlayerId(0));
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: divination,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    assert_eq!(
        life(&state, PlayerId(0)),
        opening + 1,
        "a Mare that named blue gains life from a blue spell"
    );
}

/// "You cast" is the controller's own casting (CR 601), so an opponent's spell of the
/// chosen colour does nothing — and a colourless spell is of no colour at all (CR 105.2),
/// so it never matches whatever was named.
#[test]
fn an_opponents_spell_and_a_colourless_spell_both_fire_nothing() {
    let db = db();
    let (state, _) = mare(&main_phase(), &db, PlayerId(0), Color::Red);
    let opening = life(&state, PlayerId(0));

    // Player 1 casts a red spell. Diamond Mare belongs to player 0.
    let mut theirs = state.clone();
    theirs.priority = PlayerId(1);
    theirs.consecutive_passes = 0;
    let shock = to_hand(&mut theirs, &db, "shock", PlayerId(1));
    let theirs = apply_action(
        &theirs,
        &Action::CastSpell {
            card: shock,
            mode: None,
            x: None,
            targets: vec![sage_engine::Target::Player(PlayerId(0))],
            payment: Vec::new(),
        },
        &db,
    );
    assert_eq!(
        life(&theirs, PlayerId(0)),
        opening,
        "the trigger watches its own controller's casts"
    );

    // A colourless artifact spell matches no chosen colour.
    let mut colourless = state.clone();
    let manalith = to_hand(&mut colourless, &db, "manalith", PlayerId(0));
    let colourless = apply_action(
        &colourless,
        &Action::CastSpell {
            card: manalith,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    assert_eq!(
        life(&colourless, PlayerId(0)),
        opening,
        "a colourless spell is of no colour"
    );
}

// ----- the seam is unchanged for every other card ---------------------------

/// A card that names nothing still enters in one action, with no question posed and no
/// colour recorded — the deferral is opt-in, declared by the card.
#[test]
fn a_card_that_names_no_colour_enters_as_it_always_did() {
    let db = db();
    let state = cast(&main_phase(), &db, "manalith", PlayerId(0));

    assert!(pending_player_choice(&state).is_none());
    assert_eq!(state.battlefield.len(), 1, "it arrived straight away");
    assert_eq!(state.battlefield[0].chosen_color, None);
}
