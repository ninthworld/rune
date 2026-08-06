//! Tests for the combat-damage assignment computed in the parent module: which
//! creatures deal in which step, how a blocked attacker spreads its power, and the
//! deathtouch, trample, and lifelink facts each assignment carries.

#![allow(clippy::unwrap_used)]

use super::*;
use crate::fixtures::{fixture, id_in};
use crate::id::CardId;
use crate::state::Permanent;

/// A first-strike attacker and a plain blocker/attacker, as an inline catalog —
/// first strike and deathtouch have no clean M19 representative, so the combat
/// tests that need those keywords build their own definitions (ADR 0009).
pub(crate) fn keyword_db() -> CardDatabase {
    let json = r#"[
        {"schema_version":1,"functional_id":"test_duelist","name":"Test Duelist",
         "types":["creature"],"subtypes":["Human","Knight"],"mana_cost":"{1}{W}","colors":["white"],
         "power":2,"toughness":2,"keywords":["first_strike"]},
        {"schema_version":1,"functional_id":"test_adder","name":"Test Adder",
         "types":["creature"],"subtypes":["Snake"],"mana_cost":"{G}","colors":["green"],
         "power":1,"toughness":1,"keywords":["deathtouch"]},
        {"schema_version":1,"functional_id":"test_basilisk","name":"Test Basilisk",
         "types":["creature"],"subtypes":["Basilisk"],"mana_cost":"{4}{G}","colors":["green"],
         "power":4,"toughness":5},
        {"schema_version":1,"functional_id":"test_boar","name":"Test Boar",
         "types":["creature"],"subtypes":["Boar"],"mana_cost":"{2}{G}","colors":["green"],
         "power":3,"toughness":2},
        {"schema_version":1,"functional_id":"test_twinstrike","name":"Test Twinstrike",
         "types":["creature"],"subtypes":["Cat"],"mana_cost":"{2}{W}","colors":["white"],
         "power":2,"toughness":2,"keywords":["double_strike"]},
        {"schema_version":1,"functional_id":"test_paragon","name":"Test Paragon",
         "types":["creature"],"subtypes":["Human","Knight"],"mana_cost":"{2}{W}{W}","colors":["white"],
         "power":2,"toughness":2,"keywords":["first_strike","double_strike"]}
    ]"#;
    CardDatabase::from_json(json).unwrap()
}

/// The bundled card database, for tests that need oracle data.
fn db() -> CardDatabase {
    CardDatabase::bundled().unwrap()
}

/// Put a creature of printed card `card` on the battlefield under `controller`,
/// untapped, entered on turn `entered_turn`; returns its fresh id. Used to
/// place the keyword-bearing real cards (flying, reach, vigilance, haste).
pub(crate) fn creature_card(
    state: &mut GameState,
    card: CardId,
    controller: crate::id::PlayerId,
    entered_turn: u32,
) -> PermanentId {
    let inst = state.new_instance(card);
    let id = PermanentId(state.mint_id());
    state.battlefield.push(Permanent {
        id,
        instance: inst.id,
        printed: card.into(),
        controller,
        tapped: false,
        entered_turn,
        attacking: None,
        blocking: Vec::new(),
        skips_untap: false,
        damage: 0,
        counters: Default::default(),
        attached_to: None,
        chosen_color: None,
    });
    id
}

/// Place an attacking creature of `card` under `controller` attacking the sole
/// opponent (the two-player default); returns its id.
fn attacker(state: &mut GameState, card: CardId, controller: crate::id::PlayerId) -> PermanentId {
    let defender = crate::combat::defending_player(state).unwrap_or(crate::id::PlayerId(1));
    attacker_of(state, card, controller, defender)
}

/// Place an attacking creature of `card` under `controller` attacking
/// `defender`; returns its id. Used by the multi-defender combat tests.
fn attacker_of(
    state: &mut GameState,
    card: CardId,
    controller: crate::id::PlayerId,
    defender: crate::id::PlayerId,
) -> PermanentId {
    let id = creature_card(state, card, controller, 0);
    if let Some(perm) = state.battlefield.iter_mut().find(|p| p.id == id) {
        perm.attacking = Some(crate::combat::AttackTarget::Player(defender));
    }
    id
}

