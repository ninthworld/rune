//! Isareth the Awakener (issue #706): an **X paid in the middle of a resolution**, and a
//! replacement on leaving the battlefield.
//!
//! X was only ever *announced* (CR 601.2b), where it rides on the action that puts an
//! object on the stack and is fixed before anything resolves. `You may pay {X}` on a
//! trigger has no announcement to ride, so the amount is a question asked mid-resolution —
//! the tenth shape of question the engine can pose, and the first whose answer is a plain
//! number rather than something about an object.
//!
//! The interesting part is what happens to the number afterwards. `Return target creature
//! card with mana value X` is a sentence about a value nobody had until that moment, and
//! the ability that says it is aimed *later*, when it goes on the stack (CR 603.11b). So
//! the value is **substituted into the effects** as it is paid: by the time anybody
//! chooses a target, the spec names a concrete mana value and every reader downstream —
//! the candidate enumeration, the legality gate, the CR 608.2b re-check — sees an ordinary
//! spec with no X in it.
//!
//! The other half is CR 614.1a. `If that creature would leave the battlefield, exile it
//! instead of putting it anywhere else` is asked at every road **out** of the battlefield,
//! so dying, being bounced, and being tucked all get one answer. It replaces the
//! destination and not the leaving: the permanent still leaves, so everything that watches
//! a departure sees what it always saw.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, pending_player_choice, valid_actions, Action, CardDatabase, CardId, Color,
    CounterKind, FunctionalId, GameState, Permanent, PermanentId, PlayerId, Step, Target,
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
    state
}

/// Put ten of each colour into seat 0's pool.
///
/// Called **after** combat is reached rather than at setup, because a mana pool empties
/// between steps (CR 500.4) — an attack trigger is asked for payment in the declare
/// attackers step, and mana added in the main phase is long gone by then.
fn fill_pool(state: &mut GameState) {
    for color in [
        Color::White,
        Color::Blue,
        Color::Black,
        Color::Red,
        Color::Green,
    ] {
        state.players[0].mana_pool.add(color, 10);
    }
    state.players[0].mana_pool.add_colorless(10);
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

/// Put `slug` into seat 0's graveyard and return its instance id.
fn bury(state: &mut GameState, db: &CardDatabase, slug: &str) -> sage_engine::CardInstanceId {
    let card = state.new_instance(cid(db, slug));
    state.players[0].graveyard.push(card);
    card.id
}

/// Isareth on the battlefield, able to attack.
fn isareth(state: &mut GameState, db: &CardDatabase) -> PermanentId {
    let id = place(state, db, "isareth_the_awakener", PlayerId(0));
    if let Some(perm) = state.battlefield.iter_mut().find(|perm| perm.id == id) {
        perm.entered_turn = 0;
    }
    id
}

/// Walk to declare-attackers, attack, and stop at the first question.
fn attack_with(state: &GameState, db: &CardDatabase, attacker: PermanentId) -> GameState {
    let mut state = state.clone();
    for _ in 0..40 {
        if state.step == Step::DeclareAttackers && !state.attackers_declared {
            break;
        }
        state = apply_action(&state, &Action::PassPriority, db);
    }
    apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: vec![sage_engine::Attack {
                attacker,
                defender: sage_engine::AttackTarget::Player(PlayerId(1)),
            }],
        },
        db,
    )
}

/// Pass priority until the stack empties or somebody is asked something.
fn settle(state: &GameState, db: &CardDatabase) -> GameState {
    let mut state = state.clone();
    for _ in 0..16 {
        if state.stack.is_empty() || pending_player_choice(&state).is_some() {
            break;
        }
        state = apply_action(&state, &Action::PassPriority, db);
    }
    state
}

/// The aiming action currently on offer, if any.
fn aiming(state: &GameState, db: &CardDatabase) -> Option<Action> {
    valid_actions(state, db)
        .into_iter()
        .find(|action| matches!(action, Action::ChooseTriggerTargets { .. }))
}

