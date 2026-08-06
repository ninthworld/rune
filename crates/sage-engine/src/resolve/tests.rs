//! Stack-resolution tests: target re-checking, fizzling, and final zones.

#![allow(clippy::unwrap_used)]

use super::*;
use crate::actions::Action;
use crate::apply_action;
use crate::fixtures::{fixture, id_in};
use crate::id::{CardId, CardInstance, PlayerId};
use crate::mana::Color;
use crate::phase::Step;
use crate::stack::StackId;

/// The bundled card database, for tests that need oracle data.
fn db() -> CardDatabase {
    CardDatabase::bundled().unwrap()
}

/// A two-player game in the precombat main phase with player 0 holding a
/// Forest and Llanowar Elves, and one card to draw in the library. Each card
/// is a freshly minted [`CardInstance`] so copies stay distinguishable.
fn slice_state() -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    let forest = state.new_instance(fixture("forest"));
    let scout = state.new_instance(fixture("llanowar_elves"));
    let draw = state.new_instance(fixture("onakke_ogre"));
    state.players[0].hand = vec![forest, scout];
    state.players[0].library = vec![draw];
    state
}

/// The first hand instance in `seat`'s hand whose printed card is `card`.
fn hand_instance(state: &GameState, seat: usize, card: CardId) -> CardInstance {
    *state.players[seat]
        .hand
        .iter()
        .find(|c| c.card == card)
        .unwrap()
}

#[test]
fn resolving_a_creature_spell_puts_it_on_the_battlefield() {
    let db = db();
    let mut state = slice_state();
    state.players[0].mana_pool.add(Color::Green, 1);
    let scout = hand_instance(&state, 0, fixture("llanowar_elves"));
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: scout,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    // The permanent that resolves carries the same instance the spell had.
    let perm = state
        .battlefield
        .iter()
        .find(|p| p.printed.card() == Some(fixture("llanowar_elves")))
        .unwrap();
    assert_eq!(perm.instance, scout.id);
}

#[test]
fn issue_47_non_permanent_spell_resolves_to_graveyard_not_battlefield() {
    // A resolving instant must never create a Permanent; it goes to its
    // owner's graveyard (CR 608.3 / 608.2m). The casting gate still only
    // offers creature casts (out of scope for #47), so we seed a synthetic
    // instant directly on the stack and drive resolution through the public
    // apply_action path (both players pass → the top of the stack resolves).
    let json = r#"[{"schema_version":1,"functional_id":"test_bolt","name":"Test Bolt","types":["instant"],"mana_cost":"{R}"}]"#;
    let db = CardDatabase::from_json(json).unwrap();

    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    let bolt = state.new_instance(id_in(&db, "test_bolt"));
    let sid = state.mint_id();
    state.stack.push(StackObject {
        paid: Default::default(),
        id: StackId(sid),
        controller: PlayerId(0),
        kind: StackObjectKind::Spell {
            card: bolt,
            mode: None,
            x: None,
        },
        targets: Vec::new(),
    });

    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    assert!(state.stack.is_empty());
    assert!(state.battlefield.is_empty());
    assert_eq!(state.players[0].graveyard, vec![bolt]);
}

/// Put a creature (Llanowar Elves, a 1/1) onto the battlefield under
/// player 0's control and return its fresh [`PermanentId`].
fn creature_on_battlefield(state: &mut GameState) -> PermanentId {
    let inst = state.new_instance(fixture("llanowar_elves"));
    let id = state.mint_id();
    state.battlefield.push(Permanent {
        id: PermanentId(id),
        instance: inst.id,
        printed: fixture("llanowar_elves").into(),
        controller: PlayerId(0),
        tapped: false,
        entered_turn: 0,
        attacking: None,
        blocking: Vec::new(),
        skips_untap: false,
        dealt_damage: false,
        damage: 0,
        counters: Default::default(),
        attached_to: None,
        chosen_color: None,
        named_card: None,
    });
    PermanentId(id)
}

/// Push a "tap target creature" ability onto the stack aimed at `target`,
/// with both players already having passed once so the next pass resolves it.
fn tap_ability_targeting(state: &mut GameState, source: PermanentId, target: Target) {
    let sid = state.mint_id();
    state.stack.push(StackObject {
        paid: Default::default(),
        id: StackId(sid),
        controller: PlayerId(0),
        kind: StackObjectKind::Ability {
            source: crate::stack::AbilitySource::Permanent(source),
            origin: AbilityOrigin::Activated,
            effects: vec![Effect::Tap {
                target: TargetSpec::AnyCreature,
            }],
        },
        targets: vec![target],
    });
}

