//! Targeted continuous modifications: pumps, granted keywords, and Auras.

#![allow(clippy::unwrap_used)]

use super::*;
use crate::apply::test_support::*;

#[test]
fn issue_150_pump_spell_boosts_its_target_until_end_of_turn_end_to_end() {
    // Cast Titanic Growth (+4/+4 until end of turn) on a 1/1 Llanowar Elves: on
    // resolution the creature computes as a 5/5 and one until-end-of-turn layer-7c
    // modifier is in force.
    use crate::characteristics::characteristics;
    let db = db();
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    let creature = place_permanent(&mut state, fixture("llanowar_elves"), PlayerId(0), false, 0);
    let surge = state.new_instance(fixture("titanic_growth"));
    state.players[0].hand = vec![surge];
    state.players[0].mana_pool.add(Color::Green, 1);
    state.players[0].mana_pool.colorless = 2;

    // The Elves is a printed 1/1 before the pump.
    assert_eq!(characteristics(&state, creature, &db).power, Some(1));

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: surge,
            targets: vec![Target::Permanent(creature)],
            payment: Vec::new(),
        },
        &db,
    );
    // Pass twice: the spell resolves and applies its pump.
    let state = pass_full_round(&state, &db);

    assert!(state.stack.is_empty());
    let ch = characteristics(&state, creature, &db);
    assert_eq!(ch.power, Some(5), "printed 1 + 4 until end of turn");
    assert_eq!(ch.toughness, Some(5));
    assert_eq!(state.static_effects.len(), 1);
    assert_eq!(
        state.static_effects[0].duration,
        Duration::UntilEndOfTurn,
        "the pump is an until-end-of-turn effect"
    );
    // The instant itself went to the graveyard (CR 608.2m).
    assert!(state.players[0].graveyard.iter().any(|c| c.id == surge.id));
}

#[test]
fn issue_150_pumped_creature_survives_lethal_to_base_damage_then_expires_at_cleanup_cr_514_2() {
    // CR 514.2: a 1/1 pumped to 4/4 that has taken 3 marked damage (lethal to
    // its *base* toughness of 1, but not to 4) survives the turn, and at
    // cleanup its pump wears off and its damage is removed **simultaneously** —
    // so the CR 704.5g check that follows never sees a 1/1 with 3 damage and
    // the creature survives cleanup as a printed 1/1.
    let db = db();
    let mut state = GameState::new_two_player();
    state.step = Step::End; // player 0, turn 1; empty hand so no discard.
    let creature = place_permanent(&mut state, fixture("llanowar_elves"), PlayerId(0), false, 3);
    pump(&mut state, creature, 3, 3);

    // Mid-turn: 4/4 with 3 marked damage is not lethal, so state-based actions
    // leave it on the battlefield.
    let mut mid = state.clone();
    run_state_based_actions(&mut mid, &db);
    assert!(
        mid.battlefield.iter().any(|p| p.id == creature),
        "3 damage is not lethal to a pumped 4/4"
    );

    // Walk through the cleanup step into the next turn.
    let after = pass_full_round(&state, &db);
    assert!(
        after.battlefield.iter().any(|p| p.id == creature),
        "the creature survives cleanup: damage and pump end simultaneously (CR 514.2)"
    );
    assert!(
        after.static_effects.is_empty(),
        "the until-end-of-turn pump wore off at cleanup"
    );
    assert_eq!(
        find_perm(&after, creature).damage,
        0,
        "marked damage was wiped at cleanup"
    );
}

