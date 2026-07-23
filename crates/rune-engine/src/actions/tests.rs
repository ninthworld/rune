#![allow(clippy::unwrap_used)]

use super::definition::{Action, Attack};
use super::generation::valid_actions;
use super::targeting::{legal_targets_for_spec, target_requirements};

use crate::ability::{Target, TargetSpec};
use crate::apply_action;
use crate::fixtures::{fixture, id_in};
use crate::id::{CardId, CardInstance, CardInstanceId, PermanentId, PlayerId};
use crate::mana::{Color, ManaPool};
use crate::phase::Step;
use crate::stack::{StackId, StackObject, StackObjectKind};
use crate::state::{GameState, Permanent};
use crate::CardDatabase;

/// The bundled card database, for tests that need oracle data.
fn db() -> CardDatabase {
    CardDatabase::bundled().unwrap()
}

/// A database with a "Tapper" artifact whose activated ability is
/// `{T}: Tap target creature.` and a vanilla "Bear" creature to target.
fn targeting_db() -> CardDatabase {
    let json = r#"[
        {"schema_version":1,"functional_id":"tapper","name":"Tapper","types":["artifact"],"mana_cost":"",
         "abilities":[{"type":"activated","cost":[{"kind":"tap"}],
                      "effects":[{"kind":"tap","target":"any_creature"}]}]},
        {"schema_version":1,"functional_id":"bear","name":"Bear","types":["creature"],"mana_cost":"",
         "power":2,"toughness":2}
    ]"#;
    CardDatabase::from_json(json).unwrap()
}

/// Put a permanent of printed card `card` onto the battlefield under player
/// 0's control (untapped) and return its fresh [`PermanentId`].
fn put_on_battlefield(state: &mut GameState, card: CardId) -> PermanentId {
    let inst = state.new_instance(card);
    let id = state.mint_id();
    state.battlefield.push(Permanent {
        id: PermanentId(id),
        instance: inst.id,
        card,
        controller: PlayerId(0),
        tapped: false,
        entered_turn: 0,
        attacking: None,
        blocking: None,
        damage: 0,
        counters: Default::default(),
        attached_to: None,
    });
    PermanentId(id)
}

/// A two-player game at precombat main with a Tapper and `creatures` Bears on
/// the battlefield under player 0. Returns the state, the Tapper's id, and the
/// Bears' ids.
fn tapper_and_creatures(creatures: usize) -> (GameState, PermanentId, Vec<PermanentId>) {
    let db = targeting_db();
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    let tapper = put_on_battlefield(&mut state, id_in(&db, "tapper"));
    let bears = (0..creatures)
        .map(|_| put_on_battlefield(&mut state, id_in(&db, "bear")))
        .collect();
    (state, tapper, bears)
}

#[test]
fn valid_actions_offers_pass_priority_to_the_priority_holder() {
    let state = GameState::new_two_player();
    // Pass plus the always-available concede (CR 104.3a).
    assert_eq!(
        valid_actions(&state, &db()),
        vec![Action::PassPriority, Action::Concede]
    );
}

#[test]
fn issue_119_concede_is_always_offered_cr_104_3a() {
    // CR 104.3a: a player may concede at any time. Concede is offered to the
    // acting seat in every window the engine surfaces a choice — a normal
    // priority round and the special turn-based choices alike.
    let db = db();

    // Normal priority round (untap).
    let state = GameState::new_two_player();
    assert!(valid_actions(&state, &db).contains(&Action::Concede));

    // Declare-attackers window: only the declaration and concede are offered.
    let mut combat = GameState::new_two_player();
    combat.turn = 2;
    combat.step = Step::DeclareAttackers;
    let offered = valid_actions(&combat, &db);
    assert!(offered.contains(&Action::Concede));
    assert!(!offered.contains(&Action::PassPriority));
}

