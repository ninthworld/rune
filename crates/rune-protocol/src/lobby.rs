//! The lobby message set: the pre-game analogue of the in-game two-message
//! contract (docs/decisions/0012-lobby-protocol.md). A full-state [`LobbyView`]
//! is pushed on every change and the client answers with a [`LobbyCommand`];
//! once a game is constructed the connection switches to the in-game contract.

use serde::{Deserialize, Serialize};

use crate::PlayerId;

/// Server-issued opaque session/reconnect token. The client stores it and echoes
/// it verbatim on a later [`Hello`] (after a refresh or dropped socket) to prove
/// it is the same connection and be reunited with a held-open seat. Opaque — the
/// client never parses it. This is an *identity* handle, not authentication of a
/// human (ADR 0012, Out of scope).
pub type SessionToken = String;

/// Opaque room identifier, issued by the server on [`CreateRoom`] and shared
/// out-of-band so a second player can [`JoinRoom`]. The client never parses it.
pub type RoomId = String;

/// Opaque game-setup identifier carried in a [`RoomConfig`]. It names which setup
/// (players, starting life, hand size, …) the room builds its game from. The
/// catalogue of setups and their internal shape are owned by ADR 0013; this crate
/// treats the id as an opaque value the server validates.
pub type GameSetupId = String;

/// Opaque card-identity handle used in a submitted [`SubmitDeck`] decklist. The
/// identity-vs-printing model is owned by ADR 0013 — these are card *identities*,
/// never printings or images. The server validates each against its card
/// database; the client never parses them.
///
/// Concretely, an identity is a card's authored `functional_id` (ADR 0018 §3): a
/// lowercase `snake_case` slug such as `llanowar_elves`. That is the only card identity
/// stable across builds — the engine's `CardId` is interned from the catalog's sort
/// order, so it shifts whenever a card is authored ahead of it. Clients still treat this
/// as an opaque string; the note is here so nobody reintroduces an integer.
pub type CardIdentity = String;

/// Configuration for a room, supplied by the creator in [`CreateRoom`] and echoed
/// back in every [`RoomView`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomConfig {
    /// Number of seats in the room. Validated server-side into the inclusive
    /// range `2..=8`; the lobby supports 2–8 seats even while the engine remains
    /// two-player (ADR 0012).
    pub seats: u8,
    /// Which game setup the room will build its game from (opaque; see
    /// [`GameSetupId`]).
    pub game_setup: GameSetupId,
}

/// One seat in a room's roster, as seen by any connection. Hidden information
/// stays redacted: a seat's decklist contents are never exposed, only the fact
/// that the seat is decked.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeatView {
    /// Zero-based seat index within the room.
    pub seat: u8,
    /// The player occupying this seat, or `None` if it is empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occupied_by: Option<PlayerId>,
    /// The occupant's chosen human-readable display name (issue #294), if they set
    /// one. Public, display-only information — the seat's identity remains its
    /// [`occupied_by`](SeatView::occupied_by) [`PlayerId`]. `None`/omitted for an
    /// empty seat or an occupant who has not named themselves, in which case a client
    /// falls back to a seat-derived label (e.g. `"Player 2"`), so an older server that
    /// never sends names keeps working.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Whether this seat has submitted a server-validated deck.
    #[serde(default, skip_serializing_if = "crate::is_false")]
    pub decked: bool,
    /// Whether this seat has declared itself ready.
    #[serde(default, skip_serializing_if = "crate::is_false")]
    pub ready: bool,
    /// When this seat is filled by an **AI opponent** (issue #415), the id of the AI
    /// kind occupying it — e.g. `"random"`. `None`/omitted for an empty seat or a
    /// human occupant; a human seat is identified by [`occupied_by`](SeatView::occupied_by)
    /// instead. An AI seat carries no [`occupied_by`](SeatView::occupied_by) (it is not
    /// a session) and always reports `decked`/`ready` as `true` — its deck was chosen by
    /// the host when it was seated and it is ready by construction. A free-form string
    /// like the other lobby id fields, so a newer AI kind never breaks an older client;
    /// the client renders the kind's advertised label from the [`CatalogView`]'s
    /// [`AiOption`](crate::AiOption) list and needs to parse nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai: Option<String>,
}

/// The room a connection is currently in, with its config and full seat roster.
/// Absent from a [`LobbyView`] when the connection is not in a room.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomView {
    /// The room's opaque id, shared to invite a second player.
    pub room_id: RoomId,
    /// The room's configuration.
    pub config: RoomConfig,
    /// Every seat in the room, in seat order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub seats: Vec<SeatView>,
}

