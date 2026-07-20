//! Building target `requirement` slots and their candidate labels.

use super::*;

/// The stable requirement-slot id for the blockers assigned to `attacker` in a
/// [`Action::DeclareBlockers`] projection. One slot per declared attacker, keyed by
/// the attacker's permanent id, so the returned choice names which attacker the
/// selected blockers are assigned to. Recomputed (never parsed) on resolution.
pub(crate) fn blocker_slot(attacker: PermanentId) -> String {
    format!("block_{}", attacker.0)
}

/// The combat-damage assignment-order slot for a multi-blocked attacker (CR 510.1,
/// issue #346), keyed by the attacker's permanent id so the returned permutation
/// names which attacker it orders. Recomputed (never parsed) on resolution.
pub(crate) fn damage_order_slot(attacker: PermanentId) -> String {
    format!("order_{}", attacker.0)
}

/// One `order` prompt per attacker that owes a combat-damage assignment order
/// ([`attackers_needing_damage_order`], issue #346): the prompt's `items` are that
/// attacker's blockers in battlefield order, and the client returns a permutation of
/// them. Empty when no attacker is multi-blocked (a choice-free action).
pub(crate) fn damage_order_prompts(state: &GameState, db: &CardDatabase) -> Vec<Prompt> {
    attackers_needing_damage_order(state)
        .into_iter()
        .map(|attacker| {
            let items: Vec<String> = state
                .battlefield
                .iter()
                .filter(|p| p.blocking == Some(attacker))
                .map(|p| permanent_entity_id(p.id))
                .collect();
            Prompt::Order {
                slot: damage_order_slot(attacker),
                prompt: format!(
                    "Order damage assignment for {}",
                    permanent_card_name(state, attacker, db)
                ),
                items,
            }
        })
        .collect()
}

/// The per-attacker defender-choice slot of a multiplayer [`Action::DeclareAttackers`]
/// (CR 508.1a, issue #345), keyed by the attacker's permanent id so the returned
/// choice names which attacker the selected defender is assigned to — the exact
/// parallel of [`blocker_slot`]. Recomputed (never parsed) on resolution.
pub(crate) fn defender_slot(attacker: PermanentId) -> String {
    format!("defend_{}", attacker.0)
}

/// The bottoming requirement slot for a mulligan [`Action::Keep`] (CR 103.5,
/// London): the [`bottom_requirement`] candidates (the deciding seat's hand cards)
/// projected as a single multi-select slot asking for `count` cards. Empty for a
/// first-hand keep (nothing owed), so that keep stays a plain, choice-free action.
pub(crate) fn keep_requirements(state: &GameState, action: &Action) -> Vec<TargetRequirement> {
    match bottom_requirement(state, action) {
        Some(req) => vec![TargetRequirement {
            slot: "bottom".to_string(),
            prompt: format!("Put {} card(s) on the bottom of your library", req.count),
            candidates: req.candidates.into_iter().map(target_entity_id).collect(),
        }],
        None => Vec::new(),
    }
}

/// The attacker-declaration requirement slots (CR 508.1a): the engine's
/// [`attacker_candidates`] as one multi-select `attackers` slot, plus — only when the
/// active player has more than one opponent to attack (issue #341/#345) — one
/// `defend_<id>` slot per attacker candidate listing the defender candidates
/// ([`defender_candidates`], as player entity ids) that attacker may be assigned to.
///
/// Empty when no creature may attack, so declaring no attackers stays a plain,
/// choice-free action. In a two-player game the sole opponent is the only defender,
/// so no `defend_*` slot is offered and the wire is exactly as before — the client
/// gains no extra step (issue #347); [`bind_attackers`] assigns that sole defender.
pub(crate) fn attacker_requirements(
    state: &GameState,
    db: &CardDatabase,
) -> Vec<TargetRequirement> {
    let candidates = attacker_candidates(state, db);
    if candidates.is_empty() {
        return Vec::new();
    }
    let mut reqs = vec![TargetRequirement {
        slot: "attackers".to_string(),
        prompt: "Choose which creatures attack".to_string(),
        candidates: candidates
            .iter()
            .copied()
            .map(permanent_entity_id)
            .collect(),
    }];
    // Multiplayer: each attacker chooses a defending player. With a single opponent
    // there is nothing to choose, so no defender slots are offered.
    let defenders = defender_candidates(state);
    if defenders.len() > 1 {
        let defender_ids: Vec<String> = defenders.iter().copied().map(player_id).collect();
        for attacker in candidates {
            reqs.push(TargetRequirement {
                slot: defender_slot(attacker),
                prompt: format!(
                    "Choose whom {} attacks",
                    permanent_card_name(state, attacker, db)
                ),
                candidates: defender_ids.clone(),
            });
        }
    }
    reqs
}

