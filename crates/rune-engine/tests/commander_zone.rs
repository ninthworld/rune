//! The command zone end to end (issue #370): casting a commander from the command
//! zone with the escalating commander tax (CR 903.8), and the CR 903.9a choice to
//! return a commander that went to a graveyard or exile to the command zone.
//!
//! These drive the real [`apply_action`]/[`valid_actions`] pipeline; the commander
//! rides the *same* casting path as a hand spell (same stack object, same
//! resolution to the battlefield with a fresh identity), never a parallel one.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rune_engine::{
    apply_action, valid_actions, Action, CardDatabase, CardId, CardInstance, Color, CommanderState,
    Effect, GameEvent, GameState, Permanent, PermanentId, PlayerId, StackId, StackObject,
    StackObjectKind, Step, Target, TargetSpec,
};

/// An inline catalog with a legendary creature commander costing `{G}` and a
/// vanilla bystander creature, so the tax arithmetic ({G}, {2}{G}, {4}{G}) is easy
/// to read and does not depend on any bundled card's cost.
fn db() -> CardDatabase {
    let json = r#"[
        {"schema_version":1,"functional_id":"test_general","name":"Test General",
         "types":["creature"],"supertypes":["legendary"],"subtypes":["Elf"],
         "mana_cost":"{G}","colors":["green"],"power":2,"toughness":2},
        {"schema_version":1,"functional_id":"test_ally","name":"Test Ally",
         "types":["creature"],"subtypes":["Elf"],"mana_cost":"{G}","colors":["green"],
         "power":1,"toughness":1}
    ]"#;
    CardDatabase::from_json(json).unwrap()
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    db.card_id(&slug.to_string().try_into().unwrap()).unwrap()
}

/// A two-player scaffold at player 0's precombat main with player 0's commander (a
/// `{G}` creature) sitting in their command zone, designated with `casts` prior
/// casts. Returns the state and the command-zone commander instance.
fn commander_in_command_zone(db: &CardDatabase, casts: u32) -> (GameState, CardInstance) {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    let commander = state.new_instance(cid(db, "test_general"));
    state.players[0].command.push(commander);
    state.players[0].commander = Some(CommanderState {
        casts,
        ..CommanderState::new(commander.card, commander.id)
    });
    (state, commander)
}

/// Whether the command-zone cast of `card` is on offer.
fn cast_offered(state: &GameState, db: &CardDatabase, card: CardInstance) -> bool {
    valid_actions(state, db).contains(&Action::CastSpell {
        card,
        targets: Vec::new(),
    })
}

#[test]
fn cr_903_8_commander_is_castable_from_the_command_zone_when_a_normal_cast_would_be() {
    // The commander is offered as a normal CastSpell of the command-zone copy, at
    // sorcery speed with mana available — exactly the timing a hand creature obeys.
    let db = db();
    let (mut state, commander) = commander_in_command_zone(&db, 0);
    state.players[0].mana_pool.add(Color::Green, 1);
    assert!(cast_offered(&state, &db, commander));

    // Not offered with an empty pool (unpayable) …
    let mut broke = state.clone();
    broke.players[0].mana_pool = Default::default();
    assert!(!cast_offered(&broke, &db, commander));

    // … nor off-turn: a creature is sorcery-speed, so the opponent's turn is out.
    let mut off_turn = state.clone();
    off_turn.active_player = PlayerId(1);
    off_turn.priority = PlayerId(0);
    assert!(!cast_offered(&off_turn, &db, commander));
}

#[test]
fn cr_903_8_command_zone_cast_goes_on_the_stack_and_resolves_to_the_battlefield() {
    // Routing through the normal pipeline: casting removes the commander from the
    // command zone onto the stack, pays the cost, and on resolution it enters the
    // battlefield with a fresh permanent identity but its stable instance id.
    let db = db();
    let (mut state, commander) = commander_in_command_zone(&db, 0);
    state.players[0].mana_pool.add(Color::Green, 1);

    let state = apply_action(
        &state,
        &Action::CastSpell {
            card: commander,
            targets: Vec::new(),
        },
        &db,
    );
    assert_eq!(state.stack.len(), 1, "the commander is on the stack");
    assert!(
        state.players[0].command.is_empty(),
        "it left the command zone"
    );
    assert_eq!(
        state.players[0].mana_pool.green, 0,
        "the green pip was paid"
    );
    assert_eq!(
        state.players[0].commander.unwrap().casts,
        1,
        "one cast counted"
    );

    // Both players pass: it resolves onto the battlefield keeping its instance id.
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    assert!(state.stack.is_empty());
    let perm = state
        .battlefield
        .iter()
        .find(|p| p.card == cid(&db, "test_general"))
        .unwrap();
    assert_eq!(perm.instance, commander.id, "keeps its physical identity");
}

