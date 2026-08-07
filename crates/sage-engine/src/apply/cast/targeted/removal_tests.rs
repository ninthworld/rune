//! Targeted removal, counters, and damage — including the CR 608.2b fizzle.

#![allow(clippy::unwrap_used)]

use super::*;
use crate::apply::test_support::*;

#[test]
fn issue_148_counterspell_counters_a_creature_spell_end_to_end_cr_701_5() {
    // A creature spell (player 1) waits on the stack; player 0, holding
    // priority, casts Cancel ({1}{U}{U} instant) targeting it. The
    // counterspell records its target at cast (CR 601.2c) and, resolving first
    // (LIFO), removes the creature spell to its owner's graveyard without
    // resolving (CR 701.5a) — the creature never enters the battlefield.
    let db = db();
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;

    // Player 1's Onakke Ogre (vanilla creature) on the stack.
    let boar = state.new_instance(fixture("onakke_ogre"));
    let boar_sid = StackId(state.mint_id());
    state.stack.push(StackObject {
        paid: Default::default(),
        id: boar_sid,
        controller: PlayerId(1),
        kind: StackObjectKind::Spell {
            card: boar,
            mode: None,
            x: None,
            from_hand: true,
        },
        targets: Vec::new(),
    });

    // Player 0 holds priority with the counterspell and {1}{U}{U}.
    let negation = state.new_instance(fixture("cancel"));
    state.players[0].hand = vec![negation];
    state.players[0].mana_pool.add(Color::Blue, 2);
    state.players[0].mana_pool.colorless = 1;
    state.priority = PlayerId(0);

    // Cast the counterspell targeting the creature spell (CR 601.2c).
    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: negation,
            mode: None,
            x: None,
            targets: vec![Target::Spell(boar_sid)],
            payment: Vec::new(),
        },
        &db,
    );
    assert_eq!(
        state.stack.len(),
        2,
        "counterspell stacked over the creature"
    );
    assert_eq!(
        state.stack[1].targets,
        vec![Target::Spell(boar_sid)],
        "the chosen target is recorded on the stack at cast (CR 601.2c)"
    );
    assert_eq!(
        state.players[0].mana_pool.blue, 0,
        "the {{1}}{{U}}{{U}} was paid"
    );

    // Both pass: the counterspell resolves first and counters the creature.
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    assert!(state.stack.is_empty(), "both spells have left the stack");
    assert!(
        state
            .battlefield
            .iter()
            .all(|p| p.printed.card() != Some(fixture("onakke_ogre"))),
        "the countered creature never entered the battlefield (CR 701.5a)"
    );
    assert!(
        state.players[1].graveyard.contains(&boar),
        "the countered spell went to its owner's graveyard (CR 701.5a)"
    );
    assert!(
        state.players[0]
            .graveyard
            .iter()
            .any(|c| c.id == negation.id),
        "the resolved counterspell went to its owner's graveyard (CR 608.2m)"
    );
}

#[test]
fn issue_148_counterspell_fizzles_when_its_target_resolves_first_cr_608_2b() {
    // If the targeted spell resolves before the counterspell (the counterspell
    // sits *beneath* it), the counterspell's only target is gone at resolution,
    // so it fizzles (CR 608.2b): no spell is countered, and the counterspell
    // still goes to its owner's graveyard.
    let db = db();
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;

    // Bottom of the stack: player 0's counterspell aimed at the creature above.
    let negation = state.new_instance(fixture("cancel"));
    let neg_sid = StackId(state.mint_id());
    let boar = state.new_instance(fixture("onakke_ogre"));
    let boar_sid = StackId(state.mint_id());
    state.stack.push(StackObject {
        paid: Default::default(),
        id: neg_sid,
        controller: PlayerId(0),
        kind: StackObjectKind::Spell {
            card: negation,
            mode: None,
            x: None,
            from_hand: true,
        },
        targets: vec![Target::Spell(boar_sid)],
    });
    // Top of the stack: player 1's vanilla creature spell, resolves first.
    state.stack.push(StackObject {
        paid: Default::default(),
        id: boar_sid,
        controller: PlayerId(1),
        kind: StackObjectKind::Spell {
            card: boar,
            mode: None,
            x: None,
            from_hand: true,
        },
        targets: Vec::new(),
    });

    // Resolve the top (the creature): it enters the battlefield.
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    assert!(
        state
            .battlefield
            .iter()
            .any(|p| p.printed.card() == Some(fixture("onakke_ogre"))),
        "the creature spell resolved onto the battlefield"
    );

    // Resolve the counterspell: its target is gone, so it fizzles (CR 608.2b).
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    assert!(state.stack.is_empty());
    assert!(
        state
            .battlefield
            .iter()
            .any(|p| p.printed.card() == Some(fixture("onakke_ogre"))),
        "the creature survives — nothing was countered"
    );
    assert!(
        state.players[0]
            .graveyard
            .iter()
            .any(|c| c.id == negation.id),
        "a fizzled spell still goes to its owner's graveyard (CR 608.2b)"
    );
}

