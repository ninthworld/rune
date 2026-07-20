//! Command handlers for the lobby state machine: room creation, membership
//! (join/spectate/leave), the deck-submission and ready gate that constructs the game,
//! and display-name setting. The `Lobby` methods here are an additional `impl Lobby`
//! block; the free functions round out the [`LobbyCommand`] routing in the module root.
//! Pure code motion out of the lobby module root (issue #409) — no behavior change.

use super::*;

impl Lobby {
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
        if !SEAT_RANGE.contains(&config.seats) {
            return Err(LobbyError::InvalidSeatCount(config.seats));
        }
        // The `game_setup` id must name a registered format (ADR 0013 §4); an unknown
        // id is refused before a room is opened, so no room ever holds a setup the
        // server cannot build a game from or validate decks against.
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

    /// Handle `submit_deck`: resolve every card identity against the database, then
    /// validate the whole decklist against the room's **format** (ADR 0013 §4) and,
    /// on success, store the seat's deck (leaving it decked) and re-notify the room.
    ///
    /// Validation is authoritative and all-or-nothing, in two stages: first the first
    /// identity that does not resolve rejects the whole command with
    /// [`LobbyError::UnknownCard`] ("unknown ids → typed error, seat stays undecked",
    /// ADR 0012); then the resolved deck is checked against the format's deck-legality
    /// rules — size and per-card copy limit — and an illegal deck is rejected with a
    /// structured [`LobbyError::IllegalDeck`] naming the violation (ADR 0013 §4). On
    /// any rejection the seat keeps whatever deck it had (it stays undecked if it had
    /// none). Re-submitting a legal deck clears that seat's ready flag, so a changed
    /// deck must be re-readied. Deck legality is *server* policy, never an engine rule.
    pub(crate) fn submit_deck(
        &self,
        registry: &mut Registry,
        token: &SessionToken,
        cards: &[String],
        commander: Option<&str>,
    ) -> Result<(), LobbyError> {
        let (room_id, seat) = seat_of(registry, token)?;
        let room = registry
            .rooms
            .get_mut(&room_id)
            .ok_or(LobbyError::NotSeated)?;
        if room.game.is_some() {
            return Err(LobbyError::GameStarted);
        }
        // Resolve the whole list before mutating, so a bad identity leaves the seat's
        // existing gate state untouched.
        let mut deck = Vec::with_capacity(cards.len());
        for identity in cards {
            let card = resolve_card(&self.inner.db, identity)
                .ok_or_else(|| LobbyError::UnknownCard(identity.clone()))?;
            deck.push(card);
        }
        // Resolve the designated commander (CR 903.3, issue #372) the same way, so an
        // unknown commander identity is the same typed rejection as an unknown deck
        // card and leaves the seat's gate untouched.
        let commander = match commander {
            Some(identity) => Some(
                resolve_card(&self.inner.db, identity)
                    .ok_or_else(|| LobbyError::UnknownCard(identity.to_string()))?,
            ),
            None => None,
        };
        // Validate the resolved deck (and any commander) against the room's format
        // before storing it, so an illegal deck never seats a broken game (ADR 0013
        // §4). The format is guaranteed present: `create_room` rejected any unknown
        // `game_setup` id.
        if let Some(format) = self.inner.formats.get(&room.config.game_setup) {
            format
                .validate_deck(&deck, commander, &self.inner.db)
                .map_err(LobbyError::IllegalDeck)?;
        }
        if let Some(gate) = room.gate.get_mut(seat) {
            gate.deck = Some(deck);
            gate.commander = commander;
            gate.ready = false;
        }
        push_room(registry, &room_id);
        info!(%token, %room_id, seat, "seat submitted a valid deck");
        Ok(())
    }

