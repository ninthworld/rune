//! Double strike (CR 702.4b): a creature that deals in both damage steps, and how the
//! chosen damage order and trample carry across them.

#![allow(clippy::unwrap_used)]

use super::*;
use crate::apply::test_support::*;

#[test]
fn issue_373_unblocked_double_striker_deals_its_power_twice_cr_702_4b() {
    // CR 702.4b: an unblocked double striker deals combat damage in both the
    // first-strike and the regular step — a 2/2 double striker hits the defending
    // player for 2 twice, so it loses 4.
    let db = combat_db();
    let mut state = at_declare_attackers();
    let start_life = state.players[1].life;
    let striker = place_permanent(
        &mut state,
        id_in(&db, "test_twinstrike"),
        PlayerId(0),
        false,
        0,
    );

    let after = run_combat(&state, vec![striker], Vec::new(), &db);

    assert_eq!(
        after.players[1].life,
        start_life - 4,
        "a 2/2 double striker deals its power twice (CR 702.4b)"
    );
    assert!(alive(&after, striker), "the unblocked striker is untouched");
}

#[test]
fn issue_373_blocked_double_striker_deals_no_regular_damage_without_trample_cr_702_4b() {
    // CR 702.4b: a 2/2 double striker kills its 3/2 blocker in the first-strike
    // step (2 ≥ 2). With its blocker dead and no trample, its regular-step strike
    // has nowhere to go — it deals no damage to anything, and takes none back (the
    // blocker died before it could strike).
    let db = combat_db();
    let mut state = at_declare_attackers();
    let start_life = state.players[1].life;
    let striker = place_permanent(
        &mut state,
        id_in(&db, "test_twinstrike"),
        PlayerId(0),
        false,
        0,
    );
    let boar = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(1), false, 0);

    let after = run_combat(
        &state,
        vec![striker],
        vec![Block {
            blocker: boar,
            attacker: striker,
        }],
        &db,
    );

    assert!(!alive(&after, boar), "the blocker died to first strike");
    assert!(
        alive(&after, striker),
        "the striker survives its dead blocker"
    );
    assert_eq!(
        find_perm(&after, striker).damage,
        0,
        "the blocker never struck back (CR 510.5)"
    );
    assert_eq!(
        after.players[1].life, start_life,
        "a blocked non-trampler's regular strike hits nothing (CR 509.1h)"
    );
}

#[test]
fn issue_373_double_strike_trample_carries_the_regular_strike_over_a_dead_blocker_cr_702_4b() {
    // CR 702.4b + 702.19e: a 3/3 double-strike trampler blocked by a 3/2 Boar
    // assigns 2 (lethal) to the Boar and tramples 1 in the first-strike step; the
    // Boar dies before the regular step, so the whole 3 of the regular strike
    // tramples to the player. The defender loses 1 + 3 = 4.
    let db = combat_db();
    let mut state = at_declare_attackers();
    let start_life = state.players[1].life;
    let trampler = place_permanent(
        &mut state,
        id_in(&db, "test_twintrample"),
        PlayerId(0),
        false,
        0,
    );
    let boar = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(1), false, 0);

    let after = run_combat(
        &state,
        vec![trampler],
        vec![Block {
            blocker: boar,
            attacker: trampler,
        }],
        &db,
    );

    assert!(!alive(&after, boar), "the blocker died to first strike");
    assert!(alive(&after, trampler), "the trampler survives");
    assert_eq!(
        after.players[1].life,
        start_life - 4,
        "1 trample excess in the first step, the full 3 over the dead blocker in \
         the regular step (CR 702.4b/702.19e)"
    );
}

