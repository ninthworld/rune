//! The lobby seam: a fresh connection, a held seat, and reconnect by token.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::lobby::test_support::*;
use sage_protocol::Hello;

#[tokio::test]
async fn issue_367_a_lobby_connection_obtains_the_catalog_without_a_room_or_game() {
    // A fresh, roomless connection can browse the full catalog: every supported card
    // and every advertised format, with no room joined and no game constructed.
    let lobby = lobby(4);
    let alice = Client::connect(&lobby).await;
    assert!(alice.current().room.is_none(), "no room joined");
    assert!(!alice.started(), "no game constructed");

    let catalog = lobby.catalog();
    assert!(!catalog.cards.is_empty(), "the catalog lists cards");
    assert!(!catalog.formats.is_empty(), "the catalog lists formats");
    // It projects the whole bundled database.
    assert_eq!(catalog.cards.len(), CardDatabase::bundled().unwrap().len());

    // Routing the request through the registry is a harmless ack that changes no
    // lobby state — the connection stays roomless and no game starts (issue #367).
    lobby
        .command(&alice.token, LobbyCommand::RequestCatalog)
        .await
        .expect("request_catalog is accepted");
    assert!(alice.current().room.is_none());
    assert!(!alice.started());
}

#[tokio::test]
async fn a_new_connection_lands_in_the_lobby_with_a_session_and_no_game() {
    let lobby = lobby(4);
    let mut client = Client::connect(&lobby).await;
    let view = client.view().await;

    // Issued a session token and a public identity; not in any room.
    assert!(!view.session.is_empty());
    assert!(!view.you.is_empty());
    assert!(view.room.is_none());
    // Only the create/join/spectate commands are legal before a room exists.
    assert_eq!(
        view.valid_commands,
        vec![
            "set_name".to_string(),
            "create_room".to_string(),
            "join_room".to_string(),
            "spectate_room".to_string()
        ]
    );
}

#[tokio::test]
async fn a_dropped_connection_holds_its_seat_open_for_token_reconnect() {
    // Reconnect model (issue #113): a disconnect no longer vacates a seat or
    // reclaims the room — the seat is held so the token can return to it. Only an
    // explicit `Leave` vacates (covered by the reclamation tests below).
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
    let alice_you = alice.current().you.clone();
    let alice_token = alice.token.clone();

    // Alice's socket drops. Her seat is HELD open and the room is NOT reclaimed.
    lobby.disconnect(&alice.handle()).await;

    // A brand-new joiner takes the *other* seat — never Alice's held seat 0 — and
    // the room is proven to still exist (it was not reclaimed on her disconnect).
    let mut bob = Client::connect(&lobby).await;
    let _ = bob.view().await;
    lobby
        .command(
            &bob.token,
            LobbyCommand::JoinRoom(JoinRoom {
                room_id: room_id.clone(),
            }),
        )
        .await
        .expect("the held-open room still accepts a joiner");
    let bob_room = bob
        .view()
        .await
        .room
        .expect("bob joined the surviving room");
    assert_eq!(bob_room.room_id, room_id);
    assert_eq!(
        bob_room.seats[1].occupied_by.as_deref(),
        Some(bob.current().you.as_str())
    );
    assert_eq!(
        bob_room.seats[0].occupied_by.as_deref(),
        Some(alice_you.as_str()),
        "Alice's seat is still held while she is away",
    );

    // Alice reconnects with her token and is resynced into the *same* seat 0.
    let alice2 = Client::reconnect(&lobby, Some(alice_token)).await;
    let resumed = alice2.current().room.expect("alice reclaims her held room");
    assert_eq!(resumed.room_id, room_id);
    assert_eq!(
        resumed.seats[0].occupied_by.as_deref(),
        Some(alice_you.as_str())
    );
    assert_eq!(
        alice2.current().you,
        alice_you,
        "same identity across reconnect"
    );
}