#[test]
fn a_legal_target_resolves_onto_that_target() {
    // "Tap target creature" aimed at a creature still on the battlefield taps
    // exactly that creature.
    let db = db();
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    let creature = creature_on_battlefield(&mut state);
    tap_ability_targeting(&mut state, creature, Target::Permanent(creature));

    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    assert!(state.stack.is_empty());
    let perm = state.battlefield.iter().find(|p| p.id == creature).unwrap();
    assert!(perm.tapped);
}

#[test]
fn an_object_whose_target_became_illegal_fizzles() {
    // The chosen creature leaves the battlefield before the ability resolves.
    // With its only target now illegal the ability is removed from the stack
    // without effect (CR 608.2b) — nothing is tapped.
    let db = db();
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    let creature = creature_on_battlefield(&mut state);
    // A second, untargeted creature to prove resolution touches nothing.
    let bystander = creature_on_battlefield(&mut state);
    tap_ability_targeting(&mut state, creature, Target::Permanent(creature));

    // The targeted creature is gone by the time the ability would resolve.
    state.battlefield.retain(|p| p.id != creature);

    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    assert!(state.stack.is_empty());
    // The bystander was never a target and stays untapped: no effect happened.
    let perm = state
        .battlefield
        .iter()
        .find(|p| p.id == bystander)
        .unwrap();
    assert!(!perm.tapped);
}

#[test]
fn resolving_does_not_mutate_the_input_state() {
    // apply_action is pure: resolving a targeted ability leaves the input
    // state untouched (the tap lands only on the returned copy).
    let db = db();
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    let creature = creature_on_battlefield(&mut state);
    tap_ability_targeting(&mut state, creature, Target::Permanent(creature));
    let state = apply_action(&state, &Action::PassPriority, &db);

    // One pass remains before resolution, so the input still has the ability
    // on its stack and an untapped creature.
    let snapshot = state.clone();
    let _after = apply_action(&state, &Action::PassPriority, &db);
    assert_eq!(state, snapshot);
}

#[test]
fn target_legality_tracks_current_state() {
    // The predicate re-derives legality from whatever state it is handed.
    let db = db();
    let mut state = GameState::new_two_player();
    let creature = creature_on_battlefield(&mut state);
    let target = Target::Permanent(creature);

    // Legal while the creature is present…
    assert!(target_is_legal(
        TargetSpec::AnyCreature,
        target,
        &state,
        PlayerId(0),
        &db
    ));
    assert!(target_is_legal(
        TargetSpec::AnyPermanent,
        target,
        &state,
        PlayerId(0),
        &db
    ));
    // …a player is a legal AnyPlayer target, but not an AnyCreature one.
    assert!(target_is_legal(
        TargetSpec::AnyPlayer,
        Target::Player(PlayerId(1)),
        &state,
        PlayerId(0),
        &db
    ));
    assert!(!target_is_legal(
        TargetSpec::AnyCreature,
        Target::Player(PlayerId(1)),
        &state,
        PlayerId(0),
        &db
    ));

    // …and illegal once it is gone.
    state.battlefield.clear();
    assert!(!target_is_legal(
        TargetSpec::AnyCreature,
        target,
        &state,
        PlayerId(0),
        &db
    ));
}

#[test]
fn issue_148_spell_on_stack_target_is_legal_only_while_the_spell_is_on_the_stack() {
    // CR 701.5: a "counter target spell" target is legal while that exact spell
    // is on the stack and illegal once it has left (resolved/countered). An
    // ability on the stack is not a spell and is never a legal target.
    let db = db();
    let mut state = GameState::new_two_player();
    let spell = state.new_instance(fixture("onakke_ogre"));
    let sid = StackId(state.mint_id());
    state.stack.push(StackObject {
        paid: Default::default(),
        id: sid,
        controller: PlayerId(0),
        kind: StackObjectKind::Spell {
            card: spell,
            mode: None,
            x: None,
        },
        targets: Vec::new(),
    });
    // An ability sharing the stack is not a spell target.
    let aid = StackId(state.mint_id());
    state.stack.push(StackObject {
        paid: Default::default(),
        id: aid,
        controller: PlayerId(0),
        kind: StackObjectKind::Ability {
            source: crate::stack::AbilitySource::Permanent(crate::id::PermanentId(1)),
            origin: AbilityOrigin::Activated,
            effects: vec![Effect::DrawCard { count: 1 }],
        },
        targets: Vec::new(),
    });

    assert!(target_is_legal(
        TargetSpec::SpellOnStack,
        Target::Spell(sid),
        &state,
        PlayerId(0),
        &db
    ));
    assert!(
        !target_is_legal(
            TargetSpec::SpellOnStack,
            Target::Spell(aid),
            &state,
            PlayerId(0),
            &db
        ),
        "an ability on the stack is not a spell"
    );

    // Once the spell leaves the stack it is no longer a legal target.
    state.stack.retain(|o| o.id != sid);
    assert!(!target_is_legal(
        TargetSpec::SpellOnStack,
        Target::Spell(sid),
        &state,
        PlayerId(0),
        &db
    ));
}