    /// Handle `ready`: toggle the seat's ready flag, then — when readying up completes
    /// the gate — construct the game and hand every seat off to the in-game contract.
    ///
    /// Readying up requires a submitted deck ([`LobbyError::NotDecked`] otherwise);
    /// un-readying (`ready == false`) is always allowed for a seated player before the
    /// game starts. When the last seat readies and every seat is filled, decked, and
    /// ready, [`start_game`](Lobby::start_game) builds the `GameState` and switches the
    /// room to the game phase (ADR 0012).
    pub(crate) fn ready(
        &self,
        registry: &mut Registry,
        token: &SessionToken,
        ready: bool,
    ) -> Result<(), LobbyError> {
        let (room_id, seat) = seat_of(registry, token)?;
        let room = registry
            .rooms
            .get_mut(&room_id)
            .ok_or(LobbyError::NotSeated)?;
        if room.game.is_some() {
            return Err(LobbyError::GameStarted);
        }
        if ready && room.gate.get(seat).is_none_or(|g| g.deck.is_none()) {
            return Err(LobbyError::NotDecked);
        }
        if let Some(gate) = room.gate.get_mut(seat) {
            gate.ready = ready;
        }
        // Everyone in the room sees the changed ready flag.
        push_room(registry, &room_id);
        if ready {
            self.start_game(registry, &room_id);
        }
        info!(%token, %room_id, seat, ready, "seat toggled ready");
        Ok(())
    }

    /// Construct the game and hand off, but only if the room is fully gated: every
    /// seat occupied, decked, and ready. Otherwise a no-op — the room stays pre-game.
    ///
    /// On the gate passing, builds the room format's engine [`GameSetup`] (ADR 0013
    /// §4) from the seats' submitted decks in seat order with a server-generated seed,
    /// spawns a [`Room`] around
    /// [`GameState::new`], stores its handle on the [`RoomEntry`], and pushes each
    /// seated session a [`LobbySignal::Start`] carrying its seat and the room handle.
    /// Each connection then reunites its socket and switches to `serve_connection`
    /// (`GameView`s from here on). If construction fails — which cannot happen once
    /// every deck validated at submit against the same database — the game is not
    /// started and the room stays pre-game (logged), rather than panicking.
    fn start_game(&self, registry: &mut Registry, room_id: &RoomId) {
        let Some(room) = registry.rooms.get(room_id) else {
            return;
        };
        // Gate: every seat filled, decked, and ready.
        let ready_to_start = room
            .seats
            .iter()
            .zip(&room.gate)
            .all(|(occupant, gate)| occupant.is_some() && gate.deck.is_some() && gate.ready);
        if !ready_to_start {
            return;
        }

        // Build the setup from each seat's deck, in seat order. A seat that
        // designated a commander (CR 903.3, issue #372) hands it to the engine, which
        // sets that copy aside into the command zone (CR 903.6); a seat with none
        // behaves exactly as before commanders existed.
        let players: Vec<PlayerSetup> = room
            .gate
            .iter()
            .map(|gate| {
                let deck = gate.deck.clone().unwrap_or_default();
                match gate.commander {
                    Some(commander) => PlayerSetup::with_commander(deck, commander),
                    None => PlayerSetup::new(deck),
                }
            })
            .collect();
        // Seed the shuffle: a pinned override (deterministic games for the e2e
        // suite, ADR 0014 / issue #145) if configured, else a fresh per-game seed.
        let seed = self.inner.seed_override.unwrap_or_else(generate_seed);
        // The format supplies the engine `GameSetup` parameters (ADR 0013 §4); it is
        // guaranteed present (create_room rejected any unknown id), but fall back to
        // engine defaults rather than panicking if it is somehow absent.
        let mut setup: GameSetup = match self.inner.formats.get(&room.config.game_setup) {
            Some(format) => format.game_setup(players, seed),
            None => GameSetup::new(players, seed),
        };
        // A pinned starting life (e2e short game, issue #145) overrides the format's
        // default; normal play keeps the format's value.
        if let Some(life) = self.inner.life_override {
            setup.starting_life = life;
        }
        // Each seat's chosen display name in seat order (issue #294), so the room can
        // label players in every `GameView::player_names`. A seat with no name is `None`
        // and simply has no entry in the projected map.
        let player_names: Vec<Option<String>> = room
            .seats
            .iter()
            .map(|occupant| {
                occupant
                    .as_ref()
                    .and_then(|token| registry.sessions.get(token))
                    .and_then(|session| session.name.clone())
            })
            .collect();
        let db = self.inner.db.clone();
        let state = match GameState::new(&setup, &db) {
            Ok(state) => state,
            Err(error) => {
                // Unreachable in practice: every card id was validated at submit.
                warn!(%room_id, %error, "game construction failed; room stays pre-game");
                return;
            }
        };
        // Basic priority automation is on for real games (issue #264): an idle seat's
        // priority auto-passes so a spell-less turn does not cost a click per step,
        // gated by each seat's own `set_stops` preferences. Off only in unit tests
        // that drive priority pass-by-pass.
        let (handle, _task) = Room::new(state, db)
            .with_player_names(player_names)
            .with_auto_pass(AutoPassPolicy::On)
            .spawn();

        // Hand every seated session off to the in-game contract.
        let occupants: Vec<(Seat, SessionToken)> = room
            .seats
            .iter()
            .enumerate()
            .filter_map(|(seat, occupant)| occupant.clone().map(|token| (seat, token)))
            .collect();
        for (seat, token) in &occupants {
            if let Some(session) = registry.sessions.get(token) {
                let _ = session.outbox.send(Some(LobbySignal::Start {
                    seat: *seat,
                    room: handle.clone(),
                }));
            }
        }
        // Mark the room started so it rejects further lobby commands and is never
        // reaped as empty. The task handle keeps the room alive alongside the
        // connections' own handles.
        if let Some(room) = registry.rooms.get_mut(room_id) {
            room.game = Some(handle);
        }
        // The room flipped to `in_progress` in the directory: re-project to everyone
        // browsing (the room's own seats are on the in-game contract now and are
        // skipped by `push_view`, so their terminal `Start` hand-off is preserved).
        broadcast_views(registry);
        info!(%room_id, seats = occupants.len(), "ready gate passed; game constructed");
    }
}

