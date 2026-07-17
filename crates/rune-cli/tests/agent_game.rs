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

use std::collections::{HashMap, HashSet};

use rune_cli::{choose_action, fill_answers};
use rune_engine::{
    Ability, CardDatabase, CardId, CardType, Color, Effect, FunctionalId, GameSetup, GameState,
    Supertype,
};
use rune_protocol::{ChooseAction, ClientMessage, GameView};
use rune_server::{AutoPassPolicy, Room, RoomInput};
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
    let deck = decklist(&db);
    play_seeded_game_with(seed, deck.clone(), deck, AutoPassPolicy::Off).await
}

/// Drive a seeded [`Room`] to completion with an explicit deck per seat — the
/// generalization of [`play_seeded_game`] used to play the bundled starter decks
/// (issue #257) rather than only the green mirror.
async fn play_seeded_game_with(
    seed: u64,
    deck0: Vec<CardId>,
    deck1: Vec<CardId>,
    auto_pass: AutoPassPolicy,
) -> (GameView, Vec<(usize, String)>) {
    let db = CardDatabase::bundled().expect("bundled cards");
    let setup = GameSetup::two_player(deck0, deck1, seed);
    let state = GameState::new(&setup, &db).expect("valid setup");
    let (handle, task) = Room::new(state, db).with_auto_pass(auto_pass).spawn();

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

/// Play a full seeded game with basic priority automation on (issue #264).
async fn play_seeded_game_auto(seed: u64) -> (GameView, Vec<(usize, String)>) {
    let db = CardDatabase::bundled().expect("bundled cards");
    let deck = decklist(&db);
    play_seeded_game_with(seed, deck.clone(), deck, AutoPassPolicy::On).await
}

#[tokio::test]
async fn issue_264_agent_vs_agent_is_deterministic_with_automation_on() {
    // The replay stays deterministic with automation on: the same seed reproduces the
    // same action-by-action transcript and the same terminal result. (Automation only
    // relieves the agents of idle passes the room now applies — a pure function of the
    // state — so nothing about the game becomes nondeterministic.)
    let seed = 0x5EED_264A_0000_0001;
    let (first_terminal, first_transcript) = play_seeded_game_auto(seed).await;
    let (second_terminal, second_transcript) = play_seeded_game_auto(seed).await;

    assert_eq!(
        first_transcript, second_transcript,
        "with automation on, the same seed reproduces the same transcript",
    );
    assert_eq!(
        first_terminal.result, second_terminal.result,
        "with automation on, the same seed reproduces the same result",
    );
    assert!(
        first_terminal.result.is_some_and(|r| r.winner.is_some()),
        "the automated game still finishes with a decisive winner",
    );
}

#[tokio::test]
async fn issue_264_automation_finishes_a_game_with_fewer_agent_decisions() {
    // Automation makes the agents click far fewer times to finish the same game: the
    // room auto-passes their idle priority, so their transcript is a strict, much
    // smaller subset of moves than the manual baseline — while still reaching a
    // decisive result.
    let seed = 0x5EED_264A_0000_0002;
    let (auto_terminal, auto_transcript) = play_seeded_game_auto(seed).await;
    let (manual_terminal, manual_transcript) = play_seeded_game(seed).await;

    assert!(
        auto_terminal.result.is_some_and(|r| r.winner.is_some()),
        "the automated game finishes with a winner",
    );
    assert!(
        manual_terminal.result.is_some(),
        "the manual baseline also finishes",
    );
    assert!(
        auto_transcript.len() < manual_transcript.len(),
        "automation costs the agents fewer decisions: auto={} manual={}",
        auto_transcript.len(),
        manual_transcript.len(),
    );
}

// ----- The bundled starter decks play for real (issue #257) -----

/// One bundled starter deck: its display name and the flat list of authored
/// `functional_id`s it runs (each card repeated `count` times).
struct StarterDeck {
    name: String,
    identities: Vec<String>,
}

/// Read the bundled starter decks from the **single source of truth** the web client
/// also imports: `clients/web/src/starter-decks.json`. Reading that exact file here —
/// rather than re-listing the cards in Rust — is what keeps the client's decks and
/// this wire test's decks from silently drifting apart (issue #257). The client owns
/// which decks the lobby offers; the engine merely validates and plays them.
fn bundled_decklists() -> Vec<StarterDeck> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../clients/web/src/starter-decks.json"
    );
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("reading the shared starter decks at {path}: {e}"));
    let json: serde_json::Value = serde_json::from_str(&text).expect("starter-decks.json parses");
    json["decks"]
        .as_array()
        .expect("a `decks` array")
        .iter()
        .map(|deck| {
            let name = deck["name"].as_str().expect("a deck name").to_string();
            let mut identities = Vec::new();
            for entry in deck["entries"].as_array().expect("deck entries") {
                let identity = entry["identity"].as_str().expect("an identity");
                let count = entry["count"].as_u64().expect("a count");
                for _ in 0..count {
                    identities.push(identity.to_string());
                }
            }
            StarterDeck { name, identities }
        })
        .collect()
}