/// Place a creature of `card` under `controller` blocking `blocks`; returns its id.
fn blocker(
    state: &mut GameState,
    card: CardId,
    controller: crate::id::PlayerId,
    blocks: PermanentId,
) -> PermanentId {
    let id = creature_card(state, card, controller, 0);
    if let Some(perm) = state.battlefield.iter_mut().find(|p| p.id == id) {
        perm.blocking = vec![blocks];
    }
    id
}

#[test]
fn issue_153_flying_can_be_blocked_only_by_flying_or_reach_cr_702_9c() {
    // CR 702.9c / 702.17b: a flyer can be blocked only by flying or reach.
    // Tested both directions: a ground creature cannot, flying and reach can.
    let db = db();
    let mut state = GameState::new_two_player();
    let flyer = creature_card(
        &mut state,
        fixture("snapping_drake"),
        crate::id::PlayerId(0),
        0,
    ); // flying
    let ground = creature_card(
        &mut state,
        fixture("walking_corpse"),
        crate::id::PlayerId(1),
        0,
    ); // no keyword
    let other_flyer = creature_card(
        &mut state,
        fixture("snapping_drake"),
        crate::id::PlayerId(1),
        0,
    );
    let reacher = creature_card(
        &mut state,
        fixture("giant_spider"),
        crate::id::PlayerId(1),
        0,
    ); // reach

    assert!(
        !crate::combat::blocker_can_block_attacker(&state, flyer, ground, &db),
        "a ground creature cannot block a flyer (CR 702.9c)"
    );
    assert!(
        crate::combat::blocker_can_block_attacker(&state, flyer, other_flyer, &db),
        "a flyer can block a flyer (CR 702.9c)"
    );
    assert!(
        crate::combat::blocker_can_block_attacker(&state, flyer, reacher, &db),
        "a reach creature can block a flyer (CR 702.17b)"
    );

    // A non-flying attacker imposes no evasion constraint: the ground creature
    // can block a ground attacker.
    let ground_attacker = creature_card(
        &mut state,
        fixture("walking_corpse"),
        crate::id::PlayerId(0),
        0,
    );
    assert!(crate::combat::blocker_can_block_attacker(
        &state,
        ground_attacker,
        ground,
        &db
    ));
}

#[test]
fn issue_153_haste_creature_is_an_attacker_candidate_cr_702_10b() {
    // CR 702.10b: haste exempts a creature from the summoning-sickness attack
    // restriction, so one that entered this very turn may still attack. A
    // vanilla creature that entered this turn stays ineligible (CR 302.6).
    let db = db();
    let mut state = GameState::new_two_player();
    state.turn = 2;
    let hasty = creature_card(
        &mut state,
        fixture("volcanic_dragon"),
        crate::id::PlayerId(0),
        2,
    ); // haste, entered this turn
    let sick = creature_card(
        &mut state,
        fixture("walking_corpse"),
        crate::id::PlayerId(0),
        2,
    ); // entered this turn

    let candidates = crate::combat::attacker_candidates(&state, &db);
    assert!(
        candidates.contains(&hasty),
        "a hasty creature attacks the turn it enters (CR 702.10b)"
    );
    assert!(
        !candidates.contains(&sick),
        "a non-hasty creature that entered this turn cannot attack (CR 302.6)"
    );
}

#[test]
fn issue_154_first_strike_present_needs_two_damage_steps_cr_510_5() {
    // CR 510.5: any first striker in combat means two damage steps; without one
    // a single step suffices.
    let db = keyword_db();
    let mut state = GameState::new_two_player();
    let atk = attacker(
        &mut state,
        id_in(&db, "test_duelist"),
        crate::id::PlayerId(0),
    ); // first strike
    let _blk = blocker(
        &mut state,
        id_in(&db, "test_boar"),
        crate::id::PlayerId(1),
        atk,
    );
    assert!(combat_has_first_strike(&state, &db));

    let mut plain = GameState::new_two_player();
    let a = attacker(&mut plain, id_in(&db, "test_boar"), crate::id::PlayerId(0));
    let _b = blocker(
        &mut plain,
        id_in(&db, "test_boar"),
        crate::id::PlayerId(1),
        a,
    );
    assert!(!combat_has_first_strike(&plain, &db));
}