#[test]
fn issue_373_double_striker_slain_in_the_first_step_deals_no_regular_damage_cr_702_4b() {
    // CR 702.4b: a double striker that dies during/after the first-strike step
    // deals no regular-step damage. A 2/2 double striker attacks a 3/3 first-strike
    // blocker: in the first-strike step both deal — the striker marks 2 on the ward
    // (which survives), the ward's 3 kills the striker. The dead striker never
    // deals its second hit, so the ward keeps exactly 2 marked (a second hit would
    // make it 4 and destroy it).
    let db = combat_db();
    let mut state = at_declare_attackers();
    let striker = place_permanent(
        &mut state,
        id_in(&db, "test_twinstrike"),
        PlayerId(0),
        false,
        0,
    );
    let ward = place_permanent(&mut state, id_in(&db, "test_ward"), PlayerId(1), false, 0);

    let after = run_combat(
        &state,
        vec![striker],
        vec![Block {
            blocker: ward,
            attacker: striker,
        }],
        &db,
    );

    assert!(
        !alive(&after, striker),
        "the double striker took lethal first strike"
    );
    assert!(
        alive(&after, ward),
        "the 3/3 ward survived the one 2-damage hit"
    );
    assert_eq!(
        find_perm(&after, ward).damage,
        2,
        "the slain double striker dealt no regular-step damage (CR 702.4b)"
    );
}

#[test]
fn issue_399_double_strike_first_step_assigns_lethal_then_next_in_chosen_order_cr_510_1c() {
    // ACCEPTANCE 1 / CR 510.1c–d: a double striker blocked by TWO creatures under a
    // player-chosen order assigns lethal-then-next in the FIRST-STRIKE step. A 3/4
    // double striker ordered [boar_b, boar_a] over two 3/2 Boars puts the lethal 2
    // on the first-ordered Boar, then the leftover 1 on the next — battlefield order
    // (boar_a, boar_b) is overridden by the chosen order.
    let db = combat_db();
    let mut state = at_declare_attackers();
    let start_life = state.players[1].life;
    let striker = place_permanent(
        &mut state,
        id_in(&db, "test_twinsoldier"),
        PlayerId(0),
        false,
        0,
    ); // 3/4 double strike
    let boar_a = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(1), false, 0);
    let boar_b = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(1), false, 0);

    let blocks = vec![
        Block {
            blocker: boar_a,
            attacker: striker,
        },
        Block {
            blocker: boar_b,
            attacker: striker,
        },
    ];
    let orders = vec![DamageOrder {
        attacker: striker,
        blockers: vec![boar_b, boar_a], // reverse of battlefield order
    }];
    let ordered = ordered_before_damage(&state, vec![striker], blocks.clone(), orders.clone(), &db);

    // First-strike step: lethal (2) to the first-ordered Boar, remainder (1) to the
    // next — in the chosen order, not battlefield order (CR 510.1c–d).
    let blocked = blocked_attackers(&ordered);
    let first = combat_damage(&ordered, &db, DamageStep::FirstStrike, &blocked);
    assert_eq!(
        first,
        vec![
            CombatDamage::ToPermanent {
                permanent: boar_b,
                amount: 2,
                deathtouch: false,
                lifelink: None,
            },
            CombatDamage::ToPermanent {
                permanent: boar_a,
                amount: 1,
                deathtouch: false,
                lifelink: None,
            },
        ],
        "first-strike step honors the chosen order: lethal to boar_b, remainder to boar_a"
    );

    // End-to-end through the real pipeline: boar_b dies to first strike; the regular
    // step finishes boar_a (its 1 marked + a fresh 1) while the survivor's 3 marks
    // the 3/4 striker, which lives. No trample, so the regular excess hits nothing.
    let after = run_combat_ordered(&state, vec![striker], blocks, orders, &db);
    assert!(
        !alive(&after, boar_b),
        "first-ordered Boar died to first strike"
    );
    assert!(
        !alive(&after, boar_a),
        "second-ordered Boar died in the regular step"
    );
    assert!(alive(&after, striker), "the 3/4 double striker survives");
    assert_eq!(
        find_perm(&after, striker).damage,
        3,
        "only the surviving Boar struck back (CR 510.5)"
    );
    assert_eq!(
        after.players[1].life, start_life,
        "no trample: the regular-step excess is dealt nowhere (CR 702.19e)"
    );
}

