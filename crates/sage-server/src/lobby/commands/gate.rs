//! The pre-game gate: submitting a deck, seating an AI, readying up, and the moment the
//! last seat readies and the game is constructed (issue #112).

use crate::lobby::*;

#[cfg(test)]
mod tests;

impl Lobby {
    /// Handle `submit_deck`: resolve every card identity against the database, then
    /// validate the whole decklist against the room's **format** and,
    /// on success, store the seat's deck (leaving it decked) and re-notify the room.
    ///
    /// Validation is authoritative and all-or-nothing, in two stages: first the first
    /// identity that does not resolve rejects the whole command with
    /// [`LobbyError::UnknownCard`] — unknown ids give a typed error and the seat stays
    /// undecked; then the resolved deck is checked against the format's deck-legality
    /// rules — size and per-card copy limit — and an illegal deck is rejected with a
    /// structured [`LobbyError::IllegalDeck`] naming the violation. On
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
        // before storing it, so an illegal deck never seats a broken game. The
        // format is guaranteed present: `create_room` rejected any unknown
        // `game_setup` id.
        if let Some(format) = self.inner.formats.get(&room.config.game_setup) {
            format
                .validate_deck(&deck, commander, &self.inner.db)
                .map_err(LobbyError::IllegalDeck)?;
        }
        if let Some(gate) = room.gate.get_mut(seat) {
            // What this seat now shows the table, derived here because this is where a deck
            // is accepted and where the card database is in hand.
            gate.shown = shown_deck(&self.inner.db, &deck, commander);
            gate.deck = Some(deck);
            gate.commander = commander;
            gate.ready = false;
        }
        push_room(registry, &room_id);
        info!(%token, %room_id, seat, "seat submitted a valid deck");
        Ok(())
    }

    /// Handle `add_ai` (issue #415): the **host** fills an empty seat with an AI
    /// opponent of `kind` playing the given deck.
    ///
    /// Host-only and pre-game, with the same authoritative validation a human seat gets:
    /// the sender must occupy seat 0 of the room ([`LobbyError::NotHost`]); the game must
    /// not have started ([`LobbyError::GameStarted`]); the target `seat` must be in range
    /// ([`LobbyError::SeatIndexOutOfRange`]) and currently empty — no human and no existing
    /// AI ([`LobbyError::SeatOccupied`]); the `kind` must name a supported AI
    /// ([`LobbyError::UnknownAiKind`]); and the deck must resolve and be legal for the
    /// room's format (the same [`LobbyError::UnknownCard`]/[`LobbyError::IllegalDeck`] a
    /// `submit_deck` yields). On success the seat is AI-occupied and stored decked + ready
    /// in its gate, so the ready gate treats it exactly like a filled, ready human seat.
    pub(crate) fn add_ai(
        &self,
        registry: &mut Registry,
        token: &SessionToken,
        seat: u8,
        kind: &str,
        cards: &[String],
        commander: Option<&str>,
    ) -> Result<(), LobbyError> {
        let (room_id, host_seat) = seat_of(registry, token)?;
        // Only the host (seat 0 occupant) manages AI seats.
        if host_seat != 0 {
            return Err(LobbyError::NotHost);
        }
        // Resolve the kind before touching the room, so an unknown kind changes nothing.
        let kind =
            AiKind::from_id(kind).ok_or_else(|| LobbyError::UnknownAiKind(kind.to_string()))?;
        // Resolve and validate the AI's deck exactly as a human `submit_deck` does, before
        // mutating, so a bad card or illegal deck leaves the seat untouched.
        let mut deck = Vec::with_capacity(cards.len());
        for identity in cards {
            let card = resolve_card(&self.inner.db, identity)
                .ok_or_else(|| LobbyError::UnknownCard(identity.clone()))?;
            deck.push(card);
        }
        let commander = match commander {
            Some(identity) => Some(
                resolve_card(&self.inner.db, identity)
                    .ok_or_else(|| LobbyError::UnknownCard(identity.to_string()))?,
            ),
            None => None,
        };

        let room = registry
            .rooms
            .get_mut(&room_id)
            .ok_or(LobbyError::NotSeated)?;
        if room.game.is_some() {
            return Err(LobbyError::GameStarted);
        }
        let index = seat as usize;
        if index >= room.seats.len() {
            return Err(LobbyError::SeatIndexOutOfRange(seat));
        }
        // The target seat must be empty: no human occupant and no existing AI.
        if room.seats[index].is_some() || room.ai_seats[index].is_some() {
            return Err(LobbyError::SeatOccupied(seat));
        }
        if let Some(format) = self.inner.formats.get(&room.config.game_setup) {
            format
                .validate_deck(&deck, commander, &self.inner.db)
                .map_err(LobbyError::IllegalDeck)?;
        }

        room.ai_seats[index] = Some(AiSeat {
            kind,
            name: kind.name().to_string(),
        });
        // Store the AI's deck in the gate, decked + ready, so the ready gate and game
        // construction read it uniformly with human seats.
        if let Some(gate) = room.gate.get_mut(index) {
            // A bot shows the table what it brought exactly as a human seat does.
            gate.shown = shown_deck(&self.inner.db, &deck, commander);
            gate.deck = Some(deck);
            gate.commander = commander;
            gate.ready = true;
        }
        // Everyone browsing sees the new occupancy count; the room's occupants see the
        // AI seat in their roster.
        broadcast_views(registry);
        // Adding the AI may complete the gate (it fills, decks, and readies a seat in one
        // step), so attempt to start the game just like a human readying up does.
        self.start_game(registry, &room_id);
        info!(%token, %room_id, seat, kind = kind.id(), "host seated an AI opponent");
        Ok(())
    }

    /// Handle `ready`: toggle the seat's ready flag, then — when readying up completes
    /// the gate — construct the game and hand every seat off to the in-game contract.
    ///
    /// Readying up requires a submitted deck ([`LobbyError::NotDecked`] otherwise);
    /// un-readying (`ready == false`) is always allowed for a seated player before the
    /// game starts. When the last seat readies and every seat is filled, decked, and
    /// ready, [`start_game`](Lobby::start_game) builds the `GameState` and switches the
    /// room to the game phase.
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
    /// On the gate passing, builds the room format's engine [`GameSetup`] from the
    /// seats' submitted decks in seat order with a server-generated seed,
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
        // Gate: every seat filled (by a human or an AI, issue #415), decked, and ready.
        // An AI seat is stored decked + ready in its gate at `add_ai` time, so it satisfies
        // the same gate a human does.
        let ready_to_start =
            room.seats
                .iter()
                .zip(&room.ai_seats)
                .zip(&room.gate)
                .all(|((session, ai), gate)| {
                    (session.is_some() || ai.is_some()) && gate.deck.is_some() && gate.ready
                });
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
        // suite, ADR 0006 / issue #145) if configured, else a fresh per-game seed.
        let seed = self.inner.seed_override.unwrap_or_else(generate_seed);
        // The format supplies the engine `GameSetup` parameters; it is
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
        // label players in every `GameView::player_names`. A human seat with no name is
        // `None`; an AI seat (issue #415) contributes its kind's label so an opponent reads
        // "Random" rather than a bare seat label.
        let player_names: Vec<Option<String>> = room
            .seats
            .iter()
            .zip(&room.ai_seats)
            .map(|(session, ai)| {
                session
                    .as_ref()
                    .and_then(|token| registry.sessions.get(token))
                    .and_then(|session| session.name.clone())
                    .or_else(|| ai.as_ref().map(|ai| ai.name.clone()))
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
        // In-match presentation metadata (issue #553), both room/lobby knowledge the
        // engine has no notion of: which seats are AI-controlled, and the format this
        // game is played under. The `commander` flag comes from the registered format's
        // own `requires_commander` deck rule — the single source of truth (issue #394)
        // — so a client never string-matches the format id to decide it is Commander.
        let ai_seats: Vec<bool> = room.ai_seats.iter().map(Option::is_some).collect();
        let match_format = MatchFormat {
            id: room.config.game_setup.clone(),
            commander: self
                .inner
                .formats
                .get(&room.config.game_setup)
                .is_some_and(|format| format.deck_rules.require_commander),
        };
        let (handle, _task) = Room::new(state, db)
            .with_player_names(player_names)
            .with_ai_seats(ai_seats)
            .with_format(match_format)
            .with_auto_pass(AutoPassPolicy::On)
            // Human seats start stopped at their own main phases (issue #455). ADR
            // 0020's automation is what makes a spell-less turn cheap; the default
            // stop is what keeps it from being *invisible* — without it a human whose
            // turn holds nothing castable watches the settle run both of their main
            // phases, and the whole turn, between two broadcasts. AI seats are seeded
            // with nothing, so an AI-only or mixed game keeps its throughput, and the
            // first `set_stops` a player sends replaces the seed for good.
            .with_stop_policy(StopPolicy::HumanMainPhases)
            .spawn();

        // Hand every seated *human* session off to the in-game contract.
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
        // Spawn an in-process driver for every **AI seat** (issue #415): the server-side
        // sibling of a human's `serve_connection`. Each AI plays its own seat from a policy
        // seeded off the game seed (so a pinned seed replays the AI identically, issue
        // #145), joining the room and reacting to its `GameView`s until the game ends.
        let ai_occupants: Vec<(Seat, AiKind)> = room
            .ai_seats
            .iter()
            .enumerate()
            .filter_map(|(seat, ai)| ai.as_ref().map(|ai| (seat, ai.kind)))
            .collect();
        for (seat, kind) in ai_occupants {
            // Distinct per-seat sub-seed so two AI seats in one game do not draw the same
            // stream, while the whole game stays reproducible under a pinned seed.
            let ai_seed = seed
                ^ (seat as u64)
                    .wrapping_add(1)
                    .wrapping_mul(0x9E37_79B9_7F4A_7C15);
            let policy = policy_for(kind, ai_seed);
            tokio::spawn(serve_ai_seat(
                seat,
                handle.clone(),
                policy,
                std::future::pending::<()>(),
            ));
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

/// What a seat shows the table about the deck it just submitted: the colours it is in,
/// and the commander it designated by name.
///
/// **A summary, computed once, from a deck that has already been validated.** The colours
/// are the union of the cards' colour identities — the same reading
/// [`color_identity_of`](crate::format::color_identity_of) gives one card, in the same
/// WUBRG order a `CardView` carries — and the commander is its stable `functional_id`,
/// which is what a client addresses a card by. Neither says which cards are in the deck,
/// and neither is taken from the client: both are read off the resolved list here, so a
/// seat cannot claim colours it is not playing.
///
/// Derived at the gate rather than in [`views`](crate::lobby::views) because the view
/// builder holds the registry and no card database, and because a summary of a deck that
/// only changes when the deck does has no business being recomputed on every broadcast.
fn shown_deck(db: &CardDatabase, deck: &[CardId], commander: Option<CardId>) -> SeatShown {
    let mut colors = std::collections::HashSet::new();
    for card in deck {
        if let Some(data) = db.card(*card) {
            colors.extend(crate::format::color_identity_of(db, data));
        }
    }
    SeatShown {
        colors: crate::view::colors_in_wubrg(&colors),
        commander: commander
            .and_then(|card| db.card(card))
            .map(|data| data.functional_id.to_string()),
    }
}
