//! View building and push fan-out for the lobby: projecting the registry into the
//! per-session [`LobbyView`], the public room directory, and the per-room roster, plus
//! the outbox pushes that deliver them. Pure code motion out of the lobby module root
//! (issue #409) — no behavior change.

use super::*;

/// Build the [`LobbyView`] for one session, or `None` if the token is unknown.
fn build_view(registry: &Registry, token: &SessionToken) -> Option<LobbyView> {
    let session = registry.sessions.get(token)?;
    let room = session
        .room
        .as_ref()
        .and_then(|room_id| build_room_view(registry, room_id));
    let valid_commands = valid_commands(registry, session);
    Some(LobbyView {
        session: token.clone(),
        you: session.player.clone(),
        name: session.name.clone(),
        room,
        directory: build_directory(registry),
        valid_commands,
    })
}

/// Project the room registry into the public room directory (issue #280): one
/// [`RoomSummary`] per browsable room, so a connection can discover and join an open
/// game without an out-of-band id. Only the config summary, the occupancy count, and
/// the lifecycle state are exposed — never a seat roster, a decklist, or any game
/// state. The list is the same for every connection (a global browser) and sorted by
/// room id for a stable, deterministic order.
///
/// A room is `gathering` while pre-game, `in_progress` once its game has started, and
/// **omitted** once that game has ended (the room task has stopped, so its handle is
/// no longer active) — a finished room simply leaves the list.
fn build_directory(registry: &Registry) -> Vec<RoomSummary> {
    let mut directory: Vec<RoomSummary> = registry
        .rooms
        .iter()
        .filter_map(|(room_id, room)| {
            let state = match &room.game {
                None => RoomState::Gathering,
                Some(handle) if handle.is_active() => RoomState::InProgress,
                // A finished game's task has stopped: drop it from the directory.
                Some(_) => return None,
            };
            // A seat is filled by a human *or* an AI opponent (issue #415), so both count
            // toward occupancy in the public directory.
            let filled = room
                .seats
                .iter()
                .zip(&room.ai_seats)
                .filter(|(session, ai)| session.is_some() || ai.is_some())
                .count();
            Some(RoomSummary {
                room_id: room_id.clone(),
                config: room.config.clone(),
                filled: u8::try_from(filled).unwrap_or(u8::MAX),
                // The room's spectator count (ADR 0022, issue #351): observers, not
                // seats — a count only, never a spectator identity.
                spectators: u8::try_from(room.spectators.len()).unwrap_or(u8::MAX),
                state,
            })
        })
        .collect();
    directory.sort_by(|a, b| a.room_id.cmp(&b.room_id));
    directory
}

/// The lobby commands legal for a session right now — the only source of
/// interactivity in a [`LobbyView`], exactly as `valid_actions` is in `GameView`.
///
/// Roomless: `create_room`/`join_room`. Seated in a pre-game room: always
/// `submit_deck` and `leave`, plus `ready` once the seat is decked, or `unready`
/// once it is ready. (A started room's seats are on the in-game contract and never
/// see a `LobbyView`, so no in-game case appears here.)
fn valid_commands(registry: &Registry, session: &Session) -> Vec<String> {
    // `set_name` is always available in the pre-game phase (issue #294): a connection
    // may name itself before joining a room and rename at any point up to game start.
    let Some(room_id) = session.room.as_ref() else {
        return vec![
            "set_name".to_string(),
            "create_room".to_string(),
            "join_room".to_string(),
            // A roomless connection may also spectate an in-progress room (issue #351).
            "spectate_room".to_string(),
        ];
    };
    let mut commands = vec!["set_name".to_string(), "submit_deck".to_string()];
    if let (Some(room), Some(seat)) = (registry.rooms.get(room_id), session.seat) {
        if let Some(gate) = room.gate.get(seat) {
            if gate.ready {
                commands.push("unready".to_string());
            } else if gate.deck.is_some() {
                commands.push("ready".to_string());
            }
        }
        // AI-seat management is host-only (issue #415): the seat 0 occupant may fill an
        // empty seat with an AI (`add_ai`) whenever one is open, and clear an AI seat
        // (`remove_ai`) whenever one exists. Advertising these in `valid_commands` is the
        // only signal the client needs — it renders the affordance from this, never from a
        // client-side "host" inference.
        if seat == 0 {
            let has_empty_seat = room
                .seats
                .iter()
                .zip(&room.ai_seats)
                .any(|(session, ai)| session.is_none() && ai.is_none());
            if has_empty_seat {
                commands.push("add_ai".to_string());
            }
            if room.ai_seats.iter().any(Option::is_some) {
                commands.push("remove_ai".to_string());
            }
        }
    }
    commands.push("leave".to_string());
    commands
}