#[test]
fn issue_154_first_striker_deals_only_in_the_first_step_cr_510_5() {
    // CR 510.5: a first-strike attacker deals in the first-strike step; its
    // vanilla blocker deals in the regular step. `deals_in_step` gates each.
    let db = keyword_db();
    let mut state = GameState::new_two_player();
    let atk = attacker(
        &mut state,
        id_in(&db, "test_duelist"),
        crate::id::PlayerId(0),
    ); // first strike 2/2
    let blk = blocker(
        &mut state,
        id_in(&db, "test_boar"),
        crate::id::PlayerId(1),
        atk,
    ); // vanilla 3/2
    let blocked = blocked_attackers(&state);

    // First-strike step: only the attacker deals (2 to the blocker).
    let first = combat_damage(&state, &db, DamageStep::FirstStrike, &blocked);
    assert_eq!(
        first,
        vec![CombatDamage::ToPermanent {
            permanent: blk,
            amount: 2,
            deathtouch: false,
            lifelink: None,
        }]
    );
    // Regular step: only the (still-present, in this pure call) blocker deals.
    let regular = combat_damage(&state, &db, DamageStep::Regular, &blocked);
    assert_eq!(
        regular,
        vec![CombatDamage::ToPermanent {
            permanent: atk,
            amount: 3,
            deathtouch: false,
            lifelink: None,
        }]
    );
}

#[test]
fn issue_373_double_striker_alone_needs_two_damage_steps_cr_510_5() {
    // CR 702.4b: a double striker deals in the first-strike step, so its mere
    // presence splits combat into two steps even when no creature has plain first
    // strike.
    let db = keyword_db();
    let mut state = GameState::new_two_player();
    let _atk = attacker(
        &mut state,
        id_in(&db, "test_twinstrike"),
        crate::id::PlayerId(0),
    ); // double strike
    assert!(
        combat_has_first_strike(&state, &db),
        "a lone double striker still needs the two-step sequence (CR 510.5)"
    );
}

#[test]
fn issue_373_unblocked_double_striker_deals_in_both_steps_cr_702_4b() {
    // CR 702.4b: an unblocked double striker assigns its power in the first-strike
    // step AND again in the regular step — its power to the defending player twice.
    let db = keyword_db();
    let mut state = GameState::new_two_player();
    let _atk = attacker(
        &mut state,
        id_in(&db, "test_twinstrike"),
        crate::id::PlayerId(0),
    ); // 2/2 double strike
    let blocked = blocked_attackers(&state);

    let first = combat_damage(&state, &db, DamageStep::FirstStrike, &blocked);
    assert_eq!(
        first,
        vec![CombatDamage::ToPlayer {
            player: crate::id::PlayerId(1),
            amount: 2,
            source_commander: None,
            lifelink: None,
        }],
        "the double striker deals in the first-strike step (CR 702.4b)"
    );
    let regular = combat_damage(&state, &db, DamageStep::Regular, &blocked);
    assert_eq!(
        regular,
        vec![CombatDamage::ToPlayer {
            player: crate::id::PlayerId(1),
            amount: 2,
            source_commander: None,
            lifelink: None,
        }],
        "and deals its power again in the regular step (CR 702.4b)"
    );
}

#[test]
fn cr_702_4c_double_strike_with_first_strike_deals_exactly_twice() {
    // CR 702.4c: a creature with both first strike and double strike deals combat
    // damage exactly twice — once per step, never a third time. Combat has only
    // the two steps, and the creature deals its power in each, not more.
    let db = keyword_db();
    let mut state = GameState::new_two_player();
    let _atk = attacker(
        &mut state,
        id_in(&db, "test_paragon"),
        crate::id::PlayerId(0),
    ); // first strike + double strike
    let blocked = blocked_attackers(&state);

    let first = combat_damage(&state, &db, DamageStep::FirstStrike, &blocked);
    let regular = combat_damage(&state, &db, DamageStep::Regular, &blocked);
    // One hit in each step, and there is no third step: exactly twice.
    assert_eq!(
        first,
        vec![CombatDamage::ToPlayer {
            player: crate::id::PlayerId(1),
            amount: 2,
            source_commander: None,
            lifelink: None,
        }],
    );
    assert_eq!(
        regular,
        vec![CombatDamage::ToPlayer {
            player: crate::id::PlayerId(1),
            amount: 2,
            source_commander: None,
            lifelink: None,
        }],
    );
}

