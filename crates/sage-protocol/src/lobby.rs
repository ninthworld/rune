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
/// human — authentication is out of scope.
pub type SessionToken = String;

/// Opaque room identifier, issued by the server on [`CreateRoom`] and shared
/// out-of-band so a second player can [`JoinRoom`]. The client never parses it.
pub type RoomId = String;

/// Opaque game-setup identifier carried in a [`RoomConfig`]. It names which setup
/// (players, starting life, hand size, …) the room builds its game from. The
/// catalogue of setups and their internal shape are the server's; this crate treats
/// the id as an opaque value the server validates.
pub type GameSetupId = String;

/// Opaque card-identity handle used in a submitted [`SubmitDeck`] decklist. The
/// identity-vs-printing model is owned by ADR 0008 — these are card *identities*,
/// never printings or images. The server validates each against its card
/// database; the client never parses them.
///
/// Concretely, an identity is a card's authored `functional_id` (ADR 0008 §3): a
/// lowercase `snake_case` slug such as `llanowar_elves`. That is the only card identity
/// stable across builds — the engine's `CardId` is interned from the catalog's sort
/// order, so it shifts whenever a card is authored ahead of it. Clients still treat this
/// as an opaque string; the note is here so nobody reintroduces an integer.
pub type CardIdentity = String;

/// Whether a room is listed in the lobby's public [`directory`](LobbyView::directory)
/// (issue #546). A closed, `snake_case`-tagged enum in the shape of [`RoomState`], its
/// nearest neighbour on the wire: the value is a *vocabulary* the UI renders as a word,
/// not a flag the client would have to invent prose for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoomVisibility {
    /// Listed in the public room directory: anyone browsing the lobby can find the
    /// room and join an open seat. The default, and the behaviour of every room
    /// created before this field existed.
    #[default]
    Public,
    /// Not listed anywhere: the room is reachable only by a [`JoinRoom`] carrying its
    /// [`RoomId`], which its host shares out-of-band. It is omitted from the directory
    /// for **every** connection, so it cannot be browsed or spectated by strangers.
    Private,
}

impl RoomVisibility {
    /// Whether this is the default (`public`) visibility. The `skip_serializing_if`
    /// predicate for [`RoomConfig::visibility`]: the crate's shared predicates in
    /// `lib.rs` cover `bool`/integer fields only, so an enum-valued field carries its
    /// own, which keeps the default off the wire exactly like every other elided field.
    pub(crate) fn is_public(&self) -> bool {
        matches!(self, Self::Public)
    }
}

/// Configuration for a room, supplied by the creator in [`CreateRoom`], changed by its
/// host with [`UpdateRoom`], and echoed back in every [`RoomView`] and [`RoomSummary`].
///
/// [`name`](RoomConfig::name) and [`visibility`](RoomConfig::visibility) are additive
/// (issue #546): both elide from the wire at their defaults, so a client that omits
/// them creates exactly the room the pre-#546 shape created — a public table the UI
/// labels by its format.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomConfig {
    /// Number of seats in the room. Validated server-side into the inclusive
    /// range `2..=8`; the lobby supports 2–8 seats even while the engine remains
    /// two-player.
    pub seats: u8,
    /// Which game setup the room will build its game from (opaque; see
    /// [`GameSetupId`]).
    pub game_setup: GameSetupId,
    /// The host's chosen name for the table (issue #546) — public, display-only text,
    /// validated server-side under the same bounds a [`SetName`] display name gets
    /// (trimmed, non-empty, ≤ 32 characters, printable). `None`/omitted when the host
    /// named nothing, in which case a client labels the table by its
    /// [`game_setup`](RoomConfig::game_setup) exactly as it did before this field
    /// existed. The server never invents a name: the fallback is the client's own
    /// display concern, so no prose rides the wire.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Whether the room is listed in the public directory (issue #546). Defaults to
    /// [`RoomVisibility::Public`] and elides from the wire at that default, so an
    /// older client that omits it gets today's behaviour.
    #[serde(default, skip_serializing_if = "RoomVisibility::is_public")]
    pub visibility: RoomVisibility,
}