/// Build the [`RoomView`] for a room: its config and full seat roster, with each
/// occupant resolved to its public [`PlayerId`]. Decklist *contents* are never
/// exposed — only the derived `decked` flag and the `ready` flag per seat.
fn build_room_view(registry: &Registry, room_id: &RoomId) -> Option<RoomView> {
    let room = registry.rooms.get(room_id)?;
    let seats = room
        .seats
        .iter()
        .enumerate()
        .map(|(index, occupant)| {
            let session = occupant.as_ref().and_then(|tok| registry.sessions.get(tok));
            let ai = room.ai_seats.get(index).and_then(Option::as_ref);
            // A human occupant is named by its public `PlayerId`; an AI seat (issue #415)
            // has no session identity and instead reports its kind in `ai`.
            let occupied_by = session.map(|session| session.player.clone());
            // The occupant's chosen display name (issue #294), public in the roster; for an
            // AI seat, the kind's own label so the roster reads e.g. "Random".
            let name = session
                .and_then(|session| session.name.clone())
                .or_else(|| ai.map(|ai| ai.name.clone()));
            let gate = room.gate.get(index);
            SeatView {
                seat: index as u8,
                occupied_by,
                name,
                decked: gate.is_some_and(|g| g.deck.is_some()),
                ready: gate.is_some_and(|g| g.ready),
                // The AI kind occupying this seat, if any (a human/empty seat omits it).
                ai: ai.map(|ai| ai.kind.id().to_string()),
            }
        })
        .collect();
    Some(RoomView {
        room_id: room_id.clone(),
        config: room.config.clone(),
        seats,
    })
}

/// Push a fresh [`LobbyView`] to one session's outbox. A closed outbox (the reader
/// is gone) is ignored — the disconnect path cleans the session up.
///
/// A session seated in a **started** room is skipped: it has already been sent the
/// terminal [`LobbySignal::Start`], and pushing a view would overwrite that hand-off
/// in the latest-value channel before the connection reads it. Started seats are on
/// the in-game contract and no longer render `LobbyView`s.
pub(crate) fn push_view(registry: &Registry, token: &SessionToken) {
    let Some(session) = registry.sessions.get(token) else {
        return;
    };
    if session
        .room
        .as_ref()
        .and_then(|room_id| registry.rooms.get(room_id))
        .is_some_and(|room| room.game.is_some())
    {
        return;
    }
    if let Some(view) = build_view(registry, token) {
        let _ = session.outbox.send(Some(LobbySignal::View(view)));
    }
}

/// Push a fresh [`LobbyView`] to every occupant of a room (their shared roster
/// changed).
pub(crate) fn push_room(registry: &Registry, room_id: &RoomId) {
    let Some(room) = registry.rooms.get(room_id) else {
        return;
    };
    let occupants: Vec<SessionToken> = room.seats.iter().flatten().cloned().collect();
    for token in &occupants {
        push_view(registry, token);
    }
}

