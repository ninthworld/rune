//! The gate, exercised end to end: decks accepted and rejected, AI seats, and the
//! hand-off every seat gets when the last one readies.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::lobby::test_support::*;
use crate::test_support::fixture;

#[tokio::test]
async fn issue_415_host_fills_an_empty_seat_with_an_ai() {
    // The host seats a random AI in the open seat: the roster shows it AI-occupied,
    // decked, and ready, with no human occupant, and the host is now also offered
    // `remove_ai`.
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
    let _ = alice.view().await;

    lobby
        .command(&alice.token, add_random_ai(1))
        .await
        .expect("host seats an AI");
    let view = alice.current();
    let room = view.room.expect("alice in room");
    let ai_seat = &room.seats[1];
    assert_eq!(ai_seat.ai.as_deref(), Some("random"));
    assert!(ai_seat.occupied_by.is_none(), "an AI seat has no player id");
    assert!(
        ai_seat.decked && ai_seat.ready,
        "an AI seat is decked and ready"
    );
    assert_eq!(ai_seat.name.as_deref(), Some("Random"));
    // The room is now full, so `add_ai` is no longer offered, but `remove_ai` is.
    assert!(view.valid_commands.contains(&"remove_ai".to_string()));
    assert!(!view.valid_commands.contains(&"add_ai".to_string()));
    // No game yet — alice has not decked/readied her own seat.
    assert!(!alice.started());
}

#[tokio::test]
async fn issue_415_only_the_host_may_add_or_remove_ai() {
    // In a 3-seat free-for-all, alice hosts (seat 0), bob joins (seat 1); seat 2 is
    // open. A non-host (bob) may not seat or remove an AI.
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
        .expect("alice creates a 3-seat FFA");
    let room_id = alice.view().await.room.expect("alice in room").room_id;
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
        .expect("bob joins");
    let _ = bob.view().await;

    // Bob is not the host, so neither AI command is even offered to him, and issuing
    // one is a typed rejection.
    assert!(!bob.current().valid_commands.contains(&"add_ai".to_string()));
    assert_eq!(
        lobby.command(&bob.token, add_random_ai(2)).await,
        Err(LobbyError::NotHost)
    );
    assert_eq!(
        lobby
            .command(
                &bob.token,
                LobbyCommand::RemoveAi(sage_protocol::RemoveAi { seat: 2 })
            )
            .await,
        Err(LobbyError::NotHost)
    );
}

#[tokio::test]
async fn issue_415_add_ai_validates_kind_seat_and_occupancy() {
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
    let _ = alice.view().await;

    // An unknown AI kind is rejected and seats nothing.
    assert_eq!(
        lobby
            .command(
                &alice.token,
                LobbyCommand::AddAi(sage_protocol::AddAi {
                    seat: 1,
                    kind: "sentient_singularity".to_string(),
                    cards: decklist(),
                    commander: None,
                })
            )
            .await,
        Err(LobbyError::UnknownAiKind(
            "sentient_singularity".to_string()
        ))
    );
    // A seat index past the room's seat range is rejected.
    assert_eq!(
        lobby.command(&alice.token, add_random_ai(9)).await,
        Err(LobbyError::SeatIndexOutOfRange(9))
    );
    // The host's own occupied seat cannot be overwritten with an AI.
    assert_eq!(
        lobby.command(&alice.token, add_random_ai(0)).await,
        Err(LobbyError::SeatOccupied(0))
    );
    // And an AI seat cannot be doubly filled.
    lobby
        .command(&alice.token, add_random_ai(1))
        .await
        .expect("first AI seats");
    assert_eq!(
        lobby.command(&alice.token, add_random_ai(1)).await,
        Err(LobbyError::SeatOccupied(1))
    );
    assert_eq!(
        alice.current().room.unwrap().seats[1].ai.as_deref(),
        Some("random")
    );
}

