//! Cost modification (CR 601.2f), and the M19 card that needs it.
//!
//! Goreclaw, Terror of Qal Sisma makes a class of creature spells cheaper, and every
//! test here drives the **real** [`apply_action`] pipeline over the bundled catalog: a
//! definition that parses is not evidence that a cost was changed anywhere it matters.
//!
//! What "anywhere it matters" means is the whole point of the file. A cost is read in
//! three places, and a modification that reached only one of them would be a bug a
//! player meets immediately:
//!
//! - the **offer**: [`valid_actions`] advertises a cast only when a payment for it can be
//!   assembled, so a reduction that skipped it would leave the discount unusable;
//! - the **payment**: [`apply_action`] charges for the cast, so a reduction that skipped
//!   it would advertise a cast the game then refuses as a no-op — and an automated player
//!   would take it, be refused, and take it again for ever;
//! - the **idle predicate**: [`priority_has_no_meaningful_action`] floats what a board
//!   could still tap for and asks whether any real action remains, so a reduction that
//!   skipped it would auto-pass a seat that had a play (ADR 0010).
//!
//! Each of the three gets its own test, and the last is the one that could not be caught
//! by playing carelessly: the seat simply never gets asked.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, characteristics, parse_mana_cost, priority_has_no_meaningful_action,
    total_cast_cost, valid_actions, Action, Attack, AttackTarget, CardDatabase, CardId,
    CardInstance, Color, CostPayment, FunctionalId, GameState, Keyword, ManaSource, Permanent,
    PermanentId, PlayerId, Step,
};

// ----- fixtures -------------------------------------------------------------

/// Enough actions to walk a turn; a settle that has not arrived by then is a hang, and
/// failing beats spinning.
const SETTLE_LIMIT: usize = 200;

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// A two-player game at player 0's precombat main with **empty pools**. Every test here
/// is about what a cast costs, so nothing may be stocked that would pay for it by
/// accident.
fn main_phase(db: &CardDatabase) -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    let forest = cid(db, "forest");
    for seat in 0..2 {
        state.players[seat].library = (0..10).map(|_| state.new_instance(forest)).collect();
    }
    state
}

/// Put a permanent of `slug` onto the battlefield under `controller`, untapped and free
/// of summoning sickness.
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

/// Put `slug` into player 0's hand and return the instance.
fn hold(state: &mut GameState, db: &CardDatabase, slug: &str) -> CardInstance {
    let instance = state.new_instance(cid(db, slug));
    state.players[0].hand.push(instance);
    instance
}

/// Whether player 0 is offered a cast of `card` right now.
fn offers_cast(state: &GameState, db: &CardDatabase, card: CardInstance) -> bool {
    valid_actions(state, db).contains(&Action::CastSpell {
        card,
        mode: None,
        x: None,
        targets: Vec::new(),
        payment: Vec::new(),
    })
}

