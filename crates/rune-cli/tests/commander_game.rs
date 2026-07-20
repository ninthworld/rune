//! Full-game integration for a **deterministic commander game** (issue #398).
//!
//! Two deterministic [`rune_cli`] agent policies play a complete, legal **commander**
//! game against a real layer-2 [`rune_server::Room`] built from a pinned seed, until
//! the engine declares a single winner — the format-level, over-the-wire proof that
//! #350 gave the free-for-all. It is the commander analogue of `tests/agent_game.rs`
//! and `tests/ffa_game.rs`: same driver shape (drive the room task directly so the
//! test can read every seat's terminal [`GameView`] and record a deterministic
//! transcript), but seated for the commander format — 40 starting life (CR 903.7),
//! commanders in visible command zones (CR 903.6) — and driven so the run passes
//! *through* the commander-specific mechanics rather than merely starting in the
//! format:
//!
//! - a commander is **cast from the command zone** (CR 903.8) by both seats;
//! - one commander dies in combat, is returned (CR 903.9a) and **recast paying the
//!   `{2}` tax** (CR 903.8), asserted from the public `commander_tax` projection; and
//! - the game ends by **commander damage reaching 21** (CR 903.10a), asserted via the
//!   [`GameOverReason::CommanderDamage`] loss reason and the `commander_damage` tally.
//!
//! Determinism. The two decks are purpose-built and minimal (an inline card set, the
//! same technique the engine's own `tests/commander_zone.rs` uses): each is a
//! singleton mono-green commander deck whose library is **entirely basic Forests**
//! (the sole non-land, the commander, starts set aside in the command zone), so every
//! draw is identical and the mana curve does not depend on the shuffle at all — the
//! game is reproducible for the pinned seed *and* across seeds. The aggressor's
//! commander is an evasive (flying) legend the ground-bound defender can never block,
//! so it connects for a clean 21; the defender's commander trades into it once during
//! its summoning-sick block window, forcing exactly the one taxed recast. The agents
//! choose only from the offered `valid_actions` (never any engine access), reusing the
//! shared [`rune_cli::fill_answers`] to fill every declaration slot.
//!
//! Runtime. One short seeded game (about a dozen turns over an all-Forest curve),
//! comparable to the #350 free-for-all, so it runs in the normal `make check` /
//! `cargo test --workspace` gate. The driver caps its loop so a future policy or rules
//! regression surfaces as a bounded failure rather than a hang.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;

use rune_cli::{choose_action, fill_answers};
use rune_engine::{CardDatabase, CardId, FunctionalId, GameSetup, GameState, PlayerSetup};
use rune_protocol::{
    ChooseAction, ClientMessage, GameOverReason, GameView, Phase, PlayerId, ValidAction,
};
use rune_server::{AutoPassPolicy, Room, RoomInput};
use tokio::sync::watch;

/// The life total a commander game begins at (CR 903.7): 40. Fed through
/// [`GameSetup::starting_life`] exactly as the server's commander format does.
const COMMANDER_STARTING_LIFE: i32 = 40;

/// A minimal, self-contained card set for a deterministic commander game. Three
/// definitions, authored against the card schema (`docs/card-schema.md`) exactly like
/// the bundled catalog, loaded into an in-memory [`CardDatabase`] so the scenario does
/// not depend on any bundled card's stats (the technique the engine's own
/// `tests/commander_zone.rs` uses):
///
/// - `skyfire_titan` — the aggressor's commander: a `{2}{G}` **legendary** creature
///   with **flying**, **vigilance**, and power 7, so three unblocked hits deal exactly
///   21 combat damage (CR 903.10a) that no ground creature can ever block, while
///   vigilance keeps it back to block on every defender turn;
/// - `thornback_alpha` — the defender's commander: a `{1}{G}` legendary 3/3 that,
///   whenever it attacks, is blocked and killed by the vigilant flyer, so it dies and
///   is returned (CR 903.9a) and recast paying the `{2}` tax (CR 903.8); and
/// - `forest` — the basic land both singleton decks are otherwise built from, the
///   sole card in either library so every draw is identical.
const CARDS_JSON: &str = r#"[
  {
    "schema_version": 1,
    "functional_id": "skyfire_titan",
    "name": "Skyfire Titan",
    "supertypes": ["legendary"],
    "types": ["creature"],
    "subtypes": ["Giant"],
    "mana_cost": "{2}{G}",
    "colors": ["green"],
    "power": 7,
    "toughness": 7,
    "keywords": ["flying", "vigilance"]
  },
  {
    "schema_version": 1,
    "functional_id": "thornback_alpha",
    "name": "Thornback Alpha",
    "supertypes": ["legendary"],
    "types": ["creature"],
    "subtypes": ["Beast"],
    "mana_cost": "{1}{G}",
    "colors": ["green"],
    "power": 3,
    "toughness": 3
  },
  {
    "schema_version": 1,
    "functional_id": "forest",
    "name": "Forest",
    "supertypes": ["basic"],
    "types": ["land"],
    "subtypes": ["Forest"],
    "mana_cost": "",
    "colors": [],
    "abilities": [
      {
        "type": "activated",
        "cost": [{ "kind": "tap" }],
        "effects": [{ "kind": "add_mana", "color": "green", "amount": 1 }]
      }
    ]
  }
]"#;

