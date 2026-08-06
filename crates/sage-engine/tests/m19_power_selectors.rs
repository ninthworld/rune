//! The M19 cards whose text asks a numeric question about a creature's power.
//!
//! Three selectors learned a power threshold in one change — the permanent count an
//! intervening if reads, the observed-permanent selector an enters-the-battlefield
//! watcher takes, and the blocking restriction beside the colour one — and this file is
//! the evidence that each does what the printed card says.
//!
//! Every test drives the **real** [`apply_action`] pipeline over the bundled catalog:
//! a definition that parses is not evidence of anything. Cards are named by their
//! authored `functional_id`, never by an interned handle (ADR 0008 §3).
//!
//! The pair of assertions that matters most is the one each card gets about **computed**
//! power. A power bound is the one selector field read through
//! [`characteristics`] rather than off the printed face, because power is what the
//! implemented layers actually change: a creature pumped past the bound has really
//! escaped it, and one shrunk into it has really fallen in. Reading the printed face
//! would answer both wrongly, and a test that only ever used vanilla bodies would never
//! notice.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, blocker_candidates_for, characteristics, valid_actions, Action, Attack,
    AttackTarget, Block, CardDatabase, CardId, Color, FunctionalId, GameState, Permanent,
    PermanentId, PlayerId, Step, Target,
};

// ----- fixtures -------------------------------------------------------------

/// Enough actions to walk a turn or two; a settle that has not arrived by then is a
/// hang, and failing beats spinning.
const SETTLE_LIMIT: usize = 200;

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// A two-player game at player 0's precombat main, both pools stocked so payability
/// never decides a test that is about a selector, and both libraries stocked so a draw
/// has somewhere to draw from.
fn main_phase(db: &CardDatabase) -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
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
    stock_libraries(&mut state, db);
    state
}

/// Ten Forests in each library, so nothing in these tests decks out and every draw has
/// a card to take.
fn stock_libraries(state: &mut GameState, db: &CardDatabase) {
    let forest = cid(db, "forest");
    for seat in 0..2 {
        state.players[seat].library = (0..10).map(|_| state.new_instance(forest)).collect();
    }
}

/// Put a permanent of `slug` onto the battlefield under `controller`, untapped and free
/// of summoning sickness, and return its battlefield identity.
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

