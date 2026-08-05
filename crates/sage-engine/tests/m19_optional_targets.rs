//! Optional effects that choose a target (issue #725): the `may` that declares the
//! target group of the one effect it wraps.
//!
//! The two halves of the card happen at different times and this file is mostly about
//! keeping them apart. The **target** is chosen at announcement (CR 601.2c), so it is
//! offered as a trigger slot like any other and an aim with no candidate withholds the
//! trigger entirely (CR 603.3c). The **yes-or-no** waits for resolution (CR 608.2), so
//! declining skips the wrapped effect and costs the rest of the game nothing. And a
//! target that has gone in between takes neither path: the object never resolves
//! (CR 608.2b) and the question is not asked at all.
//!
//! Every test drives the real `apply_action`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, pending_player_choice, pending_trigger_target_choice, target_requirements,
    Action, CardDatabase, CardId, CardInstance, Color, FunctionalId, GameEvent, GameState,
    Permanent, PermanentId, PlayerId, Step, Target,
};

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// A two-player game at seat 0's precombat main, with mana enough for anything here.
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

/// Put a permanent of `slug` onto the battlefield under `controller`.
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

/// Cast the creature `slug` from seat 0's hand and let it resolve, so its
/// enters-the-battlefield trigger reaches the stack unaimed.
fn cast_creature(state: &GameState, db: &CardDatabase, slug: &str) -> GameState {
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

/// Aim the trigger that is owed targets at `target` and let it resolve.
fn aim_and_resolve(state: &GameState, db: &CardDatabase, target: Target) -> GameState {
    let ability = pending_trigger_target_choice(state).expect("the trigger owes a target");
    let state = apply_action(
        state,
        &Action::ChooseTriggerTargets {
            ability,
            targets: vec![target],
        },
        db,
    );
    let state = apply_action(&state, &Action::PassPriority, db);
    apply_action(&state, &Action::PassPriority, db)
}

fn answer(state: &GameState, db: &CardDatabase, accept: bool) -> GameState {
    apply_action(state, &Action::AnswerConfirm { accept }, db)
}

fn events(state: &GameState) -> Vec<&GameEvent> {
    state.log.iter().map(|entry| &entry.event).collect()
}

// ----- the target is chosen at announcement ---------------------------------

#[test]
fn issue_725_an_optional_effect_declares_its_targets_slot_at_announcement() {
    // The wrapper forwards the group of the effect it wraps, so Gravedigger's trigger
    // owes exactly one target and the candidate set is the wrapped effect's own: the
    // creature cards in its controller's graveyard, and nothing else.
    let db = db();
    let mut state = main_phase();
    let creature = to_graveyard(&mut state, &db, "onakke_ogre", PlayerId(0));
    let instant = to_graveyard(&mut state, &db, "shock", PlayerId(0));
    let theirs = to_graveyard(&mut state, &db, "walking_corpse", PlayerId(1));

    let state = cast_creature(&state, &db, "gravedigger");
    let ability = pending_trigger_target_choice(&state).expect("the ETB owes a target");
    let slots = target_requirements(
        &state,
        &db,
        &Action::ChooseTriggerTargets {
            ability,
            targets: Vec::new(),
        },
    );
    assert_eq!(slots.len(), 1, "one wrapped effect, one slot");
    assert!(!slots[0].optional, "and a mandatory one to fill");
    let legal = &slots[0].candidates;
    assert!(legal.contains(&Target::Card(creature.id)));
    assert!(!legal.contains(&Target::Card(instant.id)), "creature cards");
    assert!(
        !legal.contains(&Target::Card(theirs.id)),
        "and only from your own graveyard",
    );

    // No question is owed yet: the aim comes first, the yes-or-no on resolution.
    assert!(pending_player_choice(&state).is_none());
}

#[test]
fn issue_725_a_trigger_with_nothing_to_aim_at_never_reaches_the_stack() {
    // CR 603.3c. The slot is real, so an empty graveyard leaves it unfillable and the
    // trigger is withheld rather than put on the stack to ask a question about nothing.
    let db = db();
    let mut state = main_phase();
    to_graveyard(&mut state, &db, "shock", PlayerId(0));

    let state = cast_creature(&state, &db, "gravedigger");
    assert!(pending_trigger_target_choice(&state).is_none());
    assert!(state.stack.is_empty(), "no trigger, and no question");
    assert!(pending_player_choice(&state).is_none());
}

// ----- and answered on resolution -------------------------------------------

#[test]
fn issue_725_accepting_returns_the_card_the_announcement_chose() {
    // The whole path: aim, resolve, be asked, say yes. The target chosen at
    // announcement is the card that comes back — it rode the suspension on the offer
    // rather than being re-derived on the way out.
    let db = db();
    let mut state = main_phase();
    let ogre = to_graveyard(&mut state, &db, "onakke_ogre", PlayerId(0));
    let corpse = to_graveyard(&mut state, &db, "walking_corpse", PlayerId(0));

    let state = cast_creature(&state, &db, "gravedigger");
    let owed = aim_and_resolve(&state, &db, Target::Card(ogre.id));

    let pending = pending_player_choice(&owed).expect("the yes-or-no is owed");
    assert_eq!(pending.chooser, PlayerId(0));
    assert_eq!(
        pending.question.confirm().expect("a yes-or-no").targets,
        vec![Target::Card(ogre.id)],
        "the offer carries the target the announcement chose",
    );

    let taken = answer(&owed, &db, true);
    assert!(taken.players[0].hand.iter().any(|c| c.id == ogre.id));
    assert!(!taken.players[0].graveyard.iter().any(|c| c.id == ogre.id));
    assert!(
        taken.players[0].graveyard.iter().any(|c| c.id == corpse.id),
        "the card it was not aimed at stays where it was",
    );
    assert!(events(&taken)
        .iter()
        .any(|event| matches!(event, GameEvent::OptionalApplied { .. })));
}

#[test]
fn issue_725_declining_leaves_the_target_exactly_where_it_was() {
    // A decline is a decline, not a fizzle: the card stays in the graveyard, the
    // creature that asked is on the battlefield as it would be either way, and the log
    // records that the offer was made and refused.
    let db = db();
    let mut state = main_phase();
    let ogre = to_graveyard(&mut state, &db, "onakke_ogre", PlayerId(0));

    let state = cast_creature(&state, &db, "gravedigger");
    let owed = aim_and_resolve(&state, &db, Target::Card(ogre.id));
    let refused = answer(&owed, &db, false);

    assert!(pending_player_choice(&refused).is_none());
    assert!(refused.players[0].graveyard.iter().any(|c| c.id == ogre.id));
    assert!(!refused.players[0].hand.iter().any(|c| c.id == ogre.id));
    assert!(
        refused
            .battlefield
            .iter()
            .any(|p| p.printed.card() == Some(cid(&db, "gravedigger"))),
        "the creature that offered it is unaffected",
    );
    assert!(refused.stack.is_empty(), "and the ability is done");
    assert!(events(&refused)
        .iter()
        .any(|event| matches!(event, GameEvent::OptionalDeclined { .. })));
    assert!(!events(&refused)
        .iter()
        .any(|event| matches!(event, GameEvent::OptionalApplied { .. })));
}

#[test]
fn issue_725_a_target_gone_by_resolution_fizzles_the_ability_before_the_question() {
    // CR 608.2b: an object whose every target has become illegal does not resolve, so
    // there is nothing to accept. Reclamation Sage aims at an artifact that leaves in
    // response — the trigger is removed, no yes-or-no is posed, and nothing is
    // destroyed. Asserted against the same run where the artifact survives, which does
    // pose the question.
    let db = db();
    let mut state = main_phase();
    let artifact = place(&mut state, &db, "manalith", PlayerId(1));

    let aimed = cast_creature(&state, &db, "reclamation_sage");
    let ability = pending_trigger_target_choice(&aimed).expect("the ETB owes a target");
    let aimed = apply_action(
        &aimed,
        &Action::ChooseTriggerTargets {
            ability,
            targets: vec![Target::Permanent(artifact)],
        },
        &db,
    );

    // The control: left alone, the trigger resolves and the offer is made.
    let mut offered = aimed.clone();
    for _ in 0..2 {
        offered = apply_action(&offered, &Action::PassPriority, &db);
    }
    let pending = pending_player_choice(&offered).expect("the yes-or-no is owed");
    assert_eq!(
        pending.question.confirm().expect("a yes-or-no").targets,
        vec![Target::Permanent(artifact)],
    );
    let destroyed = answer(&offered, &db, true);
    assert!(
        !destroyed.battlefield.iter().any(|p| p.id == artifact),
        "accepting destroys what was aimed at",
    );

    // The real case: the artifact is gone before the trigger resolves.
    let mut vanished = aimed;
    vanished.battlefield.retain(|p| p.id != artifact);
    for _ in 0..2 {
        vanished = apply_action(&vanished, &Action::PassPriority, &db);
    }
    assert!(
        pending_player_choice(&vanished).is_none(),
        "a fizzled ability asks nobody anything",
    );
    assert!(vanished.stack.is_empty(), "and leaves the stack");
    assert!(!events(&vanished).iter().any(|event| matches!(
        event,
        GameEvent::OptionalApplied { .. } | GameEvent::OptionalDeclined { .. }
    )));
}

#[test]
fn issue_725_accepting_a_target_that_has_gone_applies_nothing() {
    // The other side of CR 608.2b, at the splice: the target rides the offer and is
    // handed back to the wrapped effect, which re-checks it (CR 608.2c) rather than
    // trusting the announcement. Nothing in the game can take a permanent away while a
    // question is owed — only mana abilities are legal then — so the departure is
    // staged directly, which is the point: the guarantee is the resumed walk's, not the
    // action generator's.
    let db = db();
    let mut state = main_phase();
    let artifact = place(&mut state, &db, "manalith", PlayerId(1));
    let bystander = place(&mut state, &db, "ajani_s_welcome", PlayerId(1));

    let state = cast_creature(&state, &db, "reclamation_sage");
    let mut owed = aim_and_resolve(&state, &db, Target::Permanent(artifact));
    assert!(pending_player_choice(&owed).is_some());
    owed.battlefield.retain(|p| p.id != artifact);

    let after = answer(&owed, &db, true);
    assert!(pending_player_choice(&after).is_none(), "the offer is over");
    assert!(
        after.battlefield.iter().any(|p| p.id == bystander),
        "an illegal target destroys nothing, least of all something else",
    );
    assert!(
        events(&after)
            .iter()
            .any(|event| matches!(event, GameEvent::OptionalApplied { .. })),
        "the acceptance still happened; it is the effect that found nothing",
    );
}

#[test]
fn issue_725_accepting_destroys_only_what_the_ability_was_aimed_at() {
    // Two legal candidates, one slot: the enchantment the announcement passed over is
    // untouched, which is the difference between a target and a class.
    let db = db();
    let mut state = main_phase();
    let artifact = place(&mut state, &db, "manalith", PlayerId(1));
    let enchantment = place(&mut state, &db, "ajani_s_welcome", PlayerId(1));

    let state = cast_creature(&state, &db, "reclamation_sage");
    let owed = aim_and_resolve(&state, &db, Target::Permanent(enchantment));
    let after = answer(&owed, &db, true);

    assert!(!after.battlefield.iter().any(|p| p.id == enchantment));
    assert!(
        after.battlefield.iter().any(|p| p.id == artifact),
        "the other candidate is not a target and is not destroyed",
    );
}
