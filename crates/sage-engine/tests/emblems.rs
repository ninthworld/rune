//! Emblems (CR 114) — the zoneless, controller-scoped object, and the two ability
//! paths that had to grow a second source list to reach it (issue #620).
//!
//! Every test drives the **real** [`apply_action`] pipeline. The emblem shapes are
//! exercised through inline `test_*` definitions (ADR 0009) rather than through the
//! shipped planeswalkers, because an emblem is a mechanism and the cards that make one
//! are a separate question — `tests/planeswalkers.rs` answers that one.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, characteristics, target_requirements, valid_actions, Action, CardDatabase,
    CardId, FunctionalId, GameState, Keyword, Permanent, PermanentId, PlayerId, Step,
};

/// An inline catalog of emblem-makers, one per shape the two source lists have to reach.
///
/// No M19 planeswalker's ultimate is a *bare* emblem — each also creates tokens or
/// searches a library — so the emblem mechanism itself is exercised by definitions built
/// for it (ADR 0009). Each `test_*` maker is an enchantment with a cost-free activated
/// ability, so a test can create the emblem in one action without a loyalty budget.
fn emblem_db() -> CardDatabase {
    let json = r#"[
        {"schema_version":1,"functional_id":"test_anthem_maker","name":"Test Anthem Maker",
         "types":["enchantment"],"mana_cost":"{W}","colors":["white"],
         "abilities":[{"type":"activated","cost":[],
            "effects":[{"kind":"create_emblem","abilities":[
              {"type":"static","affects":{"scope":"creatures_you_control"},
               "modification":{"kind":"power_toughness","power":2,"toughness":2}},
              {"type":"static","affects":{"scope":"creatures_you_control"},
               "modification":{"kind":"grant_keyword","keyword":"indestructible"}}]}]}]},
        {"schema_version":1,"functional_id":"test_upkeep_maker","name":"Test Upkeep Maker",
         "types":["enchantment"],"mana_cost":"{B}","colors":["black"],
         "abilities":[{"type":"activated","cost":[],
            "effects":[{"kind":"create_emblem","abilities":[
              {"type":"triggered",
               "event":{"beginning_of_step":{"step":"upkeep","whose_turn":"yours"}},
               "effects":[{"kind":"gain_life","player_ref":"controller","amount":3}]}]}]}]},
        {"schema_version":1,"functional_id":"test_bear","name":"Test Bear",
         "types":["creature"],"subtypes":["Bear"],"mana_cost":"{1}{G}","colors":["green"],
         "power":2,"toughness":2}
    ]"#;
    CardDatabase::from_json(json).unwrap()
}

/// The interned handle for an authored identity.
fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// A two-player game at player 0's precombat main, priority held, stack empty.
///
/// Both libraries are stocked, because several of these tests walk many turns and an
/// empty library would end the game to the CR 704.5c decking loss long before the
/// emblem's persistence had been demonstrated.
fn main_phase(db: &CardDatabase) -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    let bear = cid(db, "test_bear");
    for seat in [PlayerId(0), PlayerId(1)] {
        for _ in 0..60 {
            let instance = state.new_instance(bear);
            state.players[seat.0].library.push(instance);
        }
    }
    state
}

/// Put a permanent of `slug` onto the battlefield under `controller`.
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

