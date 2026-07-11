//! Layer 1 lobby — the room registry and connection→room routing.
//!
//! The lobby is the connective tissue between the accept loop (issue #30) and the
//! room task (issue #31). It owns the **room registry** — the shared
//! `Arc<RwLock<...>>` of active rooms from `docs/brief.md` — and, on each accepted
//! and handshaken connection, hands it a seat in a room. From there the connection
//! is driven entirely by [`serve_connection`](crate::serve_connection): the lobby
//! never reads or writes game state, so it holds **no game logic** (the engine owns
//! the rules; the room owns the one game).
//!
//! # Seating policy — auto-pairing, "next open seat"
//! A new connection takes the first free seat in any existing room; only when no
//! room has a free seat is a fresh two-player room created. When every room is full
//! and the registry is already at capacity ([`Lobby::DEFAULT_MAX_ROOMS`], or the
//! bound passed to [`Lobby::new`]), the connection is rejected (oversubscribed).
//! There is deliberately no identity, auth, room naming, chat, or matchmaking — all
//! out of scope for this milestone.
//!
//! # Disconnect — a vacated seat is retired, never reissued
//! When a connection ends, its seat is **retired**: [`Lobby::release`] marks it
//! permanently spent so [`Lobby::assign`] never hands it to another connection. A
//! seat only ever moves `Open → Taken → Retired`; it never returns to `Open`.
//!
//! This closes a hidden-hand leak (issue #48). The room deliberately holds a seat
//! open across disconnects (issue #31) and, on any join, pushes that seat's *full
//! personalized* [`GameView`](rune_protocol::GameView) — including its private
//! `my_hand`. Before this policy, `release` re-opened the seat and `assign` handed
//! it to the next stranger, who then joined as the departed player and received
//! their hand. The lobby has **no identity binding**, so it cannot tell a returning
//! player from a stranger; it must therefore treat every new connection as a
//! stranger and refuse to seat one into occupied-then-vacated territory.
//!
//! True reconnection — reuniting a returning player with their held-open seat — is
//! consequently blocked on an identity / reconnect-token mechanism (a future
//! milestone). Until it lands, a game whose opponent left simply sits with a
//! permanently reserved seat. That reservation, and rooms in general, are not
//! garbage-collected in this milestone (a separate issue).

use std::collections::HashMap;
use std::sync::Arc;

use rune_engine::{CardDatabase, GameState};
use tokio::sync::RwLock;
use tracing::info;

use crate::room::{Room, RoomHandle, Seat};

/// Seats in every room the lobby opens. RUNE hosts two-player games only.
const SEATS_PER_ROOM: usize = 2;

/// A stable identifier for a room within the [`Lobby`] registry.
type RoomId = u64;

/// The shared room registry (layer 1 of `docs/brief.md`).
///
/// Cloning a [`Lobby`] is cheap: every clone shares one registry behind an
/// `Arc<RwLock<...>>`, so each connection task can hold its own handle. The lobby
/// owns the [`CardDatabase`] every room is built from and the cap on how many rooms
/// it will host concurrently.
#[derive(Clone)]
pub struct Lobby {
    inner: Arc<Inner>,
}

/// The `Arc`-shared interior of a [`Lobby`].
struct Inner {
    /// The mutable set of active rooms.
    registry: RwLock<Registry>,
    /// The card database every room is built from.
    db: CardDatabase,
    /// The cap on concurrently hosted rooms.
    max_rooms: usize,
}

/// The registry of active rooms, keyed by a monotonic [`RoomId`].
#[derive(Default)]
struct Registry {
    /// The next id to hand out; only ever increases, so ids are never reused.
    next_id: RoomId,
    /// Active rooms by id.
    rooms: HashMap<RoomId, RoomSlot>,
}

/// One room's registry entry: a handle to its task and its per-seat occupancy.
struct RoomSlot {
    /// Handle for delivering inputs to the room task.
    handle: RoomHandle,
    /// Lifecycle per seat, indexed by [`Seat`].
    seats: Vec<SeatState>,
}

/// The lifecycle of one seat in a room.
///
/// A seat only ever moves forward — `Open → Taken → Retired` — and never returns
/// to `Open`. That one-way transition is what keeps a vacated seat (and the private
/// hand behind it) from being handed to a stranger; see the module docs.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SeatState {
    /// Never occupied; free for the next connection to take.
    Open,
    /// Currently held by a live connection.
    Taken,
    /// Was occupied and then vacated. Held reserved for the rest of the room's
    /// life and never reissued (no identity binding to authorize a rejoin yet).
    Retired,
}