#[test]
fn issue_154_deathtouch_makes_one_damage_lethal_for_assignment_cr_510_1e() {
    // CR 510.1e / 702.2b: a deathtouch source needs assign only 1 to a blocker
    // to count as lethal. A 1/1 deathtouch attacker assigns 1 to a 4/5 blocker,
    // flagged deathtouch; the assignment records the deathtouch flag.
    let db = keyword_db();
    let mut state = GameState::new_two_player();
    let atk = attacker(&mut state, id_in(&db, "test_adder"), crate::id::PlayerId(0)); // deathtouch 1/1
    let blk = blocker(
        &mut state,
        id_in(&db, "test_basilisk"),
        crate::id::PlayerId(1),
        atk,
    ); // 4/5
    let blocked = blocked_attackers(&state);

    let batch = combat_damage(&state, &db, DamageStep::Only, &blocked);
    assert!(batch.contains(&CombatDamage::ToPermanent {
        permanent: blk,
        amount: 1,
        deathtouch: true,
        lifelink: None,
    }));
    // The blocker deals its 4 back to the 1/1 attacker.
    assert!(batch.contains(&CombatDamage::ToPermanent {
        permanent: atk,
        amount: 4,
        deathtouch: false,
        lifelink: None,
    }));
}

#[test]
fn issue_374_aura_granted_flying_makes_host_unblockable_and_reverts_cr_702_9c() {
    // CR 613.1f + 702.9c: an Aura granting flying makes its host a flier, so a
    // ground creature cannot block it — exactly as a printed flier. The grant
    // disappears when the Aura leaves.
    let mut state = GameState::new_two_player();
    // M19 prints no Aura granting flying, so the shape is exercised inline
    // (ADR 0009): a `{U}` Aura whose only grant is flying, over a ground body.
    let db = CardDatabase::from_json(
        r#"[
            {"schema_version":1,"functional_id":"test_flight","name":"Test Flight",
             "types":["enchantment"],"subtypes":["Aura"],"mana_cost":"{U}","colors":["blue"],
             "attachment":{"kind":"aura","attach_to":"any_creature","keywords":["flying"]}},
            {"schema_version":1,"functional_id":"test_corpse","name":"Test Corpse",
             "types":["creature"],"subtypes":["Zombie"],"mana_cost":"{1}{B}","colors":["black"],
             "power":2,"toughness":2}
        ]"#,
    )
    .unwrap();
    let corpse = crate::fixtures::id_in(&db, "test_corpse");
    let host = creature_card(&mut state, corpse, crate::id::PlayerId(0), 0); // ground
    let ground = creature_card(&mut state, corpse, crate::id::PlayerId(1), 0);
    // Baseline: a ground creature can block a ground attacker.
    assert!(crate::combat::blocker_can_block_attacker(
        &state, host, ground, &db
    ));

    // Attach the flying-granting Aura to the host.
    let aura = creature_card(
        &mut state,
        crate::fixtures::id_in(&db, "test_flight"),
        crate::id::PlayerId(0),
        0,
    );
    state
        .battlefield
        .iter_mut()
        .find(|p| p.id == aura)
        .unwrap()
        .attached_to = Some(host);
    assert!(
        !crate::combat::blocker_can_block_attacker(&state, host, ground, &db),
        "the enchanted creature is a flier; a ground creature cannot block it (CR 702.9c)"
    );

    // The Aura leaves: the grant reverts and the ground creature can block again.
    state.battlefield.retain(|p| p.id != aura);
    assert!(
        crate::combat::blocker_can_block_attacker(&state, host, ground, &db),
        "removing the Aura reverts the granted flying"
    );
}