/// Regression for issue #113, referencing issue #48 (the hidden-hand leak the
/// one-way seat retirement was guarding against). A held seat is handed back
/// **only** to the exact secret token that owns it: a returning stranger — no
/// token, a forged token, or another seat's *public* identity — never lands in
/// someone else's held seat. That is precisely what stops a reconnect from
/// leaking the private state a held seat guards (in game, the absent player's
/// hand and library; #48), and the token never resolves to a *different* seat.
#[tokio::test]
async fn issue_113_reconnect_token_never_leaks_a_held_seat_referencing_48() {
    let lobby = lobby(4);

    // Alice opens a room (seat 0); Bob joins (seat 1).
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
    let alice_you = alice.current().you.clone();
    let alice_token = alice.token.clone();

    let mut bob = Client::connect(&lobby).await;
    let _ = bob.view().await;
    lobby
        .command(
            &bob.token,
            LobbyCommand::JoinRoom(JoinRoom {
                room_id: room_id.clone(),
            }),
        )
        .await
        .unwrap();
    let _ = bob.view().await;
    let _ = alice.view().await;

    // Alice's socket drops; seat 0 is held open, still showing her as occupant.
    lobby.disconnect(&alice.handle()).await;

    // A stranger tries every value they might present: no token, a forged token,
    // and Alice's PUBLIC identity (which other seats legitimately see). None is
    // her secret session token, so none reclaims her seat — each yields a fresh,
    // roomless identity that never sees the inside of the room.
    for forged in [
        None,
        Some("s-forged-guess".to_string()),
        Some(alice_you.clone()),
    ] {
        let stranger = Client::reconnect(&lobby, forged).await;
        let view = stranger.current();
        assert!(view.room.is_none(), "a stranger never lands in a held seat");
        assert_ne!(
            view.session, alice_token,
            "a stranger is never handed Alice's secret token",
        );
        assert_ne!(
            view.you, alice_you,
            "a stranger gets its own identity, never Alice's seat",
        );
    }

    // Only the real secret token reclaims the seat — and always the SAME seat 0,
    // never Bob's seat 1.
    let alice2 = Client::reconnect(&lobby, Some(alice_token)).await;
    let resumed = alice2
        .current()
        .room
        .expect("the true token reclaims the seat");
    assert_eq!(resumed.room_id, room_id);
    assert_eq!(
        resumed.seats[0].occupied_by.as_deref(),
        Some(alice_you.as_str()),
        "reclaimed her own seat 0",
    );
    assert_ne!(
        resumed.seats[1].occupied_by.as_deref(),
        Some(alice_you.as_str()),
        "the token never grants a different seat",
    );
}

#[tokio::test]
async fn issue_113_a_new_connection_with_the_token_supersedes_the_old_one() {
    // Stale-duplicate handling: the new connection supersedes the old. Even while
    // the old connection is still nominally alive, presenting its token takes the
    // seat over, and the old connection's later teardown is a no-op (stale
    // generation) — so it cannot vacate the seat the new connection now holds.
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
    let _ = alice.view().await;
    let alice_token = alice.token.clone();
    let stale_handle = alice.handle();

    // A new connection presents the same token and reclaims the seat.
    let alice2 = Client::reconnect(&lobby, Some(alice_token.clone())).await;
    assert!(
        alice2.current().room.is_some(),
        "the superseding connection holds the seat",
    );
    assert_eq!(alice2.token, alice_token, "same session, new connection");

    // The OLD connection tears down: its generation is stale, so this is ignored
    // and the reclaimed session survives.
    lobby.disconnect(&stale_handle).await;

    // Proof the seat is still held for the new connection: it can reconnect again.
    let alice3 = Client::reconnect(&lobby, Some(alice_token)).await;
    assert!(
        alice3.current().room.is_some(),
        "the superseding connection still owns the held seat",
    );
}

#[tokio::test]
async fn hello_command_is_acknowledged_with_a_fresh_view() {
    // A `Hello` routed through `command` (rather than the serve loop's reconnect
    // path) is a harmless ack that re-sends the current view.
    let lobby = lobby(4);
    let mut alice = Client::connect(&lobby).await;
    let first = alice.view().await;
    lobby
        .command(&alice.token, LobbyCommand::Hello(Hello::default()))
        .await
        .expect("hello acknowledged");
    let again = alice.view().await;
    assert_eq!(again.session, first.session);
    assert!(again.room.is_none());
}

#[tokio::test]
async fn hello_without_a_token_keeps_the_fresh_identity() {
    // A first-contact Hello (no token) has nothing to reclaim: the connection
    // keeps the identity it was minted at connect and is re-sent its view.
    let lobby = lobby(4);
    let mut alice = Client::connect(&lobby).await;
    let first = alice.view().await;
    let handle = alice.handle();
    let after = lobby.hello(&handle, None).await;
    assert_eq!(after.token, first.session, "identity unchanged");
    assert_eq!(after.generation, handle.generation, "no supersede");
    let again = alice.view().await;
    assert_eq!(again.session, first.session);
    assert!(again.room.is_none());
}

#[tokio::test]
async fn hello_with_an_unknown_token_gets_a_clean_roomless_view() {
    // A token for a session/room that no longer exists (the "room gone" case)
    // never resolves to another seat: the connection keeps its fresh, roomless
    // identity rather than being routed anywhere.
    let lobby = lobby(4);
    let mut alice = Client::connect(&lobby).await;
    let first = alice.view().await;
    let handle = alice.handle();
    let after = lobby
        .hello(&handle, Some("s-does-not-exist".to_string()))
        .await;
    assert_eq!(
        after.token, first.session,
        "unknown token grants no other seat"
    );
    let again = alice.view().await;
    assert!(again.room.is_none(), "clean roomless lobby response");
}

#[tokio::test]
async fn a_command_from_an_unknown_session_is_rejected() {
    let lobby = lobby(4);
    assert_eq!(
        lobby
            .command(&"s-nope".to_string(), LobbyCommand::Leave)
            .await,
        Err(LobbyError::UnknownSession)
    );
}