/// A seat the lobby assigned to a connection, with the handle to reach its room.
pub(crate) struct SeatAssignment {
    /// The room the seat belongs to.
    pub(crate) room_id: RoomId,
    /// The seat index within that room.
    pub(crate) seat: Seat,
    /// A handle for driving the assigned room.
    pub(crate) room: RoomHandle,
}

impl Lobby {
    /// The default cap on concurrently hosted rooms. Kept modest and explicit for
    /// this milestone; real capacity planning is a later concern (`docs/brief.md`
    /// targets tens of thousands of games per node).
    pub const DEFAULT_MAX_ROOMS: usize = 1024;

    /// Create an empty lobby that builds every room from `db` and hosts at most
    /// `max_rooms` rooms at once.
    #[must_use]
    pub fn new(db: CardDatabase, max_rooms: usize) -> Self {
        Self {
            inner: Arc::new(Inner {
                registry: RwLock::new(Registry::default()),
                db,
                max_rooms,
            }),
        }
    }

    /// Create a lobby whose rooms use the engine's bundled card database.
    ///
    /// # Errors
    /// Returns the underlying [`serde_json::Error`] if the bundled snapshot fails
    /// to parse (see [`CardDatabase::bundled`]).
    pub fn bundled(max_rooms: usize) -> Result<Self, serde_json::Error> {
        Ok(Self::new(CardDatabase::bundled()?, max_rooms))
    }

    /// Assign the next open seat to a connection, opening a fresh room only when no
    /// existing room has a free seat (auto-pairing).
    ///
    /// Returns `None` when every room is full and the registry is at capacity — the
    /// oversubscribed case the caller rejects cleanly.
    pub(crate) async fn assign(&self) -> Option<SeatAssignment> {
        let mut registry = self.inner.registry.write().await;

        // Prefer an existing room that still has a never-occupied (`Open`) seat.
        // A `Retired` seat is deliberately skipped: it is held reserved forever so
        // its vacated hand can never leak to a newcomer (see module docs).
        for (&room_id, slot) in registry.rooms.iter_mut() {
            if let Some(seat) = slot
                .seats
                .iter()
                .position(|state| *state == SeatState::Open)
            {
                slot.seats[seat] = SeatState::Taken;
                return Some(SeatAssignment {
                    room_id,
                    seat,
                    room: slot.handle.clone(),
                });
            }
        }

        // Every room is full: open a new one if capacity allows.
        if registry.rooms.len() >= self.inner.max_rooms {
            return None;
        }
        let (handle, _task) = Room::new(GameState::new_two_player(), self.inner.db.clone()).spawn();
        // The opener takes seat 0; the room task lives as long as its registry
        // entry keeps this handle (rooms are not reclaimed in this milestone).
        let mut seats = vec![SeatState::Open; SEATS_PER_ROOM];
        if let Some(first) = seats.first_mut() {
            *first = SeatState::Taken;
        }
        let room_id = registry.next_id;
        registry.next_id += 1;
        registry.rooms.insert(
            room_id,
            RoomSlot {
                handle: handle.clone(),
                seats,
            },
        );
        info!(room_id, "opened room");
        Some(SeatAssignment {
            room_id,
            seat: 0,
            room: handle,
        })
    }