/// Activate ability 0 of `permanent` and let it resolve (both players pass).
fn activate(state: &GameState, db: &CardDatabase, permanent: PermanentId) -> GameState {
    let after = apply_action(
        state,
        &Action::ActivateAbility {
            permanent,
            index: 0,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        db,
    );
    assert_ne!(&after, state, "the activation was rejected");
    let after = apply_action(&after, &Action::PassPriority, db);
    apply_action(&after, &Action::PassPriority, db)
}

/// Take one action on behalf of whoever holds priority, passing where passing is
/// offered — enough to walk a game forward without caring which step owes what.
fn advance(state: &GameState, db: &CardDatabase) -> GameState {
    let offered = valid_actions(state, db);
    let chosen = if offered.contains(&Action::PassPriority) {
        Action::PassPriority
    } else {
        offered
            .into_iter()
            .find(|a| a != &Action::Concede)
            .expect("some action is always available")
    };
    let after = apply_action(state, &chosen, db);
    assert_ne!(&after, state, "the pipeline stalled");
    after
}

#[test]
fn issue_620_an_effect_creates_an_emblem_under_a_named_player() {
    // CR 114.3: a resolving effect gives one player an emblem. It carries the abilities
    // the effect wrote down and nothing else — no zone, no counters, no tapped state,
    // because an emblem has no characteristics but its abilities (CR 114.1).
    let db = emblem_db();
    let mut state = main_phase(&db);
    let maker = place(&mut state, &db, "test_anthem_maker", PlayerId(0));
    assert!(state.emblems.is_empty(), "no emblem before the ability");

    let after = activate(&state, &db, maker);

    assert_eq!(after.emblems.len(), 1);
    assert_eq!(after.emblems[0].controller, PlayerId(0));
    assert_eq!(after.emblems[0].abilities.len(), 2);
    // Its id comes from the same monotonic counter every other object's does, so it is
    // unique across the game and usable as a CR 613.7 timestamp.
    assert!(after.emblems[0].id > 0);
}

#[test]
fn issue_620_an_emblems_static_ability_outlives_its_source_and_the_rest_of_the_game() {
    // The load-bearing property: the object that made the emblem is gone and the
    // emblem's continuous effect is untouched. A permanent's static ability would have
    // ended the instant its source left the battlefield; an emblem is in no zone, so
    // there is nothing for it to leave.
    let db = emblem_db();
    let mut state = main_phase(&db);
    let maker = place(&mut state, &db, "test_anthem_maker", PlayerId(0));
    let bear = place(&mut state, &db, "test_bear", PlayerId(0));
    assert_eq!(characteristics(&state, bear, &db).power, Some(2));

    let mut after = activate(&state, &db, maker);
    assert_eq!(characteristics(&after, bear, &db).power, Some(4));

    // The maker leaves the battlefield entirely.
    after.battlefield.retain(|p| p.id != maker);
    assert_eq!(
        characteristics(&after, bear, &db).power,
        Some(4),
        "the emblem's anthem does not follow the object that made it"
    );

    // And it is still in force many turns later.
    let mut later = after;
    for _ in 0..40 {
        later = advance(&later, &db);
    }
    assert_eq!(later.emblems.len(), 1, "nothing collected the emblem");
    assert_eq!(characteristics(&later, bear, &db).power, Some(4));
}

#[test]
fn issue_620_an_emblems_static_ability_modifies_the_right_permanents_and_no_others() {
    // "Creatures you control" is the *emblem's* controller, exactly as it is a
    // permanent's — an emblem is not on the battlefield, but it is controlled, and that
    // is the whole of what the selector needs.
    let db = emblem_db();
    let mut state = main_phase(&db);
    let maker = place(&mut state, &db, "test_anthem_maker", PlayerId(0));
    let mine = place(&mut state, &db, "test_bear", PlayerId(0));
    let theirs = place(&mut state, &db, "test_bear", PlayerId(1));

    let after = activate(&state, &db, maker);

    assert_eq!(characteristics(&after, mine, &db).power, Some(4));
    assert!(characteristics(&after, mine, &db)
        .keywords
        .contains(&Keyword::Indestructible));
    assert_eq!(
        characteristics(&after, theirs, &db).power,
        Some(2),
        "an opponent's creature is outside the emblem's scope"
    );
    assert!(!characteristics(&after, theirs, &db)
        .keywords
        .contains(&Keyword::Indestructible));
    // A creature that arrives *after* the emblem picks the anthem up, because a static
    // ability selects a live set rather than a frozen one (CR 611.3).
    let mut later = after;
    let latecomer = place(&mut later, &db, "test_bear", PlayerId(0));
    assert_eq!(characteristics(&later, latecomer, &db).power, Some(4));
}

#[test]
fn issue_620_an_emblems_static_ability_never_enters_stored_state() {
    // ADR 0005 §1: a continuous effect derived from a source in force is *derived*,
    // never pushed. If an emblem's anthem were stored there would be an entry to prune
    // and no seam that prunes it — an emblem never leaves.
    let db = emblem_db();
    let mut state = main_phase(&db);
    let maker = place(&mut state, &db, "test_anthem_maker", PlayerId(0));
    place(&mut state, &db, "test_bear", PlayerId(0));

    let after = activate(&state, &db, maker);

    assert!(
        after.static_effects.is_empty(),
        "an emblem's static ability must be derived, not stored"
    );
}

#[test]
fn issue_620_an_emblems_triggered_ability_fires_on_its_controllers_turns_only() {
    // The step-trigger contract of issue #607, from a source that is in no zone: once
    // per crossing, on its controller's turns, for the rest of the game. Walked over
    // real turns so the "your upkeep" scope is decided by the engine's own rotation.
    let db = emblem_db();
    let mut state = main_phase(&db);
    let maker = place(&mut state, &db, "test_upkeep_maker", PlayerId(0));
    let state = activate(&state, &db, maker);
    assert_eq!(state.emblems.len(), 1);

    let start_life = state.players[0].life;
    let start_turn = state.turn;
    let mut walked = state;
    // Five turn boundaries from player 0's turn 1: their own upkeeps on turns 3 and 5,
    // the opponent's on turns 2 and 4.
    while walked.turn < start_turn + 5 {
        walked = advance(&walked, &db);
    }

    let gained = walked.players[0].life - start_life;
    assert_eq!(
        gained, 6,
        "three life on each of the controller's two upkeeps, and none on the opponent's"
    );
    assert_eq!(
        walked.players[1].life,
        sage_engine::STARTING_LIFE,
        "the opponent gains nothing from an emblem they do not have"
    );
}

#[test]
fn issue_620_nothing_destroys_exiles_bounces_or_targets_an_emblem() {
    // CR 114.5: an emblem is not a permanent and is in no zone. Two facts carry that,
    // and between them there is nothing left to write:
    //
    // - **Untargetable by construction.** A chosen target is a player, a permanent, a
    //   card, or a spell ([`Target`]), and an emblem is none of the four — so no target
    //   spec can name one, present or future, without a new variant being added first.
    //   What is asserted here is the consequence: every candidate a real action offers
    //   names an object that is *not* the emblem.
    // - **Uncollectable.** The state-based-action loop walks the battlefield, and the
    //   emblem is not on it, so wiping the board leaves it exactly where it was.
    let db = emblem_db();
    let mut state = main_phase(&db);
    let maker = place(&mut state, &db, "test_anthem_maker", PlayerId(0));
    let bear = place(&mut state, &db, "test_bear", PlayerId(0));
    let mut after = activate(&state, &db, maker);
    let emblem_id = after.emblems[0].id;

    // Every candidate the maker's own targeted-action surface offers is a battlefield
    // object; none of them is the emblem's id.
    let battlefield_ids: Vec<u64> = after.battlefield.iter().map(|p| p.id.0).collect();
    assert!(!battlefield_ids.contains(&emblem_id));
    for action in valid_actions(&after, &db) {
        for requirement in target_requirements(&after, &db, &action) {
            for target in requirement.candidates {
                assert!(
                    !format!("{target:?}").contains(&emblem_id.to_string()),
                    "{action:?} offered something naming the emblem"
                );
            }
        }
    }

    // Wiping the battlefield — every permanent, including the one that made it — leaves
    // the emblem untouched.
    after.battlefield.clear();
    let settled = apply_action(&after, &Action::PassPriority, &db);
    assert_eq!(
        settled.emblems.len(),
        1,
        "no state-based action collects it"
    );
    assert_eq!(settled.emblems[0].id, emblem_id);
    assert!(!settled.battlefield.iter().any(|p| p.id.0 == bear.0));
}

#[test]
fn issue_620_two_emblems_stack_and_each_keeps_its_own_controller() {
    // Two emblems are two objects, each with its own timestamp and its own "you".
    let db = emblem_db();
    let mut state = main_phase(&db);
    let mine = place(&mut state, &db, "test_anthem_maker", PlayerId(0));
    let theirs = place(&mut state, &db, "test_anthem_maker", PlayerId(1));
    let bear = place(&mut state, &db, "test_bear", PlayerId(0));

    let state = activate(&state, &db, mine);
    // Hand the second maker's controller priority so they may activate it.
    let mut opponents_turn = state;
    opponents_turn.priority = PlayerId(1);
    opponents_turn.active_player = PlayerId(1);
    let after = activate(&opponents_turn, &db, theirs);

    assert_eq!(after.emblems.len(), 2);
    assert_ne!(after.emblems[0].id, after.emblems[1].id);
    assert_eq!(
        characteristics(&after, bear, &db).power,
        Some(4),
        "only the emblem its controller has applies to their creature"
    );
}
