//! Who is in a room: joining a seat, spectating without one, leaving, removing a seated
//! AI, and setting the display name a roster shows.

use crate::lobby::*;

#[cfg(test)]
mod tests;

/// Handle `join_room`: seat the joiner in the first free seat of an existing room,
/// or return a typed error for an unknown or full room.
pub(crate) fn join_room(
    registry: &mut Registry,
    token: &SessionToken,
    room_id: &RoomId,
) -> Result<(), LobbyError> {
    if registry
        .sessions
        .get(token)
        .is_some_and(|s| s.room.is_some())
    {
        return Err(LobbyError::AlreadyInRoom);
    }
    let room = registry
        .rooms
        .get_mut(room_id)
        .ok_or(LobbyError::UnknownRoom)?;
    // A seat is joinable only when it holds neither a human nor an AI (issue #415), so a
    // joiner never lands in a seat the host filled with an AI opponent.
    let seat = room
        .seats
        .iter()
        .zip(&room.ai_seats)
        .position(|(session, ai)| session.is_none() && ai.is_none())
        .ok_or(LobbyError::RoomFull)?;
    room.seats[seat] = Some(token.clone());
    if let Some(session) = registry.sessions.get_mut(token) {
        session.room = Some(room_id.clone());
        session.seat = Some(seat);
    }
    // Every occupant's roster changed, and the room's occupancy changed in the
    // directory: re-project to occupants and to everyone browsing.
    broadcast_views(registry);
    info!(%token, %room_id, seat, "joined room");
    Ok(())
}

/// Handle `spectate_room` (issue #351): attach the sender as a spectator of
/// an **in-progress** room without consuming a seat. Unlike [`join_room`] this succeeds
/// on a room whose seats are full, but the room's game must already be running — there
/// is no board to watch until the ready gate passes ([`LobbyError::RoomNotStarted`]).
/// On success the session is marked as spectating (`room` set, `seat` left `None`), the
/// room's spectator roster gains its token (advertised as a count in the directory),
/// and the connection is handed off to the read-only spectator bridge via
/// [`LobbySignal::Spectate`].
pub(crate) fn spectate_room(
    registry: &mut Registry,
    token: &SessionToken,
    room_id: &RoomId,
) -> Result<(), LobbyError> {
    if registry
        .sessions
        .get(token)
        .is_some_and(|s| s.room.is_some())
    {
        return Err(LobbyError::AlreadyInRoom);
    }
    let room = registry
        .rooms
        .get_mut(room_id)
        .ok_or(LobbyError::UnknownRoom)?;
    // A spectator needs a live game to watch. A gathering room has no board yet.
    let handle = match &room.game {
        Some(handle) if handle.is_active() => handle.clone(),
        _ => return Err(LobbyError::RoomNotStarted),
    };
    room.spectators.push(token.clone());
    if let Some(session) = registry.sessions.get(token) {
        // Hand this connection off to the read-only spectator contract immediately —
        // like the `Start` gate, a terminal signal after which no `LobbyView` is pushed.
        let _ = session
            .outbox
            .send(Some(LobbySignal::Spectate { room: handle }));
    }
    if let Some(session) = registry.sessions.get_mut(token) {
        session.room = Some(room_id.clone());
        session.seat = None;
    }
    // The room's spectator count changed in the directory: re-project to browsers.
    broadcast_views(registry);
    info!(%token, %room_id, "joined room as spectator");
    Ok(())
}

/// Handle `leave`: vacate the sender's seat, reclaim the room if it is now empty,
/// otherwise notify the remaining occupants.
pub(crate) fn leave_room(registry: &mut Registry, token: &SessionToken) -> Result<(), LobbyError> {
    let (room_id, seat) = match registry.sessions.get(token) {
        Some(Session {
            room: Some(room_id),
            seat,
            ..
        }) => (room_id.clone(), *seat),
        _ => return Err(LobbyError::NotInRoom),
    };
    // A spectator (issue #351) holds no seat: drop it from the room's spectator roster
    // instead of vacating a seat, then clear its session and re-project the directory
    // (its spectator count changed). The room is never reaped for losing a spectator.
    let Some(seat) = seat else {
        if let Some(room) = registry.rooms.get_mut(&room_id) {
            room.spectators.retain(|t| t != token);
        }
        if let Some(session) = registry.sessions.get_mut(token) {
            session.room = None;
        }
        broadcast_views(registry);
        info!(%token, %room_id, "stopped spectating room");
        return Ok(());
    };
    vacate(registry, &room_id, seat);
    if let Some(session) = registry.sessions.get_mut(token) {
        session.room = None;
        session.seat = None;
    }
    reap_empty(registry);
    // The room's occupancy changed (or it was reclaimed and left the directory):
    // re-project to its remaining occupants and to everyone browsing.
    broadcast_views(registry);
    info!(%token, %room_id, seat, "left room");
    Ok(())
}

/// Handle `remove_ai` (issue #415): the **host** empties an AI-occupied seat again.
///
/// Host-only and pre-game, the counterpart of [`Lobby::add_ai`]: the sender must occupy
/// seat 0 ([`LobbyError::NotHost`]); the game must not have started
/// ([`LobbyError::GameStarted`]); the target `seat` must be in range
/// ([`LobbyError::SeatIndexOutOfRange`]) and currently hold an AI
/// ([`LobbyError::NotAiSeat`]). On success the AI and its gated deck are cleared, so the
/// seat is empty and joinable again.
pub(crate) fn remove_ai(
    registry: &mut Registry,
    token: &SessionToken,
    seat: u8,
) -> Result<(), LobbyError> {
    let (room_id, host_seat) = seat_of(registry, token)?;
    if host_seat != 0 {
        return Err(LobbyError::NotHost);
    }
    let room = registry
        .rooms
        .get_mut(&room_id)
        .ok_or(LobbyError::NotSeated)?;
    if room.game.is_some() {
        return Err(LobbyError::GameStarted);
    }
    let index = seat as usize;
    if index >= room.ai_seats.len() {
        return Err(LobbyError::SeatIndexOutOfRange(seat));
    }
    if room.ai_seats[index].is_none() {
        return Err(LobbyError::NotAiSeat(seat));
    }
    room.ai_seats[index] = None;
    if let Some(gate) = room.gate.get_mut(index) {
        *gate = SeatGate::default();
    }
    // The seat is empty and joinable again: re-project to occupants and browsers.
    broadcast_views(registry);
    info!(%token, %room_id, seat, "host removed an AI opponent");
    Ok(())
}

/// Handle `set_name`: validate the requested display name and store it on the
/// session (issue #294). On success the affected views are re-pushed — the sender's
/// own, and, if it is seated, the whole room roster so every occupant sees the new
/// name. On rejection the name is left untouched and a typed [`LobbyError::InvalidName`]
/// is returned; the caller re-sends the sender's current [`LobbyView`] unchanged (the
/// lobby's non-fatal error pattern).
pub(crate) fn set_name(
    registry: &mut Registry,
    token: &SessionToken,
    requested: &str,
) -> Result<(), LobbyError> {
    let name = validate_name(requested).map_err(LobbyError::InvalidName)?;
    let Some(session) = registry.sessions.get_mut(token) else {
        return Err(LobbyError::UnknownSession);
    };
    session.name = Some(name);
    // If the session is seated, its name appears in the shared roster, so re-project to
    // every occupant; otherwise only the sender's own view changed.
    match session.room.clone() {
        Some(room_id) => push_room(registry, &room_id),
        None => push_view(registry, token),
    }
    info!(%token, "connection set its display name");
    Ok(())
}