/// The lifecycle state of a room in the lobby's [`directory`](LobbyView::directory)
/// (issue #280). A room appears in the directory while it is one of these two states;
/// a finished or emptied room simply leaves the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoomState {
    /// Pre-game: the room is still filling seats, taking decks, and readying up. A
    /// `gathering` room with an open seat can be joined straight from the directory.
    Gathering,
    /// The room's game has started. Its seats are no longer joinable, but it can be
    /// **spectated**: an observer joins with [`SpectateRoom`] and watches live with
    /// full redaction (ADR 0022, issue #351). The directory advertises its spectator
    /// count in [`RoomSummary::spectators`].
    InProgress,
}

/// One room as it appears in the lobby's public **room directory** (issue #280):
/// exactly enough to browse and join an open game without an out-of-band id, and no
/// more. It carries no seat roster and no player-identifying information beyond the
/// occupancy count, and never any game state — a room browser, not a spectator feed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomSummary {
    /// The room's opaque id — the same id a [`JoinRoom`] command carries, so a client
    /// can join directly from the listing.
    pub room_id: RoomId,
    /// The room's configuration (seat count and game setup): the config summary the
    /// browser renders.
    pub config: RoomConfig,
    /// How many of the room's seats are currently occupied. The total is
    /// [`RoomConfig::seats`]; a [`RoomState::Gathering`] room with `filled` below that
    /// total has an open seat to join.
    pub filled: u8,
    /// How many **spectators** are currently watching the room (ADR 0022, issue #351).
    /// Spectators do not consume seats, so this is independent of [`Self::filled`]; a
    /// room may be spectated at any state, including [`RoomState::InProgress`]. Only a
    /// count is advertised — never a spectator's identity (no social layer in M5).
    /// Omitted from the wire when zero; a client treats a missing field as `0`.
    #[serde(default, skip_serializing_if = "crate::is_zero_u8")]
    pub spectators: u8,
    /// The room's lifecycle state (`gathering` or `in_progress`).
    pub state: RoomState,
}

/// The full pre-game state for one connection, pushed on every change — the
/// pre-game analogue of [`GameView`]. The client rebuilds its entire pre-game UI
/// from a single `LobbyView` (reconnect-safe by construction) and derives no
/// legality: [`valid_commands`](LobbyView::valid_commands) is the only source of
/// interactivity, exactly as `valid_actions` is in `GameView`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LobbyView {
    /// The connection's session/reconnect token. The client stores it and echoes
    /// it on a later [`Hello`]. Always present on the wire (like `GameView::you`).
    #[serde(default)]
    pub session: SessionToken,
    /// The connection's public player identity, used to match itself against a
    /// [`SeatView::occupied_by`]. Distinct from the secret [`session`](LobbyView::session)
    /// token, which is never shown as a seat occupant. Defaults to `""` for a
    /// payload that omits it.
    #[serde(default)]
    pub you: PlayerId,
    /// The connection's own chosen display name (issue #294), if it has set one via
    /// [`SetName`]. Lets the pre-game UI show the local player's name before a seat
    /// exists (and confirm an accepted name); once seated, the same name also rides in
    /// the matching [`SeatView::name`] of the roster. `None`/omitted when unset, in
    /// which case the client falls back to a default presentation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The room the connection is in, if any, with its config and seat roster.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub room: Option<RoomView>,
    /// The public **room directory** (issue #280): every browsable room in the lobby,
    /// so a player can discover and join an open game without being handed a room id
    /// out-of-band. Each entry is a [`RoomSummary`] (id, config, occupancy count,
    /// lifecycle state); no seat roster or player-identifying info rides here, and no
    /// game state. Re-projected and pushed on every room lifecycle change, exactly
    /// like the rest of the view. Omitted from the wire when empty (no rooms); a client
    /// treats a missing field as an empty list.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub directory: Vec<RoomSummary>,
    /// The lobby command kinds currently legal for this connection (e.g.
    /// `"create_room"`, `"join_room"`, `"submit_deck"`, `"ready"`, `"unready"`,
    /// `"leave"`). Free-form strings so new command kinds do not break older
    /// clients; the client renders exactly these and computes no legality.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub valid_commands: Vec<String>,
}

