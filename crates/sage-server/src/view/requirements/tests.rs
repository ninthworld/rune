//! Target requirements and prompts, as the view projects them.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::test_support::{fixture, id_in};
use crate::view::test_support::put_permanent;
use sage_engine::{Effect, PlayerRef};

/// A trigger waiting to be aimed is offered from *both* places a player looks for
/// it: the object on the stack that is holding the game up, and the permanent whose
/// ability is asking. Binding it to the source alone left the choice reachable only
/// by clicking a card on the battlefield, while the thing plainly stuck was sitting
/// on the stack.
#[test]
fn a_trigger_awaiting_targets_is_reachable_from_the_stack_and_from_its_source() {
    let db = CardDatabase::bundled().unwrap();
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    let source = put_permanent(
        &mut state,
        fixture("skymarch_bloodletter"),
        PlayerId(0),
        false,
        false,
    );
    let ability = StackId(state.mint_id());
    state.stack.push(sage_engine::StackObject {
        id: ability,
        controller: PlayerId(0),
        kind: StackObjectKind::Ability {
            source: source.into(),
            origin: AbilityOrigin::Triggered,
            effects: vec![Effect::LoseLife {
                player_ref: PlayerRef::TargetOpponent,
                amount: 1,
            }],
        },
        targets: Vec::new(),
    });

    let view = personalized_view(&state, &db, PlayerId(0));
    let aim = view
        .valid_actions
        .iter()
        .find(|a| a.kind == "choose_targets")
        .expect("the trigger's controller is asked to aim it");
    assert_eq!(
        aim.subject,
        vec![stack_entity_id(ability), permanent_entity_id(source)],
        "the stack object first: it is what the player sees waiting",
    );
    assert_eq!(aim.requirements.len(), 1, "one slot for the one target");
    assert_eq!(
        aim.requirements[0].candidates,
        vec![player_id(PlayerId(1))],
        "the only opponent",
    );
}

/// The declare-attackers view advertises the engine's attacker candidates
/// (CR 508.1a) as a multi-select `requirements` slot, and a returned selection
/// resolves to a `DeclareAttackers` naming exactly those permanents (issue #140).
#[test]
fn issue_140_declare_attackers_projects_candidates_and_a_selection_resolves() {
    let db = CardDatabase::bundled().unwrap();
    let mut state = GameState::new_two_player();
    state.turn = 2;
    state.step = Step::DeclareAttackers;
    // An eligible attacker (untapped, non-sick creature) for the active player,
    // plus a tapped one that is not a candidate.
    let attacker = put_permanent(
        &mut state,
        fixture("walking_corpse"),
        PlayerId(0),
        false,
        false,
    );
    let _tapped = put_permanent(
        &mut state,
        fixture("walking_corpse"),
        PlayerId(0),
        true,
        false,
    );

    let view = personalized_view(&state, &db, PlayerId(0));
    let declare = view
        .valid_actions
        .iter()
        .find(|a| a.kind == "declare_attackers")
        .expect("the active player declares attackers");
    assert_eq!(declare.requirements.len(), 1);
    assert_eq!(declare.requirements[0].slot, "attackers");
    assert_eq!(
        declare.requirements[0].candidates,
        vec![permanent_entity_id(attacker)],
        "only the eligible attacker is a candidate",
    );

    let choose = ChooseAction {
        submission: String::new(),
        action_id: declare.id.clone(),
        token: declare.token.clone(),
        targets: vec![TargetChoice {
            slot: "attackers".to_string(),
            chosen: vec![permanent_entity_id(attacker)],
        }],
    };
    let resolved =
        resolve_action(&state, &db, PlayerId(0), &choose).expect("the selection resolves");
    assert_eq!(
        resolved,
        Action::DeclareAttackers {
            attackers: vec![Attack {
                attacker,
                defender: AttackTarget::Player(PlayerId(1)),
            }],
        },
    );

    // Declaring no attackers stays legal: the token-bound answer with an empty
    // selection resolves to an empty declaration (optional multi-select).
    let none = ChooseAction {
        submission: String::new(),
        action_id: declare.id.clone(),
        token: declare.token.clone(),
        targets: Vec::new(),
    };
    assert_eq!(
        resolve_action(&state, &db, PlayerId(0), &none),
        Some(Action::DeclareAttackers {
            attackers: Vec::new(),
        }),
    );
}

