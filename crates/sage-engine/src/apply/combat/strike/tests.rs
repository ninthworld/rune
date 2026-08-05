//! Combat damage: the batch, the keywords that shape it, and clearing combat afterwards.

#![allow(clippy::unwrap_used)]

use super::*;
use crate::apply::test_support::*;

#[test]
fn issue_117_end_of_combat_removes_creatures_from_combat_cr_511_3() {
    // CR 511.3: at end of combat, all creatures are removed from combat — the
    // attacking flag and blocking assignments are cleared. Uses Giant Spiders
    // (2/4) so both survive the combat-damage step (issue #118) and are still on
    // the battlefield to check at end of combat.
    let db = db();
    let mut state = at_declare_attackers();
    let attacker = place_permanent(&mut state, fixture("giant_spider"), PlayerId(0), false, 0);
    let blocker = place_permanent(&mut state, fixture("giant_spider"), PlayerId(1), false, 0);

    // Declare attackers, pass to declare blockers, declare a block, then pass
    // through combat-damage into end-of-combat.
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
    // Passes: declare-blockers round → combat damage → end of combat.
    let state = pass_full_round(&state, &db); // → CombatDamage
    assert_eq!(state.step, Step::CombatDamage);
    let state = pass_full_round(&state, &db); // → EndCombat (turn-based action runs)
    assert_eq!(state.step, Step::EndCombat);

    assert!(find_perm(&state, attacker).attacking.is_none());
    assert_eq!(find_perm(&state, blocker).blocking, None);
}

#[test]
fn issue_118_unblocked_attacker_damages_the_defending_player_cr_510_1c() {
    // CR 510.1c: an unblocked attacker assigns its combat damage to the player
    // it is attacking. A 3/2 test Boar hits the defender for 3.
    let db = combat_db();
    let mut state = at_declare_attackers();
    let attacker = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(0), false, 0);
    let start_life = state.players[1].life;

    let after = run_combat(&state, vec![attacker], Vec::new(), &db);

    assert_eq!(
        after.players[1].life,
        start_life - 3,
        "unblocked 3/2 deals 3 to the defending player (CR 510.1c)"
    );
    // The unblocked attacker took no damage and survives.
    assert!(alive(&after, attacker));
    assert_eq!(find_perm(&after, attacker).damage, 0);
}

#[test]
fn issue_118_blocked_attacker_and_blocker_deal_lethal_and_both_die_cr_510_704_5g() {
    // CR 510.1c: a blocked attacker and its blocker deal combat damage to each
    // other. CR 704.5g: each takes lethal damage and is destroyed. Two 3/2
    // test Boars trade — both go to their owners' graveyards, and the defending
    // player takes no damage.
    let db = combat_db();
    let mut state = at_declare_attackers();
    let attacker = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(0), false, 0);
    let blocker = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(1), false, 0);
    let start_life = state.players[1].life;

    let after = run_combat(
        &state,
        vec![attacker],
        vec![Block { blocker, attacker }],
        &db,
    );

    assert!(!alive(&after, attacker), "attacker took lethal (CR 704.5g)");
    assert!(!alive(&after, blocker), "blocker took lethal (CR 704.5g)");
    assert_eq!(after.players[0].graveyard.len(), 1);
    assert_eq!(after.players[1].graveyard.len(), 1);
    assert_eq!(
        after.players[1].life, start_life,
        "a blocked attacker deals no damage to the defending player"
    );
}