#[test]
fn issue_119_terminal_state_offers_no_actions_cr_104_2a() {
    // CR 104.2a: once a player has lost and one remains, the game is over and
    // no actions are legal — not even concede.
    let mut state = GameState::new_two_player();
    state.players[1].has_lost = true;
    assert!(state.is_over());
    assert!(valid_actions(&state, &db()).is_empty());
}

#[test]
fn valid_actions_on_seatless_state_is_empty() {
    // Default has no players, so no one holds priority and nothing is legal.
    assert!(valid_actions(&GameState::default(), &db()).is_empty());
}

#[test]
fn targeted_ability_is_advertised_once_with_an_o_n_candidate_set() {
    // A "tap target creature" ability over N creatures is advertised as a
    // SINGLE ActivateAbility (requirement form, no targets), plus one target
    // slot whose candidate set lists all N creatures. The action count is
    // O(1) in the creatures and the candidate count is O(N) — never the O(N^k)
    // cartesian product a per-combination enumeration would produce.
    let db = targeting_db();
    let n = 5;
    let (state, tapper, bears) = tapper_and_creatures(n);

    let actions = valid_actions(&state, &db);
    let activations: Vec<&Action> = actions
        .iter()
        .filter(|a| matches!(a, Action::ActivateAbility { .. }))
        .collect();
    assert_eq!(activations.len(), 1, "one action, not one per target");
    assert_eq!(
        activations[0],
        &Action::ActivateAbility {
            permanent: tapper,
            index: 0,
            targets: Vec::new(),
        }
    );

    let reqs = target_requirements(&state, &db, activations[0]);
    assert_eq!(reqs.len(), 1, "one target slot");
    assert_eq!(reqs[0].spec, TargetSpec::AnyCreature);
    assert_eq!(reqs[0].candidates.len(), n, "O(N) candidates for the slot");
    for bear in &bears {
        assert!(reqs[0].candidates.contains(&Target::Permanent(*bear)));
    }
    // The Tapper is a permanent but not a creature, so it is not a candidate.
    assert!(!reqs[0].candidates.contains(&Target::Permanent(tapper)));
}

#[test]
fn a_legal_target_is_accepted_and_carried_onto_the_stack() {
    // Activating with a legal creature target puts the ability on the stack
    // carrying exactly that chosen target (CR 601.2c), and resolving it taps
    // that creature.
    let db = targeting_db();
    let (state, tapper, bears) = tapper_and_creatures(2);
    let target = Target::Permanent(bears[0]);

    let after = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: tapper,
            index: 0,
            targets: vec![target],
        },
        &db,
    );

    // The ability is on the stack with the chosen target recorded on it.
    assert_eq!(after.stack.len(), 1);
    assert!(matches!(
        after.stack[0].kind,
        StackObjectKind::Ability { source, .. } if source == tapper
    ));
    assert_eq!(after.stack[0].targets, vec![target]);

    // Resolving (both players pass) taps exactly the targeted creature.
    let after = apply_action(&after, &Action::PassPriority, &db);
    let after = apply_action(&after, &Action::PassPriority, &db);
    assert!(after.stack.is_empty());
    assert!(
        after
            .battlefield
            .iter()
            .find(|p| p.id == bears[0])
            .unwrap()
            .tapped
    );
    assert!(
        !after
            .battlefield
            .iter()
            .find(|p| p.id == bears[1])
            .unwrap()
            .tapped
    );
}

#[test]
fn an_illegal_target_makes_the_activation_a_no_op() {
    // A target of the right *kind* but outside the legal set — here the
    // Tapper itself, a permanent that is not a creature — is not in the freshly
    // computed legal set, so the whole activation is rejected as a no-op
    // (nothing on the stack, no cost paid), exactly as an illegal action.
    let db = targeting_db();
    let (state, tapper, _bears) = tapper_and_creatures(1);

    let after = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: tapper,
            index: 0,
            targets: vec![Target::Permanent(tapper)],
        },
        &db,
    );
    assert_eq!(after, state);
}

