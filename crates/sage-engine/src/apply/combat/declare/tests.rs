//! Declaration: which creatures may attack or block, and the order a multi-block takes.

#![allow(clippy::unwrap_used)]

use super::*;
use crate::apply::test_support::*;

#[test]
fn issue_117_declare_attackers_taps_and_marks_attackers_cr_508_1() {
    // CR 508.1a: the active player declares as attackers untapped creatures
    // they have controlled since the turn began. CR 508.1f: attacking taps them
    // (no vigilance modeled yet).
    let db = db();
    let mut state = at_declare_attackers();
    let attacker = place_permanent(&mut state, fixture("walking_corpse"), PlayerId(0), false, 0);

    // Before declaring, the only action offered to the active player is the
    // declaration itself — no pass, no other action (a turn-based choice).
    let offered = valid(&state, &db);
    // The declaration plus the always-available concede (CR 104.3a).
    assert_eq!(offered.len(), 2);
    assert!(matches!(offered[0], Action::DeclareAttackers { .. }));
    assert!(offered.contains(&Action::Concede));
    assert_eq!(attacker_candidates(&state, &db), vec![attacker]);

    let after = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: atk1(&[attacker]),
        },
        &db,
    );

    let perm = find_perm(&after, attacker);
    assert!(
        perm.attacking.is_some(),
        "declared creature is attacking (CR 508.1a)"
    );
    assert!(perm.tapped, "attacking taps the creature (CR 508.1f)");
    assert!(after.attackers_declared);
    // The declaration made, the step opens its priority round with the active
    // player, who may now pass.
    assert_eq!(after.priority, PlayerId(0));
    assert!(valid(&after, &db).contains(&Action::PassPriority));
}

#[test]
fn issue_117_empty_attacker_declaration_is_legal_cr_508_1a() {
    // CR 508.1a: declaring no attackers is a legal declaration; it advances the
    // step past its turn-based action without tapping anything.
    let db = db();
    let mut state = at_declare_attackers();
    let creature = place_permanent(&mut state, fixture("walking_corpse"), PlayerId(0), false, 0);

    let after = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: Vec::new(),
        },
        &db,
    );

    assert!(after.attackers_declared);
    assert!(find_perm(&after, creature).attacking.is_none());
    assert!(!find_perm(&after, creature).tapped);
    assert!(valid(&after, &db).contains(&Action::PassPriority));
}

#[test]
fn issue_117_summoning_sick_creature_cannot_attack_cr_302_6() {
    // CR 302.6: a creature that has not been controlled continuously since the
    // turn began can't attack. One that entered this very turn is not a
    // candidate, and naming it is an illegal declaration (a no-op).
    let db = db();
    let mut state = at_declare_attackers();
    let sick = place_permanent(&mut state, fixture("walking_corpse"), PlayerId(0), false, 0);
    let this_turn = state.turn;
    set_entered_turn(&mut state, sick, this_turn);

    assert!(attacker_candidates(&state, &db).is_empty());
    let after = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: atk1(&[sick]),
        },
        &db,
    );
    assert_eq!(after, state, "declaring a sick attacker is a no-op");
}

#[test]
fn issue_117_tapped_creature_cannot_attack_cr_508_1a() {
    // CR 508.1a: only untapped creatures can be declared as attackers.
    let db = db();
    let mut state = at_declare_attackers();
    let tapped = place_permanent(&mut state, fixture("walking_corpse"), PlayerId(0), true, 0);

    assert!(attacker_candidates(&state, &db).is_empty());
    let after = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: atk1(&[tapped]),
        },
        &db,
    );
    assert_eq!(after, state, "declaring a tapped attacker is a no-op");
}

#[test]
fn issue_117_defender_declares_blockers_multiple_per_attacker_cr_509_1a() {
    // CR 509.1a: the defending player assigns each blocker to one attacking
    // creature; several blockers may be assigned to the same attacker.
    let db = db();
    let mut state = at_declare_attackers();
    let attacker = place_permanent(&mut state, fixture("walking_corpse"), PlayerId(0), false, 0);
    let blocker_a = place_permanent(&mut state, fixture("walking_corpse"), PlayerId(1), false, 0);
    let blocker_b = place_permanent(&mut state, fixture("walking_corpse"), PlayerId(1), false, 0);

    // Declare the attacker, then pass to the declare-blockers step.
    let state = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: atk1(&[attacker]),
        },
        &db,
    );
    let state = pass_full_round(&state, &db);
    assert_eq!(state.step, Step::DeclareBlockers);
    // The defender (player 1) is the one who must declare, and is offered the
    // declaration; both eligible blockers are candidates.
    assert_eq!(state.priority, PlayerId(1));
    let offered = valid(&state, &db);
    // The declaration plus the always-available concede (CR 104.3a).
    assert_eq!(offered.len(), 2);
    assert!(matches!(offered[0], Action::DeclareBlockers { .. }));
    assert!(offered.contains(&Action::Concede));
    let candidates = blocker_candidates(&state, &db);
    assert!(candidates.contains(&blocker_a) && candidates.contains(&blocker_b));

    // Assign both blockers to the single attacker.
    let after = apply_action(
        &state,
        &Action::DeclareBlockers {
            blocks: vec![
                Block {
                    blocker: blocker_a,
                    attacker,
                },
                Block {
                    blocker: blocker_b,
                    attacker,
                },
            ],
        },
        &db,
    );
    assert_eq!(find_perm(&after, blocker_a).blocking, Some(attacker));
    assert_eq!(find_perm(&after, blocker_b).blocking, Some(attacker));
    assert!(after.blockers_declared);
    // After blockers are declared the active player receives priority (CR 509.4).
    assert_eq!(after.priority, PlayerId(0));
}