#[test]
fn issue_149_any_target_is_legal_for_creatures_and_in_game_players() {
    // CR 115.4: an "any target" is a creature or an in-game player. A player
    // who has left the game and a non-creature permanent are both illegal.
    let db = db();
    let mut state = GameState::new_two_player();
    let creature = creature_on_battlefield(&mut state);
    assert!(target_is_legal(
        TargetSpec::AnyTarget,
        Target::Permanent(creature),
        &state,
        PlayerId(0),
        &db
    ));
    assert!(target_is_legal(
        TargetSpec::AnyTarget,
        Target::Player(PlayerId(0)),
        &state,
        PlayerId(0),
        &db
    ));

    // A non-creature permanent (a Forest) is not an "any target".
    let inst = state.new_instance(fixture("forest"));
    let forest = PermanentId(state.mint_id());
    state.battlefield.push(Permanent {
        id: forest,
        instance: inst.id,
        printed: fixture("forest").into(),
        controller: PlayerId(0),
        tapped: false,
        entered_turn: 0,
        attacking: None,
        blocking: Vec::new(),
        skips_untap: false,
        dealt_damage: false,
        damage: 0,
        counters: Default::default(),
        attached_to: None,
        chosen_color: None,
        named_card: None,
    });
    assert!(!target_is_legal(
        TargetSpec::AnyTarget,
        Target::Permanent(forest),
        &state,
        PlayerId(0),
        &db
    ));

    // A player who has lost is no longer a legal target.
    state.players[1].has_lost = true;
    assert!(!target_is_legal(
        TargetSpec::AnyTarget,
        Target::Player(PlayerId(1)),
        &state,
        PlayerId(0),
        &db
    ));
}

#[test]
fn issue_149_put_counters_ability_lands_on_its_target_cr_122() {
    // The PutCounters verb runs through the *ability* resolution path exactly
    // as it does through a spell: a "+1/+1 counter on target creature" ability
    // adds one counter to the chosen creature.
    use crate::state::CounterKind;
    let db = db();
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    let creature = creature_on_battlefield(&mut state);
    let sid = state.mint_id();
    state.stack.push(StackObject {
        paid: Default::default(),
        id: StackId(sid),
        controller: PlayerId(0),
        kind: StackObjectKind::Ability {
            source: crate::stack::AbilitySource::Permanent(creature),
            origin: AbilityOrigin::Activated,
            effects: vec![Effect::PutCounters {
                targets: crate::ability::TargetCount::Exactly(1),
                target: TargetSpec::AnyCreature,
                counter: CounterKind::PlusOnePlusOne,
                count: 1,
            }],
        },
        targets: vec![Target::Permanent(creature)],
    });

    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    let perm = state.battlefield.iter().find(|p| p.id == creature).unwrap();
    assert_eq!(perm.counter_count(CounterKind::PlusOnePlusOne), 1);
}

