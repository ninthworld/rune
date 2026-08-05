//! Amounts derived from a count, and the two zones an effect can now push a card into:
//! a whole graveyard to exile, and one permanent to the top of a library.
//!
//! The point of a derived amount is *when* X is taken. It is computed once, on
//! resolution (CR 608.2), from the board as it stands then — so a creature that dies
//! afterwards does not take the life back, and one that arrives afterwards was never
//! counted.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, pending_trigger_target_choice, Action, CardDatabase, CardId, CardInstance, Color,
    FunctionalId, GameState, Permanent, PermanentId, PlayerId, Step, Target,
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

/// Cast `slug` as a creature spell and resolve it, leaving any trigger it put on the
/// stack still owing its targets.
fn resolve_creature(state: &GameState, db: &CardDatabase, slug: &str) -> GameState {
    let mut state = state.clone();
    let instance = to_hand(&mut state, db, slug, PlayerId(0));
    let mut state = apply_action(
        &state,
        &Action::CastSpell {
            card: instance,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        db,
    );
    for _ in 0..2 {
        state = apply_action(&state, &Action::PassPriority, db);
    }
    state
}

fn on_battlefield(state: &GameState, id: PermanentId) -> bool {
    state.battlefield.iter().any(|p| p.id == id)
}

/// Dwarven Priest counts the creatures its controller controls — including itself,
/// because it is on the battlefield by the time its own trigger resolves.
#[test]
fn dwarven_priest_gains_one_life_per_creature_including_itself() {
    let db = db();
    let mut state = main_phase();
    place(&mut state, &db, "onakke_ogre", PlayerId(0));
    place(&mut state, &db, "onakke_ogre", PlayerId(0));
    place(&mut state, &db, "onakke_ogre", PlayerId(1));
    let life = state.players[0].life;

    let mut state = resolve_creature(&state, &db, "dwarven_priest");
    for _ in 0..2 {
        state = apply_action(&state, &Action::PassPriority, &db);
    }
    assert_eq!(
        state.players[0].life,
        life + 3,
        "two Ogres and the Priest; the opponent's Ogre is not yours"
    );
}

/// Volley Veteran's damage is the Goblin count, taken on resolution. With three
/// Goblins out it is three damage, which kills a 3/3 and does not kill a 3/4.
#[test]
fn volley_veteran_deals_damage_equal_to_your_goblin_count() {
    let db = db();
    let mut state = main_phase();
    place(&mut state, &db, "goblin_motivator", PlayerId(0));
    place(&mut state, &db, "goblin_motivator", PlayerId(0));
    let target = place(&mut state, &db, "onakke_ogre", PlayerId(1));

    // Volley Veteran is itself a Goblin, so resolving it makes three.
    let state = resolve_creature(&state, &db, "volley_veteran");
    let ability = pending_trigger_target_choice(&state).expect("the ETB owes a target");
    let state = apply_action(
        &state,
        &Action::ChooseTriggerTargets {
            ability,
            targets: vec![Target::Permanent(target)],
        },
        &db,
    );
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    assert!(
        !on_battlefield(&state, target),
        "three damage killed the 4/2"
    );
}

/// With no other Goblin the same card deals one damage, which the same creature
/// survives — the amount really is read off the board rather than baked in.
#[test]
fn volley_veteran_deals_one_damage_alone() {
    let db = db();
    let mut state = main_phase();
    let target = place(&mut state, &db, "onakke_ogre", PlayerId(1));

    let state = resolve_creature(&state, &db, "volley_veteran");
    let ability = pending_trigger_target_choice(&state).expect("the ETB owes a target");
    let state = apply_action(
        &state,
        &Action::ChooseTriggerTargets {
            ability,
            targets: vec![Target::Permanent(target)],
        },
        &db,
    );
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    assert!(on_battlefield(&state, target), "a 4/2 survives one damage");
    assert_eq!(
        state
            .battlefield
            .iter()
            .find(|p| p.id == target)
            .unwrap()
            .damage,
        1
    );
}

/// Remorseful Cleric spends itself to empty a graveyard. Every card goes at once, and
/// an already-empty graveyard is a legal target that simply does nothing.
#[test]
fn remorseful_cleric_exiles_a_whole_graveyard() {
    let db = db();
    let mut state = main_phase();
    for _ in 0..3 {
        let instance = state.new_instance(cid(&db, "shock"));
        state.players[1].graveyard.push(instance);
    }
    let cleric = place(&mut state, &db, "remorseful_cleric", PlayerId(0));

    let after = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: cleric,
            index: 0,
            targets: vec![Target::Player(PlayerId(1))],
        },
        &db,
    );
    assert!(
        !on_battlefield(&after, cleric),
        "the cost is paid on activation"
    );
    let after = apply_action(&after, &Action::PassPriority, &db);
    let after = apply_action(&after, &Action::PassPriority, &db);
    assert!(after.players[1].graveyard.is_empty());
    assert_eq!(after.players[1].exile.len(), 3);
    // The Cleric's own graveyard is untouched — it named one player.
    assert_eq!(after.players[0].graveyard.len(), 1, "the Cleric itself");
}

/// Totally Lost puts a permanent where its owner has to draw it again. The card lands
/// on **top**, which is the last element of the library.
#[test]
fn totally_lost_puts_a_permanent_on_top_of_its_owners_library() {
    let db = db();
    let mut state = main_phase();
    let ogre = place(&mut state, &db, "onakke_ogre", PlayerId(1));
    let instance = state
        .battlefield
        .iter()
        .find(|p| p.id == ogre)
        .unwrap()
        .instance;
    let library = state.players[1].library.len();

    let spell = to_hand(&mut state, &db, "totally_lost", PlayerId(0));
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: spell,
            targets: vec![Target::Permanent(ogre)],
            payment: Vec::new(),
        },
        &db,
    );
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    assert!(!on_battlefield(&state, ogre));
    assert_eq!(state.players[1].library.len(), library + 1);
    assert_eq!(
        state.players[1].library.last().map(|c| c.id),
        Some(instance),
        "on top, which is the last element"
    );
}

/// A **token** put anywhere but the battlefield ceases to exist (CR 111.7), so it never
/// reaches the library either.
#[test]
fn a_token_put_on_top_of_a_library_ceases_to_exist_instead() {
    let db = db();
    let mut state = main_phase();
    let token = sage_engine::PermanentId(state.mint_id());
    state.battlefield.push(Permanent {
        id: token,
        instance: sage_engine::CardInstanceId(9_999),
        printed: sage_engine::Printed::Token(Box::new(sage_engine::TokenData {
            name: "Goblin".to_string(),
            types: vec![sage_engine::CardType::Creature],
            subtypes: vec!["Goblin".to_string()],
            colors: vec![Color::Red],
            power: Some(1),
            toughness: Some(1),
            ..Default::default()
        })),
        controller: PlayerId(1),
        ..Default::default()
    });
    let library = state.players[1].library.len();

    let spell = to_hand(&mut state, &db, "totally_lost", PlayerId(0));
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: spell,
            targets: vec![Target::Permanent(token)],
            payment: Vec::new(),
        },
        &db,
    );
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    assert!(!on_battlefield(&state, token));
    assert_eq!(state.players[1].library.len(), library, "it simply stopped");
}