#[test]
fn issue_117_tapped_creature_cannot_block_cr_509_1a() {
    // CR 509.1a: a tapped creature can't be declared as a blocker.
    let db = db();
    let mut state = at_declare_attackers();
    let attacker = place_permanent(&mut state, fixture("walking_corpse"), PlayerId(0), false, 0);
    let tapped_blocker =
        place_permanent(&mut state, fixture("walking_corpse"), PlayerId(1), true, 0);
    let state = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: atk1(&[attacker]),
        },
        &db,
    );
    let state = pass_full_round(&state, &db);

    assert!(!blocker_candidates(&state, &db).contains(&tapped_blocker));
    let after = apply_action(
        &state,
        &Action::DeclareBlockers {
            blocks: vec![Block {
                blocker: tapped_blocker,
                attacker,
            }],
        },
        &db,
    );
    assert_eq!(after, state, "declaring a tapped blocker is a no-op");
}

#[test]
fn issue_117_blocker_must_be_assigned_to_an_attacking_creature_cr_509_1a() {
    // CR 509.1a: a blocker is assigned to an *attacking* creature. Assigning it
    // to a creature that is not attacking is an illegal declaration (a no-op).
    let db = db();
    let mut state = at_declare_attackers();
    let attacker = place_permanent(&mut state, fixture("walking_corpse"), PlayerId(0), false, 0);
    let non_attacker =
        place_permanent(&mut state, fixture("walking_corpse"), PlayerId(0), false, 0);
    let blocker = place_permanent(&mut state, fixture("walking_corpse"), PlayerId(1), false, 0);
    let state = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: atk1(&[attacker]),
        },
        &db,
    );
    let state = pass_full_round(&state, &db);

    let after = apply_action(
        &state,
        &Action::DeclareBlockers {
            blocks: vec![Block {
                blocker,
                attacker: non_attacker,
            }],
        },
        &db,
    );
    assert_eq!(after, state, "blocking a non-attacker is a no-op");
}

#[test]
fn issue_117_a_creature_cannot_be_declared_as_two_blocks_cr_509_1a() {
    // CR 509.1a: each blocker is assigned to *one* attacking creature, so the
    // same creature cannot appear as a blocker twice in one declaration.
    let db = db();
    let mut state = at_declare_attackers();
    let atk_a = place_permanent(&mut state, fixture("walking_corpse"), PlayerId(0), false, 0);
    let atk_b = place_permanent(&mut state, fixture("walking_corpse"), PlayerId(0), false, 0);
    let blocker = place_permanent(&mut state, fixture("walking_corpse"), PlayerId(1), false, 0);
    let state = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: atk1(&[atk_a, atk_b]),
        },
        &db,
    );
    let state = pass_full_round(&state, &db);

    let after = apply_action(
        &state,
        &Action::DeclareBlockers {
            blocks: vec![
                Block {
                    blocker,
                    attacker: atk_a,
                },
                Block {
                    blocker,
                    attacker: atk_b,
                },
            ],
        },
        &db,
    );
    assert_eq!(after, state, "one creature blocking twice is a no-op");
}

#[test]
fn issue_117_priority_is_withheld_until_attackers_are_declared_cr_508_1() {
    // CR 508.1: declaring attackers is a turn-based action performed before any
    // player receives priority in the step. The defender is offered nothing
    // until the active player has declared.
    let db = db();
    let mut state = at_declare_attackers();
    let _attacker = place_permanent(&mut state, fixture("walking_corpse"), PlayerId(0), false, 0);

    // The non-active player has no actions during the pre-declaration window.
    let mut defender_view = state.clone();
    defender_view.priority = PlayerId(1);
    assert!(valid(&defender_view, &db).is_empty());
}