/// Handle `join_room`: seat the joiner in the first free seat of an existing room,
/// or return a typed error for an unknown or full room.
pub(crate) fn join_room(
    registry: &mut Registry,
    token: &SessionToken,
    room_id: &RoomId,
) -> Result<(), LobbyError> {
    if registry
        .sessions
        .get(token)
        .is_some_and(|s| s.room.is_some())
    {
        return Err(LobbyError::AlreadyInRoom);
    }
    let room = registry
        .rooms
        .get_mut(room_id)
        .ok_or(LobbyError::UnknownRoom)?;
    let seat = room
        .seats
        .iter()
        .position(Option::is_none)
        .ok_or(LobbyError::RoomFull)?;
    room.seats[seat] = Some(token.clone());
    if let Some(session) = registry.sessions.get_mut(token) {
        session.room = Some(room_id.clone());
        session.seat = Some(seat);
    }
    // Every occupant's roster changed, and the room's occupancy changed in the
    // directory: re-project to occupants and to everyone browsing.
    broadcast_views(registry);
    info!(%token, %room_id, seat, "joined room");
    Ok(())
}

/// Handle `spectate_room` (ADR 0022, issue #351): attach the sender as a spectator of
/// an **in-progress** room without consuming a seat. Unlike [`join_room`] this succeeds
/// on a room whose seats are full, but the room's game must already be running — there
/// is no board to watch until the ready gate passes ([`LobbyError::RoomNotStarted`]).
/// On success the session is marked as spectating (`room` set, `seat` left `None`), the
/// room's spectator roster gains its token (advertised as a count in the directory),
/// and the connection is handed off to the read-only spectator bridge via
/// [`LobbySignal::Spectate`].
pub(crate) fn spectate_room(
    registry: &mut Registry,
    token: &SessionToken,
    room_id: &RoomId,
) -> Result<(), LobbyError> {
    if registry
        .sessions
        .get(token)
        .is_some_and(|s| s.room.is_some())
    {
        return Err(LobbyError::AlreadyInRoom);
    }
    let room = registry
        .rooms
        .get_mut(room_id)
        .ok_or(LobbyError::UnknownRoom)?;
    // A spectator needs a live game to watch. A gathering room has no board yet.
    let handle = match &room.game {
        Some(handle) if handle.is_active() => handle.clone(),
        _ => return Err(LobbyError::RoomNotStarted),
    };
    room.spectators.push(token.clone());
    if let Some(session) = registry.sessions.get(token) {
        // Hand this connection off to the read-only spectator contract immediately —
        // like the `Start` gate, a terminal signal after which no `LobbyView` is pushed.
        let _ = session
            .outbox
            .send(Some(LobbySignal::Spectate { room: handle }));
    }
    if let Some(session) = registry.sessions.get_mut(token) {
        session.room = Some(room_id.clone());
        session.seat = None;
    }
    // The room's spectator count changed in the directory: re-project to browsers.
    broadcast_views(registry);
    info!(%token, %room_id, "joined room as spectator");
    Ok(())
}

