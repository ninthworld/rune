//! Combat damage assigned by a characteristic other than power (CR 510.1a, modified),
//! driven through the real `apply_action` combat-damage step (issue #741).
//!
//! The override is read once, where the step asks how much a creature assigns, and every
//! test here is a consumer of that one number: an unblocked hit, trample's excess
//! (CR 702.19e), the marked damage the lethal-damage state-based action reads
//! (CR 704.5g), a split across two blockers (CR 510.1c), and a prevention shield
//! (CR 615.1) that sees the substituted amount exactly as it would have seen the power.
//!
//! Nothing here asserts a power that changed, because none does — and the first test
//! asserts that explicitly. A layer-7b `set power to toughness` would pass most of these
//! and be a different card.

use crate::apply::test_support::*;
use crate::characteristics::characteristics;
use crate::replacement::DamageFilter;

/// A two-player board at declare-attackers with seat 0 holding a Wall of Mist — a **0/5**
/// with defender, whose power and toughness are as far apart as the catalog goes, so no
/// assertion below could pass by reading the wrong one. `arcades` puts Arcades, the
/// Strategist beside it, granting both the damage override and the attack permission.
fn wall_and_arcades(arcades: bool) -> (GameState, CardDatabase, PermanentId) {
    let db = db();
    let mut state = at_declare_attackers();
    let wall = place_permanent(&mut state, fixture("wall_of_mist"), PlayerId(0), false, 0);
    if arcades {
        place_permanent(
            &mut state,
            fixture("arcades_the_strategist"),
            PlayerId(0),
            false,
            0,
        );
    }
    (state, db, wall)
}

#[test]
fn issue_741_an_unblocked_defender_deals_its_toughness_and_keeps_its_power() {
    // Both halves of the crux in one test. The Wall deals 5 — its toughness — to the
    // defending player, and its power is still 0 afterwards: the override is a fact about
    // one rule, not a P/T change, so the number every other reader sees is unmoved.
    let (state, db, wall) = wall_and_arcades(true);
    let after = run_combat(&state, vec![wall], vec![], &db);

    assert_eq!(
        after.players[1].life, 15,
        "a 0/5 under Arcades deals 5, not 0 (CR 510.1a as modified)"
    );
    let current = characteristics(&after, wall, &db);
    assert_eq!(
        current.power,
        Some(0),
        "the creature's power is unchanged — this is not a P/T effect"
    );
    assert_eq!(current.toughness, Some(5), "and neither is its toughness");
}

#[test]
fn issue_741_without_the_override_the_same_wall_would_deal_nothing() {
    // The control. Without Arcades the Wall cannot attack at all, so the override is
    // asserted against the only other creature that can carry it: a 0/5 blocking. It
    // assigns its power, which is none, and the attacker takes nothing.
    let db = db();
    let mut state = at_declare_attackers();
    let ogre = place_permanent(&mut state, fixture("onakke_ogre"), PlayerId(0), false, 0);
    let wall = place_permanent(&mut state, fixture("wall_of_mist"), PlayerId(1), false, 0);

    let after = run_combat(
        &state,
        vec![ogre],
        vec![Block {
            blocker: wall,
            attacker: ogre,
        }],
        &db,
    );
    assert_eq!(
        find_perm(&after, ogre).damage,
        0,
        "a 0/5 blocker with nothing modifying it assigns its power (CR 510.1a)"
    );
}

#[test]
fn issue_741_a_blocking_defender_also_assigns_its_toughness() {
    // The override is a fact about the creature, not about attacking: the same Wall
    // blocking assigns 5 to the attacker it blocks. Seat 1 holds the Wall and its own
    // Arcades, and seat 0 attacks into it with a 4/2 Ogre.
    let db = db();
    let mut state = at_declare_attackers();
    let ogre = place_permanent(&mut state, fixture("onakke_ogre"), PlayerId(0), false, 0);
    let wall = place_permanent(&mut state, fixture("wall_of_mist"), PlayerId(1), false, 0);
    place_permanent(
        &mut state,
        fixture("arcades_the_strategist"),
        PlayerId(1),
        false,
        0,
    );

    let after = run_combat(
        &state,
        vec![ogre],
        vec![Block {
            blocker: wall,
            attacker: ogre,
        }],
        &db,
    );
    assert!(
        !alive(&after, ogre),
        "5 assigned damage is lethal to a 4/2 (CR 704.5g)"
    );
}

#[test]
fn issue_741_trample_excess_is_computed_from_the_assigned_amount() {
    // CR 702.19e assigns the trampler's leftover to the defending player, and "leftover"
    // is what is left of the *assigned* amount after each blocker's lethal. The Wall
    // assigns 5, the 4/2 Ogre blocking it needs 2, and 3 tramples over — a calculation
    // that reads 0 and overflows nothing if it reads power instead.
    let (mut state, db, wall) = wall_and_arcades(true);
    let ogre = place_permanent(&mut state, fixture("onakke_ogre"), PlayerId(1), false, 0);
    grant_keyword(&mut state, wall, Keyword::Trample);

    let after = run_combat(
        &state,
        vec![wall],
        vec![Block {
            blocker: ogre,
            attacker: wall,
        }],
        &db,
    );
    assert!(!alive(&after, ogre), "the blocker took its lethal 2");
    assert_eq!(
        after.players[1].life, 17,
        "3 of the 5 assigned trampled over (CR 702.19e), not 0 of the power"
    );
}