#[test]
fn cr_903_8_each_recast_costs_two_generic_more() {
    // CR 903.8: the tax is {2} generic per previous cast from the command zone.
    // Verified across three casts (0, +2, +4): each is castable with exactly its
    // taxed cost and not with one mana less.
    let db = db();
    for (casts, need) in [(0u32, 1u8), (1, 3), (2, 5)] {
        let (mut state, commander) = commander_in_command_zone(&db, casts);
        // One point of the mana is green (the {G} pip); the rest is generic tax.
        state.players[0].mana_pool.add(Color::Green, 1);
        state.players[0].mana_pool.colorless = need - 1;
        assert!(
            cast_offered(&state, &db, commander),
            "castable with the taxed cost ({} total for {casts} prior casts)",
            need
        );

        // One mana short: not payable, so not offered. Drop a generic point when
        // there is one (the taxed casts), otherwise the sole green pip.
        let mut short = state.clone();
        if short.players[0].mana_pool.colorless > 0 {
            short.players[0].mana_pool.colorless -= 1;
        } else {
            short.players[0].mana_pool.green -= 1;
        }
        assert!(
            !cast_offered(&short, &db, commander),
            "not castable one mana short of the taxed cost"
        );
    }
}

/// Put `instance` onto the battlefield under player 0 as a fresh permanent, returning
/// its minted permanent id. Used for both a designated commander and a plain
/// permanent, so the seam can be exercised on each.
fn put_permanent_on_battlefield(state: &mut GameState, instance: CardInstance) -> PermanentId {
    let id = PermanentId(state.mint_id());
    state.battlefield.push(Permanent {
        id,
        instance: instance.id,
        card: instance.card,
        controller: PlayerId(0),
        entered_turn: 0,
        ..Permanent::default()
    });
    id
}

/// Push an "exile target permanent" ability onto the stack aimed at `target`, so the
/// next two priority passes resolve it through the real effect-resolution path
/// ([`apply_action`] → resolve → the battlefield→exile seam). Player 0 controls it.
fn exile_ability_targeting(state: &mut GameState, target: PermanentId) {
    let sid = state.mint_id();
    state.stack.push(StackObject {
        id: StackId(sid),
        controller: PlayerId(0),
        kind: StackObjectKind::Ability {
            source: target,
            effects: vec![Effect::Exile {
                target: TargetSpec::AnyPermanent,
            }],
        },
        targets: vec![Target::Permanent(target)],
    });
}

#[test]
fn cr_903_9a_a_destroyed_commander_flags_the_return_decision() {
    // When the commander is put into a graveyard (here by lethal marked damage, a
    // state-based action) the return decision is flagged on its owner — the seam
    // is the battlefield-leaves move, not a replacement effect.
    let db = db();
    let (mut state, commander) = commander_in_command_zone(&db, 1);
    state.players[0].command.clear(); // it is on the battlefield, not the command zone
    let perm = put_permanent_on_battlefield(&mut state, commander);
    // Mark lethal damage (a 2/2 with 2 damage) so the SBA loop destroys it.
    state
        .battlefield
        .iter_mut()
        .find(|p| p.id == perm)
        .unwrap()
        .damage = 2;

    // A mana-ability-free way to run the pipeline's SBA loop: pass priority. The
    // commander dies during this transition.
    let state = apply_action(&state, &Action::PassPriority, &db);

    assert!(
        !state.battlefield.iter().any(|p| p.id == perm),
        "the commander was destroyed (CR 704.5g)"
    );
    assert!(
        state.players[0]
            .graveyard
            .iter()
            .any(|c| c.id == commander.id),
        "it went to the graveyard"
    );
    assert!(
        state.players[0].commander.unwrap().return_pending,
        "its owner is owed the CR 903.9a return decision"
    );
    // The tax count is untouched by the death — it is keyed to the designation.
    assert_eq!(state.players[0].commander.unwrap().casts, 1);
}