#[test]
fn a_stale_target_makes_the_activation_a_no_op() {
    // A target that named a real candidate which has since left the battlefield
    // is no longer in the legal set recomputed from current state, so the
    // activation is a no-op — the stale id can never rebind to a live object.
    let db = targeting_db();
    let (mut state, tapper, bears) = tapper_and_creatures(1);
    let stale = Target::Permanent(bears[0]);
    // The creature is gone by the time the activation is submitted.
    state.battlefield.retain(|p| p.id != bears[0]);

    let after = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: tapper,
            index: 0,
            targets: vec![stale],
        },
        &db,
    );
    assert_eq!(after, state);
}

#[test]
fn a_targeting_activation_with_no_target_is_a_no_op() {
    // The requirement form (no targets) is what `valid_actions` advertises,
    // but it is not a legal *submission* for an ability that requires a target:
    // the slot count must match, so an unfilled selection is rejected.
    let db = targeting_db();
    let (state, tapper, _bears) = tapper_and_creatures(1);

    let after = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: tapper,
            index: 0,
            targets: Vec::new(),
        },
        &db,
    );
    assert_eq!(after, state);
}

#[test]
fn a_non_targeting_ability_needs_no_targets_and_still_resolves() {
    // A mana ability declares no target specs, so its activation validates with
    // an empty selection and is unaffected by this machinery.
    let db = db();
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    let forest = put_on_battlefield(&mut state, fixture("forest"));

    assert!(target_requirements(
        &state,
        &db,
        &Action::ActivateAbility {
            permanent: forest,
            index: 0,
            targets: Vec::new(),
        },
    )
    .is_empty());

    let after = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: forest,
            index: 0,
            targets: Vec::new(),
        },
        &db,
    );
    assert_eq!(after.players[0].mana_pool.green, 1);
}

// ----- Cast every card type with real timing (issue #147) -----
//
// These tests exercise casting *timing* and stack mechanics per card type, so they
// must not depend on any bundled card's behavior. Bundled cards gain real effects
// over time (e.g. issue #256 wired the four formerly-functionless cards these once
// borrowed), which would silently change what a "generic instant" does — or, for a
// now-targeted spell, make it uncastable with no target. A minimal synthetic catalog
// gives one no-effect card of each castable type, immune to that drift.
fn probe_db() -> CardDatabase {
    let json = r#"[
        {"schema_version":1,"functional_id":"probe_instant","name":"Probe Instant","types":["instant"],"mana_cost":"{R}","colors":["red"]},
        {"schema_version":1,"functional_id":"probe_sorcery","name":"Probe Sorcery","types":["sorcery"],"mana_cost":"{U}","colors":["blue"]},
        {"schema_version":1,"functional_id":"probe_artifact","name":"Probe Artifact","types":["artifact"],"mana_cost":"{1}"},
        {"schema_version":1,"functional_id":"probe_enchantment","name":"Probe Enchantment","types":["enchantment"],"mana_cost":"{G}","colors":["green"]},
        {"schema_version":1,"functional_id":"probe_creature","name":"Probe Creature","types":["creature"],"mana_cost":"{G}","colors":["green"],"power":2,"toughness":2}
    ]"#;
    CardDatabase::from_json(json).unwrap()
}
fn instant_id(db: &CardDatabase) -> CardId {
    id_in(db, "probe_instant")
}
fn sorcery_id(db: &CardDatabase) -> CardId {
    id_in(db, "probe_sorcery")
}
fn artifact_id(db: &CardDatabase) -> CardId {
    id_in(db, "probe_artifact")
}
fn enchantment_id(db: &CardDatabase) -> CardId {
    id_in(db, "probe_enchantment")
}

/// Whether `card` is offered as a [`Action::CastSpell`] for the hand instance
/// `inst`.
fn cast_offered(state: &GameState, db: &CardDatabase, inst: CardInstance) -> bool {
    valid_actions(state, db).contains(&Action::CastSpell {
        card: inst,
        targets: Vec::new(),
    })
}