/// Resolve a decklist of authored identities to this build's interned handles.
fn resolve_deck(db: &CardDatabase, identities: &[String]) -> Vec<CardId> {
    identities
        .iter()
        .map(|slug| {
            let fid = FunctionalId::try_from(slug.clone()).expect("a well-formed identity");
            db.card_id(&fid)
                .unwrap_or_else(|| panic!("`{slug}` is not in the bundled catalog"))
        })
        .collect()
}

/// The set of colors a decklist's basic lands can produce.
fn producible_colors(db: &CardDatabase, ids: &[CardId]) -> HashSet<Color> {
    let mut colors = HashSet::new();
    for &id in ids {
        let card = db.card(id).expect("a bundled card");
        if !card.has_type(CardType::Land) {
            continue;
        }
        for ability in &card.abilities {
            if let Ability::Activated { effects, .. } = ability {
                for effect in effects {
                    if let Effect::AddMana { color, .. } = effect {
                        colors.insert(*color);
                    }
                }
            }
        }
    }
    colors
}

#[test]
fn every_bundled_decklist_is_legal_and_castable_from_its_own_mana_base() {
    // The exact bug that shipped as "Temur Tempo": blue and red cards over an
    // all-Forest mana base, uncastable. For every bundled deck, prove from catalog
    // data — not by eyeball — that it is format-legal and self-castable: at least 40
    // cards, at most four copies of any non-basic, every nonland card's colors
    // producible by its own lands, and at least one instant or sorcery so the stack
    // and targeting flows are reachable.
    let db = CardDatabase::bundled().unwrap();
    let decks = bundled_decklists();
    assert!(decks.len() >= 2, "there are multiple bundled decks");

    for deck in &decks {
        let ids = resolve_deck(&db, &deck.identities);
        let name = &deck.name;
        assert!(
            ids.len() >= 40,
            "{name} has {} cards, under the 40-card minimum",
            ids.len()
        );

        // Copy limit: at most four of any non-basic card (basic lands exempt).
        let mut counts: HashMap<CardId, usize> = HashMap::new();
        for &id in &ids {
            *counts.entry(id).or_default() += 1;
        }
        for (&id, &n) in &counts {
            let card = db.card(id).unwrap();
            let is_basic = card.supertypes.contains(&Supertype::Basic);
            assert!(
                is_basic || n <= 4,
                "{name}: {} appears {n} times (copy limit is 4)",
                card.name
            );
        }

        // Castability: the deck's own lands must produce every color it asks for.
        let producible = producible_colors(&db, &ids);
        for &id in &ids {
            let card = db.card(id).unwrap();
            if card.has_type(CardType::Land) {
                continue;
            }
            for color in &card.colors {
                assert!(
                    producible.contains(color),
                    "{name}: {} needs {color:?}, but the deck's mana base cannot produce it",
                    card.name
                );
            }
        }

        // Spells present: the stack/targeting/counter flows must be reachable.
        let has_spell = ids.iter().any(|&id| {
            let c = db.card(id).unwrap();
            c.has_type(CardType::Instant) || c.has_type(CardType::Sorcery)
        });
        assert!(has_spell, "{name} contains no instants or sorceries");
    }
}

#[tokio::test]
async fn bundled_decklists_play_to_a_deterministic_completion() {
    // The M3 exit-criterion proof: the *actual bundled decklists* (the same file the
    // client submits) play full games through the real layer-2 room and wire protocol
    // to a decisive winner. Round-robin so every deck is exercised and every pairing
    // interacts — including the archetype clashes (aggro vs midrange vs tempo).
    let db = CardDatabase::bundled().unwrap();
    let decks: Vec<Vec<CardId>> = bundled_decklists()
        .iter()
        .map(|deck| resolve_deck(&db, &deck.identities))
        .collect();
    assert!(decks.len() >= 2, "need at least two decks to pair");

    for i in 0..decks.len() {
        let j = (i + 1) % decks.len();
        let seed = 0x5EED_D0D0_0000_0000 ^ (((i as u64) << 8) | j as u64);
        let (terminal, transcript) = play_seeded_game_with(
            seed,
            decks[i].clone(),
            decks[j].clone(),
            AutoPassPolicy::Off,
        )
        .await;
        let result = terminal.result.expect("a finished game carries a result");
        assert!(
            result.winner.is_some(),
            "decks {i} vs {j} ended in a draw: {result:?}"
        );
        assert_eq!(result.losers.len(), 1, "decks {i} vs {j}: {result:?}");
        assert!(
            !transcript.is_empty(),
            "decks {i} vs {j}: the agents played no actions"
        );
    }
}