/// Cast `card` out of the floating pool and report the resulting state.
fn cast(state: &GameState, db: &CardDatabase, card: CardInstance) -> GameState {
    apply_action(
        state,
        &Action::CastSpell {
            card,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        db,
    )
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

/// The computed power of `id`.
fn power(state: &GameState, db: &CardDatabase, id: PermanentId) -> i32 {
    characteristics(state, id, db).power.expect("a creature")
}

/// Whether `id` currently has trample (CR 613.1f, computed).
fn tramples(state: &GameState, db: &CardDatabase, id: PermanentId) -> bool {
    characteristics(state, id, db)
        .keywords
        .contains(&Keyword::Trample)
}

// ----- the offer, the payment, and the idle predicate -----------------------

/// **The reduction is the only reason the spell is castable.** Thornhide Wolves is
/// `{4}{G}` and the seat is holding three mana; with Goreclaw on the battlefield it
/// costs `{2}{G}`, which those three cover exactly.
///
/// Both directions are asserted on both the offer *and* the charge, because an offer
/// that widened without the charge following would be a cast a player is invited to
/// take and then silently refused.
#[test]
fn cr_601_2f_a_reduction_makes_a_spell_castable_that_was_not() {
    let db = db();

    // Without the reducer: three mana against a five-mana spell.
    let mut bare = main_phase(&db);
    bare.players[0].mana_pool.add(Color::Green, 3);
    let wolves = hold(&mut bare, &db, "thornhide_wolves");
    assert!(
        !offers_cast(&bare, &db, wolves),
        "{{4}}{{G}} is not castable off three mana"
    );
    assert!(
        cast(&bare, &db, wolves).stack.is_empty(),
        "and submitting it anyway is a no-op"
    );

    // The same board, with Goreclaw.
    let mut reduced = bare.clone();
    place(
        &mut reduced,
        &db,
        "goreclaw_terror_of_qal_sisma",
        PlayerId(0),
    );
    assert_eq!(
        total_cast_cost(&reduced, &db, wolves),
        Some(parse_mana_cost("{2}{G}")),
        "{{2}} comes off the generic half"
    );
    assert!(
        offers_cast(&reduced, &db, wolves),
        "the discount is advertised, not just honoured"
    );

    let after = cast(&reduced, &db, wolves);
    assert_eq!(after.stack.len(), 1, "the spell is on the stack");
    assert_eq!(
        after.players[0].mana_pool.total(),
        0,
        "and three mana paid for it — no more and no less"
    );
}

/// **A spell under the threshold is not cheaper.** Centaur Courser is a `{2}{G}` 3/3, so
/// Goreclaw's class does not reach it: the cost is unchanged and three mana is exactly
/// what it always was.
///
/// The companion assertion to the one above, and the one that would catch a selector
/// that matched every creature spell — a bug the affordability test alone would call a
/// pass.
#[test]
fn cr_601_2f_a_spell_below_the_power_threshold_keeps_its_printed_cost() {
    let db = db();
    let mut state = main_phase(&db);
    place(&mut state, &db, "goreclaw_terror_of_qal_sisma", PlayerId(0));
    let courser = hold(&mut state, &db, "centaur_courser");

    assert_eq!(
        total_cast_cost(&state, &db, courser),
        Some(parse_mana_cost("{2}{G}")),
        "a 3/3 is outside the class, so nothing comes off"
    );

    // Two mana is one short of the printed cost, and no discount arrives to close it.
    state.players[0].mana_pool.add(Color::Green, 2);
    assert!(!offers_cast(&state, &db, courser));
    assert!(cast(&state, &db, courser).stack.is_empty());
}

/// **A generic reduction never touches a coloured pip, and never runs a cost below
/// `{0}`** (CR 601.2f).
///
/// Gigantosaurus is a 10/10 for `{G}{G}{G}{G}{G}` — squarely inside Goreclaw's class,
/// and with no generic half for the `{2}` to come off. The rule's floor is what decides
/// what happens: the mana component "can't be reduced to less than {0}", so the cost is
/// exactly what it was printed as. Five green still means five green, and a seat holding
/// four cannot cast it.
#[test]
fn cr_601_2f_a_reduction_stops_at_zero_and_leaves_coloured_requirements_alone() {
    let db = db();
    let mut state = main_phase(&db);
    place(&mut state, &db, "goreclaw_terror_of_qal_sisma", PlayerId(0));
    let saurus = hold(&mut state, &db, "gigantosaurus");

    let cost = total_cast_cost(&state, &db, saurus).expect("a bundled card");
    assert_eq!(
        cost.generic, 0,
        "the generic half floors at zero, never below"
    );
    assert_eq!(
        cost,
        parse_mana_cost("{G}{G}{G}{G}{G}"),
        "every pip survives"
    );

    // Four green is one short, and the reduction cannot make up the difference: it has
    // nothing generic left to take off and may not take a pip.
    state.players[0].mana_pool.add(Color::Green, 4);
    assert!(
        !offers_cast(&state, &db, saurus),
        "a reduced cost is not a discounted colour requirement"
    );
    assert!(cast(&state, &db, saurus).stack.is_empty());

    state.players[0].mana_pool.add(Color::Green, 1);
    assert!(offers_cast(&state, &db, saurus));
    assert_eq!(cast(&state, &db, saurus).stack.len(), 1);
}

/// **The seat is not judged idle while it holds a play the reduction unlocked**
/// (ADR 0010, the acceptance criterion of issue #735).
///
/// This is the call site a cost modification is easiest to forget, and the one whose
/// failure a player never sees as a refusal: the room auto-passes on this predicate, so
/// a seat whose only play is a discounted spell would simply never be asked.
///
/// The board is two floating green and one untapped Forest — three mana, which is
/// exactly `{2}{G}` and short of `{4}{G}`. So the same board answers the predicate
/// differently with the reducer on it, and the play the "not idle" answer protects is
/// one the seat can really make: the test finishes by making it, tapping the Forest as
/// part of the cast.
#[test]
fn issue_735_a_seat_holding_a_discounted_play_is_not_idle() {
    let db = db();
    let mut bare = main_phase(&db);
    bare.players[0].mana_pool.add(Color::Green, 2);
    let forest = place(&mut bare, &db, "forest", PlayerId(0));
    let wolves = hold(&mut bare, &db, "thornhide_wolves");
    assert!(
        priority_has_no_meaningful_action(&bare, &db),
        "three mana against a five-mana spell is a pass, a land to tap, and nothing else"
    );

    let mut reduced = bare.clone();
    place(
        &mut reduced,
        &db,
        "goreclaw_terror_of_qal_sisma",
        PlayerId(0),
    );
    assert!(
        !priority_has_no_meaningful_action(&reduced, &db),
        "with the discount those same three mana are a real play, and the seat must \
         never be auto-passed past it"
    );

    // ...and it is a play in the strong sense: the cast completes, tapping the Forest
    // for the third mana as part of the same action.
    let after = apply_action(
        &reduced,
        &Action::CastSpell {
            card: wolves,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: vec![CostPayment::Mana(ManaSource {
                permanent: forest,
                index: 0,
            })],
        },
        &db,
    );
    assert_eq!(after.stack.len(), 1, "the discounted spell is on the stack");
    assert!(
        after.battlefield.iter().any(|p| p.id == forest && p.tapped),
        "and the Forest paid for it"
    );
}

/// **The reducer's presence is the whole of its duration.** It is derived on every read,
/// so a Goreclaw that has left the battlefield takes its discount with it — with nothing
/// stored anywhere to prune.
#[test]
fn cr_601_2f_a_reducer_that_leaves_stops_reducing() {
    let db = db();
    let mut state = main_phase(&db);
    let goreclaw = place(&mut state, &db, "goreclaw_terror_of_qal_sisma", PlayerId(0));
    let wolves = hold(&mut state, &db, "thornhide_wolves");
    assert_eq!(
        total_cast_cost(&state, &db, wolves),
        Some(parse_mana_cost("{2}{G}"))
    );

    state.battlefield.retain(|perm| perm.id != goreclaw);
    assert_eq!(
        total_cast_cost(&state, &db, wolves),
        Some(parse_mana_cost("{4}{G}")),
        "the printed cost is back the instant its source is gone"
    );
}

/// **An opponent's spells are not cheaper.** The ability says "you cast", and the class
/// is read relative to the reducer's controller — so the same board that discounts
/// player 0's Wolves charges player 1 full price for theirs.
#[test]
fn cr_601_2f_a_reducer_reaches_only_its_own_controllers_casts() {
    let db = db();
    let mut state = main_phase(&db);
    place(&mut state, &db, "goreclaw_terror_of_qal_sisma", PlayerId(0));
    let theirs = state.new_instance(cid(&db, "thornhide_wolves"));
    state.players[1].hand.push(theirs);

    state.priority = PlayerId(1);
    assert_eq!(
        total_cast_cost(&state, &db, theirs),
        Some(parse_mana_cost("{4}{G}")),
        "an opponent pays what the card says"
    );
}

// ----- the attack trigger ---------------------------------------------------

/// **Goreclaw's attack trigger reaches exactly the creatures at power 4 or greater.**
///
/// Two effects over one class: `+1/+1` and trample until end of turn. The board carries
/// a creature on each side of the threshold, so the test says what the class *excludes*
/// as well as what it includes — a mass effect that quietly hit everything would pass a
/// test that only looked at the big creatures.
///
/// The bound is read through the **computed** power, which is why the pumped creatures
/// are checked after the trigger rather than only before it: Goreclaw is a 4/4 that its
/// own trigger takes to 5/5, and a 3/3 stays a 3/3 because nothing ever reached it.
#[test]
fn issue_735_goreclaw_pumps_and_tramples_only_the_power_four_creatures() {
    let db = db();
    let mut state = main_phase(&db);
    let goreclaw = place(&mut state, &db, "goreclaw_terror_of_qal_sisma", PlayerId(0));
    let wolves = place(&mut state, &db, "thornhide_wolves", PlayerId(0));
    let courser = place(&mut state, &db, "centaur_courser", PlayerId(0));

    assert_eq!(power(&state, &db, goreclaw), 4);
    assert_eq!(power(&state, &db, wolves), 4);
    assert_eq!(power(&state, &db, courser), 3);
    for id in [goreclaw, wolves, courser] {
        assert!(!tramples(&state, &db, id), "nothing tramples before combat");
    }

    let state = settle_until(&state, &db, |s| s.step == Step::DeclareAttackers);
    let state = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: vec![Attack {
                attacker: goreclaw,
                defender: AttackTarget::Player(PlayerId(1)),
            }],
        },
        &db,
    );
    // The trigger goes on the stack behind the declaration; resolving it is part of
    // "Goreclaw attacked" from the card's point of view.
    let state = settle_until(&state, &db, |s| s.stack.is_empty());

    assert_eq!(
        power(&state, &db, goreclaw),
        5,
        "the attacker pumped itself"
    );
    assert_eq!(power(&state, &db, wolves), 5, "and the other 4-power body");
    assert_eq!(
        power(&state, &db, courser),
        3,
        "the 3/3 is outside the class and untouched"
    );
    assert!(tramples(&state, &db, goreclaw));
    assert!(
        tramples(&state, &db, wolves),
        "a creature that never attacked is still in the class"
    );
    assert!(!tramples(&state, &db, courser), "and the 3/3 gains nothing");
}

/// **A creature the pump lifts past the threshold was already in the class.** The two
/// mass effects enumerate their class independently, so a body sitting exactly on the
/// bound must be caught by both — the `+1/+1` must not push it out of the grant that
/// follows.
///
/// Bristling Boar is a 4/3: in the class at 4, and a 5/3 by the time trample is handed
/// out. It ends the trigger with both halves, which is the whole assertion.
#[test]
fn issue_735_a_creature_exactly_on_the_threshold_gets_both_halves() {
    let db = db();
    let mut state = main_phase(&db);
    let goreclaw = place(&mut state, &db, "goreclaw_terror_of_qal_sisma", PlayerId(0));
    let boar = place(&mut state, &db, "bristling_boar", PlayerId(0));
    assert_eq!(power(&state, &db, boar), 4, "exactly on the bound");

    let state = settle_until(&state, &db, |s| s.step == Step::DeclareAttackers);
    let state = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: vec![Attack {
                attacker: goreclaw,
                defender: AttackTarget::Player(PlayerId(1)),
            }],
        },
        &db,
    );
    let state = settle_until(&state, &db, |s| s.stack.is_empty());

    assert_eq!(power(&state, &db, boar), 5);
    assert!(tramples(&state, &db, boar));
}
