//! An effect whose two target slots do not share a spec (issue #737): the one-sided
//! bite, the mutual fight, and what each does when one of the two creatures has gone.

#![allow(clippy::unwrap_used)]

use super::*;
use crate::actions::target_requirements;
use crate::apply::test_support::*;

/// A precombat main phase with player 0 on turn, holding Rabid Bite and the {1}{G} to
/// cast it, plus `mine` under their control and `theirs` under player 1's. Returns the
/// state, the spell in hand, and the two permanents in slot order.
fn bite_setup(mine: &str, theirs: &str) -> (GameState, CardInstance, PermanentId, PermanentId) {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    let dealer = place_permanent(&mut state, fixture(mine), PlayerId(0), false, 0);
    let dealt_to = place_permanent(&mut state, fixture(theirs), PlayerId(1), false, 0);
    let bite = state.new_instance(fixture("rabid_bite"));
    state.players[0].hand = vec![bite];
    state.players[0].mana_pool.add(Color::Green, 1);
    state.players[0].mana_pool.colorless = 1;
    (state, bite, dealer, dealt_to)
}

/// Cast `bite` at the two permanents and let it resolve.
fn cast_and_resolve(
    state: &GameState,
    bite: CardInstance,
    targets: Vec<Target>,
    db: &CardDatabase,
) -> GameState {
    let state = apply_action(
        state,
        &Action::CastSpell {
            card: bite,
            mode: None,
            x: None,
            targets,
            payment: Vec::new(),
        },
        db,
    );
    assert_eq!(state.stack.len(), 1, "the bite is on the stack");
    pass_full_round(&state, db)
}

#[test]
fn issue_737_rabid_bite_deals_the_dealer_s_power_one_way_cr_701_12() {
    // Rabid Bite: the creature its controller chose deals damage equal to its power to
    // the creature an opponent controls, and takes **nothing** back — the whole
    // difference between this and a fight. Two 3/3 Centaur Coursers, so the one-sidedness
    // is visible in which of them survives.
    let db = db();
    let (state, bite, mine, theirs) = bite_setup("centaur_courser", "centaur_courser");

    let state = cast_and_resolve(
        &state,
        bite,
        vec![Target::Permanent(mine), Target::Permanent(theirs)],
        &db,
    );

    assert!(
        !alive(&state, theirs),
        "3 damage from a 3/3 is lethal to a 3/3 (CR 704.5g)"
    );
    assert!(alive(&state, mine), "the dealer takes nothing back");
    assert_eq!(
        find_perm(&state, mine).damage,
        0,
        "a one-sided bite marks no damage on the creature that dealt it"
    );
    assert_eq!(
        state.players.iter().map(|p| p.life).collect::<Vec<_>>(),
        vec![20, 20],
        "damage to a creature is marked, never life loss (CR 120.3d)"
    );
    assert!(
        state.players[0]
            .graveyard
            .iter()
            .any(|c| c.card == fixture("rabid_bite")),
        "the sorcery goes to its owner's graveyard (CR 608.3)"
    );
}

#[test]
fn issue_737_nonlethal_bite_marks_damage_without_destroying() {
    // A 4/2 biting a 6/6: the damage is marked (CR 120.3d) and the recipient lives,
    // which is what proves the effect reads the *dealer's* power rather than a fixed
    // amount or the recipient's toughness.
    let db = db();
    let (state, bite, mine, theirs) = bite_setup("onakke_ogre", "colossal_dreadmaw");

    let state = cast_and_resolve(
        &state,
        bite,
        vec![Target::Permanent(mine), Target::Permanent(theirs)],
        &db,
    );

    assert_eq!(
        find_perm(&state, theirs).damage,
        4,
        "the 4/2's power, marked on the 6/6"
    );
    assert_eq!(find_perm(&state, mine).damage, 0);
}

#[test]
fn issue_737_each_slot_only_accepts_its_own_class_cr_601_2c() {
    // The point of the whole change: the two slots have different specs, so each
    // enumerates a different candidate set, and an announcement that aims them at the
    // wrong creatures is illegal.
    let db = db();
    let (state, bite, mine, theirs) = bite_setup("centaur_courser", "centaur_courser");

    let offer = Action::CastSpell {
        card: bite,
        mode: None,
        x: None,
        targets: Vec::new(),
        payment: Vec::new(),
    };
    assert!(valid_actions(&state, &db).contains(&offer));

    let reqs = target_requirements(&state, &db, &offer);
    assert_eq!(reqs.len(), 2, "two slots, not one group of two");
    assert_eq!(reqs[0].spec, TargetSpec::AnyCreatureYouControl);
    assert_eq!(reqs[0].candidates, vec![Target::Permanent(mine)]);
    assert_eq!(reqs[1].spec, TargetSpec::AnyCreatureAnOpponentControls);
    assert_eq!(reqs[1].candidates, vec![Target::Permanent(theirs)]);
    assert!(
        reqs.iter().all(|slot| !slot.optional),
        "both slots are required (CR 601.2c)"
    );

    // Swapping them fills each slot with a creature of the other's class, which is not a
    // legal announcement — `apply_action` returns the state untouched.
    let swapped = apply_action(
        &state,
        &Action::CastSpell {
            card: bite,
            mode: None,
            x: None,
            targets: vec![Target::Permanent(theirs), Target::Permanent(mine)],
            payment: Vec::new(),
        },
        &db,
    );
    assert!(
        swapped.stack.is_empty() && swapped.players[0].hand.len() == 1,
        "an announcement with the slots swapped is rejected outright"
    );
}

