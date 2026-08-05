//! Sleep, and the skipped untap step it leaves behind.
//!
//! The card is two sentences and each is a separate claim about the game: every creature
//! one seat controls is tapped *now*, and those same creatures sit out that seat's *next*
//! untap step. The second is the one worth testing hard, because it is the only piece of
//! state in the engine that survives a turn boundary on purpose.
//!
//! Every test drives the **real** [`apply_action`] pipeline over the bundled catalog, and
//! walks the real turn structure rather than poking `Step` — a skipped untap step that is
//! only skipped when a test sets the step by hand would prove nothing. Cards are named by
//! their authored `functional_id`, never by an interned handle (ADR 0008 §3).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, valid_actions, Action, CardDatabase, CardId, Color, FunctionalId, GameState,
    Permanent, PermanentId, PlayerId, Step, Target,
};

// ----- fixtures -------------------------------------------------------------

/// Enough actions to walk several whole turns; a settle that has not arrived by then is a
/// hang, and failing beats spinning.
const SETTLE_LIMIT: usize = 400;

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// A two-player game at player 0's precombat main, both pools stocked so payability never
/// decides a test about an effect, and both libraries stocked so a multi-turn walk never
/// trips the CR 704.5c decking loss.
fn main_phase(db: &CardDatabase) -> GameState {
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
            state.players[seat].mana_pool.add(color, 10);
        }
        state.players[seat].mana_pool.add_colorless(10);
        state.players[seat].library = (0..20).map(|_| state.new_instance(forest)).collect();
    }
    state
}

/// Put a permanent of `slug` onto the battlefield under `controller`, untapped and free of
/// summoning sickness, and return its battlefield identity.
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

/// Cast `slug` from player 0's hand with `targets` and let it resolve. Goes through the
/// ordinary cast gate, so a spell `valid_actions` would not offer fails here rather than
/// silently doing nothing.
fn cast(state: &GameState, db: &CardDatabase, slug: &str, targets: Vec<Target>) -> GameState {
    let mut state = state.clone();
    let instance = state.new_instance(cid(db, slug));
    state.players[0].hand.push(instance);
    assert!(
        valid_actions(&state, db).contains(&Action::CastSpell {
            card: instance,
            targets: Vec::new(),
            payment: Vec::new(),
        }),
        "{slug} was not offered as a castable spell"
    );
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: instance,
            targets,
            payment: Vec::new(),
        },
        db,
    );
    let state = apply_action(&state, &Action::PassPriority, db);
    apply_action(&state, &Action::PassPriority, db)
}

/// Walk the game forward one legal action at a time until `done` holds — passing where
/// passing is offered, and otherwise taking the first non-concede action there is.
fn settle_until(
    state: &GameState,
    db: &CardDatabase,
    done: impl Fn(&GameState) -> bool,
) -> GameState {
    let mut state = state.clone();
    for _ in 0..SETTLE_LIMIT {
        if done(&state) {
            return state;
        }
        let offered = valid_actions(&state, db);
        let action = if offered.contains(&Action::PassPriority) {
            Action::PassPriority
        } else {
            offered
                .into_iter()
                .find(|action| action != &Action::Concede)
                .expect("some action is always available")
        };
        state = apply_action(&state, &action, db);
    }
    panic!("the game never reached the state under test");
}

/// Whether `id` is on the battlefield and tapped.
fn tapped(state: &GameState, id: PermanentId) -> bool {
    state
        .battlefield
        .iter()
        .any(|perm| perm.id == id && perm.tapped)
}

/// Whether `id` is still flagged to sit out an untap step.
fn flagged(state: &GameState, id: PermanentId) -> bool {
    state
        .battlefield
        .iter()
        .any(|perm| perm.id == id && perm.skips_untap)
}

/// Walk to the start of `turn`'s precombat main — past that turn's untap step, which is the
/// one every test here is about.
fn through_untap_of(state: &GameState, db: &CardDatabase, turn: u32) -> GameState {
    settle_until(state, db, |s| {
        s.turn == turn && s.step == Step::PrecombatMain
    })
}

