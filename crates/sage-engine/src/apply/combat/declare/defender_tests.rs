//! Attacking **as though** the creature did not have defender (CR 609.4 over CR 702.3b),
//! driven through the real `apply_action` declaration (issue #741).
//!
//! Every test here is about the seam between a permission and a removal. The permission
//! has to reach exactly one rule — the attacker declaration — and reach nothing else, and
//! the card that proves it is Arcades: its own class is "each creature you control **with
//! defender**", so a permission implemented as keyword removal would delete the class it
//! was granted from the moment it applied.

use crate::apply::test_support::*;
use crate::apply_action;
use crate::card::Keyword;
use crate::characteristics::characteristics;

/// A two-player board at declare-attackers with seat 0 holding a Wall of Mist (a 0/5 with
/// defender) that entered long enough ago to be free of summoning sickness. `arcades`
/// puts Arcades, the Strategist beside it.
fn wall_at_declare_attackers(arcades: bool) -> (GameState, CardDatabase, PermanentId) {
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
fn issue_741_a_defender_attacks_under_the_permission() {
    // CR 702.3b says a creature with defender can't attack; CR 609.4 lets one rule apply
    // as though it did not have one. With Arcades out, the Wall is an attacker candidate
    // and the declaration is applied for real — tapped and attacking.
    let (state, db, wall) = wall_at_declare_attackers(true);
    assert!(
        attacker_candidates(&state, &db).contains(&wall),
        "the permission puts a defender into the candidate set"
    );

    let declared = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: atk1(&[wall]),
        },
        &db,
    );
    let wall_perm = find_perm(&declared, wall);
    assert_eq!(
        wall_perm.attacking,
        Some(crate::combat::AttackTarget::Player(PlayerId(1))),
        "the Wall really is attacking, not merely offered"
    );
    assert!(wall_perm.tapped, "attacking taps it (CR 508.1f)");
}

#[test]
fn issue_741_a_defender_without_the_permission_still_cannot_attack() {
    // The control, and the reason the test above proves anything: the same Wall with no
    // Arcades on the battlefield is not a candidate, and the declaration naming it is
    // rejected outright rather than quietly dropped.
    let (state, db, wall) = wall_at_declare_attackers(false);
    assert!(
        !attacker_candidates(&state, &db).contains(&wall),
        "defender keeps a creature out of the candidate set (CR 702.3b)"
    );

    let rejected = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: atk1(&[wall]),
        },
        &db,
    );
    assert_eq!(
        rejected, state,
        "declaring a creature with defender as an attacker is illegal, so nothing happens"
    );
}

#[test]
fn issue_741_the_permission_leaves_the_keyword_in_place() {
    // The crux. An as-though permission is not [`Modification::LoseKeyword`]: the Wall
    // still *has* defender while it is attacking under the permission, which is what
    // keeps it inside Arcades' own "each creature you control with defender" class. A
    // permission built out of keyword removal would take the creature out of the class
    // that granted it, and the very next read would put defender back — or not grant the
    // damage override at all.
    let (state, db, wall) = wall_at_declare_attackers(true);
    let declared = apply_action(
        &state,
        &Action::DeclareAttackers {
            attackers: atk1(&[wall]),
        },
        &db,
    );

    assert!(
        characteristics(&declared, wall, &db)
            .keywords
            .contains(&Keyword::Defender),
        "the creature attacking as though it had no defender still has defender"
    );
    // Arcades' other clause reaches it through that same keyword filter, so the class
    // demonstrably still matches while the permission is being used.
    assert_eq!(
        crate::characteristics::assigns_combat_damage_by(&declared, wall, &db),
        crate::card::DamageCharacteristic::Toughness,
        "the defender-filtered class still finds the attacker it just let out"
    );
}

#[test]
fn issue_741_the_permission_ends_with_its_source() {
    // Derived on every read, never stored (ADR 0005 §1): Arcades leaving takes the
    // permission with it, with nothing to prune.
    let (mut state, db, wall) = wall_at_declare_attackers(true);
    assert!(attacker_candidates(&state, &db).contains(&wall));
    state
        .battlefield
        .retain(|perm| perm.printed.card() != Some(fixture("arcades_the_strategist")));
    assert!(
        !attacker_candidates(&state, &db).contains(&wall),
        "the permission is gone the instant its source is"
    );
}

#[test]
fn issue_741_novice_knight_attacks_only_while_it_is_attached() {
    // Novice Knight carries the permission itself, conditioned on being enchanted or
    // equipped — one `as long as …` re-asked on every read. Bare, it is a plain creature
    // with defender.
    let db = db();
    let mut state = at_declare_attackers();
    let knight = place_permanent(&mut state, fixture("novice_knight"), PlayerId(0), false, 0);
    assert!(
        !attacker_candidates(&state, &db).contains(&knight),
        "an unattached Novice Knight is just a creature with defender"
    );

    // Oakenform is an enchant-creature Aura; attaching it satisfies the condition.
    let aura = place_permanent(&mut state, fixture("oakenform"), PlayerId(0), false, 0);
    if let Some(perm) = state.battlefield.iter_mut().find(|p| p.id == aura) {
        perm.attached_to = Some(knight);
    }
    assert!(
        attacker_candidates(&state, &db).contains(&knight),
        "enchanted, it may attack as though it had no defender"
    );

    // And the condition is re-asked, not remembered: moving the Aura off takes the
    // permission with it.
    if let Some(perm) = state.battlefield.iter_mut().find(|p| p.id == aura) {
        perm.attached_to = None;
    }
    assert!(
        !attacker_candidates(&state, &db).contains(&knight),
        "the `as long as` clause ends the permission with nothing to prune"
    );
}

#[test]
fn issue_741_the_permission_lifts_defender_and_nothing_else() {
    // An as-though clause modifies the rule it names. A tapped creature still cannot be
    // declared (CR 508.1a), and neither can a summoning-sick one (CR 302.6) — the
    // permission says nothing about either.
    let db = db();
    let mut state = at_declare_attackers();
    place_permanent(
        &mut state,
        fixture("arcades_the_strategist"),
        PlayerId(0),
        false,
        0,
    );
    let tapped = place_permanent(&mut state, fixture("wall_of_mist"), PlayerId(0), true, 0);
    let sick = place_permanent(&mut state, fixture("wall_of_mist"), PlayerId(0), false, 0);
    let turn = state.turn;
    set_entered_turn(&mut state, sick, turn);

    let candidates = attacker_candidates(&state, &db);
    assert!(
        !candidates.contains(&tapped),
        "a tapped creature can't attack"
    );
    assert!(
        !candidates.contains(&sick),
        "a summoning-sick creature can't attack (CR 302.6)"
    );
}
