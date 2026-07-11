//! RUNE protocol — the entire client/server contract.
//!
//! Two *in-game* message types (docs/protocol.md):
//! - Server -> client: a personalized [`GameView`]
//! - Client -> server: a [`ClientMessage`] (only variant: [`ChooseAction`])
//!
//! These are flanked by a small **lobby** message set that governs the pre-game
//! phase and hands off to the in-game contract once a game is constructed
//! (docs/decisions/0012-lobby-protocol.md):
//! - Server -> client: a [`LobbyView`] (full pre-game state, `GameView`-style)
//! - Client -> server: a [`LobbyCommand`] (`hello`, `create_room`, `join_room`,
//!   `submit_deck`, `ready`, `leave`)
//!
//! Everything here serializes to the JSON documented in `docs/protocol.md`. Any
//! change to these shapes must update that document in the same PR. Clients and
//! server tolerate unknown fields (serde ignores them) so the wire format can
//! grow without breaking older clients — see the forward-compat test below.

use serde::{Deserialize, Serialize};

/// Opaque player identity (server-assigned).
pub type PlayerId = String;

/// Opaque per-game entity id: a card, permanent, or stack object.
pub type EntityId = String;

/// A card object, shown only to a player entitled to see it (`my_hand`, public
/// zones, revealed cards). Characteristics are server-computed; the client never
/// derives them. Grows alongside the card database (backlog: engine card loader).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardView {
    /// Entity id of this card instance.
    pub id: EntityId,
    /// Display name.
    pub name: String,
    /// e.g. `"Creature — Elf Warrior"`.
    pub type_line: String,
    /// Displayed mana cost string, e.g. `"{1}{G}"`. `None` for cards without one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mana_cost: Option<String>,
    /// Rules text as displayed.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub oracle_text: String,
    /// Displayed power (a string so `*` and other non-numeric values round-trip).
    /// Present only for creatures.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power: Option<String>,
    /// Displayed toughness; see [`CardView::power`]. Present only for creatures.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toughness: Option<String>,
}

/// What the receiving player is allowed to know about an opponent: hidden zones
/// are reduced to counts, public state is exact.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpponentView {
    /// Which opponent this describes.
    pub player_id: PlayerId,
    /// Number of cards in hand (contents hidden).
    pub hand_size: u32,
    /// Current life total.
    pub life: i32,
    /// Number of cards left in library.
    pub library_size: u32,
    /// Number of cards in the graveyard.
    pub graveyard_size: u32,
    /// Free-form status labels (e.g. `"monarch"`, `"hexproof"`) for display only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub statuses: Vec<String>,
}

/// A permanent on the battlefield with its server-computed characteristics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Permanent {
    /// Entity id of this permanent.
    pub id: EntityId,
    /// Player who currently controls it.
    pub controller: PlayerId,
    /// Player who owns it (matters when control changes).
    pub owner: PlayerId,
    /// The permanent's current (computed) card face.
    pub card: CardView,
    /// Whether the permanent is tapped.
    #[serde(default, skip_serializing_if = "is_false")]
    pub tapped: bool,
    /// Whether this permanent is currently attacking — declared as an attacker
    /// this combat (CR 508). Server-computed; the client displays it and never
    /// derives it. Omitted from the wire when `false`.
    #[serde(default, skip_serializing_if = "is_false")]
    pub attacking: bool,
    /// The permanent this one is blocking, if it was declared as a blocker this
    /// combat (CR 509): the attacker's entity id. `None`/omitted when it is not
    /// blocking. Several blockers may name the same attacker.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocking: Option<EntityId>,
    /// Named counters and their quantities, e.g. `{"+1/+1": 2}`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub counters: Vec<Counter>,
}

/// A named counter on a permanent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counter {
    /// Counter name, e.g. `"+1/+1"` or `"loyalty"`.
    pub kind: String,
    /// How many of this counter are present.
    pub count: u32,
}

/// One object on the stack — a spell or an ability. Ability entries carry their
/// source permanent so the client can point back at it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StackItem {
    /// Entity id of this stack object.
    pub id: EntityId,
    /// Player who controls it (chooses targets/resolution).
    pub controller: PlayerId,
    /// Spell name or ability text as it should be displayed.
    pub description: String,
    /// Source permanent for an ability; `None` for a spell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<EntityId>,
}

