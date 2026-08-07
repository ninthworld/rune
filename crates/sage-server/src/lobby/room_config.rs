//! The two commands that own a room's [`RoomConfig`]: `create_room`, and the host-only
//! `update_room` that changes a room's configuration after it exists (issue #546).
//!
//! They are one module because they enforce **one** set of rules. A config is legal or
//! it is not, and it must be judged the same way whether it arrives on a `create_room`
//! or on an `update_room` — a table you could not have created is a table you must not
//! be able to edit into. [`Lobby::validate_config`] is that single judgment;
//! `update_room` adds only the constraints that exist because the room is already real
//! (host-only, pre-game, and no shrinking a seat out from under an occupant).
//!
//! `create_room` moved here from [`commands`](super::commands) unchanged apart from
//! delegating its validation — pure code motion, keeping that module under the
//! file-size ceiling while the config rules gained a second caller.

use super::*;

impl Lobby {
    /// Validate and normalize a [`RoomConfig`] arriving on `create_room` or
    /// `update_room` — the one place a config is judged, so both commands accept
    /// exactly the same tables.
    ///
    /// Checks, in order: the seat count is inside [`SEAT_RANGE`]; the `game_setup` id
    /// names a registered format, so no room ever holds a setup the
    /// server cannot build a game from or validate decks against; the seat count is one
    /// that format itself allows (issue #349); and the optional table name (issue #546)
    /// is valid under the same bounds a display name gets. Returns the config with its
    /// name trimmed and a blank name normalized to `None`, so a room never stores
    /// whitespace as a title.
    fn validate_config(&self, mut config: RoomConfig) -> Result<RoomConfig, LobbyError> {
        if !SEAT_RANGE.contains(&config.seats) {
            return Err(LobbyError::InvalidSeatCount(config.seats));
        }
        let Some(format) = self.inner.formats.get(&config.game_setup) else {
            return Err(LobbyError::UnknownFormat(config.game_setup.clone()));
        };
        // The seat count must also be one the chosen format allows (issue #349): a
        // two-player format refuses a free-for-all count, and a free-for-all refuses a
        // duel. Non-fatal — the current lobby view is re-sent, like every other
        // rejected command.
        if !format.seats.contains(&config.seats) {
            return Err(LobbyError::SeatCountForFormat {
                seats: config.seats,
                format: config.game_setup.clone(),
            });
        }
        config.name =
            validate_room_name(config.name.as_deref()).map_err(LobbyError::InvalidRoomName)?;
        Ok(config)
    }

    /// Handle `create_room`: validate the config, reap empty rooms, enforce the room
    /// cap, then open a room and seat the creator at seat 0.
    pub(crate) fn create_room(
        &self,
        registry: &mut Registry,
        token: &SessionToken,
        config: RoomConfig,
    ) -> Result<(), LobbyError> {
        if registry
            .sessions
            .get(token)
            .is_some_and(|s| s.room.is_some())
        {
            return Err(LobbyError::AlreadyInRoom);
        }
        let config = self.validate_config(config)?;
        // Free capacity held by empty rooms before checking the cap, so a creator is
        // never refused for a slot no live room still needs.
        reap_empty(registry);
        if registry.rooms.len() >= self.inner.max_rooms {
            return Err(LobbyError::AtCapacity);
        }

        let n = registry.next_room;
        registry.next_room += 1;
        let room_id = format!("r{n}");
        let seat_count = config.seats as usize;
        let mut seats = vec![None; seat_count];
        seats[0] = Some(token.clone());
        registry.rooms.insert(
            room_id.clone(),
            RoomEntry {
                config,
                seats,
                ai_seats: vec![None; seat_count],
                gate: vec![SeatGate::default(); seat_count],
                game: None,
                spectators: Vec::new(),
            },
        );
        if let Some(session) = registry.sessions.get_mut(token) {
            session.room = Some(room_id.clone());
            session.seat = Some(0);
        }
        // A new room appeared in the directory: re-project it to everyone browsing.
        broadcast_views(registry);
        info!(%token, %room_id, "opened room");
        Ok(())
    }