/// A two-player game at player 0's precombat main with a single copy of
/// `card` in player 0's hand and a mana pool generous enough to pay any of
/// the single-pip fixture costs. Returns the state and the hand instance.
fn hand_with(card: CardId) -> (GameState, CardInstance) {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    let inst = state.new_instance(card);
    state.players[0].hand = vec![inst];
    // One of each color plus colorless covers every fixture cost here.
    state.players[0].mana_pool.add(Color::Red, 1);
    state.players[0].mana_pool.add(Color::Blue, 1);
    state.players[0].mana_pool.add(Color::Green, 1);
    state.players[0].mana_pool.colorless = 1;
    (state, inst)
}

#[test]
fn issue_147_instant_castable_with_a_nonempty_stack_and_off_turn_cr_117_1a() {
    // CR 117.1a: a player may cast an instant any time they have priority —
    // including while another spell waits on the stack (the first "respond to
    // a spell" path) and during an opponent's turn.
    let db = probe_db();

    // Another object already on the stack, player 0 holding priority.
    let (mut mid_stack, bolt) = hand_with(instant_id(&db));
    let sid = mid_stack.mint_id();
    let other = mid_stack.new_instance(instant_id(&db));
    mid_stack.stack.push(StackObject {
        id: StackId(sid),
        controller: PlayerId(0),
        kind: StackObjectKind::Spell { card: other },
        targets: Vec::new(),
    });
    assert!(!mid_stack.stack.is_empty());
    assert!(
        cast_offered(&mid_stack, &db, bolt),
        "an instant is castable with a spell on the stack (CR 117.1a)"
    );

    // Opponent's turn: player 1 is active, player 0 holds priority.
    let (mut off_turn, bolt) = hand_with(instant_id(&db));
    off_turn.active_player = PlayerId(1);
    off_turn.priority = PlayerId(0);
    assert!(
        cast_offered(&off_turn, &db, bolt),
        "an instant is castable on the opponent's turn (CR 117.1a)"
    );
}

#[test]
fn issue_147_sorcery_not_offered_off_turn_or_mid_stack_cr_304_1() {
    // CR 304.1: a sorcery may be cast only at sorcery speed — the active
    // player, a main phase, an empty stack. It is offered in none of the
    // windows an instant is, only in that one.
    let db = probe_db();

    // Positive control: on-turn, empty stack, main phase — offered.
    let (on_turn, sorcery) = hand_with(sorcery_id(&db));
    assert!(cast_offered(&on_turn, &db, sorcery));

    // Off-turn (player 0 holds priority on player 1's turn) — not offered.
    let (mut off_turn, sorcery) = hand_with(sorcery_id(&db));
    off_turn.active_player = PlayerId(1);
    off_turn.priority = PlayerId(0);
    assert!(!cast_offered(&off_turn, &db, sorcery));

    // Mid-stack (own turn, but a spell is on the stack) — not offered.
    let (mut mid_stack, sorcery) = hand_with(sorcery_id(&db));
    let sid = mid_stack.mint_id();
    let other = mid_stack.new_instance(instant_id(&db));
    mid_stack.stack.push(StackObject {
        id: StackId(sid),
        controller: PlayerId(0),
        kind: StackObjectKind::Spell { card: other },
        targets: Vec::new(),
    });
    assert!(!cast_offered(&mid_stack, &db, sorcery));
}