#[test]
fn issue_374_granted_deathtouch_is_lethal_in_combat_cr_510_1e() {
    // CR 613.1f + 510.1e: a granted deathtouch behaves in combat exactly like a
    // printed one — an attacker with deathtouch granted until end of turn needs
    // assign only 1 to its blocker to be lethal, flagged deathtouch.
    use crate::state::{Duration, EffectAffects, Modification, StaticEffect};
    let db = db();
    let mut state = GameState::new_two_player();
    let atk = attacker(&mut state, fixture("onakke_ogre"), crate::id::PlayerId(0)); // 4/2, no keyword
    let blk = blocker(
        &mut state,
        fixture("colossal_dreadmaw"),
        crate::id::PlayerId(1),
        atk,
    ); // 6/6
    let source = state.mint_id();
    state.static_effects.push(StaticEffect {
        source,
        affects: EffectAffects::SpecificPermanent(atk),
        modification: Modification::GrantKeyword(Keyword::Deathtouch),
        duration: Duration::UntilEndOfTurn,
    });
    let blocked = blocked_attackers(&state);

    let batch = combat_damage(&state, &db, DamageStep::Only, &blocked);
    assert!(
        batch.contains(&CombatDamage::ToPermanent {
            permanent: blk,
            amount: 1,
            deathtouch: true,
            lifelink: None,
        }),
        "granted deathtouch makes 1 damage lethal to the 6/6 blocker (CR 510.1e)"
    );
}

#[test]
fn issue_154_trample_assigns_lethal_then_excess_to_the_player_cr_702_19e() {
    // CR 702.19e: a blocked trampler assigns just-lethal to its blocker, the
    // rest to the defending player. A 6/6 trampler over a 4/2 Ogre assigns 2
    // (lethal) to the Ogre and 4 to player 1.
    let db = db();
    let mut state = GameState::new_two_player();
    let atk = attacker(
        &mut state,
        fixture("colossal_dreadmaw"),
        crate::id::PlayerId(0),
    ); // trample 6/6
    let blk = blocker(
        &mut state,
        fixture("onakke_ogre"),
        crate::id::PlayerId(1),
        atk,
    ); // 4/2
    let blocked = blocked_attackers(&state);

    let batch = combat_damage(&state, &db, DamageStep::Only, &blocked);
    assert!(batch.contains(&CombatDamage::ToPermanent {
        permanent: blk,
        amount: 2,
        deathtouch: false,
        lifelink: None,
    }));
    assert!(batch.contains(&CombatDamage::ToPlayer {
        player: crate::id::PlayerId(1),
        amount: 4,
        source_commander: None,
        lifelink: None,
    }));
}

#[test]
fn issue_154_deathtouch_trample_assigns_one_per_blocker_rest_to_player() {
    // CR 510.1e + 702.19e together: a deathtouch trampler needs assign only 1
    // per blocker before the rest tramples over. The assignment math is verified
    // by exercising `lethal_needed` directly against a blocker — deathtouch makes
    // 1 lethal, otherwise its full toughness is.
    let db = db();
    let mut state = GameState::new_two_player();
    let blk = creature_card(
        &mut state,
        fixture("giant_spider"),
        crate::id::PlayerId(1),
        0,
    ); // 2/4
    assert_eq!(
        super::super::helpers::lethal_needed(&state, blk, &db, true),
        1,
        "deathtouch: 1 is lethal"
    );
    assert_eq!(
        super::super::helpers::lethal_needed(&state, blk, &db, false),
        4,
        "without deathtouch: full toughness is lethal"
    );
}

#[test]
fn issue_154_lifelink_gains_its_controller_life_in_the_same_batch_cr_702_15e() {
    // CR 702.15e: a lifelink source's damage gains its controller that much life,
    // carried on the assignment itself so the gain is measured against the damage
    // actually dealt. An unblocked 2/1 lifelinker attacking player 1 hits for 2 and
    // names seat 0 as the gainer.
    let db = db();
    let mut state = GameState::new_two_player();
    let _atk = attacker(
        &mut state,
        fixture("child_of_night"),
        crate::id::PlayerId(0),
    ); // lifelink 2/1
    let blocked = blocked_attackers(&state);

    let batch = combat_damage(&state, &db, DamageStep::Only, &blocked);
    assert!(batch.contains(&CombatDamage::ToPlayer {
        player: crate::id::PlayerId(1),
        amount: 2,
        source_commander: None,
        lifelink: Some(crate::id::PlayerId(0)),
    }));
}