/// A public, ordered pile owned by one player (graveyard or exile).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZonePile {
    /// Player who owns the pile.
    pub player_id: PlayerId,
    /// Cards in zone order (top last).
    pub cards: Vec<CardView>,
}

/// The current turn step. The full sequence lives in the engine's phase FSM
/// (backlog); the protocol carries the current step for overview/focus rendering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Untap step.
    Untap,
    /// Upkeep step.
    Upkeep,
    /// Draw step.
    Draw,
    /// Precombat main phase.
    PrecombatMain,
    /// Beginning of combat step.
    BeginCombat,
    /// Declare attackers step.
    DeclareAttackers,
    /// Declare blockers step.
    DeclareBlockers,
    /// Combat damage step.
    CombatDamage,
    /// End of combat step.
    EndCombat,
    /// Postcombat main phase.
    PostcombatMain,
    /// End step.
    End,
    /// Cleanup step.
    Cleanup,
}

/// One entry of [`GameView::valid_actions`]. The client renders these; it never
/// invents its own. `subject` names the entities this action belongs to so the
/// client can put the action ON the card rather than in a global bar
/// (docs/decisions/0004-subject-owned-actions.md).
///
/// A multi-step action (a targeted spell, and later a mode/X choice) additionally
/// carries an ordered [`requirements`](ValidAction::requirements) list the client
/// walks as a prompt queue, plus a content-binding [`token`](ValidAction::token)
/// the client echoes verbatim in [`ChooseAction`]. Both are decided in
/// docs/decisions/0009-targeting-model.md (§Protocol).
///
/// `Default` yields an empty, unbound action (no subject, no requirements, empty
/// token); it exists so callers that build an action field-by-field need not
/// restate the newer fields, not because an empty action is meaningful.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidAction {
    /// Opaque id the client echoes back in [`ChooseAction`] to take this action.
    pub id: String,
    /// Coarse action category (e.g. `"pass_priority"`, `"activate_ability"`).
    /// A free-form string, not an enum, so new action kinds do not break older
    /// clients that only key off `subject` and `label`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Human-readable label to render for this action.
    pub label: String,
    /// Entity ids this action belongs to; empty for global actions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subject: Vec<String>,
    /// Ordered choice steps this action requires before it can be taken — one per
    /// target slot (modes/X ride the same mechanism later). The client walks them
    /// as a prompt queue and answers every slot **atomically** in a single
    /// [`ChooseAction`], never a stateful multi-message handshake
    /// (docs/protocol.md, two-message philosophy). Empty for a plain action that
    /// needs no sub-choice; omitted from the wire when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirements: Vec<TargetRequirement>,
    /// Content-binding token: a server-issued value bound to this action's exact
    /// content (kind + subject + requirements). The client echoes it verbatim in
    /// [`ChooseAction::token`]; the server recomputes it from the freshly
    /// regenerated action and rejects a mismatch, so a stale positional `id` can
    /// never rebind to a *different* action. Opaque — the client never parses or
    /// derives it. Omitted only for legacy/unbound actions, where it deserializes
    /// to `""` (which no real token matches, so such an answer is safely rejected).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub token: String,
}

/// One choice step of a multi-step [`ValidAction`]: a single target slot the
/// player must fill, listing exactly the legal candidates the server computed.
/// The client renders the prompt, highlights the candidates, and computes no
/// legality of its own (docs/decisions/0009-targeting-model.md §Client).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetRequirement {
    /// Stable slot id the client echoes back as [`TargetChoice::slot`] to key its
    /// answer to this step. Opaque; the client never parses it.
    pub slot: String,
    /// Human-readable prompt describing what to choose, e.g. `"target creature"`.
    pub prompt: String,
    /// The legal candidate entity ids for this slot — the **only** choices the
    /// client may offer. Enumerated O(N) per slot, never the cartesian product of
    /// combinations across slots (docs/decisions/0009-targeting-model.md
    /// §Enumeration).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<EntityId>,
}