/// Resolve an authored identity to this build's interned handle in `db`.
fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let fid = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&fid).expect("a card in the inline set")
}

/// A 100-card singleton commander deck (CR 903.6): `commander` plus Forests to 100,
/// mono-green and within the commander's color identity. Setup sets the commander
/// aside into the command zone, leaving an all-Forest library.
fn commander_deck(db: &CardDatabase, commander: &str) -> Vec<CardId> {
    let mut deck = vec![cid(db, commander)];
    while deck.len() < 100 {
        deck.push(cid(db, "forest"));
    }
    deck
}

/// The seeded commander game's setup: seat 0 brings the flying aggressor commander,
/// seat 1 the ground defender commander, both at 40 life (CR 903.7).
fn commander_setup(db: &CardDatabase, seed: u64) -> GameSetup {
    let players = vec![
        PlayerSetup::with_commander(
            commander_deck(db, "skyfire_titan"),
            cid(db, "skyfire_titan"),
        ),
        PlayerSetup::with_commander(
            commander_deck(db, "thornback_alpha"),
            cid(db, "thornback_alpha"),
        ),
    ];
    GameSetup {
        starting_life: COMMANDER_STARTING_LIFE,
        ..GameSetup::new(players, seed)
    }
}

// --- The deterministic agent policy ---------------------------------------------
//
// A thin commander-aware wrapper over the shared rule-based policy
// ([`rune_cli::choose_action`]): it adds the one thing that policy does not model —
// developing mana toward, and casting, a commander that lives in the *command zone*
// rather than the hand — and defers everything else (mulligan keeps, combat
// declarations, attacks, blocks, passes) to the shared policy unchanged. It reads only
// the personalized `GameView` + `valid_actions`, never any engine state.

/// The first offered action of `kind`, if any.
fn first_of_kind<'a>(view: &'a GameView, kind: &str) -> Option<&'a ValidAction> {
    view.valid_actions.iter().find(|a| a.kind == kind)
}

/// The entity ids of the cards in the receiver's own command zone (their commander,
/// while it is there).
fn my_command_zone_ids(view: &GameView) -> BTreeSet<String> {
    view.command
        .iter()
        .filter(|pile| pile.player_id == view.you)
        .flat_map(|pile| pile.cards.iter().map(|card| card.id.clone()))
        .collect()
}

/// The offered command-zone cast of the receiver's commander (a `cast_spell` whose
/// subject is a card in their command zone), if one is on offer.
fn command_zone_cast(view: &GameView) -> Option<&ValidAction> {
    let in_zone = my_command_zone_ids(view);
    view.valid_actions
        .iter()
        .find(|a| a.kind == "cast_spell" && a.subject.iter().any(|s| in_zone.contains(s)))
}

/// An offered land mana ability (an `activate_ability` with no target requirements
/// whose source is a land the receiver controls), for developing mana toward a
/// command-zone cast — the same shape the shared policy taps for hand spells.
fn land_mana_ability(view: &GameView) -> Option<&ValidAction> {
    view.valid_actions.iter().find(|a| {
        a.kind == "activate_ability"
            && a.requirements.is_empty()
            && a.subject.iter().any(|id| {
                view.battlefield
                    .iter()
                    .any(|p| &p.id == id && p.card.type_line.to_lowercase().contains("land"))
            })
    })
}

