//! Round-trip and compatibility tests for the lobby message set.
//!
//! Split out of `lobby.rs` for size (issue #711). Pure code motion — every test is
//! unchanged and moves with the code it exercises.

#![allow(clippy::unwrap_used, clippy::panic)] // panics are the failure signal in tests

use crate::*;

#[test]
fn lobby_command_hello_omits_absent_token() {
    // First contact carries no token; the minimal `{type}` wire shape must be
    // preserved so an older/fresh client stays compatible.
    let msg = LobbyCommand::Hello(Hello { token: None });
    let json = serde_json::to_value(&msg).unwrap();
    assert_eq!(json, serde_json::json!({ "type": "hello" }));
    let back: LobbyCommand = serde_json::from_value(json).unwrap();
    assert_eq!(back, msg);
}

#[test]
fn lobby_command_hello_round_trips_with_token() {
    // A reconnect echoes the previously issued session token verbatim.
    let msg = LobbyCommand::Hello(Hello {
        token: Some("s:ab12".into()),
    });
    let json = serde_json::to_value(&msg).unwrap();
    assert_eq!(
        json,
        serde_json::json!({ "type": "hello", "token": "s:ab12" })
    );
    let back: LobbyCommand = serde_json::from_value(json).unwrap();
    assert_eq!(back, msg);
}

#[test]
fn lobby_command_create_room_carries_config() {
    let msg = LobbyCommand::CreateRoom(CreateRoom {
        config: RoomConfig {
            seats: 4,
            game_setup: "standard_2p".into(),
            ..Default::default()
        },
    });
    let json = serde_json::to_value(&msg).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "type": "create_room",
            "config": { "seats": 4, "game_setup": "standard_2p" }
        })
    );
    let back: LobbyCommand = serde_json::from_value(json).unwrap();
    assert_eq!(back, msg);
}

#[test]
fn issue_546_room_config_carries_a_name_and_visibility_and_elides_the_defaults() {
    // A named private table round-trips with both new fields on the wire.
    let named = RoomConfig {
        seats: 4,
        game_setup: "commander".into(),
        name: Some("Casual Commander".into()),
        visibility: RoomVisibility::Private,
    };
    let json = serde_json::to_value(&named).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "seats": 4,
            "game_setup": "commander",
            "name": "Casual Commander",
            "visibility": "private"
        })
    );
    assert_eq!(serde_json::from_value::<RoomConfig>(json).unwrap(), named);

    // An unnamed public table is byte-for-byte the pre-#546 shape: both fields
    // elide, so an older server/client sees exactly what it always saw.
    let plain = RoomConfig {
        seats: 2,
        game_setup: "standard_2p".into(),
        ..Default::default()
    };
    let json = serde_json::to_value(&plain).unwrap();
    assert_eq!(
        json,
        serde_json::json!({ "seats": 2, "game_setup": "standard_2p" })
    );

    // And a pre-#546 payload deserializes to that same default: unnamed, public.
    let legacy: RoomConfig =
        serde_json::from_str(r#"{"seats":2,"game_setup":"standard_2p"}"#).unwrap();
    assert_eq!(legacy, plain);
    assert_eq!(legacy.name, None);
    assert_eq!(legacy.visibility, RoomVisibility::Public);
}

#[test]
fn issue_546_lobby_command_update_room_round_trips() {
    // The host's edit carries a whole config, exactly like `create_room`, so the
    // same client surface can serve both.
    let msg = LobbyCommand::UpdateRoom(UpdateRoom {
        config: RoomConfig {
            seats: 4,
            game_setup: "commander".into(),
            name: Some("Casual Commander".into()),
            visibility: RoomVisibility::Private,
        },
    });
    let json = serde_json::to_value(&msg).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "type": "update_room",
            "config": {
                "seats": 4,
                "game_setup": "commander",
                "name": "Casual Commander",
                "visibility": "private"
            }
        })
    );
    let back: LobbyCommand = serde_json::from_value(json).unwrap();
    assert_eq!(back, msg);
}

