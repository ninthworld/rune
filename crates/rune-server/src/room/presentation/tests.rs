//! Unit tests for the presentation trail itself (issue #594) — the parts that are
//! easier to state directly on the structure than through a live room: the
//! information-safety guard on the face cache, aggregation, per-seat filtering, and the
//! window bound. The end-to-end behaviour is in
//! [`crate::room::input::presentation_tests`].

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rune_engine::{Permanent, PermanentId, PlayerId};
use rune_protocol::{GameLogEntry, GameLogEvent};

use super::*;
use crate::room::test_support::db;
use crate::test_support::fixture;

/// A state with a stocked hand and library on both seats, one battlefield permanent,
/// and one card in a graveyard — every zone the observation walk could reach.
fn stocked_state() -> GameState {
    let mut state = GameState::new_two_player();
    for seat in 0..2 {
        let hand = vec![
            state.new_instance(fixture("forest")),
            state.new_instance(fixture("shock")),
        ];
        let library = vec![state.new_instance(fixture("onakke_ogre"))];
        state.players[seat].hand = hand;
        state.players[seat].library = library;
    }
    let buried = state.new_instance(fixture("walking_corpse"));
    state.players[0].graveyard.push(buried);

    let card = fixture("llanowar_elves");
    let instance = state.new_instance(card);
    let perm = PermanentId(state.mint_id());
    state.battlefield.push(Permanent {
        id: perm,
        instance: instance.id,
        card,
        controller: PlayerId(0),
        tapped: false,
        entered_turn: 0,
        attacking: None,
        blocking: None,
        damage: 0,
        counters: Default::default(),
        attached_to: None,
    });
    state
}

fn drawn(sequence: u64, count: u32) -> GameLogEntry {
    GameLogEntry {
        sequence,
        event: GameLogEvent::CardsDrawn {
            player: "p0".into(),
            count,
        },
    }
}

#[test]
fn issue_594_a_hand_or_library_face_is_never_retained() {
    // The information-safety guard, asserted where it is enforced. A moment crosses
    // seats — every opponent and every spectator read the same public moments — so a
    // privately-known face that reached the cache could not be redacted afterwards.
    // The walk therefore never touches a hand or a library at all.
    let state = stocked_state();
    let mut trail = PresentationTrail::default();
    trail.observe(&state, &db());

    let permanent = &state.battlefield[0];
    assert!(
        trail.faces.contains_key(&permanent_entity_id(permanent.id)),
        "a battlefield face is public and is retained"
    );
    assert!(
        trail
            .faces
            .contains_key(&card_entity_id(state.players[0].graveyard[0].id)),
        "so is a graveyard face"
    );

    for player in &state.players {
        for hidden in player.hand.iter().chain(player.library.iter()) {
            assert!(
                !trail.faces.contains_key(&card_entity_id(hidden.id)),
                "a hand or library face must never be retained: {}",
                card_entity_id(hidden.id)
            );
            assert!(
                !trail.origins.contains_key(&card_entity_id(hidden.id)),
                "and a hidden card's zone is not recorded either"
            );
        }
    }
}

#[test]
fn issue_594_the_batch_origin_is_where_an_object_was_when_the_batch_opened() {
    // CR 903.9a needs an origin the event does not state, and the only honest one is
    // where the room saw the object *before* the batch's actions ran. A later
    // observation inside the same batch must not overwrite it, or the recorded origin
    // would be the destination.
    let mut state = stocked_state();
    let mut trail = PresentationTrail::default();
    trail.observe(&state, &db());
    let commander = state.players[0].graveyard[0];
    assert_eq!(
        trail.origins.get(&card_entity_id(commander.id)),
        Some(&MomentZone::Graveyard)
    );

    // The card moves to the command zone and the room observes again, still inside the
    // same batch: the origin stands.
    state.players[0].graveyard.clear();
    state.players[0].command.push(commander);
    trail.observe(&state, &db());
    assert_eq!(
        trail.origins.get(&card_entity_id(commander.id)),
        Some(&MomentZone::Graveyard),
        "the origin is the batch's opening zone, not the newest observation"
    );
}

#[test]
fn issue_594_consecutive_identical_moments_aggregate_into_one_count() {
    // Repeated triggers and repeated damage cost one caption, not one dwell each — the
    // window is short and a run of identical beats would starve it.
    let state = GameState::new_two_player();
    let mut trail = PresentationTrail::default();
    let entries = vec![drawn(1, 1), drawn(2, 1), drawn(3, 1), drawn(4, 2)];
    trail.record(&state, &entries, &[Vec::new(), Vec::new()], &[]);

    let window = trail.for_seat(0);
    assert_eq!(
        window.len(),
        2,
        "three identical draws collapse: {window:?}"
    );
    assert_eq!(window[0].count, 3);
    assert_eq!(window[1].count, 1, "a differing draw is its own moment");
    assert!(
        window[0].id < window[1].id,
        "aggregation keeps the first id, so ids stay strictly increasing"
    );
}