#[test]
fn issue_118_multi_block_mutual_destruction_cr_510_1c() {
    // CR 510.1c multi-block: a 4/5 Basilisk double-blocked by two 3/2 Boars
    // assigns its 4 power across the blockers in battlefield order,
    // lethal-per-blocker (2 each) — killing both — while the blockers deal a
    // combined 6 back, lethal to the 5-toughness attacker (CR 704.5g). All
    // three creatures are destroyed.
    let db = combat_db();
    let mut state = at_declare_attackers();
    let attacker = place_permanent(
        &mut state,
        id_in(&db, "test_basilisk"),
        PlayerId(0),
        false,
        0,
    );
    let blocker_a = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(1), false, 0);
    let blocker_b = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(1), false, 0);

    let after = run_combat(
        &state,
        vec![attacker],
        vec![
            Block {
                blocker: blocker_a,
                attacker,
            },
            Block {
                blocker: blocker_b,
                attacker,
            },
        ],
        &db,
    );

    assert!(!alive(&after, attacker), "4/5 dies to 3+3 combat damage");
    assert!(!alive(&after, blocker_a), "first blocker took lethal 2");
    assert!(!alive(&after, blocker_b), "second blocker took lethal 2");
    assert_eq!(after.players[0].graveyard.len(), 1);
    assert_eq!(after.players[1].graveyard.len(), 2);
}

#[test]
fn issue_118_multi_block_assigns_lethal_in_battlefield_order_cr_510_1c() {
    // CR 510.1c: with no player-chosen order (deferred), the default splits the
    // attacker's power across blockers in battlefield order, assigning each
    // just-lethal before the next. A 4/5 Basilisk double-blocked by two 1/3
    // Otters assigns 3 (lethal) to the first Otter and the remaining 1 to the
    // second, so only the first dies; the leftover cannot spill further (no
    // trample). The Basilisk survives the 1+1 it takes, with that damage marked.
    let db = combat_db();
    let mut state = at_declare_attackers();
    let attacker = place_permanent(
        &mut state,
        id_in(&db, "test_basilisk"),
        PlayerId(0),
        false,
        0,
    );
    let first = place_permanent(&mut state, id_in(&db, "test_otter"), PlayerId(1), false, 0);
    let second = place_permanent(&mut state, id_in(&db, "test_otter"), PlayerId(1), false, 0);

    let after = run_combat(
        &state,
        vec![attacker],
        vec![
            Block {
                blocker: first,
                attacker,
            },
            Block {
                blocker: second,
                attacker,
            },
        ],
        &db,
    );

    assert!(
        !alive(&after, first),
        "first blocker took lethal 3 (1/3 Otter)"
    );
    assert!(
        alive(&after, second),
        "second blocker took only the leftover 1 and survives"
    );
    assert_eq!(
        find_perm(&after, second).damage,
        1,
        "the remaining 1 damage is marked on the second blocker"
    );
    assert!(
        alive(&after, attacker),
        "the 4/5 survives 1+1 combat damage"
    );
    assert_eq!(
        find_perm(&after, attacker).damage,
        2,
        "both blockers' 1 power is marked on the attacker"
    );
}

#[test]
fn issue_118_combat_life_loss_flows_into_the_life_sba_cr_704_5a() {
    // CR 704.5a: a player at 0 or less life loses. Unblocked combat damage
    // (CR 510) reduces life, and the same SBA loop that runs after the action
    // registers the loss. Defender at 3 life takes 4 from a Basilisk and loses.
    let db = combat_db();
    let mut state = at_declare_attackers();
    state.players[1].life = 3;
    let attacker = place_permanent(
        &mut state,
        id_in(&db, "test_basilisk"),
        PlayerId(0),
        false,
        0,
    );

    let after = run_combat(&state, vec![attacker], Vec::new(), &db);

    assert_eq!(after.players[1].life, -1);
    assert!(
        after.players[1].has_lost,
        "combat life loss flows into the life ≤ 0 SBA (CR 704.5a)"
    );
    assert!(!after.players[0].has_lost);
}

