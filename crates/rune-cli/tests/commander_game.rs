//! Full-game integration for the **commander format** (issue #398): a seeded,
//! deterministic commander game plays to a single winner through a real
//! [`rune_server::Room`], and the winning line runs *through the commander-specific
//! mechanics* — a cast from the command zone, the commander's death and CR 903.9a
//! return to the command zone, a recast paying the `{2}` tax (CR 903.8), and a loss to
//! 21 commander damage (CR 903.10a) — all chosen from the offered `valid_actions` over
//! the wire. This is the format-level, over-the-wire proof that the #350 four-player
//! free-for-all gave the multiplayer slice; it exercises the commander loop
//! (#370/#371/#372) end to end rather than at the isolated engine seams the unit tests
//! cover.
//!
//! Like the free-for-all test (`tests/ffa_game.rs`) it drives the room task directly
//! rather than over a duplex socket, so the driver can read both seats' terminal
//! `GameView`s (winner, loser, and loss reason) and record a deterministic transcript.
//! The two seats decide **only** among the `valid_actions` the engine already offered
//! — filling slots with [`rune_cli::fill_answers`], exactly what the socket loop sends
//! — but, unlike the free-for-all's single win-seeking policy, they run a purpose-built
//! **asymmetric** policy: a symmetric win-seeker cannot reliably manufacture a clean
//! 21-commander-damage line (the loser must fall to *commander* damage while still
//! above zero life), so the aggressor's only creature is its commander and the victim
//! never races back. The victim blocks exactly once — to kill the commander and force
//! the taxed recast — then lets it connect to 21. Every choice is still a legal,
//! server-offered action; none is a hardcoded action index.
//!
//! Runtime: one seeded game with basic priority automation on, comparable to the
//! free-for-all gate test, so it runs in the normal `make check` / `cargo test
//! --workspace` gate. The driver caps its loop so a policy bug surfaces as a bounded
//! failure rather than a hang.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rune_cli::fill_answers;
use rune_engine::{
    CardDatabase, CardId, FunctionalId, GameSetup, GameState, PlayerSetup,
    DEFAULT_STARTING_HAND_SIZE,
};
use rune_protocol::{
    ChooseAction, ClientMessage, GameOverReason, GameView, Phase, TargetChoice, ValidAction,
};
use rune_server::{AutoPassPolicy, Room, RoomInput};
use tokio::sync::watch;

/// Commander starting life (CR 903.7).
const COMMANDER_LIFE: i32 = 40;

/// The seat that wins with its commander: its *only* creature is the commander, so the
/// only combat damage the victim ever takes is commander damage — the CR 903.10a line
/// stays clean (the victim falls to 21 commander damage while still above zero life).
const AGGRESSOR: usize = 0;
/// The seat that loses to commander damage. It fields a single blocker only to kill the
/// commander once (forcing the taxed recast, CR 903.8), then never blocks again.
const VICTIM: usize = 1;

/// The display name of the victim's blocker — a 10/10 that survives blocking the 5/5
/// commander and kills it, sending it to the graveyard so its owner must return it to
/// the command zone (CR 903.9a) and recast it at the `{2}` tax.
const BLOCKER_NAME: &str = "Gigantosaurus";

/// Resolve `slug` (an authored `functional_id`, ADR 0018 §3) to its catalog [`CardId`].
fn card(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// The aggressor's deck: Jedit Ojanen (a `{4}{G}{G}` 5/5 legendary) as commander, with
/// nothing but Forests behind it — so the commander is the seat's sole attacker and the
/// victim only ever takes commander damage. Exactly 100 cards with the commander set
/// aside (CR 903.5).
fn aggressor_deck(db: &CardDatabase) -> Vec<CardId> {
    let mut deck = vec![card(db, "jedit_ojanen")];
    deck.extend(std::iter::repeat_n(card(db, "forest"), 99));
    deck
}

/// The victim's deck: its own Jedit commander, a wall of Gigantosaurus blockers, and
/// Forests to cast them. This is an **engine fixture**, not a server-validated list
/// (deck legality is a server concern, ADR 0013 §4; `GameState::new` checks only that
/// ids resolve): the extra blocker copies make "draw and cast a blocker, then block the
/// commander once" reliable across seeds without pinning the shuffle to a single draw.
fn victim_deck(db: &CardDatabase) -> Vec<CardId> {
    let mut deck = vec![card(db, "jedit_ojanen")];
    deck.extend(std::iter::repeat_n(card(db, BLOCKER_SLUG), 12));
    deck.extend(std::iter::repeat_n(card(db, "forest"), 87));
    deck
}

/// The blocker's authored identity (its display name is [`BLOCKER_NAME`]).
const BLOCKER_SLUG: &str = "gigantosaurus";

/// The seat index encoded in a `p{N}` player id.
fn seat_of(player_id: &str) -> usize {
    player_id
        .strip_prefix('p')
        .and_then(|n| n.parse::<usize>().ok())
        .unwrap_or_else(|| panic!("player id `{player_id}` is not the expected p{{N}} form"))
}

/// The first offered action of `kind`, if any.
fn action_of_kind<'a>(actions: &'a [ValidAction], kind: &str) -> Option<&'a ValidAction> {
    actions.iter().find(|action| action.kind == kind)
}