#[test]
fn issue_546_room_summary_carries_the_table_name_through_its_config() {
    // The directory renders a table's name from the config it already carried —
    // there is no second, divergent name field to keep in sync.
    let listed = RoomSummary {
        room_id: "r0".into(),
        config: RoomConfig {
            seats: 4,
            game_setup: "commander".into(),
            name: Some("Casual Commander".into()),
            ..Default::default()
        },
        filled: 2,
        spectators: 0,
        state: RoomState::Gathering,
    };
    let json = serde_json::to_value(&listed).unwrap();
    assert_eq!(
        json["config"]["name"],
        serde_json::json!("Casual Commander")
    );
    // A listed room is public by definition, so `visibility` stays elided.
    assert!(json["config"].get("visibility").is_none());
    assert_eq!(serde_json::from_value::<RoomSummary>(json).unwrap(), listed);
}

#[test]
fn lobby_command_join_room_round_trips() {
    let msg = LobbyCommand::JoinRoom(JoinRoom {
        room_id: "r:7f3".into(),
    });
    let json = serde_json::to_value(&msg).unwrap();
    assert_eq!(
        json,
        serde_json::json!({ "type": "join_room", "room_id": "r:7f3" })
    );
    let back: LobbyCommand = serde_json::from_value(json).unwrap();
    assert_eq!(back, msg);
}

#[test]
fn lobby_command_submit_deck_round_trips_and_elides_empty() {
    // A populated decklist round-trips as a flat list of identities. With no
    // commander the `commander` field elides, so the frame is the pre-commander
    // shape (issue #372, additive).
    let msg = LobbyCommand::SubmitDeck(SubmitDeck {
        cards: vec!["ci_bear".into(), "ci_bear".into(), "ci_forest".into()],
        commander: None,
    });
    let json = serde_json::to_value(&msg).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "type": "submit_deck",
            "cards": ["ci_bear", "ci_bear", "ci_forest"]
        })
    );
    let back: LobbyCommand = serde_json::from_value(json).unwrap();
    assert_eq!(back, msg);

    // An empty decklist with no commander elides both fields entirely.
    let empty = LobbyCommand::SubmitDeck(SubmitDeck {
        cards: vec![],
        commander: None,
    });
    let json = serde_json::to_value(&empty).unwrap();
    assert_eq!(json, serde_json::json!({ "type": "submit_deck" }));
}

#[test]
fn issue_372_submit_deck_carries_the_designated_commander() {
    // The commander designation rides the submit-deck frame as a bare
    // `functional_id` (CR 903.3), present only when designated.
    let msg = LobbyCommand::SubmitDeck(SubmitDeck {
        cards: vec!["ci_lathliss".into(), "ci_forest".into()],
        commander: Some("ci_lathliss".into()),
    });
    let json = serde_json::to_value(&msg).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "type": "submit_deck",
            "cards": ["ci_lathliss", "ci_forest"],
            "commander": "ci_lathliss"
        })
    );
    let back: LobbyCommand = serde_json::from_value(json).unwrap();
    assert_eq!(back, msg);
}

#[test]
fn issue_415_add_ai_command_round_trips_and_elides_empty_deck() {
    // A populated AI seating carries the seat, kind, deck, and (commander format)
    // designated commander.
    let msg = LobbyCommand::AddAi(AddAi {
        seat: 2,
        kind: "random".into(),
        cards: vec!["ci_bear".into(), "ci_forest".into()],
        commander: Some("ci_lathliss".into()),
    });
    let json = serde_json::to_value(&msg).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "type": "add_ai",
            "seat": 2,
            "kind": "random",
            "cards": ["ci_bear", "ci_forest"],
            "commander": "ci_lathliss"
        })
    );
    let back: LobbyCommand = serde_json::from_value(json).unwrap();
    assert_eq!(back, msg);

    // An empty deck with no commander elides both fields (like `submit_deck`).
    let bare = LobbyCommand::AddAi(AddAi {
        seat: 0,
        kind: "random".into(),
        cards: vec![],
        commander: None,
    });
    let json = serde_json::to_value(&bare).unwrap();
    assert_eq!(
        json,
        serde_json::json!({ "type": "add_ai", "seat": 0, "kind": "random" })
    );
}

#[test]
fn issue_415_remove_ai_command_round_trips() {
    let msg = LobbyCommand::RemoveAi(RemoveAi { seat: 3 });
    let json = serde_json::to_value(&msg).unwrap();
    assert_eq!(json, serde_json::json!({ "type": "remove_ai", "seat": 3 }));
    let back: LobbyCommand = serde_json::from_value(json).unwrap();
    assert_eq!(back, msg);
}