#[test]
fn issue_118_combat_marked_damage_is_cleared_at_cleanup_cr_514_2() {
    // CR 514.2: marked damage is removed at cleanup. A 4/5 Basilisk that
    // survives combat carries marked damage through the rest of the turn; by
    // the time the turn passes to the opponent, its combat cleanup has wiped it.
    let db = combat_db();
    let mut state = at_declare_attackers();
    let attacker = place_permanent(
        &mut state,
        id_in(&db, "test_basilisk"),
        PlayerId(0),
        false,
        0,
    );
    let blocker = place_permanent(&mut state, id_in(&db, "test_otter"), PlayerId(1), false, 0);

    let mut state = run_combat(
        &state,
        vec![attacker],
        vec![Block { blocker, attacker }],
        &db,
    );
    assert!(alive(&state, attacker));
    assert_eq!(
        find_perm(&state, attacker).damage,
        1,
        "1 damage marked in combat"
    );

    // Pass rounds until the turn advances to the opponent; the active player's
    // cleanup (CR 514.2) runs on the way and clears the marked damage.
    let mut guard = 0;
    while state.turn == 2 {
        state = pass_full_round(&state, &db);
        guard += 1;
        assert!(guard < 40, "combat should reach the next turn");
    }
    assert_eq!(
        find_perm(&state, attacker).damage,
        0,
        "marked damage is cleared at cleanup (CR 514.2)"
    );
}

#[test]
fn issue_154_first_striker_kills_its_blocker_before_it_strikes_back_cr_510_5() {
    // CR 510.5: a 2/2 first striker deals in the first-strike step, killing a
    // 3/2 Boar (2 ≥ 2) before the regular step — so the Boar deals no damage
    // back and the first striker survives untouched, though a 3/2 would
    // otherwise have killed it.
    let db = combat_db();
    let mut state = at_declare_attackers();
    let striker = place_permanent(
        &mut state,
        id_in(&db, "test_duelist"),
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
        "the first striker survives — its blocker never dealt damage"
    );
    assert_eq!(
        find_perm(&after, striker).damage,
        0,
        "no damage was dealt back to the first striker (CR 510.5)"
    );
}

#[test]
fn issue_154_two_first_strikers_still_trade_cr_510_5() {
    // CR 510.5: two 2/2 first strikers both deal in the first-strike step, so
    // they trade normally — each deals lethal to the other simultaneously.
    let db = combat_db();
    let mut state = at_declare_attackers();
    let attacker = place_permanent(
        &mut state,
        id_in(&db, "test_duelist"),
        PlayerId(0),
        false,
        0,
    );
    let blocker = place_permanent(
        &mut state,
        id_in(&db, "test_duelist"),
        PlayerId(1),
        false,
        0,
    );

    let after = run_combat(
        &state,
        vec![attacker],
        vec![Block { blocker, attacker }],
        &db,
    );

    assert!(
        !alive(&after, attacker),
        "first striker took lethal first strike"
    );
    assert!(
        !alive(&after, blocker),
        "first striker took lethal first strike"
    );
}

#[test]
fn issue_154_deathtouch_one_damage_destroys_a_big_creature_cr_704_5h() {
    // CR 702.2b / 704.5h: a 1/1 deathtouch blocker deals 1 to a 4/5 attacker,
    // which is not lethal by toughness (1 < 5) but is lethal by deathtouch — the
    // Basilisk is destroyed. The 1/1 dies to the Basilisk's 4 (CR 704.5g).
    let db = combat_db();
    let mut state = at_declare_attackers();
    let basilisk = place_permanent(
        &mut state,
        id_in(&db, "test_basilisk"),
        PlayerId(0),
        false,
        0,
    );
    let adder = place_permanent(&mut state, id_in(&db, "test_adder"), PlayerId(1), false, 0);

    let after = run_combat(
        &state,
        vec![basilisk],
        vec![Block {
            blocker: adder,
            attacker: basilisk,
        }],
        &db,
    );

    assert!(
        !alive(&after, basilisk),
        "1 deathtouch damage destroys the 4/5 (CR 704.5h)"
    );
    assert!(
        !alive(&after, adder),
        "the 1/1 took the Basilisk's 4 (CR 704.5g)"
    );
    assert!(
        after.deathtouch_struck.is_empty(),
        "the deathtouch flag is consumed by the SBA loop"
    );
}

