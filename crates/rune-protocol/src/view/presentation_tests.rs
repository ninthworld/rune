//! Tests for [`GameView::presentation`](crate::GameView::presentation), the
//! display-only presentation-moment window (issue #594).
//!
//! Split out of `view/tests.rs` rather than added to it: that module was already at the
//! file-size ceiling in `docs/coding-standards.md`, and the #594 tests — including the
//! canonical cross-language fixture — form their own cohesive seam.

#![allow(clippy::unwrap_used, clippy::panic)] // panics are the failure signal in tests

use crate::*;

#[test]
fn issue_594_presentation_elides_when_empty_and_round_trips_when_present() {
    // The additive window: absent from an ordinary frame, carried in order when a settle
    // produced anything worth pacing.
    let mut view = GameView {
        you: "p0".into(),
        phase: Phase::PrecombatMain,
        turn: 4,
        active_player: "p0".into(),
        ..Default::default()
    };
    let json = serde_json::to_value(&view).unwrap();
    assert!(json.get("presentation").is_none());

    view.presentation = vec![
        PresentationMoment {
            id: 101,
            batch: 9,
            turn: 4,
            phase: Phase::PrecombatMain,
            kind: MomentKind::Cast {
                player: "p0".into(),
                object: MomentObject {
                    id: "card_3".into(),
                    name: "Quickfire Bolt".into(),
                    card: None,
                },
            },
            cause: None,
            count: 1,
        },
        PresentationMoment {
            id: 102,
            batch: 9,
            turn: 4,
            phase: Phase::PrecombatMain,
            kind: MomentKind::Resolved {
                player: "p0".into(),
                object: MomentObject {
                    id: "card_3".into(),
                    name: "Quickfire Bolt".into(),
                    card: None,
                },
            },
            cause: Some(101),
            count: 1,
        },
    ];
    let json = serde_json::to_value(&view).unwrap();
    assert_eq!(json["presentation"][0]["id"], 101);
    assert_eq!(json["presentation"][0]["kind"]["kind"], "cast");
    assert_eq!(json["presentation"][1]["cause"], 101);
    let back: GameView = serde_json::from_value(json).unwrap();
    assert_eq!(back, view);
}

#[test]
fn issue_594_an_older_payload_carries_no_presentation_window() {
    // A view from a server that predates the field deserializes to the documented
    // default — an empty window, which is exactly "nothing to pace", not "unknown".
    let legacy: GameView = serde_json::from_str(r#"{"you":"p0","phase":"upkeep"}"#).unwrap();
    assert!(legacy.presentation.is_empty());

    // A consumer that ignores the field entirely (the CLI, the AI harness) sees an
    // identical game: nothing about the board, the legal set, or the result is carried
    // here, so dropping the window loses only captions.
    let paced: GameView = serde_json::from_str(
        r#"{"you":"p0","phase":"upkeep","presentation":[
             {"id":5,"batch":1,"turn":1,"phase":"upkeep",
              "kind":{"kind":"phase_change","phase":"upkeep"}}]}"#,
    )
    .unwrap();
    assert_eq!(paced.presentation.len(), 1);
    assert_eq!(paced.presentation[0].count, 1);
    let stripped = GameView {
        presentation: Vec::new(),
        ..paced.clone()
    };
    assert_eq!(stripped.valid_actions, paced.valid_actions);
    assert_eq!(stripped.battlefield, paced.battlefield);
    assert_eq!(stripped.result, paced.result);
}

#[test]
fn issue_594_the_carried_window_may_start_late_and_may_have_gaps() {
    // Two properties a client must tolerate rather than repair. The window is bounded,
    // so it can start well after id one; and per-seat moments (`phases_skipped`, which
    // names where *this* receiver was passed) are filtered out of every other seat's
    // stream, so ids skip. Neither is a lost message: there is no backfill request, and
    // a client de-duplicates by id without ever waiting for a missing one.
    let view: GameView = serde_json::from_str(
        r#"{"you":"p0","phase":"draw","turn":6,"presentation":[
             {"id":880,"batch":40,"turn":6,"phase":"upkeep",
              "kind":{"kind":"phase_change","phase":"upkeep"}},
             {"id":884,"batch":40,"turn":6,"phase":"draw",
              "kind":{"kind":"drew","player":"p1","count":1}}]}"#,
    )
    .unwrap();
    assert_eq!(view.presentation[0].id, 880);
    assert_eq!(view.presentation[1].id, 884);
    // The order carried is the order to render; a client never sorts or renumbers.
    assert!(view.presentation[0].id < view.presentation[1].id);
    // Both moments belong to one causal group — one applied action and the settle after
    // it — which is what lets a client tell "these happened because of that" from "these
    // are separate turns of the crank".
    assert_eq!(view.presentation[0].batch, view.presentation[1].batch);
}