#[test]
fn issue_415_seat_view_ai_round_trips_and_elides_when_absent() {
    // An AI seat reports its kind, no occupant, and decked+ready by construction.
    let ai = SeatView {
        seat: 1,
        occupied_by: None,
        name: Some("Random".into()),
        decked: true,
        ready: true,
        ai: Some("random".into()),
    };
    let json = serde_json::to_value(&ai).unwrap();
    assert_eq!(json.get("ai"), Some(&serde_json::json!("random")));
    assert_eq!(json.get("occupied_by"), None);
    assert_eq!(serde_json::from_value::<SeatView>(json).unwrap(), ai);

    // A human/empty seat omits `ai` entirely.
    let human = SeatView {
        ai: None,
        ..ai.clone()
    };
    let json = serde_json::to_value(&human).unwrap();
    assert!(json.get("ai").is_none());
}

#[test]
fn lobby_command_ready_and_leave_round_trip() {
    let ready = LobbyCommand::Ready(Ready { ready: true });
    let json = serde_json::to_value(&ready).unwrap();
    assert_eq!(json, serde_json::json!({ "type": "ready", "ready": true }));
    assert_eq!(serde_json::from_value::<LobbyCommand>(json).unwrap(), ready);

    let leave = LobbyCommand::Leave;
    let json = serde_json::to_value(&leave).unwrap();
    assert_eq!(json, serde_json::json!({ "type": "leave" }));
    assert_eq!(serde_json::from_value::<LobbyCommand>(json).unwrap(), leave);
}

#[test]
fn issue_351_lobby_command_spectate_room_round_trips() {
    let msg = LobbyCommand::SpectateRoom(SpectateRoom {
        room_id: "r:7f3".into(),
    });
    let json = serde_json::to_value(&msg).unwrap();
    assert_eq!(
        json,
        serde_json::json!({ "type": "spectate_room", "room_id": "r:7f3" })
    );
    let back: LobbyCommand = serde_json::from_value(json).unwrap();
    assert_eq!(back, msg);
}

#[test]
fn issue_351_room_summary_carries_a_spectator_count_and_elides_zero() {
    // An in-progress room with spectators advertises the count.
    let watched = RoomSummary {
        room_id: "r:1".into(),
        config: RoomConfig {
            seats: 4,
            game_setup: "standard_ffa".into(),
            ..Default::default()
        },
        filled: 4,
        spectators: 3,
        state: RoomState::InProgress,
    };
    let json = serde_json::to_value(&watched).unwrap();
    assert_eq!(json.get("spectators"), Some(&serde_json::json!(3)));
    assert_eq!(json.get("state"), Some(&serde_json::json!("in_progress")));
    assert_eq!(
        serde_json::from_value::<RoomSummary>(json).unwrap(),
        watched
    );

    // Zero spectators elide from the wire; an older payload without the field
    // deserializes to zero.
    let unwatched = RoomSummary {
        spectators: 0,
        ..watched.clone()
    };
    let json = serde_json::to_value(&unwatched).unwrap();
    assert!(json.get("spectators").is_none());
    let legacy: RoomSummary = serde_json::from_str(
        r#"{"room_id":"r:1","config":{"seats":4,"game_setup":"standard_ffa"},"filled":4,"state":"in_progress"}"#,
    )
    .unwrap();
    assert_eq!(legacy.spectators, 0);
}

#[test]
fn lobby_view_round_trips_populated() {
    let view = LobbyView {
        session: "s:ab12".into(),
        you: "p1".into(),
        name: Some("Alice".into()),
        room: Some(RoomView {
            room_id: "r:7f3".into(),
            config: RoomConfig {
                seats: 2,
                game_setup: "standard_2p".into(),
                ..Default::default()
            },
            seats: vec![
                SeatView {
                    seat: 0,
                    occupied_by: Some("p1".into()),
                    name: Some("Alice".into()),
                    decked: true,
                    ready: true,
                    ai: None,
                },
                SeatView {
                    seat: 1,
                    occupied_by: Some("p2".into()),
                    name: None,
                    decked: true,
                    ready: false,
                    ai: None,
                },
            ],
        }),
        directory: vec![],
        valid_commands: vec!["submit_deck".into(), "unready".into(), "leave".into()],
    };
    let json = serde_json::to_string(&view).unwrap();
    let back: LobbyView = serde_json::from_str(&json).unwrap();
    assert_eq!(back, view);
}

