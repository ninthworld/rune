//! Registry and session helpers for the lobby: seat/room lookups, card and name
//! validation, and the seed/token minting the connection lifecycle draws on. Pure
//! code motion out of the lobby module root (issue #409) — no behavior change.

use super::*;

/// Validate a requested display name (issue #294), returning the cleaned name to
/// store or a typed [`NameError`]. Policy: trim surrounding whitespace; reject an
/// empty result, a name longer than [`MAX_NAME_LEN`] scalar values, or one holding a
/// control character (newlines, NUL, and other non-printable code points). Names need
/// not be unique — the seat's [`PlayerId`] remains the identity, so a collision is
/// allowed rather than rejected (two "Alice"s are disambiguated by their seat).
pub(crate) fn validate_name(requested: &str) -> Result<String, NameError> {
    let trimmed = requested.trim();
    if trimmed.is_empty() {
        return Err(NameError::Empty);
    }
    let len = trimmed.chars().count();
    if len > MAX_NAME_LEN {
        return Err(NameError::TooLong(len));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(NameError::Unprintable);
    }
    Ok(trimmed.to_string())
}

/// Clear a seat's occupant and reset its pre-game gate state (a vacated seat is
/// undecked and unready). A stale room id/seat is ignored.
pub(crate) fn vacate(registry: &mut Registry, room_id: &RoomId, seat: usize) {
    if let Some(room) = registry.rooms.get_mut(room_id) {
        if let Some(slot) = room.seats.get_mut(seat) {
            *slot = None;
        }
        if let Some(gate) = room.gate.get_mut(seat) {
            *gate = SeatGate::default();
        }
    }
}

/// Reclaim rooms the lobby no longer needs to hold, freeing the capacity they held:
///
/// - a **pre-game** room ([`RoomEntry::game`] is `None`) with no remaining occupants
///   (every seat explicitly vacated); and
/// - a **finished** started room, whose game task has stopped so its
///   [`RoomHandle`](crate::room::RoomHandle) is no longer active (issue #280) — a live
///   game's room is kept, since its task still owns the seats' lifecycle.
pub(crate) fn reap_empty(registry: &mut Registry) {
    registry.rooms.retain(|room_id, room| {
        match &room.game {
            // A live game: keep it (its task owns the seats now).
            Some(handle) if handle.is_active() => true,
            // A finished game: its task has stopped, so reclaim the room.
            Some(_) => {
                info!(%room_id, "reclaimed finished room");
                false
            }
            // Pre-game: keep only while at least one seat is still occupied.
            None => {
                let occupied = room.seats.iter().any(Option::is_some);
                if !occupied {
                    info!(%room_id, "reclaimed empty room");
                }
                occupied
            }
        }
    });
}

/// The room id and seat index of a seated session, or [`LobbyError::NotSeated`] if
/// the session is not seated in a room.
pub(crate) fn seat_of(
    registry: &Registry,
    token: &SessionToken,
) -> Result<(RoomId, usize), LobbyError> {
    match registry.sessions.get(token) {
        Some(Session {
            room: Some(room_id),
            seat: Some(seat),
            ..
        }) => Ok((room_id.clone(), *seat)),
        _ => Err(LobbyError::NotSeated),
    }
}

/// Resolve a wire [`CardIdentity`] to an engine [`CardId`], or `None` if it does not
/// name a card in `db`.
///
/// A decklist names cards by their authored `functional_id` (ADR 0018 §3) — the identity
/// vocabulary ADR 0013 deferred and ADR 0018 settled. It cannot name them by `CardId`:
/// that handle is interned by `build.rs` from the catalog's sort order, so authoring one
/// new card renumbers its neighbours, and a decklist written against an integer would
/// silently come to mean different cards. The `functional_id` is the only card identity
/// stable across builds, which is exactly why it is what crosses the wire.
///
/// The wire *shape* is unchanged: [`CardIdentity`] is an opaque string the client never
/// parses, and the server remains the sole authority on what it resolves to.
///
/// [`CardIdentity`]: rune_protocol::CardIdentity
pub(crate) fn resolve_card(db: &CardDatabase, identity: &str) -> Option<CardId> {
    let functional_id = FunctionalId::try_from(identity.to_string()).ok()?;
    db.card_id(&functional_id)
}

/// A server-generated shuffle seed for a starting game (ADR 0012). The engine is
/// pure and takes its only randomness from this seed; the *server* is where the
/// entropy is sourced. Mixes the wall clock with a process-lifetime counter so two
/// games constructed in the same instant still get distinct seeds.
pub(crate) fn generate_seed() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    nanos ^ n.wrapping_mul(0x9E37_79B9_7F4A_7C15)
}

