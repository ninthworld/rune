//! Hired Blade (M19 #100): flash (CR 702.8), and nothing else.
//!
//! A vanilla creature with one word on it is the cleanest statement the mechanic gets:
//! whatever these tests show is the keyword's doing, because the card has nothing else
//! to attribute it to. The comparison throughout is a creature *without* flash in the
//! same hand at the same moment, which is what makes "the gate lifted" different from
//! "the gate was never there".
//!
//! Every test drives the real [`apply_action`] and the real [`valid_actions`] over the
//! bundled catalog; cards are named by their authored `functional_id` (ADR 0008 §3).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, valid_actions, Action, CardDatabase, CardId, CardInstance, Color, FunctionalId,
    GameState, PlayerId, StackObjectKind, Step,
};

// ----- fixtures -------------------------------------------------------------

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// A two-player game with both pools stocked, so payability never decides a test that
/// is about timing.
fn stocked() -> GameState {
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

fn cast_of(card: CardInstance) -> Action {
    Action::CastSpell {
        card,
        mode: None,
        x: None,
        targets: Vec::new(),
        payment: Vec::new(),
    }
}

fn offered(state: &GameState, db: &CardDatabase, card: CardInstance) -> bool {
    valid_actions(state, db).contains(&cast_of(card))
}

/// Whether the cast **actually happened**: the card reached the stack rather than
/// `apply_action` returning the state it was handed.
fn reaches_the_stack(state: &GameState, db: &CardDatabase, card: CardInstance) -> bool {
    apply_action(state, &cast_of(card), db).stack.iter().any(
        |o| matches!(o.kind, StackObjectKind::Spell { card: on_stack, .. } if on_stack.id == card.id),
    )
}

// ----- the timing permission -------------------------------------------------

#[test]
fn issue_748_hired_blade_is_castable_on_an_opponents_turn() {
    // CR 702.8: flash is permission to cast at instant speed, so the sorcery-speed gate
    // — the active player, a main phase, an empty stack — is only two-thirds satisfied
    // here and that is enough.
    let db = db();
    let mut state = stocked();
    state.active_player = PlayerId(1);
    state.priority = PlayerId(0);
    let blade = to_hand(&mut state, &db, "hired_blade", PlayerId(0));
    let vanilla = to_hand(&mut state, &db, "onakke_ogre", PlayerId(0));

    assert!(
        offered(&state, &db, blade),
        "flash ignores whose turn it is"
    );
    assert!(
        !offered(&state, &db, vanilla),
        "a creature without it is still bound by the gate"
    );
    assert!(reaches_the_stack(&state, &db, blade));
}

#[test]
fn issue_748_hired_blade_is_castable_with_something_on_the_stack() {
    // The other two-thirds: it is player 0's own main phase, but a spell is waiting to
    // resolve. Holding a creature up in response is the whole reason to print flash.
    let db = db();
    let mut state = stocked();
    let blade = to_hand(&mut state, &db, "hired_blade", PlayerId(0));
    let vanilla = to_hand(&mut state, &db, "onakke_ogre", PlayerId(0));

    // The stack is loaded by seat 0 casting an instant of its own, which is the one
    // way to reach "a spell is waiting" without leaving the seat's own main phase.
    let shock = to_hand(&mut state, &db, "shock", PlayerId(0));
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: shock,
            mode: None,
            x: None,
            targets: vec![sage_engine::Target::Player(PlayerId(1))],
            payment: Vec::new(),
        },
        &db,
    );
    assert!(!state.stack.is_empty(), "the stack is loaded");
    assert_eq!(state.priority, PlayerId(0), "and priority came back");

    assert!(
        offered(&state, &db, blade),
        "flash ignores the empty-stack half of the gate too"
    );
    assert!(!offered(&state, &db, vanilla));
    assert!(reaches_the_stack(&state, &db, blade));
}

#[test]
fn issue_748_a_creature_without_flash_is_refused_by_apply_action_itself() {
    // The gate is re-derived rather than remembered. `apply_action` is handed a cast
    // nobody offered — the shape a stale or forged action id takes — and refuses it by
    // asking the timing question again, so an offer made at one moment cannot be
    // redeemed at another. Asserted on both sides of the same board so the refusal is
    // demonstrably about the keyword: the flash creature, submitted identically, is
    // allowed through.
    let db = db();
    let mut state = stocked();
    state.active_player = PlayerId(1);
    state.priority = PlayerId(0);
    let blade = to_hand(&mut state, &db, "hired_blade", PlayerId(0));
    let vanilla = to_hand(&mut state, &db, "onakke_ogre", PlayerId(0));

    assert!(!offered(&state, &db, vanilla), "not offered …");
    let refused = apply_action(&state, &cast_of(vanilla), &db);
    assert_eq!(&refused, &state, "… and not applied either");
    assert!(
        refused.players[0].hand.iter().any(|c| c.id == vanilla.id),
        "the card is still in hand, and no mana was spent"
    );
    assert_eq!(refused.players[0].mana_pool, state.players[0].mana_pool);

    assert!(reaches_the_stack(&state, &db, blade), "flash still passes");
}

#[test]
fn issue_748_flash_changes_only_the_timing() {
    // What the keyword does *not* do is as much of the card as what it does: the
    // creature that arrives is an ordinary 3/2 with no abilities, entering the way any
    // other creature spell's would.
    let db = db();
    let mut state = stocked();
    state.active_player = PlayerId(1);
    state.priority = PlayerId(0);
    let blade = to_hand(&mut state, &db, "hired_blade", PlayerId(0));

    let state = apply_action(&state, &cast_of(blade), &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    let perm = state
        .battlefield
        .iter()
        .find(|p| p.instance == blade.id)
        .expect("the creature resolved onto the battlefield");
    let face = perm.printed.face(&db).expect("a printed face");
    assert_eq!((face.power(), face.toughness()), (Some(3), Some(2)));
    assert!(state.stack.is_empty(), "and nothing is left over");
}
