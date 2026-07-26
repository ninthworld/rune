//! Wire-shape, round-trip, and compatibility tests for the presentation-moment
//! contract (issue #594).

#![allow(clippy::unwrap_used, clippy::panic)] // panics are the failure signal in tests

use crate::*;

/// A minimal retained object: a name and an id, no face.
fn object(id: &str, name: &str) -> MomentObject {
    MomentObject {
        id: id.into(),
        name: name.into(),
        card: None,
    }
}

/// A moment at turn 4, precombat main, with the given kind and default aggregation.
fn moment(id: u64, kind: MomentKind) -> PresentationMoment {
    PresentationMoment {
        id,
        batch: 7,
        turn: 4,
        phase: Phase::PrecombatMain,
        kind,
        cause: None,
        count: 1,
    }
}

#[test]
fn issue_594_moment_carries_its_identity_and_position_and_elides_its_defaults() {
    // The wire shape of one moment: identity (`id`, `batch`), the position it happened
    // at (`turn`, `phase` — where the game *was*, not where it is), and the tagged kind
    // nested under `kind`, exactly as `damage_dealt` nests its tagged target.
    let cast = moment(
        12,
        MomentKind::Cast {
            player: "p0".into(),
            object: object("card_3", "Quickfire Bolt"),
        },
    );
    assert_eq!(
        serde_json::to_value(&cast).unwrap(),
        serde_json::json!({
            "id": 12,
            "batch": 7,
            "turn": 4,
            "phase": "precombat_main",
            "kind": {
                "kind": "cast",
                "player": "p0",
                "object": { "id": "card_3", "name": "Quickfire Bolt" },
            },
        })
    );

    // The two additive fields elide at their documented defaults: an unaggregated moment
    // carries no `count`, and a root moment carries no `cause`. A retained object with no
    // public face carries no `card`.
    let json = serde_json::to_value(&cast).unwrap();
    assert!(json.get("count").is_none());
    assert!(json.get("cause").is_none());
    assert!(json["kind"]["object"].get("card").is_none());
}

#[test]
fn issue_594_aggregation_count_and_cause_round_trip_when_present() {
    // Aggregation and causation are stated, never inferred: six identical triggers are
    // one moment with `count: 6`, and the graveyard move names the death that produced
    // it rather than leaving a client to read causation off adjacency.
    let died = moment(
        20,
        MomentKind::Died {
            object: object("perm_bear", "Grizzly Bears"),
        },
    );
    let mut moved = moment(
        21,
        MomentKind::ZoneMove {
            object: object("perm_bear", "Grizzly Bears"),
            from: MomentZone::Battlefield,
            to: MomentZone::Graveyard,
        },
    );
    moved.cause = Some(died.id);
    moved.count = 6;

    let json = serde_json::to_value(&moved).unwrap();
    assert_eq!(json["cause"], 20);
    assert_eq!(json["count"], 6);
    assert_eq!(json["kind"]["from"], "battlefield");
    assert_eq!(json["kind"]["to"], "graveyard");
    let back: PresentationMoment = serde_json::from_value(json).unwrap();
    assert_eq!(back, moved);
}

#[test]
fn issue_594_an_older_payload_reads_count_as_one_and_cause_as_absent() {
    // A payload from a server that predates aggregation — or any ordinary moment, since
    // both fields ride the wire only when they say something — deserializes to the
    // documented defaults: one occurrence, no stated cause, no retained face.
    let legacy: PresentationMoment = serde_json::from_str(
        r#"{"id":3,"batch":1,"turn":2,"phase":"draw",
            "kind":{"kind":"drew","player":"p0","count":1}}"#,
    )
    .unwrap();
    assert_eq!(legacy.count, 1);
    assert_eq!(legacy.cause, None);
    // ...and re-encoding it does not invent the fields back onto the wire.
    let json = serde_json::to_value(&legacy).unwrap();
    assert!(json.get("count").is_none());
    assert!(json.get("cause").is_none());

    // An unknown kind is a *parse* failure for the strongly typed Rust consumer, which is
    // why the client normalizer drops entries it cannot read rather than guessing: the
    // classifying tag has no safe default.
    let unknown: Result<PresentationMoment, _> = serde_json::from_str(
        r#"{"id":4,"batch":1,"turn":2,"phase":"draw","kind":{"kind":"counter_changed"}}"#,
    );
    assert!(unknown.is_err());
}