/// Push a fresh [`LobbyView`] to **every** session, so a change to the room directory
/// (a room created, joined, left, or started — issue #280) reaches connections that
/// are browsing the room list, not just the affected room's own occupants. A session
/// seated in a started room is skipped by [`push_view`] (it is on the in-game
/// contract), so this only re-projects the directory to connections still in the
/// lobby phase.
pub(crate) fn broadcast_views(registry: &Registry) {
    // All borrows here are shared, so iterating the session keys while `push_view`
    // reads the registry is fine.
    for token in registry.sessions.keys() {
        push_view(registry, token);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::lobby::test_support::*;

    #[tokio::test]
    async fn create_room_seats_the_creator_and_returns_a_room_id() {
        let lobby = lobby(4);
        let mut client = Client::connect(&lobby).await;
        let initial = client.view().await;

        lobby
            .command(
                &client.token,
                LobbyCommand::CreateRoom(CreateRoom { config: config(2) }),
            )
            .await
            .expect("create a valid room");
        let view = client.view().await;

        let room = view.room.expect("creator is now in a room");
        assert!(!room.room_id.is_empty());
        assert_eq!(room.config.seats, 2);
        assert_eq!(room.seats.len(), 2);
        // The creator holds seat 0; seat 1 is empty.
        assert_eq!(
            room.seats[0].occupied_by.as_deref(),
            Some(initial.you.as_str())
        );
        assert!(room.seats[1].occupied_by.is_none());
        // No game is constructed: the roster reflects nobody decked or ready, and no seat
        // is filled by an AI (issue #415).
        assert!(room
            .seats
            .iter()
            .all(|s| !s.decked && !s.ready && s.ai.is_none()));
        // Seated but undecked: the seat may submit a deck or leave, not ready up. As the
        // host (seat 0) of a room with an open seat, it may also fill it with an AI
        // opponent (`add_ai`, issue #415).
        assert_eq!(
            view.valid_commands,
            vec![
                "set_name".to_string(),
                "submit_deck".to_string(),
                "add_ai".to_string(),
                "leave".to_string()
            ]
        );
    }

    #[tokio::test]
    async fn a_full_room_stays_in_the_lobby_phase_with_no_game() {
        // Two seats, both filled — yet with no ready gate passing nobody starts a
        // game: both occupants stay in the lobby phase. This is what retires the old
        // "live with one player and empty decks" behavior.
        let lobby = lobby(4);
        let mut alice = Client::connect(&lobby).await;
        let _ = alice.view().await;
        lobby
            .command(
                &alice.token,
                LobbyCommand::CreateRoom(CreateRoom { config: config(2) }),
            )
            .await
            .unwrap();
        let room_id = alice.view().await.room.unwrap().room_id;

        let mut bob = Client::connect(&lobby).await;
        let _ = bob.view().await;
        lobby
            .command(&bob.token, LobbyCommand::JoinRoom(JoinRoom { room_id }))
            .await
            .unwrap();

        // Both remain in the lobby: interactivity is deck submission / leave, never
        // in-game actions. No game has been constructed.
        assert_eq!(
            bob.view().await.valid_commands,
            vec![
                "set_name".to_string(),
                "submit_deck".to_string(),
                "leave".to_string()
            ]
        );
        assert_eq!(
            alice.view().await.valid_commands,
            vec![
                "set_name".to_string(),
                "submit_deck".to_string(),
                "leave".to_string()
            ]
        );
        assert!(!bob.started() && !alice.started());
    }

    #[tokio::test]
    async fn leaving_vacates_the_seat_and_notifies_peers() {
        let lobby = lobby(4);
        let mut alice = Client::connect(&lobby).await;
        let _ = alice.view().await;
        lobby
            .command(
                &alice.token,
                LobbyCommand::CreateRoom(CreateRoom { config: config(2) }),
            )
            .await
            .unwrap();
        let room_id = alice.view().await.room.unwrap().room_id;

        let mut bob = Client::connect(&lobby).await;
        let _ = bob.view().await;
        lobby
            .command(&bob.token, LobbyCommand::JoinRoom(JoinRoom { room_id }))
            .await
            .unwrap();
        let _ = bob.view().await;
        let _ = alice.view().await; // roster-updated push from bob's join

        // Bob leaves: his seat empties, he is roomless again, and alice is notified.
        lobby
            .command(&bob.token, LobbyCommand::Leave)
            .await
            .unwrap();
        let bob_after = bob.view().await;
        assert!(bob_after.room.is_none());
        assert_eq!(
            bob_after.valid_commands,
            vec![
                "set_name".to_string(),
                "create_room".to_string(),
                "join_room".to_string(),
                "spectate_room".to_string()
            ]
        );

        let alice_after = alice.view().await.room.expect("alice still holds the room");
        assert!(alice_after.seats[0].occupied_by.is_some());
        assert!(alice_after.seats[1].occupied_by.is_none());
    }

    #[tokio::test]
    async fn submit_deck_marks_the_seat_decked_for_everyone_and_offers_ready() {
        let lobby = lobby(4);
        let (alice, bob, _room) = seated_pair(&lobby).await;

        lobby
            .command(
                &alice.token,
                LobbyCommand::SubmitDeck(SubmitDeck {
                    cards: decklist(),
                    commander: None,
                }),
            )
            .await
            .expect("valid deck accepted");

        // Alice sees herself decked and is now offered `ready`.
        let alice_view = alice.current();
        let alice_room = alice_view.room.expect("alice in room");
        assert!(alice_room.seats[0].decked);
        assert!(!alice_room.seats[0].ready);
        assert_eq!(
            alice_view.valid_commands,
            vec![
                "set_name".to_string(),
                "submit_deck".to_string(),
                "ready".to_string(),
                "leave".to_string()
            ]
        );

        // Bob (an undecked peer) is told alice is decked, but never sees her deck.
        let bob_room = bob.current().room.expect("bob in room");
        assert!(bob_room.seats[0].decked);
        assert!(!bob_room.seats[1].decked);
    }

    #[tokio::test]
    async fn ready_toggles_and_un_ready_is_allowed_before_start() {
        // `command` pushes synchronously, so `current()` reflects the latest state
        // as soon as the call returns — no need to await intermediate frames (which
        // a latest-value watch would coalesce anyway).
        let lobby = lobby(4);
        let (alice, bob, _room) = seated_pair(&lobby).await;
        submit_valid_deck(&lobby, &alice).await;

        // Ready up: alice's seat shows ready and she is now offered `unready`, and
        // her peer sees the flag too.
        lobby
            .command(&alice.token, LobbyCommand::Ready(Ready { ready: true }))
            .await
            .expect("ready accepted");
        assert!(alice.current().room.expect("in room").seats[0].ready);
        assert_eq!(
            alice.current().valid_commands,
            vec![
                "set_name".to_string(),
                "submit_deck".to_string(),
                "unready".to_string(),
                "leave".to_string()
            ]
        );
        assert!(bob.current().room.expect("in room").seats[0].ready);

        // Un-ready: allowed before the game starts; the flag clears for everyone.
        lobby
            .command(&alice.token, LobbyCommand::Ready(Ready { ready: false }))
            .await
            .expect("un-ready accepted");
        assert!(!alice.current().room.expect("in room").seats[0].ready);
        assert!(!bob.current().room.expect("in room").seats[0].ready);
        // Only the decked seat readied then un-readied: still no game.
        assert!(!alice.started() && !bob.started());
    }

    #[tokio::test]
    async fn issue_351_a_spectator_watches_a_started_game_with_redaction_and_a_directory_count() {
        // Two players start a game; a third connection spectates it mid-game, is handed
        // off to the spectator bridge, and reads a redacted SpectatorView — while the
        // directory advertises the spectator as a count to everyone else browsing.
        let lobby = Lobby::bundled_with_overrides(8, Some(0xABCD), None).expect("bundled cards");
        let (alice, bob, room_id) = seated_pair(&lobby).await;
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

        // A browsing client sees the room in progress with no spectators yet.
        let mut carol = Client::connect(&lobby).await;
        let dir0 = carol.view().await;
        let listed = dir0
            .directory
            .iter()
            .find(|r| r.room_id == room_id)
            .expect("the started room is listed");
        assert_eq!(listed.state, RoomState::InProgress);
        assert_eq!(listed.spectators, 0);

        // Carol spectates the in-progress room and is handed off to the spectator bridge.
        lobby
            .command(
                &carol.token,
                LobbyCommand::SpectateRoom(SpectateRoom {
                    room_id: room_id.clone(),
                }),
            )
            .await
            .expect("spectate accepted");
        let handle = carol.spectate_handle().expect("a spectate hand-off");

        // Join as a spectator and read the first redacted view.
        let (tx, mut rx) = watch::channel::<Option<rune_protocol::SpectatorView>>(None);
        assert!(handle.send(crate::RoomInput::JoinSpectator { outbox: tx }));
        let spec = loop {
            if let Some(view) = rx.borrow_and_update().clone() {
                break view;
            }
            rx.changed()
                .await
                .expect("the first SpectatorView is pushed");
        };
        // Every seat is public; there is no receiver or decision surface at all.
        assert_eq!(spec.players.len(), 2, "both seats appear as public state");
        let json = serde_json::to_value(&spec).unwrap();
        assert!(
            json.get("you").is_none(),
            "no receiver id leaks to a spectator"
        );
        assert!(
            json.get("my_hand").is_none(),
            "no hand leaks to a spectator"
        );
        assert!(
            json.get("valid_actions").is_none(),
            "no decision surface for a spectator"
        );

        // The directory now advertises the spectator (count only) to another browser.
        let mut dave = Client::connect(&lobby).await;
        let dir1 = dave.view().await;
        let watched = dir1
            .directory
            .iter()
            .find(|r| r.room_id == room_id)
            .expect("the started room is still listed");
        assert_eq!(
            watched.spectators, 1,
            "the directory advertises one spectator"
        );
    }

    #[tokio::test]
    async fn set_name_is_accepted_and_projects_into_the_lobby_and_roster() {
        // Issue #294: a chosen name lands on the connection's own view and, once seated,
        // in the shared roster every occupant sees.
        let lobby = lobby(4);
        let (mut alice, mut bob, _room) = seated_pair(&lobby).await;

        lobby
            .command(
                &alice.token,
                LobbyCommand::SetName(SetName {
                    name: "Alice".into(),
                }),
            )
            .await
            .expect("a valid name is accepted");

        // Alice's own view carries her name...
        assert_eq!(alice.view().await.name.as_deref(), Some("Alice"));
        // ...and the roster names her seat for the peer (a public, un-redacted field).
        let bob_room = bob.view().await.room.expect("bob in room");
        assert_eq!(bob_room.seats[0].name.as_deref(), Some("Alice"));
        assert_eq!(bob_room.seats[1].name, None, "bob has not named himself");
    }
}
