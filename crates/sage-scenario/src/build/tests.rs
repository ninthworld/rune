//! Tests for the scenario builder: that a file becomes the position it describes, and that
//! a file that cannot become one is refused by the field an author would go and change.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::scenario::parse;
use sage_engine::controller_of;
use sage_engine::CounterKind;
use std::sync::OnceLock;

/// The bundled catalog, parsed once for the whole test binary.
fn db() -> &'static CardDatabase {
    static DB: OnceLock<CardDatabase> = OnceLock::new();
    DB.get_or_init(|| CardDatabase::bundled().expect("the bundled catalog must load"))
}

fn built(text: &str) -> Position {
    let scenario = parse(text).expect("the scenario parses");
    build(&scenario, db()).expect("the scenario builds")
}

fn failure(text: &str) -> ScenarioError {
    let scenario = parse(text).expect("the scenario parses");
    build(&scenario, db()).expect_err("the scenario is refused")
}

/// A duel where seat 0 holds Murder and two Swamps and seat 1 has a creature to kill.
const DUEL: &str = r#"
    turn = 6
    seed = 7

    [[players]]
    name = "You"
    life = 18
    hand = ["murder"]
    library = ["forest", "swamp"]

    [[players.battlefield]]
    card = "swamp"

    [[players.battlefield]]
    card = "swamp"

    [[players]]
    ai = "random"
    life = 20
    library = ["mountain"]

    [[players.battlefield]]
    card = "colossal_dreadmaw"
"#;

#[test]
fn a_position_is_built_where_the_file_says_it_is() {
    let position = built(DUEL);
    let state = &position.state;
    assert_eq!(state.turn, 6);
    assert_eq!(state.step, Step::PrecombatMain);
    assert_eq!(state.active_player, PlayerId(0));
    assert_eq!(state.priority, PlayerId(0));
    assert!(state.mulligan.is_none(), "a scenario is past the mulligan");
    assert_eq!(state.players[0].life, 18);
    assert_eq!(state.players[0].hand.len(), 1);
    assert_eq!(state.battlefield.len(), 3);
    assert_eq!(position.seats[0].ai, None);
    assert_eq!(position.seats[1].ai, Some(AiKind::Random));
    assert_eq!(position.seats[0].name.as_deref(), Some("You"));
}

#[test]
fn every_object_and_card_identity_is_unique() {
    // Authors never write an internal id, so the builder owes them ids that cannot
    // collide — across zones, across seats, and against the battlefield.
    let state = built(DUEL).state;
    let mut instances: Vec<u64> = state
        .players
        .iter()
        .flat_map(|player| {
            player
                .hand
                .iter()
                .chain(&player.library)
                .chain(&player.graveyard)
                .chain(&player.exile)
                .chain(&player.command)
                .map(|card| card.id.0)
        })
        .chain(state.battlefield.iter().map(|perm| perm.instance.0))
        .collect();
    let total = instances.len();
    instances.sort_unstable();
    instances.dedup();
    assert_eq!(instances.len(), total, "no two physical cards share an id");

    let mut permanents: Vec<u64> = state.battlefield.iter().map(|perm| perm.id.0).collect();
    let count = permanents.len();
    permanents.sort_unstable();
    permanents.dedup();
    assert_eq!(permanents.len(), count, "no two permanents share an id");
    assert!(
        state.next_object_id > instances.last().copied().unwrap_or(0),
        "the counter is past every id it minted",
    );
}

#[test]
fn the_library_is_authored_top_card_first() {
    // The file reads like a person describes a library; the engine draws off the end
    // of the vector, so the first authored card must be the one drawn next.
    let mut state = built(DUEL).state;
    let drawn = state.players[0].draw();
    assert!(drawn);
    let top = state.players[0].hand.last().expect("a card was drawn");
    assert_eq!(
        db().card(top.card).map(|data| data.functional_id.as_str()),
        Some("forest"),
    );
}