#[test]
fn issue_374_grant_keyword_spell_grants_the_keyword_until_end_of_turn_end_to_end() {
    // Cast Mighty Leap (+2/+2 and gains flying until end of turn) on a ground
    // Llanowar Elves: on resolution the creature computes with flying, and the
    // layer-6 grant among its two until-end-of-turn effects is in force.
    use crate::characteristics::characteristics;
    let db = db();
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    let creature = place_permanent(&mut state, fixture("llanowar_elves"), PlayerId(0), false, 0);
    let jump = state.new_instance(fixture("mighty_leap"));
    state.players[0].hand = vec![jump];
    state.players[0].mana_pool.add(Color::White, 1);
    state.players[0].mana_pool.add_colorless(1);

    // The Elves has no flying before the spell.
    assert!(!characteristics(&state, creature, &db)
        .keywords
        .contains(&Keyword::Flying));

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: jump,
            targets: vec![Target::Permanent(creature)],
            payment: Vec::new(),
        },
        &db,
    );
    // Pass twice: the spell resolves and applies its grant.
    let state = pass_full_round(&state, &db);

    assert!(state.stack.is_empty());
    assert!(
        characteristics(&state, creature, &db)
            .keywords
            .contains(&Keyword::Flying),
        "the resolved spell granted flying (CR 613.1f)"
    );
    // Two until-end-of-turn effects: the P/T pump (layer 7c) and the keyword
    // grant (layer 6) the assertion above reads.
    assert_eq!(state.static_effects.len(), 2);
    assert!(state
        .static_effects
        .iter()
        .all(|e| e.duration == Duration::UntilEndOfTurn));
    assert!(state.players[0].graveyard.iter().any(|c| c.id == jump.id));
}

#[test]
fn issue_374_until_end_of_turn_grant_expires_at_cleanup_cr_514_2() {
    // CR 514.2: an until-end-of-turn keyword grant ends in the cleanup step. The
    // grant is present the turn it is made and gone once the turn passes — verified
    // across the turn boundary.
    use crate::characteristics::characteristics;
    let db = db();
    let mut state = GameState::new_two_player();
    state.step = Step::End; // player 0, turn 1; empty hand so no discard.
    let creature = place_permanent(&mut state, fixture("walking_corpse"), PlayerId(0), false, 0);
    grant_keyword(&mut state, creature, Keyword::Flying);

    // Before cleanup the creature has the granted keyword.
    assert!(
        characteristics(&state, creature, &db)
            .keywords
            .contains(&Keyword::Flying),
        "the grant is in force during the turn it was made"
    );

    // Walk through the cleanup step into the next turn.
    let after = pass_full_round(&state, &db);
    assert!(
        after.static_effects.is_empty(),
        "the until-end-of-turn grant wore off at cleanup (CR 514.2)"
    );
    assert!(
        !characteristics(&after, creature, &db)
            .keywords
            .contains(&Keyword::Flying),
        "the granted keyword is gone across the turn boundary"
    );
}

#[test]
fn issue_150_two_pumps_in_one_turn_stack_and_both_expire_at_cleanup() {
    // CR 613.7 / 514.2: two pumps on one creature this turn both apply (they
    // stack in timestamp order) and both wear off at cleanup.
    use crate::characteristics::characteristics;
    let db = db();
    let mut state = GameState::new_two_player();
    state.step = Step::End; // player 0, turn 1; empty hand.
    let creature = place_permanent(&mut state, fixture("llanowar_elves"), PlayerId(0), false, 0);
    let first = pump(&mut state, creature, 2, 2);
    let second = pump(&mut state, creature, 1, 1);
    assert!(second > first, "the later pump has the later timestamp");

    // Printed 1/1 + (+2/+2) + (+1/+1) = 4/4 while both are in force.
    let ch = characteristics(&state, creature, &db);
    assert_eq!(ch.power, Some(4));
    assert_eq!(ch.toughness, Some(4));

    let after = pass_full_round(&state, &db);
    assert!(
        after.static_effects.is_empty(),
        "both until-end-of-turn pumps expired at cleanup (CR 514.2)"
    );
    let reverted = characteristics(&after, creature, &db);
    assert_eq!(reverted.power, Some(1), "back to the printed 1/1");
    assert_eq!(reverted.toughness, Some(1));
}

#[test]
fn issue_150_pump_never_outlives_its_permanent() {
    // A pumped creature that dies mid-turn (here to lethal-to-its-4/4 damage)
    // leaves no dangling modifier: the state-based-actions loop destroys it and
    // prunes its now-orphaned pump in the same pass.
    let db = db();
    let mut state = GameState::new_two_player();
    let creature = place_permanent(&mut state, fixture("llanowar_elves"), PlayerId(0), false, 5);
    pump(&mut state, creature, 3, 3); // 1/1 -> 4/4, but 5 damage is lethal

    run_state_based_actions(&mut state, &db);

    assert!(
        !state.battlefield.iter().any(|p| p.id == creature),
        "5 damage is lethal to the pumped 4/4 (CR 704.5g)"
    );
    assert!(
        state.static_effects.is_empty(),
        "the pump was pruned when its permanent left — no dangling modifier"
    );
}