// ----- the card -------------------------------------------------------------

#[test]
fn sleep_taps_the_targeted_seats_creatures_and_leaves_its_own_alone() {
    // "Tap all creatures target player controls." Whose creatures is the whole of the
    // targeting, so a card that tapped the caster's board too would pass a test that only
    // looked at the victim.
    let db = db();
    let mut state = main_phase(&db);
    let theirs = place(&mut state, &db, "centaur_courser", PlayerId(1));
    let also_theirs = place(&mut state, &db, "sun_sentinel", PlayerId(1));
    let mine = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let their_land = place(&mut state, &db, "forest", PlayerId(1));

    let after = cast(&state, &db, "sleep", vec![Target::Player(PlayerId(1))]);

    assert!(tapped(&after, theirs));
    assert!(tapped(&after, also_theirs));
    assert!(!tapped(&after, mine), "the caster's own board is untouched");
    assert!(
        !tapped(&after, their_land),
        "a land is not a creature — the card says creatures"
    );
}

#[test]
fn sleep_holds_them_down_through_the_next_untap_step_and_no_further() {
    // The second sentence, and the reason this card needed engine state at all: the
    // creatures stay tapped through their controller's *next* untap step, then untap
    // normally at the one after it.
    let db = db();
    let mut state = main_phase(&db);
    let victim = place(&mut state, &db, "centaur_courser", PlayerId(1));

    let after = cast(&state, &db, "sleep", vec![Target::Player(PlayerId(1))]);
    assert!(tapped(&after, victim));
    assert!(flagged(&after, victim));

    // Seat 1's next untap step is turn 2's. It is skipped, and the flag is spent there.
    let turn_two = through_untap_of(&after, &db, 2);
    assert!(
        tapped(&turn_two, victim),
        "the creature sat out its controller's untap step"
    );
    assert!(
        !flagged(&turn_two, victim),
        "and the flag was spent doing it — it names one untap step, not every one"
    );

    // Seat 1's turn after that untaps normally. Turn 3 is seat 0's, so the walk has to
    // reach turn 4 to test seat 1's next untap step rather than the wrong seat's.
    let turn_four = through_untap_of(&turn_two, &db, 4);
    assert!(
        !tapped(&turn_four, victim),
        "the following untap step is an ordinary one"
    );
}

#[test]
fn only_the_flagged_seats_own_untap_step_spends_the_flag() {
    // The flag names *its controller's* next untap step, not simply the next one the game
    // reaches. Aimed at the caster, the intervening turn belongs to the other seat: its
    // untap step must leave both the tap and the flag alone, and only turn 3 — seat 0's
    // own — may spend them. Getting this wrong would make the card resolve a turn early
    // against every seat but the active one.
    let db = db();
    let mut state = main_phase(&db);
    let mine = place(&mut state, &db, "centaur_courser", PlayerId(0));

    let after = cast(&state, &db, "sleep", vec![Target::Player(PlayerId(0))]);
    assert!(tapped(&after, mine), "a caster may aim this at themselves");

    // Turn 2 is seat 1's. Their untap step is not the one this flag named.
    let turn_two = through_untap_of(&after, &db, 2);
    assert!(tapped(&turn_two, mine));
    assert!(
        flagged(&turn_two, mine),
        "another seat's untap step neither untapped it nor spent its flag"
    );

    // Turn 3 is seat 0's, and is the step the card named.
    let turn_three = through_untap_of(&turn_two, &db, 3);
    assert!(tapped(&turn_three, mine), "skipped, as the card says");
    assert!(!flagged(&turn_three, mine), "and the flag is spent there");
}