#[test]
fn a_possessive_target_spec_means_a_different_set_from_each_seat() {
    // "Target creature you control" is authored once and read from whichever seat
    // is choosing, so the same spec must name player 0's creatures for player 0
    // and player 1's for player 1. Getting this wrong would let a card aimed at
    // "a creature you control" hit an opponent's board.
    let db = db();
    let mut state = GameState::new_two_player();
    let mine = creature_on_battlefield(&mut state);
    let theirs = creature_on_battlefield(&mut state);
    state
        .battlefield
        .iter_mut()
        .filter(|p| p.id == theirs)
        .for_each(|p| p.controller = PlayerId(1));

    for (spec, from_zero, from_one) in [
        (TargetSpec::AnyCreatureYouControl, mine, theirs),
        (TargetSpec::AnyCreatureAnOpponentControls, theirs, mine),
    ] {
        assert!(target_is_legal(
            spec,
            Target::Permanent(from_zero),
            &state,
            PlayerId(0),
            &db
        ));
        assert!(!target_is_legal(
            spec,
            Target::Permanent(from_one),
            &state,
            PlayerId(0),
            &db
        ));
        // The mirror image holds from the other seat.
        assert!(target_is_legal(
            spec,
            Target::Permanent(from_one),
            &state,
            PlayerId(1),
            &db
        ));
    }

    // "Target opponent" likewise excludes the chooser's own seat.
    assert!(target_is_legal(
        TargetSpec::AnyOpponent,
        Target::Player(PlayerId(1)),
        &state,
        PlayerId(0),
        &db
    ));
    assert!(!target_is_legal(
        TargetSpec::AnyOpponent,
        Target::Player(PlayerId(0)),
        &state,
        PlayerId(0),
        &db
    ));
    // An opponent who has left the game is no longer a legal target either.
    state.players[1].has_lost = true;
    assert!(!target_is_legal(
        TargetSpec::AnyOpponent,
        Target::Player(PlayerId(1)),
        &state,
        PlayerId(0),
        &db
    ));
    assert!(!target_is_legal(
        TargetSpec::AnyCreatureAnOpponentControls,
        Target::Permanent(theirs),
        &state,
        PlayerId(0),
        &db
    ));
}

#[test]
fn type_scoped_target_specs_admit_exactly_their_own_class() {
    // Each spec is checked against a permanent of every class it might be confused
    // with, so an arm that reads the wrong type is caught by the *rejection* rather
    // than only by the acceptance.
    let db = db();
    let mut state = GameState::new_two_player();
    let creature = creature_on_battlefield(&mut state); // llanowar_elves
    let land = place(&mut state, "forest");
    let artifact = place(&mut state, "millstone");

    let cases = [
        (TargetSpec::AnyCreature, creature),
        (TargetSpec::AnyLand, land),
        (TargetSpec::AnyArtifact, artifact),
        (TargetSpec::AnyArtifactOrEnchantment, artifact),
        (TargetSpec::AnyNonlandPermanent, creature),
    ];
    for (spec, legal) in cases {
        assert!(
            target_is_legal(spec, Target::Permanent(legal), &state, PlayerId(0), &db),
            "{spec:?} accepts its own class"
        );
    }
    for (spec, illegal) in [
        (TargetSpec::AnyCreature, land),
        (TargetSpec::AnyLand, creature),
        (TargetSpec::AnyArtifact, creature),
        (TargetSpec::AnyArtifactOrEnchantment, land),
        (TargetSpec::AnyNonlandPermanent, land),
        (TargetSpec::AnyEnchantment, artifact),
    ] {
        assert!(
            !target_is_legal(spec, Target::Permanent(illegal), &state, PlayerId(0), &db),
            "{spec:?} rejects the wrong class"
        );
    }

    // A tapped-creature target tracks the tap state, not a printed characteristic.
    assert!(!target_is_legal(
        TargetSpec::AnyTappedCreature,
        Target::Permanent(creature),
        &state,
        PlayerId(0),
        &db
    ));
    state
        .battlefield
        .iter_mut()
        .filter(|p| p.id == creature)
        .for_each(|p| p.tapped = true);
    assert!(target_is_legal(
        TargetSpec::AnyTappedCreature,
        Target::Permanent(creature),
        &state,
        PlayerId(0),
        &db
    ));
    // A tapped *land* is still not a creature.
    state
        .battlefield
        .iter_mut()
        .filter(|p| p.id == land)
        .for_each(|p| p.tapped = true);
    assert!(!target_is_legal(
        TargetSpec::AnyTappedCreature,
        Target::Permanent(land),
        &state,
        PlayerId(0),
        &db
    ));
}

/// Put a permanent of the bundled card `slug` onto the battlefield under player 0.
fn place(state: &mut GameState, slug: &str) -> PermanentId {
    let card = fixture(slug);
    let inst = state.new_instance(card);
    let id = PermanentId(state.mint_id());
    state.battlefield.push(Permanent {
        id,
        instance: inst.id,
        printed: card.into(),
        controller: PlayerId(0),
        tapped: false,
        entered_turn: 0,
        attacking: None,
        blocking: Vec::new(),
        skips_untap: false,
        dealt_damage: false,
        damage: 0,
        counters: Default::default(),
        attached_to: None,
        chosen_color: None,
        named_card: None,
    });
    id
}