#[test]
fn issue_150_while_on_battlefield_effect_is_not_ended_by_cleanup() {
    // CR 514.2 ends only "until end of turn" effects; a permanent-lifetime
    // anthem (WhileOnBattlefield) is untouched by the cleanup step.
    let db = db();
    let mut state = GameState::new_two_player();
    state.step = Step::End; // player 0, turn 1; empty hand.
    let _creature = place_permanent(&mut state, fixture("walking_corpse"), PlayerId(0), false, 0);
    let source = state.mint_id();
    state.static_effects.push(StaticEffect {
        source,
        affects: EffectAffects::CreaturesControlledBy(PlayerId(0)),
        modification: Modification::PowerToughness {
            power: 1,
            toughness: 1,
        },
        duration: Duration::WhileOnBattlefield,
    });

    let after = pass_full_round(&state, &db);
    assert_eq!(
        after.static_effects.len(),
        1,
        "a while-on-battlefield anthem persists through cleanup (CR 514.2)"
    );
}

#[test]
fn issue_152_minus_x_aura_cast_kills_its_host_and_follows_it_cr_704_5f() {
    // Full slice through the real cast path: cast a -2/-2 Aura on a 3/2 host. On
    // resolution the Aura enters attached, its -2/-2 drops the host's current
    // toughness to 0, and the pipeline's state-based-actions loop puts the host
    // into the graveyard (CR 704.5f) and its now-orphaned Aura with it (CR
    // 704.5m) — both gone in the same fixed point, the modifier vanishing with the
    // Aura. P/T Auras have no clean M19 card, so this is inline.
    use crate::ability::Target;
    use crate::characteristics::characteristics;
    let json = r#"[
        {"schema_version":1,"functional_id":"test_curse","name":"Test Curse",
         "types":["enchantment"],"subtypes":["Aura"],"mana_cost":"{B}","colors":["black"],
         "attachment":{"kind":"aura","attach_to":"any_creature","power":-2,"toughness":-2}},
        {"schema_version":1,"functional_id":"test_boar","name":"Test Boar",
         "types":["creature"],"subtypes":["Boar"],"mana_cost":"{2}{G}","colors":["green"],
         "power":3,"toughness":2}
    ]"#;
    let db = CardDatabase::from_json(json).unwrap();
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    let host = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(0), false, 0); // 3/2
    let curse = state.new_instance(id_in(&db, "test_curse")); // -2/-2 Aura, {B}
    state.players[0].hand = vec![curse];
    state.players[0].mana_pool.add(Color::Black, 1);

    // The host is a healthy 3/2 before the Aura is cast.
    assert_eq!(characteristics(&state, host, &db).toughness, Some(2));

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: curse,
            targets: vec![Target::Permanent(host)],
            payment: Vec::new(),
        },
        &db,
    );
    // Both players pass: the Aura resolves, attaches, and the SBA loop settles.
    let state = pass_full_round(&state, &db);

    assert!(
        !state.battlefield.iter().any(|p| p.id == host),
        "the host at 0 toughness is put into the graveyard (CR 704.5f)"
    );
    assert!(
        !state
            .battlefield
            .iter()
            .any(|p| p.printed.card() == Some(id_in(&db, "test_curse"))),
        "the Aura follows its dead host to the graveyard (CR 704.5m)"
    );
    assert!(
        state.static_effects.is_empty(),
        "the Aura's derived modifier leaves no dangling static effect"
    );
    // The Boar and the Curse are both in the graveyard.
    assert_eq!(state.players[0].graveyard.len(), 2);
}

// === issue #401: behavior of the nontrivial M19 catalog additions ===
// Vanilla and single-keyword bodies (Loxodon Line Breaker, Havoc Devils,
// Daybreak Chaplain, …) reuse mechanics already covered by the generic
// keyword/combat tests, so only the cards that *do* something get a boundary
// test here. Skeleton Archer's ETB "deal 1 damage to any target" is omitted:
// like Viashino Pyromancer it is a triggered ability, and triggers carry no
// chosen targets until issue #71 — so its damage is exercised only by the
// rules-text generator, not end-to-end.