/// Whether the seat `who` currently has a card in its (public) command zone.
fn commander_in_command_zone(view: &GameView, who: &str) -> bool {
    view.command
        .iter()
        .any(|pile| pile.player_id == who && !pile.cards.is_empty())
}

/// Whether the receiver controls a [`BLOCKER_NAME`] permanent on the battlefield.
fn blocker_in_play(view: &GameView) -> bool {
    view.battlefield
        .iter()
        .any(|perm| perm.controller == view.you && perm.card.name == BLOCKER_NAME)
}

/// The `cast_spell` action for the blocker *in hand*, if offered. Keyed to a hand card
/// so it can never match the command-zone commander's cast (whose subject is the
/// command-zone copy, not a hand card) — the victim never casts its own commander.
fn cast_blocker_in_hand<'a>(
    view: &'a GameView,
    actions: &'a [ValidAction],
) -> Option<&'a ValidAction> {
    actions.iter().find(|action| {
        action.kind == "cast_spell"
            && action.subject.first().is_some_and(|id| {
                view.my_hand
                    .iter()
                    .any(|c| &c.id == id && c.name == BLOCKER_NAME)
            })
    })
}

/// Whether `phase` is a main phase (the only window a sorcery-speed commander cast is
/// offered, so mana is floated toward it only there — never wasted in upkeep).
fn is_main_phase(phase: Phase) -> bool {
    matches!(phase, Phase::PrecombatMain | Phase::PostcombatMain)
}

/// The first offered mana ability (a one-gesture tap-for-mana, ADR 0025).
fn mana_ability(actions: &[ValidAction]) -> Option<&ValidAction> {
    actions.iter().find(|action| action.mana_ability)
}

/// Build the wire answer for `action`, filling its slots per [`fill_answers`].
fn answer(view: &GameView, action: &ValidAction) -> ChooseAction {
    ChooseAction {
        action_id: action.id.clone(),
        token: action.token.clone(),
        targets: fill_answers(view, action).unwrap_or_default(),
    }
}

/// Build the wire answer for `action` with an **empty** selection — a legal "declare no
/// attackers/blockers" (CR 508.1a / 509.1a), which the server binds from empty targets.
fn decline(action: &ValidAction) -> ChooseAction {
    ChooseAction {
        action_id: action.id.clone(),
        token: action.token.clone(),
        targets: Vec::<TargetChoice>::new(),
    }
}

/// The asymmetric two-seat commander policy, plus the running transcript and the
/// one-shot "the victim has spent its block" latch that turns the commander's death into
/// a single taxed recast rather than an endless block-and-die loop.
#[derive(Default)]
struct Driver {
    victim_has_blocked: bool,
    transcript: Vec<(usize, String)>,
}

impl Driver {
    /// Decide the acting seat's move from its `view`, or `None` if nothing is offered.
    fn decide(&mut self, view: &GameView) -> Option<ChooseAction> {
        let seat = seat_of(&view.you);
        let choice = match seat {
            AGGRESSOR => self.aggressor(view),
            _ => self.victim(view),
        }?;
        // Record the action *kind* (a stable, meaningful label) rather than the opaque
        // per-view action id, so the transcript reads as the line that was played and
        // stays comparable across runs.
        let kind = view
            .valid_actions
            .iter()
            .find(|a| a.id == choice.action_id)
            .map(|a| a.kind.clone())
            .unwrap_or_default();
        self.transcript.push((seat, kind));
        Some(choice)
    }

