//! Membership: joining by id, the typed rejections, spectating, and what survives a
//! reconnect.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::lobby::test_support::*;

#[tokio::test]
async fn issue_351_spectating_a_gathering_room_is_rejected_non_fatally() {
    // A room that has not started has no board to watch: spectate is rejected with
    // the lobby's non-fatal error, and the would-be spectator stays roomless.
    let lobby = Lobby::bundled_with_overrides(8, None, None).expect("bundled cards");
    let (alice, _bob, room_id) = seated_pair(&lobby).await; // a gathering room
    let mut carol = Client::connect(&lobby).await;
    let _ = carol.view().await;

    let err = lobby
        .command(
            &carol.token,
            LobbyCommand::SpectateRoom(SpectateRoom {
                room_id: room_id.clone(),
            }),
        )
        .await
        .expect_err("spectating a gathering room is rejected");
    assert_eq!(err, LobbyError::RoomNotStarted);
    // Carol is still roomless (no spectate hand-off, no seat).
    assert!(carol.spectate_handle().is_none());
    assert!(carol.current().room.is_none());
    // The seated player is unaffected.
    assert!(!alice.started());
}

#[tokio::test]
async fn join_by_id_seats_the_joiner_and_updates_every_roster() {
    let lobby = lobby(4);
    let mut alice = Client::connect(&lobby).await;
    let _ = alice.view().await;
    lobby
        .command(
            &alice.token,
            LobbyCommand::CreateRoom(CreateRoom { config: config(2) }),
        )
        .await
        .expect("alice creates");
    let alice_room = alice.view().await.room.expect("alice in room");
    let room_id = alice_room.room_id.clone();

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
        .expect("bob joins by id");

    // Bob is seated at seat 1 of the same room.
    let bob_room = bob.view().await.room.expect("bob in room");
    assert_eq!(bob_room.room_id, room_id);
    assert_eq!(
        bob_room.seats[1].occupied_by.as_deref(),
        Some(bob.current().you.as_str())
    );

    // Alice was pushed an updated roster showing both seats filled.
    let alice_after = alice.view().await.room.expect("alice still in room");
    assert!(alice_after.seats[0].occupied_by.is_some());
    assert!(alice_after.seats[1].occupied_by.is_some());
}

#[tokio::test]
async fn joining_an_unknown_room_is_a_typed_error() {
    let lobby = lobby(4);
    let mut bob = Client::connect(&lobby).await;
    let _ = bob.view().await;
    let err = lobby
        .command(
            &bob.token,
            LobbyCommand::JoinRoom(JoinRoom {
                room_id: "r-nope".to_string(),
            }),
        )
        .await
        .expect_err("unknown room id is rejected");
    assert_eq!(err, LobbyError::UnknownRoom);
    assert!(bob.current().room.is_none());
}

#[tokio::test]
async fn joining_a_full_room_is_a_typed_error() {
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
        .expect("bob fills the second seat");
    let _ = bob.view().await;

    // The two-seat room is full: a third joiner is refused and stays roomless.
    let mut carol = Client::connect(&lobby).await;
    let _ = carol.view().await;
    let err = lobby
        .command(&carol.token, LobbyCommand::JoinRoom(JoinRoom { room_id }))
        .await
        .expect_err("a full room is rejected");
    assert_eq!(err, LobbyError::RoomFull);
    assert!(carol.current().room.is_none());
}

#[tokio::test]
async fn create_or_join_while_already_in_a_room_is_rejected() {
    let lobby = lobby(4);
    let mut alice = Client::connect(&lobby).await;
    let _ = alice.view().await;
    lobby
        .command(
            &alice.token,
            LobbyCommand::CreateRoom(CreateRoom {
                config: config_with(3, "standard_ffa"),
            }),
        )
        .await
        .unwrap();
    let room_id = alice.view().await.room.unwrap().room_id;

    // A second create while seated is rejected.
    assert_eq!(
        lobby
            .command(
                &alice.token,
                LobbyCommand::CreateRoom(CreateRoom { config: config(2) })
            )
            .await,
        Err(LobbyError::AlreadyInRoom)
    );
    // As is a join while seated.
    assert_eq!(
        lobby
            .command(&alice.token, LobbyCommand::JoinRoom(JoinRoom { room_id }))
            .await,
        Err(LobbyError::AlreadyInRoom)
    );
}

#[tokio::test]
async fn a_display_name_survives_a_reconnect() {
    // Issue #294: the name is bound to the session, so a per-tab reconnect (echoing
    // the session token) is reunited with the same name.
    let lobby = lobby(4);
    let (alice, _bob, _room) = seated_pair(&lobby).await;
    lobby
        .command(
            &alice.token,
            LobbyCommand::SetName(SetName {
                name: "Alice".into(),
            }),
        )
        .await
        .expect("name accepted");

    // Drop the connection (the seated session is held open) and reconnect by token.
    lobby.disconnect(&alice.handle()).await;
    let mut returning = Client::reconnect(&lobby, Some(alice.token.clone())).await;
    let resumed = returning.view().await;
    assert_eq!(
        resumed.name.as_deref(),
        Some("Alice"),
        "name survived reconnect"
    );
    let room = resumed.room.expect("reclaimed the held seat");
    assert_eq!(room.seats[0].name.as_deref(), Some("Alice"));
}

#[tokio::test]
async fn issue_628_a_reconnect_into_a_live_game_is_handed_back_to_it() {
    // A seat is held open across a disconnect and its game keeps playing, so the
    // connection that proves it owns that seat has to be put back on the in-game
    // contract. Answering it with a lobby view cannot work — `push_view` deliberately
    // sends a started seat nothing — which left a reconnecting player on a silent socket
    // with no way back into their own match.
    let lobby = Lobby::bundled_with_overrides(4, Some(0x5EED), None).expect("bundled cards");
    let mut alice = Client::connect(&lobby).await;
    let _ = alice.view().await;
    lobby
        .command(
            &alice.token,
            LobbyCommand::CreateRoom(CreateRoom { config: config(2) }),
        )
        .await
        .expect("alice creates");
    let _ = alice.view().await;
    submit_valid_deck(&lobby, &alice).await;
    lobby
        .command(&alice.token, LobbyCommand::Ready(Ready { ready: true }))
        .await
        .expect("alice readies");
    lobby
        .command(&alice.token, add_random_ai(1))
        .await
        .expect("host seats the AI and the game starts");
    assert!(alice.started(), "the game started");

    // The socket drops. The seat is held, not vacated.
    lobby.disconnect(&alice.handle()).await;

    // A new socket presents the token. It is handed the same room and the same seat.
    let returning = Client::reconnect(&lobby, Some(alice.token.clone())).await;
    assert_eq!(returning.token, alice.token, "reclaimed the held session");
    assert!(
        returning.started(),
        "a reconnect into a live game is a hand-off, not a lobby view"
    );
    assert_eq!(returning.start_seat(), Some(0));

    // And the hand-off is usable: joining brings the seat current with a whole view,
    // which is the entire content of "resume" (the complete-view principle).
    let handle = returning.start_handle().expect("the running room");
    let (tx, mut rx) = watch::channel::<Option<sage_protocol::GameView>>(None);
    assert!(handle.send(crate::RoomInput::Join {
        seat: 0,
        outbox: tx
    }));
    let resumed = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let Some(view) = rx.borrow_and_update().clone() {
                return view;
            }
            rx.changed().await.expect("the room answers a join");
        }
    })
    .await
    .expect("the reconnected seat is brought current");
    assert_eq!(resumed.you, "p0");
}