#[test]
fn issue_401_aegis_of_the_heavens_pumps_plus_one_plus_seven() {
    // Aegis of the Heavens: a {1}{W} instant, +1/+7 until end of turn.
    use crate::characteristics::characteristics;
    let db = db();
    let mut state = main_phase_p0();
    let creature = place_permanent(&mut state, fixture("llanowar_elves"), PlayerId(0), false, 0);
    let aegis = state.new_instance(fixture("aegis_of_the_heavens"));
    state.players[0].hand = vec![aegis];
    state.players[0].mana_pool.add(Color::White, 1);
    state.players[0].mana_pool.colorless = 1;

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: aegis,
            targets: vec![Target::Permanent(creature)],
            payment: Vec::new(),
        },
        &db,
    );
    let state = pass_full_round(&state, &db);

    let ch = characteristics(&state, creature, &db);
    assert_eq!(ch.power, Some(2), "printed 1 + 1");
    assert_eq!(ch.toughness, Some(8), "printed 1 + 7");
}

#[test]
fn issue_401_mighty_leap_pumps_and_grants_flying_in_one_spell() {
    // Mighty Leap: +2/+2 *and* gains flying until end of turn — **one** effect
    // and therefore one target slot, so the cast supplies a single creature and
    // both halves land on it.
    use crate::characteristics::characteristics;
    let db = db();
    let mut state = main_phase_p0();
    let creature = place_permanent(&mut state, fixture("llanowar_elves"), PlayerId(0), false, 0);
    let leap = state.new_instance(fixture("mighty_leap"));
    state.players[0].hand = vec![leap];
    state.players[0].mana_pool.add(Color::White, 1);
    state.players[0].mana_pool.colorless = 1;

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: leap,
            targets: vec![Target::Permanent(creature)],
            payment: Vec::new(),
        },
        &db,
    );
    let state = pass_full_round(&state, &db);

    let ch = characteristics(&state, creature, &db);
    assert_eq!(ch.power, Some(3), "1 + 2");
    assert_eq!(ch.toughness, Some(3));
    assert!(
        ch.keywords.contains(&Keyword::Flying),
        "the same spell granted flying (CR 613.1f)"
    );
}

#[test]
fn mighty_leap_pumps_and_grants_to_one_creature_not_two() {
    // The card reads "target creature gets +2/+2 **and** gains flying": one
    // target, both halves. Before this it was authored as two effects, which the
    // targeting pipeline advertised as two independent slots — so a player could
    // pump one creature and hand a *different* one flying, a card strictly better
    // than the printed one. The single slot is the fix, and naming two targets is
    // now an illegal announcement rather than a bonus.
    use crate::characteristics::characteristics;
    let db = db();
    let mut state = main_phase_p0();
    let mine = place_permanent(&mut state, fixture("llanowar_elves"), PlayerId(0), false, 0);
    let theirs = place_permanent(&mut state, fixture("onakke_ogre"), PlayerId(1), false, 0);
    let leap = state.new_instance(fixture("mighty_leap"));
    state.players[0].hand = vec![leap];
    state.players[0].mana_pool.add(Color::White, 1);
    state.players[0].mana_pool.colorless = 1;

    // One slot: the spell advertises exactly one target requirement.
    let groups = crate::CardData::cast_target_groups(db.card(fixture("mighty_leap")).unwrap());
    assert_eq!(groups.len(), 1, "one effect, one target group");

    // Two different creatures is not a legal announcement.
    assert!(!crate::actions::action_is_legal(
        &state,
        &Action::CastSpell {
            card: leap,
            targets: vec![Target::Permanent(mine), Target::Permanent(theirs)],
            payment: Vec::new(),
        },
        &db
    ));

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: leap,
            targets: vec![Target::Permanent(mine)],
            payment: Vec::new(),
        },
        &db,
    );
    let state = pass_full_round(&state, &db);

    // Everything landed on the one creature named, and nothing on the other.
    let ch = characteristics(&state, mine, &db);
    assert_eq!(ch.power, Some(3));
    assert!(ch.keywords.contains(&Keyword::Flying));
    let other = characteristics(&state, theirs, &db);
    assert_eq!(other.power, Some(4), "the Ogre's printed power, unpumped");
    assert!(!other.keywords.contains(&Keyword::Flying));
}