#[test]
fn issue_147_artifact_and_enchantment_cast_at_sorcery_speed_and_enter_battlefield() {
    // CR 307.1 (enchantments) / artifacts are permanent spells cast at
    // sorcery speed; on resolution they enter the battlefield (CR 608.3).
    let db = probe_db();
    for card in [artifact_id(&db), enchantment_id(&db)] {
        let (state, inst) = hand_with(card);

        // Offered at sorcery speed...
        assert!(
            cast_offered(&state, &db, inst),
            "a permanent spell is castable at sorcery speed"
        );
        // ...but not while a spell is on the stack (sorcery-speed gate).
        let mut mid_stack = state.clone();
        let sid = mid_stack.mint_id();
        let other = mid_stack.new_instance(instant_id(&db));
        mid_stack.stack.push(StackObject {
            id: StackId(sid),
            controller: PlayerId(0),
            kind: StackObjectKind::Spell { card: other },
            targets: Vec::new(),
        });
        assert!(!cast_offered(&mid_stack, &db, inst));

        // Cast it, then both players pass: it resolves onto the battlefield.
        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: inst,
                targets: Vec::new(),
            },
            &db,
        );
        assert_eq!(state.stack.len(), 1);
        let state = apply_action(&state, &Action::PassPriority, &db);
        let state = apply_action(&state, &Action::PassPriority, &db);
        assert!(state.stack.is_empty());
        // The permanent spell entered the battlefield (CR 608.3).
        let perm = state.battlefield.iter().find(|p| p.card == card).unwrap();
        assert_eq!(perm.instance, inst.id, "keeps its instance identity");
    }
}

#[test]
fn issue_147_cast_instant_resolves_after_a_later_instant_lifo_cr_608_1() {
    // CR 608.1: the stack resolves last-in, first-out. Cast instant A, then —
    // with A still on the stack — cast instant B. B is on top, so it resolves
    // first: the two non-permanent spells reach the graveyard in the order
    // B, A, the reverse of the order they were cast.
    let db = probe_db();
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    let a = state.new_instance(instant_id(&db));
    let b = state.new_instance(instant_id(&db));
    state.players[0].hand = vec![a, b];
    state.players[0].mana_pool.add(Color::Red, 2);

    // Cast A, then B (legal to respond because both are instants).
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: a,
            targets: Vec::new(),
        },
        &db,
    );
    assert!(cast_offered(&state, &db, b));
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: b,
            targets: Vec::new(),
        },
        &db,
    );
    assert_eq!(state.stack.len(), 2);

    // Resolve the top (B), then the next (A).
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    assert!(state.stack.is_empty());

    // LIFO: the later-cast B resolved first, so it is first in the graveyard.
    let grave: Vec<CardInstanceId> = state.players[0].graveyard.iter().map(|c| c.id).collect();
    assert_eq!(grave, vec![b.id, a.id]);
}

#[test]
fn issue_147_unpayable_spells_are_never_offered() {
    // The unpayable-cost invariant extends to every new castable type: with an
    // empty pool, no instant/sorcery/artifact/enchantment is offered; with the
    // exact mana it is (sorcery-speed types on-turn with an empty stack).
    let db = probe_db();
    for card in [
        instant_id(&db),
        sorcery_id(&db),
        artifact_id(&db),
        enchantment_id(&db),
    ] {
        let (state, inst) = hand_with(card);

        // Drain the pool: nothing is payable, so nothing is offered.
        let mut broke = state.clone();
        broke.players[0].mana_pool = ManaPool::default();
        assert!(
            !cast_offered(&broke, &db, inst),
            "an unpayable spell is never offered"
        );
        // With mana in the pool it is offered.
        assert!(cast_offered(&state, &db, inst));
    }
}

