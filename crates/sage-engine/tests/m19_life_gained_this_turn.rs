//! `if you gained life this turn` — the intervening if whose window is the turn
//! (issue #727, CR 608.2).
//!
//! Every test here drives the **real** [`apply_action`] pipeline across whole turns,
//! because the two properties that matter are properties of the walk rather than of any
//! one state. The condition is a sum of the turn's *life-gain events*, so gaining life
//! and losing it again in the same turn leaves every total exactly where it started and
//! still answers yes — and the events stop counting at the turn boundary, which only a
//! walk that crosses one can show.
//!
//! Regal Bloodlord asks the plain question at its controller's end step; Resplendent
//! Angel asks it with a threshold of five at every end step. Sovereign's Bite is how a
//! seat takes life away from itself: it is printed as "target player loses 3 life and
//! you gain 3 life", and nothing stops the caster from being the target.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, Action, CardDatabase, CardId, CardInstance, Color, FunctionalId, GameState,
    Permanent, PermanentId, PlayerId, Step, Target,
};

/// Enough actions to walk several whole turns; a walk that has not reached its goal by
/// then is a hang, and failing beats spinning.
const ACTION_CAP: usize = 400;

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// A two-player game at the very start of turn 1, both libraries stocked with a card
/// nobody can afford so the walk never trips the CR 704.5c decking loss and never finds
/// a spell worth casting on its own.
fn fresh_game(db: &CardDatabase) -> GameState {
    let mut state = GameState::new_two_player();
    for seat in 0..2 {
        let library: Vec<_> = (0..8)
            .map(|_| state.new_instance(cid(db, "colossal_dreadmaw")))
            .collect();
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

/// Fill `seat`'s pool with enough of every colour to cast anything these tests cast.
/// The pool empties at each step boundary (CR 500.4), so a spell has to be cast in the
/// step the mana was added in — which is what every caller does.
fn with_mana(state: &mut GameState, seat: PlayerId) {
    for color in [
        Color::White,
        Color::Blue,
        Color::Black,
        Color::Red,
        Color::Green,
    ] {
        state.players[seat.0].mana_pool.add(color, 10);
    }
    state.players[seat.0].mana_pool.add_colorless(10);
}

/// Pass priority until seat 0 holds it, so the caster below is the seat whose hand the
/// spell comes out of ([`Action::CastSpell`] is taken by the priority holder).
fn priority_to_seat_0(state: &GameState, db: &CardDatabase) -> GameState {
    let mut state = state.clone();
    for _ in 0..4 {
        if state.priority == PlayerId(0) {
            return state;
        }
        state = apply_action(&state, &Action::PassPriority, db);
    }
    panic!("priority never reached seat 0");
}

/// Cast `slug` from seat 0's hand at `targets` and resolve it, leaving the stack empty.
fn cast_and_resolve(
    state: &GameState,
    db: &CardDatabase,
    slug: &str,
    targets: Vec<Target>,
) -> GameState {
    let mut state = priority_to_seat_0(state, db);
    with_mana(&mut state, PlayerId(0));
    let instance = to_hand(&mut state, db, slug, PlayerId(0));
    let mut state = apply_action(
        &state,
        &Action::CastSpell {
            card: instance,
            mode: None,
            x: None,
            targets,
            payment: Vec::new(),
        },
        db,
    );
    for _ in 0..2 {
        state = apply_action(&state, &Action::PassPriority, db);
    }
    assert!(state.stack.is_empty(), "`{slug}` did not resolve");
    state
}

/// The one action the walk ever takes: submit the empty combat declaration when one is
/// owed, and otherwise pass. No test here attacks or blocks, so an empty declaration is
/// the whole of combat and the turn structure is free to move on.
fn walk_action(state: &GameState) -> Action {
    match state.step {
        Step::DeclareAttackers if !state.attackers_declared => Action::DeclareAttackers {
            attackers: Vec::new(),
        },
        Step::DeclareBlockers if !state.blockers_declared => {
            Action::DeclareBlockers { blocks: Vec::new() }
        }
        _ => Action::PassPriority,
    }
}

/// Walk the game forward under [`walk_action`] until `done` holds.
fn walk_until(
    state: &GameState,
    db: &CardDatabase,
    done: impl Fn(&GameState) -> bool,
) -> GameState {
    let mut state = state.clone();
    for _ in 0..ACTION_CAP {
        if done(&state) {
            return state;
        }
        let next = apply_action(&state, &walk_action(&state), db);
        assert_ne!(next, state, "the walk made no progress");
        state = next;
    }
    panic!("the walk ran past its cap without reaching the goal");
}

/// The permanents on the battlefield that are tokens named `name` — a token has no
/// card behind it (ADR 0015), which is what tells a created Bat from a cast creature.
fn tokens(state: &GameState, db: &CardDatabase, name: &str) -> usize {
    state
        .battlefield
        .iter()
        .filter(|perm| {
            perm.printed.card().is_none()
                && perm
                    .printed
                    .face(db)
                    .is_some_and(|face| face.name() == name)
        })
        .count()
}

/// A turn with no life gained in it makes no Bat: the trigger fires either way, and the
/// condition is what decides whether anything happens.
#[test]
fn regal_bloodlord_makes_no_bat_on_a_turn_that_gained_nothing() {
    let db = db();
    let mut state = fresh_game(&db);
    place(&mut state, &db, "regal_bloodlord", PlayerId(0));

    let state = walk_until(&state, &db, |s| {
        s.turn == 2 && s.step == Step::PrecombatMain
    });
    assert_eq!(
        tokens(&state, &db, "Bat"),
        0,
        "seat 0's end step came and went with no life gained"
    );
}

/// The plain question, answered by a life gain earlier in the same turn.
#[test]
fn regal_bloodlord_makes_a_bat_when_the_turn_gained_life() {
    let db = db();
    let mut state = fresh_game(&db);
    place(&mut state, &db, "regal_bloodlord", PlayerId(0));

    let state = walk_until(&state, &db, |s| {
        s.turn == 1 && s.step == Step::PrecombatMain
    });
    let opening = state.players[0].life;
    let state = cast_and_resolve(&state, &db, "revitalize", Vec::new());
    assert_eq!(state.players[0].life, opening + 3);

    let state = walk_until(&state, &db, |s| {
        s.turn == 2 && s.step == Step::PrecombatMain
    });
    assert_eq!(
        tokens(&state, &db, "Bat"),
        1,
        "the end step's condition saw the turn's life gain"
    );
}

/// The headline property (issue #727): the condition reads the turn's life-gain
/// **events**, so a gain that was given straight back still counts. Sovereign's Bite
/// aimed at its own caster takes three life and hands three back, leaving the life
/// total exactly where the turn found it — and a reading taken from life totals, of any
/// kind, would see a turn in which nothing happened.
#[test]
fn issue_727_life_gained_and_lost_again_in_one_turn_still_counts() {
    let db = db();
    let mut state = fresh_game(&db);
    place(&mut state, &db, "regal_bloodlord", PlayerId(0));

    let state = walk_until(&state, &db, |s| {
        s.turn == 1 && s.step == Step::PrecombatMain
    });
    let opening = state.players[0].life;
    let state = cast_and_resolve(
        &state,
        &db,
        "sovereign_s_bite",
        vec![Target::Player(PlayerId(0))],
    );
    assert_eq!(
        state.players[0].life, opening,
        "three life lost and three gained leaves the total untouched"
    );

    let state = walk_until(&state, &db, |s| {
        s.turn == 2 && s.step == Step::PrecombatMain
    });
    assert_eq!(
        state.players[0].life, opening,
        "and it is still untouched at the end step that asks"
    );
    assert_eq!(
        tokens(&state, &db, "Bat"),
        1,
        "the turn gained three life, whatever the total says"
    );
}

/// The window closes at the turn boundary. Seat 0 gains life on turn 1 and nothing
/// afterwards; its turn-1 end step makes a Bat and its turn-3 one does not, from a
/// board that is otherwise identical.
#[test]
fn issue_727_the_life_a_turn_gained_does_not_carry_into_the_next() {
    let db = db();
    let mut state = fresh_game(&db);
    place(&mut state, &db, "regal_bloodlord", PlayerId(0));

    let state = walk_until(&state, &db, |s| {
        s.turn == 1 && s.step == Step::PrecombatMain
    });
    let state = cast_and_resolve(&state, &db, "revitalize", Vec::new());
    let raised = state.players[0].life;

    let state = walk_until(&state, &db, |s| {
        s.turn == 2 && s.step == Step::PrecombatMain
    });
    assert_eq!(tokens(&state, &db, "Bat"), 1, "turn 1 gained life");

    // Turn 3 is seat 0's next end step. The life total is still the raised one — which
    // is exactly why a total cannot answer this question — and no life was *gained*
    // during turn 3.
    let state = walk_until(&state, &db, |s| {
        s.turn == 4 && s.step == Step::PrecombatMain
    });
    assert_eq!(
        state.players[0].life, raised,
        "the life stayed gained; the turn that gained it did not"
    );
    assert_eq!(
        tokens(&state, &db, "Bat"),
        1,
        "turn 3 gained nothing, so it made nothing"
    );
}

/// A threshold of five is not met by three, and the amount is the turn's **total**: two
/// separate gains of three make six, and six is five or more.
#[test]
fn resplendent_angel_wants_five_life_across_the_whole_turn() {
    let db = db();
    let mut state = fresh_game(&db);
    place(&mut state, &db, "resplendent_angel", PlayerId(0));

    let state = walk_until(&state, &db, |s| {
        s.turn == 1 && s.step == Step::PrecombatMain
    });
    let state = cast_and_resolve(&state, &db, "revitalize", Vec::new());
    let state = walk_until(&state, &db, |s| {
        s.turn == 2 && s.step == Step::PrecombatMain
    });
    assert_eq!(
        tokens(&state, &db, "Angel"),
        0,
        "three life is not five or more"
    );

    // Turn 3 is seat 0's again: two Revitalizes, six life, one Angel.
    let state = walk_until(&state, &db, |s| {
        s.turn == 3 && s.step == Step::PrecombatMain
    });
    let state = cast_and_resolve(&state, &db, "revitalize", Vec::new());
    let state = cast_and_resolve(&state, &db, "revitalize", Vec::new());
    let state = walk_until(&state, &db, |s| {
        s.turn == 4 && s.step == Step::PrecombatMain
    });
    assert_eq!(
        tokens(&state, &db, "Angel"),
        1,
        "two gains of three are six gained this turn"
    );
}

/// Resplendent Angel's trigger is scoped to **each** end step, so life gained on an
/// opponent's turn is asked about on that turn. The condition is still its
/// controller's: what seat 1 gains is nothing to seat 0's Angel.
#[test]
fn resplendent_angel_asks_at_an_opponents_end_step_too() {
    let db = db();
    let mut state = fresh_game(&db);
    place(&mut state, &db, "resplendent_angel", PlayerId(0));

    // Seat 1's turn. Seat 0 gains six life during it, at instant speed.
    let state = walk_until(&state, &db, |s| {
        s.turn == 2 && s.step == Step::PrecombatMain
    });
    let state = cast_and_resolve(&state, &db, "revitalize", Vec::new());
    let state = cast_and_resolve(&state, &db, "revitalize", Vec::new());

    let state = walk_until(&state, &db, |s| {
        s.turn == 3 && s.step == Step::PrecombatMain
    });
    assert_eq!(
        tokens(&state, &db, "Angel"),
        1,
        "an each-scope end step asks on an opponent's turn as well"
    );
}