#[test]
fn issue_149_burn_spell_kills_a_creature_via_lethal_damage_sba_cr_704_5g() {
    // A burn spell that deals damage equal to a creature's toughness marks
    // lethal damage; the CR 704.5g state-based action then destroys it.
    let db = db();
    let mut state = main_phase_p0();
    // Onakke Ogre is a 4/2; Shock deals exactly 2 → lethal to its toughness.
    let boar = place_permanent(&mut state, fixture("onakke_ogre"), PlayerId(1), false, 0);
    let shock = state.new_instance(fixture("shock"));
    state.players[0].hand = vec![shock];
    state.players[0].mana_pool.add(Color::Red, 1);

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: shock,
            mode: None,
            x: None,
            targets: vec![Target::Permanent(boar)],
            payment: Vec::new(),
        },
        &db,
    );
    assert_eq!(state.stack.len(), 1, "the burn spell is on the stack");
    let state = pass_full_round(&state, &db);

    assert!(
        !state.battlefield.iter().any(|p| p.id == boar),
        "the burned creature is destroyed (CR 704.5g)"
    );
    assert_eq!(
        state.players[1].graveyard.len(),
        1,
        "it went to its owner's graveyard"
    );
}

#[test]
fn issue_149_burn_spell_to_a_player_drops_life_and_loses_at_zero_cr_704_5a() {
    // The same burn verb aimed at a player is life loss (CR 120.3a); dropping a
    // player to 0 feeds the zero-life loss (CR 704.5a).
    let db = db();
    let mut state = main_phase_p0();
    state.players[1].life = 2;
    let shock = state.new_instance(fixture("shock"));
    state.players[0].hand = vec![shock];
    state.players[0].mana_pool.add(Color::Red, 1);

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: shock,
            mode: None,
            x: None,
            targets: vec![Target::Player(PlayerId(1))],
            payment: Vec::new(),
        },
        &db,
    );
    let state = pass_full_round(&state, &db);

    assert_eq!(state.players[1].life, 0);
    assert!(state.players[1].has_lost);
    assert_eq!(state.players[1].loss_reason, Some(LossReason::ZeroLife));
}

#[test]
fn issue_256_lightning_strike_deals_three_to_any_target() {
    // Lightning Strike is a {1}{R} bolt — 3 damage to any target, distinct from
    // Shock's 2. Aimed at a player on 3 life, it drops them to 0 (CR 704.5a).
    let db = db();
    let mut state = main_phase_p0();
    state.players[1].life = 3;
    let bolt = state.new_instance(fixture("lightning_strike"));
    state.players[0].hand = vec![bolt];
    state.players[0].mana_pool.add(Color::Red, 1);
    state.players[0].mana_pool.colorless = 1;

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: bolt,
            mode: None,
            x: None,
            targets: vec![Target::Player(PlayerId(1))],
            payment: Vec::new(),
        },
        &db,
    );
    let state = pass_full_round(&state, &db);

    assert_eq!(state.players[1].life, 0);
    assert!(state.players[1].has_lost);
}

#[test]
fn issue_149_destroy_puts_a_creature_in_its_owners_graveyard_cr_701_7() {
    let db = db();
    let mut state = main_phase_p0();
    let boar = place_permanent(&mut state, fixture("onakke_ogre"), PlayerId(1), false, 0);
    // Murder is a {1}{B}{B} instant: two black pips and one generic.
    let ray = state.new_instance(fixture("murder"));
    state.players[0].hand = vec![ray];
    state.players[0].mana_pool.add(Color::Black, 2);
    state.players[0].mana_pool.colorless = 1;

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: ray,
            mode: None,
            x: None,
            targets: vec![Target::Permanent(boar)],
            payment: Vec::new(),
        },
        &db,
    );
    let state = pass_full_round(&state, &db);

    assert!(
        !state.battlefield.iter().any(|p| p.id == boar),
        "the targeted creature is destroyed (CR 701.7)"
    );
    assert!(state.players[1]
        .graveyard
        .iter()
        .any(|c| c.card == fixture("onakke_ogre")));
}

#[test]
fn issue_149_destroy_fizzles_if_its_target_left_first_cr_608_2b() {
    let db = db();
    let mut state = main_phase_p0();
    let boar = place_permanent(&mut state, fixture("onakke_ogre"), PlayerId(1), false, 0);
    let ray = state.new_instance(fixture("murder"));
    state.players[0].hand = vec![ray];
    state.players[0].mana_pool.add(Color::Black, 2);
    state.players[0].mana_pool.colorless = 1;

    let mut state = apply_action(
        &state,
        &Action::CastSpell {
            card: ray,
            mode: None,
            x: None,
            targets: vec![Target::Permanent(boar)],
            payment: Vec::new(),
        },
        &db,
    );
    // The target leaves the battlefield before the sorcery resolves.
    state.battlefield.retain(|p| p.id != boar);

    let state = pass_full_round(&state, &db);
    assert!(state.stack.is_empty());
    assert!(
        state.players[0].graveyard.iter().any(|c| c.id == ray.id),
        "a fizzled spell still goes to its owner's graveyard (CR 608.2b)"
    );
}