/// Choose one offered action for `view`, deterministically. Prioritizes the
/// commander-specific moves the shared policy does not model, then defers to it.
fn commander_choose_action(view: &GameView) -> Option<&ValidAction> {
    if view.valid_actions.is_empty() {
        return None;
    }
    // CR 903.9a: whenever the return-to-command-zone decision is owed (the commander
    // died into a graveyard), take it — that is what enables the taxed recast.
    if let Some(ret) = first_of_kind(view, "return_commander_to_command_zone") {
        return Some(ret);
    }

    // On our own main phase with an empty stack (sorcery timing), develop toward and
    // cast the commander from the command zone.
    let our_main = view.active_player == view.you
        && matches!(view.phase, Phase::PrecombatMain | Phase::PostcombatMain)
        && view.stack.is_empty();
    if our_main {
        // Make our land drop first, so tapping never costs us a turn's development.
        if let Some(land) = first_of_kind(view, "play_land") {
            return Some(land);
        }
        // Cast the commander from the command zone when it is affordable (CR 903.8).
        if let Some(cast) = command_zone_cast(view) {
            return Some(cast);
        }
        // Still in the command zone and uncast: tap a land toward its (taxed) cost.
        if !my_command_zone_ids(view).is_empty() {
            if let Some(mana) = land_mana_ability(view) {
                return Some(mana);
            }
        }
    }

    // Everything else — mulligan keeps, combat declarations, attacks, blocks, and the
    // final pass — is the shared rule-based policy, unchanged.
    choose_action(view)
}

/// The `p{N}` id for a seat index, as it appears throughout the view.
fn seat_id(seat: usize) -> PlayerId {
    format!("p{seat}")
}

/// The commander-tax progression observed for one commander over a game: the set of
/// `(casts, tax)` pairs the public `commander_tax` projection carried at any point.
type TaxTrail = BTreeSet<(u32, u32)>;

/// The terminal outcome of a seeded commander game: both seats' terminal [`GameView`],
/// the `(seat, action_id)` transcript, and the per-commander tax trail.
struct CommanderOutcome {
    /// Each seat's terminal view (both carry the same `result`).
    terminals: [GameView; 2],
    /// The action-by-action transcript, enough to check reproducibility.
    transcript: Vec<(usize, String)>,
    /// For each commander (keyed by its owning seat's id), the `(casts, tax)` pairs
    /// its public tax projection carried over the game.
    tax_trail: std::collections::BTreeMap<PlayerId, TaxTrail>,
}

/// Drive a two-seat commander [`Room`] to completion with the commander policy on both
/// seats, returning the [`CommanderOutcome`]. Priority automation is off so the agents
/// drive every decision themselves, exactly as the manual agent full-game tests do.
async fn play_seeded_commander_game(seed: u64) -> CommanderOutcome {
    let db = CardDatabase::from_json(CARDS_JSON).expect("the inline card set loads");
    let setup = commander_setup(&db, seed);
    let state = GameState::new(&setup, &db).expect("valid commander setup");
    let (handle, task) = Room::new(state, db)
        .with_auto_pass(AutoPassPolicy::Off)
        .spawn();

    let (tx0, mut rx0) = watch::channel::<Option<GameView>>(None);
    let (tx1, mut rx1) = watch::channel::<Option<GameView>>(None);
    assert!(handle.send(RoomInput::Join {
        seat: 0,
        outbox: tx0
    }));
    assert!(handle.send(RoomInput::Join {
        seat: 1,
        outbox: tx1
    }));

    let mut transcript: Vec<(usize, String)> = Vec::new();
    let mut tax_trail: std::collections::BTreeMap<PlayerId, TaxTrail> =
        std::collections::BTreeMap::new();
    let mut terminals: Option<[GameView; 2]> = None;

    // A generous cap so a policy or rules bug surfaces as a bounded failure, never a
    // hang; the all-Forest curve finishes in far fewer iterations than this.
    for _ in 0..1_000_000usize {
        let views = [
            rx0.borrow_and_update().clone(),
            rx1.borrow_and_update().clone(),
        ];

        // Record the public commander-tax projection from whichever views are current.
        for view in views.iter().flatten() {
            for entry in &view.commander_tax {
                tax_trail
                    .entry(entry.commander.clone())
                    .or_default()
                    .insert((entry.casts, entry.tax));
            }
        }

        // The game is over once both seats' views carry the terminal result.
        if views
            .iter()
            .all(|v| v.as_ref().is_some_and(|v| v.result.is_some()))
        {
            terminals = Some(views.map(|v| v.expect("a terminal view on each seat")));
            break;
        }

        // Exactly one seat holds priority (is offered actions) at a time; it acts.
        let actor = views.iter().enumerate().find_map(|(seat, view)| {
            view.as_ref()
                .filter(|v| !v.valid_actions.is_empty())
                .map(|v| (seat, v.clone()))
        });
        if let Some((seat, view)) = actor {
            let action = commander_choose_action(&view).expect("the agent always has a move");
            let targets = fill_answers(&view, action).expect("the agent fills every slot");
            transcript.push((seat, action.id.clone()));
            assert!(handle.send(RoomInput::Message {
                seat,
                message: ClientMessage::ChooseAction(ChooseAction {
                    action_id: action.id.clone(),
                    token: action.token.clone(),
                    targets,
                }),
            }));
        }

        // Await the next broadcast on either seat; channel closure is caught by the
        // terminal check at the top of the next iteration.
        let alive = tokio::select! {
            r = rx0.changed() => r.is_ok(),
            r = rx1.changed() => r.is_ok(),
        };
        if !alive {
            break;
        }
    }

    drop(handle);
    let _ = task.await;
    CommanderOutcome {
        terminals: terminals.expect("the commander game reached a terminal state within the cap"),
        transcript,
        tax_trail,
    }
}