/// Cast `slug` from `seat`'s hand and let both players pass, so it resolves. Goes
/// through the ordinary cast gate, so a spell the pipeline would refuse fails here
/// rather than silently doing nothing.
fn cast_by(
    state: &GameState,
    db: &CardDatabase,
    slug: &str,
    seat: PlayerId,
    targets: Vec<Target>,
) -> GameState {
    let mut state = state.clone();
    state.priority = seat;
    state.consecutive_passes = 0;
    let instance = state.new_instance(cid(db, slug));
    state.players[seat.0].hand.push(instance);
    let state = apply_action(
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
    let state = apply_action(&state, &Action::PassPriority, db);
    let state = apply_action(&state, &Action::PassPriority, db);
    // A creature arriving may put a watcher's trigger on the stack behind it; resolving
    // that is part of "the spell resolved" from the test's point of view.
    settle_stack(&state, db)
}

/// Pass priority until the stack has emptied or a mid-resolution question is owed —
/// the two states in which nothing more will happen without a real decision.
fn settle_stack(state: &GameState, db: &CardDatabase) -> GameState {
    let mut state = state.clone();
    for _ in 0..SETTLE_LIMIT {
        if state.stack.is_empty() || sage_engine::pending_player_choice(&state).is_some() {
            return state;
        }
        state = apply_action(&state, &Action::PassPriority, db);
    }
    panic!("the stack never settled");
}

/// Activate ability `index` of `permanent` with `targets` and let it resolve.
fn activate(
    state: &GameState,
    db: &CardDatabase,
    permanent: PermanentId,
    index: usize,
    targets: Vec<Target>,
) -> GameState {
    let action = Action::ActivateAbility {
        permanent,
        index,
        targets,
        payment: Vec::new(),
    };
    let state = apply_action(state, &action, db);
    let state = apply_action(&state, &Action::PassPriority, db);
    apply_action(&state, &Action::PassPriority, db)
}

/// Walk the game forward one legal action at a time until `done` holds — passing where
/// passing is offered, and otherwise taking the first non-concede action there is (the
/// empty combat declaration a declare step owes).
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

/// The computed power of `id`, which is what every bound in this file is judged against.
fn power(state: &GameState, db: &CardDatabase, id: PermanentId) -> i32 {
    characteristics(state, id, db).power.expect("a creature")
}

/// The number of cards in `seat`'s hand.
fn hand(state: &GameState, seat: PlayerId) -> usize {
    state.players[seat.0].hand.len()
}

/// Whether the game is currently owing a yes-or-no.
///
/// The offer list is deliberately not the test here: `valid_actions` advertises only the
/// decline (an acceptance is validated in `apply_action`, which re-derives payability
/// rather than trusting an offer), so "the trigger fired" is read off the pending
/// question instead.
fn confirm_owed(state: &GameState) -> bool {
    sage_engine::pending_player_choice(state)
        .is_some_and(|pending| pending.question.confirm().is_some())
}

// ----- Mentor of the Meek: a bound on an observed permanent -----------------

#[test]
fn mentor_of_the_meek_notices_a_small_arrival_and_ignores_a_large_one() {
    // "Whenever another creature you control with power 2 or less enters." The bound is
    // the whole card: without it the watcher fires on every arrival.
    let db = db();
    let mut state = main_phase(&db);
    place(&mut state, &db, "mentor_of_the_meek", PlayerId(0));
    let before = hand(&state, PlayerId(0));

    // A 2/2 is inside the bound: the trigger is owed, and its optional effect asks.
    let small = cast_by(&state, &db, "sun_sentinel", PlayerId(0), Vec::new());
    assert!(
        confirm_owed(&small),
        "a power-2 arrival owes its controller the may-pay question"
    );
    assert!(sage_engine::confirm_is_payable(&small));
    let drawn = apply_action(&small, &Action::AnswerConfirm { accept: true }, &db);
    assert_eq!(
        hand(&drawn, PlayerId(0)),
        before + 1,
        "paying the {{1}} draws the card"
    );

    // A 3/3 is outside it: no trigger, so nothing is ever asked.
    let large = cast_by(&state, &db, "centaur_courser", PlayerId(0), Vec::new());
    assert!(
        !confirm_owed(&large),
        "a power-3 arrival is not observed at all"
    );
    assert!(large.stack.is_empty(), "and puts no trigger on the stack");
}

#[test]
fn mentor_of_the_meek_reads_computed_power_not_the_printed_face() {
    // The assertion this whole change turns on. Greenwood Sentinel is a printed 2/2 —
    // inside the bound — and an Elf, so an Elvish Clancaller already on the battlefield
    // makes it a 3/3 the instant it arrives (CR 613.1f, layer 7c). A printed reading
    // would trigger; the computed one must not.
    let db = db();
    let mut state = main_phase(&db);
    place(&mut state, &db, "mentor_of_the_meek", PlayerId(0));

    let bare = cast_by(&state, &db, "greenwood_sentinel", PlayerId(0), Vec::new());
    let sentinel = bare
        .battlefield
        .iter()
        .find(|perm| perm.printed.face(&db).map(|face| face.name()) == Some("Greenwood Sentinel"))
        .expect("the Sentinel resolved")
        .id;
    assert_eq!(power(&bare, &db, sentinel), 2);
    assert!(confirm_owed(&bare), "a 2/2 Elf is inside the bound");

    let mut lorded = state.clone();
    place(&mut lorded, &db, "elvish_clancaller", PlayerId(0));
    let lorded = cast_by(&lorded, &db, "greenwood_sentinel", PlayerId(0), Vec::new());
    let sentinel = lorded
        .battlefield
        .iter()
        .find(|perm| perm.printed.face(&db).map(|face| face.name()) == Some("Greenwood Sentinel"))
        .expect("the Sentinel resolved")
        .id;
    assert_eq!(power(&lorded, &db, sentinel), 3, "the lord is already on");
    assert!(
        !confirm_owed(&lorded),
        "the same printed 2/2 entered as a 3/3 and is outside the bound"
    );
    assert!(lorded.stack.is_empty());
}

#[test]
fn mentor_of_the_meek_says_another_by_permanent_not_by_card() {
    // `except_this` compares the permanent, so one Mentor notices a second Mentor
    // arriving — both are printed 2/2s, so the bound lets it through and only the
    // "another" is under test.
    let db = db();
    let mut state = main_phase(&db);
    place(&mut state, &db, "mentor_of_the_meek", PlayerId(0));

    let second = cast_by(&state, &db, "mentor_of_the_meek", PlayerId(0), Vec::new());
    assert!(
        confirm_owed(&second),
        "the first Mentor observed the second arriving"
    );
}

// ----- Colossal Majesty: a bound on a counted class -------------------------

#[test]
fn colossal_majesty_draws_only_beside_a_creature_of_power_four() {
    // An upkeep trigger gated on an intervening if that counts by power. The walk goes
    // through the real turn structure, so the trigger fires where the card says it does.
    let db = db();
    let mut state = main_phase(&db);
    state.step = Step::Untap;
    place(&mut state, &db, "colossal_majesty", PlayerId(0));

    let mut small = state.clone();
    place(&mut small, &db, "centaur_courser", PlayerId(0));
    let before = hand(&small, PlayerId(0));
    let small = settle_until(&small, &db, |s| s.step == Step::PrecombatMain);
    assert_eq!(
        hand(&small, PlayerId(0)),
        before,
        "a 3/3 fails the condition, so the upkeep passes without a card"
    );

    let mut large = state.clone();
    place(&mut large, &db, "colossal_dreadmaw", PlayerId(0));
    let before = hand(&large, PlayerId(0));
    let large = settle_until(&large, &db, |s| s.step == Step::PrecombatMain);
    assert_eq!(
        hand(&large, PlayerId(0)),
        before + 1,
        "the 6/6 satisfies it, so the upkeep trigger draws"
    );
}

#[test]
fn colossal_majesty_counts_computed_power() {
    // The counted class reads the same computed power the watcher does. Ghirapur Guide
    // is a printed 3/2 — one short — and an Elf, so a Clancaller lifts it over the bar
    // without any card in the count changing its printed face.
    let db = db();
    let mut state = main_phase(&db);
    state.step = Step::Untap;
    place(&mut state, &db, "colossal_majesty", PlayerId(0));
    let guide = place(&mut state, &db, "ghirapur_guide", PlayerId(0));

    let bare = settle_until(&state, &db, |s| s.step == Step::Draw);
    assert_eq!(power(&bare, &db, guide), 3);
    let bare_hand = hand(&bare, PlayerId(0));

    let mut lorded = state.clone();
    place(&mut lorded, &db, "elvish_clancaller", PlayerId(0));
    let lorded = settle_until(&lorded, &db, |s| s.step == Step::Draw);
    assert_eq!(power(&lorded, &db, guide), 4, "the lord pushed it to 4/3");
    assert_eq!(
        hand(&lorded, PlayerId(0)),
        bare_hand + 1,
        "and that is the whole difference between drawing and not"
    );
}

// ----- Ghirapur Guide: a bound on a blocking restriction --------------------

/// Declare `attacker` as an attacker on player 1 and walk to the declare-blockers step,
/// where player 1 owes the declaration.
fn attack_with(state: &GameState, db: &CardDatabase, attacker: PermanentId) -> GameState {
    let state = settle_until(state, db, |s| s.step == Step::DeclareAttackers);
    let state = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: vec![Attack {
                attacker,
                defender: AttackTarget::Player(PlayerId(1)),
            }],
        },
        db,
    );
    settle_until(&state, db, |s| s.step == Step::DeclareBlockers)
}