#[test]
fn issue_741_the_lethal_damage_state_based_action_reads_the_assigned_amount() {
    // CR 704.5g destroys a creature whose marked damage is at least its toughness, and
    // what gets marked is what was assigned. A 2/4 Giant Spider blocking the Wall takes 5
    // and dies; the Wall takes the Spider's 2 and lives. Reading power would mark nothing
    // and the Spider would survive, which is the assertion pair below.
    let (mut state, db, wall) = wall_and_arcades(true);
    let spider = place_permanent(&mut state, fixture("giant_spider"), PlayerId(1), false, 0);

    let after = run_combat(
        &state,
        vec![wall],
        vec![Block {
            blocker: spider,
            attacker: wall,
        }],
        &db,
    );
    assert!(
        !alive(&after, spider),
        "5 assigned is lethal to a 2/4 (CR 704.5g)"
    );
    assert!(
        alive(&after, wall),
        "the Wall's own 5 toughness holds the 2"
    );
}

#[test]
fn issue_741_a_defender_without_the_override_leaves_its_blocker_alive() {
    // The other half of the test above, with Arcades absent. The Wall still needs a way
    // into combat, so it blocks instead of attacking: it assigns its power, the Spider
    // takes nothing, and both live.
    let db = db();
    let mut state = at_declare_attackers();
    let spider = place_permanent(&mut state, fixture("giant_spider"), PlayerId(0), false, 0);
    let wall = place_permanent(&mut state, fixture("wall_of_mist"), PlayerId(1), false, 0);

    let after = run_combat(
        &state,
        vec![spider],
        vec![Block {
            blocker: wall,
            attacker: spider,
        }],
        &db,
    );
    assert!(alive(&after, spider), "a 0-power blocker destroys nothing");
    assert_eq!(find_perm(&after, spider).damage, 0);
}

#[test]
fn issue_741_a_toughness_assigner_splits_across_two_blockers() {
    // CR 510.1c spreads the assignment across the blockers in the attacking player's
    // chosen order, each just-lethal before the next. Two 2/4 Spiders share the Wall's 5:
    // the first takes its lethal 4 and dies, the last takes the remaining 1 and lives with
    // it marked. Every number here comes off the substituted amount.
    let (mut state, db, wall) = wall_and_arcades(true);
    let first = place_permanent(&mut state, fixture("giant_spider"), PlayerId(1), false, 0);
    let second = place_permanent(&mut state, fixture("giant_spider"), PlayerId(1), false, 0);

    let after = run_combat_ordered(
        &state,
        vec![wall],
        vec![
            Block {
                blocker: first,
                attacker: wall,
            },
            Block {
                blocker: second,
                attacker: wall,
            },
        ],
        vec![DamageOrder {
            attacker: wall,
            blockers: vec![first, second],
        }],
        &db,
    );
    assert!(
        !alive(&after, first),
        "the first in the order took lethal 4"
    );
    assert!(alive(&after, second), "the last took only the remaining 1");
    assert_eq!(
        find_perm(&after, second).damage,
        1,
        "5 assigned, 4 spent on the first blocker, 1 left for the second"
    );
}

#[test]
fn issue_741_prevented_damage_from_a_toughness_assigner_is_still_prevented() {
    // CR 615.1 is consulted at the one seam damage is dealt, which is downstream of the
    // assignment — so a shield sees the substituted amount exactly as it would have seen
    // the power, and prevents all of it. The override changes how much is assigned, never
    // whether it can be prevented.
    let (mut state, db, wall) = wall_and_arcades(true);
    state.prevention.push(DamageFilter { combat_only: true });

    let after = run_combat(&state, vec![wall], vec![], &db);
    assert_eq!(
        after.players[1].life, 20,
        "all 5 of the assigned combat damage was prevented (CR 615.1)"
    );
}

#[test]
fn issue_741_counters_fold_into_the_substituted_characteristic() {
    // The substitute is read through the same computed characteristics the power would
    // have been, so CR 613 layer 7c applies to it: a `+1/+1` counter on the Wall makes it
    // a 1/6, and it assigns 6.
    let (mut state, db, wall) = wall_and_arcades(true);
    if let Some(perm) = state.battlefield.iter_mut().find(|p| p.id == wall) {
        perm.counters
            .insert(crate::state::CounterKind::PlusOnePlusOne, 1);
    }

    let after = run_combat(&state, vec![wall], vec![], &db);
    assert_eq!(
        after.players[1].life, 14,
        "the counter raised the toughness it assigns by, exactly as it would a power"
    );
}
