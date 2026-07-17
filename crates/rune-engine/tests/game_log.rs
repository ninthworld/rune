//! Structured game-log emission (issue #259): the engine records each public fact
//! at its seam in the transition pipeline, in causal order, carrying enough identity
//! to name a referenced object even after it leaves play.
//!
//! These drive the real [`apply_action`] pipeline over the bundled card database and
//! assert on the [`GameEvent`] window left on [`GameState`], covering each vocabulary
//! variant, the step-before-consequences ordering, and creature-only death detection.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rune_engine::{
    apply_action, Action, CardDatabase, CardId, CardInstance, Color, DamageTarget, FunctionalId,
    GameEvent, GameState, Permanent, PermanentId, PlayerId, StackId, StackObject, StackObjectKind,
    Step, Target,
};

/// The bundled card database.
fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

/// The interned [`CardId`] for an authored `functional_id`.
fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// The events currently on the state's log window, in order.
fn events(state: &GameState) -> Vec<&GameEvent> {
    state.log.iter().map(|entry| &entry.event).collect()
}

/// The index of the first event matching `pred`, or `None`.
fn find(state: &GameState, pred: impl Fn(&GameEvent) -> bool) -> Option<usize> {
    events(state).iter().position(|event| pred(event))
}

/// A two-player game parked in the precombat main phase with player 0 to act, its
/// stack empty and priority reset — a neutral place to seed and resolve spells.
fn main_phase() -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    state
}

/// Seed `card` as a spell on the stack under `controller` with `targets`, returning
/// its stack id. Bypasses the cast gate so instants (whose cast action is not yet
/// offered) can be resolved through the public pass-priority path.
fn push_spell(
    state: &mut GameState,
    card: CardInstance,
    controller: PlayerId,
    targets: Vec<Target>,
) -> StackId {
    let id = StackId(state.mint_id());
    state.stack.push(StackObject {
        id,
        controller,
        kind: StackObjectKind::Spell { card },
        targets,
    });
    id
}

/// Place a permanent of `slug` under player 0 with `damage` marked, returning its id.
fn place(state: &mut GameState, db: &CardDatabase, slug: &str, damage: u32) -> PermanentId {
    let card = cid(db, slug);
    let instance = state.new_instance(card).id;
    let id = PermanentId(state.mint_id());
    state.battlefield.push(Permanent {
        id,
        instance,
        card,
        controller: PlayerId(0),
        damage,
        ..Default::default()
    });
    id
}

/// Pass priority once for the current holder.
fn pass(state: &GameState, db: &CardDatabase) -> GameState {
    apply_action(state, &Action::PassPriority, db)
}

/// Both players pass in succession, resolving the top of the stack.
fn resolve_top(state: &GameState, db: &CardDatabase) -> GameState {
    pass(&pass(state, db), db)
}

// ----- Spell lifecycle: cast, resolve, counter, fizzle -----

#[test]
fn a_creature_cast_logs_spell_cast_then_spell_resolved_in_order() {
    let db = db();
    let mut state = main_phase();
    let scout = state.new_instance(cid(&db, "verdant_scout"));
    state.players[0].hand = vec![scout];
    state.players[0].mana_pool.add(Color::Green, 1);

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: scout,
            targets: Vec::new(),
        },
        &db,
    );
    let cast = find(
        &state,
        |e| matches!(e, GameEvent::SpellCast { card, .. } if card.id == scout.id),
    )
    .expect("the cast is logged");

    let state = resolve_top(&state, &db);
    let resolved = find(
        &state,
        |e| matches!(e, GameEvent::SpellResolved { card, .. } if card.id == scout.id),
    )
    .expect("the resolution is logged");
    assert!(cast < resolved, "cast is logged before resolution");
}

#[test]
fn burn_to_a_player_logs_damage_dealt_after_resolution_and_no_life_change() {
    // Damage to a player is a `damage_dealt` event, never a `life_changed` one, so a
    // client can report the hit and the two never double-count.
    let db = db();
    let mut state = main_phase();
    let bolt = state.new_instance(cid(&db, "quickfire_bolt"));
    push_spell(
        &mut state,
        bolt,
        PlayerId(0),
        vec![Target::Player(PlayerId(1))],
    );

    let state = resolve_top(&state, &db);

    let resolved =
        find(&state, |e| matches!(e, GameEvent::SpellResolved { .. })).expect("the bolt resolves");
    let damaged = find(&state, |e| {
        matches!(
            e,
            GameEvent::DamageDealt {
                target: DamageTarget::Player(p),
                amount: 3,
            } if *p == PlayerId(1)
        )
    })
    .expect("3 damage to the player is logged");
    assert!(resolved < damaged, "resolution precedes its damage");
    assert!(
        !events(&state)
            .iter()
            .any(|e| matches!(e, GameEvent::LifeChanged { .. })),
        "damage to a player is not also a life_changed event"
    );
    assert_eq!(state.players[1].life, 17, "20 - 3 damage");
}