/// Attack, accept the offer, and pay `x` — leaving the reflexive ability on the stack,
/// waiting to be aimed.
fn attack_and_pay(state: &GameState, db: &CardDatabase, x: u32) -> GameState {
    let attacker = state
        .battlefield
        .iter()
        .find(|perm| perm.printed.card() == Some(cid(db, "isareth_the_awakener")))
        .map(|perm| perm.id)
        .expect("Isareth is on the battlefield");
    let mut state = attack_with(state, db, attacker);
    fill_pool(&mut state);
    let state = settle(&state, db);
    assert!(
        pending_player_choice(&state).is_some(),
        "the offer to pay is posed"
    );
    let state = apply_action(&state, &Action::AnswerConfirm { accept: true }, db);
    assert!(
        matches!(
            pending_player_choice(&state).map(|p| &p.question),
            Some(sage_engine::ChoiceQuestion::Number(_))
        ),
        "accepting asks how much"
    );
    let state = apply_action(&state, &Action::AnswerNumber { value: x }, db);
    settle(&state, db)
}

/// **The crux.** Accepting asks for an amount, and the amount is bounded by what the
/// player could actually pay.
#[test]
fn issue_706_accepting_the_offer_asks_how_much_to_pay() {
    let db = db();
    let mut state = main_phase();
    let attacker = isareth(&mut state, &db);
    bury(&mut state, &db, "bogstomper");

    let mut state = attack_with(&state, &db, attacker);
    fill_pool(&mut state);
    let state = settle(&state, &db);
    let state = apply_action(&state, &Action::AnswerConfirm { accept: true }, &db);

    let (min, max) = sage_engine::number_bounds(&state, &db);
    assert_eq!(min, 0, "paying nothing is always a legal payment of X");
    assert!(max >= 5, "and the ceiling is what the pool could reach");
}

/// Declining pays nothing and makes nothing — no ability is created at all.
#[test]
fn issue_706_declining_the_offer_creates_no_ability() {
    let db = db();
    let mut state = main_phase();
    let attacker = isareth(&mut state, &db);
    bury(&mut state, &db, "bogstomper");

    let mut state = attack_with(&state, &db, attacker);
    fill_pool(&mut state);
    let before = state.players[0].mana_pool.total();
    let state = settle(&state, &db);
    let state = apply_action(&state, &Action::AnswerConfirm { accept: false }, &db);
    let state = settle(&state, &db);

    assert!(
        aiming(&state, &db).is_none(),
        "nothing is waiting to be aimed"
    );
    assert_eq!(
        state.players[0].graveyard.len(),
        1,
        "and the creature is still in the graveyard"
    );
    assert_eq!(
        state.players[0].mana_pool.total(),
        before,
        "and nothing was paid"
    );
}

/// **The substitution.** X is written into the ability the payment bought, so the slot
/// names creature cards of exactly that mana value — and nothing else.
#[test]
fn issue_706_the_paid_x_becomes_the_mana_value_the_slot_names() {
    let db = db();
    let mut state = main_phase();
    isareth(&mut state, &db);
    // Bogstomper costs six; Grasping Scoundrel costs one.
    let big = bury(&mut state, &db, "bogstomper");
    let small = bury(&mut state, &db, "grasping_scoundrel");

    // Pay one: only the one-drop is a legal target.
    let state = attack_and_pay(&state, &db, 1);
    let Some(Action::ChooseTriggerTargets { ability, .. }) = aiming(&state, &db) else {
        panic!("the bought ability is waiting to be aimed");
    };
    let wrong = apply_action(
        &state,
        &Action::ChooseTriggerTargets {
            ability,
            mode: None,
            targets: vec![Target::Card(big)],
        },
        &db,
    );
    assert_eq!(wrong, state, "a six-drop is not mana value one");

    let right = apply_action(
        &state,
        &Action::ChooseTriggerTargets {
            ability,
            mode: None,
            targets: vec![Target::Card(small)],
        },
        &db,
    );
    assert_ne!(right, state, "and the one-drop is");
}

