//! Mistcaller (M19 #62): flash (CR 702.8), and a **one-shot replacement effect**
//! (CR 614.1b) that exiles the next nontoken creature to enter the battlefield this turn
//! without being cast.
//!
//! Every test drives the **real** [`apply_action`] pipeline over the bundled catalog.
//! What is under test is not that the definition parses — it is that the card can be held
//! up like an instant, that sacrificing it arms a replacement, that the replacement
//! catches a reanimation and misses a cast, and that it is spent by the first entry it
//! catches. Cards are named by their authored `functional_id`, never by an interned
//! handle (ADR 0008 §3).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    abilities_of, apply_action, valid_actions, Action, CardDatabase, CardId, CardInstance, Color,
    FunctionalId, GameState, Permanent, PermanentId, PlayerId, Printed, Step,
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
/// payability never decides a test that is about a replacement.
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

fn to_battlefield(
    state: &mut GameState,
    db: &CardDatabase,
    slug: &str,
    seat: PlayerId,
) -> PermanentId {
    let instance = state.new_instance(cid(db, slug));
    let id = PermanentId(state.mint_id());
    state.battlefield.push(Permanent {
        id,
        instance: instance.id,
        printed: Printed::Card(instance.card),
        controller: seat,
        entered_turn: 0,
        ..Default::default()
    });
    id
}

/// Sacrifice `seat`'s Mistcaller and let the ability resolve, arming its replacement.
fn arm(state: &GameState, db: &CardDatabase, seat: PlayerId) -> GameState {
    let mut state = state.clone();
    let mistcaller = to_battlefield(&mut state, db, "mistcaller", seat);
    state.priority = seat;
    state.consecutive_passes = 0;
    let state = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: mistcaller,
            index: 0,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        db,
    );
    let state = apply_action(&state, &Action::PassPriority, db);
    let state = apply_action(&state, &Action::PassPriority, db);
    assert_eq!(state.replacements.len(), 1, "the replacement is armed");
    state
}

/// Bring a Reassembling Skeleton back out of `seat`'s graveyard — a nontoken creature
/// entering the battlefield **without being cast**.
fn reanimate_skeleton(
    state: &GameState,
    db: &CardDatabase,
    seat: PlayerId,
) -> (GameState, CardInstance) {
    let mut state = state.clone();
    let card = state.new_instance(cid(db, "reassembling_skeleton"));
    state.players[seat.0].graveyard.push(card);
    state.priority = seat;
    state.consecutive_passes = 0;
    let index = abilities_of(db, card.card)
        .iter()
        .position(sage_engine::is_graveyard_ability)
        .expect("the skeleton's graveyard ability");
    let state = apply_action(
        &state,
        &Action::ActivateAbilityFromGraveyard {
            card,
            index,
            targets: Vec::new(),
        },
        db,
    );
    let state = apply_action(&state, &Action::PassPriority, db);
    (apply_action(&state, &Action::PassPriority, db), card)
}

// ----- flash ----------------------------------------------------------------

#[test]
fn issue_731_mistcaller_is_castable_at_instant_speed_on_an_opponents_turn() {
    // CR 702.8: flash lifts the sorcery-speed gate and nothing else. The comparison is a
    // creature without it, in the same hand at the same moment.
    let db = db();
    let mut state = main_phase();
    // Player 1's turn, player 0 holding priority: sorcery speed is unavailable to them.
    state.active_player = PlayerId(1);
    state.priority = PlayerId(0);
    let mistcaller = state.new_instance(cid(&db, "mistcaller"));
    let vanilla = state.new_instance(cid(&db, "tolarian_scholar"));
    state.players[0].hand.push(mistcaller);
    state.players[0].hand.push(vanilla);

    let offers = valid_actions(&state, &db);
    let castable = |card: CardInstance| {
        offers.contains(&Action::CastSpell {
            card,
            targets: Vec::new(),
            payment: Vec::new(),
        })
    };
    assert!(castable(mistcaller), "flash ignores the sorcery-speed gate");
    assert!(
        !castable(vanilla),
        "a creature without flash is still bound by it"
    );
}

// ----- the replacement ------------------------------------------------------

#[test]
fn issue_731_mistcaller_exiles_a_creature_that_enters_without_being_cast() {
    // The card as printed: a nontoken creature returning from a graveyard would enter,
    // and is exiled instead. Nothing enters, so there is no permanent for anything to
    // trigger off.
    let db = db();
    let armed = arm(&main_phase(), &db, PlayerId(1));
    let (after, skeleton) = reanimate_skeleton(&armed, &db, PlayerId(0));

    assert!(
        !after
            .battlefield
            .iter()
            .any(|perm| perm.instance == skeleton.id),
        "the entry was replaced, so nothing entered"
    );
    assert!(
        after.players[0].exile.iter().any(|c| c.id == skeleton.id),
        "it was exiled instead"
    );
    assert!(
        !after.players[0]
            .graveyard
            .iter()
            .any(|c| c.id == skeleton.id),
        "and it did not stay in the graveyard it left"
    );
    assert!(
        after.replacements.is_empty(),
        "`the next time` is spent by the first entry it catches"
    );
}

#[test]
fn issue_731_mistcaller_does_not_touch_a_creature_that_was_cast() {
    // `without being cast` is the whole of the card's restraint: the same creature cast
    // from hand enters untouched, and the replacement is still waiting afterwards.
    let db = db();
    let mut state = arm(&main_phase(), &db, PlayerId(1));
    let skeleton = state.new_instance(cid(&db, "reassembling_skeleton"));
    state.players[0].hand.push(skeleton);
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: skeleton,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    assert!(
        state
            .battlefield
            .iter()
            .any(|perm| perm.instance == skeleton.id),
        "a cast creature enters"
    );
    assert_eq!(state.replacements.len(), 1, "the replacement is untouched");
}

#[test]
fn issue_731_mistcaller_catches_only_the_next_creature() {
    // "The next time": the second reanimation of the turn is not replaced, because the
    // one-shot was spent on the first.
    let db = db();
    let armed = arm(&main_phase(), &db, PlayerId(1));
    let (after, first) = reanimate_skeleton(&armed, &db, PlayerId(0));
    let (after, second) = reanimate_skeleton(&after, &db, PlayerId(0));

    assert!(after.players[0].exile.iter().any(|c| c.id == first.id));
    assert!(
        after
            .battlefield
            .iter()
            .any(|perm| perm.instance == second.id),
        "the second creature entered"
    );
}