/// A structured, human-readable explanation of why a lobby command was rejected
/// (issue #395). It is pushed to the **rejecting connection only**, following the
/// lobby's non-fatal error pattern (ADR 0012): the seat's [`LobbyView`] is otherwise
/// unchanged, so this is ephemeral feedback the client shows and never load-bearing
/// state.
///
/// The primary case is a rejected [`SubmitDeck`]: `reason` is the server's own
/// human-readable explanation (rendered from structured deck-legality data — the
/// server invents no new prose), `code` is a stable `snake_case` class id a client
/// may branch on without parsing the reason, and `card` names the offending card by
/// its [`CardIdentity`] when the rejection is about one specific card. `card` is only
/// ever a card from the **sender's own** submitted list or commander designation —
/// never another seat's deck or any hidden state.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LobbyRejection {
    /// A stable machine code for the rejection class, e.g. `"below_minimum"`,
    /// `"above_maximum"`, `"copy_limit"`, `"missing_commander"`,
    /// `"commander_not_in_deck"`, `"commander_not_legendary_creature"`,
    /// `"out_of_identity"`, or `"unknown_card"`. A free-form string (like the other
    /// lobby id fields) so a newer server can add a class without breaking an older
    /// client, which falls back to rendering [`reason`](LobbyRejection::reason).
    pub code: String,
    /// A human-readable reason, safe to display verbatim — the same explanation the
    /// server derives from structured deck-legality data (it composes no prose beyond
    /// this). Naming a specific card, it uses the card's display name.
    pub reason: String,
    /// The offending card's [`CardIdentity`] (`functional_id`), present only when the
    /// rejection is about one specific card (a copy-limit or color-identity violation,
    /// or an illegal/absent commander designation). Always a card from the sender's
    /// own submission — never another seat's. Omitted otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card: Option<CardIdentity>,
}

/// The server→client frame that carries a [`LobbyRejection`] to the connection whose
/// command was rejected (issue #395). Its single `lobby_error` key distinguishes it
/// on the wire from every other server frame (`LobbyView`, `GameView`,
/// `SpectatorView`, `CatalogView`), which carry no such field. An older client that
/// does not recognize the frame simply ignores it and keeps its current
/// [`LobbyView`], so the feedback is additive.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LobbyErrorFrame {
    /// The structured rejection reason for the receiving connection.
    pub lobby_error: LobbyRejection,
}

/// First-contact / reconnect command. Carries a previously issued
/// [`SessionToken`] when reconnecting; omitted (`None`) on a fresh connection, in
/// which case the server issues a new identity.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    /// A previously issued session token to reclaim a held-open seat, echoed
    /// verbatim. Omitted on first contact.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<SessionToken>,
}

/// Create a new room with the given [`RoomConfig`]. The server replies with a
/// [`LobbyView`] whose [`RoomView`] carries the freshly issued room id.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateRoom {
    /// The configuration for the new room.
    pub config: RoomConfig,
}

/// Join an existing room by its id. There is no matchmaking or discovery — the id
/// must have been shared out-of-band by the room's creator.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JoinRoom {
    /// The opaque id of the room to join.
    pub room_id: RoomId,
}

/// Join an existing room as a **spectator** (ADR 0022, issue #351): a non-seated
/// observer watching the game live with all hidden information redacted. Unlike
/// [`JoinRoom`], a spectator does **not** consume a seat, so it may join a room whose
/// seats are full — including a room whose game is already **in progress**
/// ([`RoomState::InProgress`]); the spectator reconstructs the whole public board from
/// its first [`SpectatorView`]. The room advertises its spectator count in
/// [`RoomSummary::spectators`] but never a spectator's identity to the seated players.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpectateRoom {
    /// The opaque id of the room to spectate.
    pub room_id: RoomId,
}

/// Submit a decklist for this connection's seat. The list is a flat sequence of
/// [`CardIdentity`] handles (a card appearing multiple times is repeated). The
/// server validates it authoritatively against its card database and reflects
/// only *decked: yes/no* to other seats, never the contents.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmitDeck {
    /// The card identities that make up the deck, duplicates repeated.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cards: Vec<CardIdentity>,
    /// The card this seat designates as its **commander** (CR 903.3), named by its
    /// [`CardIdentity`] — additive for the commander format (issue #372). Omitted
    /// (`None`) for a non-commander deck, in which case the wire frame is
    /// byte-for-byte the pre-commander shape, so older clients and non-commander
    /// formats are unaffected. The server validates that the designation is one of
    /// the deck's cards and a legendary creature within the format's rules; the
    /// designation is never legality the client computes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commander: Option<CardIdentity>,
}