#[test]
fn issue_737_neither_is_damaged_when_the_dealer_has_gone_cr_701_12c() {
    // CR 701.12c: if either creature is an illegal target as the effect is reached,
    // neither deals nor is dealt damage. Here the *dealer* leaves in response, so the
    // creature that would have been bitten takes nothing — even though its own slot is
    // still perfectly legal, and the spell therefore still resolves (CR 608.2b).
    let db = db();
    let (state, bite, mine, theirs) = bite_setup("centaur_courser", "centaur_courser");

    let mut state = apply_action(
        &state,
        &Action::CastSpell {
            card: bite,
            mode: None,
            x: None,
            targets: vec![Target::Permanent(mine), Target::Permanent(theirs)],
            payment: Vec::new(),
        },
        &db,
    );
    // The dealer is removed in response — exiled, bounced, whatever put it elsewhere.
    state.battlefield.retain(|p| p.id != mine);
    let state = pass_full_round(&state, &db);

    assert!(state.stack.is_empty(), "the bite left the stack");
    assert!(alive(&state, theirs), "the survivor is untouched");
    assert_eq!(
        find_perm(&state, theirs).damage,
        0,
        "no damage is dealt when either creature has gone (CR 701.12c)"
    );
    assert!(
        !state
            .log
            .iter()
            .any(|e| matches!(e.event, GameEvent::SpellFizzled { .. })),
        "one target is still legal, so the spell resolved rather than fizzling"
    );
}

#[test]
fn issue_737_neither_is_damaged_when_the_recipient_has_gone_cr_701_12c() {
    // The mirror of the previous test: the *recipient* leaves, and the dealer — whose
    // slot is still legal — deals nothing to anyone.
    let db = db();
    let (state, bite, mine, theirs) = bite_setup("centaur_courser", "centaur_courser");

    let mut state = apply_action(
        &state,
        &Action::CastSpell {
            card: bite,
            mode: None,
            x: None,
            targets: vec![Target::Permanent(mine), Target::Permanent(theirs)],
            payment: Vec::new(),
        },
        &db,
    );
    state.battlefield.retain(|p| p.id != theirs);
    let state = pass_full_round(&state, &db);

    assert!(alive(&state, mine));
    assert_eq!(find_perm(&state, mine).damage, 0);
    assert_eq!(
        state.players[1].life, 20,
        "the damage does not fall through to the recipient's controller"
    );
}

#[test]
fn issue_737_bite_fizzles_when_both_creatures_have_gone_cr_608_2b() {
    // With *every* target illegal the spell does not resolve at all: it is removed from
    // the stack and put into its owner's graveyard, logged as a fizzle (CR 608.2b).
    let db = db();
    let (state, bite, mine, theirs) = bite_setup("centaur_courser", "centaur_courser");

    let mut state = apply_action(
        &state,
        &Action::CastSpell {
            card: bite,
            mode: None,
            x: None,
            targets: vec![Target::Permanent(mine), Target::Permanent(theirs)],
            payment: Vec::new(),
        },
        &db,
    );
    state.battlefield.retain(|p| p.id != mine && p.id != theirs);
    let state = pass_full_round(&state, &db);

    assert!(state.stack.is_empty());
    assert!(
        state
            .log
            .iter()
            .any(|e| matches!(e.event, GameEvent::SpellFizzled { .. })),
        "all targets illegal is a fizzle (CR 608.2b)"
    );
    assert!(
        state.players[0]
            .graveyard
            .iter()
            .any(|c| c.card == fixture("rabid_bite")),
        "a fizzled spell still goes to its owner's graveyard"
    );
}

#[test]
fn issue_737_bite_damage_carries_the_dealer_s_deathtouch_cr_702_2b() {
    // The damage has a *source* — a creature — which is what no other damage effect in
    // the IR has. A 2/2 Daggerback Basilisk bites a 6/6: two damage is nowhere near
    // lethal, and deathtouch destroys it anyway (CR 702.2b / 704.5h).
    let db = db();
    let (state, bite, mine, theirs) = bite_setup("daggerback_basilisk", "colossal_dreadmaw");

    let state = cast_and_resolve(
        &state,
        bite,
        vec![Target::Permanent(mine), Target::Permanent(theirs)],
        &db,
    );

    assert!(
        !alive(&state, theirs),
        "any nonzero damage from a deathtouch source is lethal (CR 704.5h)"
    );
    assert!(
        state.deathtouch_struck.is_empty(),
        "the state-based action consumed the flag"
    );
}