/// One seat in a room's roster, as seen by any connection. Hidden information
/// stays redacted: a seat's decklist contents are never exposed, only the fact
/// that the seat is decked, the colours that deck is in, and the commander it
/// designated — the two things a player shows the table before a game rather than
/// keeps to themselves.
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
    /// The colours of the deck this seat submitted (CR 903.4), in WUBRG order — the
    /// union of its cards' colour identities, computed when the deck was accepted.
    ///
    /// **This is a summary, never the list.** It says a seat is playing red and green;
    /// it says nothing about which cards, and a seat that has submitted no deck carries
    /// none. Public because the colours a player brings to a table are what everyone at
    /// it can see before the first card is drawn.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub colors: Vec<crate::Color>,
    /// The commander this seat designated (CR 903.3), as its stable `functional_id`.
    ///
    /// Public for the same reason the physical card is: a commander begins the game in
    /// the command zone, face up, where every player can read it. `None`/omitted for a
    /// seat with no commander, which is every seat in a format that wants none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commander: Option<CardIdentity>,
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
    /// full redaction (issue #351). The directory advertises its spectator
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
    /// The room's configuration: the config summary the browser renders. It is the
    /// **whole** [`RoomConfig`], so the host's chosen [`name`](RoomConfig::name) reaches
    /// the directory through the field it already carried (issue #546) rather than
    /// through a second, divergent copy. A [`RoomVisibility::Private`] room never
    /// appears here at all, so the visibility a listed entry reports is always
    /// [`Public`](RoomVisibility::Public).
    pub config: RoomConfig,
    /// How many of the room's seats are currently occupied. The total is
    /// [`RoomConfig::seats`]; a [`RoomState::Gathering`] room with `filled` below that
    /// total has an open seat to join.
    pub filled: u8,
    /// How many **spectators** are currently watching the room (issue #351).
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
    /// `"create_room"`, `"update_room"`, `"join_room"`, `"submit_deck"`, `"ready"`,
    /// `"unready"`, `"leave"`). Free-form strings so new command kinds do not break
    /// older clients; the client renders exactly these and computes no legality — a
    /// host-only affordance such as Edit Table exists because `"update_room"` is
    /// advertised here, never because a client decided it was the host.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub valid_commands: Vec<String>,
}

/// A structured, human-readable explanation of why a lobby command was rejected
/// (issue #395). It is pushed to the **rejecting connection only**, following the
/// lobby's non-fatal error pattern: the seat's [`LobbyView`] is otherwise
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

/// Change the configuration of the room this connection **hosts** (issue #546).
///
/// The counterpart of [`CreateRoom`], and deliberately the same shape: it carries a
/// **whole** [`RoomConfig`], not a patch of changed fields, so the command says what the
/// table *is* rather than what moved — the same full-state discipline [`LobbyView`]
/// follows, and the reason one client surface can serve both creating and editing.
///
/// The server accepts it only from the room's host (the seat 0 occupant,
/// like [`AddAi`]) and only before the game starts, validates the new config exactly as
/// [`CreateRoom`] does, and additionally refuses a seat count that would remove an
/// occupied seat — shrinking is rejected, never silently clamped. On acceptance every
/// seat's readiness is cleared when the seats or the format changed (nobody stays ready
/// to a table they did not agree to), and a changed
/// [`game_setup`](RoomConfig::game_setup) additionally clears every submitted deck,
/// since those decks were validated against a format that no longer applies.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateRoom {
    /// The room's complete new configuration.
    pub config: RoomConfig,
}

/// Join an existing room by its id. There is no matchmaking or discovery — the id
/// must have been shared out-of-band by the room's creator.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JoinRoom {
    /// The opaque id of the room to join.
    pub room_id: RoomId,
}

/// Join an existing room as a **spectator** (issue #351): a non-seated
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
    /// Change the configuration of the room this connection hosts (issue #546).
    UpdateRoom(UpdateRoom),
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
    /// Join an existing room as a spectator (issue #351) — no seat consumed.
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
mod tests;
