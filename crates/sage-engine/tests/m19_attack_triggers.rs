//! Attack triggers that aim at something **relative to the attacker** (issue #706).
//!
//! Pegasus Courser and Star-Crowned Stag are the first two cards whose target class is
//! not a property of the board or of a player, but of the ability's own source: *another*
//! attacking creature is another one than this one, and the *defending player* is the
//! player this creature is attacking. Neither could be said before the targeting layer
//! was handed the source.
//!
//! Both are worth testing at the seam rather than at the effect: the effects (grant a
//! keyword, tap a permanent) are covered elsewhere, and what is new is which objects the
//! game offers as legal aims — before the ability resolves, and again when it does.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, characteristics, pending_trigger_target_choice, target_requirements, Action,
    Attack, AttackTarget, CardDatabase, CardId, Color, FunctionalId, GameState, Keyword, Permanent,
    PermanentId, PlayerId, Step, Target,
};

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// Seat 0 on the attack, with a stocked library so nothing trips the decking loss.
fn combat(db: &CardDatabase) -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::DeclareAttackers;
    state.active_player = PlayerId(0);
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    state.players[0].turn_began = state.turn;
    for player in &mut state.players {
        player.mana_pool.add(Color::White, 5);
    }
    let forest = cid(db, "forest");
    for seat in 0..2 {
        let library: Vec<_> = (0..12).map(|_| state.new_instance(forest)).collect();
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

fn attack(attacker: PermanentId) -> Attack {
    Attack {
        attacker,
        defender: AttackTarget::Player(PlayerId(1)),
    }
}

/// The candidates the game offers for the trigger that is owed a target.
fn offered(state: &GameState, db: &CardDatabase) -> Vec<Target> {
    let ability = pending_trigger_target_choice(state).expect("a trigger owes a target");
    let action = Action::ChooseTriggerTargets {
        ability,
        targets: Vec::new(),
    };
    target_requirements(state, db, &action)
        .into_iter()
        .flat_map(|requirement| requirement.candidates)
        .collect()
}

fn keywords(state: &GameState, db: &CardDatabase, id: PermanentId) -> Vec<Keyword> {
    characteristics(state, id, db).keywords
}

/// "Another target attacking creature": attacking, and not the Courser itself.
#[test]
fn pegasus_courser_grants_flying_to_another_attacker() {
    let db = db();
    let mut state = combat(&db);
    let courser = place(&mut state, &db, "pegasus_courser", PlayerId(0));
    let ally = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let home = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    for perm in &mut state.battlefield {
        perm.entered_turn = 0;
    }

    let state = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: vec![attack(courser), attack(ally)],
        },
        &db,
    );

    let candidates = offered(&state, &db);
    assert!(
        candidates.contains(&Target::Permanent(ally)),
        "the other attacker is offered"
    );
    assert!(
        !candidates.contains(&Target::Permanent(courser)),
        "and the Courser is not — *another* excludes itself"
    );
    assert!(
        !candidates.contains(&Target::Permanent(home)),
        "nor is the creature that stayed home"
    );

    let ability = pending_trigger_target_choice(&state).expect("owed a target");
    let state = apply_action(
        &state,
        &Action::ChooseTriggerTargets {
            ability,
            targets: vec![Target::Permanent(ally)],
        },
        &db,
    );
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    assert!(
        keywords(&state, &db, ally).contains(&Keyword::Flying),
        "the Ogre is flying for the turn"
    );
    assert!(
        !keywords(&state, &db, home).contains(&Keyword::Flying),
        "and nothing else gained anything"
    );
}

/// Attacking alone, the trigger has nothing to aim at — so it never goes on the stack
/// (CR 603.3c), and the attack still happens.
#[test]
fn pegasus_courser_attacking_alone_puts_no_trigger_on_the_stack() {
    let db = db();
    let mut state = combat(&db);
    let courser = place(&mut state, &db, "pegasus_courser", PlayerId(0));
    for perm in &mut state.battlefield {
        perm.entered_turn = 0;
    }

    let state = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: vec![attack(courser)],
        },
        &db,
    );

    assert!(
        pending_trigger_target_choice(&state).is_none(),
        "there is no *another* attacking creature"
    );
    assert!(state.stack.is_empty());
    assert!(
        state
            .battlefield
            .iter()
            .any(|perm| perm.id == courser && perm.attacking.is_some()),
        "and the Courser is still attacking"
    );
}

/// "Target creature defending player controls": read off the Stag's own attack, so an
/// ally's creature is never one.
#[test]
fn star_crowned_stag_taps_a_creature_the_defender_controls() {
    let db = db();
    let mut state = combat(&db);
    let stag = place(&mut state, &db, "star_crowned_stag", PlayerId(0));
    let theirs = place(&mut state, &db, "onakke_ogre", PlayerId(1));
    let mine = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    for perm in &mut state.battlefield {
        perm.entered_turn = 0;
    }

    let state = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: vec![attack(stag)],
        },
        &db,
    );

    let candidates = offered(&state, &db);
    assert!(
        candidates.contains(&Target::Permanent(theirs)),
        "the defending player's creature is offered"
    );
    assert!(
        !candidates.contains(&Target::Permanent(mine)),
        "and yours is not, however much it is a creature"
    );

    let ability = pending_trigger_target_choice(&state).expect("owed a target");
    let state = apply_action(
        &state,
        &Action::ChooseTriggerTargets {
            ability,
            targets: vec![Target::Permanent(theirs)],
        },
        &db,
    );
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    assert!(
        state
            .battlefield
            .iter()
            .any(|perm| perm.id == theirs && perm.tapped),
        "tapped"
    );
    assert!(
        state
            .battlefield
            .iter()
            .any(|perm| perm.id == mine && !perm.tapped),
        "and your own creature untouched"
    );
}

/// The phrase means nothing outside combat: a source that is not attacking names no
/// defending player, so the class is empty rather than "everyone".
#[test]
fn a_source_that_is_not_attacking_names_no_defending_player() {
    let db = db();
    let mut state = combat(&db);
    let stag = place(&mut state, &db, "star_crowned_stag", PlayerId(0));
    place(&mut state, &db, "onakke_ogre", PlayerId(1));
    for perm in &mut state.battlefield {
        perm.entered_turn = 0;
    }
    let state = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: vec![attack(stag)],
        },
        &db,
    );
    let ability = pending_trigger_target_choice(&state).expect("owed a target");

    // Combat ends under the trigger — the Stag is removed from it, exactly as the
    // end-of-combat turn-based action does (CR 511.3).
    let mut state = state;
    for perm in &mut state.battlefield {
        perm.attacking = None;
    }

    let action = Action::ChooseTriggerTargets {
        ability,
        targets: Vec::new(),
    };
    let candidates: Vec<Target> = target_requirements(&state, &db, &action)
        .into_iter()
        .flat_map(|requirement| requirement.candidates)
        .collect();
    assert!(
        candidates.is_empty(),
        "nothing is a creature the defending player controls when nobody is defending"
    );
}