/// The declare-blockers view advertises one slot per declared attacker
/// (CR 509.1a), each listing the defender's eligible blockers, and a returned
/// blocker→attacker assignment resolves to a `DeclareBlockers` (issue #140).
#[test]
fn issue_140_declare_blockers_projects_candidates_and_a_selection_resolves() {
    let db = CardDatabase::bundled().unwrap();
    let mut state = GameState::new_two_player();
    state.turn = 2;
    state.step = Step::DeclareBlockers;
    // The defending player (seat 1) is deciding.
    state.priority = PlayerId(1);
    let attacker = put_permanent(
        &mut state,
        fixture("walking_corpse"),
        PlayerId(0),
        true,
        true,
    );
    let blocker = put_permanent(
        &mut state,
        fixture("walking_corpse"),
        PlayerId(1),
        false,
        false,
    );

    let view = personalized_view(&state, &db, PlayerId(1));
    let declare = view
        .valid_actions
        .iter()
        .find(|a| a.kind == "declare_blockers")
        .expect("the defender declares blockers");
    assert_eq!(
        declare.requirements.len(),
        1,
        "one slot per declared attacker"
    );
    assert_eq!(declare.requirements[0].slot, blocker_slot(attacker));
    assert_eq!(
        declare.requirements[0].candidates,
        vec![permanent_entity_id(blocker)],
    );

    let choose = ChooseAction {
        submission: String::new(),
        action_id: declare.id.clone(),
        token: declare.token.clone(),
        targets: vec![TargetChoice {
            slot: blocker_slot(attacker),
            chosen: vec![permanent_entity_id(blocker)],
        }],
    };
    let resolved =
        resolve_action(&state, &db, PlayerId(1), &choose).expect("the assignment resolves");
    assert_eq!(
        resolved,
        Action::DeclareBlockers {
            blocks: vec![Block { blocker, attacker }],
        },
    );
}

#[test]
fn issue_140_ability_target_requirements_project_and_a_selection_resolves() {
    // A Tapper artifact ({T}: Tap target creature) and a Bear to target.
    let json = r#"[
        {"schema_version":1,"functional_id":"tapper","name":"Tapper","types":["artifact"],"mana_cost":"",
         "abilities":[{"type":"activated","cost":[{"kind":"tap"}],
                      "effects":[{"kind":"tap","target":"any_creature"}]}]},
        {"schema_version":1,"functional_id":"bear","name":"Bear","types":["creature"],"mana_cost":"",
         "power":2,"toughness":2}
    ]"#;
    let db = CardDatabase::from_json(json).unwrap();
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    let tapper = put_permanent(&mut state, id_in(&db, "tapper"), PlayerId(0), false, false);
    let bear = put_permanent(&mut state, id_in(&db, "bear"), PlayerId(0), false, false);

    let view = personalized_view(&state, &db, PlayerId(0));
    let activate = view
        .valid_actions
        .iter()
        .find(|a| a.kind == "activate_ability")
        .expect("the Tapper's ability is activatable");
    assert_eq!(activate.subject, vec![permanent_entity_id(tapper)]);
    assert_eq!(activate.requirements.len(), 1, "one target slot");
    assert_eq!(activate.requirements[0].slot, "t0");
    assert_eq!(
        activate.requirements[0].candidates,
        vec![permanent_entity_id(bear)],
        "only the creature is a legal target (not the Tapper itself)",
    );

    let choose = ChooseAction {
        submission: String::new(),
        action_id: activate.id.clone(),
        token: activate.token.clone(),
        targets: vec![TargetChoice {
            slot: "t0".to_string(),
            chosen: vec![permanent_entity_id(bear)],
        }],
    };
    let resolved = resolve_action(&state, &db, PlayerId(0), &choose).expect("the target resolves");
    assert_eq!(
        resolved,
        Action::ActivateAbility {
            permanent: tapper,
            index: 0,
            targets: vec![Target::Permanent(bear)],
            payment: Vec::new(),
        },
    );

    // A target outside the advertised candidates (the Tapper itself) is rejected.
    let illegal = ChooseAction {
        submission: String::new(),
        action_id: activate.id.clone(),
        token: activate.token.clone(),
        targets: vec![TargetChoice {
            slot: "t0".to_string(),
            chosen: vec![permanent_entity_id(tapper)],
        }],
    };
    assert!(resolve_action(&state, &db, PlayerId(0), &illegal).is_none());
}

#[test]
fn multi_ability_activations_carry_distinguishable_rules_sentence_labels() {
    // A permanent with two activated abilities offers two actions; each must be
    // labeled with its OWN generated rules sentence (ADR 0008), not a shared
    // generic "Activate ability" — otherwise the dock renders identical buttons
    // the player cannot tell apart.
    let json = r#"[
        {"schema_version":1,"functional_id":"toolbox","name":"Toolbox","types":["artifact"],"mana_cost":"",
         "abilities":[
            {"type":"activated","cost":[{"kind":"tap"}],
             "effects":[{"kind":"add_mana","color":"green","amount":1}]},
            {"type":"activated","cost":[{"kind":"tap"}],
             "effects":[{"kind":"tap","target":"any_creature"}]}
         ]},
        {"schema_version":1,"functional_id":"bear","name":"Bear","types":["creature"],"mana_cost":"",
         "power":2,"toughness":2}
    ]"#;
    let db = CardDatabase::from_json(json).unwrap();
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    put_permanent(&mut state, id_in(&db, "toolbox"), PlayerId(0), false, false);
    put_permanent(&mut state, id_in(&db, "bear"), PlayerId(0), false, false);

    let view = personalized_view(&state, &db, PlayerId(0));
    let labels: Vec<&str> = view
        .valid_actions
        .iter()
        .filter(|a| a.kind == "activate_ability")
        .map(|a| a.label.as_str())
        .collect();
    assert_eq!(labels.len(), 2, "both abilities are offered");
    // Each label is that ability's cost-colon-effect sentence, and they differ.
    assert_ne!(labels[0], labels[1]);
    for label in &labels {
        assert!(
            label.starts_with("{T}: "),
            "cost leads the sentence: {label}"
        );
        assert_ne!(*label, "Activate ability");
    }
}

