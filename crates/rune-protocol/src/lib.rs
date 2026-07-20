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
//! grow without breaking older clients — see the forward-compat tests.
//!
//! The contract is organized into focused modules, each re-exported at the crate
//! root so every type is reachable as `rune_protocol::Foo` regardless of where it
//! is defined:
//! - [`log`] — structured game-log events
//! - [`card`] — in-game card, board, and zone views
//! - [`action`] — the valid-action and prompt/targeting contract
//! - [`result`] — game-end outcome and commander tallies
//! - [`view`] — the personalized in-game [`GameView`]
//! - [`spectator`] — the redacted [`SpectatorView`]
//! - [`client`] — client → server in-game messages
//! - [`lobby`] — the pre-game lobby message set
//! - [`catalog`] — the public card catalog

mod action;
mod card;
mod catalog;
mod client;
mod lobby;
mod log;
mod result;
mod spectator;
mod view;

pub use action::{Prompt, PromptOption, TargetRequirement, ValidAction};
pub use card::{CardView, Counter, OpponentView, Permanent, Phase, SelfView, StackItem, ZonePile};
pub use catalog::{AiOption, CatalogCard, CatalogFormat, CatalogView, CATALOG_VERSION};
pub use client::{ChooseAction, ClientMessage, SetStops, TargetChoice};
pub use lobby::{
    AddAi, CardIdentity, CreateRoom, GameSetupId, Hello, JoinRoom, LobbyCommand, LobbyView, Ready,
    RemoveAi, RoomConfig, RoomId, RoomState, RoomSummary, RoomView, SeatView, SessionToken,
    SetName, SpectateRoom, SubmitDeck,
};
pub use log::{GameLogEntry, GameLogEvent, LogBlock, LogDamageTarget, LogEntity};
pub use result::{CommanderDamage, CommanderTax, GameOverReason, GameResult};
pub use spectator::SpectatorView;
pub use view::GameView;

/// Opaque player identity (server-assigned).
pub type PlayerId = String;

/// Opaque per-game entity id: a card, permanent, or stack object.
pub type EntityId = String;

#[allow(clippy::trivially_copy_pass_by_ref)]
pub(crate) fn is_false(b: &bool) -> bool {
    !*b
}

#[allow(clippy::trivially_copy_pass_by_ref)]
pub(crate) fn is_zero(n: &u32) -> bool {
    *n == 0
}

#[allow(clippy::trivially_copy_pass_by_ref)]
pub(crate) fn is_zero_u8(n: &u8) -> bool {
    *n == 0
}