#[test]
fn lobby_view_elides_empties_and_redacts_seat_flags() {
    // A connection with an identity but not yet in a room: `room` is absent
    // and a still-empty seat's `decked`/`ready`/`occupied_by` all elide.
    let view = LobbyView {
        session: "s:new".into(),
        you: "p9".into(),
        name: None,
        room: None,
        directory: vec![],
        valid_commands: vec!["create_room".into(), "join_room".into()],
    };
    let json = serde_json::to_value(&view).unwrap();
    assert!(json.get("room").is_none());
    // An empty directory elides from the wire, like every other empty collection.
    assert!(json.get("directory").is_none());
    // `session` and `you` are always present on the wire (like `GameView::you`).
    assert_eq!(json.get("session"), Some(&serde_json::json!("s:new")));
    assert_eq!(json.get("you"), Some(&serde_json::json!("p9")));

    // An empty seat serializes to just its index.
    let empty_seat = SeatView {
        seat: 3,
        occupied_by: None,
        name: None,
        decked: false,
        ready: false,
        ai: None,
    };
    let seat_json = serde_json::to_value(&empty_seat).unwrap();
    assert_eq!(seat_json, serde_json::json!({ "seat": 3 }));
    let back: LobbyView = serde_json::from_value(json).unwrap();
    assert_eq!(back, view);
}

#[test]
fn lobby_view_ignores_unknown_fields() {
    // Forward-compat invariant: a newer server may add lobby fields; older
    // clients must still deserialize the message.
    let json = r#"{ "session": "s:1", "you": "p1", "some_future_field": true }"#;
    let view: LobbyView = serde_json::from_str(json).unwrap();
    assert_eq!(view.session, "s:1");
    assert_eq!(view.you, "p1");
    assert!(view.room.is_none());
}

#[test]
fn lobby_command_ignores_unknown_fields() {
    // A command from a newer client with extra fields still deserializes.
    let json = r#"{ "type": "join_room", "room_id": "r:1", "future": 7 }"#;
    let cmd: LobbyCommand = serde_json::from_str(json).unwrap();
    assert_eq!(
        cmd,
        LobbyCommand::JoinRoom(JoinRoom {
            room_id: "r:1".into()
        })
    );
}

#[test]
fn lobby_view_defaults_identity_when_absent() {
    // A payload that omits `session`/`you` still deserializes, defaulting both
    // to `""` rather than failing the whole message.
    let json = r#"{ "valid_commands": ["hello"] }"#;
    let view: LobbyView = serde_json::from_str(json).unwrap();
    assert_eq!(view.session, "");
    assert_eq!(view.you, "");
    assert!(view.directory.is_empty());
    assert_eq!(view.valid_commands, vec!["hello".to_string()]);
}

#[test]
fn room_summary_round_trips_and_tags_its_state() {
    // Issue #280: a directory entry carries the room id, its config summary, the
    // occupancy count, and the lifecycle state tagged snake_case on the wire.
    let gathering = RoomSummary {
        room_id: "r0".into(),
        config: RoomConfig {
            seats: 2,
            game_setup: "standard_2p".into(),
            ..Default::default()
        },
        filled: 1,
        spectators: 0,
        state: RoomState::Gathering,
    };
    let json = serde_json::to_value(&gathering).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "room_id": "r0",
            "config": { "seats": 2, "game_setup": "standard_2p" },
            "filled": 1,
            "state": "gathering"
        })
    );
    assert_eq!(
        serde_json::from_value::<RoomSummary>(json).unwrap(),
        gathering
    );

    // The started state tags as `in_progress`.
    let in_progress = RoomSummary {
        state: RoomState::InProgress,
        filled: 2,
        ..gathering.clone()
    };
    let json = serde_json::to_value(&in_progress).unwrap();
    assert_eq!(json["state"], serde_json::json!("in_progress"));
    assert_eq!(
        serde_json::from_value::<RoomSummary>(json).unwrap(),
        in_progress
    );
}