    /// The aggressor: keep the hand, accept the CR 903.9a return, attack with the
    /// commander, and — while the commander sits in the command zone — float mana toward
    /// casting it (the rule-based agent floats mana only for *hand* cards, so a
    /// command-zone cast needs this dedicated ramp). Otherwise develop lands and pass.
    fn aggressor(&self, view: &GameView) -> Option<ChooseAction> {
        let actions = &view.valid_actions;
        // Forced/priority windows first (each is offered without a pass alongside), then
        // main-phase development, in a fixed order so the game is deterministic.
        for kind in [
            "mulligan_decision",
            "discard",
            "return_commander_to_command_zone",
            "declare_attackers",
            "declare_blockers",
            "play_land",
            "cast_spell",
        ] {
            if let Some(action) = action_of_kind(actions, kind) {
                return Some(answer(view, action));
            }
        }
        // No cast on offer yet: float one more mana toward the command-zone commander.
        if commander_in_command_zone(view, &view.you) && is_main_phase(view.phase) {
            if let Some(action) = mana_ability(actions) {
                return Some(answer(view, action));
            }
        }
        if let Some(pass) = action_of_kind(actions, "pass_priority") {
            return Some(decline(pass));
        }
        // A forced choice with no pass: any non-concede action, else concede last.
        let fallback = actions
            .iter()
            .find(|a| a.kind != "concede")
            .or_else(|| actions.first())?;
        Some(answer(view, fallback))
    }

    /// The victim: keep the hand, **never attack**, and block the commander exactly once
    /// — the first time a blocker is in play — to kill it and force the taxed recast,
    /// then never block again so the recast commander connects to 21. It develops a
    /// single blocker (never its own commander) and otherwise passes.
    fn victim(&mut self, view: &GameView) -> Option<ChooseAction> {
        let actions = &view.valid_actions;
        if let Some(action) = action_of_kind(actions, "mulligan_decision") {
            return Some(answer(view, action));
        }
        if let Some(action) = action_of_kind(actions, "discard") {
            return Some(answer(view, action));
        }
        // Never attack: an empty declaration passes combat without swinging.
        if let Some(action) = action_of_kind(actions, "declare_attackers") {
            return Some(decline(action));
        }
        if let Some(action) = action_of_kind(actions, "declare_blockers") {
            // Block exactly once, and only when there is really an attacker to block
            // (a block window with no declared attacker carries no requirement slots) and
            // a blocker actually gets assigned — so the one-shot latch is spent on the
            // block that kills the commander, never on an empty pre-commander window.
            if !self.victim_has_blocked && !action.requirements.is_empty() && blocker_in_play(view)
            {
                let targets = fill_answers(view, action).unwrap_or_default();
                if targets.iter().any(|choice| !choice.chosen.is_empty()) {
                    self.victim_has_blocked = true;
                    // The profitable blocker (the 10/10) is assigned to the 5/5
                    // commander: it survives, the commander dies (CR 704.5g).
                    return Some(ChooseAction {
                        action_id: action.id.clone(),
                        token: action.token.clone(),
                        targets,
                    });
                }
            }
            return Some(decline(action));
        }
        // Establish a single blocker, then idle until the commander comes in.
        if !blocker_in_play(view) {
            if let Some(action) = action_of_kind(actions, "play_land") {
                return Some(answer(view, action));
            }
            if let Some(action) = cast_blocker_in_hand(view, actions) {
                return Some(answer(view, action));
            }
            if is_main_phase(view.phase) {
                if let Some(action) = mana_ability(actions) {
                    return Some(answer(view, action));
                }
            }
        }
        if let Some(pass) = action_of_kind(actions, "pass_priority") {
            return Some(decline(pass));
        }
        let fallback = actions
            .iter()
            .find(|a| a.kind != "concede")
            .or_else(|| actions.first())?;
        Some(answer(view, fallback))
    }
}

