//! CR 613 layers 4 and 7b: what a permanent **is**, and what its base power and toughness
//! **are** (issue #706).
//!
//! Skilled Animator and Sigiled Sword of Valeron are the two cards that need them, and
//! between them they need both halves of what makes a layer a layer:
//!
//! - **Layer 4 runs before everything that asks.** An artifact animated into a creature is
//!   inside an anthem's class, can be declared as an attacker, and dies to a creature
//!   sweeper. Nothing about those rules knows it was ever not a creature.
//! - **Layer 7b sets a base**, not a total. Counters and anthems fold onto it afterwards
//!   (7c), so an animated 5/5 with a `+1/+1` counter is a 6/6.
//!
//! And the durations are the two the engine already had: `for as long as this creature
//! remains on the battlefield` ends when the source leaves, and an Equipment's grant ends
//! when it is unattached.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, attacker_candidates, characteristics, pending_trigger_target_choice, Action,
    CardDatabase, CardId, CardInstance, CardType, Color, FunctionalId, GameState, Keyword,
    Permanent, PermanentId, PlayerId, Step, Target,
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
    let filler = cid(db, "bogstomper");
    for seat in 0..2 {
        let library: Vec<_> = (0..12).map(|_| state.new_instance(filler)).collect();
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
        entered_turn: 0,
        ..Default::default()
    });
    id
}

fn to_hand(state: &mut GameState, db: &CardDatabase, slug: &str, seat: PlayerId) -> CardInstance {
    let instance = state.new_instance(cid(db, slug));
    state.players[seat.0].hand.push(instance);
    instance
}

/// Cast the Animator and point its trigger at `artifact`.
fn animate(
    state: &GameState,
    db: &CardDatabase,
    artifact: PermanentId,
) -> (GameState, PermanentId) {
    let mut state = state.clone();
    let animator = to_hand(&mut state, db, "skilled_animator", PlayerId(0));
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: animator,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        db,
    );
    let state = apply_action(&state, &Action::PassPriority, db);
    let state = apply_action(&state, &Action::PassPriority, db);
    let ability = pending_trigger_target_choice(&state).expect("the enters trigger owes a target");
    let state = apply_action(
        &state,
        &Action::ChooseTriggerTargets {
            ability,
            mode: None,
            targets: vec![Target::Permanent(artifact)],
        },
        db,
    );
    let state = apply_action(&state, &Action::PassPriority, db);
    let state = apply_action(&state, &Action::PassPriority, db);
    let source = state
        .battlefield
        .iter()
        .find(|perm| perm.printed.card() == Some(cid(db, "skilled_animator")))
        .expect("the Animator is there")
        .id;
    (state, source)
}

/// An animated artifact **is** a creature, and a 5/5 one.
#[test]
fn skilled_animator_makes_an_artifact_into_a_creature() {
    let db = db();
    let mut state = main_phase(&db);
    // Fountain of Renewal is an artifact with no power or toughness at all.
    let fountain = place(&mut state, &db, "fountain_of_renewal", PlayerId(0));
    let before = characteristics(&state, fountain, &db);
    assert_eq!(before.power, None, "an artifact prints no power");

    let (state, _) = animate(&state, &db, fountain);

    let after = characteristics(&state, fountain, &db);
    assert!(
        after.types.contains(&CardType::Creature),
        "it is a creature"
    );
    assert!(
        after.types.contains(&CardType::Artifact),
        "and still an artifact — the types are added, not replaced"
    );
    assert_eq!((after.power, after.toughness), (Some(5), Some(5)));
}

/// Layer 4 runs before the rules that ask: an animated artifact can attack.
#[test]
fn an_animated_artifact_can_be_declared_as_an_attacker() {
    let db = db();
    let mut state = main_phase(&db);
    let fountain = place(&mut state, &db, "fountain_of_renewal", PlayerId(0));
    let (mut state, _) = animate(&state, &db, fountain);
    state.step = Step::DeclareAttackers;
    for perm in &mut state.battlefield {
        perm.entered_turn = 0;
    }

    assert!(
        attacker_candidates(&state, &db).contains(&fountain),
        "a creature that has been one since before this turn may attack"
    );
}

/// Layer 7b is a **base**: a counter folds onto it at 7c.
#[test]
fn a_counter_folds_onto_the_base_it_was_given() {
    let db = db();
    let mut state = main_phase(&db);
    let fountain = place(&mut state, &db, "fountain_of_renewal", PlayerId(0));
    let (mut state, _) = animate(&state, &db, fountain);
    if let Some(perm) = state
        .battlefield
        .iter_mut()
        .find(|perm| perm.id == fountain)
    {
        perm.counters
            .insert(sage_engine::CounterKind::PlusOnePlusOne, 1);
    }

    let stats = characteristics(&state, fountain, &db);
    assert_eq!((stats.power, stats.toughness), (Some(6), Some(6)));
}

/// `For as long as this creature remains on the battlefield`: the Animator leaving takes
/// both halves with it.
#[test]
fn the_animation_ends_when_the_animator_leaves() {
    let db = db();
    let mut state = main_phase(&db);
    let fountain = place(&mut state, &db, "fountain_of_renewal", PlayerId(0));
    let (mut state, animator) = animate(&state, &db, fountain);
    assert!(characteristics(&state, fountain, &db)
        .types
        .contains(&CardType::Creature));

    state.battlefield.retain(|perm| perm.id != animator);
    // The effect is ended by the state-based-action pass, which is where every other
    // "this permanent is gone" consequence is settled — so the walk needs one action.
    let state = apply_action(&state, &Action::PassPriority, &db);

    let after = characteristics(&state, fountain, &db);
    assert!(
        !after.types.contains(&CardType::Creature),
        "it is an artifact again"
    );
    assert_eq!(after.power, None, "with no power at all, as it printed");
}

/// An Equipment can add a type too, and it goes at the same layer.
#[test]
fn sigiled_sword_knights_its_bearer() {
    let db = db();
    let mut state = main_phase(&db);
    let bearer = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let sword = place(&mut state, &db, "sigiled_sword_of_valeron", PlayerId(0));
    if let Some(perm) = state.battlefield.iter_mut().find(|perm| perm.id == sword) {
        perm.attached_to = Some(bearer);
    }

    let stats = characteristics(&state, bearer, &db);
    assert!(
        stats.subtypes.iter().any(|kind| kind == "Knight"),
        "a Knight in addition to its other types"
    );
    assert!(
        stats.subtypes.iter().any(|kind| kind == "Ogre"),
        "and still an Ogre"
    );
    assert_eq!(stats.power, Some(6), "a 4/2 with +2/+0");
    assert!(stats.keywords.contains(&Keyword::Vigilance));
}

/// Unequipping takes the type back, exactly as it takes the keyword back.
#[test]
fn unequipping_takes_the_type_with_it() {
    let db = db();
    let mut state = main_phase(&db);
    let bearer = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let sword = place(&mut state, &db, "sigiled_sword_of_valeron", PlayerId(0));
    if let Some(perm) = state.battlefield.iter_mut().find(|perm| perm.id == sword) {
        perm.attached_to = Some(bearer);
    }
    assert!(characteristics(&state, bearer, &db)
        .subtypes
        .iter()
        .any(|kind| kind == "Knight"));

    if let Some(perm) = state.battlefield.iter_mut().find(|perm| perm.id == sword) {
        perm.attached_to = None;
    }

    let stats = characteristics(&state, bearer, &db);
    assert!(
        !stats.subtypes.iter().any(|kind| kind == "Knight"),
        "the grant left with the Equipment"
    );
    assert!(!stats.keywords.contains(&Keyword::Vigilance));
}