#[test]
fn issue_594_moment_positions_are_where_the_game_was_not_where_it_is() {
    // The load-bearing distinction for any current-position UI: a cross-turn settle
    // broadcasts one view whose `turn`/`phase` have already moved past the moments it
    // carries. A client labels each moment from the moment and reads the current
    // position from the view — never the other way round.
    let view: GameView = serde_json::from_str(
        r#"{"you":"p0","phase":"precombat_main","turn":7,"active_player":"p0",
            "presentation":[
             {"id":900,"batch":50,"turn":6,"phase":"end",
              "kind":{"kind":"phase_change","phase":"end"}},
             {"id":901,"batch":50,"turn":7,"phase":"untap",
              "kind":{"kind":"turn_change","turn":7,"active_player":"p0"}}]}"#,
    )
    .unwrap();
    assert_eq!(view.turn, 7);
    assert_eq!(view.phase, Phase::PrecombatMain);
    assert_eq!(view.presentation[0].turn, 6);
    assert_eq!(view.presentation[0].phase, Phase::End);
    assert_ne!(view.presentation[1].phase, view.phase);
}

#[test]
fn issue_594_moments_fixture_round_trips_and_matches_typed_fields() {
    // Single-sourced cross-language contract fixture (the pattern issue #56 set): a
    // realistic removal sequence on a mid-game frame — cast, resolved, died, the zone
    // move the death produced, and the grouped per-seat skipped phases at the end of the
    // batch. A field renamed, retyped, or removed in this crate without updating the
    // fixture fails to deserialize (or mismatches an assertion) here.
    let json = include_str!("../../fixtures/gameview-moments.json");
    let view: GameView = serde_json::from_str(json).unwrap();

    let reencoded = serde_json::to_string(&view).unwrap();
    let back: GameView = serde_json::from_str(&reencoded).unwrap();
    assert_eq!(back, view);

    // The window is ordered, single-batch, and starts well after id one — a bounded
    // window that began mid-game is the ordinary case, not a defect.
    let moments = &view.presentation;
    assert_eq!(moments.len(), 6);
    assert!(moments.windows(2).all(|pair| pair[0].id < pair[1].id));
    assert_eq!(moments[0].id, 412);
    assert!(moments.iter().all(|m| m.batch == 57));

    // Cast → resolved, with the resolution naming the cast it followed from.
    assert!(
        matches!(&moments[0].kind, MomentKind::Cast { player, object }
        if player == "p1" && object.name == "Quickfire Bolt")
    );
    assert_eq!(moments[1].cause, Some(412));

    // The death retains the creature's public face, so the caption is renderable on a
    // view where the permanent is already gone from `battlefield`.
    let MomentKind::Died { object } = &moments[2].kind else {
        panic!("the third moment is the death");
    };
    assert_eq!(object.card.as_ref().unwrap().functional_id, "grizzly_bears");
    assert!(!view
        .battlefield
        .iter()
        .any(|permanent| permanent.id == object.id));

    // ...and the zone move that followed it names the death as its cause and states both
    // endpoints, which no board diff could supply.
    assert_eq!(moments[3].cause, Some(414));
    assert!(matches!(
        &moments[3].kind,
        MomentKind::ZoneMove {
            from: MomentZone::Battlefield,
            to: MomentZone::Graveyard,
            ..
        }
    ));

    // Repeated identical damage collapses into one moment with an occurrence count; the
    // count is a tally, never an amount (the amount rides the kind).
    let MomentKind::Damage { target, amount } = &moments[4].kind else {
        panic!("the fifth moment is the aggregated damage");
    };
    assert_eq!(*amount, 1);
    assert_eq!(moments[4].count, 2);
    assert!(matches!(target, LogDamageTarget::Player { player } if player == "p1"));

    // The per-seat skipped-phases moment is last in the batch, holds the whole ordered
    // path in one entry, and states its reason.
    let MomentKind::PhasesSkipped { steps, reason } = &moments[5].kind else {
        panic!("the last moment is the grouped skipped phases");
    };
    assert_eq!(steps.len(), 3);
    assert_eq!(steps[0].turn, 8);
    assert_eq!(steps[2].phase, Phase::BeginCombat);
    assert_eq!(*reason, AutoPassReason::NoResponseAvailable);
    // It is the moment form of the same path `auto_passed_steps` carries on this frame.
    assert!(view.auto_passed);
    assert_eq!(view.auto_passed_steps.len(), 3);
}
