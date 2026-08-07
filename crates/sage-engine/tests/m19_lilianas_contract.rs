//! Liliana's Contract: a card that ends the game by winning it (issue #727).
//!
//! `When this enchantment enters, you draw four cards and you lose 4 life.`
//! `At the beginning of your upkeep, if you control four or more Demons with different
//! names, you win the game.`
//!
//! Two things here are new. The condition counts **names** rather than permanents, so
//! four copies of one Demon is one name and not four; and the payoff is the first effect
//! that ends a game without anyone's life reaching zero. Winning is written as what it
//! does to everyone else — the engine derives a winner from who has lost (CR 104.2a) —
//! so the assertions are about [`GameState::result`], which is the same derivation the
//! server projects.
//!
//! Every test drives the real [`apply_action`] pipeline and the real trigger walk.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, forced_declaration_without_choice, priority_has_no_meaningful_action, Action,
    CardDatabase, CardId, CardType, Color, FunctionalId, GameState, LossReason, Permanent,
    PermanentId, PlayerId, Printed, Step, TokenData,
};

/// Enough steps to walk a turn or two; a settle that has not arrived by then is a hang.
const STEP_CAP: usize = 400;

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// A two-player game at the start of turn 1, with libraries deep enough that the walk
/// never trips the CR 704.5c decking loss and empty hands so cleanup never asks for a
/// discard.
///
/// The filler is a vanilla creature rather than a land, because a land in hand is a
/// *meaningful action* and the settle would rightly stop for it — with an empty pool a
/// creature is uncastable, so nothing in hand is ever a decision.
fn fresh_game(db: &CardDatabase) -> GameState {
    let mut state = GameState::new_two_player();
    for seat in 0..2 {
        let library: Vec<_> = (0..12)
            .map(|_| state.new_instance(cid(db, "bogstomper")))
            .collect();
        state.players[seat].library = library;
    }
    state
}

/// Put a card permanent onto the battlefield under `controller`.
fn place(
    state: &mut GameState,
    db: &CardDatabase,
    slug: &str,
    controller: PlayerId,
) -> PermanentId {
    let card = cid(db, slug);
    let instance = state.new_instance(card).id;
    let id = PermanentId(state.mint_id());
    state.battlefield.push(Permanent {
        id,
        instance,
        printed: card.into(),
        controller,
        ..Default::default()
    });
    id
}

/// Put a 5/5 black Demon **token** called `name` onto the battlefield (CR 111.4 — a
/// token's name is its own, which is what the count compares).
fn demon_token(state: &mut GameState, controller: PlayerId, name: &str) -> PermanentId {
    let id = PermanentId(state.mint_id());
    state.battlefield.push(Permanent {
        id,
        instance: sage_engine::CardInstanceId(9_000 + id.0),
        printed: Printed::Token(Box::new(TokenData {
            name: name.to_string(),
            types: vec![CardType::Creature],
            subtypes: vec!["Demon".to_string()],
            colors: vec![Color::Black],
            power: Some(5),
            toughness: Some(5),
            ..Default::default()
        })),
        controller,
        ..Default::default()
    });
    id
}

/// The room's settle policy, replayed (`room::input::auto_action_for`), plus one thing
/// the room leaves to a player: a board of Demons *could* attack, and this card is not
/// about combat, so the walk declares none rather than stalling on a real choice.
fn auto_action(state: &GameState, db: &CardDatabase) -> Option<Action> {
    if priority_has_no_meaningful_action(state, db) {
        return Some(Action::PassPriority);
    }
    forced_declaration_without_choice(state, db).or(match state.step {
        Step::DeclareAttackers => Some(Action::DeclareAttackers {
            attackers: Vec::new(),
        }),
        Step::DeclareBlockers => Some(Action::DeclareBlockers { blocks: Vec::new() }),
        _ => None,
    })
}