/// What comes back arrives with a corpse counter, and paying really costs the mana.
#[test]
fn issue_706_the_reanimated_creature_arrives_with_a_corpse_counter() {
    let db = db();
    let mut state = main_phase();
    isareth(&mut state, &db);
    let small = bury(&mut state, &db, "grasping_scoundrel");

    let state = attack_and_pay(&state, &db, 1);
    // The pool was filled with sixty and one point of it bought the reanimation.
    assert_eq!(
        state.players[0].mana_pool.total(),
        59,
        "one mana left the pool"
    );
    let Some(Action::ChooseTriggerTargets { ability, .. }) = aiming(&state, &db) else {
        panic!("the bought ability is waiting to be aimed");
    };
    let state = apply_action(
        &state,
        &Action::ChooseTriggerTargets {
            ability,
            mode: None,
            targets: vec![Target::Card(small)],
        },
        &db,
    );
    let state = settle(&state, &db);

    let reanimated = state
        .battlefield
        .iter()
        .find(|perm| perm.instance == small)
        .expect("it came back");
    assert_eq!(
        reanimated.counters.get(&CounterKind::Corpse),
        Some(&1),
        "with a corpse counter on it"
    );
    assert!(
        state.players[0].graveyard.is_empty(),
        "and it left the graveyard"
    );
}

/// **The replacement.** A creature reanimated this way is exiled when it would die —
/// it never reaches a graveyard.
#[test]
fn issue_706_a_reanimated_creature_is_exiled_instead_of_dying() {
    let db = db();
    let mut state = main_phase();
    isareth(&mut state, &db);
    let small = bury(&mut state, &db, "grasping_scoundrel");

    let state = attack_and_pay(&state, &db, 1);
    let Some(Action::ChooseTriggerTargets { ability, .. }) = aiming(&state, &db) else {
        panic!("the bought ability is waiting to be aimed");
    };
    let state = apply_action(
        &state,
        &Action::ChooseTriggerTargets {
            ability,
            mode: None,
            targets: vec![Target::Card(small)],
        },
        &db,
    );
    let mut state = settle(&state, &db);
    let reanimated = state
        .battlefield
        .iter()
        .find(|perm| perm.instance == small)
        .map(|perm| perm.id)
        .expect("it came back");

    // Kill it. The one road it would have taken to a graveyard is redirected.
    state.destroy_permanent(reanimated, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    assert!(
        state.players[0].graveyard.is_empty(),
        "it never reached the graveyard"
    );
    assert!(
        state.players[0].exile.iter().any(|card| card.id == small),
        "it was exiled instead"
    );
}

/// The replacement covers every road out, not just dying: a bounce exiles it too.
#[test]
fn issue_706_the_replacement_covers_every_road_off_the_battlefield() {
    let db = db();
    let mut state = main_phase();
    isareth(&mut state, &db);
    let small = bury(&mut state, &db, "grasping_scoundrel");

    let state = attack_and_pay(&state, &db, 1);
    let Some(Action::ChooseTriggerTargets { ability, .. }) = aiming(&state, &db) else {
        panic!("the bought ability is waiting to be aimed");
    };
    let state = apply_action(
        &state,
        &Action::ChooseTriggerTargets {
            ability,
            mode: None,
            targets: vec![Target::Card(small)],
        },
        &db,
    );
    let mut state = settle(&state, &db);
    let reanimated = state
        .battlefield
        .iter()
        .find(|perm| perm.instance == small)
        .map(|perm| perm.id)
        .expect("it came back");

    // A bounce would put it in a hand. It goes to exile instead.
    state.return_permanent_to_hand(reanimated);
    let state = apply_action(&state, &Action::PassPriority, &db);

    assert!(
        !state.players[0].hand.iter().any(|card| card.id == small),
        "it never reached the hand"
    );
    assert!(
        state.players[0].exile.iter().any(|card| card.id == small),
        "it was exiled instead"
    );
}

/// Paying zero is a legal answer, and it buys a search for a creature card that costs
/// nothing — of which there is usually none, so the ability finds no target and is
/// removed (CR 603.3c).
#[test]
fn issue_706_paying_zero_is_legal_and_names_a_zero_cost_creature() {
    let db = db();
    let mut state = main_phase();
    isareth(&mut state, &db);
    bury(&mut state, &db, "grasping_scoundrel");

    let state = attack_and_pay(&state, &db, 0);

    assert_eq!(
        state.players[0].mana_pool.total(),
        60,
        "paying nothing costs nothing"
    );
    assert!(
        aiming(&state, &db).is_none(),
        "and no creature card has mana value zero, so the ability is removed"
    );
}