#[tokio::test]
async fn issue_415_add_ai_rejects_an_illegal_deck_and_seats_nothing() {
    // The AI's deck is validated against the room's format exactly like a human's: a
    // too-small deck for `starter-1v1` is rejected, and the seat stays empty.
    let lobby = lobby(4);
    let mut alice = Client::connect(&lobby).await;
    let _ = alice.view().await;
    lobby
        .command(
            &alice.token,
            LobbyCommand::CreateRoom(CreateRoom {
                config: config_with(2, "starter-1v1"),
            }),
        )
        .await
        .expect("alice creates");
    let _ = alice.view().await;

    let err = lobby
        .command(
            &alice.token,
            LobbyCommand::AddAi(sage_protocol::AddAi {
                seat: 1,
                kind: "random".to_string(),
                cards: vec![wire_id("forest"); 10],
                commander: None,
            }),
        )
        .await
        .expect_err("an under-minimum AI deck is rejected");
    assert_eq!(
        err,
        LobbyError::IllegalDeck(DeckError::BelowMinimum { have: 10, min: 40 })
    );
    assert!(alice.current().room.unwrap().seats[1].ai.is_none());
}

#[tokio::test]
async fn issue_415_a_human_joiner_never_lands_in_an_ai_seat() {
    // Alice hosts a 3-seat FFA and seats an AI in seat 1; a joiner takes seat 2, never
    // the AI's seat 1.
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
        .expect("alice creates a 3-seat FFA");
    let room_id = alice.view().await.room.expect("alice in room").room_id;
    lobby
        .command(&alice.token, add_random_ai(1))
        .await
        .expect("host seats an AI in seat 1");
    let _ = alice.view().await;

    let mut bob = Client::connect(&lobby).await;
    let _ = bob.view().await;
    lobby
        .command(&bob.token, LobbyCommand::JoinRoom(JoinRoom { room_id }))
        .await
        .expect("bob joins");
    let room = bob.view().await.room.expect("bob in room");
    assert_eq!(
        room.seats[1].ai.as_deref(),
        Some("random"),
        "seat 1 is still the AI"
    );
    assert_eq!(
        room.seats[2].occupied_by.as_deref(),
        Some(bob.current().you.as_str()),
        "the joiner took the open seat 2, not the AI seat",
    );
}

#[tokio::test]
async fn issue_415_remove_ai_empties_the_seat_again() {
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
    let _ = alice.view().await;
    lobby
        .command(&alice.token, add_random_ai(1))
        .await
        .expect("seats an AI");
    assert_eq!(
        alice.current().room.unwrap().seats[1].ai.as_deref(),
        Some("random")
    );

    // Removing a non-AI seat is a typed error; removing the AI empties it.
    assert_eq!(
        lobby
            .command(
                &alice.token,
                LobbyCommand::RemoveAi(sage_protocol::RemoveAi { seat: 0 })
            )
            .await,
        Err(LobbyError::NotAiSeat(0))
    );
    lobby
        .command(
            &alice.token,
            LobbyCommand::RemoveAi(sage_protocol::RemoveAi { seat: 1 }),
        )
        .await
        .expect("removes the AI");
    let view = alice.current();
    assert!(
        view.room.unwrap().seats[1].ai.is_none(),
        "seat 1 is empty again"
    );
    // With an open seat once more, `add_ai` is offered again and `remove_ai` is not.
    assert!(view.valid_commands.contains(&"add_ai".to_string()));
    assert!(!view.valid_commands.contains(&"remove_ai".to_string()));
}