/// A scaffold where player 0's commander is already sitting in their graveyard with
/// the return decision pending and player 0 holding priority. Returns the state and
/// the commander instance. The exile side of CR 903.9a is exercised at the real
/// battlefield→exile boundary instead (see the exile tests below).
fn commander_awaiting_return(db: &CardDatabase) -> (GameState, CardInstance) {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    let commander = state.new_instance(cid(db, "test_general"));
    state.players[0].graveyard.push(commander);
    state.players[0].commander = Some(CommanderState {
        return_pending: true,
        ..CommanderState::new(commander.card, commander.id)
    });
    (state, commander)
}

#[test]
fn cr_903_9a_owner_is_offered_the_return_choice_and_accepting_moves_it_from_the_graveyard() {
    // With the decision pending, the owner's only actions are accept / decline /
    // concede. Accepting moves the commander to the command zone as a fresh object
    // (its instance id carries over), clears the decision, and logs the movement.
    let db = db();
    let (state, commander) = commander_awaiting_return(&db);

    let offered = valid_actions(&state, &db);
    assert_eq!(
        offered,
        vec![
            Action::ReturnCommanderToCommandZone { card: commander },
            Action::DeclineCommanderReturn { card: commander },
            Action::Concede,
        ],
        "a pending return is a forced choice: accept, decline, or concede"
    );

    let after = apply_action(
        &state,
        &Action::ReturnCommanderToCommandZone { card: commander },
        &db,
    );
    assert_eq!(
        after.players[0].command,
        vec![commander],
        "now in the command zone"
    );
    assert!(
        after.players[0].graveyard.is_empty(),
        "no longer in the graveyard"
    );
    assert!(
        !after.players[0].commander.unwrap().return_pending,
        "decision resolved"
    );
    assert!(
        after.log.iter().any(|e| matches!(
            &e.event,
            GameEvent::CommanderReturnedToCommandZone { player, card }
                if *player == PlayerId(0) && card.id == commander.id
        )),
        "the movement is recorded in the log"
    );
    // Back in the command zone it is castable again (the tax keeps climbing).
    let mut recast = after;
    recast.players[0].mana_pool.add(Color::Green, 1);
    assert!(cast_offered(&recast, &db, commander));
}

#[test]
fn cr_903_9a_declining_leaves_the_commander_where_it_went() {
    // Declining clears the decision and leaves the commander in the graveyard; the
    // owner then resumes normal play (the choice is not offered again).
    let db = db();
    let (state, commander) = commander_awaiting_return(&db);

    let after = apply_action(
        &state,
        &Action::DeclineCommanderReturn { card: commander },
        &db,
    );
    assert_eq!(
        after.players[0].graveyard,
        vec![commander],
        "it stayed in the graveyard"
    );
    assert!(
        after.players[0].command.is_empty(),
        "not moved to the command zone"
    );
    assert!(
        !after.players[0].commander.unwrap().return_pending,
        "decision resolved"
    );
    // Normal play resumes: the return choice is gone, passing priority is offered.
    assert!(valid_actions(&after, &db).contains(&Action::PassPriority));
    assert!(!valid_actions(&after, &db)
        .contains(&Action::ReturnCommanderToCommandZone { card: commander }));
}

// ----- The battlefield→exile seam end to end (issue #397) -----
//
// The exile side of CR 903.9a is proven at the *real* boundary: an exile effect
// resolves through [`apply_action`], moving the commander off the battlefield into
// exile via the one battlefield→exile seam — no hand-set `return_pending`.

