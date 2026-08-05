//! Menace, as the blocker slot has to state it: a restriction only judgeable over the
//! assembled declaration must reach the player in the prompt.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::test_support::fixture;
use sage_engine::{Attack, PlayerId, Step};

/// The attackers slot states which of its candidates choosing would tap, so a client
/// can draw a declaration it is still assembling — and says the slot may be left
/// unanswered, which is the difference between "choose none of these" and a
/// submission the server must reject.
#[test]
fn the_attackers_slot_states_what_choosing_each_candidate_taps() {
    let db = CardDatabase::bundled().unwrap();
    let mut state = GameState::new_two_player();
    state.step = Step::DeclareAttackers;
    state.priority = PlayerId(0);
    // Sun Sentinel has vigilance; Onakke Ogre does not.
    let vigilant = crate::view::test_support::put_permanent(
        &mut state,
        fixture("sun_sentinel"),
        PlayerId(0),
        false,
        false,
    );
    let plain = crate::view::test_support::put_permanent(
        &mut state,
        fixture("onakke_ogre"),
        PlayerId(0),
        false,
        false,
    );

    let reqs = attacker_requirements(&state, &db);
    let attackers = reqs
        .iter()
        .find(|req| req.slot == "attackers")
        .expect("the declaration slot");
    assert!(
        attackers.optional,
        "declaring no attackers is a legal declaration (CR 508.1a)"
    );
    assert_eq!(
        attackers.taps,
        vec![permanent_entity_id(plain)],
        "only the creature attacking would tap is named (CR 508.1f / 702.20b)"
    );
    assert!(
        attackers
            .candidates
            .contains(&permanent_entity_id(vigilant)),
        "the vigilant creature is still a candidate — it just does not turn"
    );
}

/// The blocker slot of a menacing attacker says so, so a player is told the
/// two-or-more rule *before* submitting rather than by a declaration the engine
/// silently refuses (CR 702.110b).
#[test]
fn a_menacing_attackers_blocker_slot_names_its_restriction() {
    let db = CardDatabase::bundled().unwrap();
    let mut state = GameState::new_two_player();
    state.step = Step::DeclareAttackers;
    state.priority = PlayerId(0);
    let brute = crate::view::test_support::put_permanent(
        &mut state,
        fixture("boggart_brute"),
        PlayerId(0),
        false,
        false,
    );
    let plain = crate::view::test_support::put_permanent(
        &mut state,
        fixture("onakke_ogre"),
        PlayerId(0),
        false,
        false,
    );
    crate::view::test_support::put_permanent(
        &mut state,
        fixture("sun_sentinel"),
        PlayerId(1),
        false,
        false,
    );

    let state = sage_engine::apply_action(
        &state,
        &sage_engine::Action::DeclareAttackers {
            attackers: vec![
                Attack {
                    attacker: brute,
                    defender: AttackTarget::Player(PlayerId(1)),
                },
                Attack {
                    attacker: plain,
                    defender: AttackTarget::Player(PlayerId(1)),
                },
            ],
        },
        &db,
    );
    let mut state = state;
    while state.step != Step::DeclareBlockers {
        state = sage_engine::apply_action(&state, &sage_engine::Action::PassPriority, &db);
    }

    let prompts: Vec<String> = blocker_requirements(&state, &db)
        .into_iter()
        .map(|r| r.prompt)
        .collect();
    assert!(
        prompts
            .iter()
            .any(|p| p.contains("Boggart Brute") && p.contains("menace")),
        "the menacing attacker's slot states the restriction: {prompts:?}"
    );
    assert!(
        prompts
            .iter()
            .any(|p| p == "Choose blockers for Onakke Ogre"),
        "an ordinary attacker's slot is unchanged: {prompts:?}"
    );
}

/// The mirroring ceiling gets the same treatment as menace's floor: a restriction
/// the engine can only judge over the assembled selection is stated in words rather
/// than left to a submit that silently does nothing (CR 509.1b, issue #606).
#[test]
fn issue_606_a_block_count_ceiling_is_named_in_the_slot_prompt() {
    let db = CardDatabase::bundled().unwrap();
    let (state, _) = combat_with(&db, "bristling_boar", "sun_sentinel");
    let prompts: Vec<String> = blocker_requirements(&state, &db)
        .into_iter()
        .map(|r| r.prompt)
        .collect();
    assert!(
        prompts
            .iter()
            .any(|p| p.contains("Bristling Boar") && p.contains("no more than one blocker")),
        "the ceiling is stated before the player submits: {prompts:?}"
    );
}

/// A pairwise restriction needs no words: it is already visible as the slot's
/// candidate list, and an attacker nothing may block gets no slot at all — asking
/// a question with no answer is worse than not asking (issue #606).
#[test]
fn issue_606_pairwise_evasion_is_projected_as_candidates_not_as_prose() {
    let db = CardDatabase::bundled().unwrap();

    // Vine Mare can't be blocked by black creatures: the black candidate drops out
    // of its slot while the green one stays.
    let (state, defenders) =
        combat_with_blockers(&db, "vine_mare", &["walking_corpse", "centaur_courser"]);
    let slots = blocker_requirements(&state, &db);
    assert_eq!(slots.len(), 1, "one attacker, one slot");
    assert!(
        !slots[0]
            .candidates
            .contains(&permanent_entity_id(defenders[0])),
        "the black creature is not offered"
    );
    assert!(
        slots[0]
            .candidates
            .contains(&permanent_entity_id(defenders[1])),
        "the green one is"
    );
    assert_eq!(
        slots[0].prompt, "Choose blockers for Vine Mare",
        "a pairwise restriction adds no prose"
    );
}

/// A two-player combat parked at declare-blockers: `attacker` attacks alone and the
/// defender controls one creature of each named card. Returns the state and the
/// defender's permanents in the order they were named.
fn combat_with_blockers(
    db: &CardDatabase,
    attacker: &str,
    blockers: &[&str],
) -> (GameState, Vec<PermanentId>) {
    let mut state = GameState::new_two_player();
    state.step = Step::DeclareAttackers;
    state.priority = PlayerId(0);
    let atk = crate::view::test_support::put_permanent(
        &mut state,
        fixture(attacker),
        PlayerId(0),
        false,
        false,
    );
    let defenders: Vec<PermanentId> = blockers
        .iter()
        .map(|slug| {
            crate::view::test_support::put_permanent(
                &mut state,
                fixture(slug),
                PlayerId(1),
                false,
                false,
            )
        })
        .collect();
    let mut state = sage_engine::apply_action(
        &state,
        &sage_engine::Action::DeclareAttackers {
            attackers: vec![Attack {
                attacker: atk,
                defender: AttackTarget::Player(PlayerId(1)),
            }],
        },
        db,
    );
    while state.step != Step::DeclareBlockers {
        state = sage_engine::apply_action(&state, &sage_engine::Action::PassPriority, db);
    }
    (state, defenders)
}

/// [`combat_with_blockers`] for the common one-blocker case.
fn combat_with(db: &CardDatabase, attacker: &str, blocker: &str) -> (GameState, PermanentId) {
    let (state, defenders) = combat_with_blockers(db, attacker, &[blocker]);
    (state, defenders[0])
}
