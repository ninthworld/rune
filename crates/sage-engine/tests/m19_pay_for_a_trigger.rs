//! `You may pay {X}. When you do, …` — a reflexive trigger bought with a payment
//! (CR 603.11, issue #706).
//!
//! The difference from an ordinary `you may` is where the effects happen, and it is
//! visible on both cards here: they go on the **stack** after the payment, so their
//! targets are chosen then (CR 603.11b) and both players get priority before anything
//! happens. Sparktongue Dragon cannot choose what to burn when it is cast — nobody knows
//! yet whether it will be paid for, and a target chosen up front would be one chosen
//! before the decision.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, characteristics, pending_player_choice, pending_trigger_target_choice, Action,
    CardDatabase, CardId, CardInstance, Color, FunctionalId, GameState, Keyword, Permanent,
    PermanentId, PlayerId, Step, Target,
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
    let filler = cid(db, "bogstomper");
    for seat in 0..2 {
        let library: Vec<_> = (0..12).map(|_| state.new_instance(filler)).collect();
        state.players[seat].library = library;
    }
    state
}

fn mana(state: &mut GameState, amount: u8) {
    for color in [
        Color::White,
        Color::Blue,
        Color::Black,
        Color::Red,
        Color::Green,
    ] {
        state.players[0].mana_pool.add(color, amount);
    }
    state.players[0].mana_pool.add_colorless(amount);
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

/// Cast `card` and pass twice so it resolves and its enters trigger goes on the stack.
fn cast(state: &GameState, db: &CardDatabase, card: CardInstance) -> GameState {
    let state = apply_action(
        state,
        &Action::CastSpell {
            card,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        db,
    );
    let state = apply_action(&state, &Action::PassPriority, db);
    let state = apply_action(&state, &Action::PassPriority, db);
    let state = apply_action(&state, &Action::PassPriority, db);
    apply_action(&state, &Action::PassPriority, db)
}

/// Paying buys an ability, and that ability is aimed **after** the payment.
#[test]
fn sparktongue_dragon_aims_after_it_has_been_paid_for() {
    let db = db();
    let mut state = main_phase(&db);
    mana(&mut state, 10);
    let victim = place(&mut state, &db, "onakke_ogre", PlayerId(1));
    let dragon = to_hand(&mut state, &db, "sparktongue_dragon", PlayerId(0));

    let state = cast(&state, &db, dragon);

    // Nothing has been aimed yet: the question is whether to pay at all.
    assert!(
        pending_trigger_target_choice(&state).is_none(),
        "no target is owed before the payment"
    );
    let pending = pending_player_choice(&state).expect("the offer is made");
    assert_eq!(pending.chooser, PlayerId(0));
    let paid = state.players[0].mana_pool.red;

    let state = apply_action(&state, &Action::AnswerConfirm { accept: true }, &db);

    assert_eq!(
        state.players[0].mana_pool.red,
        paid - 1,
        "{{2}}{{R}} charged"
    );
    // *Now* it is on the stack, unaimed, and its controller aims it.
    let ability = pending_trigger_target_choice(&state).expect("the ability owes a target");
    let state = apply_action(
        &state,
        &Action::ChooseTriggerTargets {
            ability,
            targets: vec![Target::Permanent(victim)],
        },
        &db,
    );
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    assert!(
        !state.battlefield.iter().any(|perm| perm.id == victim),
        "a 4/2 does not survive 3 damage"
    );
}

/// Declining buys nothing: no ability, no payment, and the resolution is over.
#[test]
fn sparktongue_dragon_declined_creates_no_ability() {
    let db = db();
    let mut state = main_phase(&db);
    mana(&mut state, 10);
    let victim = place(&mut state, &db, "onakke_ogre", PlayerId(1));
    let dragon = to_hand(&mut state, &db, "sparktongue_dragon", PlayerId(0));
    let state = cast(&state, &db, dragon);
    let paid = state.players[0].mana_pool.red;

    let state = apply_action(&state, &Action::AnswerConfirm { accept: false }, &db);

    assert!(
        pending_trigger_target_choice(&state).is_none(),
        "no ability"
    );
    assert!(state.stack.is_empty());
    assert_eq!(state.players[0].mana_pool.red, paid, "and nothing charged");
    assert!(state.battlefield.iter().any(|perm| perm.id == victim));
}

/// A controller who cannot pay is never asked — there is nothing to decide.
#[test]
fn an_unpayable_offer_is_not_posed() {
    let db = db();
    let mut state = main_phase(&db);
    // Just enough to cast the Dragon, nothing left for its offer.
    state.players[0].mana_pool.add(Color::Red, 2);
    state.players[0].mana_pool.add_colorless(3);
    let dragon = to_hand(&mut state, &db, "sparktongue_dragon", PlayerId(0));

    let state = cast(&state, &db, dragon);

    assert!(
        pending_player_choice(&state).is_none(),
        "no mana to pay with, so no question"
    );
    assert!(state.stack.is_empty());
}

/// Skyrider Patrol buys a two-effect ability, and both effects land on the one creature
/// its single slot named.
#[test]
fn skyrider_patrol_buys_a_counter_and_a_keyword_for_one_creature() {
    let db = db();
    let mut state = main_phase(&db);
    let patrol = place(&mut state, &db, "skyrider_patrol", PlayerId(0));
    let ally = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    // Two lands to pay {G}{U} with. The pool empties between steps, so the mana has to be
    // made *while the offer is owed* (CR 605.3a) — which is the only time it is legal.
    let forest = place(&mut state, &db, "forest", PlayerId(0));
    let island = place(&mut state, &db, "island", PlayerId(0));
    for perm in &mut state.battlefield {
        perm.entered_turn = 0;
    }

    // The step trigger fires as the game *crosses into* begin combat, so the walk has to
    // do the crossing rather than the setup asserting it.
    for _ in 0..8 {
        if pending_player_choice(&state).is_some() {
            break;
        }
        state = apply_action(&state, &Action::PassPriority, &db);
    }
    assert_eq!(state.step, Step::BeginCombat, "the trigger's own step");
    assert!(pending_player_choice(&state).is_some(), "the offer is made");

    for land in [forest, island] {
        state = apply_action(
            &state,
            &Action::ActivateAbility {
                permanent: land,
                index: 0,
                targets: Vec::new(),
                payment: Vec::new(),
            },
            &db,
        );
    }
    let state = apply_action(&state, &Action::AnswerConfirm { accept: true }, &db);

    let ability = pending_trigger_target_choice(&state).expect("the ability owes targets");
    let state = apply_action(
        &state,
        &Action::ChooseTriggerTargets {
            ability,
            targets: vec![Target::Permanent(ally), Target::Permanent(ally)],
        },
        &db,
    );
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    let stats = characteristics(&state, ally, &db);
    assert_eq!(stats.power, Some(5), "a 4/2 with a +1/+1 counter");
    assert!(stats.keywords.contains(&Keyword::Flying), "and flying");
    // "Another" is the source-relative class: the Patrol is not one of its own targets.
    assert_eq!(
        characteristics(&state, patrol, &db).power,
        Some(2),
        "the Patrol gained nothing"
    );
}