#[test]
fn issue_594_every_moment_kind_tags_its_snake_case_name_and_round_trips() {
    // The whole vocabulary is a contract: each kind serializes under its snake_case
    // `kind` tag and survives a JSON round trip inside a moment.
    let kinds = [
        (
            "cast",
            MomentKind::Cast {
                player: "p0".into(),
                object: object("card_3", "Quickfire Bolt"),
            },
        ),
        (
            "resolved",
            MomentKind::Resolved {
                player: "p0".into(),
                object: object("card_3", "Quickfire Bolt"),
            },
        ),
        (
            "countered",
            MomentKind::Countered {
                player: "p1".into(),
                object: object("card_9", "Runic Negation"),
            },
        ),
        (
            "fizzled",
            MomentKind::Fizzled {
                player: "p0".into(),
                object: object("card_3", "Quickfire Bolt"),
            },
        ),
        (
            "zone_move",
            MomentKind::ZoneMove {
                object: object("card_3", "Quickfire Bolt"),
                from: MomentZone::Stack,
                to: MomentZone::Graveyard,
            },
        ),
        (
            "died",
            MomentKind::Died {
                object: object("perm_bear", "Grizzly Bears"),
            },
        ),
        (
            "damage",
            MomentKind::Damage {
                target: LogDamageTarget::Player {
                    player: "p1".into(),
                },
                amount: 3,
            },
        ),
        (
            "life",
            MomentKind::Life {
                player: "p0".into(),
                amount: -2,
            },
        ),
        (
            "attacked",
            MomentKind::Attacked {
                player: "p0".into(),
                attackers: vec![object("perm_bear", "Grizzly Bears")],
            },
        ),
        (
            "blocked",
            MomentKind::Blocked {
                player: "p1".into(),
                blocks: vec![LogBlock {
                    blocker: LogEntity {
                        id: "perm_wall".into(),
                        name: "Wall of Roots".into(),
                    },
                    attacker: LogEntity {
                        id: "perm_bear".into(),
                        name: "Grizzly Bears".into(),
                    },
                }],
            },
        ),
        (
            "drew",
            MomentKind::Drew {
                player: "p0".into(),
                count: 2,
            },
        ),
        (
            "turn_change",
            MomentKind::TurnChange {
                turn: 5,
                active_player: "p1".into(),
            },
        ),
        (
            "phase_change",
            MomentKind::PhaseChange {
                phase: Phase::DeclareAttackers,
            },
        ),
        (
            "phases_skipped",
            MomentKind::PhasesSkipped {
                steps: vec![AutoPassedStep {
                    phase: Phase::Upkeep,
                    turn: 4,
                }],
                reason: AutoPassReason::NoResponseAvailable,
            },
        ),
        (
            "eliminated",
            MomentKind::Eliminated {
                player: "p2".into(),
                reason: GameOverReason::LifeZero,
            },
        ),
        (
            "game_over",
            MomentKind::GameOver {
                result: GameResult {
                    winner: Some("p0".into()),
                    losers: vec!["p1".into()],
                    reason: GameOverReason::Concede,
                },
            },
        ),
    ];

    for (tag, kind) in kinds {
        let entry = moment(1, kind);
        let json = serde_json::to_value(&entry).unwrap();
        assert_eq!(json["kind"]["kind"], tag, "kind `{tag}` must tag itself");
        let back: PresentationMoment = serde_json::from_value(json).unwrap();
        assert_eq!(back, entry, "kind `{tag}` must round-trip");
    }
}

#[test]
fn issue_594_retained_object_carries_a_public_face_and_survives_the_object() {
    // The retained snapshot is what makes a moment renderable *after* the object is
    // gone: the name is fixed at record time and the public face travels with it, so a
    // client never looks the dead permanent up on a board it has already left.
    let mut died = moment(
        30,
        MomentKind::Died {
            object: MomentObject {
                id: "perm_bear".into(),
                name: "Grizzly Bears".into(),
                card: Some(CardView {
                    id: "perm_bear".into(),
                    name: "Grizzly Bears".into(),
                    type_line: "Creature — Bear".into(),
                    mana_cost: Some("{1}{G}".into()),
                    rules_text: String::new(),
                    functional_id: "grizzly_bears".into(),
                    power: Some("2".into()),
                    toughness: Some("2".into()),
                    keywords: vec![],
                }),
            },
        },
    );
    died.phase = Phase::CombatDamage;

    let json = serde_json::to_value(&died).unwrap();
    assert_eq!(
        json["kind"]["object"]["card"]["functional_id"],
        "grizzly_bears"
    );
    assert_eq!(json["kind"]["object"]["name"], "Grizzly Bears");
    let back: PresentationMoment = serde_json::from_value(json).unwrap();
    assert_eq!(back, died);

    // A face-less object is the ordinary case, not an error, and its absence says
    // nothing about secrecy: a hand or library face is never retained at all, so `None`
    // means "no public face was known" and a client renders the name and stops.
    let drawn = moment(
        31,
        MomentKind::ZoneMove {
            object: object("card_77", "Forest"),
            from: MomentZone::Library,
            to: MomentZone::Hand,
        },
    );
    let json = serde_json::to_value(&drawn).unwrap();
    assert!(json["kind"]["object"].get("card").is_none());
    let back: PresentationMoment = serde_json::from_value(json).unwrap();
    assert_eq!(back, drawn);
}