/// Fill an empty seat with an **AI opponent** (issue #415). A host-only command: the
/// server accepts it only from the seat 0 occupant, and only for a seat of the host's
/// own room that is currently empty and whose game has not started. It names the target
/// [`seat`](AddAi::seat), the [`kind`](AddAi::kind) of AI to seat (one of the ids the
/// [`CatalogView`](crate::CatalogView) advertises in [`AiOption`](crate::AiOption)), and
/// the deck the AI will play — the same flat [`CardIdentity`] list (and optional
/// [`commander`](AddAi::commander)) a human [`SubmitDeck`] carries, validated
/// authoritatively against the room's format. On success the seat shows as AI-occupied
/// ([`SeatView::ai`]) and already decked + ready; the AI plays its own seat once the game
/// starts. Deck legality is server policy — the client never computes it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddAi {
    /// The zero-based index of the seat to fill with an AI opponent.
    pub seat: u8,
    /// The AI kind to seat, one of the [`CatalogView`](crate::CatalogView)'s advertised
    /// [`AiOption::id`](crate::AiOption::id)s (e.g. `"random"`).
    pub kind: String,
    /// The card identities that make up the AI's deck, duplicates repeated — the same
    /// shape a human [`SubmitDeck::cards`] carries.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cards: Vec<CardIdentity>,
    /// The AI's designated **commander** (CR 903.3) for a commander-format room, named by
    /// its [`CardIdentity`]. Omitted (`None`) for a non-commander deck, exactly like
    /// [`SubmitDeck::commander`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commander: Option<CardIdentity>,
}

/// Remove an **AI opponent** from a seat (issue #415), emptying it again. Host-only and
/// pre-game, the counterpart of [`AddAi`]: the server accepts it only from the seat 0
/// occupant of the room, and only for a seat that is currently AI-occupied and whose game
/// has not started. On success the seat is empty and joinable again.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoveAi {
    /// The zero-based index of the AI seat to empty.
    pub seat: u8,
}

/// Declare (or retract) readiness for this connection's seat. A seat may ready
/// only once it is occupied and has a validated deck; the game is constructed the
/// instant every seat is simultaneously filled, decked, and ready.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ready {
    /// `true` to ready up, `false` to un-ready.
    pub ready: bool,
}

/// Set (or change) this connection's public display name (issue #294). The name is
/// how other players read this one — it appears in the lobby roster
/// ([`SeatView::name`]) and, once a game starts, in every in-game view
/// ([`GameView::player_names`]). The server validates it (length bounds, printable
/// characters) and rejects an invalid value with the lobby's non-fatal error
/// pattern — the current [`LobbyView`] is re-sent unchanged. The name is bound to
/// the *session*, so it survives a per-tab reconnect. It is a display label only,
/// never an identity or authentication handle.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetName {
    /// The requested display name. The server trims and validates it before storing.
    pub name: String,
}

/// Everything a client can send in the lobby phase. Serializes with a `type`
/// discriminator (`{"type":"create_room", ...}`), structurally parallel to
/// [`ClientMessage`], so the wire stays self-describing and open to future
/// commands. The server validates every command against authoritative state and
/// answers with a fresh [`LobbyView`]; an invalid command is rejected and the
/// current `LobbyView` re-sent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LobbyCommand {
    /// First contact or reconnect; optionally carries a prior session token.
    Hello(Hello),
    /// Create a new room with a config.
    CreateRoom(CreateRoom),
    /// Join an existing room by id.
    JoinRoom(JoinRoom),
    /// Submit a decklist for this connection's seat.
    SubmitDeck(SubmitDeck),
    /// Fill an empty seat with an AI opponent (host only, issue #415).
    AddAi(AddAi),
    /// Remove an AI opponent from a seat (host only, issue #415).
    RemoveAi(RemoveAi),
    /// Declare or retract readiness.
    Ready(Ready),
    /// Set or change this connection's public display name (issue #294).
    SetName(SetName),
    /// Join an existing room as a spectator (ADR 0022, issue #351) — no seat consumed.
    SpectateRoom(SpectateRoom),
    /// Request the public card catalog and per-format deck rules (issue #367). The
    /// server answers with a one-shot [`CatalogView`] and changes no lobby state, so a
    /// connection can browse the supported card pool and format rules without joining
    /// or starting a game. Serializes as the bare tag `{"type":"request_catalog"}`.
    RequestCatalog,
    /// Leave the current room (vacating the seat, or ending a spectator session).
    Leave,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)] // panics are the failure signal in tests
mod tests {
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
            cards: vec!["ci_jedit".into(), "ci_forest".into()],
            commander: Some("ci_jedit".into()),
        });
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "type": "submit_deck",
                "cards": ["ci_jedit", "ci_forest"],
                "commander": "ci_jedit"
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
            commander: Some("ci_jedit".into()),
        });
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "type": "add_ai",
                "seat": 2,
                "kind": "random",
                "cards": ["ci_bear", "ci_forest"],
                "commander": "ci_jedit"
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
}