/// Mint an unguessable per-session token from the operating-system CSPRNG.
///
/// The token authenticates a reconnect to a held seat (issue #113), so — unlike the
/// sequential, public room id — it is a **secret** and must be neither guessable nor
/// derivable from any public value (issue #48). It carries 128 bits of entropy,
/// hex-encoded behind an `s` tag; the value is opaque to clients (`docs/protocol.md`).
///
/// # Errors
/// Propagates [`getrandom::Error`] if the OS entropy source is unavailable.
pub(crate) fn mint_token() -> Result<SessionToken, getrandom::Error> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes)?;
    let mut token = String::with_capacity(1 + bytes.len() * 2);
    token.push('s');
    for byte in bytes {
        // Indices are always < 16, so this never panics.
        token.push(HEX[usize::from(byte >> 4)] as char);
        token.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    Ok(token)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::lobby::test_support::*;

    #[tokio::test]
    async fn an_empty_room_is_reclaimed_and_frees_capacity() {
        // Capacity for exactly one room. Alice creates it, filling the cap; a second
        // creator is refused. Alice leaves, emptying and reclaiming the room, which
        // frees the slot for the next creator.
        let lobby = lobby(1);
        let mut alice = Client::connect(&lobby).await;
        let _ = alice.view().await;
        lobby
            .command(
                &alice.token,
                LobbyCommand::CreateRoom(CreateRoom { config: config(2) }),
            )
            .await
            .unwrap();
        let _ = alice.view().await;

        let mut bob = Client::connect(&lobby).await;
        let _ = bob.view().await;
        let err = lobby
            .command(
                &bob.token,
                LobbyCommand::CreateRoom(CreateRoom { config: config(2) }),
            )
            .await
            .expect_err("at capacity");
        assert_eq!(err, LobbyError::AtCapacity);

        // Alice leaves: her room is empty and reclaimed, freeing the single slot.
        lobby
            .command(&alice.token, LobbyCommand::Leave)
            .await
            .unwrap();

        // Bob can now create a room where he previously could not.
        lobby
            .command(
                &bob.token,
                LobbyCommand::CreateRoom(CreateRoom { config: config(2) }),
            )
            .await
            .expect("capacity freed by reclamation");
        assert!(bob.view().await.room.is_some());
    }

    #[tokio::test]
    async fn leaving_the_last_seat_reclaims_the_room() {
        // The seat is only ever vacated by an explicit `Leave`; once the room is
        // empty it is reclaimed and its id becomes unknown.
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

        lobby
            .command(&alice.token, LobbyCommand::Leave)
            .await
            .unwrap();

        let mut carol = Client::connect(&lobby).await;
        let _ = carol.view().await;
        let err = lobby
            .command(&carol.token, LobbyCommand::JoinRoom(JoinRoom { room_id }))
            .await
            .expect_err("the reclaimed room is gone");
        assert_eq!(err, LobbyError::UnknownRoom);
    }

    #[tokio::test]
    async fn session_tokens_are_unguessable_and_distinct_from_the_public_identity() {
        let lobby = lobby(4);
        let a = Client::connect(&lobby).await;
        let b = Client::connect(&lobby).await;

        // Per-session and unique.
        assert_ne!(a.token, b.token);
        // Real entropy, not the old sequential "s{n}" scheme an attacker could guess.
        assert!(a.token.len() >= 16, "token carries real entropy");
        assert!(
            !matches!(a.token.as_str(), "s0" | "s1" | "s2"),
            "tokens are not sequential/guessable",
        );
        // The secret token is never the public identity shown to opponents.
        assert_ne!(a.token, a.current().you, "secret token != public identity");
    }

    #[tokio::test]
    async fn commands_that_require_a_seat_are_typed_errors_when_roomless() {
        let lobby = lobby(4);
        let mut alice = Client::connect(&lobby).await;
        let _ = alice.view().await;

        // Roomless: leave, submit_deck, and ready all require a seat.
        assert_eq!(
            lobby.command(&alice.token, LobbyCommand::Leave).await,
            Err(LobbyError::NotInRoom)
        );
        assert_eq!(
            lobby
                .command(
                    &alice.token,
                    LobbyCommand::SubmitDeck(SubmitDeck::default())
                )
                .await,
            Err(LobbyError::NotSeated)
        );
        assert_eq!(
            lobby
                .command(&alice.token, LobbyCommand::Ready(Ready { ready: true }))
                .await,
            Err(LobbyError::NotSeated)
        );
    }

    #[tokio::test]
    async fn set_name_trims_and_rejects_invalid_names_non_fatally() {
        // Issue #294: validation policy — trim; reject empty/whitespace, over-long, and
        // control-character names with a typed error, leaving the stored name untouched
        // (the lobby's non-fatal pattern; the caller re-sends the current view).
        let lobby = lobby(4);
        let mut alice = Client::connect(&lobby).await;
        let _ = alice.view().await;

        // A surrounding-whitespace name is trimmed, not rejected.
        lobby
            .command(
                &alice.token,
                LobbyCommand::SetName(SetName {
                    name: "  Alice  ".into(),
                }),
            )
            .await
            .expect("a trimmable name is accepted");
        assert_eq!(alice.view().await.name.as_deref(), Some("Alice"));

        // Empty / whitespace-only.
        assert_eq!(
            lobby
                .command(
                    &alice.token,
                    LobbyCommand::SetName(SetName { name: "   ".into() })
                )
                .await,
            Err(LobbyError::InvalidName(NameError::Empty))
        );
        // Over the length limit (counted in scalar values).
        let too_long = "x".repeat(MAX_NAME_LEN + 1);
        assert_eq!(
            lobby
                .command(
                    &alice.token,
                    LobbyCommand::SetName(SetName { name: too_long })
                )
                .await,
            Err(LobbyError::InvalidName(NameError::TooLong(
                MAX_NAME_LEN + 1
            )))
        );
        // A control character (newline) is non-printable.
        assert_eq!(
            lobby
                .command(
                    &alice.token,
                    LobbyCommand::SetName(SetName {
                        name: "Al\nice".into()
                    })
                )
                .await,
            Err(LobbyError::InvalidName(NameError::Unprintable))
        );

        // The earlier accepted name is untouched by the rejected attempts.
        assert_eq!(alice.current().name.as_deref(), Some("Alice"));
    }
}