#[tokio::test]
async fn issue_415_a_human_plus_an_ai_starts_and_the_ai_drives_its_own_seat() {
    // The end-to-end proof that the AI *plays*: a 1-human + 1-AI game starts, and the
    // AI seat's own driver keeps at the mulligan on its own — the human (seat 0) only
    // ever answers its own decisions, yet the game advances past the pre-game mulligan
    // into turn 1, which is possible only if the AI (seat 1) also acted.
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
    // Seating the AI fills the last seat, decked and ready, so the gate passes and the
    // game is constructed with an AI driver spawned for seat 1.
    lobby
        .command(&alice.token, add_random_ai(1))
        .await
        .expect("host seats the AI and the game starts");
    assert!(alice.started(), "the game started");

    // Join seat 0 (the human) and drive only its own mulligan keep. The AI drives seat
    // 1 independently in its spawned task.
    let handle = alice.start_handle().expect("game constructed");
    let (tx, mut rx) = watch::channel::<Option<sage_protocol::GameView>>(None);
    assert!(handle.send(crate::RoomInput::Join {
        seat: 0,
        outbox: tx
    }));

    // Answer seat 0's mulligan keep, and watch for the game to progress past the
    // mulligan — a play_land / pass / cast action on offer means turn 1 has begun,
    // which requires the AI seat to have kept too.
    let reached_turn_one = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let view = {
                let latest = rx.borrow_and_update().clone();
                match latest {
                    Some(view) => view,
                    None => {
                        rx.changed().await.expect("a view is pushed");
                        continue;
                    }
                }
            };
            // A post-mulligan actionable view offers ordinary priority actions.
            if view.valid_actions.iter().any(|a| {
                matches!(
                    a.kind.as_str(),
                    "play_land" | "pass_priority" | "cast_spell"
                )
            }) {
                return true;
            }
            // Otherwise answer our own mulligan keep, if that is what is on offer.
            if let Some(decision) = view
                .valid_actions
                .iter()
                .find(|a| a.kind == "mulligan_decision")
            {
                handle.send(crate::RoomInput::Message {
                    seat: 0,
                    message: sage_protocol::ClientMessage::ChooseAction(
                        sage_protocol::ChooseAction {
                            submission: String::new(),
                            action_id: decision.id.clone(),
                            token: decision.token.clone(),
                            targets: vec![sage_protocol::TargetChoice {
                                slot: "decision".to_string(),
                                chosen: vec!["keep".to_string()],
                            }],
                        },
                    ),
                });
            }
            rx.changed().await.expect("the game advances");
        }
    })
    .await
    .expect("the game reaches turn 1 without stalling on the AI's mulligan");
    assert!(reached_turn_one, "turn 1 began, so the AI kept on its own");
}

#[tokio::test]
async fn a_pinned_starting_life_overrides_the_format_default() {
    // Seat 0 sees seat 1 (its only opponent) start at the pinned life, not the
    // format's 20 — proof the override reaches game construction (issue #145).
    let view = first_game_view_for(Some(0xABCD), Some(4)).await;
    let opponent_life = view.opponents.first().expect("one opponent").life;
    assert_eq!(opponent_life, 4, "the starting-life override applied");
}

#[tokio::test]
async fn a_pinned_seed_reproduces_the_same_opening_hand() {
    // Same override → identical shuffle (ADR 0006), so the opening hand matches.
    let first = opening_hand_names_for_seed(Some(0xC0FF_EE00_1234_5678)).await;
    let again = opening_hand_names_for_seed(Some(0xC0FF_EE00_1234_5678)).await;
    assert!(!first.is_empty(), "the opening hand is non-empty");
    assert_eq!(first, again, "a pinned seed reproduces the opening hand");

    // A different pinned seed diverges (the shuffle actually depends on it).
    let other = opening_hand_names_for_seed(Some(0x1111_2222_3333_4444)).await;
    assert_ne!(
        first, other,
        "a different seed shuffles to a different opening hand"
    );
}

#[tokio::test]
async fn submit_deck_with_an_unknown_card_is_rejected_and_seat_stays_undecked() {
    let lobby = lobby(4);
    let (alice, _bob, _room) = seated_pair(&lobby).await;

    // A non-existent id (bundled db holds only 1..=6) rejects the whole list.
    let err = lobby
        .command(
            &alice.token,
            LobbyCommand::SubmitDeck(SubmitDeck {
                cards: vec![wire_id("forest"), "no_such_card".to_string()],
                commander: None,
            }),
        )
        .await
        .expect_err("unknown card id is rejected");
    assert_eq!(err, LobbyError::UnknownCard("no_such_card".to_string()));
    // The seat stays undecked; the rejection re-sent the current view.
    assert!(!alice.current().room.expect("in room").seats[0].decked);
}

#[tokio::test]
async fn submit_deck_under_the_minimum_size_is_rejected_and_seat_stays_undecked() {
    // The seeded format requires 40 cards; a ten-card deck of known
    // ids is rejected as illegal, and the seat is left undecked.
    let lobby = lobby(4);
    let (alice, _bob, _room) = seated_pair_in(&lobby, "starter-1v1").await;

    let err = lobby
        .command(
            &alice.token,
            LobbyCommand::SubmitDeck(SubmitDeck {
                cards: vec![wire_id("forest"); 10],
                commander: None,
            }),
        )
        .await
        .expect_err("an under-minimum deck is rejected");
    assert_eq!(
        err,
        LobbyError::IllegalDeck(DeckError::BelowMinimum { have: 10, min: 40 })
    );
    assert!(!alice.current().room.expect("in room").seats[0].decked);
}