/// The personalized state the server sends after every change (docs/protocol.md).
/// Hidden information is redacted server-side before this is built. A client must
/// be able to fully reconstruct its UI from a single `GameView` — no client state
/// is load-bearing across messages.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GameView {
    /// The receiver's own seat entity id (the `p{N}` form used for players
    /// throughout the view). Lets a client identify itself directly instead of
    /// inferring it from which id is not an opponent. `#[serde(default)]` so a
    /// payload from an older server that omits it still deserializes (to `""`).
    #[serde(default)]
    pub you: PlayerId,
    /// Full card objects for the receiving player only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub my_hand: Vec<CardView>,
    /// Redacted views of every other player.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub opponents: Vec<OpponentView>,
    /// All permanents in play.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub battlefield: Vec<Permanent>,
    /// The stack, bottom first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stack: Vec<StackItem>,
    /// Each player's graveyard.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub graveyards: Vec<ZonePile>,
    /// Each player's exile zone.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exile: Vec<ZonePile>,
    /// The current turn step.
    pub phase: Phase,
    /// The receiving player's unspent mana, as pip strings (e.g. `["{G}", "{G}"]`).
    /// Server-computed; the client only displays it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mana_pool: Vec<String>,
    /// The player who currently holds priority, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority_player: Option<PlayerId>,
    /// The only source of interactivity: what the receiving player may do now.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub valid_actions: Vec<ValidAction>,
    /// Seconds remaining for the pending decision, if a clock is running.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_deadline: Option<f64>,
}

/// The client's chosen action, answered atomically: the `id` of one issued
/// [`ValidAction`], its content-binding [`token`](ChooseAction::token), and the
/// full set of [`targets`](ChooseAction::targets) filling that action's
/// requirement slots. The server validates the id, verifies the token against the
/// action it currently offers, and checks each chosen target against that slot's
/// freshly computed legal set; anything else is rejected and the current
/// `GameView` is re-sent (docs/decisions/0009-targeting-model.md §Protocol).
///
/// `Default` yields the minimal no-choice answer (empty token and targets), so a
/// caller answering a plain action can set only `action_id`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChooseAction {
    /// The `id` of the chosen [`ValidAction`].
    pub action_id: String,
    /// The chosen action's [`ValidAction::token`], echoed verbatim. Binds this
    /// answer to the exact action content the client saw, closing the stale-`id`
    /// rebinding hole. Omitted (`""`) only for a legacy unbound action; a real
    /// server rejects an answer whose token does not match.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub token: String,
    /// One entry per [`ValidAction::requirements`] slot, carrying the entity ids
    /// the player selected. Submitted all at once (never a multi-message
    /// handshake); empty for an action with no requirements.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<TargetChoice>,
}

/// The player's answer to one [`TargetRequirement`] slot: the selected entity
/// ids, keyed back to the slot by `slot`. Each id must be one of that slot's
/// advertised `candidates` or the server treats the action as a no-op.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetChoice {
    /// The [`TargetRequirement::slot`] this answers.
    pub slot: String,
    /// The entity ids chosen for this slot (one for a single-target slot; the
    /// list generalizes to multi-select choices the model defers for now).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chosen: Vec<EntityId>,
}

/// Everything a client can send about the game. Serializes with a `type`
/// discriminator (`{"type":"choose_action", ...}`) so the wire stays
/// self-describing and open to future message types.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// The player chose one of the issued valid actions.
    ChooseAction(ChooseAction),
}

// ---------------------------------------------------------------------------
// Lobby protocol (docs/decisions/0012-lobby-protocol.md)
//
// The pre-game analogue of the in-game two-message contract: a full-state
// `LobbyView` pushed on every change (mirroring `GameView`) and a tagged
// `LobbyCommand` the client sends to act (mirroring `ChooseAction`). The client
// reconstructs its entire pre-game UI from one `LobbyView` and computes no
// legality of its own. Once a game is constructed the connection switches to the
// in-game `GameView`/`ClientMessage` contract for the life of that game.
// ---------------------------------------------------------------------------

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
    /// Whether this seat has submitted a server-validated deck.
    #[serde(default, skip_serializing_if = "is_false")]
    pub decked: bool,
    /// Whether this seat has declared itself ready.
    #[serde(default, skip_serializing_if = "is_false")]
    pub ready: bool,
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
    /// The room the connection is in, if any, with its config and seat roster.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub room: Option<RoomView>,
    /// The lobby command kinds currently legal for this connection (e.g.
    /// `"create_room"`, `"join_room"`, `"submit_deck"`, `"ready"`, `"unready"`,
    /// `"leave"`). Free-form strings so new command kinds do not break older
    /// clients; the client renders exactly these and computes no legality.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub valid_commands: Vec<String>,
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