#[test]
fn issue_152_aura_castable_only_with_a_legal_enchant_target_cr_303_4c() {
    // CR 303.4c/601.2c: an Aura is offered only when a legal object to enchant
    // exists — its enchant restriction is a target chosen at cast. With a
    // creature on the battlefield the Aura is castable; with none it is not
    // (its one slot has zero candidates), even though its sorcery-speed timing
    // and mana are satisfied. A vanilla (non-Aura) enchantment of the same cost
    // is always castable, proving the gate is the enchant target, not the type.
    let json = r#"[
        {"schema_version":1,"functional_id":"test_aura","name":"Test Aura","types":["enchantment"],"subtypes":["Aura"],
         "mana_cost":"{G}",
         "aura":{"enchant":"any_creature","power":1,"toughness":1}},
        {"schema_version":1,"functional_id":"test_charm","name":"Test Charm","types":["enchantment"],
         "mana_cost":"{G}"},
        {"schema_version":1,"functional_id":"test_bear","name":"Test Bear","types":["creature"],"mana_cost":"",
         "power":2,"toughness":2}
    ]"#;
    let db = CardDatabase::from_json(json).unwrap();

    // No creature to enchant: the Aura is not offered, the charm still is.
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    let aura = state.new_instance(id_in(&db, "test_aura"));
    let charm = state.new_instance(id_in(&db, "test_charm"));
    state.players[0].hand = vec![aura, charm];
    state.players[0].mana_pool.add(Color::Green, 2);
    assert!(
        !cast_offered(&state, &db, aura),
        "an Aura with no legal object to enchant is not offered (CR 303.4c)"
    );
    assert!(
        cast_offered(&state, &db, charm),
        "a non-Aura enchantment is castable with no creature present"
    );

    // Put a creature on the battlefield: now the Aura is offered, and its one
    // requirement slot is the enchant restriction listing that creature.
    let bear = put_on_battlefield(&mut state, id_in(&db, "test_bear"));
    assert!(
        cast_offered(&state, &db, aura),
        "an Aura is castable once a legal enchant target exists (CR 303.4c)"
    );
    let reqs = target_requirements(
        &state,
        &db,
        &Action::CastSpell {
            card: aura,
            targets: Vec::new(),
        },
    );
    assert_eq!(reqs.len(), 1, "the Aura's single enchant slot");
    assert_eq!(reqs[0].spec, TargetSpec::AnyCreature);
    assert_eq!(reqs[0].candidates, vec![Target::Permanent(bear)]);
}

// ----- Spell targets at cast + the first counterspell (issue #148) -----
//
// Cancel ({1}{U}{U} instant, "Counter target spell." — a
// `CounterSpell { SpellOnStack }` spell effect); a vanilla Onakke Ogre is the
// creature spell it counters.
fn counterspell_id() -> CardId {
    fixture("cancel")
}
fn creature_id() -> CardId {
    fixture("onakke_ogre")
}

/// A two-player game at player 0's precombat main with a Runic Negation in
/// hand and `{U}` in pool, and a creature spell (controlled by player 1) on
/// the stack for it to target. Returns the state, the counterspell hand
/// instance, and the creature spell's [`StackId`].
fn counterspell_over_a_creature_spell() -> (GameState, CardInstance, StackId) {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    let boar = state.new_instance(creature_id());
    let sid = StackId(state.mint_id());
    state.stack.push(StackObject {
        id: sid,
        controller: PlayerId(1),
        kind: StackObjectKind::Spell { card: boar },
        targets: Vec::new(),
    });
    let negation = state.new_instance(counterspell_id());
    state.players[0].hand = vec![negation];
    state.players[0].mana_pool.add(Color::Blue, 2);
    state.players[0].mana_pool.colorless = 1;
    (state, negation, sid)
}

#[test]
fn issue_148_targeted_cast_advertised_once_with_the_spell_on_stack_as_a_candidate() {
    // A "counter target spell" cast is offered once in its requirement form
    // (empty targets), and its single slot lists exactly the spell on the
    // stack (CR 601.2c / 701.5).
    let db = db();
    let (state, negation, sid) = counterspell_over_a_creature_spell();

    let cast = Action::CastSpell {
        card: negation,
        targets: Vec::new(),
    };
    assert!(valid_actions(&state, &db).contains(&cast));

    let reqs = target_requirements(&state, &db, &cast);
    assert_eq!(reqs.len(), 1, "one target slot");
    assert_eq!(reqs[0].spec, TargetSpec::SpellOnStack);
    assert_eq!(reqs[0].candidates, vec![Target::Spell(sid)]);
}

