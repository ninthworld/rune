//! A connection's life in the lobby: the session it is issued, the token it may say
//! `hello` with, the commands it drives itself with, and the socket loop that carries
//! all three (issues #110, #113).

use crate::lobby::*;

impl Lobby {
    /// Register a freshly accepted connection: issue it an unguessable session token
    /// and a public identity, store its `outbox`, and push it its initial
    /// [`LobbyView`] (a roomless view offering `create_room`/`join_room`). Returns a
    /// [`SessionHandle`] the connection uses to address, and later tear down, the
    /// session.
    ///
    /// # Errors
    /// Returns the underlying [`getrandom::Error`] if the OS CSPRNG is unavailable:
    /// without unguessable entropy the server cannot safely mint a reconnect token,
    /// so the connection is refused rather than issued a weak one.
    pub(crate) async fn connect(
        &self,
        outbox: LobbyOutbox,
    ) -> Result<SessionHandle, getrandom::Error> {
        let mut registry = self.inner.registry.write().await;
        // The public identity is sequential (it is shown to opponents); the secret
        // token is not — it authenticates reconnect, so it must be unguessable.
        let n = registry.next_session;
        registry.next_session += 1;
        let player = format!("p{n}");
        let token = loop {
            let candidate = mint_token()?;
            if !registry.sessions.contains_key(&candidate) {
                break candidate;
            }
        };
        registry.sessions.insert(
            token.clone(),
            Session {
                player,
                name: None,
                room: None,
                seat: None,
                outbox,
                generation: 0,
            },
        );
        push_view(&registry, &token);
        info!(%token, "connection entered the lobby");
        Ok(SessionHandle {
            token,
            generation: 0,
        })
    }

    /// Route a [`Hello`](sage_protocol::Hello). `current` is the handle the
    /// connection was issued at [`connect`](Lobby::connect) (a fresh identity). If
    /// `echoed` names a *different*, still-known session, this connection proves it
    /// owns that seat by presenting the secret token, so it **supersedes** whatever
    /// connection last held it: the connection's outbox is moved onto the reclaimed
    /// session, the fresh identity is discarded, the session's generation is bumped
    /// (retiring the superseded connection), and the reclaimed session is resynced
    /// from one full [`LobbyView`]. Any other case — no token, the connection's own
    /// token, an unknown token, or one whose room has been reclaimed — keeps the
    /// fresh identity and re-sends its clean, roomless view (the "room gone"
    /// response). Returns the handle the connection should use henceforth.
    pub(crate) async fn hello(
        &self,
        current: &SessionHandle,
        echoed: Option<SessionToken>,
    ) -> SessionHandle {
        let mut registry = self.inner.registry.write().await;

        // Only a *different* token that names a live/held session is a reconnect. An
        // absent, self, unknown, or reaped token falls through to a fresh identity —
        // a token can never resolve to a seat that is not the one it was issued for
        // (issue #48), so a stranger never lands in a held seat.
        let target = echoed
            .as_ref()
            .filter(|t| **t != current.token && registry.sessions.contains_key(*t))
            .cloned();
        let Some(target) = target else {
            push_view(&registry, &current.token);
            return current.clone();
        };

        // Move this connection's outbox onto the reclaimed session and drop the fresh
        // identity `connect` minted, so only the reclaimed session survives.
        let Some(fresh) = registry.sessions.remove(&current.token) else {
            // The current session always exists here; defensively fall back to a
            // no-op reconnect if it somehow does not.
            push_view(&registry, &current.token);
            return current.clone();
        };
        let Some(session) = registry.sessions.get_mut(&target) else {
            // Unreachable: presence was checked above under the same lock. Restore
            // the fresh identity rather than lose the connection's outbox.
            registry.sessions.insert(current.token.clone(), fresh);
            push_view(&registry, &current.token);
            return current.clone();
        };
        session.outbox = fresh.outbox;
        session.generation += 1;
        let generation = session.generation;
        // A seat whose game is still running is handed straight back to it (issue #628); every
        // other reclaimed session is answered with its lobby view, as it always was.
        if !push_resume(&registry, &target) {
            push_view(&registry, &target);
        }
        info!(token = %target, "connection reclaimed a held seat via session token");
        SessionHandle {
            token: target,
            generation,
        }
    }