// ----- Auras: enchant, attachment, and fizzle (issue #152) -----
//
// P/T Auras have no clean M19 representative, so these tests build an inline
// catalog (ADR 0009): `test_aegis` ({1}{G} Aura, "+2/+2, enchant creature") and a
// 1/1 `test_scout` host, named by authored identity rather than interned handle.
fn aura_db() -> CardDatabase {
    let json = r#"[
        {"schema_version":1,"functional_id":"test_aegis","name":"Test Aegis",
         "types":["enchantment"],"subtypes":["Aura"],"mana_cost":"{1}{G}","colors":["green"],
         "attachment":{"kind":"aura","attach_to":"any_creature","power":2,"toughness":2}},
        {"schema_version":1,"functional_id":"test_scout","name":"Test Scout",
         "types":["creature"],"subtypes":["Elf"],"mana_cost":"{G}","colors":["green"],
         "power":1,"toughness":1}
    ]"#;
    CardDatabase::from_json(json).unwrap()
}

/// Put the 1/1 `test_scout` host from `db` onto the battlefield under player 0.
fn aura_host(state: &mut GameState, db: &CardDatabase) -> PermanentId {
    let inst = state.new_instance(id_in(db, "test_scout"));
    let id = PermanentId(state.mint_id());
    state.battlefield.push(Permanent {
        id,
        instance: inst.id,
        printed: id_in(db, "test_scout").into(),
        controller: PlayerId(0),
        tapped: false,
        entered_turn: 0,
        attacking: None,
        blocking: Vec::new(),
        skips_untap: false,
        dealt_damage: false,
        damage: 0,
        counters: Default::default(),
        attached_to: None,
        chosen_color: None,
        named_card: None,
    });
    id
}

#[test]
fn issue_152_aura_resolves_attached_to_its_target_and_boosts_it_cr_303_4d() {
    // CR 303.4d: a resolving Aura enters the battlefield attached to the object
    // its enchant ability chose, and its +2/+2 grant folds into the host's
    // current P/T (CR 613.7c). The host is a printed 1/1 -> 3/3 enchanted.
    use crate::characteristics::characteristics;
    let db = aura_db();
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    let host = aura_host(&mut state, &db); // test_scout, 1/1
    let aura = state.new_instance(id_in(&db, "test_aegis"));
    state.players[0].hand = vec![aura];
    state.players[0].mana_pool.add(Color::Green, 1);
    state.players[0].mana_pool.colorless = 1;

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: aura,
            mode: None,
            x: None,
            targets: vec![Target::Permanent(host)],
            payment: Vec::new(),
        },
        &db,
    );
    // Both players pass: the Aura resolves onto the battlefield attached.
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    let aura_perm = state
        .battlefield
        .iter()
        .find(|p| p.printed.card() == Some(id_in(&db, "test_aegis")))
        .unwrap();
    assert_eq!(
        aura_perm.attached_to,
        Some(host),
        "the Aura entered attached to its cast-time target (CR 303.4d)"
    );
    let ch = characteristics(&state, host, &db);
    assert_eq!(ch.power, Some(3), "printed 1 + Aura's +2");
    assert_eq!(ch.toughness, Some(3));
}

#[test]
fn issue_152_aura_fizzles_when_its_target_left_before_resolution_cr_608_2b() {
    // CR 608.2b: with its only target (the enchant object) now illegal, the Aura
    // spell is removed from the stack without resolving — it never enters the
    // battlefield, and (a card that failed to resolve) goes to the graveyard.
    let db = aura_db();
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    let host = aura_host(&mut state, &db);
    let aura = state.new_instance(id_in(&db, "test_aegis"));
    state.players[0].hand = vec![aura];
    state.players[0].mana_pool.add(Color::Green, 1);
    state.players[0].mana_pool.colorless = 1;

    let mut state = apply_action(
        &state,
        &Action::CastSpell {
            card: aura,
            mode: None,
            x: None,
            targets: vec![Target::Permanent(host)],
            payment: Vec::new(),
        },
        &db,
    );
    // The chosen host is killed in response, before the Aura resolves.
    state.battlefield.retain(|p| p.id != host);

    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    assert!(state.stack.is_empty());
    assert!(
        !state
            .battlefield
            .iter()
            .any(|p| p.printed.card() == Some(id_in(&db, "test_aegis"))),
        "a fizzled Aura never enters the battlefield (CR 608.2b)"
    );
    assert!(
        state.players[0].graveyard.iter().any(|c| c.id == aura.id),
        "the fizzled Aura spell goes to its owner's graveyard"
    );
}