    /// Handle `update_room` (issue #546): the **host** changes its table's
    /// configuration — name, format, seat count, or visibility — after the room exists.
    ///
    /// The rules, all authoritative and all non-fatal on rejection (the sender's
    /// current [`LobbyView`] is re-sent unchanged):
    ///
    /// - **Host only.** The sender must occupy seat 0 of the room, exactly like
    ///   [`add_ai`](Lobby::add_ai) ([`LobbyError::NotHost`]). The client renders Edit
    ///   Table from the advertised `update_room` command, never from its own idea of
    ///   who the host is.
    /// - **Pre-game only** ([`LobbyError::GameStarted`]): a started room's seats speak
    ///   `GameView`s, and its config is what the running game was built from.
    /// - **The same config rules a create gets** ([`Lobby::validate_config`]).
    /// - **Shrinking never evicts.** A seat count that would drop an occupied seat —
    ///   human or AI, at *any* index, not merely a smaller total — is rejected with
    ///   [`LobbyError::SeatsBelowOccupancy`] carrying the smallest count that keeps
    ///   everyone seated. Growing appends empty, joinable seats.
    /// - **Readiness follows the change.** Changing the seat count, the undo rule
    ///   (issue #648), or the format clears
    ///   every seat's ready flag: nobody stays ready to a table they did not agree to.
    ///   Changing the **format** additionally clears every submitted deck, because each
    ///   was validated against a format that no longer applies — the alternative is a
    ///   seat sitting decked with a deck the room would now refuse. A name- or
    ///   visibility-only edit disturbs nothing.
    ///
    /// Because an accepted update can only ever *clear* gate state, it never completes
    /// the ready gate and so never constructs a game.
    pub(crate) fn update_room(
        &self,
        registry: &mut Registry,
        token: &SessionToken,
        config: RoomConfig,
    ) -> Result<(), LobbyError> {
        let (room_id, host_seat) = seat_of(registry, token)?;
        if host_seat != 0 {
            return Err(LobbyError::NotHost);
        }
        // Validate against the same rules a create gets, before touching the room, so a
        // rejected edit leaves the table exactly as it was.
        let config = self.validate_config(config)?;
        let room = registry
            .rooms
            .get_mut(&room_id)
            .ok_or(LobbyError::NotSeated)?;
        if room.game.is_some() {
            return Err(LobbyError::GameStarted);
        }

        // Shrinking is refused, never clamped: report the smallest count that keeps
        // every current occupant (one past the highest occupied seat index).
        let seats = config.seats as usize;
        let occupied_through = room
            .seats
            .iter()
            .zip(&room.ai_seats)
            .rposition(|(session, ai)| session.is_some() || ai.is_some())
            .map_or(0, |index| index + 1);
        if seats < occupied_through {
            return Err(LobbyError::SeatsBelowOccupancy {
                seats: config.seats,
                needed: u8::try_from(occupied_through).unwrap_or(u8::MAX),
            });
        }

        let seats_changed = room.config.seats != config.seats;
        let format_changed = room.config.game_setup != config.game_setup;
        // Undo is a rule about how the game plays (issue #648), so turning it on or off
        // is the same kind of change a seat count is: nobody stays ready to a table that
        // now takes moves back — or that no longer will.
        let undo_changed = room.config.undo_enabled != config.undo_enabled;
        room.config = config;
        // Resize the seat-parallel vectors. Only trailing *empty* seats can be dropped
        // (the occupancy check above guarantees it), and new seats arrive empty.
        room.seats.resize(seats, None);
        room.ai_seats.resize_with(seats, || None);
        room.gate.resize_with(seats, SeatGate::default);
        if format_changed {
            // Every stored deck was validated against the old format; none of them is
            // known-legal here. Clear the gate wholesale so the room cannot start on a
            // deck it would now refuse.
            for gate in &mut room.gate {
                *gate = SeatGate::default();
            }
            // An AI seat's deck was cleared with it, so the seat can no longer stand:
            // empty it and let the host re-seat an AI with a deck this format accepts.
            for ai in &mut room.ai_seats {
                *ai = None;
            }
        } else if seats_changed || undo_changed {
            for gate in &mut room.gate {
                gate.ready = false;
            }
            // An AI seat is ready by construction and keeps its (still-legal) deck, so
            // restore its flag rather than leaving it un-readyable — it has no
            // connection to press Ready.
            for (gate, ai) in room.gate.iter_mut().zip(&room.ai_seats) {
                if ai.is_some() {
                    gate.ready = true;
                }
            }
        }

        // The table's name, format, size, or listing changed: re-project to its
        // occupants and to everyone browsing the directory.
        broadcast_views(registry);
        info!(%token, %room_id, "host updated the room configuration");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::lobby::test_support::*;

    /// An `update_room` carrying the given config.
    fn update(config: RoomConfig) -> LobbyCommand {
        LobbyCommand::UpdateRoom(UpdateRoom { config })
    }

    /// A named config, for the table-name tests.
    fn named(seats: u8, game_setup: &str, name: &str) -> RoomConfig {
        RoomConfig {
            name: Some(name.to_string()),
            ..config_with(seats, game_setup)
        }
    }

    #[tokio::test]
    async fn room_config_supports_two_through_eight_seats() {
        let lobby = lobby(8);
        for seats in SEAT_RANGE {
            let mut client = Client::connect(&lobby).await;
            let _ = client.view().await;
            lobby
                .command(
                    &client.token,
                    LobbyCommand::CreateRoom(CreateRoom {
                        // The full plumbing range is reached through the permissive
                        // multiplayer catch-all: since #707 the duel formats seat two,
                        // so the format that seats 2–8 is the one named for it.
                        config: config_with(seats, "standard_multiplayer"),
                    }),
                )
                .await
                .unwrap_or_else(|_| panic!("{seats} seats is in range"));
            let room = client.view().await.room.expect("room created");
            assert_eq!(room.seats.len(), usize::from(seats));
        }
    }

    #[tokio::test]
    async fn create_room_rejects_seat_counts_outside_the_range() {
        let lobby = lobby(4);
        for seats in [0u8, 1, 9, 255] {
            let mut client = Client::connect(&lobby).await;
            let _ = client.view().await;
            let err = lobby
                .command(
                    &client.token,
                    LobbyCommand::CreateRoom(CreateRoom {
                        config: config(seats),
                    }),
                )
                .await
                .expect_err("out-of-range seat count is rejected");
            assert_eq!(err, LobbyError::InvalidSeatCount(seats));
            // Rejection re-sends the current view: still roomless.
            assert!(client.current().room.is_none());
        }
    }

    #[tokio::test]
    async fn create_room_with_an_unknown_game_setup_is_rejected() {
        // The `game_setup` id must key into the format registry; an
        // unknown id is refused and no room is opened.
        let lobby = lobby(4);
        let mut client = Client::connect(&lobby).await;
        let _ = client.view().await;
        let err = lobby
            .command(
                &client.token,
                LobbyCommand::CreateRoom(CreateRoom {
                    config: config_with(2, "no-such-format"),
                }),
            )
            .await
            .expect_err("unknown game_setup is rejected");
        assert_eq!(err, LobbyError::UnknownFormat("no-such-format".to_string()));
        // Rejection re-sends the current view: still roomless.
        assert!(client.current().room.is_none());
    }

    #[tokio::test]
    async fn create_room_accepts_the_seeded_starter_format() {
        // The seeded "starter-1v1" format resolves, so a room can be opened with it.
        let lobby = lobby(4);
        let mut client = Client::connect(&lobby).await;
        let _ = client.view().await;
        lobby
            .command(
                &client.token,
                LobbyCommand::CreateRoom(CreateRoom {
                    config: config_with(2, "starter-1v1"),
                }),
            )
            .await
            .expect("the seeded starter format is accepted");
        assert!(client.view().await.room.is_some());
    }

    #[tokio::test]
    async fn issue_349_ffa_format_rejects_a_seat_count_it_does_not_allow() {
        // The free-for-all format seats 3–4 (issue #349): a two-seat request is a
        // valid lobby seat count but not one this format allows, so it is rejected
        // non-fatally and no room opens.
        let lobby = lobby(4);
        let mut client = Client::connect(&lobby).await;
        let _ = client.view().await;
        let err = lobby
            .command(
                &client.token,
                LobbyCommand::CreateRoom(CreateRoom {
                    config: config_with(2, "standard_ffa"),
                }),
            )
            .await
            .expect_err("2 seats is not a free-for-all count");
        assert_eq!(
            err,
            LobbyError::SeatCountForFormat {
                seats: 2,
                format: "standard_ffa".to_string(),
            }
        );
        assert!(client.current().room.is_none());
    }

    #[tokio::test]
    async fn issue_546_create_room_carries_a_table_name_and_visibility() {
        // The two additive config fields survive into the room the creator sees, and
        // the host is offered `update_room` to change them again.
        let lobby = lobby(4);
        let mut alice = Client::connect(&lobby).await;
        let _ = alice.view().await;
        lobby
            .command(
                &alice.token,
                LobbyCommand::CreateRoom(CreateRoom {
                    config: RoomConfig {
                        visibility: RoomVisibility::Private,
                        ..named(2, "standard_2p", "  Casual Commander  ")
                    },
                }),
            )
            .await
            .expect("a named private table is created");
        let view = alice.view().await;
        let room = view.room.expect("alice in room");
        // The name is stored trimmed; the visibility is carried verbatim.
        assert_eq!(room.config.name.as_deref(), Some("Casual Commander"));
        assert_eq!(room.config.visibility, RoomVisibility::Private);
        assert!(view.valid_commands.contains(&"update_room".to_string()));
    }

    #[tokio::test]
    async fn issue_546_an_unnamed_room_stays_unnamed_and_public() {
        // A client that omits both fields creates exactly the pre-#546 room: no name
        // (the client labels it by its format) and public.
        let lobby = lobby(4);
        let mut alice = Client::connect(&lobby).await;
        let _ = alice.view().await;
        lobby
            .command(
                &alice.token,
                LobbyCommand::CreateRoom(CreateRoom { config: config(2) }),
            )
            .await
            .expect("an unnamed room is created");
        let room = alice.view().await.room.expect("alice in room");
        assert_eq!(room.config.name, None, "the server invents no name");
        assert_eq!(room.config.visibility, RoomVisibility::Public);

        // A whitespace-only name is normalized away rather than stored as a title.
        lobby
            .command(&alice.token, update(named(2, "standard_2p", "   ")))
            .await
            .expect("a blank name normalizes to none");
        assert_eq!(
            alice.current().room.expect("in room").config.name,
            None,
            "a blank name is not a title"
        );
    }

    #[tokio::test]
    async fn issue_546_an_invalid_table_name_is_rejected_and_the_table_is_untouched() {
        let lobby = lobby(4);
        let mut alice = Client::connect(&lobby).await;
        let _ = alice.view().await;
        lobby
            .command(
                &alice.token,
                LobbyCommand::CreateRoom(CreateRoom {
                    config: named(2, "standard_2p", "Casual Commander"),
                }),
            )
            .await
            .expect("alice creates a named table");
        let _ = alice.view().await;

        let too_long = "x".repeat(MAX_NAME_LEN + 1);
        assert_eq!(
            lobby
                .command(&alice.token, update(named(2, "standard_2p", &too_long)))
                .await,
            Err(LobbyError::InvalidRoomName(NameError::TooLong(
                MAX_NAME_LEN + 1
            )))
        );
        assert_eq!(
            lobby
                .command(&alice.token, update(named(2, "standard_2p", "bad\nname")))
                .await,
            Err(LobbyError::InvalidRoomName(NameError::Unprintable))
        );
        // Non-fatal: the table keeps the name it had.
        assert_eq!(
            alice
                .current()
                .room
                .expect("in room")
                .config
                .name
                .as_deref(),
            Some("Casual Commander")
        );
    }

    #[tokio::test]
    async fn issue_546_only_the_host_may_update_the_room() {
        // Alice hosts a 3-seat free-for-all, bob joins seat 1. `update_room` is not even
        // offered to bob, and issuing it anyway is a typed rejection that changes
        // nothing.
        let lobby = lobby(4);
        let mut alice = Client::connect(&lobby).await;
        let _ = alice.view().await;
        lobby
            .command(
                &alice.token,
                LobbyCommand::CreateRoom(CreateRoom {
                    config: config_with(3, "standard_ffa"),
                }),
            )
            .await
            .expect("alice creates a 3-seat FFA");
        let room_id = alice.view().await.room.expect("alice in room").room_id;
        let mut bob = Client::connect(&lobby).await;
        let _ = bob.view().await;
        lobby
            .command(&bob.token, LobbyCommand::JoinRoom(JoinRoom { room_id }))
            .await
            .expect("bob joins");
        let bob_view = bob.view().await;

        assert!(
            !bob_view.valid_commands.contains(&"update_room".to_string()),
            "a non-host is never offered update_room"
        );
        assert_eq!(
            lobby
                .command(&bob.token, update(named(4, "standard_ffa", "Bob's Table")))
                .await,
            Err(LobbyError::NotHost)
        );
        assert_eq!(
            bob.current().room.expect("bob in room").config.seats,
            3,
            "the rejected edit changed nothing"
        );
    }

    #[tokio::test]
    async fn issue_546_shrinking_below_the_occupied_seats_is_rejected_not_clamped() {
        // A 4-seat FFA with alice (seat 0), an AI (seat 1), and bob (seat 2). Shrinking
        // to 2 would drop bob's seat, so it is refused outright — the room keeps its
        // four seats and everyone keeps their place.
        let lobby = lobby(4);
        let mut alice = Client::connect(&lobby).await;
        let _ = alice.view().await;
        lobby
            .command(
                &alice.token,
                LobbyCommand::CreateRoom(CreateRoom {
                    config: config_with(4, "standard_ffa"),
                }),
            )
            .await
            .expect("alice creates a 4-seat FFA");
        let room_id = alice.view().await.room.expect("alice in room").room_id;
        lobby
            .command(
                &alice.token,
                LobbyCommand::AddAi(AddAi {
                    seat: 1,
                    kind: "random".to_string(),
                    cards: decklist(),
                    commander: None,
                }),
            )
            .await
            .expect("host seats an AI in seat 1");
        let mut bob = Client::connect(&lobby).await;
        let _ = bob.view().await;
        lobby
            .command(&bob.token, LobbyCommand::JoinRoom(JoinRoom { room_id }))
            .await
            .expect("bob joins seat 2");
        let _ = alice.view().await;

        // Seats 0..=2 are occupied, so two seats would evict bob: rejected outright,
        // with the smallest workable count named rather than silently clamped.
        assert_eq!(
            lobby
                .command(&alice.token, update(config_with(2, "standard_2p")))
                .await,
            Err(LobbyError::SeatsBelowOccupancy {
                seats: 2,
                needed: 3
            })
        );
        let room = alice.current().room.expect("alice still in room");
        assert_eq!(room.config.seats, 4, "the table is untouched");
        assert_eq!(room.seats.len(), 4);
        assert!(room.seats[2].occupied_by.is_some(), "bob keeps his seat");
        assert!(room.seats[1].ai.is_some(), "the AI keeps its seat");

        // Dropping only the trailing *empty* seat is fine: seat 3 holds nobody.
        lobby
            .command(&alice.token, update(config_with(3, "standard_ffa")))
            .await
            .expect("shrinking onto empty seats is allowed");
        assert_eq!(
            alice.current().room.expect("in room").seats.len(),
            3,
            "the empty tail seat was dropped"
        );
    }

    #[tokio::test]
    async fn issue_546_a_seat_count_change_clears_readiness_but_keeps_decks() {
        // A 3-seat free-for-all with two decked, ready humans and one open seat. Growing
        // it to four seats un-readies everyone — nobody stays ready to a table they did
        // not agree to — while their validated decks survive, because the format they
        // were validated against did not change.
        let lobby = lobby(4);
        let mut alice = Client::connect(&lobby).await;
        let _ = alice.view().await;
        lobby
            .command(
                &alice.token,
                LobbyCommand::CreateRoom(CreateRoom {
                    config: config_with(3, "standard_ffa"),
                }),
            )
            .await
            .expect("alice creates a 3-seat FFA");
        let room_id = alice.view().await.room.expect("alice in room").room_id;
        let mut bob = Client::connect(&lobby).await;
        let _ = bob.view().await;
        lobby
            .command(&bob.token, LobbyCommand::JoinRoom(JoinRoom { room_id }))
            .await
            .expect("bob joins");
        let _ = bob.view().await;
        submit_valid_deck(&lobby, &alice).await;
        submit_valid_deck(&lobby, &bob).await;
        lobby
            .command(&alice.token, LobbyCommand::Ready(Ready { ready: true }))
            .await
            .expect("alice readies");
        lobby
            .command(&bob.token, LobbyCommand::Ready(Ready { ready: true }))
            .await
            .expect("bob readies");
        assert!(alice.current().room.expect("in room").seats[0].ready);

        lobby
            .command(&alice.token, update(config_with(4, "standard_ffa")))
            .await
            .expect("the host grows the table");

        let room = alice.current().room.expect("alice in room");
        assert_eq!(room.seats.len(), 4, "a fourth, empty seat was appended");
        assert!(room.seats[3].occupied_by.is_none());
        assert!(
            room.seats.iter().all(|seat| !seat.ready),
            "a resized table un-readies every seat"
        );
        assert!(
            room.seats[0].decked && room.seats[1].decked,
            "the decks survive an unchanged format"
        );
        // Bob sees the same table, and no game was constructed by the edit.
        assert_eq!(bob.current().room.expect("bob in room").seats.len(), 4);
        assert!(!alice.started() && !bob.started());
    }

    #[tokio::test]
    async fn issue_546_a_format_change_clears_every_submitted_deck() {
        // The decks in a room were validated against its format. Changing the format
        // invalidates that judgment, so both the deck and the readiness are cleared and
        // every seat must submit again — the room can never start on a deck the new
        // format would refuse.
        let lobby = lobby(4);
        let (alice, bob, _room) = seated_pair_in(&lobby, "starter-1v1").await;
        submit_valid_deck(&lobby, &alice).await;
        submit_valid_deck(&lobby, &bob).await;
        lobby
            .command(&alice.token, LobbyCommand::Ready(Ready { ready: true }))
            .await
            .expect("alice readies");
        assert!(alice.current().room.expect("in room").seats[0].decked);

        lobby
            .command(&alice.token, update(config_with(2, "standard_2p")))
            .await
            .expect("the host changes the format");

        let room = alice.current().room.expect("alice in room");
        assert_eq!(room.config.game_setup, "standard_2p");
        assert!(
            room.seats.iter().all(|seat| !seat.decked && !seat.ready),
            "a format change clears every deck and ready flag"
        );
        // Alice is offered `submit_deck` again, and not `ready` (she has no deck).
        let commands = alice.current().valid_commands;
        assert!(commands.contains(&"submit_deck".to_string()));
        assert!(!commands.contains(&"ready".to_string()));
        assert!(!alice.started() && !bob.started());
    }

    #[tokio::test]
    async fn issue_546_a_private_table_leaves_the_public_directory() {
        // Visibility is a real server behaviour, not a label: a private room is absent
        // from every connection's directory and is reachable only by its id.
        let lobby = lobby(4);
        let mut alice = Client::connect(&lobby).await;
        let _ = alice.view().await;
        lobby
            .command(
                &alice.token,
                LobbyCommand::CreateRoom(CreateRoom { config: config(2) }),
            )
            .await
            .expect("alice creates a public table");
        let room_id = alice.view().await.room.expect("alice in room").room_id;

        let mut bob = Client::connect(&lobby).await;
        assert_eq!(
            bob.view().await.directory.len(),
            1,
            "a public table is browsable"
        );

        lobby
            .command(
                &alice.token,
                update(RoomConfig {
                    visibility: RoomVisibility::Private,
                    ..config(2)
                }),
            )
            .await
            .expect("the host makes the table private");
        assert!(
            bob.view().await.directory.is_empty(),
            "a private table is not listed"
        );
        // But it is still joinable by the id its host shares out-of-band.
        lobby
            .command(
                &bob.token,
                LobbyCommand::JoinRoom(JoinRoom {
                    room_id: room_id.clone(),
                }),
            )
            .await
            .expect("a private table is joinable by id");
        assert_eq!(bob.view().await.room.expect("bob in room").room_id, room_id);
    }

    #[tokio::test]
    async fn issue_546_update_room_is_refused_once_the_game_has_started() {
        let lobby = lobby(4);
        let (alice, bob, _room) = seated_pair(&lobby).await;
        submit_valid_deck(&lobby, &alice).await;
        submit_valid_deck(&lobby, &bob).await;
        lobby
            .command(&alice.token, LobbyCommand::Ready(Ready { ready: true }))
            .await
            .expect("alice readies");
        lobby
            .command(&bob.token, LobbyCommand::Ready(Ready { ready: true }))
            .await
            .expect("bob readies");
        assert!(alice.started(), "the game started");

        assert_eq!(
            lobby
                .command(&alice.token, update(named(2, "standard_2p", "Too late")))
                .await,
            Err(LobbyError::GameStarted)
        );
    }

    #[tokio::test]
    async fn issue_546_a_roomless_connection_cannot_update_a_room() {
        let lobby = lobby(4);
        let mut alice = Client::connect(&lobby).await;
        let view = alice.view().await;
        assert!(!view.valid_commands.contains(&"update_room".to_string()));
        assert_eq!(
            lobby
                .command(&alice.token, update(config(2)))
                .await
                .expect_err("a roomless connection has no table to edit"),
            LobbyError::NotSeated
        );
    }

    #[tokio::test]
    async fn issue_648_the_undo_rule_is_carried_to_every_seat_and_editable_by_the_host() {
        // Undo is a table rule: chosen at creation, echoed to everyone in the room, and
        // changed by the host while the table is still gathering — so a player reads
        // whether this table takes moves back before they sit down, not after.
        let lobby = lobby(4);
        let mut alice = Client::connect(&lobby).await;
        let _ = alice.view().await;
        lobby
            .command(
                &alice.token,
                LobbyCommand::CreateRoom(CreateRoom {
                    config: RoomConfig {
                        undo_enabled: true,
                        ..config(2)
                    },
                }),
            )
            .await
            .expect("alice creates an undo table");
        let room_id = alice.view().await.room.expect("alice in room").room_id;
        let mut bob = Client::connect(&lobby).await;
        let _ = bob.view().await;
        lobby
            .command(&bob.token, LobbyCommand::JoinRoom(JoinRoom { room_id }))
            .await
            .expect("bob joins");
        assert!(
            bob.view()
                .await
                .room
                .expect("bob in room")
                .config
                .undo_enabled,
            "the rule reaches the seat that joined, not just the one that chose it"
        );

        // The host turns it off before the game starts, and every seat's readiness goes
        // with it: nobody stays ready to a table that no longer takes moves back.
        submit_valid_deck(&lobby, &alice).await;
        submit_valid_deck(&lobby, &bob).await;
        lobby
            .command(&alice.token, LobbyCommand::Ready(Ready { ready: true }))
            .await
            .expect("alice readies");
        assert!(alice.current().room.expect("in room").seats[0].ready);
        lobby
            .command(&alice.token, update(config(2)))
            .await
            .expect("the host drops the undo rule");
        let room = alice.current().room.expect("alice in room");
        assert!(!room.config.undo_enabled, "the table no longer allows undo");
        assert!(
            room.seats.iter().all(|seat| !seat.ready),
            "a changed table rule un-readies every seat"
        );
        assert!(
            room.seats.iter().all(|seat| seat.decked),
            "the decks are untouched: the format did not change"
        );
    }
}