#[test]
fn issue_346_multi_block_projects_an_order_action_and_binds_the_permutation() {
    // A multi-blocked attacker projects an `order_combat_damage` action carrying
    // one `order` prompt over its blockers; a returned permutation binds back to
    // the concrete OrderCombatDamage action (CR 510.1, issue #346).
    let db = CardDatabase::bundled().unwrap();
    let mut state = GameState::new_two_player();
    state.turn = 2;
    state.step = Step::DeclareBlockers;
    state.active_player = PlayerId(0);
    state.priority = PlayerId(0);
    state.attackers_declared = true;
    state.blockers_declared = true;
    let attacker = put_permanent(&mut state, fixture("onakke_ogre"), PlayerId(0), true, true);
    let blk_a = put_permanent(
        &mut state,
        fixture("onakke_ogre"),
        PlayerId(1),
        false,
        false,
    );
    let blk_b = put_permanent(
        &mut state,
        fixture("onakke_ogre"),
        PlayerId(1),
        false,
        false,
    );
    for b in [blk_a, blk_b] {
        state
            .battlefield
            .iter_mut()
            .find(|p| p.id == b)
            .unwrap()
            .blocking = vec![attacker];
    }

    let view = personalized_view(&state, &db, PlayerId(0));
    let order = view
        .valid_actions
        .iter()
        .find(|a| a.kind == "order_combat_damage")
        .expect("the attacking player orders combat damage");
    assert_eq!(order.prompts.len(), 1);
    let Prompt::Order { items, slot, .. } = &order.prompts[0] else {
        panic!("expected an order prompt");
    };
    assert_eq!(slot, &format!("order_{}", attacker.0));
    assert_eq!(items.len(), 2, "both blockers are orderable");

    let choose = ChooseAction {
        submission: String::new(),
        action_id: order.id.clone(),
        token: order.token.clone(),
        targets: vec![TargetChoice {
            slot: format!("order_{}", attacker.0),
            chosen: vec![permanent_entity_id(blk_b), permanent_entity_id(blk_a)],
        }],
    };
    let resolved = resolve_action(&state, &db, PlayerId(0), &choose).expect("the order resolves");
    assert_eq!(
        resolved,
        Action::OrderCombatDamage {
            orders: vec![DamageOrder {
                attacker,
                blockers: vec![blk_b, blk_a],
            }],
        }
    );
}

#[test]
fn issue_345_declare_attackers_offers_a_defender_slot_per_attacker_in_multiplayer() {
    // With more than one opponent, the declare_attackers requirements enumerate a
    // defender choice per attacker candidate; a two-player game offers none.
    let db = CardDatabase::bundled().unwrap();
    let mut state = GameState::new_multiplayer(3);
    state.turn = 2;
    state.step = Step::DeclareAttackers;
    state.active_player = PlayerId(0);
    state.priority = PlayerId(0);
    let attacker = put_permanent(
        &mut state,
        fixture("walking_corpse"),
        PlayerId(0),
        false,
        false,
    );

    let view = personalized_view(&state, &db, PlayerId(0));
    let declare = view
        .valid_actions
        .iter()
        .find(|a| a.kind == "declare_attackers")
        .expect("the active player declares attackers");
    // The attackers multi-select, plus one defender slot for the candidate.
    assert!(declare.requirements.iter().any(|r| r.slot == "attackers"));
    let defender_req = declare
        .requirements
        .iter()
        .find(|r| r.slot == format!("defend_{}", attacker.0))
        .expect("a defender slot for the attacker candidate");
    assert_eq!(
        defender_req.candidates,
        vec![player_id(PlayerId(1)), player_id(PlayerId(2))],
        "both living opponents are defender candidates",
    );

    // A returned declaration pairing the attacker with seat 2 binds that defender.
    let choose = ChooseAction {
        submission: String::new(),
        action_id: declare.id.clone(),
        token: declare.token.clone(),
        targets: vec![
            TargetChoice {
                slot: "attackers".to_string(),
                chosen: vec![permanent_entity_id(attacker)],
            },
            TargetChoice {
                slot: format!("defend_{}", attacker.0),
                chosen: vec![player_id(PlayerId(2))],
            },
        ],
    };
    let resolved = resolve_action(&state, &db, PlayerId(0), &choose)
        .expect("the multiplayer declaration resolves");
    assert_eq!(
        resolved,
        Action::DeclareAttackers {
            attackers: vec![Attack {
                attacker,
                defender: AttackTarget::Player(PlayerId(2)),
            }],
        }
    );
}