/// What a seeded commander game demonstrated: both seats' terminal views, the peak
/// commander-tax cast count and commander-damage tally observed for the aggressor's
/// commander, and the transcript — enough to prove every acceptance criterion.
struct Outcome {
    /// Each seat's terminal view (both carry the same `result`).
    terminals: [GameView; 2],
    /// The most casts-from-the-command-zone the aggressor's commander ever showed
    /// (CR 903.8): `>= 2` proves a recast paying the `{2}` tax happened.
    max_casts: u32,
    /// The greatest `{2}` tax the aggressor's commander was seen *owing* before a recast
    /// — proof the recast was actually taxed, not free.
    max_tax_owed: u32,
    /// The most commander damage the aggressor's commander dealt the victim (CR 903.10a).
    max_commander_damage: u32,
    /// The aggressor's first-observed life total (should be the commander 40, CR 903.7).
    aggressor_start_life: i32,
    /// The ordered `(seat, action_id)` transcript.
    transcript: Vec<(usize, String)>,
}

/// Drive a two-seat commander [`Room`] to completion with the asymmetric policy and
/// basic priority automation on, recording the [`Outcome`].
async fn play_seeded_commander_game(seed: u64) -> Outcome {
    let db = CardDatabase::bundled().expect("bundled cards");
    let players = vec![
        PlayerSetup::with_commander(aggressor_deck(&db), card(&db, "jedit_ojanen")),
        PlayerSetup::with_commander(victim_deck(&db), card(&db, "jedit_ojanen")),
    ];
    let setup = GameSetup {
        players,
        starting_life: COMMANDER_LIFE,
        starting_hand_size: DEFAULT_STARTING_HAND_SIZE,
        rng_seed: seed,
    };
    let state = GameState::new(&setup, &db).expect("valid setup");
    let (handle, task) = Room::new(state, db)
        .with_auto_pass(AutoPassPolicy::On)
        .spawn();

    let (tx0, mut rx0) = watch::channel::<Option<GameView>>(None);
    let (tx1, mut rx1) = watch::channel::<Option<GameView>>(None);
    for (seat, outbox) in [(0, tx0), (1, tx1)] {
        assert!(handle.send(RoomInput::Join { seat, outbox }));
    }

    let mut driver = Driver::default();
    let mut max_casts = 0u32;
    let mut max_tax_owed = 0u32;
    let mut max_commander_damage = 0u32;
    let mut aggressor_start_life = 0i32;

    // A generous cap so a policy bug surfaces as a bounded failure, never a hang.
    for _ in 0..4_000_000usize {
        let views = [
            rx0.borrow_and_update().clone(),
            rx1.borrow_and_update().clone(),
        ];

        // Track the public commander tallies as they climb, from whichever view is
        // current (both are public, so either seat's copy agrees).
        for view in views.iter().flatten() {
            if aggressor_start_life == 0 {
                if let Some(life) = current_life(view, AGGRESSOR) {
                    aggressor_start_life = life;
                }
            }
            for tax in &view.commander_tax {
                if seat_of(&tax.commander) == AGGRESSOR {
                    max_casts = max_casts.max(tax.casts);
                    max_tax_owed = max_tax_owed.max(tax.tax);
                }
            }
            for dmg in &view.commander_damage {
                if seat_of(&dmg.commander) == AGGRESSOR && seat_of(&dmg.damaged) == VICTIM {
                    max_commander_damage = max_commander_damage.max(dmg.amount);
                }
            }
        }

        // The game is over once both seats' views carry the terminal result.
        if views
            .iter()
            .all(|v| v.as_ref().is_some_and(|v| v.result.is_some()))
        {
            let terminals = views.map(|v| v.expect("a terminal view on every seat"));
            drop(handle);
            let _ = task.await;
            return Outcome {
                terminals,
                max_casts,
                max_tax_owed,
                max_commander_damage,
                aggressor_start_life,
                transcript: driver.transcript,
            };
        }

        // Exactly one seat is offered actions at a time; it acts.
        let actor = views
            .iter()
            .find_map(|v| v.as_ref().filter(|v| !v.valid_actions.is_empty()));
        if let Some(view) = actor {
            if let Some(choice) = driver.decide(view) {
                let seat = seat_of(&view.you);
                assert!(handle.send(RoomInput::Message {
                    seat,
                    message: ClientMessage::ChooseAction(choice),
                }));
            }
        }

        let alive = tokio::select! {
            r = rx0.changed() => r.is_ok(),
            r = rx1.changed() => r.is_ok(),
        };
        if !alive {
            break;
        }
    }

    panic!("the commander game did not reach a terminal state within the iteration cap");
}