#[test]
fn lethal_burn_to_a_creature_logs_nonlethal_damage_then_a_single_death() {
    let db = db();
    let mut state = main_phase();
    let boar = place(&mut state, &db, "thornback_boar", 0); // 3/2
    let bolt = state.new_instance(cid(&db, "quickfire_bolt")); // 3 damage, lethal
    push_spell(&mut state, bolt, PlayerId(0), vec![Target::Permanent(boar)]);

    let state = resolve_top(&state, &db);

    let damaged = find(&state, |e| {
        matches!(e, GameEvent::DamageDealt { target: DamageTarget::Permanent(lp), amount: 3 } if lp.permanent == boar)
    })
    .expect("damage to the creature is logged");
    let died = find(
        &state,
        |e| matches!(e, GameEvent::PermanentDied { permanent } if permanent.permanent == boar),
    )
    .expect("the creature death is logged");
    assert!(
        damaged < died,
        "damage is logged before the death it causes"
    );
    assert_eq!(
        events(&state)
            .iter()
            .filter(|e| matches!(e, GameEvent::PermanentDied { .. }))
            .count(),
        1,
        "exactly one death"
    );
}

#[test]
fn nonlethal_burn_logs_damage_but_no_death() {
    let db = db();
    let mut state = main_phase();
    let basilisk = place(&mut state, &db, "stonehide_basilisk", 0); // 4/5
    let shock = state.new_instance(cid(&db, "cinder_shock")); // 2 damage, nonlethal to a 4/5
    push_spell(
        &mut state,
        shock,
        PlayerId(0),
        vec![Target::Permanent(basilisk)],
    );

    let state = resolve_top(&state, &db);

    assert!(
        find(&state, |e| matches!(
            e,
            GameEvent::DamageDealt { amount: 2, .. }
        ))
        .is_some(),
        "nonlethal damage is still reported"
    );
    assert!(
        !events(&state)
            .iter()
            .any(|e| matches!(e, GameEvent::PermanentDied { .. })),
        "a survivable hit produces no death"
    );
}

#[test]
fn countering_a_spell_logs_spell_countered_for_its_controller() {
    let db = db();
    let mut state = main_phase();
    // A creature spell owned by player 0, with a counterspell on top owned by player 1.
    let scout = state.new_instance(cid(&db, "verdant_scout"));
    let target = push_spell(&mut state, scout, PlayerId(0), Vec::new());
    let negation = state.new_instance(cid(&db, "runic_negation"));
    push_spell(
        &mut state,
        negation,
        PlayerId(1),
        vec![Target::Spell(target)],
    );

    // Both pass: the counter (top) resolves and removes the creature spell.
    let state = resolve_top(&state, &db);

    let countered = find(&state, |e| {
        matches!(e, GameEvent::SpellCountered { player, card } if *player == PlayerId(0) && card.id == scout.id)
    })
    .expect("the countered spell is logged against its controller");
    let resolved = find(
        &state,
        |e| matches!(e, GameEvent::SpellResolved { card, .. } if card.id == negation.id),
    )
    .expect("the counterspell itself resolved");
    assert!(resolved < countered, "the counter resolves, then counters");
    assert!(
        state.stack.is_empty(),
        "both the counter and its target left the stack"
    );
}

#[test]
fn a_spell_whose_only_target_vanished_logs_spell_fizzled() {
    let db = db();
    let mut state = main_phase();
    let boar = place(&mut state, &db, "thornback_boar", 0);
    let bolt = state.new_instance(cid(&db, "quickfire_bolt"));
    push_spell(&mut state, bolt, PlayerId(0), vec![Target::Permanent(boar)]);
    // The target leaves before the bolt resolves.
    state.battlefield.retain(|p| p.id != boar);

    let state = resolve_top(&state, &db);

    assert!(
        find(
            &state,
            |e| matches!(e, GameEvent::SpellFizzled { card, .. } if card.id == bolt.id)
        )
        .is_some(),
        "the fizzle is logged"
    );
    assert!(
        !events(&state)
            .iter()
            .any(|e| matches!(e, GameEvent::DamageDealt { .. })),
        "a fizzled bolt deals no damage"
    );
    assert!(
        !events(&state)
            .iter()
            .any(|e| matches!(e, GameEvent::SpellResolved { .. })),
        "a fizzled spell never resolves"
    );
}

#[test]
fn gaining_life_logs_life_changed_not_damage() {
    let db = db();
    let mut state = main_phase();
    let balm = state.new_instance(cid(&db, "soothing_balm")); // gain 3 life
    push_spell(&mut state, balm, PlayerId(0), Vec::new());

    let state = resolve_top(&state, &db);

    assert!(
        find(
            &state,
            |e| matches!(e, GameEvent::LifeChanged { player, amount: 3 } if *player == PlayerId(0))
        )
        .is_some(),
        "life gain is a life_changed event"
    );
    assert!(
        !events(&state)
            .iter()
            .any(|e| matches!(e, GameEvent::DamageDealt { .. })),
        "life gain is not damage"
    );
    assert_eq!(state.players[0].life, 23, "20 + 3");
}