/// Settle forward under [`auto_action`] until `done` holds — or until the game is over,
/// which is the one place a settle legitimately runs out of actions.
fn settle_until(
    state: &GameState,
    db: &CardDatabase,
    done: impl Fn(&GameState) -> bool,
) -> GameState {
    let mut state = state.clone();
    for _ in 0..STEP_CAP {
        if done(&state) || state.is_over() {
            return state;
        }
        let action = auto_action(&state, db).unwrap_or_else(|| {
            panic!("the settle stalled at turn {} {:?}", state.turn, state.step)
        });
        state = apply_action(&state, &action, db);
    }
    panic!("the settle ran past its cap without reaching the goal");
}

/// A board where seat 0 has the Contract and `demons` Demons, walked to its next upkeep.
fn upkeep_with(demons: &[&str]) -> (CardDatabase, GameState) {
    let db = db();
    let mut state = fresh_game(&db);
    place(&mut state, &db, "liliana_s_contract", PlayerId(0));
    for name in demons {
        if *name == "demon_of_catastrophes" {
            place(&mut state, &db, name, PlayerId(0));
        } else {
            demon_token(&mut state, PlayerId(0), name);
        }
    }
    // Turn 1 is seat 0's, and the trigger is checked as its upkeep begins. Settling to
    // the precombat main means the upkeep — and anything it put on the stack — is behind
    // us.
    let settled = settle_until(&state, &db, |s| {
        s.turn == 1 && s.step == Step::PrecombatMain
    });
    (db, settled)
}

/// The enters trigger: four cards, four life, both halves of one sentence.
///
/// Cast for real rather than placed, because "enters" is a transition and a permanent
/// that was always there never took one.
#[test]
fn issue_727_it_draws_four_and_costs_four_life_as_it_enters() {
    let db = db();
    let state = fresh_game(&db);
    let mut state = settle_until(&state, &db, |s| {
        s.turn == 1 && s.step == Step::PrecombatMain
    });
    let contract = state.new_instance(cid(&db, "liliana_s_contract"));
    state.players[0].hand.push(contract);
    state.players[0].mana_pool.add(Color::Black, 2);
    state.players[0].mana_pool.add_colorless(3);
    let hand = state.players[0].hand.len();
    let life = state.players[0].life;
    let library = state.players[0].library.len();

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: contract,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    // The spell resolves, the permanent enters, and its trigger goes on the stack and
    // resolves in turn — three rounds of priority the settle walks for us.
    let state = settle_until(&state, &db, |s| {
        s.stack.is_empty() && s.battlefield.iter().any(|p| p.controller == PlayerId(0))
    });
    let state = settle_until(&state, &db, |s| {
        s.stack.is_empty() && s.step == Step::Upkeep
    });

    assert_eq!(
        state.players[0].hand.len(),
        hand - 1 + 4,
        "the Contract left the hand and four cards arrived"
    );
    assert_eq!(state.players[0].library.len(), library - 4, "off the top");
    assert_eq!(state.players[0].life, life - 4, "and four life paid");
}

/// Four Demons, four names: the game is won.
#[test]
fn issue_727_four_differently_named_demons_win_the_game() {
    let (_, state) = upkeep_with(&[
        "demon_of_catastrophes",
        "Ravenous Demon",
        "Shadow Demon",
        "Bloodgorged Demon",
    ]);

    let result = state.result().expect("the game is over");
    assert_eq!(
        result.winner,
        Some(PlayerId(0)),
        "the Contract's controller"
    );
    assert_eq!(result.losers, vec![PlayerId(1)], "and nobody else remains");
    assert_eq!(
        result.reason,
        LossReason::OpponentWon,
        "lost because someone else won, not because anything happened to them"
    );
    // Nothing took anyone to zero: the game ended on the card's own terms.
    assert!(
        state.players[1].life > 0,
        "the loser is at {} life",
        state.players[1].life
    );
}

/// Four Demons, **one** name: the whole point of the clause.
#[test]
fn issue_727_four_demons_of_one_name_are_one_name() {
    let (_, state) = upkeep_with(&[
        "Ravenous Demon",
        "Ravenous Demon",
        "Ravenous Demon",
        "Ravenous Demon",
    ]);

    assert!(
        !state.is_over(),
        "four permanents is not four names, so the condition does not hold"
    );
    assert!(state.players.iter().all(|player| !player.has_lost));
}