#[test]
fn lobby_view_directory_round_trips_and_elides_when_empty() {
    // Issue #280: the room directory rides on `LobbyView`, round-trips populated,
    // and elides from the wire when there are no rooms.
    let mut view = LobbyView {
        session: "s:ab12".into(),
        you: "p1".into(),
        name: None,
        room: None,
        directory: vec![],
        valid_commands: vec!["create_room".into(), "join_room".into()],
    };
    // Empty directory: the field elides entirely.
    assert!(serde_json::to_value(&view)
        .unwrap()
        .get("directory")
        .is_none());

    // Populated: a gathering room and an in-progress room both survive the trip.
    view.directory = vec![
        RoomSummary {
            room_id: "r0".into(),
            config: RoomConfig {
                seats: 2,
                game_setup: "standard_2p".into(),
                ..Default::default()
            },
            filled: 1,
            spectators: 0,
            state: RoomState::Gathering,
        },
        RoomSummary {
            room_id: "r1".into(),
            config: RoomConfig {
                seats: 4,
                game_setup: "ffa-4".into(),
                ..Default::default()
            },
            filled: 4,
            spectators: 2,
            state: RoomState::InProgress,
        },
    ];
    let back: LobbyView = serde_json::from_str(&serde_json::to_string(&view).unwrap()).unwrap();
    assert_eq!(back, view);
    assert_eq!(back.directory[0].state, RoomState::Gathering);
    assert_eq!(back.directory[1].state, RoomState::InProgress);
}

#[test]
fn set_name_command_round_trips() {
    // Issue #294: the display-name command is a tagged lobby command carrying the
    // requested name verbatim; the server validates it before storing.
    let msg = LobbyCommand::SetName(SetName {
        name: "Alice".into(),
    });
    let json = serde_json::to_value(&msg).unwrap();
    assert_eq!(
        json,
        serde_json::json!({ "type": "set_name", "name": "Alice" })
    );
    let back: LobbyCommand = serde_json::from_value(json).unwrap();
    assert_eq!(back, msg);
}

#[test]
fn seat_view_name_round_trips_and_elides_when_absent() {
    // Issue #294: a named occupant's display name rides in the roster and
    // round-trips; an unnamed (or empty) seat omits it entirely.
    let named = SeatView {
        seat: 0,
        occupied_by: Some("p1".into()),
        name: Some("Alice".into()),
        decked: true,
        ready: false,
        ai: None,
    };
    let json = serde_json::to_value(&named).unwrap();
    assert_eq!(json.get("name"), Some(&serde_json::json!("Alice")));
    assert_eq!(serde_json::from_value::<SeatView>(json).unwrap(), named);

    let unnamed = SeatView {
        name: None,
        ..named.clone()
    };
    let json = serde_json::to_value(&unnamed).unwrap();
    assert!(json.get("name").is_none());
}

#[test]
fn issue_395_lobby_error_frame_round_trips_with_a_named_card() {
    // A copy-limit rejection names the offending card by its identity and carries a
    // stable code plus the human-readable reason.
    let frame = LobbyErrorFrame {
        lobby_error: LobbyRejection {
            code: "copy_limit".into(),
            reason: "Onakke Ogre appears 5 times, above the 4-copy limit".into(),
            card: Some("onakke_ogre".into()),
        },
    };
    let json = serde_json::to_value(&frame).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "lobby_error": {
                "code": "copy_limit",
                "reason": "Onakke Ogre appears 5 times, above the 4-copy limit",
                "card": "onakke_ogre"
            }
        })
    );
    // The `lobby_error` key is the on-wire discriminator; no other frame has it.
    assert!(json.get("lobby_error").is_some());
    let back: LobbyErrorFrame = serde_json::from_value(json).unwrap();
    assert_eq!(back, frame);
}

#[test]
fn issue_395_lobby_error_frame_elides_card_when_not_card_specific() {
    // A size rejection names no card, so `card` elides from the wire entirely.
    let frame = LobbyErrorFrame {
        lobby_error: LobbyRejection {
            code: "below_minimum".into(),
            reason: "deck has 39 cards, below the 40-card minimum".into(),
            card: None,
        },
    };
    let json = serde_json::to_value(&frame).unwrap();
    assert!(json["lobby_error"].get("card").is_none());
    let back: LobbyErrorFrame = serde_json::from_value(json).unwrap();
    assert_eq!(back, frame);
    assert_eq!(back.lobby_error.card, None);
}

#[test]
fn lobby_view_name_round_trips_and_elides_when_absent() {
    // Issue #294: the connection's own display name rides on the lobby view (so the
    // pre-game UI can show it before a seat exists) and elides when unset.
    let mut view = LobbyView {
        session: "s:ab12".into(),
        you: "p1".into(),
        name: Some("Alice".into()),
        room: None,
        directory: vec![],
        valid_commands: vec!["set_name".into(), "create_room".into()],
    };
    let json = serde_json::to_value(&view).unwrap();
    assert_eq!(json.get("name"), Some(&serde_json::json!("Alice")));
    assert_eq!(serde_json::from_value::<LobbyView>(json).unwrap(), view);

    view.name = None;
    let json = serde_json::to_value(&view).unwrap();
    assert!(json.get("name").is_none());
}