#[test]
fn issue_154_deathtouch_attacker_kills_the_five_five_it_strikes() {
    // Acceptance: a deathtouch 1/1 kills a large creature in combat. The 1/1
    // attacker assigns 1 (deathtouch-lethal) to a 4/5 blocker; the blocker is
    // destroyed by CR 704.5h.
    let db = combat_db();
    let mut state = at_declare_attackers();
    let adder = place_permanent(&mut state, id_in(&db, "test_adder"), PlayerId(0), false, 0);
    let basilisk = place_permanent(
        &mut state,
        id_in(&db, "test_basilisk"),
        PlayerId(1),
        false,
        0,
    );

    let after = run_combat(
        &state,
        vec![adder],
        vec![Block {
            blocker: basilisk,
            attacker: adder,
        }],
        &db,
    );

    assert!(
        !alive(&after, basilisk),
        "deathtouch kills the 4/5 (CR 704.5h)"
    );
}

#[test]
fn issue_154_trample_over_a_chump_block_hits_the_player_cr_702_19e() {
    // CR 702.19e: a blocked 5/4 trampler assigns 2 (lethal) to a 3/2 Boar and
    // tramples the remaining 3 to the defending player.
    let db = combat_db();
    let mut state = at_declare_attackers();
    let start_life = state.players[1].life;
    let trampler = place_permanent(
        &mut state,
        id_in(&db, "test_trampler"),
        PlayerId(0),
        false,
        0,
    );
    let chump = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(1), false, 0);

    let after = run_combat(
        &state,
        vec![trampler],
        vec![Block {
            blocker: chump,
            attacker: trampler,
        }],
        &db,
    );

    assert!(!alive(&after, chump), "the chump blocker died");
    assert_eq!(
        after.players[1].life,
        start_life - 3,
        "the excess 3 tramples over to the player (CR 702.19e)"
    );
}

#[test]
fn issue_154_full_block_absorbs_all_trample_damage_cr_702_19e() {
    // CR 702.19e: only the excess over lethal tramples. A 5/4 trampler fully
    // blocked by a 4/5 Basilisk assigns all 5 to the Basilisk (still 5 short of
    // absorbing? no — 5 ≥ 5 toughness) with none left over, so the player takes
    // nothing.
    let db = combat_db();
    let mut state = at_declare_attackers();
    let start_life = state.players[1].life;
    let trampler = place_permanent(
        &mut state,
        id_in(&db, "test_trampler"),
        PlayerId(0),
        false,
        0,
    ); // 5/4
    let wall = place_permanent(
        &mut state,
        id_in(&db, "test_basilisk"),
        PlayerId(1),
        false,
        0,
    ); // 4/5

    let after = run_combat(
        &state,
        vec![trampler],
        vec![Block {
            blocker: wall,
            attacker: trampler,
        }],
        &db,
    );

    assert_eq!(
        after.players[1].life, start_life,
        "a fully-absorbing blocker leaves no trample excess (CR 702.19e)"
    );
    assert!(!alive(&after, wall), "the 4/5 took lethal 5");
}