    /// End a connection. A **seated** session is *held open* — neither its seat nor
    /// its room is touched — so its token can reclaim the seat later (issue #113); a
    /// **roomless** session holds nothing to reconnect to and is removed. The seat is
    /// only ever vacated by an explicit `Leave`.
    ///
    /// The `handle`'s generation must still match the session's, so a **superseded**
    /// connection (an older generation retired by a token reconnect) cannot tear down
    /// the session a newer connection reclaimed. A handle for an already-removed
    /// session is likewise ignored, so a double disconnect cannot corrupt the
    /// registry.
    pub(crate) async fn disconnect(&self, handle: &SessionHandle) {
        let mut registry = self.inner.registry.write().await;
        let Some(session) = registry.sessions.get(&handle.token) else {
            return;
        };
        if session.generation != handle.generation {
            // A newer connection has superseded this one; leave its session intact.
            return;
        }
        // A **spectator** (issue #351) owns no seat, so there is nothing to hold open:
        // drop it from the room's spectator roster (keeping the advertised count
        // accurate) and remove the session. Reconnecting to watch is a fresh
        // `spectate_room`, which reconstructs the whole public board from its first
        // `SpectatorView` — the complete-view principle makes that indistinguishable
        // from resuming.
        if session.room.is_some() && session.seat.is_none() {
            let room_id = session.room.clone();
            registry.sessions.remove(&handle.token);
            if let Some(room_id) = room_id {
                if let Some(room) = registry.rooms.get_mut(&room_id) {
                    room.spectators.retain(|t| *t != handle.token);
                }
            }
            broadcast_views(&registry);
            info!(token = %handle.token, "spectator connection left");
            return;
        }
        if session.room.is_some() {
            info!(token = %handle.token, "connection dropped; seat held open for reconnect");
            return;
        }
        registry.sessions.remove(&handle.token);
        info!(token = %handle.token, "connection left the lobby");
    }

    /// Route one [`LobbyCommand`] from `token` against authoritative state. On
    /// success the affected connections are pushed a fresh [`LobbyView`]; on a typed
    /// [`LobbyError`] the sender's current view is re-sent unchanged and the error is
    /// returned (for logging/tests).
    pub(crate) async fn command(
        &self,
        token: &SessionToken,
        command: LobbyCommand,
    ) -> Result<(), LobbyError> {
        let mut registry = self.inner.registry.write().await;
        if !registry.sessions.contains_key(token) {
            return Err(LobbyError::UnknownSession);
        }
        let result = match command {
            // Reconnect-by-token is driven by [`Lobby::hello`] from the serve loop,
            // which can supersede the connection's identity (a generation change this
            // token-only router cannot express). A `Hello` reaching here — e.g. a
            // direct call in a test — is a harmless ack that re-sends the current view.
            LobbyCommand::Hello(_) => Ok(()),
            LobbyCommand::CreateRoom(CreateRoom { config }) => {
                self.create_room(&mut registry, token, config)
            }
            LobbyCommand::UpdateRoom(UpdateRoom { config }) => {
                self.update_room(&mut registry, token, config)
            }
            LobbyCommand::JoinRoom(JoinRoom { room_id }) => {
                join_room(&mut registry, token, &room_id)
            }
            LobbyCommand::SpectateRoom(SpectateRoom { room_id }) => {
                spectate_room(&mut registry, token, &room_id)
            }
            LobbyCommand::Leave => leave_room(&mut registry, token),
            LobbyCommand::SubmitDeck(SubmitDeck { cards, commander }) => {
                self.submit_deck(&mut registry, token, &cards, commander.as_deref())
            }
            LobbyCommand::AddAi(AddAi {
                seat,
                kind,
                cards,
                commander,
            }) => self.add_ai(
                &mut registry,
                token,
                seat,
                &kind,
                &cards,
                commander.as_deref(),
            ),
            LobbyCommand::RemoveAi(RemoveAi { seat }) => remove_ai(&mut registry, token, seat),
            LobbyCommand::Ready(Ready { ready }) => self.ready(&mut registry, token, ready),
            LobbyCommand::SetName(SetName { name }) => set_name(&mut registry, token, &name),
            // A catalog request is answered directly by the serve loop with a one-shot
            // `CatalogView` (it needs the socket, not this registry), so a request
            // reaching this router — e.g. a direct call in a test — is a harmless ack
            // that re-sends the current view (issue #367).
            LobbyCommand::RequestCatalog => Ok(()),
        };
        // Whether the command succeeded (and already pushed the affected views) or
        // was rejected, the sender always ends with a fresh, authoritative view.
        push_view(&registry, token);
        result
    }
}