/// Handle `leave`: vacate the sender's seat, reclaim the room if it is now empty,
/// otherwise notify the remaining occupants.
pub(crate) fn leave_room(registry: &mut Registry, token: &SessionToken) -> Result<(), LobbyError> {
    let (room_id, seat) = match registry.sessions.get(token) {
        Some(Session {
            room: Some(room_id),
            seat,
            ..
        }) => (room_id.clone(), *seat),
        _ => return Err(LobbyError::NotInRoom),
    };
    // A spectator (issue #351) holds no seat: drop it from the room's spectator roster
    // instead of vacating a seat, then clear its session and re-project the directory
    // (its spectator count changed). The room is never reaped for losing a spectator.
    let Some(seat) = seat else {
        if let Some(room) = registry.rooms.get_mut(&room_id) {
            room.spectators.retain(|t| t != token);
        }
        if let Some(session) = registry.sessions.get_mut(token) {
            session.room = None;
        }
        broadcast_views(registry);
        info!(%token, %room_id, "stopped spectating room");
        return Ok(());
    };
    vacate(registry, &room_id, seat);
    if let Some(session) = registry.sessions.get_mut(token) {
        session.room = None;
        session.seat = None;
    }
    reap_empty(registry);
    // The room's occupancy changed (or it was reclaimed and left the directory):
    // re-project to its remaining occupants and to everyone browsing.
    broadcast_views(registry);
    info!(%token, %room_id, seat, "left room");
    Ok(())
}