/// Whether assigning `blocker` to `attacker` is accepted by the pipeline — submitted as
/// a real declaration, so an illegal one is a no-op rather than an error.
fn block_is_legal(
    state: &GameState,
    db: &CardDatabase,
    blocker: PermanentId,
    attacker: PermanentId,
) -> bool {
    let action = Action::DeclareBlockers {
        blocks: vec![Block { blocker, attacker }],
    };
    &apply_action(state, &action, db) != state
}

#[test]
fn ghirapur_guide_stops_small_blockers_and_leaves_the_rest() {
    // The numeric sibling of Vine Mare's colour evasion, enforced at the same pairwise
    // gate: a 2/2 may not block the creature it names, and a 3/3 beside it may.
    let db = db();
    let mut state = main_phase(&db);
    let guide = place(&mut state, &db, "ghirapur_guide", PlayerId(0));
    let attacker = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let small = place(&mut state, &db, "sun_sentinel", PlayerId(1));
    let large = place(&mut state, &db, "centaur_courser", PlayerId(1));

    let state = activate(&state, &db, guide, 0, vec![Target::Permanent(attacker)]);
    let state = attack_with(&state, &db, attacker);

    assert!(!block_is_legal(&state, &db, small, attacker));
    assert!(
        block_is_legal(&state, &db, large, attacker),
        "a power-3 blocker is outside the bound and may block"
    );
    assert!(
        blocker_candidates_for(&state, PlayerId(1), &db).contains(&small),
        "the 2/2 is still a blocker candidate — the restriction is pairwise, about \
         this attacker, not about the blocker"
    );
}

#[test]
fn ghirapur_guide_reads_the_blockers_computed_power() {
    // The other side of the same reading: a 2/2 pumped out of range really has escaped,
    // which is only true if the gate asks the layers rather than the printed face.
    let db = db();
    let mut state = main_phase(&db);
    let guide = place(&mut state, &db, "ghirapur_guide", PlayerId(0));
    let attacker = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let blocker = place(&mut state, &db, "sun_sentinel", PlayerId(1));

    let state = activate(&state, &db, guide, 0, vec![Target::Permanent(attacker)]);
    let pumped = cast_by(
        &state,
        &db,
        "titanic_growth",
        PlayerId(1),
        vec![Target::Permanent(blocker)],
    );
    assert_eq!(power(&pumped, &db, blocker), 6);

    let blocked = attack_with(&pumped, &db, attacker);
    assert!(
        block_is_legal(&blocked, &db, blocker, attacker),
        "the pump carried it past the bound"
    );
}

#[test]
fn ghirapur_guides_restriction_ends_with_the_turn() {
    // Imposed until end of turn like every other `restrict`, so the bound is gone by the
    // next combat rather than becoming a permanent property of the creature.
    let db = db();
    let mut state = main_phase(&db);
    let guide = place(&mut state, &db, "ghirapur_guide", PlayerId(0));
    let attacker = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let small = place(&mut state, &db, "sun_sentinel", PlayerId(1));

    let state = activate(&state, &db, guide, 0, vec![Target::Permanent(attacker)]);
    let next_turn = settle_until(&state, &db, |s| {
        s.turn == 3 && s.step == Step::PrecombatMain
    });
    let combat = attack_with(&next_turn, &db, attacker);
    assert!(
        block_is_legal(&combat, &db, small, attacker),
        "the imposition expired in cleanup (CR 514.2)"
    );
}
