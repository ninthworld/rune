//! The projection of a permanent under a continuous effect that is **in no CR 613 layer**
//! (issue #741) — split from the main [`tests`](super::tests) module, which is already
//! past the size the coding standards allow.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;
use crate::test_support::fixture;
use crate::view::test_support::put_permanent;

/// Issue #741: a creature that assigns combat damage by its **toughness** projects with
/// the power it actually has.
///
/// The override is a rule modification, not a CR 613 layer, so nothing about the wire
/// changes: a Wall of Mist under Arcades is a 0/5 on the board, exactly as its card says,
/// and the client is never told a number the rules do not agree with. A layer-7b
/// power-setting effect — the wrong way to build this card — would project a 5/5 here.
///
/// The keyword line is the other half: the Wall still has `defender`, because an
/// as-though permission takes nothing away.
#[test]
fn issue_741_a_toughness_assigner_projects_its_unchanged_power() {
    let db = CardDatabase::bundled().unwrap();
    let mut state = GameState::new_two_player();
    let wall = put_permanent(
        &mut state,
        fixture("wall_of_mist"),
        PlayerId(0),
        false,
        false,
    );
    put_permanent(
        &mut state,
        fixture("arcades_the_strategist"),
        PlayerId(0),
        false,
        false,
    );

    let view = personalized_view(&state, &db, PlayerId(0));
    let card = &view
        .battlefield
        .iter()
        .find(|p| p.id == permanent_entity_id(wall))
        .expect("the Wall is on the projected battlefield")
        .card;

    assert_eq!(
        card.power.as_deref(),
        Some("0"),
        "the displayed power is the printed one — the override is not a P/T change"
    );
    assert_eq!(card.toughness.as_deref(), Some("5"));
    assert!(
        card.keywords.contains(&"defender".to_string()),
        "an as-though permission takes no keyword away"
    );
}