#[test]
fn issue_594_a_seat_receives_only_its_own_skipped_path() {
    // The per-seat moment, and the sanctioned reason a receiver's ids have gaps: seat 0
    // never learns where seat 1 was passed, and a spectator learns neither.
    let state = GameState::new_two_player();
    let mut trail = PresentationTrail::default();
    let path = |turn| {
        vec![AutoPassedStep {
            phase: Phase::Upkeep,
            turn,
        }]
    };
    trail.record(
        &state,
        &[drawn(1, 1)],
        &[path(1), path(1)],
        &[
            AutoPassReason::NoResponseAvailable,
            AutoPassReason::ForcedDeclaration,
        ],
    );

    let skipped = |window: &[PresentationMoment]| {
        window
            .iter()
            .filter(|moment| matches!(moment.kind, MomentKind::PhasesSkipped { .. }))
            .count()
    };
    assert_eq!(skipped(&trail.for_seat(0)), 1);
    assert_eq!(skipped(&trail.for_seat(1)), 1);
    assert_eq!(skipped(&trail.public()), 0, "a spectator owns no seat");
    // Identical paths for two seats stay two moments — a per-seat moment must never be
    // absorbed into another seat's.
    let seat0 = trail.for_seat(0);
    let seat1 = trail.for_seat(1);
    let id = |window: &[PresentationMoment]| {
        window
            .iter()
            .find(|moment| matches!(moment.kind, MomentKind::PhasesSkipped { .. }))
            .map(|moment| moment.id)
            .unwrap()
    };
    assert_ne!(id(&seat0), id(&seat1));
    assert!(
        !seat0.iter().any(|moment| moment.id == id(&seat1)),
        "seat 0's stream has a gap where seat 1's moment sits"
    );
    // The reason is per seat and stated, never derived.
    let reason = |window: &[PresentationMoment]| match &window
        .iter()
        .find(|moment| matches!(moment.kind, MomentKind::PhasesSkipped { .. }))
        .unwrap()
        .kind
    {
        MomentKind::PhasesSkipped { reason, .. } => *reason,
        other => panic!("expected phases_skipped, got {other:?}"),
    };
    assert_eq!(reason(&seat0), AutoPassReason::NoResponseAvailable);
    assert_eq!(reason(&seat1), AutoPassReason::ForcedDeclaration);
}

#[test]
fn issue_594_a_long_settle_never_grows_the_carried_window() {
    // The bound is part of the contract, not an implementation detail: a settle that
    // applies dozens of actions must not make the most expensive message the protocol
    // sends arrive exactly when the receiver is least able to watch it.
    let state = GameState::new_two_player();
    let mut trail = PresentationTrail::default();
    let entries: Vec<GameLogEntry> = (1..=200u64)
        .map(|sequence| drawn(sequence, u32::try_from(sequence).unwrap()))
        .collect();
    trail.record(&state, &entries, &[Vec::new(), Vec::new()], &[]);

    let window = trail.for_seat(0);
    assert_eq!(window.len(), PRESENTATION_WINDOW);
    assert!(
        window.windows(2).all(|pair| pair[0].id < pair[1].id),
        "the window is the newest suffix, still in order"
    );
    assert!(
        trail.moments.len() <= PRESENTATION_WINDOW * 2,
        "and the room retains no history no view will ever carry"
    );
}

#[test]
fn issue_594_a_settle_that_did_nothing_records_nothing() {
    // A batch id names a real causal group. A settle that applied no action and passed
    // no seat produces no moments and burns no batch id, so "these things happened
    // together" never points at an empty group.
    let state = GameState::new_two_player();
    let mut trail = PresentationTrail::default();
    trail.record(&state, &[], &[Vec::new(), Vec::new()], &[]);
    assert!(trail.for_seat(0).is_empty());
    assert_eq!(trail.next_batch, 0);

    // And the same log projected twice yields the moments once (idempotence under the
    // repeated projections a coalescing broadcast implies).
    trail.record(&state, &[drawn(1, 1)], &[Vec::new(), Vec::new()], &[]);
    trail.record(&state, &[drawn(1, 1)], &[Vec::new(), Vec::new()], &[]);
    assert_eq!(trail.for_seat(0).len(), 1);
    assert_eq!(trail.next_batch, 1);
}