#[test]
fn issue_154_deathtouch_trampler_assigns_one_per_blocker_rest_to_player() {
    // CR 510.1e + 702.19e: a 4/4 trample+deathtouch attacker needs assign only 1
    // to a 4/5 blocker (deathtouch makes 1 lethal), tramping the other 3 over to
    // the player; the blocker is destroyed by CR 704.5h.
    let db = combat_db();
    let mut state = at_declare_attackers();
    let start_life = state.players[1].life;
    let baneclaw = place_permanent(
        &mut state,
        id_in(&db, "test_baneclaw"),
        PlayerId(0),
        false,
        0,
    ); // 4/4
    let blocker = place_permanent(
        &mut state,
        id_in(&db, "test_basilisk"),
        PlayerId(1),
        false,
        0,
    ); // 4/5

    let after = run_combat(
        &state,
        vec![baneclaw],
        vec![Block {
            blocker,
            attacker: baneclaw,
        }],
        &db,
    );

    assert!(
        !alive(&after, blocker),
        "1 deathtouch damage destroys the blocker (CR 704.5h)"
    );
    assert_eq!(
        after.players[1].life,
        start_life - 3,
        "assigns 1 to the blocker, tramples 3 to the player (CR 510.1e/702.19e)"
    );
}

#[test]
fn issue_154_lifelink_gains_life_in_the_same_event_as_the_damage_cr_702_15e() {
    // CR 702.15e: a lifelink source gains its controller life equal to the
    // damage, simultaneously. An unblocked 2/3 lifelinker hits player 1 for 2
    // and its controller (player 0) gains 2.
    let db = combat_db();
    let mut state = at_declare_attackers();
    let atk_life = state.players[0].life;
    let def_life = state.players[1].life;
    let cleric = place_permanent(
        &mut state,
        id_in(&db, "test_lifelinker"),
        PlayerId(0),
        false,
        0,
    );

    let after = run_combat(&state, vec![cleric], Vec::new(), &db);

    assert_eq!(
        after.players[0].life,
        atk_life + 2,
        "lifelink gains its controller 2 (CR 702.15e)"
    );
    assert_eq!(after.players[1].life, def_life - 2, "the defender took 2");
}

#[test]
fn issue_154_lifelink_on_blocking_damage_gains_life_cr_702_15e() {
    // CR 702.15e: lifelink applies to any damage the source deals, including a
    // blocker's damage to the attacker. A 2/3 lifelink blocker deals 2 to a 3/2
    // Boar and its controller gains 2, even as the blocker dies to the Boar.
    let db = combat_db();
    let mut state = at_declare_attackers();
    let boar = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(0), false, 0);
    let cleric = place_permanent(
        &mut state,
        id_in(&db, "test_lifelinker"),
        PlayerId(1),
        false,
        0,
    );
    let def_life = state.players[1].life;

    let after = run_combat(
        &state,
        vec![boar],
        vec![Block {
            blocker: cleric,
            attacker: boar,
        }],
        &db,
    );

    assert_eq!(
        after.players[1].life,
        def_life + 2,
        "the lifelink blocker's controller gains 2 from its combat damage"
    );
}

#[test]
fn issue_344_damage_is_computed_once_after_all_declarations_route_per_defender() {
    // After both defenders declare, passing the priority round advances to the
    // combat-damage step, where damage is computed once and routes per #341:
    // each attacker's block resolves against its own defender's blocker.
    let db = db();
    let (state, atk_a, _atk_b, blk1, _blk2) = split_combat_at_declare_blockers();

    // Seat 1 blocks attacker A; seat 2 declares no blockers (attacker B is
    // unblocked and will hit seat 2).
    let state = apply_action(
        &state,
        &Action::DeclareBlockers {
            blocks: vec![Block {
                blocker: blk1,
                attacker: atk_a,
            }],
        },
        &db,
    );
    let state = apply_action(&state, &Action::DeclareBlockers { blocks: Vec::new() }, &db);
    assert!(state.blockers_declared);

    // A full 3-seat priority round advances into combat damage.
    let mut state = state;
    for _ in 0..3 {
        state = apply_action(&state, &Action::PassPriority, &db);
    }
    // Attacker A (4/2) and its blocker (4/2) traded; seat 2 took 4 from the
    // unblocked attacker B.
    assert_eq!(state.players[2].life, 16, "unblocked attacker B hit seat 2");
    assert_eq!(
        state.players[1].life, 20,
        "seat 1 blocked its attacker, so took no damage"
    );
}