#[test]
fn issue_149_minus_one_counter_lowers_toughness_to_lethal_cr_704_5g() {
    // A -1/-1 counter folds into computed toughness (CR 613.7c). A 3/2 with 1
    // marked damage is not lethal (1 < 2); after a -1/-1 counter it is a 2/1
    // and 1 damage is lethal (1 ≥ 1), so the SBA destroys it. The -1/-1 counter
    // spell has no M19 representative, so both cards are inline.
    let json = r#"[
        {"schema_version":1,"functional_id":"test_boar","name":"Test Boar",
         "types":["creature"],"subtypes":["Boar"],"mana_cost":"{2}{G}","colors":["green"],
         "power":3,"toughness":2},
        {"schema_version":1,"functional_id":"test_wither","name":"Test Wither",
         "types":["sorcery"],"mana_cost":"{B}","colors":["black"],
         "spell_effects":[{"kind":"put_counters","target":"any_creature","counter":"minus_one_minus_one","count":1}]}
    ]"#;
    let db = CardDatabase::from_json(json).unwrap();
    let mut state = main_phase_p0();
    let boar = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(1), false, 1);
    let touch = state.new_instance(id_in(&db, "test_wither")); // {B} sorcery, -1/-1
    state.players[0].hand = vec![touch];
    state.players[0].mana_pool.add(Color::Black, 1);

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: touch,
            mode: None,
            x: None,
            targets: vec![Target::Permanent(boar)],
            payment: Vec::new(),
        },
        &db,
    );
    let state = pass_full_round(&state, &db);

    assert!(
        !state.battlefield.iter().any(|p| p.id == boar),
        "a -1/-1 counter made toughness ≤ marked damage → destroyed (CR 704.5g)"
    );
    assert_eq!(state.players[1].graveyard.len(), 1);
}

#[test]
fn issue_401_strangling_spores_shrinks_a_creature_to_death_cr_704_5f() {
    // Strangling Spores: target creature gets -3/-3 until end of turn — a
    // negative pump. A 4/2 Onakke Ogre drops to a 1/-1, and the CR 704.5f
    // zero-toughness state-based action puts it into the graveyard.
    let db = db();
    let mut state = main_phase_p0();
    let ogre = place_permanent(&mut state, fixture("onakke_ogre"), PlayerId(1), false, 0);
    let spores = state.new_instance(fixture("strangling_spores"));
    state.players[0].hand = vec![spores];
    state.players[0].mana_pool.add(Color::Black, 1);
    state.players[0].mana_pool.colorless = 3;

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: spores,
            mode: None,
            x: None,
            targets: vec![Target::Permanent(ogre)],
            payment: Vec::new(),
        },
        &db,
    );
    let state = pass_full_round(&state, &db);

    assert!(
        !state.battlefield.iter().any(|p| p.id == ogre),
        "-3/-3 dropped toughness to 0 or less (CR 704.5f)"
    );
    assert_eq!(state.players[1].graveyard.len(), 1);
}

#[test]
fn issue_401_lichs_caress_destroys_a_creature_and_gains_three_life() {
    // Lich's Caress: destroy target creature, then you gain 3 life.
    let db = db();
    let mut state = main_phase_p0();
    let life_before = state.players[0].life;
    let victim = place_permanent(&mut state, fixture("onakke_ogre"), PlayerId(1), false, 0);
    let caress = state.new_instance(fixture("lich_s_caress"));
    state.players[0].hand = vec![caress];
    state.players[0].mana_pool.add(Color::Black, 2);
    state.players[0].mana_pool.colorless = 3;

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: caress,
            mode: None,
            x: None,
            targets: vec![Target::Permanent(victim)],
            payment: Vec::new(),
        },
        &db,
    );
    let state = pass_full_round(&state, &db);

    assert!(
        !state.battlefield.iter().any(|p| p.id == victim),
        "the targeted creature is destroyed (CR 701.7)"
    );
    assert_eq!(
        state.players[0].life,
        life_before + 3,
        "and its controller gains 3 life"
    );
}

#[test]
fn issue_401_lava_axe_deals_five_to_a_player() {
    // Lava Axe: 5 damage to target player (planeswalkers are unmodeled, so the
    // spec is `any_player`) — the first shipped burn aimed only at a player.
    let db = db();
    let mut state = main_phase_p0();
    state.players[1].life = 20;
    let axe = state.new_instance(fixture("lava_axe"));
    state.players[0].hand = vec![axe];
    state.players[0].mana_pool.add(Color::Red, 1);
    state.players[0].mana_pool.colorless = 4;

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: axe,
            mode: None,
            x: None,
            targets: vec![Target::Player(PlayerId(1))],
            payment: Vec::new(),
        },
        &db,
    );
    let state = pass_full_round(&state, &db);

    assert_eq!(state.players[1].life, 15, "20 - 5 (CR 120.3a)");
}
