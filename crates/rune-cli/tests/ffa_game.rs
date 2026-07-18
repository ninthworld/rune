//! Full-game integration for a **4-player free-for-all** (issue #350).
//!
//! Four [`RuleBasedAgent`](rune_cli::RuleBasedAgent) policies play a complete, legal
//! free-for-all against a real layer-2 [`rune_server::Room`] seated for four players,
//! built from a pinned seed and bundled starter decks, until the engine declares a
//! single winner. This is the M5 exit-criterion proof for the multiplayer slice: it
//! exercises the integration seams that only a >2-seat game reaches — per-attacker
//! declarations routed across four seats (#341/#344/#345), elimination mid-game with
//! the room continuing (#342), per-seat view redaction, and the last-player-standing
//! result — none of which a unit test can prove.
//!
//! Like the two-player full-game test (`tests/agent_game.rs`) it drives the room task
//! directly rather than over a duplex socket, so the test can read every seat's
//! terminal `GameView` (and thus the winner, the losers, and each seat's eliminated
//! state) and record a deterministic transcript, which the socket loop does not
//! surface. The agents consume only the personalized [`GameView`] + `valid_actions`
//! (never any engine access), deciding with [`rune_cli::choose_action`] and filling
//! every slot — including the multiplayer `defend_<id>` defender choice — with
//! [`rune_cli::fill_answers`], exactly what the socket loop sends on the wire.
//!
//! Runtime: this is one seeded game with basic priority automation on, and completes
//! well under the per-test budget of the existing agent full-game suite, so it runs in
//! the normal `make check` / `cargo test --workspace` gate (not gated behind
//! `--ignored`). The driver caps its loop so a future policy bug surfaces as a bounded
//! failure rather than a hang.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashSet;

use rune_cli::{choose_action, fill_answers};
use rune_engine::{CardDatabase, CardId, FunctionalId, GameSetup, GameState, PlayerSetup};
use rune_protocol::{ChooseAction, ClientMessage, GameView};
use rune_server::{AutoPassPolicy, Room, RoomInput};
use tokio::sync::watch;

/// The six bundled cards the free-for-all decks are built from: five green creatures
/// and a Forest to cast them with — the same green starter list the two-player
/// full-game and CLI lobby tests use. Named by authored `functional_id` (ADR 0018 §3)
/// so the deck stays stable as the catalog grows.
const STARTER_CARDS: [&str; 6] = [
    "thornback_boar",
    "riverbank_otter",
    "emberfang_jackal",
    "stonehide_basilisk",
    "forest",
    "verdant_scout",
];

/// A 40-card green starter deck, resolved from the catalog by authored identity.
fn decklist(db: &CardDatabase) -> Vec<CardId> {
    (0..40)
        .map(|i| {
            let slug = FunctionalId::try_from(STARTER_CARDS[i % 6].to_string())
                .expect("a well-formed identity");
            db.card_id(&slug).expect("a bundled card")
        })
        .collect()
}

/// The seat index encoded in a `p{N}` player id (the form used throughout the view).
fn seat_of(player_id: &str) -> usize {
    player_id
        .strip_prefix('p')
        .and_then(|n| n.parse::<usize>().ok())
        .unwrap_or_else(|| panic!("player id `{player_id}` is not the expected p{{N}} form"))
}

/// The terminal outcome of a seeded four-player free-for-all: every seat's terminal
/// [`GameView`], the ordered `(seat, action_id)` transcript, and the seats that were
/// eliminated (observed as `eliminated` in a surviving seat's view) before the game
/// ended, in the order first observed.
struct FfaOutcome {
    /// Each seat's terminal view (all four carry the same `result`).
    terminals: [GameView; 4],
    /// The action-by-action transcript, enough to check reproducibility.
    transcript: Vec<(usize, String)>,
    /// Seats eliminated mid-game (game continued), in first-observed order.
    eliminations: Vec<usize>,
}