#[test]
fn issue_399_double_strike_regular_step_honors_order_over_survivors_only_cr_510_1c() {
    // ACCEPTANCE 2 / CR 510.1c–d across CR 702.4b steps: the regular step honors the
    // SAME chosen order against the SURVIVING blockers — a blocker killed in the
    // first-strike step receives NOTHING in the second step, and (no trample) excess
    // carries over nowhere. A 3/4 double striker ordered [o2, o3, o1] over three 1/3
    // Otters kills o2 in the first-strike step; the regular step then assigns to
    // o3 (next in the surviving order), leaving o1 untouched.
    let db = combat_db();
    let mut state = at_declare_attackers();
    let start_life = state.players[1].life;
    let striker = place_permanent(
        &mut state,
        id_in(&db, "test_twinsoldier"),
        PlayerId(0),
        false,
        0,
    ); // 3/4 double strike, power 3
    let o1 = place_permanent(&mut state, id_in(&db, "test_otter"), PlayerId(1), false, 0); // 1/3
    let o2 = place_permanent(&mut state, id_in(&db, "test_otter"), PlayerId(1), false, 0);
    let o3 = place_permanent(&mut state, id_in(&db, "test_otter"), PlayerId(1), false, 0);

    let blocks = vec![
        Block {
            blocker: o1,
            attacker: striker,
        },
        Block {
            blocker: o2,
            attacker: striker,
        },
        Block {
            blocker: o3,
            attacker: striker,
        },
    ];
    let orders = vec![DamageOrder {
        attacker: striker,
        blockers: vec![o2, o3, o1],
    }];
    let mut walk = ordered_before_damage(&state, vec![striker], blocks, orders, &db);
    let blocked = blocked_attackers(&walk);

    // First-strike step: all 3 power goes to the first-ordered Otter, which is
    // lethal (toughness 3) — o2 alone is struck.
    let first = combat_damage(&walk, &db, DamageStep::FirstStrike, &blocked);
    assert_eq!(
        first,
        vec![CombatDamage::ToPermanent {
            permanent: o2,
            amount: 3,
            deathtouch: false,
            lifelink: None,
        }],
        "the first-strike step spends all lethal on the first-ordered survivor"
    );
    apply_combat_batch(&mut walk, first, &[], &db);
    run_state_based_actions(&mut walk, &db); // CR 510.5: SBAs between the two steps
    assert!(!alive(&walk, o2), "o2 died in the first-strike step");

    // Regular step: the SAME order is honored over the SURVIVORS [o3, o1] — o3 (next)
    // takes the lethal 3, o1 gets nothing, and the dead o2 receives nothing.
    let regular = combat_damage(&walk, &db, DamageStep::Regular, &blocked);
    assert!(
        !regular.iter().any(|d| matches!(
            d,
            CombatDamage::ToPermanent { permanent, .. } if *permanent == o2
        )),
        "a blocker killed in the first-strike step receives nothing in the regular step"
    );
    assert!(
        regular.contains(&CombatDamage::ToPermanent {
            permanent: o3,
            amount: 3,
            deathtouch: false,
            lifelink: None,
        }),
        "the regular step assigns lethal to o3, the next survivor in the chosen order"
    );
    assert!(
        !regular.iter().any(|d| matches!(
            d,
            CombatDamage::ToPermanent { permanent, .. } if *permanent == o1
        )),
        "o1, last in the order, receives no attacker damage"
    );
    apply_combat_batch(&mut walk, regular, &[], &db);
    run_state_based_actions(&mut walk, &db);

    assert!(
        !alive(&walk, o3),
        "o3 took its lethal 3 in the regular step"
    );
    assert!(alive(&walk, o1), "o1 survived — it was never assigned to");
    assert_eq!(
        find_perm(&walk, o1).damage,
        0,
        "the untouched survivor has no marked damage"
    );
    assert!(
        alive(&walk, striker),
        "the 3/4 striker survives o3+o1's 2 back"
    );
    assert_eq!(
        walk.players[1].life, start_life,
        "no trample: nothing carries over to the defending player"
    );

    // The real pipeline reaches the same outcome end-to-end.
    let after = run_combat_ordered(
        &state,
        vec![striker],
        vec![
            Block {
                blocker: o1,
                attacker: striker,
            },
            Block {
                blocker: o2,
                attacker: striker,
            },
            Block {
                blocker: o3,
                attacker: striker,
            },
        ],
        vec![DamageOrder {
            attacker: striker,
            blockers: vec![o2, o3, o1],
        }],
        &db,
    );
    assert!(!alive(&after, o2) && !alive(&after, o3));
    assert!(alive(&after, o1) && alive(&after, striker));
    assert_eq!(after.players[1].life, start_life);
}

