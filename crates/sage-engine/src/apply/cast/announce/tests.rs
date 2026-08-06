//! Announcing and paying: a land played, an ability activated, a spell put on the stack.

#![allow(clippy::unwrap_used)]

use super::*;
use crate::apply::test_support::*;

#[test]
fn forest_mana_ability_adds_green_without_using_the_stack() {
    let db = db();
    let mut state = slice_state();
    let inst = state.new_instance(fixture("forest"));
    let id = state.mint_id();
    state.battlefield.push(Permanent {
        id: PermanentId(id),
        instance: inst.id,
        printed: fixture("forest").into(),
        controller: PlayerId(0),
        tapped: false,
        entered_turn: 0,
        attacking: None,
        blocking: Vec::new(),
        skips_untap: false,
        dealt_damage: false,
        damage: 0,
        counters: Default::default(),
        attached_to: None,
        chosen_color: None,
        named_card: None,
        copied: None,
    });
    let after = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: PermanentId(id),
            index: 0,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    assert_eq!(after.players[0].mana_pool.green, 1);
    assert!(after.battlefield[0].tapped);
    assert!(after.stack.is_empty());
}

#[test]
fn mana_ability_does_not_pass_priority() {
    let db = db();
    let mut state = slice_state();
    let inst = state.new_instance(fixture("forest"));
    let id = state.mint_id();
    state.battlefield.push(Permanent {
        id: PermanentId(id),
        instance: inst.id,
        printed: fixture("forest").into(),
        controller: PlayerId(0),
        tapped: false,
        entered_turn: 0,
        attacking: None,
        blocking: Vec::new(),
        skips_untap: false,
        dealt_damage: false,
        damage: 0,
        counters: Default::default(),
        attached_to: None,
        chosen_color: None,
        named_card: None,
        copied: None,
    });
    let after = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: PermanentId(id),
            index: 0,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    assert_eq!(after.priority, PlayerId(0));
    assert_eq!(after.consecutive_passes, 0);
}

#[test]
fn casting_a_creature_moves_it_to_the_stack_and_pays_mana() {
    let db = db();
    let mut state = slice_state();
    state.players[0].mana_pool.add(Color::Green, 1);
    let scout = hand_instance(&state, 0, fixture("llanowar_elves"));
    let after = apply_action(
        &state,
        &Action::CastSpell {
            card: scout,
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    assert_eq!(after.stack.len(), 1);
    assert_eq!(after.players[0].mana_pool.green, 0);
    assert!(!after.players[0].hand.iter().any(|c| c.id == scout.id));
}

#[test]
fn issue_155_tapland_enters_tapped_with_no_untapped_window_cr_614_1c() {
    // CR 614.1c/614.12: a land with an "enters tapped" self-replacement is tapped
    // the instant it is on the battlefield. Tranquil Expanse is played as a land
    // (CR 116.2a): the resulting permanent is already tapped, and because a {T}
    // mana ability is unpayable while tapped, no action to tap it for mana is
    // offered this same priority window — there is no observable untapped state.
    let db = db();
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    let land = state.new_instance(fixture("tranquil_expanse"));
    state.players[0].hand = vec![land];

    let after = apply_action(&state, &Action::PlayLand { card: land }, &db);

    assert_eq!(after.battlefield.len(), 1);
    let perm = &after.battlefield[0];
    assert!(
        perm.tapped,
        "the tapland is tapped the moment it enters (CR 614.1c/614.12)"
    );
    // No ActivateAbility for the tapland is on offer: its {T} abilities can't be
    // paid while it is tapped, so it cannot be tapped for mana this turn.
    assert!(
        !valid_actions(&after, &db).iter().any(
            |a| matches!(a, Action::ActivateAbility { permanent, .. } if *permanent == perm.id)
        ),
        "a tapland offers no mana ability the turn it enters — no untapped window"
    );
}

#[test]
fn issue_51_duplicate_cards_have_distinct_instances_and_routable_actions() {
    // Two copies of the same printed card (two Forests) in one hand must be
    // individually addressable: distinct instance ids, one PlayLand action
    // per copy, and applying one action plays that exact copy — not "the
    // first matching copy".
    let db = db();
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    let forest_a = state.new_instance(fixture("forest"));
    let forest_b = state.new_instance(fixture("forest"));
    state.players[0].hand = vec![forest_a, forest_b];

    // Same printed card, but two distinct physical instances.
    assert_eq!(forest_a.card, forest_b.card);
    assert_ne!(forest_a.id, forest_b.id);

    // The engine offers one land action per copy, each naming its own copy.
    let plays: Vec<CardInstance> = valid_actions(&state, &db)
        .into_iter()
        .filter_map(|action| match action {
            Action::PlayLand { card } => Some(card),
            _ => None,
        })
        .collect();
    assert_eq!(plays.len(), 2);
    assert!(plays.contains(&forest_a));
    assert!(plays.contains(&forest_b));

    // Routing the action for the second copy removes exactly that copy,
    // leaving the first untouched in hand.
    let after = apply_action(&state, &Action::PlayLand { card: forest_b }, &db);
    assert_eq!(after.players[0].hand, vec![forest_a]);
    assert_eq!(after.battlefield.len(), 1);
    assert_eq!(after.battlefield[0].instance, forest_b.id);
}