#[test]
fn canonical_lobby_fixture_round_trips_and_matches_typed_fields() {
    // Cross-language contract fixture: this exact JSON is also parsed by the web client's
    // `protocol.test.ts`, which additionally asserts nothing in it is dropped. A field
    // renamed, retyped, or removed here fails on this side; a field added here and not
    // mirrored fails on that side.
    let json = include_str!("../../fixtures/lobbyview.json");
    let view: LobbyView = serde_json::from_str(json).unwrap();

    let reencoded = serde_json::to_string(&view).unwrap();
    let back: LobbyView = serde_json::from_str(&reencoded).unwrap();
    assert_eq!(back, view);

    assert_eq!(view.session, "s_7f3a9c21");
    assert_eq!(view.you, "p0");
    assert_eq!(view.name.as_deref(), Some("Ari"));

    let room = view.room.as_ref().unwrap();
    assert_eq!(room.room_id, "r_204");
    assert_eq!(room.config.seats, 2);
    assert_eq!(room.config.game_setup, "starter-1v1");
    // Non-default visibility must survive: the field is elided only when public.
    assert_eq!(room.config.visibility, RoomVisibility::Private);
    assert_eq!(room.seats.len(), 2);
    assert_eq!(room.seats[0].occupied_by.as_deref(), Some("p0"));
    assert!(room.seats[0].ready);
    assert_eq!(room.seats[1].ai.as_deref(), Some("random"));
    // An AI seat is decked but never "ready" — readiness is a human signal.
    assert!(!room.seats[1].ready);

    assert_eq!(view.directory.len(), 2);
    assert_eq!(view.directory[0].state, RoomState::Gathering);
    // Elided when zero, so the gathering room parses back to no spectators.
    assert_eq!(view.directory[0].spectators, 0);
    assert_eq!(view.directory[1].state, RoomState::InProgress);
    assert_eq!(view.directory[1].spectators, 5);
    // A public room elides `visibility`; it must read back as the default.
    assert_eq!(view.directory[0].config.visibility, RoomVisibility::Public);

    assert!(view.valid_commands.iter().any(|c| c == "submit_deck"));
}

#[test]
fn canonical_roomless_lobby_fixture_round_trips_and_matches_typed_fields() {
    // The other half of the lobby: a connection that is *not* in a room, which is the only
    // state where the directory is what the screen is made of. Paired with the web client's
    // `protocol.test.ts` exactly like every other fixture.
    let json = include_str!("../../fixtures/lobbyview-open.json");
    let view: LobbyView = serde_json::from_str(json).unwrap();

    let reencoded = serde_json::to_string(&view).unwrap();
    let back: LobbyView = serde_json::from_str(&reencoded).unwrap();
    assert_eq!(back, view);

    assert_eq!(view.session, "s_1c04be77");
    assert_eq!(view.you, "p4");
    // Roomless: no room rides the view, and neither does a name it never set.
    assert!(view.room.is_none());
    assert_eq!(view.name, None);

    // Three browsable rooms: one with a seat free, one full but still gathering, and one
    // already running with spectators — the three cases a directory row has to tell apart.
    assert_eq!(view.directory.len(), 3);
    assert_eq!(view.directory[0].filled, 1);
    assert_eq!(view.directory[0].config.seats, 2);
    assert_eq!(view.directory[0].state, RoomState::Gathering);
    assert_eq!(view.directory[1].filled, view.directory[1].config.seats);
    assert_eq!(view.directory[1].state, RoomState::Gathering);
    // An unnamed table carries no name at all; the fallback label is the client's concern.
    assert_eq!(view.directory[1].config.name, None);
    assert_eq!(view.directory[2].state, RoomState::InProgress);
    assert_eq!(view.directory[2].spectators, 3);
    // Every listed room is public by construction, so none of them carries `visibility`.
    assert!(view
        .directory
        .iter()
        .all(|room| room.config.visibility == RoomVisibility::Public));

    assert_eq!(
        view.valid_commands,
        vec![
            "set_name".to_string(),
            "create_room".to_string(),
            "join_room".to_string(),
            "spectate_room".to_string(),
        ]
    );
}
