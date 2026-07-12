//! Full-game integration for the CLI **rule-based agent** (issue #159).
//!
//! Two [`RuleBasedAgent`](rune_cli::RuleBasedAgent) policies play a complete, legal
//! game against a real layer-2 [`rune_server::Room`] built from starter decks and a
//! pinned seed, until the engine declares a winner. The agents consume only the
//! personalized [`GameView`] + `valid_actions` (never any engine access), deciding
//! with [`rune_cli::choose_action`] and filling every slot with
//! [`rune_cli::fill_answers`] — exactly what the socket loop
//! ([`run_agent_session`](rune_cli::run_agent_session)) sends on the wire, minus the
//! WebSocket plumbing already covered by `tests/agent.rs`.
//!
//! Driving the room directly (rather than over a duplex socket) lets the test read
//! the terminal `GameView` — and thus the winner — and record a deterministic
//! transcript, which the socket loop does not surface. It exercises the real engine,
//! the real `resolve_action` binding of every prompt/requirement, and the real
//! terminal-result projection.
//!
//! Each game is an in-process room (no server binary) and runs in a fraction of a
//! second with a fully deterministic, priority-driven transcript, so both tests run
//! in the normal `make check` suite. The driver caps its loop so a future policy bug
//! surfaces as a bounded failure rather than a hang.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rune_cli::{choose_action, fill_answers};
use rune_engine::{CardDatabase, CardId, FunctionalId, GameSetup, GameState};
use rune_protocol::{ChooseAction, ClientMessage, GameView};
use rune_server::{Room, RoomInput};
use tokio::sync::watch;

/// A 40-card starter deck over the bundled ids 1..=6 (green creatures + Forest), the
/// same list the engine's mulligan and the CLI lobby tests use.
/// The six bundled cards these decks are built from: five green creatures and a Forest
/// to cast them with. Named by authored `functional_id` (ADR 0018 §3) — a `CardId` is
/// interned from the catalog's sort order, so an integer deck would silently become a
/// different (and, with no land in it, unplayable) deck the next time a card is added.
const STARTER_CARDS: [&str; 6] = [
    "thornback_boar",
    "riverbank_otter",
    "emberfang_jackal",
    "stonehide_basilisk",
    "forest",
    "verdant_scout",
];

/// A 40-card starter deck (green creatures + Forest), resolved from the catalog by
/// authored identity.
fn decklist(db: &CardDatabase) -> Vec<CardId> {
    (0..40)
        .map(|i| {
            let slug = FunctionalId::try_from(STARTER_CARDS[i % 6].to_string())
                .expect("a well-formed identity");
            db.card_id(&slug).expect("a bundled card")
        })
        .collect()
}

/// Drive both seats with the rule-based agent against a real seeded [`Room`] until
/// the game ends. Returns the terminal [`GameView`] (carrying the result) and the
/// ordered transcript of the `(seat, action_id)` each agent chose — enough to check
/// both completion and reproducibility.
async fn play_seeded_game(seed: u64) -> (GameView, Vec<(usize, String)>) {
    let db = CardDatabase::bundled().expect("bundled cards");
    let setup = GameSetup::two_player(decklist(&db), decklist(&db), seed);
    let state = GameState::new(&setup, &db).expect("valid setup");
    let (handle, task) = Room::new(state, db).spawn();

    let (tx0, mut rx0) = watch::channel::<Option<GameView>>(None);
    let (tx1, mut rx1) = watch::channel::<Option<GameView>>(None);
    assert!(handle.send(RoomInput::Join {
        seat: 0,
        outbox: tx0,
    }));
    assert!(handle.send(RoomInput::Join {
        seat: 1,
        outbox: tx1,
    }));

    let mut transcript: Vec<(usize, String)> = Vec::new();
    let mut terminal: Option<GameView> = None;

    // A generous cap so a policy bug surfaces as a bounded failure, never a hang.
    for _ in 0..2_000_000usize {
        let v0 = rx0.borrow_and_update().clone();
        let v1 = rx1.borrow_and_update().clone();

        // The game is over once either seat's view carries a terminal result.
        if let Some(view) = v0.as_ref().or(v1.as_ref()) {
            if view.result.is_some() {
                terminal = Some(view.clone());
                break;
            }
        }

        // Exactly one seat holds priority (is offered actions) at a time; it acts.
        let actor = if v0.as_ref().is_some_and(|v| !v.valid_actions.is_empty()) {
            v0.map(|view| (0usize, view))
        } else if v1.as_ref().is_some_and(|v| !v.valid_actions.is_empty()) {
            v1.map(|view| (1usize, view))
        } else {
            None
        };

        match actor {
            Some((seat, view)) => {
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
                // Wait for the resulting broadcast; channel closure is handled by the
                // terminal check at the top of the next iteration.
                tokio::select! {
                    _ = rx0.changed() => {}
                    _ = rx1.changed() => {}
                }
            }
            None => {
                // Nobody can act yet (initial join or a transient no-priority frame):
                // await the next view.
                tokio::select! {
                    r0 = rx0.changed() => { if r0.is_err() { break; } }
                    r1 = rx1.changed() => { if r1.is_err() { break; } }
                }
            }
        }
    }

    drop(handle);
    let _ = task.await;
    (
        terminal.expect("the game reached a terminal state within the iteration cap"),
        transcript,
    )
}

#[tokio::test]
async fn two_rule_based_agents_finish_a_game_with_a_winner() {
    let (terminal, transcript) = play_seeded_game(0x5EED_1234_ABCD_0001).await;

    let result = terminal.result.expect("a finished game carries a result");
    assert!(
        result.winner.is_some(),
        "the game ends with a decisive winner (not a draw): {result:?}"
    );
    assert_eq!(result.losers.len(), 1, "exactly one loser: {result:?}");
    assert!(
        terminal.valid_actions.is_empty(),
        "a terminal view offers no actions"
    );
    assert!(
        !transcript.is_empty(),
        "the agents actually played a sequence of actions"
    );
}

#[tokio::test]
async fn agent_vs_agent_with_the_same_seed_reproduces_the_same_game() {
    let seed = 0x5EED_1234_ABCD_0002;
    let (first_terminal, first_transcript) = play_seeded_game(seed).await;
    let (second_terminal, second_transcript) = play_seeded_game(seed).await;

    assert_eq!(
        first_transcript, second_transcript,
        "the same seed reproduces the same action-by-action transcript",
    );
    assert_eq!(
        first_terminal.result, second_terminal.result,
        "the same seed reproduces the same winner, losers, and reason",
    );
}