/// The pinned seed for the deterministic commander game. The all-Forest libraries make
/// the game reproducible across seeds too; a seed is pinned for form and parity with
/// the #350 free-for-all test.
const COMMANDER_SEED: u64 = 0x5EED_0398_C0DE_0001;

#[tokio::test]
async fn a_seeded_commander_game_ends_by_commander_damage_through_the_server() {
    let outcome = play_seeded_commander_game(COMMANDER_SEED).await;

    // Both seats agree on a single winner and one loser.
    let result = outcome.terminals[0]
        .result
        .as_ref()
        .expect("a finished game carries a result");
    for view in &outcome.terminals {
        assert_eq!(
            view.result.as_ref(),
            Some(result),
            "every seat's view names the same terminal result"
        );
        assert!(
            view.valid_actions.is_empty(),
            "a terminal view offers no actions"
        );
    }

    // The game ends by COMMANDER DAMAGE reaching 21 (CR 903.10a), not by life loss or
    // decking — the format-specific losing condition this test exists to prove.
    assert_eq!(
        result.reason,
        GameOverReason::CommanderDamage,
        "the game ends by the commander-damage loss reason: {result:?}"
    );
    assert_eq!(
        result.winner,
        Some(seat_id(0)),
        "the aggressor wins: {result:?}"
    );
    assert_eq!(
        result.losers,
        vec![seat_id(1)],
        "exactly one loser: {result:?}"
    );

    // The tally backs the loss reason: the winner's commander dealt the loser 21+.
    let lethal = outcome.terminals[0]
        .commander_damage
        .iter()
        .find(|d| d.commander == seat_id(0) && d.damaged == seat_id(1))
        .expect("the aggressor's commander has a damage tally against the loser");
    assert!(
        lethal.amount >= 21,
        "commander damage reached the 21 threshold: {lethal:?}"
    );

    // A commander was cast from the command zone by BOTH seats (any cast from there
    // increments the designation's cast count), and the defender's commander was
    // recast paying the {2} tax — the tax owed after one cast is {2}, and a second
    // cast (casts == 2) is a recast that paid it (CR 903.8).
    let attacker_trail = outcome
        .tax_trail
        .get(&seat_id(0))
        .expect("the aggressor's commander has a tax projection");
    assert!(
        attacker_trail.iter().any(|&(casts, _)| casts >= 1),
        "the aggressor cast its commander from the command zone: {attacker_trail:?}"
    );

    let defender_trail = outcome
        .tax_trail
        .get(&seat_id(1))
        .expect("the defender's commander has a tax projection");
    assert!(
        defender_trail.contains(&(1, 2)),
        "after the first command-zone cast the defender owed the {{2}} tax: {defender_trail:?}"
    );
    assert!(
        defender_trail.iter().any(|&(casts, _)| casts >= 2),
        "the defender recast its commander from the command zone, paying the {{2}} tax: {defender_trail:?}"
    );

    assert!(
        !outcome.transcript.is_empty(),
        "the agents played a sequence of actions"
    );
}

#[tokio::test]
async fn the_commander_game_is_deterministic_for_a_seed() {
    let first = play_seeded_commander_game(COMMANDER_SEED).await;
    let second = play_seeded_commander_game(COMMANDER_SEED).await;

    assert_eq!(
        first.transcript, second.transcript,
        "the same seed reproduces the same action-by-action transcript across both seats",
    );
    assert_eq!(
        first.terminals[0].result, second.terminals[0].result,
        "the same seed reproduces the same winner, loser, and loss reason",
    );
    assert_eq!(
        first.terminals[0].commander_damage, second.terminals[0].commander_damage,
        "the same seed reproduces the same commander-damage tally",
    );
}
