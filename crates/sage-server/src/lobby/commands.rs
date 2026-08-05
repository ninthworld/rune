//! Command handlers for the lobby state machine: membership (join/spectate/leave), the
//! deck-submission and ready gate that constructs the game, AI seating, and display-name
//! setting. The `Lobby` methods here are an additional `impl Lobby` block; the free
//! functions round out the [`LobbyCommand`] routing in the module root. Pure code motion
//! out of the lobby module root (issue #409) — no behavior change.
//!
//! The two commands that own a room's [`RoomConfig`] — `create_room` and the host-only
//! `update_room` (issue #546) — live next door in [`room_config`](super::room_config),
//! so the rules a config is judged by have one home and this module stays under the
//! file-size ceiling.

mod gate;
mod membership;

pub(crate) use membership::*;
