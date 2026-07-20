//! Shared test harness for the lobby submodules (issue #409): the [`Client`] helper
//! that drives a registered session through its outbox, plus the room/deck/seating
//! fixtures every submodule's `#[cfg(test)]` block builds on. `pub(crate)` so each
//! sibling test module can import it; compiled only under `cfg(test)`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

pub(crate) fn lobby(max_rooms: usize) -> Lobby {
    Lobby::bundled(max_rooms).expect("bundled cards")
}

pub(crate) fn config(seats: u8) -> RoomConfig {
    config_with(seats, "standard_2p")
}

/// A room config for a specific `game_setup` format — used by the deck-legality
/// tests, which need the strict `starter-1v1` rules (the default `standard_2p`
/// imposes none).
pub(crate) fn config_with(seats: u8, game_setup: &str) -> RoomConfig {
    RoomConfig {
        seats,
        game_setup: game_setup.to_string(),
    }
}

/// A legal 40-card decklist for the seeded formats, expressed as wire card
/// identities (the server interprets each as a decimal [`CardId`]): four copies
/// each of the five non-basics (ids 1,2,3,4,6) plus twenty basic Forests (id 5),
/// which are exempt from the copy limit.
pub(crate) fn decklist() -> Vec<String> {
    let mut cards = Vec::new();
    for slug in NON_BASICS {
        for _ in 0..4 {
            cards.push(wire_id(slug));
        }
    }
    for _ in 0..20 {
        cards.push(wire_id("forest"));
    }
    cards
}

/// The five non-basic cards these deck tests build with.
pub(crate) const NON_BASICS: [&str; 5] = [
    "onakke_ogre",
    "snapping_drake",
    "fire_elemental",
    "giant_spider",
    "walking_corpse",
];

/// A card as `SubmitDeck` carries it: its authored `functional_id` (ADR 0018 §3).
pub(crate) fn wire_id(slug: &str) -> String {
    slug.to_string()
}

/// Submit a valid deck for `client`. `command` pushes synchronously, so the
/// client's [`current`](Client::current) view reflects it once this returns.
pub(crate) async fn submit_valid_deck(lobby: &Lobby, client: &Client) {
    lobby
        .command(
            &client.token,
            LobbyCommand::SubmitDeck(SubmitDeck {
                cards: decklist(),
                commander: None,
            }),
        )
        .await
        .expect("valid deck accepted");
}

/// A test client: a registered session plus its outbox receiver. Holds the
/// connection `generation` too so it can build a [`SessionHandle`] for disconnect.
pub(crate) struct Client {
    pub(crate) token: SessionToken,
    generation: u64,
    rx: watch::Receiver<Option<LobbySignal>>,
}

impl Client {
    pub(crate) async fn connect(lobby: &Lobby) -> Self {
        let (tx, rx) = watch::channel(None);
        let handle = lobby.connect(tx).await.expect("mint a session token");
        Self {
            token: handle.token,
            generation: handle.generation,
            rx,
        }
    }

    /// Simulate a returning connection that echoes `echoed` on `Hello`: a brand
    /// new socket (fresh outbox + identity) that then reconnects. The resulting
    /// client carries whatever identity the reconnect resolved to, and its
    /// receiver holds the resynced view.
    pub(crate) async fn reconnect(lobby: &Lobby, echoed: Option<SessionToken>) -> Self {
        let (tx, rx) = watch::channel(None);
        let fresh = lobby.connect(tx).await.expect("mint a session token");
        let adopted = lobby.hello(&fresh, echoed).await;
        Self {
            token: adopted.token,
            generation: adopted.generation,
            rx,
        }
    }

    /// The handle a real connection would present on disconnect.
    pub(crate) fn handle(&self) -> SessionHandle {
        SessionHandle {
            token: self.token.clone(),
            generation: self.generation,
        }
    }

    /// The latest signal pushed to this client (awaiting the next change).
    pub(crate) async fn signal(&mut self) -> LobbySignal {
        self.rx.changed().await.expect("a signal was pushed");
        self.rx
            .borrow_and_update()
            .clone()
            .expect("pushed signal is never the initial empty slot")
    }

    /// The latest pre-game view pushed to this client (awaiting the next change).
    pub(crate) async fn view(&mut self) -> LobbyView {
        match self.signal().await {
            LobbySignal::View(view) => view,
            LobbySignal::Start { .. } | LobbySignal::Spectate { .. } => {
                panic!("expected a lobby view, got a hand-off")
            }
        }
    }