#[test]
fn issue_154_blocked_attacker_stays_blocked_when_its_blockers_leave() {
    // CR 509.1h: an attacker recorded as blocked deals no player damage even
    // once its blockers are gone (no trample). Removing the blocker after
    // capturing the blocked set leaves the attacker dealing nothing.
    let db = db();
    let mut state = GameState::new_two_player();
    let atk = attacker(&mut state, fixture("onakke_ogre"), crate::id::PlayerId(0)); // vanilla 3/2, no trample
    let blk = blocker(
        &mut state,
        fixture("onakke_ogre"),
        crate::id::PlayerId(1),
        atk,
    );
    let blocked = blocked_attackers(&state);
    assert_eq!(blocked, vec![atk]);

    // The blocker dies before damage: remove it, keep the blocked snapshot.
    state.battlefield.retain(|p| p.id != blk);
    let batch = combat_damage(&state, &db, DamageStep::Only, &blocked);
    assert!(
        batch.is_empty(),
        "a blocked non-trampler with no surviving blockers deals nothing"
    );
}

#[test]
fn issue_341_split_attacks_route_damage_to_each_chosen_defender() {
    // CR 510.1c: with attackers split across two defenders, each unblocked
    // attacker's damage goes to *its own* chosen defender, not one global one.
    let db = db();
    let mut state = GameState::new_multiplayer(3);
    // Seat 0 attacks: a 4/2 at seat 1 and a 4/2 at seat 2, both unblocked.
    let _at1 = super::super::declaration::tests::attacker_of(
        &mut state,
        fixture("onakke_ogre"),
        crate::id::PlayerId(0),
        crate::id::PlayerId(1),
    );
    let _at2 = super::super::declaration::tests::attacker_of(
        &mut state,
        fixture("onakke_ogre"),
        crate::id::PlayerId(0),
        crate::id::PlayerId(2),
    );
    let blocked = blocked_attackers(&state);
    assert!(blocked.is_empty());

    let batch = combat_damage(&state, &db, DamageStep::Only, &blocked);
    assert!(
        batch.contains(&CombatDamage::ToPlayer {
            player: crate::id::PlayerId(1),
            amount: 4,
            source_commander: None,
            lifelink: None,
        }),
        "the attacker assigned to seat 1 hits seat 1"
    );
    assert!(
        batch.contains(&CombatDamage::ToPlayer {
            player: crate::id::PlayerId(2),
            amount: 4,
            source_commander: None,
            lifelink: None,
        }),
        "the attacker assigned to seat 2 hits seat 2"
    );
}

#[test]
fn issue_341_trample_overflow_routes_to_the_attackers_own_defender() {
    // CR 702.19e: a blocked trampler's overflow goes to the player it is
    // attacking. A 6/6 trampler at seat 2, blocked by seat 2's 4/2, assigns 2
    // (lethal) to the blocker and tramples 4 to seat 2 — never seat 1.
    let db = db();
    let mut state = GameState::new_multiplayer(3);
    let atk = super::super::declaration::tests::attacker_of(
        &mut state,
        fixture("colossal_dreadmaw"),
        crate::id::PlayerId(0),
        crate::id::PlayerId(2),
    ); // trample 6/6
    let blk = blocker(
        &mut state,
        fixture("onakke_ogre"),
        crate::id::PlayerId(2),
        atk,
    ); // 4/2
    let blocked = blocked_attackers(&state);

    let batch = combat_damage(&state, &db, DamageStep::Only, &blocked);
    assert!(batch.contains(&CombatDamage::ToPermanent {
        permanent: blk,
        amount: 2,
        deathtouch: false,
        lifelink: None,
    }));
    assert!(
        batch.contains(&CombatDamage::ToPlayer {
            player: crate::id::PlayerId(2),
            amount: 4,
            source_commander: None,
            lifelink: None,
        }),
        "trample overflow hits the attacker's own defender (seat 2)"
    );
    assert!(
        !batch.contains(&CombatDamage::ToPlayer {
            player: crate::id::PlayerId(1),
            amount: 4,
            source_commander: None,
            lifelink: None,
        }),
        "no damage leaks to the other opponent (seat 1)"
    );
}