#[test]
fn issue_153_vigilant_attacker_stays_untapped_and_can_block_next_turn_cr_702_20b() {
    // CR 702.20b: a creature with vigilance doesn't tap when it attacks, so it
    // stays untapped through combat and is available to block on the opponent's
    // next turn (an untapped creature can block, CR 509.1a). Serra Angel has
    // vigilance; Walking Corpse is a plain control.
    let db = db();
    let mut state = at_declare_attackers();
    let vigilant = place_permanent(&mut state, fixture("sun_sentinel"), PlayerId(0), false, 0);
    let plain = place_permanent(&mut state, fixture("walking_corpse"), PlayerId(0), false, 0);

    let after = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: atk1(&[vigilant, plain]),
        },
        &db,
    );
    assert!(find_perm(&after, vigilant).attacking.is_some());
    assert!(
        !find_perm(&after, vigilant).tapped,
        "vigilance skips the attack tap (CR 702.20b)"
    );
    assert!(
        find_perm(&after, plain).tapped,
        "a non-vigilant attacker still taps (CR 508.1f)"
    );

    // Because it stayed untapped, on the opponent's turn (player 1 active, so
    // player 0 defends) it is an eligible blocker.
    let mut defense = after;
    defense.active_player = PlayerId(1);
    defense.step = Step::DeclareBlockers;
    let opp_attacker = place_permanent(
        &mut defense,
        fixture("walking_corpse"),
        PlayerId(1),
        false,
        0,
    );
    if let Some(p) = defense
        .battlefield
        .iter_mut()
        .find(|p| p.id == opp_attacker)
    {
        p.attacking = Some(crate::combat::AttackTarget::Player(PlayerId(1)));
    }
    assert!(
        blocker_candidates(&defense, &db).contains(&vigilant),
        "the still-untapped vigilant creature can block next turn (CR 509.1a)"
    );
}

#[test]
fn issue_153_hasty_creature_attacks_the_turn_it_enters_cr_702_10b() {
    // CR 702.10b: a creature with haste ignores the summoning-sickness attack
    // restriction, so Volcanic Dragon may attack even though it entered
    // this very turn — where a non-hasty creature could not (CR 302.6).
    let db = db();
    let mut state = at_declare_attackers();
    let hasty = place_permanent(
        &mut state,
        fixture("volcanic_dragon"),
        PlayerId(0),
        false,
        0,
    );
    let this_turn = state.turn;
    set_entered_turn(&mut state, hasty, this_turn);

    assert!(attacker_candidates(&state, &db).contains(&hasty));
    let after = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: atk1(&[hasty]),
        },
        &db,
    );
    assert!(
        find_perm(&after, hasty).attacking.is_some(),
        "a hasty creature attacks the turn it enters (CR 702.10b)"
    );
    assert!(
        find_perm(&after, hasty).tapped,
        "attacking still taps it — it has no vigilance"
    );
}

#[test]
fn issue_153_ground_creature_cannot_block_a_flyer_cr_702_9c() {
    // CR 702.9c / 702.17b: a ground creature assigned to block a flyer is an
    // illegal declaration (a no-op); a reach creature may block it. Snapping
    // Drake flies, Giant Spider has reach, Walking Corpse is a ground creature.
    let db = db();
    let mut state = at_declare_attackers();
    let flyer = place_permanent(&mut state, fixture("snapping_drake"), PlayerId(0), false, 0);
    let ground = place_permanent(&mut state, fixture("walking_corpse"), PlayerId(1), false, 0);
    let reacher = place_permanent(&mut state, fixture("giant_spider"), PlayerId(1), false, 0);

    let state = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: atk1(&[flyer]),
        },
        &db,
    );
    let state = pass_full_round(&state, &db);
    assert_eq!(state.step, Step::DeclareBlockers);

    // A ground creature cannot be assigned to block the flyer: a no-op.
    let blocked_by_ground = apply_action(
        &state,
        &Action::DeclareBlockers {
            blocks: vec![Block {
                blocker: ground,
                attacker: flyer,
            }],
        },
        &db,
    );
    assert_eq!(
        blocked_by_ground, state,
        "a ground creature cannot block a flyer (CR 702.9c)"
    );

    // A reach creature can: the block is recorded.
    let blocked_by_reach = apply_action(
        &state,
        &Action::DeclareBlockers {
            blocks: vec![Block {
                blocker: reacher,
                attacker: flyer,
            }],
        },
        &db,
    );
    assert_eq!(
        find_perm(&blocked_by_reach, reacher).blocking,
        Some(flyer),
        "a reach creature can block a flyer (CR 702.17b)"
    );
    assert!(blocked_by_reach.blockers_declared);
}