#[test]
fn the_same_file_and_seed_build_the_same_position() {
    // Determinism is the whole reason a scenario carries a seed: two runs of the same
    // file are the same game, down to every id.
    assert_eq!(built(DUEL).state, built(DUEL).state);
}

#[test]
fn an_established_permanent_is_not_summoning_sick_and_a_marked_one_is() {
    let position = built(
        r#"
        turn = 6

        [[players]]
        [[players.battlefield]]
        card = "colossal_dreadmaw"

        [[players.battlefield]]
        card = "onakke_ogre"
        summoning_sick = true

        [[players]]
        ai = "random"
        "#,
    );
    let state = &position.state;
    assert!(
        state.battlefield[0].entered_turn < state.players[0].turn_began,
        "an established permanent entered before its controller's turn began",
    );
    assert!(
        state.battlefield[1].entered_turn >= state.players[0].turn_began,
        "a sick permanent entered on or after it",
    );
}

#[test]
fn a_seat_that_has_not_had_a_turn_cannot_hold_an_established_permanent() {
    // Turn 1, seat 0 active: seat 1 has begun no turn, so nothing it controls can have
    // been under its control since one. Saying so beats handing back a sick permanent
    // the file asked not to be sick.
    let error = failure(
        r#"
        turn = 1

        [[players]]
        [[players]]
        ai = "random"

        [[players.battlefield]]
        card = "colossal_dreadmaw"
        "#,
    );
    assert_eq!(
        error,
        ScenarioError::CannotBeEstablished {
            seat: 1,
            card: "colossal_dreadmaw".to_string(),
        }
    );
}

#[test]
fn a_stolen_permanent_is_controlled_through_the_layer_system() {
    // `controller` is not a written-down field: it is the CR 613 layer 2 effect the
    // engine already computes every controller answer from, so every rule agrees.
    let position = built(
        r#"
        turn = 4

        [[players]]
        [[players]]
        ai = "random"

        [[players.battlefield]]
        card = "colossal_dreadmaw"
        controller = 0
        "#,
    );
    let state = &position.state;
    let perm = &state.battlefield[0];
    assert_eq!(perm.controller, PlayerId(1), "seat 1 still owns it");
    assert_eq!(
        controller_of(state, perm),
        PlayerId(0),
        "seat 0 controls it",
    );
}

#[test]
fn an_attachment_is_resolved_by_label_in_either_order() {
    let position = built(
        r#"
        turn = 5

        [[players]]
        [[players.battlefield]]
        card = "aether_tunnel"
        attached_to = "bear"

        [[players.battlefield]]
        card = "colossal_dreadmaw"
        label = "bear"

        [[players]]
        ai = "random"
        "#,
    );
    let state = &position.state;
    let host = state.battlefield[1].id;
    assert_eq!(state.battlefield[0].attached_to, Some(host));
    assert_eq!(state.battlefield[1].attached_to, None);
}

#[test]
fn counters_damage_and_tapped_state_land_on_the_permanent() {
    let position = built(
        r#"
        turn = 5

        [[players]]
        [[players.battlefield]]
        card = "colossal_dreadmaw"
        tapped = true
        damage = 3
        counters = { plus_one_plus_one = 2 }

        [[players]]
        ai = "random"
        "#,
    );
    let perm = &position.state.battlefield[0];
    assert!(perm.tapped);
    assert_eq!(perm.damage, 3);
    assert_eq!(perm.counter_count(CounterKind::PlusOnePlusOne), 2);
}

#[test]
fn mana_already_in_a_pool_is_carried() {
    let position = built(
        r#"
        [[players]]
        mana = { black = 2, colorless = 1 }

        [[players]]
        ai = "random"
        "#,
    );
    let pool = &position.state.players[0].mana_pool;
    assert_eq!((pool.black, pool.colorless, pool.green), (2, 1, 0));
}

#[test]
fn an_unknown_card_names_the_seat_the_zone_and_the_id() {
    let error = failure(
        r#"
        [[players]]
        [[players]]
        ai = "random"
        graveyard = ["not_a_real_card"]
        "#,
    );
    assert_eq!(
        error,
        ScenarioError::UnknownCard {
            seat: 1,
            site: Site::Graveyard,
            card: "not_a_real_card".to_string(),
        }
    );
    assert!(error.to_string().contains("functional_id"));
}