/// Bridge a live WebSocket connection to the lobby for its pre-game phase.
///
/// This is the pre-game analogue of [`serve_connection`](crate::serve_connection):
/// it registers a session (receiving the initial [`LobbyView`]), then pumps the
/// socket both ways until either side closes. Decoded [`LobbyCommand`]s are routed
/// through [`Lobby::command`]; every [`LobbyView`] the lobby pushes is serialized to
/// JSON and written back. On exit the session is disconnected — a **seated** session
/// has its seat held open for reconnect (issue #113), a roomless one is dropped.
///
/// It carries **no game logic** — it only (de)serializes the lobby protocol and
/// routes commands to the authoritative registry. Constructing the engine game is
/// the lobby's job at the ready gate; when it fires, this bridge learns of it via a
/// [`LobbySignal::Start`] on the outbox, reunites its socket, and **hands off to
/// [`serve_connection`]** — from there the connection speaks the in-game `GameView`
/// contract for the life of that game. Nothing game-related is written before that.
///
/// `shutdown` lets the layer-1 server stop the bridge on server shutdown: when it
/// resolves, the session is released and the socket is closed politely, just as if
/// the peer had hung up. It is forwarded to the in-game bridge on hand-off, so a
/// started game shuts down cleanly too.
pub async fn serve_lobby_connection<S, F>(lobby: Lobby, ws: WebSocketStream<S>, shutdown: F)
where
    S: AsyncRead + AsyncWrite + Unpin,
    F: Future<Output = ()>,
{
    let (mut write, mut read) = ws.split();
    let (outbox_tx, mut outbox_rx) = watch::channel::<Option<LobbySignal>>(None);
    // Registering the session pushes the initial LobbyView onto the outbox, so the
    // writer arm below sends it as the connection's first frame. The handle can be
    // reassigned mid-connection when a `Hello` reconnects to a held seat.
    let mut handle = match lobby.connect(outbox_tx).await {
        Ok(handle) => handle,
        Err(error) => {
            // Without OS entropy we cannot mint an unguessable token; refuse rather
            // than issue a weak one.
            warn!(%error, "failed to mint a session token; closing connection");
            let _ = write.close().await;
            return;
        }
    };

    // Set once the ready gate hands this connection off to a started game.
    let mut handoff: Option<(Seat, RoomHandle)> = None;
    // Set once this connection joins a running game as a spectator (issue #351).
    let mut spectate_handoff: Option<RoomHandle> = None;

    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            () = &mut shutdown => break,
            incoming = read.next() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    match serde_json::from_str::<LobbyCommand>(text.as_str()) {
                        // The catalog is static reference data, not per-connection lobby
                        // state (issue #367): answer it directly with a one-shot
                        // `CatalogView` frame rather than through the latest-value outbox,
                        // which only carries `LobbyView`s and could drop the response
                        // under a concurrent directory broadcast.
                        Ok(LobbyCommand::RequestCatalog) => {
                            match serde_json::to_string(&lobby.catalog()) {
                                Ok(json) => {
                                    if write.send(Message::Text(json)).await.is_err() {
                                        break;
                                    }
                                }
                                Err(error) => {
                                    warn!(token = %handle.token, %error, "failed to serialize catalog view");
                                }
                            }
                        }
                        // A `Hello` may reconnect this connection to a held seat and hand
                        // back a new identity, so `handle` is updated in place.
                        Ok(LobbyCommand::Hello(hello)) => {
                            handle = lobby.hello(&handle, hello.token).await;
                        }
                        // Every other command routes through the authoritative registry.
                        Ok(command) => {
                            if let Err(error) = lobby.command(&handle.token, command).await {
                                // A deck-content rejection carries a structured reason
                                // for the submitting seat alone (issue #395): send it as
                                // its own frame, out-of-band from the latest-value outbox
                                // (which would coalesce it away under the re-sent view),
                                // exactly like the one-shot catalog reply. Redaction is
                                // automatic — it rides this connection's own socket only.
                                if let Some(rejection) = lobby.deck_rejection(&error) {
                                    let frame = sage_protocol::LobbyErrorFrame {
                                        lobby_error: rejection,
                                    };
                                    match serde_json::to_string(&frame) {
                                        Ok(json) => {
                                            if write.send(Message::Text(json)).await.is_err() {
                                                break;
                                            }
                                        }
                                        Err(error) => {
                                            warn!(token = %handle.token, %error, "failed to serialize lobby error");
                                        }
                                    }
                                }
                                warn!(token = %handle.token, %error, "rejected lobby command");
                            }
                        }
                        Err(error) => {
                            warn!(token = %handle.token, %error, "ignoring undecodable lobby command");
                        }
                    }
                }
                Some(Ok(Message::Ping(payload))) => {
                    if write.send(Message::Pong(payload)).await.is_err() {
                        break;
                    }
                }
                Some(Ok(Message::Close(_))) | None => break,
                Some(Ok(_)) => {} // binary/pong/raw frames carry no protocol message
                Some(Err(error)) => {
                    warn!(token = %handle.token, %error, "websocket read error");
                    break;
                }
            },
            // Latest-value outbox: while parked on a slow `write.send`, the lobby may
            // overwrite the pending view any number of times; we serialize only the
            // newest snapshot when we loop back. Safe because each `LobbyView` is a
            // complete snapshot (`docs/protocol.md`), so superseded ones can be
            // dropped; the channel never grows under a slow reader.
            changed = outbox_rx.changed() => match changed {
                Ok(()) => {
                    let latest = outbox_rx.borrow_and_update().clone();
                    match latest {
                        Some(LobbySignal::View(view)) => match serde_json::to_string(&view) {
                            Ok(json) => {
                                if write.send(Message::Text(json)).await.is_err() {
                                    break;
                                }
                            }
                            Err(error) => {
                                warn!(token = %handle.token, %error, "failed to serialize lobby view");
                            }
                        },
                        // The ready gate passed: stop serving the lobby and hand off
                        // to the in-game contract below.
                        Some(LobbySignal::Start { seat, room }) => {
                            handoff = Some((seat, room));
                            break;
                        }
                        // Joined a running game as a spectator: hand off to the
                        // read-only spectator bridge below (issue #351).
                        Some(LobbySignal::Spectate { room }) => {
                            spectate_handoff = Some(room);
                            break;
                        }
                        None => {}
                    }
                }
                Err(_) => break,
            },
        }
    }

    if let Some((seat, room)) = handoff {
        // Reunite the split socket and switch to the in-game bridge. The session is
        // *not* disconnected: its seat is now the game's, held open for reconnect
        // (issue #113). The shutdown future carries over so the game bridge still
        // closes cleanly on server shutdown.
        match write.reunite(read) {
            Ok(ws) => serve_connection(seat, room, ws, shutdown).await,
            Err(error) => {
                warn!(token = %handle.token, %error, "failed to reunite socket for game hand-off")
            }
        }
        return;
    }

    if let Some(room) = spectate_handoff {
        // Reunite the socket and switch to the read-only spectator bridge (issue #351).
        // On exit the spectator is dropped from the lobby (it holds no seat to keep).
        match write.reunite(read) {
            Ok(ws) => serve_spectator_connection(room, ws, shutdown).await,
            Err(error) => {
                warn!(token = %handle.token, %error, "failed to reunite socket for spectator hand-off")
            }
        }
        lobby.disconnect(&handle).await;
        return;
    }

    lobby.disconnect(&handle).await;
    let _ = write.close().await;
}
