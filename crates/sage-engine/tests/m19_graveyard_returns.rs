//! Returning a card from a graveyard: to a hand, and to the battlefield tapped.
//!
//! The one target spec that names a **card in a zone** rather than an object on the
//! battlefield now carries three independent fields — whose graveyard, which class of
//! card, and how expensive — so these tests are mostly about the candidate set being
//! exactly what the printed card says, and about a returned card going to its *owner*.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, pending_trigger_target_choice, target_requirements, Action, CardDatabase, CardId,
    CardInstance, Color, FunctionalId, GameState, PlayerId, Step, Target,
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

/// Put `slug` into `seat`'s graveyard and return the instance.
fn to_graveyard(
    state: &mut GameState,
    db: &CardDatabase,
    slug: &str,
    seat: PlayerId,
) -> CardInstance {
    let instance = state.new_instance(cid(db, slug));
    state.players[seat.0].graveyard.push(instance);
    instance
}

fn to_hand(state: &mut GameState, db: &CardDatabase, slug: &str, seat: PlayerId) -> CardInstance {
    let instance = state.new_instance(cid(db, slug));
    state.players[seat.0].hand.push(instance);
    instance
}

/// Cast `slug` with `targets` and let it resolve.
fn cast(state: &GameState, db: &CardDatabase, slug: &str, targets: Vec<Target>) -> GameState {
    let mut state = state.clone();
    let instance = to_hand(&mut state, db, slug, PlayerId(0));
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
    apply_action(&state, &Action::PassPriority, db)
}