/// Three names is not four, however many Demons carry them.
#[test]
fn issue_727_three_names_across_five_demons_do_not_win() {
    let (_, state) = upkeep_with(&[
        "demon_of_catastrophes",
        "Ravenous Demon",
        "Ravenous Demon",
        "Shadow Demon",
        "Shadow Demon",
    ]);

    assert!(!state.is_over(), "three names, and the card asks for four");
}

/// The count is of **Demons**, so a fourth name on something else is not a fourth Demon.
#[test]
fn issue_727_a_non_demon_does_not_make_up_the_fourth_name() {
    let db = db();
    let mut state = fresh_game(&db);
    place(&mut state, &db, "liliana_s_contract", PlayerId(0));
    place(&mut state, &db, "demon_of_catastrophes", PlayerId(0));
    demon_token(&mut state, PlayerId(0), "Ravenous Demon");
    demon_token(&mut state, PlayerId(0), "Shadow Demon");
    // A Bird is a fourth differently-named creature and not a fourth Demon.
    let id = PermanentId(state.mint_id());
    state.battlefield.push(Permanent {
        id,
        instance: sage_engine::CardInstanceId(9_500),
        printed: Printed::Token(Box::new(TokenData {
            name: "Bird".to_string(),
            types: vec![CardType::Creature],
            subtypes: vec!["Bird".to_string()],
            colors: vec![Color::Blue],
            power: Some(1),
            toughness: Some(1),
            ..Default::default()
        })),
        controller: PlayerId(0),
        ..Default::default()
    });
    let state = settle_until(&state, &db, |s| {
        s.turn == 1 && s.step == Step::PrecombatMain
    });

    assert!(!state.is_over(), "three Demons and a Bird is three Demons");
}

/// The Demons have to be **yours**: an opponent's board does not pay off your Contract.
#[test]
fn issue_727_an_opponents_demons_do_not_count() {
    let db = db();
    let mut state = fresh_game(&db);
    place(&mut state, &db, "liliana_s_contract", PlayerId(0));
    place(&mut state, &db, "demon_of_catastrophes", PlayerId(0));
    for name in ["Ravenous Demon", "Shadow Demon", "Bloodgorged Demon"] {
        demon_token(&mut state, PlayerId(1), name);
    }
    let state = settle_until(&state, &db, |s| {
        s.turn == 1 && s.step == Step::PrecombatMain
    });

    assert!(
        !state.is_over(),
        "the count says *you control*, and three of these are not yours"
    );
}

/// The condition is checked on **your** upkeep, and it keeps checking: a board that only
/// becomes lethal later wins on the next one (CR 603.6a).
#[test]
fn issue_727_the_condition_is_asked_again_next_upkeep() {
    let db = db();
    let mut state = fresh_game(&db);
    place(&mut state, &db, "liliana_s_contract", PlayerId(0));
    place(&mut state, &db, "demon_of_catastrophes", PlayerId(0));
    demon_token(&mut state, PlayerId(0), "Ravenous Demon");
    demon_token(&mut state, PlayerId(0), "Shadow Demon");
    // Turn 1's upkeep sees three names and does nothing.
    let mut state = settle_until(&state, &db, |s| {
        s.turn == 1 && s.step == Step::PrecombatMain
    });
    assert!(!state.is_over(), "three names on the first upkeep");

    // The fourth arrives, and seat 1's upkeep in between is not seat 0's.
    demon_token(&mut state, PlayerId(0), "Bloodgorged Demon");
    let state = settle_until(&state, &db, |s| {
        s.turn == 2 && s.step == Step::PrecombatMain
    });
    assert!(
        !state.is_over(),
        "turn 2 is the opponent's upkeep, and this trigger says *your*"
    );

    let state = settle_until(&state, &db, |s| {
        s.turn == 3 && s.step == Step::PrecombatMain
    });
    let result = state
        .result()
        .expect("the game is over on the next own upkeep");
    assert_eq!(result.winner, Some(PlayerId(0)));
}