#[test]
fn issue_401_sure_strike_pumps_power_and_grants_first_strike() {
    // Sure Strike: +3/+0 and gains first strike until end of turn.
    use crate::characteristics::characteristics;
    let db = db();
    let mut state = main_phase_p0();
    let creature = place_permanent(&mut state, fixture("llanowar_elves"), PlayerId(0), false, 0);
    let strike = state.new_instance(fixture("sure_strike"));
    state.players[0].hand = vec![strike];
    state.players[0].mana_pool.add(Color::Red, 1);
    state.players[0].mana_pool.colorless = 1;

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: strike,
            targets: vec![Target::Permanent(creature)],
            payment: Vec::new(),
        },
        &db,
    );
    let state = pass_full_round(&state, &db);

    let ch = characteristics(&state, creature, &db);
    assert_eq!(ch.power, Some(4), "1 + 3");
    assert_eq!(ch.toughness, Some(1), "toughness unchanged");
    assert!(ch.keywords.contains(&Keyword::FirstStrike));
}

#[test]
fn issue_401_knights_pledge_aura_boosts_its_host_plus_two_plus_two() {
    // Knight's Pledge: a bundled +2/+2 Aura — the first shipped P/T Aura.
    use crate::characteristics::characteristics;
    let db = db();
    let mut state = main_phase_p0();
    let host = place_permanent(&mut state, fixture("llanowar_elves"), PlayerId(0), false, 0);
    let pledge = state.new_instance(fixture("knight_s_pledge"));
    state.players[0].hand = vec![pledge];
    state.players[0].mana_pool.add(Color::White, 1);
    state.players[0].mana_pool.colorless = 1;

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: pledge,
            targets: vec![Target::Permanent(host)],
            payment: Vec::new(),
        },
        &db,
    );
    let state = pass_full_round(&state, &db);

    let ch = characteristics(&state, host, &db);
    assert_eq!(ch.power, Some(3), "printed 1 + 2 while enchanted");
    assert_eq!(ch.toughness, Some(3));
    assert!(
        state
            .battlefield
            .iter()
            .any(|p| p.printed.card() == Some(fixture("knight_s_pledge"))
                && p.attached_to == Some(host)),
        "the Aura entered attached to its host (CR 303.4d)"
    );
}

#[test]
fn issue_401_oakenform_aura_boosts_its_host_plus_three_plus_three() {
    // Oakenform: a bundled +3/+3 Aura.
    use crate::characteristics::characteristics;
    let db = db();
    let mut state = main_phase_p0();
    let host = place_permanent(&mut state, fixture("llanowar_elves"), PlayerId(0), false, 0);
    let oak = state.new_instance(fixture("oakenform"));
    state.players[0].hand = vec![oak];
    state.players[0].mana_pool.add(Color::Green, 1);
    state.players[0].mana_pool.colorless = 2;

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: oak,
            targets: vec![Target::Permanent(host)],
            payment: Vec::new(),
        },
        &db,
    );
    let state = pass_full_round(&state, &db);

    let ch = characteristics(&state, host, &db);
    assert_eq!(ch.power, Some(4), "1 + 3");
    assert_eq!(ch.toughness, Some(4));
}

#[test]
fn issue_401_prodigious_growth_aura_grants_p_t_and_trample() {
    // Prodigious Growth: +7/+7 *and* trample — a P/T-and-keyword Aura in one.
    use crate::characteristics::characteristics;
    let db = db();
    let mut state = main_phase_p0();
    let host = place_permanent(&mut state, fixture("llanowar_elves"), PlayerId(0), false, 0);
    let growth = state.new_instance(fixture("prodigious_growth"));
    state.players[0].hand = vec![growth];
    state.players[0].mana_pool.add(Color::Green, 2);
    state.players[0].mana_pool.colorless = 4;

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: growth,
            targets: vec![Target::Permanent(host)],
            payment: Vec::new(),
        },
        &db,
    );
    let state = pass_full_round(&state, &db);

    let ch = characteristics(&state, host, &db);
    assert_eq!(ch.power, Some(8), "1 + 7");
    assert_eq!(ch.toughness, Some(8));
    assert!(
        ch.keywords.contains(&Keyword::Trample),
        "the Aura grants trample (CR 613.1f) alongside its P/T"
    );
}