#[test]
fn issue_594_zones_and_auto_pass_reasons_are_closed_snake_case_vocabularies() {
    // Both endpoints of a zone move are named — "graveyard → battlefield" is a
    // reanimation and "hand → battlefield" is a land drop, and neither is recoverable
    // from a board diff that shows only the arrival.
    for (zone, wire) in [
        (MomentZone::Battlefield, "battlefield"),
        (MomentZone::Graveyard, "graveyard"),
        (MomentZone::Exile, "exile"),
        (MomentZone::Hand, "hand"),
        (MomentZone::Library, "library"),
        (MomentZone::Stack, "stack"),
        (MomentZone::Command, "command"),
    ] {
        assert_eq!(serde_json::to_value(zone).unwrap(), wire);
        let back: MomentZone = serde_json::from_value(serde_json::json!(wire)).unwrap();
        assert_eq!(back, zone);
    }

    for (reason, wire) in [
        (AutoPassReason::NoResponseAvailable, "no_response_available"),
        (AutoPassReason::ForcedDeclaration, "forced_declaration"),
    ] {
        assert_eq!(serde_json::to_value(reason).unwrap(), wire);
        let back: AutoPassReason = serde_json::from_value(serde_json::json!(wire)).unwrap();
        assert_eq!(back, reason);
    }
}

#[test]
fn issue_594_phases_skipped_is_one_moment_holding_the_whole_ordered_path() {
    // The per-seat moment groups the settle's *entire* path into a single entry — never
    // one per priority window — and each entry states its own turn, because a repeated
    // step means an extra combat phase (CR 506.1) or an extra cleanup (CR 514.3a) at
    // least as often as it means a new turn.
    let skipped = moment(
        40,
        MomentKind::PhasesSkipped {
            steps: vec![
                AutoPassedStep {
                    phase: Phase::End,
                    turn: 3,
                },
                AutoPassedStep {
                    phase: Phase::Upkeep,
                    turn: 4,
                },
                AutoPassedStep {
                    phase: Phase::Draw,
                    turn: 4,
                },
            ],
            reason: AutoPassReason::NoResponseAvailable,
        },
    );
    let json = serde_json::to_value(&skipped).unwrap();
    assert_eq!(json["kind"]["kind"], "phases_skipped");
    assert_eq!(json["kind"]["reason"], "no_response_available");
    assert_eq!(
        json["kind"]["steps"],
        serde_json::json!([
            { "phase": "end", "turn": 3 },
            { "phase": "upkeep", "turn": 4 },
            { "phase": "draw", "turn": 4 },
        ])
    );
    let back: PresentationMoment = serde_json::from_value(json).unwrap();
    assert_eq!(back, skipped);

    // The forced-declaration reason is the issue #453 case: a declaration with no legal
    // non-empty answer, submitted rather than prompted. It is a different sentence to a
    // player, which is the whole reason there are two reasons.
    let forced = moment(
        41,
        MomentKind::PhasesSkipped {
            steps: vec![AutoPassedStep {
                phase: Phase::DeclareBlockers,
                turn: 4,
            }],
            reason: AutoPassReason::ForcedDeclaration,
        },
    );
    assert_eq!(
        serde_json::to_value(&forced).unwrap()["kind"]["reason"],
        "forced_declaration"
    );
}

#[test]
fn issue_594_the_carried_window_is_bounded_by_a_stated_constant() {
    // The bound is contract, not server trivia: a client sizes its backlog against it,
    // and a receiver behind by more than this has already missed moments and must catch
    // up rather than ask for the rest — there is no backfill request in this protocol.
    assert_eq!(PRESENTATION_WINDOW, 32);
}