/// Submit a decklist for this connection's seat. The list is a flat sequence of
/// [`CardIdentity`] handles (a card appearing multiple times is repeated). The
/// server validates it authoritatively against its card database and reflects
/// only *decked: yes/no* to other seats, never the contents.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmitDeck {
    /// The card identities that make up the deck, duplicates repeated.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cards: Vec<CardIdentity>,
}

/// Declare (or retract) readiness for this connection's seat. A seat may ready
/// only once it is occupied and has a validated deck; the game is constructed the
/// instant every seat is simultaneously filled, decked, and ready.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ready {
    /// `true` to ready up, `false` to un-ready.
    pub ready: bool,
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
    /// Declare or retract readiness.
    Ready(Ready),
    /// Leave the current room (vacating the seat).
    Leave,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(b: &bool) -> bool {
    !*b
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // panics are the failure signal in tests
mod tests {
    use super::*;

    #[test]
    fn choose_action_is_just_an_id() {
        let msg = ChooseAction {
            action_id: "a2".into(),
            token: String::new(),
            targets: vec![],
        };
        assert_eq!(msg.action_id, "a2");
    }

    #[test]
    fn client_message_uses_documented_wire_shape() {
        // A no-choice action: empty token and targets elide, so the minimal
        // `{type, action_id}` wire shape is preserved for backward compatibility.
        let msg = ClientMessage::ChooseAction(ChooseAction {
            action_id: "a2".into(),
            token: String::new(),
            targets: vec![],
        });
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "type": "choose_action", "action_id": "a2" })
        );
        let back: ClientMessage = serde_json::from_value(json).unwrap();
        assert_eq!(back, msg);
    }

    #[test]
    fn choose_action_carries_token_and_targets() {
        // A real targeted answer: id + content-binding token + the atomically
        // submitted selection, keyed per requirement slot.
        let msg = ClientMessage::ChooseAction(ChooseAction {
            action_id: "a3".into(),
            token: "h:9f2c".into(),
            targets: vec![TargetChoice {
                slot: "t0".into(),
                chosen: vec!["perm_bear".into()],
            }],
        });
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "type": "choose_action",
                "action_id": "a3",
                "token": "h:9f2c",
                "targets": [{ "slot": "t0", "chosen": ["perm_bear"] }]
            })
        );
        let back: ClientMessage = serde_json::from_value(json).unwrap();
        assert_eq!(back, msg);
    }

    #[test]
    fn valid_action_serializes_type_and_omits_empty_subject() {
        let pass = ValidAction {
            id: "a1".into(),
            kind: "pass_priority".into(),
            label: "Pass".into(),
            subject: vec![],
            requirements: vec![],
            token: String::new(),
        };
        let json = serde_json::to_value(&pass).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "id": "a1", "type": "pass_priority", "label": "Pass" })
        );
    }

    #[test]
    fn valid_action_carries_requirements_and_token() {
        // A targeted spell: subject is the hand card, requirements advertise the
        // one target slot's legal candidates, and a content-binding token is
        // present for the client to echo back.
        let bolt = ValidAction {
            id: "a3".into(),
            kind: "cast_spell".into(),
            label: "Cast Lightning Bolt".into(),
            subject: vec!["c3".into()],
            requirements: vec![TargetRequirement {
                slot: "t0".into(),
                prompt: "target creature or player".into(),
                candidates: vec!["perm_bear".into(), "p1".into(), "p2".into()],
            }],
            token: "h:9f2c".into(),
        };
        let json = serde_json::to_value(&bolt).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "id": "a3",
                "type": "cast_spell",
                "label": "Cast Lightning Bolt",
                "subject": ["c3"],
                "requirements": [{
                    "slot": "t0",
                    "prompt": "target creature or player",
                    "candidates": ["perm_bear", "p1", "p2"]
                }],
                "token": "h:9f2c"
            })
        );
        let back: ValidAction = serde_json::from_value(json).unwrap();
        assert_eq!(back, bolt);
    }

    #[test]
    fn legacy_valid_action_without_token_or_requirements_deserializes() {
        // A payload from a server that predates this shape omits both new fields;
        // they must default (empty requirements, empty token) rather than fail.
        let json = r#"{ "id": "a1", "type": "pass_priority", "label": "Pass" }"#;
        let action: ValidAction = serde_json::from_str(json).unwrap();
        assert!(action.requirements.is_empty());
        assert_eq!(action.token, "");
    }

    #[test]
    fn game_view_round_trips_through_json() {
        let view = GameView {
            you: "p1".into(),
            my_hand: vec![CardView {
                id: "c1".into(),
                name: "Llanowar Elves".into(),
                type_line: "Creature — Elf Druid".into(),
                mana_cost: Some("{G}".into()),
                oracle_text: "{T}: Add {G}.".into(),
                power: Some("1".into()),
                toughness: Some("1".into()),
            }],
            opponents: vec![OpponentView {
                player_id: "p2".into(),
                hand_size: 7,
                life: 20,
                library_size: 53,
                graveyard_size: 0,
                statuses: vec!["monarch".into()],
            }],
            battlefield: vec![Permanent {
                id: "perm_xyz".into(),
                controller: "p1".into(),
                owner: "p1".into(),
                card: CardView {
                    id: "perm_xyz".into(),
                    name: "Grizzly Bears".into(),
                    type_line: "Creature — Bear".into(),
                    mana_cost: Some("{1}{G}".into()),
                    oracle_text: String::new(),
                    power: Some("2".into()),
                    toughness: Some("2".into()),
                },
                tapped: true,
                attacking: false,
                blocking: None,
                counters: vec![Counter {
                    kind: "+1/+1".into(),
                    count: 2,
                }],
            }],
            stack: vec![StackItem {
                id: "s1".into(),
                controller: "p2".into(),
                description: "Lightning Bolt".into(),
                source: None,
            }],
            graveyards: vec![ZonePile {
                player_id: "p1".into(),
                cards: vec![],
            }],
            exile: vec![],
            phase: Phase::PrecombatMain,
            mana_pool: vec!["{G}".into()],
            priority_player: Some("p1".into()),
            valid_actions: vec![ValidAction {
                id: "a2".into(),
                kind: "activate_ability".into(),
                label: "Tap for mana".into(),
                subject: vec!["perm_xyz".into()],
                requirements: vec![],
                token: "h:00ab".into(),
            }],
            action_deadline: Some(12.5),
        };

        let json = serde_json::to_string(&view).unwrap();
        let back: GameView = serde_json::from_str(&json).unwrap();
        assert_eq!(back, view);
    }

    #[test]
    fn empty_game_view_round_trips() {
        let view = GameView {
            you: "p0".into(),
            my_hand: vec![],
            opponents: vec![],
            battlefield: vec![],
            stack: vec![],
            graveyards: vec![],
            exile: vec![],
            phase: Phase::Upkeep,
            mana_pool: vec![],
            priority_player: None,
            valid_actions: vec![],
            action_deadline: None,
        };
        let json = serde_json::to_string(&view).unwrap();
        let back: GameView = serde_json::from_str(&json).unwrap();
        assert_eq!(back, view);
    }

    #[test]
    fn mana_pool_is_omitted_when_empty_and_round_trips_when_present() {
        let mut view = GameView {
            you: "p0".into(),
            my_hand: vec![],
            opponents: vec![],
            battlefield: vec![],
            stack: vec![],
            graveyards: vec![],
            exile: vec![],
            phase: Phase::PrecombatMain,
            mana_pool: vec![],
            priority_player: None,
            valid_actions: vec![],
            action_deadline: None,
        };
        // Empty pool is elided from the wire.
        let json = serde_json::to_value(&view).unwrap();
        assert!(json.get("mana_pool").is_none());

        // A non-empty pool round-trips as a list of pip strings.
        view.mana_pool = vec!["{G}".into(), "{G}".into()];
        let back: GameView = serde_json::from_str(&serde_json::to_string(&view).unwrap()).unwrap();
        assert_eq!(back.mana_pool, vec!["{G}".to_string(), "{G}".to_string()]);
    }

    #[test]
    fn canonical_fixture_round_trips_and_matches_typed_fields() {
        // Single-sourced cross-language contract fixture (issue #56): this exact
        // JSON is also consumed by the web client's `wire.test.ts`. A field
        // renamed, retyped, or removed in this crate without updating the fixture
        // fails to deserialize (or mismatches an assertion) here — the same drift
        // the same-PR discipline used to catch by convention alone.
        let json = include_str!("../fixtures/gameview.json");
        let view: GameView = serde_json::from_str(json).unwrap();

        // Round-trips through serde JSON without loss.
        let reencoded = serde_json::to_string(&view).unwrap();
        let back: GameView = serde_json::from_str(&reencoded).unwrap();
        assert_eq!(back, view);

        // Load-bearing typed fields: a rename/retype in the structs breaks one of
        // these (or the deserialize above) rather than passing silently.
        assert_eq!(view.you, "p1");
        assert_eq!(view.phase, Phase::PrecombatMain);
        assert_eq!(view.mana_pool, vec!["{G}".to_string(), "{G}".to_string()]);
        assert_eq!(view.priority_player.as_deref(), Some("p1"));
        assert_eq!(view.action_deadline, Some(12.5));

        // Populated hand: creature carries P/T, the land omits them.
        assert_eq!(
            view.my_hand
                .iter()
                .map(|c| c.id.as_str())
                .collect::<Vec<_>>(),
            ["c1", "c2", "c3"]
        );
        assert_eq!(view.my_hand[0].power.as_deref(), Some("1"));
        assert_eq!(view.my_hand[1].power, None);

        // Opponent view redacts hidden zones to counts and carries statuses.
        assert_eq!(view.opponents[0].hand_size, 7);
        assert_eq!(view.opponents[0].statuses, vec!["monarch".to_string()]);

        // Battlefield: a tapped permanent with a `+1/+1` counter and a
        // planeswalker with a `loyalty` counter — exercising `Counter {kind, count}`.
        let bear = &view.battlefield[0];
        assert!(bear.tapped);
        assert_eq!(
            bear.counters,
            vec![Counter {
                kind: "+1/+1".into(),
                count: 2,
            }]
        );
        assert_eq!(view.battlefield[1].counters[0].kind, "loyalty");
        assert_eq!(view.battlefield[1].counters[0].count, 5);
        assert!(!view.battlefield[1].tapped);

        // Stack: an ability carries its `source`; a spell does not.
        assert_eq!(view.stack[0].source, None);
        assert_eq!(view.stack[1].source.as_deref(), Some("perm_bear"));

        // Public piles round-trip populated.
        assert_eq!(view.graveyards[0].cards[0].id, "g1");
        assert_eq!(view.exile[0].cards[0].id, "x1");

        // Every valid-action kind emitted today is represented, in order.
        assert_eq!(
            view.valid_actions
                .iter()
                .map(|a| a.kind.as_str())
                .collect::<Vec<_>>(),
            [
                "pass_priority",
                "play_land",
                "cast_spell",
                "activate_ability"
            ]
        );
        // `pass_priority` is subject-less; the ability action names its permanent.
        assert!(view.valid_actions[0].subject.is_empty());
        assert_eq!(view.valid_actions[3].subject, vec!["perm_bear".to_string()]);
    }

    #[test]
    fn permanent_combat_state_round_trips_and_elides_when_absent() {
        // Attack/block state (issue #117): `attacking` and `blocking` round-trip
        // when present, and both elide from the wire in the common not-in-combat
        // case so the serialized shape is unchanged for non-combat permanents.
        let base = Permanent {
            id: "perm_1".into(),
            controller: "p0".into(),
            owner: "p0".into(),
            card: CardView {
                id: "perm_1".into(),
                name: "Grizzly Bears".into(),
                type_line: "Creature — Bear".into(),
                mana_cost: Some("{1}{G}".into()),
                oracle_text: String::new(),
                power: Some("2".into()),
                toughness: Some("2".into()),
            },
            tapped: false,
            attacking: false,
            blocking: None,
            counters: vec![],
        };

        // Not in combat: both fields elide from the JSON.
        let json = serde_json::to_value(&base).unwrap();
        assert!(json.get("attacking").is_none());
        assert!(json.get("blocking").is_none());

        // An attacker and its blocker both round-trip with their state present.
        let attacker = Permanent {
            attacking: true,
            ..base.clone()
        };
        let blocker = Permanent {
            blocking: Some("perm_1".into()),
            ..base.clone()
        };
        let attacker_json = serde_json::to_value(&attacker).unwrap();
        assert_eq!(
            attacker_json.get("attacking"),
            Some(&serde_json::json!(true))
        );
        assert_eq!(
            serde_json::from_value::<Permanent>(attacker_json).unwrap(),
            attacker
        );
        let blocker_json = serde_json::to_value(&blocker).unwrap();
        assert_eq!(
            blocker_json.get("blocking"),
            Some(&serde_json::json!("perm_1"))
        );
        assert_eq!(
            serde_json::from_value::<Permanent>(blocker_json).unwrap(),
            blocker
        );
    }

    #[test]
    fn unknown_fields_are_ignored() {
        // Forward-compat invariant (docs/protocol.md): a newer server may add
        // fields; older clients must still deserialize the message.
        let json = r#"{ "phase": "draw", "some_future_field": 42 }"#;
        let view: GameView = serde_json::from_str(json).unwrap();
        assert_eq!(view.phase, Phase::Draw);
        assert!(view.my_hand.is_empty());
    }

    #[test]
    fn you_defaults_to_empty_when_absent() {
        // Backward-compat: a payload from an older server omits `you`; it must
        // still deserialize, defaulting the seat id to an empty string rather
        // than failing the whole message.
        let json = r#"{ "phase": "draw" }"#;
        let view: GameView = serde_json::from_str(json).unwrap();
        assert_eq!(view.you, "");
    }

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
        // A populated decklist round-trips as a flat list of identities.
        let msg = LobbyCommand::SubmitDeck(SubmitDeck {
            cards: vec!["ci_bear".into(), "ci_bear".into(), "ci_forest".into()],
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

        // An empty decklist elides the `cards` field entirely.
        let empty = LobbyCommand::SubmitDeck(SubmitDeck { cards: vec![] });
        let json = serde_json::to_value(&empty).unwrap();
        assert_eq!(json, serde_json::json!({ "type": "submit_deck" }));
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
    fn lobby_view_round_trips_populated() {
        let view = LobbyView {
            session: "s:ab12".into(),
            you: "p1".into(),
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
                        decked: true,
                        ready: true,
                    },
                    SeatView {
                        seat: 1,
                        occupied_by: Some("p2".into()),
                        decked: true,
                        ready: false,
                    },
                ],
            }),
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
            room: None,
            valid_commands: vec!["create_room".into(), "join_room".into()],
        };
        let json = serde_json::to_value(&view).unwrap();
        assert!(json.get("room").is_none());
        // `session` and `you` are always present on the wire (like `GameView::you`).
        assert_eq!(json.get("session"), Some(&serde_json::json!("s:new")));
        assert_eq!(json.get("you"), Some(&serde_json::json!("p9")));

        // An empty seat serializes to just its index.
        let empty_seat = SeatView {
            seat: 3,
            occupied_by: None,
            decked: false,
            ready: false,
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
        assert_eq!(view.valid_commands, vec!["hello".to_string()]);
    }

    #[test]
    fn game_view_serializes_you_on_the_wire() {
        let view = GameView {
            you: "p1".into(),
            my_hand: vec![],
            opponents: vec![],
            battlefield: vec![],
            stack: vec![],
            graveyards: vec![],
            exile: vec![],
            phase: Phase::Upkeep,
            mana_pool: vec![],
            priority_player: None,
            valid_actions: vec![],
            action_deadline: None,
        };
        let json = serde_json::to_value(&view).unwrap();
        // The receiver's own seat id is always present on the wire (like `phase`),
        // not elided the way empty collections are.
        assert_eq!(json.get("you"), Some(&serde_json::json!("p1")));
        let back: GameView = serde_json::from_value(json).unwrap();
        assert_eq!(back.you, "p1");
    }
}