    /// Retire a seat when its connection ends so it is never reissued.
    ///
    /// The room's game state is left untouched (issue #31 holds the seat open); the
    /// registry marks the seat [`SeatState::Retired`] so [`Lobby::assign`] can never
    /// hand it — and the departed player's private hand behind it — to a new,
    /// unidentified connection (issue #48; see the module docs). Rejoining a retired
    /// seat awaits a reconnect-token mechanism (future milestone).
    ///
    /// A stale `room_id`/`seat` is ignored, so a double release cannot corrupt the
    /// registry.
    pub(crate) async fn release(&self, room_id: RoomId, seat: Seat) {
        let mut registry = self.inner.registry.write().await;
        if let Some(slot) = registry.rooms.get_mut(&room_id) {
            if let Some(state) = slot.seats.get_mut(seat) {
                *state = SeatState::Retired;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::room::RoomInput;
    use tokio::sync::mpsc;

    fn lobby(max_rooms: usize) -> Lobby {
        Lobby::bundled(max_rooms).expect("bundled cards")
    }

    #[tokio::test]
    async fn pairs_two_connections_into_one_room_at_distinct_seats() {
        let lobby = lobby(4);
        let first = lobby.assign().await.expect("first seat");
        let second = lobby.assign().await.expect("second seat");
        // Auto-pairing: both land in the same room, at different seats.
        assert_eq!(first.room_id, second.room_id);
        assert_ne!(first.seat, second.seat);
    }

    #[tokio::test]
    async fn opens_a_new_room_once_the_first_is_full() {
        let lobby = lobby(4);
        let a = lobby.assign().await.expect("room 0 seat 0");
        let b = lobby.assign().await.expect("room 0 seat 1");
        assert_eq!(a.room_id, b.room_id);
        // The two-seat room is full, so the third connection opens a new room.
        let c = lobby.assign().await.expect("room 1 seat 0");
        assert_ne!(c.room_id, a.room_id);
        assert_eq!(c.seat, 0);
    }

    #[tokio::test]
    async fn rejects_when_at_capacity_and_full() {
        let lobby = lobby(1);
        lobby.assign().await.expect("seat 0");
        lobby.assign().await.expect("seat 1");
        // One room, both seats taken, capacity reached: the next assign is rejected.
        assert!(lobby.assign().await.is_none());
    }

    #[tokio::test]
    async fn releasing_a_seat_retires_it_rather_than_reopening_it() {
        // New policy (issue #48): a released seat is retired, not reopened. With the
        // room's only other seat also taken, a released seat leaves no `Open` seat,
        // so the lobby opens a fresh room rather than reissuing the vacated one.
        let lobby = lobby(4);
        let first = lobby.assign().await.expect("room 0 seat 0");
        let second = lobby.assign().await.expect("room 0 seat 1");
        assert_eq!(first.room_id, second.room_id);

        lobby.release(first.room_id, first.seat).await;

        // The next connection never gets the vacated (room, seat); it opens a new room.
        let next = lobby.assign().await.expect("fresh room");
        assert_ne!(next.room_id, first.room_id);
        assert!(!(next.room_id == first.room_id && next.seat == first.seat));
    }

    #[tokio::test]
    async fn a_retired_seat_is_the_last_seat_at_capacity() {
        // One room, both seats taken, at capacity. Releasing a seat retires it, so
        // there is still no `Open` seat and no room budget: the lobby stays full.
        let lobby = lobby(1);
        let first = lobby.assign().await.expect("seat 0");
        lobby.assign().await.expect("seat 1");
        lobby.release(first.room_id, first.seat).await;
        assert!(lobby.assign().await.is_none());
    }

    #[tokio::test]
    async fn issue_48_a_vacated_seat_never_leaks_its_hand_to_a_new_connection() {
        // Regression for issue #48: seat a player, disconnect them (`release`), then
        // open a new connection. It must land on a fresh room/seat — never the
        // vacated one — so its first `GameView` carries its own (empty) hand and
        // never the departed player's `my_hand`.
        let lobby = lobby(4);

        // A player takes a seat and is brought current in the room.
        let departed = lobby.assign().await.expect("departed seat");
        let (tx, mut rx) = mpsc::unbounded_channel();
        assert!(departed.room.send(RoomInput::Join {
            seat: departed.seat,
            outbox: tx,
        }));
        rx.recv().await.expect("departed view");

        // The player disconnects: the lobby retires their seat.
        lobby.release(departed.room_id, departed.seat).await;

        // A brand-new connection arrives. It is never handed the vacated (room, seat).
        let newcomer = lobby.assign().await.expect("newcomer seat");
        assert!(
            !(newcomer.room_id == departed.room_id && newcomer.seat == departed.seat),
            "assign reissued the vacated seat to a new connection",
        );

        // On join the room personalizes the view to the newcomer's own seat — a
        // fresh seat in a new game — so its first `GameView` has an empty hand and
        // does not contain the vacated seat's `my_hand`.
        let (tx2, mut rx2) = mpsc::unbounded_channel();
        assert!(newcomer.room.send(RoomInput::Join {
            seat: newcomer.seat,
            outbox: tx2,
        }));
        let first_view = rx2.recv().await.expect("newcomer view");
        assert!(
            first_view.my_hand.is_empty(),
            "newcomer inherited a non-empty hand from a vacated seat",
        );
    }

    #[tokio::test]
    async fn releasing_an_unknown_seat_is_a_no_op() {
        let lobby = lobby(1);
        // Never panics or corrupts the registry, even for ids that were never issued.
        lobby.release(999, 7).await;
        lobby.release(0, 42).await;
        // The registry is still fully usable afterwards.
        assert!(lobby.assign().await.is_some());
    }
}