#[tokio::test]
async fn submit_deck_over_the_copy_limit_for_a_non_basic_is_rejected() {
    // Five copies of a non-basic (id 1) in an otherwise legal 40-card deck exceed
    // the four-copy limit; the deck is rejected and stays out.
    let lobby = lobby(4);
    let (alice, _bob, _room) = seated_pair_in(&lobby, "starter-1v1").await;

    let mut cards = vec![wire_id("onakke_ogre"); 5];
    for slug in &NON_BASICS[1..] {
        for _ in 0..4 {
            cards.push(wire_id(slug));
        }
    }
    for _ in 0..19 {
        cards.push(wire_id("forest"));
    }
    assert_eq!(cards.len(), 40);

    let err = lobby
        .command(
            &alice.token,
            LobbyCommand::SubmitDeck(SubmitDeck {
                cards,
                commander: None,
            }),
        )
        .await
        .expect_err("an over-copy-limit deck is rejected");
    assert_eq!(
        err,
        LobbyError::IllegalDeck(DeckError::CopyLimit {
            card: fixture("onakke_ogre"),
            count: 5,
            limit: 4,
        })
    );
    assert!(!alice.current().room.expect("in room").seats[0].decked);
}

#[tokio::test]
async fn submit_deck_accepts_a_legal_deck_with_many_basics() {
    // The shared `decklist()` holds twenty basic Forests, far over the
    // four-copy limit, yet basics are exempt: the deck is accepted.
    let lobby = lobby(4);
    let (alice, _bob, _room) = seated_pair_in(&lobby, "starter-1v1").await;

    lobby
        .command(
            &alice.token,
            LobbyCommand::SubmitDeck(SubmitDeck {
                cards: decklist(),
                commander: None,
            }),
        )
        .await
        .expect("a legal deck with many basics is accepted");
    assert!(alice.current().room.expect("in room").seats[0].decked);
}

#[tokio::test]
async fn readying_up_requires_a_submitted_deck() {
    let lobby = lobby(4);
    let (alice, _bob, _room) = seated_pair(&lobby).await;

    // Ready before decking is a typed error; the seat stays unready.
    assert_eq!(
        lobby
            .command(&alice.token, LobbyCommand::Ready(Ready { ready: true }))
            .await,
        Err(LobbyError::NotDecked)
    );
    assert!(!alice.current().room.expect("in room").seats[0].ready);
}

#[tokio::test]
async fn start_is_blocked_while_a_seat_is_undecked() {
    let lobby = lobby(4);
    let (alice, bob, _room) = seated_pair(&lobby).await;
    // Only alice decks and readies; bob never submits a deck.
    submit_valid_deck(&lobby, &alice).await;
    lobby
        .command(&alice.token, LobbyCommand::Ready(Ready { ready: true }))
        .await
        .expect("alice readies");

    // The gate cannot pass with bob undecked: no game is constructed.
    assert!(!alice.started() && !bob.started());
    assert!(alice.current().room.expect("in room").seats[0].ready);
}

#[tokio::test]
async fn start_is_blocked_while_a_seat_is_unready() {
    let lobby = lobby(4);
    let (alice, bob, _room) = seated_pair(&lobby).await;
    // Both deck; only alice readies.
    submit_valid_deck(&lobby, &alice).await;
    submit_valid_deck(&lobby, &bob).await;
    lobby
        .command(&alice.token, LobbyCommand::Ready(Ready { ready: true }))
        .await
        .expect("alice readies");

    // Bob is decked but unready: the gate stays shut.
    assert!(!alice.started() && !bob.started());
}