/// Drive a four-seat free-for-all [`Room`] to completion with the green starter deck on
/// every seat and basic priority automation on. Returns the [`FfaOutcome`].
///
/// The driver asserts the two live-game invariants of #342/#345 as it goes: an
/// eliminated seat is **never** offered actions again, and the eliminated state is
/// visible to the seats still in the game.
async fn play_seeded_ffa(seed: u64) -> FfaOutcome {
    let db = CardDatabase::bundled().expect("bundled cards");
    let deck = decklist(&db);
    let players: Vec<PlayerSetup> = (0..4).map(|_| PlayerSetup::new(deck.clone())).collect();
    let setup = GameSetup::new(players, seed);
    let state = GameState::new(&setup, &db).expect("valid setup");
    let (handle, task) = Room::new(state, db)
        .with_auto_pass(AutoPassPolicy::On)
        .spawn();

    // One outbox per seat; keep each receiver, hand the sender to the room.
    let (tx0, mut rx0) = watch::channel::<Option<GameView>>(None);
    let (tx1, mut rx1) = watch::channel::<Option<GameView>>(None);
    let (tx2, mut rx2) = watch::channel::<Option<GameView>>(None);
    let (tx3, mut rx3) = watch::channel::<Option<GameView>>(None);
    for (seat, outbox) in [(0, tx0), (1, tx1), (2, tx2), (3, tx3)] {
        assert!(handle.send(RoomInput::Join { seat, outbox }));
    }

    let mut transcript: Vec<(usize, String)> = Vec::new();
    let mut eliminations: Vec<usize> = Vec::new();
    let mut eliminated: HashSet<usize> = HashSet::new();

    // A generous cap so a policy bug surfaces as a bounded failure, never a hang.
    for _ in 0..4_000_000usize {
        let views = [
            rx0.borrow_and_update().clone(),
            rx1.borrow_and_update().clone(),
            rx2.borrow_and_update().clone(),
            rx3.borrow_and_update().clone(),
        ];

        // Record eliminations from whichever seats' views are current: a seat that a
        // surviving player sees as an eliminated opponent has left the game (#342/#345).
        for view in views.iter().flatten() {
            for opp in &view.opponents {
                if opp.eliminated {
                    let seat = seat_of(&opp.player_id);
                    if eliminated.insert(seat) {
                        eliminations.push(seat);
                    }
                }
            }
        }
        // An eliminated seat must never again be offered a decision (#342): its view's
        // `valid_actions` stays empty for the rest of the game.
        for (seat, view) in views.iter().enumerate() {
            if let Some(view) = view {
                if eliminated.contains(&seat) {
                    assert!(
                        view.valid_actions.is_empty(),
                        "eliminated seat {seat} was offered actions: {:?}",
                        view.valid_actions
                    );
                }
            }
        }

        // The game is over once every seat's view carries the terminal result.
        if views
            .iter()
            .all(|view| view.as_ref().is_some_and(|v| v.result.is_some()))
        {
            let terminals = views.map(|view| view.expect("a terminal view on every seat"));
            drop(handle);
            let _ = task.await;
            return FfaOutcome {
                terminals,
                transcript,
                eliminations,
            };
        }

        // Exactly one seat holds priority (is offered actions) at a time; it acts.
        let actor = views.iter().enumerate().find_map(|(seat, view)| {
            view.as_ref()
                .filter(|v| !v.valid_actions.is_empty())
                .map(|v| (seat, v.clone()))
        });
        if let Some((seat, view)) = actor {
            let action = choose_action(&view).expect("the agent always has a move");
            let targets = fill_answers(&view, action).expect("the agent fills every slot");
            transcript.push((seat, action.id.clone()));
            let choose = ChooseAction {
                action_id: action.id.clone(),
                token: action.token.clone(),
                targets,
            };
            assert!(handle.send(RoomInput::Message {
                seat,
                message: ClientMessage::ChooseAction(choose),
            }));
        }

        // Await the next broadcast on any seat (whether or not we just acted). Channel
        // closure is handled by the terminal check at the top of the next iteration.
        let alive = tokio::select! {
            r = rx0.changed() => r.is_ok(),
            r = rx1.changed() => r.is_ok(),
            r = rx2.changed() => r.is_ok(),
            r = rx3.changed() => r.is_ok(),
        };
        if !alive {
            break;
        }
    }

    panic!("the free-for-all did not reach a terminal state within the iteration cap");
}

/// The pinned seed for the deterministic four-player free-for-all. Chosen so the game
/// ends promptly with a decisive winner and at least one mid-game elimination — the two
/// things a >2-seat game must demonstrate that a duel cannot.
const FFA_SEED: u64 = 0x5EED_0350_FFA4_0001;

#[tokio::test]
async fn four_agents_play_a_free_for_all_to_a_single_winner_with_an_elimination() {
    let outcome = play_seeded_ffa(FFA_SEED).await;

    // Every seat's terminal view agrees on a single winner and three losers.
    for (seat, view) in outcome.terminals.iter().enumerate() {
        let result = view
            .result
            .as_ref()
            .unwrap_or_else(|| panic!("seat {seat} has no terminal result"));
        assert!(
            result.winner.is_some(),
            "seat {seat}: the free-for-all ends with a decisive winner (not a draw): {result:?}"
        );
        assert_eq!(
            result.losers.len(),
            3,
            "seat {seat}: exactly three losers in a four-player game: {result:?}"
        );
        assert!(
            view.valid_actions.is_empty(),
            "seat {seat}: a terminal view offers no actions"
        );
    }
    // All four seats saw the same outcome.
    let winner = outcome.terminals[0].result.as_ref().unwrap().winner.clone();
    for view in &outcome.terminals {
        assert_eq!(
            view.result.as_ref().unwrap().winner,
            winner,
            "every seat's view names the same winner"
        );
    }

    // At least one player was eliminated before the game ended, with the game
    // continuing past it (the multiplayer-only behavior this test exists to prove).
    assert!(
        !outcome.eliminations.is_empty(),
        "at least one player is eliminated before the game ends"
    );

    // The agents actually played a sequence of actions across seats.
    assert!(
        !outcome.transcript.is_empty(),
        "the agents played a sequence of actions"
    );
    let seats_that_acted: HashSet<usize> = outcome.transcript.iter().map(|(s, _)| *s).collect();
    assert!(
        seats_that_acted.len() >= 2,
        "more than one seat took decisions (routing reached multiple seats): {seats_that_acted:?}"
    );
}

#[tokio::test]
async fn the_free_for_all_is_deterministic_for_a_seed() {
    let first = play_seeded_ffa(FFA_SEED).await;
    let second = play_seeded_ffa(FFA_SEED).await;

    assert_eq!(
        first.transcript, second.transcript,
        "the same seed reproduces the same action-by-action transcript across four seats",
    );
    assert_eq!(
        first.eliminations, second.eliminations,
        "the same seed reproduces the same elimination order",
    );
    assert_eq!(
        first.terminals[0].result, second.terminals[0].result,
        "the same seed reproduces the same winner, losers, and reason",
    );
}