/// The receiver's own life if `seat` is the receiver, else that seat's life as an
/// opponent — so either seat's view yields any seat's life.
fn current_life(view: &GameView, seat: usize) -> Option<i32> {
    if seat_of(&view.you) == seat {
        return Some(view.me.life);
    }
    view.opponents
        .iter()
        .find(|opp| seat_of(&opp.player_id) == seat)
        .map(|opp| opp.life)
}

/// The pinned seed for the deterministic commander game. Chosen so the aggressor
/// assembles and casts its commander, the victim trades a blocker into it once, and the
/// recast commander closes the game out on 21 commander damage.
const COMMANDER_SEED: u64 = 0x5EED_0398_C0DE_0001;

#[tokio::test]
async fn a_seeded_commander_game_ends_on_twenty_one_commander_damage_through_the_server() {
    let outcome = play_seeded_commander_game(COMMANDER_SEED).await;

    // The game reached a real terminal result on both seats.
    let result = outcome.terminals[AGGRESSOR]
        .result
        .as_ref()
        .expect("the aggressor's view is terminal");
    assert_eq!(
        outcome.terminals[VICTIM].result.as_ref(),
        Some(result),
        "both seats agree on the same terminal result",
    );

    // Acceptance: the game ends by 21 commander damage (CR 903.10a), the aggressor wins,
    // and the victim is the sole loser.
    assert_eq!(
        result.reason,
        GameOverReason::CommanderDamage,
        "the loss is attributed to commander damage, not life loss: {result:?}",
    );
    assert_eq!(result.winner.as_deref(), Some("p0"), "the aggressor wins");
    assert_eq!(
        result.losers,
        vec!["p1".to_string()],
        "the victim is the loser"
    );
    for view in &outcome.terminals {
        assert!(
            view.valid_actions.is_empty(),
            "a terminal view offers no actions",
        );
    }

    // Acceptance: a commander cast from the command zone *and* a recast paying the {2}
    // tax — the cast count reached two, and the {2} tax was observed owed in between.
    assert!(
        outcome.max_casts >= 2,
        "the commander was cast from the command zone and recast (casts = {})",
        outcome.max_casts,
    );
    assert!(
        outcome.max_tax_owed >= 2,
        "the recast was taxed at least {{2}} (max tax owed = {})",
        outcome.max_tax_owed,
    );

    // Acceptance: the commander-damage tally actually reached the lethal 21 (CR 903.10a).
    assert!(
        outcome.max_commander_damage >= 21,
        "the commander dealt 21+ combat damage to the victim (max = {})",
        outcome.max_commander_damage,
    );

    // The game was a commander game: it started at 40 life (CR 903.7), and the victim
    // fell to commander damage while still above zero life (so the reason is unambiguous).
    assert_eq!(
        outcome.aggressor_start_life, COMMANDER_LIFE,
        "a commander game starts at 40 life",
    );
    let victim_final = current_life(&outcome.terminals[AGGRESSOR], VICTIM)
        .expect("the aggressor's view carries the victim's life");
    assert!(
        victim_final > 0,
        "the victim lost to commander damage while still above zero life (life = {victim_final})",
    );

    // The seats actually traded a sequence of actions across the wire.
    let seats_that_acted: std::collections::HashSet<usize> =
        outcome.transcript.iter().map(|(s, _)| *s).collect();
    assert_eq!(
        seats_that_acted.len(),
        2,
        "both seats took decisions over the game",
    );
}

#[tokio::test]
async fn the_commander_game_is_deterministic_for_a_seed() {
    let first = play_seeded_commander_game(COMMANDER_SEED).await;
    let second = play_seeded_commander_game(COMMANDER_SEED).await;

    assert_eq!(
        first.transcript, second.transcript,
        "the same seed reproduces the same action-by-action transcript",
    );
    assert_eq!(
        first.terminals[AGGRESSOR].result, second.terminals[AGGRESSOR].result,
        "the same seed reproduces the same winner, loser, and reason",
    );
}