#[tokio::test]
async fn last_seat_readying_constructs_the_game_and_hands_off_every_seat() {
    let lobby = lobby(4);
    let (alice, bob, _room) = seated_pair(&lobby).await;
    submit_valid_deck(&lobby, &alice).await;
    submit_valid_deck(&lobby, &bob).await;

    // Alice readies first — not enough; the gate needs every seat.
    lobby
        .command(&alice.token, LobbyCommand::Ready(Ready { ready: true }))
        .await
        .expect("alice readies");
    assert!(!alice.started() && !bob.started());

    // Bob readies last: the gate passes and both seats are handed off to a game.
    // The terminal `Start` supersedes the roster push in each latest-value outbox.
    lobby
        .command(&bob.token, LobbyCommand::Ready(Ready { ready: true }))
        .await
        .expect("bob readies");

    assert_eq!(alice.start_seat(), Some(0));
    assert_eq!(bob.start_seat(), Some(1));

    // Post-start, further lobby commands to the started room are rejected.
    assert_eq!(
        lobby
            .command(&alice.token, LobbyCommand::Ready(Ready { ready: false }))
            .await,
        Err(LobbyError::GameStarted)
    );
}

#[tokio::test]
async fn issue_349_three_seat_free_for_all_starts_a_three_player_game() {
    // Creating a 3-seat free-for-all room, decking and readying every seat, starts
    // an engine game seating that many players (the FFA-format acceptance).
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
        .expect("alice creates a 3-seat FFA room");
    let room_id = alice.view().await.room.expect("alice in room").room_id;
    assert_eq!(
        alice.current().room.unwrap().seats.len(),
        3,
        "the room has three seats"
    );

    // Two more players join.
    let mut others = Vec::new();
    for _ in 0..2 {
        let mut client = Client::connect(&lobby).await;
        let _ = client.view().await;
        lobby
            .command(
                &client.token,
                LobbyCommand::JoinRoom(JoinRoom {
                    room_id: room_id.clone(),
                }),
            )
            .await
            .expect("player joins the FFA room");
        let _ = client.view().await;
        others.push(client);
    }
    let _ = alice.view().await;

    // Every seat decks and readies; the game starts only once all three are in.
    submit_valid_deck(&lobby, &alice).await;
    for client in &others {
        submit_valid_deck(&lobby, client).await;
    }
    lobby
        .command(&alice.token, LobbyCommand::Ready(Ready { ready: true }))
        .await
        .expect("alice readies");
    for client in &others {
        lobby
            .command(&client.token, LobbyCommand::Ready(Ready { ready: true }))
            .await
            .expect("player readies");
    }

    // All three seats are handed off to a running game, one per seat index.
    assert_eq!(alice.start_seat(), Some(0));
    assert_eq!(others[0].start_seat(), Some(1));
    assert_eq!(others[1].start_seat(), Some(2));
}

#[tokio::test]
async fn player_names_project_into_the_game_view_at_game_start() {
    // Issue #294: names set in the lobby reach the constructed game, keyed by the
    // `p{N}` player id, so every in-game surface can label players.
    let lobby = lobby(4);
    let (alice, bob, _room) = seated_pair(&lobby).await;
    lobby
        .command(
            &alice.token,
            LobbyCommand::SetName(SetName {
                name: "Alice".into(),
            }),
        )
        .await
        .expect("alice names herself");
    lobby
        .command(
            &bob.token,
            LobbyCommand::SetName(SetName { name: "Bob".into() }),
        )
        .await
        .expect("bob names himself");
    submit_valid_deck(&lobby, &alice).await;
    submit_valid_deck(&lobby, &bob).await;
    lobby
        .command(&alice.token, LobbyCommand::Ready(Ready { ready: true }))
        .await
        .expect("alice readies");
    lobby
        .command(&bob.token, LobbyCommand::Ready(Ready { ready: true }))
        .await
        .expect("bob readies");

    // Join seat 0's constructed game and read its first personalized GameView.
    let handle = alice.start_handle().expect("game constructed");
    let (tx, mut rx) = watch::channel::<Option<sage_protocol::GameView>>(None);
    assert!(handle.send(crate::RoomInput::Join {
        seat: 0,
        outbox: tx
    }));
    let view = loop {
        if let Some(view) = rx.borrow_and_update().clone() {
            break view;
        }
        rx.changed().await.expect("first GameView is pushed");
    };
    assert_eq!(
        view.player_names.get("p0").map(String::as_str),
        Some("Alice")
    );
    assert_eq!(view.player_names.get("p1").map(String::as_str), Some("Bob"));
}