#[test]
fn sleep_taps_the_board_as_it_stands_on_resolution() {
    // CR 611.2c: the set is enumerated on resolution. A creature that arrives afterwards
    // was never named, so it is neither tapped nor flagged — the card is a tempo swing,
    // not a lasting curse on a seat.
    let db = db();
    let mut state = main_phase(&db);
    let present = place(&mut state, &db, "centaur_courser", PlayerId(1));

    let mut after = cast(&state, &db, "sleep", vec![Target::Player(PlayerId(1))]);
    let latecomer = place(&mut after, &db, "sun_sentinel", PlayerId(1));

    assert!(tapped(&after, present));
    assert!(!tapped(&after, latecomer));
    assert!(!flagged(&after, latecomer));

    let turn_two = through_untap_of(&after, &db, 2);
    assert!(tapped(&turn_two, present), "the one it named stays down");
    assert!(
        !tapped(&turn_two, latecomer),
        "and the one it did not is untouched by the skip"
    );
}

#[test]
fn a_creature_already_tapped_is_flagged_anyway() {
    // The card is not a blank against a board that already attacked: an untapped creature
    // and a tapped one are both named, so both sit out the untap step. Tapping and
    // flagging are two things the effect does, and only one of them is a no-op here.
    let db = db();
    let mut state = main_phase(&db);
    let spent = place(&mut state, &db, "centaur_courser", PlayerId(1));
    state
        .battlefield
        .iter_mut()
        .find(|perm| perm.id == spent)
        .expect("the creature")
        .tapped = true;

    let after = cast(&state, &db, "sleep", vec![Target::Player(PlayerId(1))]);
    assert!(flagged(&after, spent));

    let turn_two = through_untap_of(&after, &db, 2);
    assert!(tapped(&turn_two, spent), "still down through their untap");
    assert!(!flagged(&turn_two, spent));
}

#[test]
fn the_flag_is_spent_even_by_an_untap_step_that_had_nothing_to_do() {
    // The one way a flag could leak: a permanent that is somehow untapped when its
    // controller's untap step arrives. The step still spends the flag, because the step a
    // card named has happened whether or not it changed anything. A flag that survived
    // here would silently skip every untap step for the rest of the game.
    let db = db();
    let mut state = main_phase(&db);
    let victim = place(&mut state, &db, "centaur_courser", PlayerId(1));

    let mut after = cast(&state, &db, "sleep", vec![Target::Player(PlayerId(1))]);
    after
        .battlefield
        .iter_mut()
        .find(|perm| perm.id == victim)
        .expect("the creature")
        .tapped = false;

    let turn_two = through_untap_of(&after, &db, 2);
    assert!(!flagged(&turn_two, victim), "the step spent it regardless");

    // And the proof that it was really spent: tapped now, it untaps on schedule.
    let mut tapped_again = turn_two;
    tapped_again
        .battlefield
        .iter_mut()
        .find(|perm| perm.id == victim)
        .expect("the creature")
        .tapped = true;
    let turn_four = through_untap_of(&tapped_again, &db, 4);
    assert!(!tapped(&turn_four, victim));
}

#[test]
fn sleep_declares_exactly_one_player_slot() {
    // The `player_ref` is what makes this a targeting spell (CR 115.1), and a mass class
    // read relative to the controller would have made it target nothing. One slot, aimable
    // at either seat, and an announcement that names no seat is not a legal Sleep.
    let db = db();
    let mut state = main_phase(&db);
    let instance = state.new_instance(cid(&db, "sleep"));
    state.players[0].hand.push(instance);

    let requirements = sage_engine::target_requirements(
        &state,
        &db,
        &Action::CastSpell {
            card: instance,
            targets: Vec::new(),
            payment: Vec::new(),
        },
    );
    assert_eq!(requirements.len(), 1, "one slot, and only one");
    let candidates = &requirements[0].candidates;
    assert!(candidates.contains(&Target::Player(PlayerId(0))));
    assert!(candidates.contains(&Target::Player(PlayerId(1))));

    // A submitted announcement with the slot unfilled is refused rather than resolving as
    // some default seat's problem.
    let unaimed = apply_action(
        &state,
        &Action::CastSpell {
            card: instance,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    assert_eq!(unaimed, state, "an unaimed Sleep is not a cast");
}