// ----- Death detection is creature-only (review P2 #3) -----

#[test]
fn killing_an_enchanted_creature_logs_only_the_creature_death_not_the_aura() {
    // The orphaned-Aura state-based action (CR 704.5m) moves the Aura to the
    // graveyard when its host dies, but that is a zone change, not a death: only the
    // creature produces `permanent_died`.
    let db = db();
    let mut state = main_phase();
    let host = place(&mut state, &db, "thornback_boar", 4); // 3/2 + Aura = 5/4, 4 marked = lethal
    let aura = place(&mut state, &db, "ironbark_aegis", 0); // +2/+2 Aura
    if let Some(perm) = state.battlefield.iter_mut().find(|p| p.id == aura) {
        perm.attached_to = Some(host);
    }

    // A single pass runs the state-based-actions loop, which kills the host and then
    // moves its orphaned Aura.
    let state = pass(&state, &db);

    assert!(
        !state.battlefield.iter().any(|p| p.id == host),
        "the host died"
    );
    assert!(
        !state.battlefield.iter().any(|p| p.id == aura),
        "the Aura followed its host"
    );
    let deaths: Vec<&GameEvent> = events(&state)
        .into_iter()
        .filter(|e| matches!(e, GameEvent::PermanentDied { .. }))
        .collect();
    assert_eq!(deaths.len(), 1, "exactly one death is logged, not the Aura");
    assert!(
        matches!(deaths[0], GameEvent::PermanentDied { permanent } if permanent.permanent == host),
        "the logged death is the creature, carrying its own identity"
    );
}

// ----- Ordering: a step is logged before the consequences of entering it (P2 #4) -----

#[test]
fn entering_the_draw_step_is_logged_before_the_draw_it_causes() {
    let db = db();
    // Turn 2 so the draw is not skipped (CR 103.8b); player 1 is active at upkeep.
    let mut state = GameState::new_two_player();
    state.turn = 2;
    state.active_player = PlayerId(1);
    state.priority = PlayerId(1);
    state.step = Step::Upkeep;
    state.players[1].library = vec![state.new_instance(cid(&db, "forest"))];

    // Both pass in upkeep, advancing into the draw step and performing its draw.
    let state = resolve_top(&state, &db);

    let step = find(&state, |e| {
        matches!(
            e,
            GameEvent::StepChanged {
                step: Step::Draw,
                ..
            }
        )
    })
    .expect("entering the draw step is logged");
    let drew = find(
        &state,
        |e| matches!(e, GameEvent::CardsDrawn { player, .. } if *player == PlayerId(1)),
    )
    .expect("the turn draw is logged");
    assert!(step < drew, "step_changed:draw precedes cards_drawn");
}

#[test]
fn entering_combat_damage_is_logged_before_the_damage_and_death() {
    let db = db();
    let mut state = GameState::new_two_player();
    state.step = Step::DeclareBlockers;
    state.active_player = PlayerId(0);
    state.priority = PlayerId(0);
    state.attackers_declared = true;
    state.blockers_declared = true;
    // An unblocked 3/2 attacker owned by player 0.
    let attacker = place(&mut state, &db, "thornback_boar", 0);
    if let Some(perm) = state.battlefield.iter_mut().find(|p| p.id == attacker) {
        perm.attacking = Some(PlayerId(1));
        perm.tapped = true;
    }

    // Both pass: advance into combat damage, which deals the attacker's damage.
    let state = resolve_top(&state, &db);

    let step = find(&state, |e| {
        matches!(
            e,
            GameEvent::StepChanged {
                step: Step::CombatDamage,
                ..
            }
        )
    })
    .expect("entering combat damage is logged");
    let damaged = find(&state, |e| {
        matches!(e, GameEvent::DamageDealt { target: DamageTarget::Player(p), .. } if *p == PlayerId(1))
    })
    .expect("the unblocked attacker's damage to the defender is logged");
    assert!(
        step < damaged,
        "step_changed:combat_damage precedes its damage"
    );
    assert_eq!(state.players[1].life, 17, "3 combat damage to the defender");
}

// ----- Mulligan decisions -----

#[test]
fn a_mulligan_then_keep_are_logged() {
    use rune_engine::GameSetup;
    let db = db();
    let deck: Vec<CardId> = (0..40).map(|_| cid(&db, "forest")).collect();
    let setup = GameSetup::two_player(deck.clone(), deck, 0x1234);
    let state = GameState::new(&setup, &db).expect("a valid setup");

    let after_mull = apply_action(&state, &Action::Mulligan, &db);
    assert!(
        find(&after_mull, |e| matches!(e, GameEvent::Mulligan { .. })).is_some(),
        "a mulligan is logged"
    );

    let after_keep = apply_action(
        &after_mull,
        &Action::Keep {
            bottom: vec![Target::Card(after_mull.players[0].hand[0].id)],
        },
        &db,
    );
    assert!(
        find(&after_keep, |e| matches!(e, GameEvent::HandKept { .. })).is_some(),
        "keeping the opening hand is logged"
    );
}