/// The blocker-declaration requirement slots (CR 509.1a) for the player who owes
/// the current declaration ([`pending_blocker_declarer`]): one slot per attacker
/// *attacking that player*, each listing the eligible blockers they control
/// ([`blocker_candidates_for`]). Empty when there is nothing for this declarer to
/// block or no creature to block with, so declaring no blockers stays a plain,
/// choice-free action. In a two-player game the sole opponent is the declarer and
/// every attacker attacks them, so this is unchanged; with attackers split across
/// several defenders (issue #344) each declarer sees only their own sub-combat.
pub(crate) fn blocker_requirements(state: &GameState, db: &CardDatabase) -> Vec<TargetRequirement> {
    let Some(declarer) = pending_blocker_declarer(state) else {
        return Vec::new();
    };
    let attackers: Vec<_> = declared_attackers(state)
        .into_iter()
        .filter(|&attacker| attacking_defender_of(state, attacker) == Some(declarer))
        .collect();
    let blockers = blocker_candidates_for(state, declarer, db);
    if attackers.is_empty() || blockers.is_empty() {
        return Vec::new();
    }
    let candidates: Vec<String> = blockers.into_iter().map(permanent_entity_id).collect();
    attackers
        .into_iter()
        .map(|attacker| TargetRequirement {
            slot: blocker_slot(attacker),
            prompt: format!(
                "Choose blockers for {}",
                permanent_card_name(state, attacker, db)
            ),
            candidates: candidates.clone(),
        })
        .collect()
}

/// The ability-target requirement slots (ADR 0009 §Enumeration, deferral #73): the
/// engine's per-slot [`target_requirements`] candidate sets projected one slot each
/// (`t0`, `t1`, …), reusing the same content-binding machinery as the mulligan and
/// combat multi-selects. Empty for a non-targeting ability.
pub(crate) fn ability_requirements(
    state: &GameState,
    db: &CardDatabase,
    action: &Action,
) -> Vec<TargetRequirement> {
    target_requirements(state, db, action)
        .into_iter()
        .enumerate()
        .map(|(index, req)| TargetRequirement {
            slot: format!("t{index}"),
            prompt: target_spec_prompt(req.spec).to_string(),
            candidates: req.candidates.into_iter().map(target_entity_id).collect(),
        })
        .collect()
}

/// The dock label for one ability activation: the ability's own generated rules
/// sentence (`ability_text`, ADR 0018), resolved through the same
/// [`abilities_of`] index the engine action names — so the words a player clicks
/// are exactly the words the card prints, and two abilities on one permanent
/// never share a label. Falls back to the old generic label if the permanent or
/// index cannot be resolved (defensive: an offered action always names a live
/// ability).
pub(crate) fn ability_label(
    state: &GameState,
    db: &CardDatabase,
    permanent: PermanentId,
    index: usize,
) -> String {
    state
        .battlefield
        .iter()
        .find(|perm| perm.id == permanent)
        .and_then(|perm| {
            let name = card_name(perm.card, db);
            abilities_of(db, perm.card)
                .get(index)
                .map(|ability| ability_text(&name, ability))
        })
        .unwrap_or_else(|| "Activate ability".to_string())
}

/// The display name of the permanent `id` on the battlefield, for a human prompt,
/// or a stable placeholder if it is not found.
fn permanent_card_name(state: &GameState, id: PermanentId, db: &CardDatabase) -> String {
    state
        .battlefield
        .iter()
        .find(|perm| perm.id == id)
        .map(|perm| card_name(perm.card, db))
        .unwrap_or_else(|| "the attacker".to_string())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::test_support::{fixture, id_in};
    use crate::view::test_support::put_permanent;

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
                    defender: PlayerId(1),
                }],
            },
        );

        // Declaring no attackers stays legal: the token-bound answer with an empty
        // selection resolves to an empty declaration (optional multi-select).
        let none = ChooseAction {
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
            action_id: activate.id.clone(),
            token: activate.token.clone(),
            targets: vec![TargetChoice {
                slot: "t0".to_string(),
                chosen: vec![permanent_entity_id(bear)],
            }],
        };
        let resolved =
            resolve_action(&state, &db, PlayerId(0), &choose).expect("the target resolves");
        assert_eq!(
            resolved,
            Action::ActivateAbility {
                permanent: tapper,
                index: 0,
                targets: vec![Target::Permanent(bear)],
            },
        );

        // A target outside the advertised candidates (the Tapper itself) is rejected.
        let illegal = ChooseAction {
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
        // labeled with its OWN generated rules sentence (ADR 0018), not a shared
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
                .blocking = Some(attacker);
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
            action_id: order.id.clone(),
            token: order.token.clone(),
            targets: vec![TargetChoice {
                slot: format!("order_{}", attacker.0),
                chosen: vec![permanent_entity_id(blk_b), permanent_entity_id(blk_a)],
            }],
        };
        let resolved =
            resolve_action(&state, &db, PlayerId(0), &choose).expect("the order resolves");
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
                    defender: PlayerId(2),
                }],
            }
        );
    }
}