/// Handle `set_name`: validate the requested display name and store it on the
/// session (issue #294). On success the affected views are re-pushed — the sender's
/// own, and, if it is seated, the whole room roster so every occupant sees the new
/// name. On rejection the name is left untouched and a typed [`LobbyError::InvalidName`]
/// is returned; the caller re-sends the sender's current [`LobbyView`] unchanged (the
/// lobby's non-fatal error pattern).
pub(crate) fn set_name(
    registry: &mut Registry,
    token: &SessionToken,
    requested: &str,
) -> Result<(), LobbyError> {
    let name = validate_name(requested).map_err(LobbyError::InvalidName)?;
    let Some(session) = registry.sessions.get_mut(token) else {
        return Err(LobbyError::UnknownSession);
    };
    session.name = Some(name);
    // If the session is seated, its name appears in the shared roster, so re-project to
    // every occupant; otherwise only the sender's own view changed.
    match session.room.clone() {
        Some(room_id) => push_room(registry, &room_id),
        None => push_view(registry, token),
    }
    info!(%token, "connection set its display name");
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::lobby::test_support::*;
    use crate::test_support::fixture;

    #[tokio::test]
    async fn issue_351_spectating_a_gathering_room_is_rejected_non_fatally() {
        // A room that has not started has no board to watch: spectate is rejected with
        // the lobby's non-fatal error, and the would-be spectator stays roomless.
        let lobby = Lobby::bundled_with_overrides(8, None, None).expect("bundled cards");
        let (alice, _bob, room_id) = seated_pair(&lobby).await; // a gathering room
        let mut carol = Client::connect(&lobby).await;
        let _ = carol.view().await;

        let err = lobby
            .command(
                &carol.token,
                LobbyCommand::SpectateRoom(SpectateRoom {
                    room_id: room_id.clone(),
                }),
            )
            .await
            .expect_err("spectating a gathering room is rejected");
        assert_eq!(err, LobbyError::RoomNotStarted);
        // Carol is still roomless (no spectate hand-off, no seat).
        assert!(carol.spectate_handle().is_none());
        assert!(carol.current().room.is_none());
        // The seated player is unaffected.
        assert!(!alice.started());
    }

    #[tokio::test]
    async fn a_pinned_starting_life_overrides_the_format_default() {
        // Seat 0 sees seat 1 (its only opponent) start at the pinned life, not the
        // format's 20 — proof the override reaches game construction (issue #145).
        let view = first_game_view_for(Some(0xABCD), Some(4)).await;
        let opponent_life = view.opponents.first().expect("one opponent").life;
        assert_eq!(opponent_life, 4, "the starting-life override applied");
    }

    #[tokio::test]
    async fn a_pinned_seed_reproduces_the_same_opening_hand() {
        // Same override → identical shuffle (ADR 0014), so the opening hand matches.
        let first = opening_hand_names_for_seed(Some(0xC0FF_EE00_1234_5678)).await;
        let again = opening_hand_names_for_seed(Some(0xC0FF_EE00_1234_5678)).await;
        assert!(!first.is_empty(), "the opening hand is non-empty");
        assert_eq!(first, again, "a pinned seed reproduces the opening hand");

        // A different pinned seed diverges (the shuffle actually depends on it).
        let other = opening_hand_names_for_seed(Some(0x1111_2222_3333_4444)).await;
        assert_ne!(
            first, other,
            "a different seed shuffles to a different opening hand"
        );
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
                        config: config(seats),
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
        // The `game_setup` id must key into the format registry (ADR 0013 §4); an
        // unknown id is refused and no room is opened.
        let lobby = lobby(4);
        let mut client = Client::connect(&lobby).await;
        let _ = client.view().await;
        let err = lobby
            .command(
                &client.token,
                LobbyCommand::CreateRoom(CreateRoom {
                    config: RoomConfig {
                        seats: 2,
                        game_setup: "no-such-format".to_string(),
                    },
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
                    config: RoomConfig {
                        seats: 2,
                        game_setup: "starter-1v1".to_string(),
                    },
                }),
            )
            .await
            .expect("the seeded starter format is accepted");
        assert!(client.view().await.room.is_some());
    }

    #[tokio::test]
    async fn join_by_id_seats_the_joiner_and_updates_every_roster() {
        let lobby = lobby(4);
        let mut alice = Client::connect(&lobby).await;
        let _ = alice.view().await;
        lobby
            .command(
                &alice.token,
                LobbyCommand::CreateRoom(CreateRoom { config: config(2) }),
            )
            .await
            .expect("alice creates");
        let alice_room = alice.view().await.room.expect("alice in room");
        let room_id = alice_room.room_id.clone();

        let mut bob = Client::connect(&lobby).await;
        let _ = bob.view().await;
        lobby
            .command(
                &bob.token,
                LobbyCommand::JoinRoom(JoinRoom {
                    room_id: room_id.clone(),
                }),
            )
            .await
            .expect("bob joins by id");

        // Bob is seated at seat 1 of the same room.
        let bob_room = bob.view().await.room.expect("bob in room");
        assert_eq!(bob_room.room_id, room_id);
        assert_eq!(
            bob_room.seats[1].occupied_by.as_deref(),
            Some(bob.current().you.as_str())
        );

        // Alice was pushed an updated roster showing both seats filled.
        let alice_after = alice.view().await.room.expect("alice still in room");
        assert!(alice_after.seats[0].occupied_by.is_some());
        assert!(alice_after.seats[1].occupied_by.is_some());
    }

    #[tokio::test]
    async fn joining_an_unknown_room_is_a_typed_error() {
        let lobby = lobby(4);
        let mut bob = Client::connect(&lobby).await;
        let _ = bob.view().await;
        let err = lobby
            .command(
                &bob.token,
                LobbyCommand::JoinRoom(JoinRoom {
                    room_id: "r-nope".to_string(),
                }),
            )
            .await
            .expect_err("unknown room id is rejected");
        assert_eq!(err, LobbyError::UnknownRoom);
        assert!(bob.current().room.is_none());
    }

    #[tokio::test]
    async fn joining_a_full_room_is_a_typed_error() {
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
            .command(
                &bob.token,
                LobbyCommand::JoinRoom(JoinRoom {
                    room_id: room_id.clone(),
                }),
            )
            .await
            .expect("bob fills the second seat");
        let _ = bob.view().await;

        // The two-seat room is full: a third joiner is refused and stays roomless.
        let mut carol = Client::connect(&lobby).await;
        let _ = carol.view().await;
        let err = lobby
            .command(&carol.token, LobbyCommand::JoinRoom(JoinRoom { room_id }))
            .await
            .expect_err("a full room is rejected");
        assert_eq!(err, LobbyError::RoomFull);
        assert!(carol.current().room.is_none());
    }

    #[tokio::test]
    async fn create_or_join_while_already_in_a_room_is_rejected() {
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
            .unwrap();
        let room_id = alice.view().await.room.unwrap().room_id;

        // A second create while seated is rejected.
        assert_eq!(
            lobby
                .command(
                    &alice.token,
                    LobbyCommand::CreateRoom(CreateRoom { config: config(2) })
                )
                .await,
            Err(LobbyError::AlreadyInRoom)
        );
        // As is a join while seated.
        assert_eq!(
            lobby
                .command(&alice.token, LobbyCommand::JoinRoom(JoinRoom { room_id }))
                .await,
            Err(LobbyError::AlreadyInRoom)
        );
    }

    #[tokio::test]
    async fn submit_deck_with_an_unknown_card_is_rejected_and_seat_stays_undecked() {
        let lobby = lobby(4);
        let (alice, _bob, _room) = seated_pair(&lobby).await;

        // A non-existent id (bundled db holds only 1..=6) rejects the whole list.
        let err = lobby
            .command(
                &alice.token,
                LobbyCommand::SubmitDeck(SubmitDeck {
                    cards: vec![wire_id("forest"), "no_such_card".to_string()],
                    commander: None,
                }),
            )
            .await
            .expect_err("unknown card id is rejected");
        assert_eq!(err, LobbyError::UnknownCard("no_such_card".to_string()));
        // The seat stays undecked; the rejection re-sent the current view.
        assert!(!alice.current().room.expect("in room").seats[0].decked);
    }

    #[tokio::test]
    async fn submit_deck_under_the_minimum_size_is_rejected_and_seat_stays_undecked() {
        // The seeded format requires 40 cards (ADR 0013 §4); a ten-card deck of known
        // ids is rejected as illegal, and the seat is left undecked.
        let lobby = lobby(4);
        let (alice, _bob, _room) = seated_pair_in(&lobby, "starter-1v1").await;

        let err = lobby
            .command(
                &alice.token,
                LobbyCommand::SubmitDeck(SubmitDeck {
                    cards: vec![wire_id("forest"); 10],
                    commander: None,
                }),
            )
            .await
            .expect_err("an under-minimum deck is rejected");
        assert_eq!(
            err,
            LobbyError::IllegalDeck(DeckError::BelowMinimum { have: 10, min: 40 })
        );
        assert!(!alice.current().room.expect("in room").seats[0].decked);
    }

    #[tokio::test]
    async fn submit_deck_over_the_copy_limit_for_a_non_basic_is_rejected() {
        // Five copies of a non-basic (id 1) in an otherwise legal 40-card deck exceed
        // the four-copy limit (ADR 0013 §4); the deck is rejected and stays out.
        let lobby = lobby(4);
        let (alice, _bob, _room) = seated_pair_in(&lobby, "starter-1v1").await;

        let mut cards = vec![wire_id("onakke_ogre"); 5];
        for slug in &NON_BASICS[1..] {
            for _ in 0..4 {
                cards.push(wire_id(slug));
            }
        }
        for _ in 0..19 {
            cards.push(wire_id("forest"));
        }
        assert_eq!(cards.len(), 40);

        let err = lobby
            .command(
                &alice.token,
                LobbyCommand::SubmitDeck(SubmitDeck {
                    cards,
                    commander: None,
                }),
            )
            .await
            .expect_err("an over-copy-limit deck is rejected");
        assert_eq!(
            err,
            LobbyError::IllegalDeck(DeckError::CopyLimit {
                card: fixture("onakke_ogre"),
                count: 5,
                limit: 4,
            })
        );
        assert!(!alice.current().room.expect("in room").seats[0].decked);
    }

    #[tokio::test]
    async fn submit_deck_accepts_a_legal_deck_with_many_basics() {
        // The shared `decklist()` holds twenty basic Forests, far over the
        // four-copy limit, yet basics are exempt (ADR 0013 §4): the deck is accepted.
        let lobby = lobby(4);
        let (alice, _bob, _room) = seated_pair_in(&lobby, "starter-1v1").await;

        lobby
            .command(
                &alice.token,
                LobbyCommand::SubmitDeck(SubmitDeck {
                    cards: decklist(),
                    commander: None,
                }),
            )
            .await
            .expect("a legal deck with many basics is accepted");
        assert!(alice.current().room.expect("in room").seats[0].decked);
    }

    #[tokio::test]
    async fn readying_up_requires_a_submitted_deck() {
        let lobby = lobby(4);
        let (alice, _bob, _room) = seated_pair(&lobby).await;

        // Ready before decking is a typed error; the seat stays unready.
        assert_eq!(
            lobby
                .command(&alice.token, LobbyCommand::Ready(Ready { ready: true }))
                .await,
            Err(LobbyError::NotDecked)
        );
        assert!(!alice.current().room.expect("in room").seats[0].ready);
    }

    #[tokio::test]
    async fn start_is_blocked_while_a_seat_is_undecked() {
        let lobby = lobby(4);
        let (alice, bob, _room) = seated_pair(&lobby).await;
        // Only alice decks and readies; bob never submits a deck.
        submit_valid_deck(&lobby, &alice).await;
        lobby
            .command(&alice.token, LobbyCommand::Ready(Ready { ready: true }))
            .await
            .expect("alice readies");

        // The gate cannot pass with bob undecked: no game is constructed.
        assert!(!alice.started() && !bob.started());
        assert!(alice.current().room.expect("in room").seats[0].ready);
    }

    #[tokio::test]
    async fn start_is_blocked_while_a_seat_is_unready() {
        let lobby = lobby(4);
        let (alice, bob, _room) = seated_pair(&lobby).await;
        // Both deck; only alice readies.
        submit_valid_deck(&lobby, &alice).await;
        submit_valid_deck(&lobby, &bob).await;
        lobby
            .command(&alice.token, LobbyCommand::Ready(Ready { ready: true }))
            .await
            .expect("alice readies");

        // Bob is decked but unready: the gate stays shut.
        assert!(!alice.started() && !bob.started());
    }

    #[tokio::test]
    async fn last_seat_readying_constructs_the_game_and_hands_off_every_seat() {
        let lobby = lobby(4);
        let (alice, bob, _room) = seated_pair(&lobby).await;
        submit_valid_deck(&lobby, &alice).await;
        submit_valid_deck(&lobby, &bob).await;

        // Alice readies first — not enough; the gate needs every seat.
        lobby
            .command(&alice.token, LobbyCommand::Ready(Ready { ready: true }))
            .await
            .expect("alice readies");
        assert!(!alice.started() && !bob.started());

        // Bob readies last: the gate passes and both seats are handed off to a game.
        // The terminal `Start` supersedes the roster push in each latest-value outbox.
        lobby
            .command(&bob.token, LobbyCommand::Ready(Ready { ready: true }))
            .await
            .expect("bob readies");

        assert_eq!(alice.start_seat(), Some(0));
        assert_eq!(bob.start_seat(), Some(1));

        // Post-start, further lobby commands to the started room are rejected.
        assert_eq!(
            lobby
                .command(&alice.token, LobbyCommand::Ready(Ready { ready: false }))
                .await,
            Err(LobbyError::GameStarted)
        );
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
    async fn issue_349_three_seat_free_for_all_starts_a_three_player_game() {
        // Creating a 3-seat free-for-all room, decking and readying every seat, starts
        // an engine game seating that many players (the FFA-format acceptance).
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
            .expect("alice creates a 3-seat FFA room");
        let room_id = alice.view().await.room.expect("alice in room").room_id;
        assert_eq!(
            alice.current().room.unwrap().seats.len(),
            3,
            "the room has three seats"
        );

        // Two more players join.
        let mut others = Vec::new();
        for _ in 0..2 {
            let mut client = Client::connect(&lobby).await;
            let _ = client.view().await;
            lobby
                .command(
                    &client.token,
                    LobbyCommand::JoinRoom(JoinRoom {
                        room_id: room_id.clone(),
                    }),
                )
                .await
                .expect("player joins the FFA room");
            let _ = client.view().await;
            others.push(client);
        }
        let _ = alice.view().await;

        // Every seat decks and readies; the game starts only once all three are in.
        submit_valid_deck(&lobby, &alice).await;
        for client in &others {
            submit_valid_deck(&lobby, client).await;
        }
        lobby
            .command(&alice.token, LobbyCommand::Ready(Ready { ready: true }))
            .await
            .expect("alice readies");
        for client in &others {
            lobby
                .command(&client.token, LobbyCommand::Ready(Ready { ready: true }))
                .await
                .expect("player readies");
        }

        // All three seats are handed off to a running game, one per seat index.
        assert_eq!(alice.start_seat(), Some(0));
        assert_eq!(others[0].start_seat(), Some(1));
        assert_eq!(others[1].start_seat(), Some(2));
    }

    #[tokio::test]
    async fn a_display_name_survives_a_reconnect() {
        // Issue #294: the name is bound to the session, so a per-tab reconnect (echoing
        // the session token) is reunited with the same name.
        let lobby = lobby(4);
        let (alice, _bob, _room) = seated_pair(&lobby).await;
        lobby
            .command(
                &alice.token,
                LobbyCommand::SetName(SetName {
                    name: "Alice".into(),
                }),
            )
            .await
            .expect("name accepted");

        // Drop the connection (the seated session is held open) and reconnect by token.
        lobby.disconnect(&alice.handle()).await;
        let mut returning = Client::reconnect(&lobby, Some(alice.token.clone())).await;
        let resumed = returning.view().await;
        assert_eq!(
            resumed.name.as_deref(),
            Some("Alice"),
            "name survived reconnect"
        );
        let room = resumed.room.expect("reclaimed the held seat");
        assert_eq!(room.seats[0].name.as_deref(), Some("Alice"));
    }

    #[tokio::test]
    async fn player_names_project_into_the_game_view_at_game_start() {
        // Issue #294: names set in the lobby reach the constructed game, keyed by the
        // `p{N}` player id, so every in-game surface can label players.
        let lobby = lobby(4);
        let (alice, bob, _room) = seated_pair(&lobby).await;
        lobby
            .command(
                &alice.token,
                LobbyCommand::SetName(SetName {
                    name: "Alice".into(),
                }),
            )
            .await
            .expect("alice names herself");
        lobby
            .command(
                &bob.token,
                LobbyCommand::SetName(SetName { name: "Bob".into() }),
            )
            .await
            .expect("bob names himself");
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

        // Join seat 0's constructed game and read its first personalized GameView.
        let handle = alice.start_handle().expect("game constructed");
        let (tx, mut rx) = watch::channel::<Option<rune_protocol::GameView>>(None);
        assert!(handle.send(crate::RoomInput::Join {
            seat: 0,
            outbox: tx
        }));
        let view = loop {
            if let Some(view) = rx.borrow_and_update().clone() {
                break view;
            }
            rx.changed().await.expect("first GameView is pushed");
        };
        assert_eq!(
            view.player_names.get("p0").map(String::as_str),
            Some("Alice")
        );
        assert_eq!(view.player_names.get("p1").map(String::as_str), Some("Bob"));
    }
}
