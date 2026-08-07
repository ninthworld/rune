//! What a death fires: the dies trigger collected from a lethal-damage diff (CR 700.4).

#![allow(clippy::unwrap_used)]

use super::*;
use crate::apply::test_support::*;

#[test]
fn issue_151_dies_trigger_fires_from_lethal_combat_damage_cr_700_4() {
    // CR 700.4 / 603.6c: a creature put into a graveyard by lethal combat
    // damage (CR 704.5g) dies, firing its dies trigger. The 2/2 Lurker attacks
    // into a 4/5 Basilisk blocker, takes 4, and dies; its controller then draws.
    let db = combat_db();
    let mut state = at_declare_attackers();
    let lurker = place_permanent(&mut state, id_in(&db, "test_lurker"), PlayerId(0), false, 0);
    let blocker = place_permanent(
        &mut state,
        id_in(&db, "test_basilisk"),
        PlayerId(1),
        false,
        0,
    );
    let draw = state.new_instance(id_in(&db, "test_boar"));
    state.players[0].library = vec![draw];

    let after = run_combat(
        &state,
        vec![lurker],
        vec![Block {
            blocker,
            attacker: lurker,
        }],
        &db,
    );

    // The lurker died through the leaves-battlefield seam; its dies trigger is a
    // synthetic stack entry that has not resolved yet (CR 603.3b).
    assert!(
        !alive(&after, lurker),
        "the 2/2 took 4 combat damage and died"
    );
    assert_eq!(after.stack.len(), 1, "the dies trigger is on the stack");
    assert!(after.players[0].hand.is_empty(), "it has not resolved yet");

    // A full priority round resolves it: player 0 draws.
    let after = pass_full_round(&after, &db);
    assert!(after.stack.is_empty());
    assert!(
        after.players[0].hand.contains(&draw),
        "the dies trigger drew its controller a card (CR 700.4)"
    );
}

#[test]
fn issue_151_dies_trigger_fires_from_a_destroy_effect_cr_701_7() {
    // CR 701.7 → 700.4: a `Destroy` effect routes the creature to its graveyard
    // through the same seam, so the dies trigger fires exactly as it does for a
    // combat death.
    use crate::ability::TargetSpec;
    let db = combat_db();
    let (mut state, lurker, draw) = state_with_lurker(&db, 0);
    push_ability(
        &mut state,
        lurker,
        vec![Effect::Destroy {
            target: TargetSpec::AnyCreature,
        }],
        vec![Target::Permanent(lurker)],
    );

    // Resolve the destroy: the lurker dies and its dies trigger replaces it.
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    assert!(!alive(&state, lurker), "the Destroy killed the lurker");
    assert_eq!(state.stack.len(), 1, "the dies trigger is on the stack");
    assert!(state.players[0].hand.is_empty(), "it has not resolved yet");

    // Resolve the dies trigger: player 0 draws.
    let state = pass_full_round(&state, &db);
    assert!(state.stack.is_empty());
    assert!(state.players[0].hand.contains(&draw));
}

#[test]
fn issue_151_dies_trigger_fires_from_a_minus_one_counter_toughness_drop() {
    // CR 704.5g → 700.4: a `-1/-1` counter drops the 2/2 Lurker to a 2/1, making
    // its 1 marked damage lethal; the SBA loop destroys it through the seam and
    // the dies trigger fires.
    use crate::ability::TargetSpec;
    use crate::state::CounterKind;
    let db = combat_db();
    let (mut state, lurker, draw) = state_with_lurker(&db, 1);
    push_ability(
        &mut state,
        lurker,
        vec![Effect::PutCounters {
            count_amount: None,
            targets: crate::ability::TargetCount::Exactly(1),
            target: TargetSpec::AnyCreature,
            counter: CounterKind::MinusOneMinusOne,
            count: 1,
        }],
        vec![Target::Permanent(lurker)],
    );

    // Resolve the counter: toughness 2→1, 1 marked damage is now lethal, the
    // lurker dies, and its dies trigger lands on the stack.
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    assert!(
        !alive(&state, lurker),
        "the -1/-1 toughness drop made the marked damage lethal (CR 704.5g)"
    );
    assert_eq!(state.stack.len(), 1, "the dies trigger is on the stack");

    // Resolve the dies trigger: player 0 draws.
    let state = pass_full_round(&state, &db);
    assert!(state.stack.is_empty());
    assert!(state.players[0].hand.contains(&draw));
}

#[test]
fn issue_151_dies_trigger_is_a_synthetic_stack_entry_resolving_after_priority_cr_603_3b() {
    // CR 603.3b: an ability that triggers during the state-based-action check is
    // put on the stack the next time a player would receive priority, not
    // resolved immediately. After the death-causing action, the trigger sits on
    // the stack with a player holding priority and the draw has not happened; it
    // resolves only once priority passes around.
    use crate::ability::TargetSpec;
    let db = combat_db();
    let (mut state, lurker, draw) = state_with_lurker(&db, 0);
    push_ability(
        &mut state,
        lurker,
        vec![Effect::Destroy {
            target: TargetSpec::AnyCreature,
        }],
        vec![Target::Permanent(lurker)],
    );

    // The action that kills the lurker leaves its dies trigger on the stack —
    // one synthetic ability entry, unresolved, with priority handed to a player.
    let paused = apply_action(&state, &Action::PassPriority, &db);
    let paused = apply_action(&paused, &Action::PassPriority, &db);
    assert_eq!(paused.stack.len(), 1);
    assert!(matches!(
        paused.stack[0].kind,
        StackObjectKind::Ability { source, .. } if source.permanent() == Some(lurker)
    ));
    assert_eq!(
        paused.consecutive_passes, 0,
        "priority was handed out fresh with the trigger on the stack (CR 603.3b)"
    );
    assert!(
        paused.players[0].library.contains(&draw),
        "the trigger has not resolved, so nothing is drawn yet"
    );

    // Only a full priority round resolves the synthetic entry.
    let resolved = pass_full_round(&paused, &db);
    assert!(resolved.stack.is_empty());
    assert!(resolved.players[0].hand.contains(&draw));
}