#[test]
fn cr_903_9a_exiling_the_commander_flags_the_return_and_accepting_moves_it_from_exile() {
    // An exile effect resolving onto the commander moves it to its owner's exile and
    // flags the CR 903.9a return; accepting puts it in the command zone as a fresh
    // object (its instance id carries over), and both the engine log records it.
    let db = db();
    let (mut state, commander) = commander_in_command_zone(&db, 1);
    state.players[0].command.clear(); // it is on the battlefield, not the command zone
    let perm = put_permanent_on_battlefield(&mut state, commander);
    exile_ability_targeting(&mut state, perm);

    // Both players pass: the exile ability resolves, moving the commander into exile
    // and flagging its owner's decision — the seam is the battlefield-leaves move.
    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    assert!(
        !state.battlefield.iter().any(|p| p.id == perm),
        "it left the battlefield"
    );
    assert!(
        state.players[0].exile.iter().any(|c| c.id == commander.id),
        "it went to exile"
    );
    assert!(
        state.players[0].commander.unwrap().return_pending,
        "its owner is owed the CR 903.9a return decision"
    );
    // The tax count is untouched by the exile — it is keyed to the designation.
    assert_eq!(state.players[0].commander.unwrap().casts, 1);

    // The owner is offered the return; accepting moves it to the command zone.
    assert!(valid_actions(&state, &db)
        .contains(&Action::ReturnCommanderToCommandZone { card: commander }));
    let after = apply_action(
        &state,
        &Action::ReturnCommanderToCommandZone { card: commander },
        &db,
    );
    assert_eq!(
        after.players[0].command,
        vec![commander],
        "now in the command zone"
    );
    assert!(after.players[0].exile.is_empty(), "no longer in exile");
    assert!(
        !after.players[0].commander.unwrap().return_pending,
        "decision resolved"
    );
    assert!(
        after.log.iter().any(|e| matches!(
            &e.event,
            GameEvent::CommanderReturnedToCommandZone { player, card }
                if *player == PlayerId(0) && card.id == commander.id
        )),
        "the movement is recorded in the log"
    );
    // Fresh object: back in the command zone it is castable again, at the taxed cost
    // ({2}{G} after one prior cast — the tax survived the exile and return).
    let mut recast = after;
    recast.players[0].mana_pool.add(Color::Green, 1);
    recast.players[0].mana_pool.colorless = 2;
    assert!(cast_offered(&recast, &db, commander));
}

#[test]
fn cr_903_9a_declining_leaves_the_exiled_commander_in_exile() {
    // Declining after a real exile leaves the commander in exile and clears the
    // decision, logging nothing (the card did not move); normal play resumes.
    let db = db();
    let (mut state, commander) = commander_in_command_zone(&db, 0);
    state.players[0].command.clear();
    let perm = put_permanent_on_battlefield(&mut state, commander);
    exile_ability_targeting(&mut state, perm);

    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);
    assert!(state.players[0].commander.unwrap().return_pending);

    let after = apply_action(
        &state,
        &Action::DeclineCommanderReturn { card: commander },
        &db,
    );
    assert!(
        after.players[0].exile.iter().any(|c| c.id == commander.id),
        "it stayed in exile"
    );
    assert!(
        after.players[0].command.is_empty(),
        "not moved to the command zone"
    );
    assert!(
        !after.players[0].commander.unwrap().return_pending,
        "decision resolved"
    );
    assert!(
        !after
            .log
            .iter()
            .any(|e| matches!(e.event, GameEvent::CommanderReturnedToCommandZone { .. })),
        "declining records no movement event"
    );
    // Normal play resumes: the return choice is gone, passing priority is offered.
    assert!(valid_actions(&after, &db).contains(&Action::PassPriority));
    assert!(!valid_actions(&after, &db)
        .contains(&Action::ReturnCommanderToCommandZone { card: commander }));
}

#[test]
fn a_non_commander_permanent_exiled_through_the_seam_lands_in_exile_with_zone_change_identity() {
    // A permanent with no commander designation exiled through the seam simply lands
    // in its owner's exile as its physical instance — the battlefield `PermanentId` is
    // dropped, the same fresh-identity rule every zone change follows — and no return
    // decision is owed to anyone.
    let db = db();
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    let ally = state.new_instance(cid(&db, "test_ally"));
    let perm = put_permanent_on_battlefield(&mut state, ally);
    exile_ability_targeting(&mut state, perm);

    let state = apply_action(&state, &Action::PassPriority, &db);
    let state = apply_action(&state, &Action::PassPriority, &db);

    assert!(
        !state.battlefield.iter().any(|p| p.id == perm),
        "it left the battlefield"
    );
    assert_eq!(
        state.players[0].exile,
        vec![ally],
        "its physical instance is in exile (a new PermanentId only on re-entry)"
    );
    assert!(
        state.players[0].commander.is_none() && state.players[1].commander.is_none(),
        "there is no commander designation to owe a return"
    );
    // No return choice exists anywhere: ordinary priority play is what's offered.
    assert!(valid_actions(&state, &db).contains(&Action::PassPriority));
}