#[test]
fn issue_399_double_strike_trample_carries_over_in_both_steps_cr_702_19e() {
    // ACCEPTANCE 3 / CR 702.4b + 702.19e: a TRAMPLING double striker blocked by two
    // creatures carries excess to the defending player in BOTH steps. A 5/5 double
    // strike + trample ordered [boar_b, boar_a] over two 3/2 Boars assigns lethal 2
    // to each (in the chosen order) and tramples 1 over in the first-strike step;
    // both Boars are dead by the regular step, so its full 5 tramples over. The
    // defender loses 1 + 5 = 6.
    let db = combat_db();
    let mut state = at_declare_attackers();
    let start_life = state.players[1].life;
    let striker = place_permanent(
        &mut state,
        id_in(&db, "test_twinjugg"),
        PlayerId(0),
        false,
        0,
    ); // 5/5 double strike + trample
    let boar_a = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(1), false, 0);
    let boar_b = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(1), false, 0);

    let blocks = vec![
        Block {
            blocker: boar_a,
            attacker: striker,
        },
        Block {
            blocker: boar_b,
            attacker: striker,
        },
    ];
    let orders = vec![DamageOrder {
        attacker: striker,
        blockers: vec![boar_b, boar_a],
    }];
    let mut walk =
        ordered_before_damage(&state, vec![striker], blocks.clone(), orders.clone(), &db);
    let blocked = blocked_attackers(&walk);

    // First-strike step: lethal 2 to each Boar in the chosen order, then trample 1
    // to the defending player (CR 702.19e).
    let first = combat_damage(&walk, &db, DamageStep::FirstStrike, &blocked);
    assert_eq!(
        first,
        vec![
            CombatDamage::ToPermanent {
                permanent: boar_b,
                amount: 2,
                deathtouch: false,
                lifelink: None,
            },
            CombatDamage::ToPermanent {
                permanent: boar_a,
                amount: 2,
                deathtouch: false,
                lifelink: None,
            },
            CombatDamage::ToPlayer {
                player: PlayerId(1),
                amount: 1,
                source_commander: None,
                lifelink: None,
            },
        ],
        "first-strike step: lethal to each blocker in order, then 1 tramples over"
    );
    apply_combat_batch(&mut walk, first, &[], &db);
    run_state_based_actions(&mut walk, &db);
    assert!(
        !alive(&walk, boar_a) && !alive(&walk, boar_b),
        "both Boars took lethal first strike"
    );

    // Regular step: no blockers survive, so the whole 5 tramples over (CR 702.19e).
    let regular = combat_damage(&walk, &db, DamageStep::Regular, &blocked);
    assert_eq!(
        regular,
        vec![CombatDamage::ToPlayer {
            player: PlayerId(1),
            amount: 5,
            source_commander: None,
            lifelink: None,
        }],
        "regular step: with every blocker dead, the full power tramples over"
    );

    // End-to-end through the real pipeline: the defender loses 1 + 5 = 6.
    let after = run_combat_ordered(&state, vec![striker], blocks, orders, &db);
    assert!(alive(&after, striker), "the 5/5 trampler is untouched");
    assert_eq!(
        after.players[1].life,
        start_life - 6,
        "trample carries over in BOTH steps: 1 then 5 (CR 702.4b/702.19e)"
    );
}