    /// The current view without waiting for a further change.
    pub(crate) fn current(&self) -> LobbyView {
        match self.rx.borrow().clone().expect("a signal is present") {
            LobbySignal::View(view) => view,
            LobbySignal::Start { .. } | LobbySignal::Spectate { .. } => {
                panic!("expected a lobby view, got a hand-off")
            }
        }
    }

    /// Whether a game-start hand-off has been pushed to this client.
    pub(crate) fn started(&self) -> bool {
        matches!(*self.rx.borrow(), Some(LobbySignal::Start { .. }))
    }

    /// The seat carried by a pushed game-start hand-off, if any.
    pub(crate) fn start_seat(&self) -> Option<Seat> {
        match &*self.rx.borrow() {
            Some(LobbySignal::Start { seat, .. }) => Some(*seat),
            _ => None,
        }
    }

    /// The room handle carried by a pushed game-start hand-off, if any — the
    /// grip a determinism test uses to join the constructed game and read its
    /// first `GameView`.
    pub(crate) fn start_handle(&self) -> Option<RoomHandle> {
        match &*self.rx.borrow() {
            Some(LobbySignal::Start { room, .. }) => Some(room.clone()),
            _ => None,
        }
    }

    /// The room handle carried by a pushed **spectate** hand-off, if any (issue
    /// #351) — the grip a spectator test uses to join the running game as an
    /// observer and read its first `SpectatorView`.
    pub(crate) fn spectate_handle(&self) -> Option<RoomHandle> {
        match &*self.rx.borrow() {
            Some(LobbySignal::Spectate { room }) => Some(room.clone()),
            _ => None,
        }
    }
}

/// Drive two seats to a started game in a lobby pinned to the given overrides,
/// then join seat 0 and return its first `GameView` — enough to assert the
/// shuffle is (or is not) reproducible and the starting-life override applied,
/// without reimplementing the engine.
pub(crate) async fn first_game_view_for(
    seed_override: Option<u64>,
    life_override: Option<i32>,
) -> rune_protocol::GameView {
    let lobby =
        Lobby::bundled_with_overrides(4, seed_override, life_override).expect("bundled cards");
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

    let handle = alice.start_handle().expect("game constructed");
    let (tx, mut rx) = watch::channel::<Option<rune_protocol::GameView>>(None);
    assert!(handle.send(crate::RoomInput::Join {
        seat: 0,
        outbox: tx
    }));
    // Await the seat's first personalized GameView.
    loop {
        if let Some(view) = rx.borrow_and_update().clone() {
            return view;
        }
        rx.changed().await.expect("first GameView is pushed");
    }
}

/// Seat 0's opening-hand card names for a pinned seed (no life override).
pub(crate) async fn opening_hand_names_for_seed(seed_override: Option<u64>) -> Vec<String> {
    first_game_view_for(seed_override, None)
        .await
        .my_hand
        .into_iter()
        .map(|card| card.name)
        .collect()
}

/// Seat a fresh two-seat room with `alice` (creator, seat 0) and `bob` (seat 1),
/// draining the roster pushes so each client's next `view()` is the seat's own.
pub(crate) async fn seated_pair(lobby: &Lobby) -> (Client, Client, RoomId) {
    seated_pair_in(lobby, "standard_2p").await
}

/// Like [`seated_pair`], but opens the room under a named `game_setup` format —
/// the deck-legality tests use `starter-1v1` so its size/copy rules apply.
pub(crate) async fn seated_pair_in(lobby: &Lobby, game_setup: &str) -> (Client, Client, RoomId) {
    let mut alice = Client::connect(lobby).await;
    let _ = alice.view().await;
    lobby
        .command(
            &alice.token,
            LobbyCommand::CreateRoom(CreateRoom {
                config: config_with(2, game_setup),
            }),
        )
        .await
        .expect("alice creates");
    let room_id = alice.view().await.room.expect("alice in room").room_id;

    let mut bob = Client::connect(lobby).await;
    let _ = bob.view().await;
    lobby
        .command(
            &bob.token,
            LobbyCommand::JoinRoom(JoinRoom {
                room_id: room_id.clone(),
            }),
        )
        .await
        .expect("bob joins");
    let _ = bob.view().await;
    let _ = alice.view().await; // roster-updated push from bob's join
    (alice, bob, room_id)
}