#[test]
fn issue_346_attacker_orders_its_blockers_and_that_chooses_which_dies() {
    // CR 510.1: the attacking player orders a multi-blocked attacker's blockers,
    // and lethal-before-next assignment follows that order. A 3-power attacker
    // blocked by two 2-toughness creatures kills whichever it orders FIRST (it
    // takes the lethal 2; the second takes the leftover 1 and survives), so the
    // chosen order — not battlefield order — decides the casualty.
    let db = combat_db();
    let mut state = at_declare_attackers();
    let attacker = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(0), false, 0);
    let blk_a = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(1), false, 0);
    let blk_b = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(1), false, 0);

    let state = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: atk1(&[attacker]),
        },
        &db,
    );
    let state = pass_full_round(&state, &db);
    let state = apply_action(
        &state,
        &Action::DeclareBlockers {
            blocks: vec![
                Block {
                    blocker: blk_a,
                    attacker,
                },
                Block {
                    blocker: blk_b,
                    attacker,
                },
            ],
        },
        &db,
    );

    // The declaration owes an ordering decision to the attacking player, and only
    // that action (plus concede) is offered.
    assert_eq!(
        crate::combat::pending_damage_order(&state),
        Some(PlayerId(0))
    );
    let offered = valid(&state, &db);
    assert!(offered
        .iter()
        .any(|a| matches!(a, Action::OrderCombatDamage { .. })));
    assert!(!offered.iter().any(|a| matches!(a, Action::PassPriority)));

    // Order blk_b first, the reverse of battlefield order.
    let state = apply_action(
        &state,
        &Action::OrderCombatDamage {
            orders: vec![DamageOrder {
                attacker,
                blockers: vec![blk_b, blk_a],
            }],
        },
        &db,
    );
    let state = pass_full_round(&state, &db);
    assert_eq!(state.step, Step::CombatDamage);

    let present = |id| state.battlefield.iter().any(|p| p.id == id);
    assert!(
        !present(blk_b),
        "the first-ordered blocker took the lethal damage"
    );
    assert!(
        present(blk_a),
        "the second-ordered blocker survived on the leftover 1"
    );
}

#[test]
fn issue_346_a_single_blocker_needs_no_damage_order_decision() {
    // CR 510.1: an attacker blocked by one creature has no assignment choice, so
    // no ordering decision is offered — the declare-blockers priority round opens
    // straight away.
    let db = combat_db();
    let mut state = at_declare_attackers();
    let attacker = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(0), false, 0);
    let blocker = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(1), false, 0);
    let state = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: atk1(&[attacker]),
        },
        &db,
    );
    let state = pass_full_round(&state, &db);
    let state = apply_action(
        &state,
        &Action::DeclareBlockers {
            blocks: vec![Block { blocker, attacker }],
        },
        &db,
    );
    assert_eq!(crate::combat::pending_damage_order(&state), None);
    assert!(valid(&state, &db)
        .iter()
        .any(|a| matches!(a, Action::PassPriority)));
}

#[test]
fn issue_344_split_attacks_each_defender_declares_in_apnap_order() {
    // CR 509.1 + 101.4: each attacked player declares their own blockers, seat 1
    // then seat 2; combat is not "done" until both have declared.
    let db = db();
    let (state, atk_a, atk_b, blk1, blk2) = split_combat_at_declare_blockers();
    assert_eq!(state.priority, PlayerId(1), "seat 1 declares first (APNAP)");

    let after1 = apply_action(
        &state,
        &Action::DeclareBlockers {
            blocks: vec![Block {
                blocker: blk1,
                attacker: atk_a,
            }],
        },
        &db,
    );
    assert_eq!(find_perm(&after1, blk1).blocking, Some(atk_a));
    assert!(
        !after1.blockers_declared,
        "seat 2 still owes a declaration, so combat is not done"
    );
    assert_eq!(after1.priority, PlayerId(2), "seat 2 declares next (APNAP)");

    let after2 = apply_action(
        &after1,
        &Action::DeclareBlockers {
            blocks: vec![Block {
                blocker: blk2,
                attacker: atk_b,
            }],
        },
        &db,
    );
    assert_eq!(find_perm(&after2, blk2).blocking, Some(atk_b));
    assert!(
        after2.blockers_declared,
        "both attacked players declared — the step is done"
    );
    assert_eq!(
        after2.priority,
        PlayerId(0),
        "the priority round opens with the active player (CR 509.4)"
    );
}

#[test]
fn issue_344_a_defender_cannot_block_an_attacker_attacking_someone_else() {
    // CR 509.1a: seat 1 may block only the attacker attacking seat 1. Assigning
    // its blocker to the attacker attacking seat 2 is illegal — a no-op.
    let db = db();
    let (state, _atk_a, atk_b, blk1, _blk2) = split_combat_at_declare_blockers();

    let rejected = apply_action(
        &state,
        &Action::DeclareBlockers {
            blocks: vec![Block {
                blocker: blk1,
                attacker: atk_b, // attacking seat 2, not seat 1
            }],
        },
        &db,
    );
    assert_eq!(
        rejected, state,
        "blocking an attacker attacking another player is rejected"
    );
}