#[test]
fn issue_737_bite_damage_carries_the_dealer_s_lifelink_cr_702_15e() {
    // Lifelink likewise rides the source: a 2/1 Child of Night biting a 6/6 gains its
    // controller two life, and nobody else's total moves.
    let db = db();
    let (state, bite, mine, theirs) = bite_setup("child_of_night", "colossal_dreadmaw");

    let state = cast_and_resolve(
        &state,
        bite,
        vec![Target::Permanent(mine), Target::Permanent(theirs)],
        &db,
    );

    assert_eq!(
        state.players.iter().map(|p| p.life).collect::<Vec<_>>(),
        vec![22, 20],
        "the dealer's controller gains life equal to the damage (CR 702.15e)"
    );
    assert_eq!(find_perm(&state, theirs).damage, 2);
}

/// An inline catalog for the **mutual** form of the verb, which no M19 card prints
/// (ADR 0009: vocabulary with no clean representative is covered by `test_*`
/// definitions rather than by a shipped card).
fn fight_db() -> CardDatabase {
    let json = r#"[
        {"schema_version":1,"functional_id":"test_brawler","name":"Test Brawler",
         "types":["creature"],"subtypes":["Bear"],"mana_cost":"{1}{G}","colors":["green"],
         "power":3,"toughness":1},
        {"schema_version":1,"functional_id":"test_bulwark","name":"Test Bulwark",
         "types":["creature"],"subtypes":["Rhino"],"mana_cost":"{3}{G}","colors":["green"],
         "power":4,"toughness":4},
        {"schema_version":1,"functional_id":"test_pounce","name":"Test Pounce",
         "types":["instant"],"mana_cost":"{1}{G}","colors":["green"],
         "spell_effects":[{"kind":"fight","dealer":"any_creature_you_control",
          "dealt_to":"any_creature_an_opponent_controls","mutual":true}]}
    ]"#;
    CardDatabase::from_json(json).unwrap()
}

#[test]
fn issue_737_a_mutual_fight_damages_both_creatures_cr_701_12a() {
    // The printed word *fights*: each creature deals damage equal to its power to the
    // other, simultaneously. A 3/1 fighting a 4/4 kills itself and leaves three damage
    // behind — the 3/1's power still lands in full, which is what "simultaneously" buys.
    let db = fight_db();
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    let mine = place_permanent(
        &mut state,
        id_in(&db, "test_brawler"),
        PlayerId(0),
        false,
        0,
    );
    let theirs = place_permanent(
        &mut state,
        id_in(&db, "test_bulwark"),
        PlayerId(1),
        false,
        0,
    );
    let pounce = state.new_instance(id_in(&db, "test_pounce"));
    state.players[0].hand = vec![pounce];
    state.players[0].mana_pool.add(Color::Green, 1);
    state.players[0].mana_pool.colorless = 1;

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: pounce,
            mode: None,
            x: None,
            targets: vec![Target::Permanent(mine), Target::Permanent(theirs)],
            payment: Vec::new(),
        },
        &db,
    );
    let state = pass_full_round(&state, &db);

    assert!(!alive(&state, mine), "the 3/1 took four and died");
    assert_eq!(
        find_perm(&state, theirs).damage,
        3,
        "the 3/1 dealt its own power before dying (CR 701.12a)"
    );
}

#[test]
fn issue_737_a_mutual_fight_does_nothing_when_one_has_gone_cr_701_12c() {
    // The same all-or-nothing rule the one-sided form follows, stated by CR 701.12c for
    // the mutual one: the survivor takes no damage.
    let db = fight_db();
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    let mine = place_permanent(
        &mut state,
        id_in(&db, "test_brawler"),
        PlayerId(0),
        false,
        0,
    );
    let theirs = place_permanent(
        &mut state,
        id_in(&db, "test_bulwark"),
        PlayerId(1),
        false,
        0,
    );
    let pounce = state.new_instance(id_in(&db, "test_pounce"));
    state.players[0].hand = vec![pounce];
    state.players[0].mana_pool.add(Color::Green, 1);
    state.players[0].mana_pool.colorless = 1;

    let mut state = apply_action(
        &state,
        &Action::CastSpell {
            card: pounce,
            mode: None,
            x: None,
            targets: vec![Target::Permanent(mine), Target::Permanent(theirs)],
            payment: Vec::new(),
        },
        &db,
    );
    state.battlefield.retain(|p| p.id != theirs);
    let state = pass_full_round(&state, &db);

    assert!(alive(&state, mine));
    assert_eq!(
        find_perm(&state, mine).damage,
        0,
        "neither creature deals or is dealt damage (CR 701.12c)"
    );
}