// ----- ETB replacements: enters with counters (issue #155) -----

#[test]
fn issue_155_zero_zero_entering_with_two_counters_lives_cr_614_12() {
    // CR 614.12 / 704.5f: a 0/0 that enters with two +1/+1 counters has the
    // counters as part of *entering* — so it is a 2/2 by the time the SBA loop
    // runs and is never put into the graveyard for 0 toughness. No clean M19 card
    // enters with a fixed number of counters, so `test_hatchling` (an inline 0/0
    // that enters with two +1/+1 counters, {1}{G}) exercises it (ADR 0009).
    use crate::characteristics::characteristics;
    use crate::state::CounterKind;
    let json = r#"[{"schema_version":1,"functional_id":"test_hatchling","name":"Test Hatchling",
        "types":["creature"],"subtypes":["Insect"],"mana_cost":"{1}{G}","colors":["green"],
        "power":0,"toughness":0,
        "abilities":[{"type":"enters_with_counters","counter":"plus_one_plus_one","count":2}]}]"#;
    let db = CardDatabase::from_json(json).unwrap();
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    let hatchling = state.new_instance(id_in(&db, "test_hatchling"));
    state.players[0].hand = vec![hatchling];
    // Pay {1}{G}.
    state.players[0].mana_pool.add(Color::Green, 1);
    state.players[0].mana_pool.colorless = 1;

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: hatchling,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    // Both players pass: the creature resolves onto the battlefield. The SBA loop
    // runs in this same transition; the counters must already be present.
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    let perm = state
        .battlefield
        .iter()
        .find(|p| p.printed.card() == Some(id_in(&db, "test_hatchling")))
        .unwrap(); // the 0/0 survives entry with two +1/+1 counters (CR 614.12/704.5f)
    assert_eq!(perm.counter_count(CounterKind::PlusOnePlusOne), 2);
    let ch = characteristics(&state, perm.id, &db);
    assert_eq!(ch.power, Some(2), "0 base + two +1/+1 counters");
    assert_eq!(ch.toughness, Some(2));
    assert!(
        state.players[0].graveyard.is_empty(),
        "no 0-toughness state-based death — it entered as a 2/2"
    );
}

#[test]
fn issue_155_etb_trigger_observes_the_replaced_counters_state_cr_614_12() {
    // CR 614.12: a co-entering enters-the-battlefield trigger observes the replaced
    // state. A synthetic creature that both enters with two +1/+1 counters and has
    // an ETB "draw a card" trigger: on resolution it is already a 2/2 with the two
    // counters AND its ETB trigger is on the stack — both from the one entry event.
    use crate::state::CounterKind;
    let json = r#"[{"schema_version":1,"functional_id":"test_broodling","name":"Test Broodling","types":["creature"],"mana_cost":"","power":0,"toughness":0,"abilities":[{"type":"enters_with_counters","counter":"plus_one_plus_one","count":2},{"type":"triggered","event":"self_enters_battlefield","effects":[{"kind":"draw_card","count":1}]}]}]"#;
    let db = CardDatabase::from_json(json).unwrap();

    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    let broodling = state.new_instance(id_in(&db, "test_broodling"));
    // A card to draw so the ETB trigger's effect has something to fetch.
    let draw = state.new_instance(id_in(&db, "test_broodling"));
    state.players[0].library = vec![draw];
    let sid = state.mint_id();
    state.stack.push(StackObject {
        paid: Default::default(),
        id: StackId(sid),
        controller: PlayerId(0),
        kind: StackObjectKind::Spell {
            card: broodling,
            mode: None,
            x: None,
        },
        targets: Vec::new(),
    });

    // Both players pass: the spell resolves — the permanent enters with its
    // counters (the replacement) and its ETB trigger is collected onto the stack.
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    let perm = state
        .battlefield
        .iter()
        .find(|p| p.printed.card() == Some(id_in(&db, "test_broodling")))
        .unwrap(); // the creature entered the battlefield
    assert_eq!(
        perm.counter_count(CounterKind::PlusOnePlusOne),
        2,
        "the replaced 'enters with counters' state is present"
    );
    assert_eq!(
        state.stack.len(),
        1,
        "the co-entering ETB trigger was collected against the replaced state"
    );
    assert!(matches!(
        state.stack[0].kind,
        StackObjectKind::Ability { .. }
    ));
}