#[test]
fn issue_148_targeted_cast_with_no_legal_candidate_is_not_offered() {
    // CR 601.2c: with no spell on the stack the counterspell's only slot has
    // zero candidates, so `valid_actions` never offers the cast.
    let db = db();
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    let negation = state.new_instance(counterspell_id());
    state.players[0].hand = vec![negation];
    state.players[0].mana_pool.add(Color::Blue, 2);
    state.players[0].mana_pool.colorless = 1;

    assert!(
        !valid_actions(&state, &db)
            .iter()
            .any(|a| matches!(a, Action::CastSpell { .. })),
        "a targeted cast with zero legal candidates is never offered"
    );
}

#[test]
fn issue_148_counterspell_cannot_target_an_ability_on_the_stack_cr_605_3() {
    // Only spells are SpellOnStack candidates: an ability on the stack is not a
    // spell, and a mana ability never uses the stack at all (CR 605.3). With
    // only an ability on the stack the counterspell has no legal target and is
    // not offered.
    let db = db();
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    let sid = StackId(state.mint_id());
    state.stack.push(StackObject {
        id: sid,
        controller: PlayerId(1),
        kind: StackObjectKind::Ability {
            source: PermanentId(999),
            effects: vec![crate::ability::Effect::DrawCard { count: 1 }],
        },
        targets: Vec::new(),
    });
    let negation = state.new_instance(counterspell_id());
    state.players[0].hand = vec![negation];
    state.players[0].mana_pool.add(Color::Blue, 2);
    state.players[0].mana_pool.colorless = 1;

    // The ability is not a candidate, so the slot is empty and no cast is offered.
    assert!(legal_targets_for_spec(TargetSpec::SpellOnStack, &state, &db).is_empty());
    assert!(!valid_actions(&state, &db)
        .iter()
        .any(|a| matches!(a, Action::CastSpell { .. })));
}

#[test]
fn issue_148_a_cast_with_an_illegal_spell_target_is_a_no_op() {
    // A target of the right kind but outside the legal set — a StackId naming
    // no spell on the stack — fails the fresh legality check, so the whole cast
    // is rejected as a no-op (nothing cast, no mana paid).
    let db = db();
    let (state, negation, _sid) = counterspell_over_a_creature_spell();

    let after = apply_action(
        &state,
        &Action::CastSpell {
            card: negation,
            targets: vec![Target::Spell(StackId(99_999))],
        },
        &db,
    );
    assert_eq!(after, state);
}

#[test]
fn issue_341_attacker_may_target_any_opponent_but_never_self_or_eliminated() {
    // CR 508.1a: an attacker may be declared to attack any opponent still in the
    // game; assigning it to the active player or an eliminated one is illegal.
    let db = db();
    let mut state = GameState::new_multiplayer(3);
    state.turn = 2;
    state.step = Step::DeclareAttackers;
    state.active_player = PlayerId(0);
    let atk = put_on_battlefield(&mut state, fixture("walking_corpse"));

    let attack = |defender| {
        [Attack {
            attacker: atk,
            defender,
        }]
    };
    // Legal against either living opponent.
    assert!(crate::actions::legality::attackers_selection_is_legal(
        &state,
        &db,
        &attack(PlayerId(1))
    ));
    assert!(crate::actions::legality::attackers_selection_is_legal(
        &state,
        &db,
        &attack(PlayerId(2))
    ));
    // Illegal against the active player themselves.
    assert!(!crate::actions::legality::attackers_selection_is_legal(
        &state,
        &db,
        &attack(PlayerId(0))
    ));
    // Illegal against a non-existent seat.
    assert!(!crate::actions::legality::attackers_selection_is_legal(
        &state,
        &db,
        &attack(PlayerId(9))
    ));

    // Once seat 2 is eliminated it is no longer a legal defender.
    state.players[2].has_lost = true;
    assert!(!crate::actions::legality::attackers_selection_is_legal(
        &state,
        &db,
        &attack(PlayerId(2))
    ));
    assert!(crate::actions::legality::attackers_selection_is_legal(
        &state,
        &db,
        &attack(PlayerId(1))
    ));
}