#[test]
fn an_unknown_ai_lists_the_kinds_that_would_have_worked() {
    let error = failure(
        r#"
        [[players]]
        [[players]]
        ai = "grandmaster"
        "#,
    );
    let ScenarioError::UnknownAi { seat, kind, known } = error else {
        panic!("expected an unknown-AI error, got {error:?}");
    };
    assert_eq!((seat, kind.as_str()), (1, "grandmaster"));
    assert!(known.contains(&"random".to_string()));
}

#[test]
fn a_one_seat_scenario_and_a_zero_turn_are_refused() {
    assert_eq!(failure("[[players]]\n"), ScenarioError::NotEnoughPlayers(1));
    assert_eq!(
        failure("turn = 0\n[[players]]\n[[players]]\nai = \"random\"\n"),
        ScenarioError::TurnIsZero
    );
}

#[test]
fn a_seat_index_past_the_table_is_refused_by_the_field_that_named_it() {
    assert_eq!(
        failure("active_player = 4\n[[players]]\n[[players]]\nai = \"random\"\n"),
        ScenarioError::NoSuchSeat {
            field: "active_player",
            seat: 4,
            seats: 2,
        }
    );
    assert_eq!(
        failure("priority = 9\n[[players]]\n[[players]]\nai = \"random\"\n"),
        ScenarioError::NoSuchSeat {
            field: "priority",
            seat: 9,
            seats: 2,
        }
    );
    assert_eq!(
        failure(
            r#"
            [[players]]
            [[players.battlefield]]
            card = "colossal_dreadmaw"
            controller = 3

            [[players]]
            ai = "random"
            "#
        ),
        ScenarioError::NoSuchSeat {
            field: "controller",
            seat: 3,
            seats: 2,
        }
    );
}

#[test]
fn a_duplicate_or_dangling_label_is_refused() {
    assert_eq!(
        failure(
            r#"
            [[players]]
            [[players.battlefield]]
            card = "colossal_dreadmaw"
            label = "it"

            [[players.battlefield]]
            card = "onakke_ogre"
            label = "it"

            [[players]]
            ai = "random"
            "#
        ),
        ScenarioError::DuplicateLabel {
            label: "it".to_string()
        }
    );
    assert_eq!(
        failure(
            r#"
            [[players]]
            [[players.battlefield]]
            card = "aether_tunnel"
            attached_to = "nobody"

            [[players]]
            ai = "random"
            "#
        ),
        ScenarioError::UnknownLabel {
            seat: 0,
            label: "nobody".to_string(),
        }
    );
}

#[test]
fn a_permanent_attached_to_itself_is_refused() {
    assert_eq!(
        failure(
            r#"
            [[players]]
            [[players.battlefield]]
            card = "aether_tunnel"
            label = "me"
            attached_to = "me"

            [[players]]
            ai = "random"
            "#
        ),
        ScenarioError::AttachedToItself {
            label: "me".to_string()
        }
    );
}

#[test]
fn a_position_that_ends_on_the_first_check_is_refused_before_it_is_served() {
    // A seat at zero life is a decided game (CR 704.5a) that the engine has not been
    // asked about yet, so `is_over` would say no and the first click would end it. The
    // runner refuses it here instead, while there is still something to point at.
    let error = failure(
        r#"
        [[players]]
        life = 0

        [[players]]
        ai = "random"
        "#,
    );
    assert_eq!(error, ScenarioError::LethalLife { seat: 0, life: 0 });
    assert!(error.to_string().contains("704.5a"));
}

#[test]
fn the_seat_holding_priority_is_offered_something_to_do() {
    // The positive of the check above: the duel above is playable, and the engine says
    // so through the same `valid_actions` the room will call.
    let position = built(DUEL);
    assert!(!valid_actions(&position.state, db()).is_empty());
}