/// The candidate set for the one slot the cast of `slug` declares.
fn candidates(state: &GameState, db: &CardDatabase, spell: CardInstance) -> Vec<Target> {
    let slots = target_requirements(
        state,
        db,
        &Action::CastSpell {
            card: spell,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
    );
    slots
        .first()
        .map(|s| s.candidates.clone())
        .unwrap_or_default()
}

/// Recollect names any card, and only in its caster's own graveyard.
#[test]
fn recollect_returns_any_card_from_your_own_graveyard_and_never_an_opponents() {
    let db = db();
    let mut state = main_phase();
    let mine = to_graveyard(&mut state, &db, "shock", PlayerId(0));
    let my_land = to_graveyard(&mut state, &db, "forest", PlayerId(0));
    let theirs = to_graveyard(&mut state, &db, "shock", PlayerId(1));

    let mut probe = state.clone();
    let spell = to_hand(&mut probe, &db, "recollect", PlayerId(0));
    let legal = candidates(&probe, &db, spell);
    assert!(legal.contains(&Target::Card(mine.id)));
    assert!(legal.contains(&Target::Card(my_land.id)), "any card");
    assert!(
        !legal.contains(&Target::Card(theirs.id)),
        "\"your graveyard\" is one graveyard"
    );

    let hand = state.players[0].hand.len();
    let after = cast(&state, &db, "recollect", vec![Target::Card(mine.id)]);
    assert!(after.players[0].hand.iter().any(|c| c.id == mine.id));
    // The Shock returned, and Recollect itself went to the graveyard.
    assert_eq!(after.players[0].hand.len(), hand + 1);
    assert!(!after.players[0].graveyard.iter().any(|c| c.id == mine.id));
}

/// Salvager of Secrets narrows the same spec to one class: an instant or sorcery card,
/// which is one class as a card writes it rather than two types.
#[test]
fn salvager_of_secrets_takes_back_an_instant_or_sorcery_and_nothing_else() {
    let db = db();
    let mut state = main_phase();
    let instant = to_graveyard(&mut state, &db, "shock", PlayerId(0));
    let sorcery = to_graveyard(&mut state, &db, "mind_rot", PlayerId(0));
    let creature = to_graveyard(&mut state, &db, "onakke_ogre", PlayerId(0));

    let salvager = to_hand(&mut state, &db, "salvager_of_secrets", PlayerId(0));
    let mut state = apply_action(
        &state,
        &Action::CastSpell {
            card: salvager,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    for _ in 0..2 {
        state = apply_action(&state, &Action::PassPriority, &db);
    }
    let ability = pending_trigger_target_choice(&state).expect("the ETB owes a target");
    let slots = target_requirements(
        &state,
        &db,
        &Action::ChooseTriggerTargets {
            ability,
            mode: None,
            targets: Vec::new(),
        },
    );
    let legal = &slots[0].candidates;
    assert!(legal.contains(&Target::Card(instant.id)));
    assert!(
        legal.contains(&Target::Card(sorcery.id)),
        "both types, one class"
    );
    assert!(!legal.contains(&Target::Card(creature.id)));

    let state = apply_action(
        &state,
        &Action::ChooseTriggerTargets {
            ability,
            mode: None,
            targets: vec![Target::Card(sorcery.id)],
        },
        &db,
    );
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    assert!(state.players[0].hand.iter().any(|c| c.id == sorcery.id));
}

/// Macabre Waltz names **up to two** targets, so casting it with none, one, or two is
/// each a legal announcement — and the discard that follows is a separate, mandatory
/// effect that fills no slot.
#[test]
fn macabre_waltz_returns_up_to_two_creature_cards_then_discards() {
    let db = db();
    let mut state = main_phase();
    let first = to_graveyard(&mut state, &db, "onakke_ogre", PlayerId(0));
    let second = to_graveyard(&mut state, &db, "walking_corpse", PlayerId(0));
    to_hand(&mut state, &db, "forest", PlayerId(0));

    let mut probe = state.clone();
    let spell = to_hand(&mut probe, &db, "macabre_waltz", PlayerId(0));
    let slots = target_requirements(
        &probe,
        &db,
        &Action::CastSpell {
            card: spell,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
    );
    assert_eq!(slots.len(), 2, "two slots for an up-to-two group");
    assert!(
        slots.iter().all(|s| s.optional),
        "and both may be left empty"
    );

    let state = cast(
        &state,
        &db,
        "macabre_waltz",
        vec![Target::Card(first.id), Target::Card(second.id)],
    );
    // Both are back, and the discard is owed as a mid-resolution choice.
    assert!(state.players[0].hand.iter().any(|c| c.id == first.id));
    assert!(state.players[0].hand.iter().any(|c| c.id == second.id));
    let pending = sage_engine::pending_player_choice(&state).expect("the discard is owed");
    assert_eq!(pending.chooser, PlayerId(0));
}

/// Gravewaker returns a creature card to the battlefield **tapped** — the entry state
/// the returning effect dictates, exactly as a token's creator dictates its own.
#[test]
fn gravewaker_reanimates_tapped() {
    let db = db();
    let mut state = main_phase();
    let corpse = to_graveyard(&mut state, &db, "onakke_ogre", PlayerId(0));
    let card = cid(&db, "gravewaker");
    let instance = state.new_instance(card).id;
    let waker = sage_engine::PermanentId(state.mint_id());
    state.battlefield.push(sage_engine::Permanent {
        id: waker,
        instance,
        printed: card.into(),
        controller: PlayerId(0),
        ..Default::default()
    });

    let after = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: waker,
            index: 0,
            targets: vec![Target::Card(corpse.id)],
            payment: Vec::new(),
        },
        &db,
    );
    let after = apply_action(&after, &Action::PassPriority, &db);
    let after = apply_action(&after, &Action::PassPriority, &db);

    let returned = after
        .battlefield
        .iter()
        .find(|p| p.instance == corpse.id)
        .expect("the Ogre is back");
    assert!(returned.tapped, "it arrives tapped");
    assert_eq!(returned.controller, PlayerId(0));
    assert!(!after.players[0].graveyard.iter().any(|c| c.id == corpse.id));
}

/// Trusty Packbeast narrows to artifact cards. A graveyard with none offers no legal
/// choice, so the trigger is never put on the stack at all (CR 603.3c).
#[test]
fn trusty_packbeast_withholds_its_trigger_with_no_artifact_in_the_graveyard() {
    let db = db();
    let mut state = main_phase();
    to_graveyard(&mut state, &db, "onakke_ogre", PlayerId(0));

    let beast = to_hand(&mut state, &db, "trusty_packbeast", PlayerId(0));
    let mut state = apply_action(
        &state,
        &Action::CastSpell {
            card: beast,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    for _ in 0..2 {
        state = apply_action(&state, &Action::PassPriority, &db);
    }
    assert!(pending_trigger_target_choice(&state).is_none());
    assert!(state.stack.is_empty(), "no trigger with nothing to aim at");
}
