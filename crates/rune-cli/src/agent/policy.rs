//! The rule-based agent policy (issue #159): a deterministic, network-free policy
//! that plays a full, legal game from a [`GameView`] + its `valid_actions` alone —
//! never any engine access, no game logic beyond heuristics over the view.
//!
//! [`choose_action`] picks one offered action (mulligan keep, cleanup discard, combat
//! declarations, main-phase development, else pass) and [`fill_answers`] fills that
//! action's target and prompt slots. The [`RuleBasedAgent`] wires them into the
//! [`Agent`](super::Agent) trait the session runtime drives. Split out of the session
//! runtime by size (docs/coding-standards.md, File size); this is pure code motion.

use rune_protocol::{CardView, GameView, Permanent, ValidAction};

use super::{Agent, AgentError, PASS_PRIORITY_KIND};

/// Slot filling ([`fill_answers`]), split into its own file by size; re-exported so
/// the crate's public path is unchanged.
mod fill;
pub use fill::fill_answers;

/// A deterministic, network-free **rule-based agent** that plays a full, legal
/// game to completion from a [`GameView`] alone (issue #159). It replaces the
/// pass-only [`PassPriorityAgent`] as the binary's opponent so two `--agent`
/// processes finish a real game.
///
/// Its priority policy, over whichever actions the server offers:
/// - **Mulligan** (option prompt): keep the opening hand rather than mulligan.
/// - **Cleanup discard** (select-from-zone prompt): discard the highest
///   mana-value card, keeping cheap, castable cards.
/// - **Combat**: declare attackers (every creature that can attack, since an
///   unblocked attacker only deals damage) and declare profitable blocks (a
///   blocker that survives or trades up), advancing combat every step.
/// - **Main phase**: play a land, cast the highest mana-value affordable creature,
///   then any other affordable spell, tapping lands for mana first to pay.
/// - Otherwise **pass priority**; never concede unless it is the only action.
///
/// Every choice breaks ties by the server's stable action/candidate order, so two
/// agents on a pinned `rng_seed` reproduce the same game. It never returns an id
/// the view did not offer, and never fills a slot with an un-advertised id
/// ([`fill_answers`]); the loop re-validates the chosen id regardless.
#[derive(Debug, Default, Clone, Copy)]
pub struct RuleBasedAgent;

impl Agent for RuleBasedAgent {
    async fn choose(&self, view: &GameView) -> Result<String, AgentError> {
        choose_action(view)
            .map(|action| action.id.clone())
            .ok_or_else(|| AgentError::Backend("no actions were offered".to_string()))
    }
}

/// The action the [`RuleBasedAgent`] takes for `view`, or `None` when the view
/// offers nothing. Pure and deterministic — a function of the view alone, breaking
/// every tie by the server's stable action order (issue #159).
///
/// The server offers the special choices (mulligan, cleanup discard, combat
/// declarations) in their own windows with no `pass_priority` alongside, so
/// checking them before the main-phase development and the final pass yields a
/// single well-defined move for every view.
#[must_use]
pub fn choose_action(view: &GameView) -> Option<&ValidAction> {
    let actions = &view.valid_actions;
    if actions.is_empty() {
        return None;
    }

    // Pre-game mulligan (CR 103.5): keep the opening hand.
    if let Some(decision) = first_of_kind(actions, "mulligan_decision") {
        return Some(decision);
    }
    // Cleanup discard-to-max (CR 514.1): shed the costliest card.
    if let Some(discard) = first_of_kind(actions, "discard") {
        return Some(discard);
    }
    // Combat (CR 508/509): take the declaration to advance combat and apply
    // pressure. The selection itself is chosen in `fill_answers`.
    if let Some(attack) = first_of_kind(actions, "declare_attackers") {
        return Some(attack);
    }
    if let Some(block) = first_of_kind(actions, "declare_blockers") {
        return Some(block);
    }

    // Main-phase development: land, then the biggest affordable creature, then any
    // other affordable spell.
    if let Some(land) = first_of_kind(actions, "play_land") {
        return Some(land);
    }
    if let Some(creature) =
        highest_mana_value_where(view, actions, "cast_spell", subject_is_creature)
    {
        return Some(creature);
    }
    if let Some(spell) = highest_mana_value_where(view, actions, "cast_spell", |_, _| true) {
        return Some(spell);
    }
    // Build mana toward a spell: chain the offered tap-for-mana abilities.
    if wants_to_cast(view) {
        if let Some(mana) = actions.iter().find(|a| is_mana_source(view, a)) {
            return Some(mana);
        }
    }

    // Nothing to develop: pass priority when offered.
    if let Some(pass) = first_of_kind(actions, PASS_PRIORITY_KIND) {
        return Some(pass);
    }

    // No pass available (a forced choice): take any non-concede action, else concede
    // as the last resort so the game never stalls.
    actions
        .iter()
        .find(|a| a.kind != "concede")
        .or_else(|| actions.first())
}

/// The first offered action of `kind`, or `None`.
fn first_of_kind<'a>(actions: &'a [ValidAction], kind: &str) -> Option<&'a ValidAction> {
    actions.iter().find(|action| action.kind == kind)
}

/// The offered action of `kind` satisfying `pred` with the greatest mana value,
/// resolving ties toward the earliest in the server's stable order (deterministic).
fn highest_mana_value_where<'a>(
    view: &GameView,
    actions: &'a [ValidAction],
    kind: &str,
    pred: impl Fn(&GameView, &ValidAction) -> bool,
) -> Option<&'a ValidAction> {
    actions
        .iter()
        .filter(|action| action.kind == kind && pred(view, action))
        .fold(None, |best: Option<&ValidAction>, action| match best {
            // Keep the earliest action at the maximum value (replace only on a
            // strictly greater one), so the choice is order-stable.
            Some(current)
                if action_mana_value(view, current) >= action_mana_value(view, action) =>
            {
                Some(current)
            }
            _ => Some(action),
        })
}

/// The mana value of the hand card an action's subject names, or `0` if the subject
/// is not a known hand card. Used to rank `cast_spell`/`discard` actions.
fn action_mana_value(view: &GameView, action: &ValidAction) -> u32 {
    action
        .subject
        .iter()
        .filter_map(|id| card_in_hand(view, id))
        .filter_map(|card| card.mana_cost.as_deref())
        .map(mana_value_of)
        .max()
        .unwrap_or(0)
}

/// The converted mana value of a cost string like `"{2}{G}"` (→ 3): each `{N}`
/// generic pip adds `N`, every other symbol (colored, hybrid, …) adds 1. Empty or
/// unparsable segments contribute nothing. A pure lexical count — no color logic.
fn mana_value_of(cost: &str) -> u32 {
    cost.split('}')
        .filter_map(|segment| {
            let symbol = segment.trim_start_matches('{');
            if symbol.is_empty() {
                None
            } else {
                Some(symbol.parse::<u32>().unwrap_or(1))
            }
        })
        .sum()
}

/// The hand card `view` shows with entity id `id`, if the viewer holds it.
fn card_in_hand<'a>(view: &'a GameView, id: &str) -> Option<&'a CardView> {
    view.my_hand.iter().find(|card| card.id == id)
}

/// Whether a card is a creature, by its (server-computed) type line.
fn is_creature_card(card: &CardView) -> bool {
    card.type_line.to_lowercase().contains("creature")
}

/// Whether a card is a land, by its (server-computed) type line.
fn is_land_card(card: &CardView) -> bool {
    card.type_line.to_lowercase().contains("land")
}

/// Whether an action's subject names a creature card in hand (a creature spell).
fn subject_is_creature(view: &GameView, action: &ValidAction) -> bool {
    action
        .subject
        .iter()
        .filter_map(|id| card_in_hand(view, id))
        .any(is_creature_card)
}

/// Whether the hand holds any spell worth building mana for (any non-land card), so
/// the agent taps lands only when a cast is the goal — never pointlessly.
fn wants_to_cast(view: &GameView) -> bool {
    view.my_hand.iter().any(|card| !is_land_card(card))
}

/// The permanent `view` shows with entity id `id`, if any.
fn permanent_in_play<'a>(view: &'a GameView, id: &str) -> Option<&'a Permanent> {
    view.battlefield.iter().find(|perm| perm.id == id)
}

/// Whether an action is a land's mana ability — an `activate_ability` with no target
/// requirements whose source permanent is a land. The agent activates these to pay
/// for a spell; it leaves other activated abilities alone.
fn is_mana_source(view: &GameView, action: &ValidAction) -> bool {
    action.kind == "activate_ability"
        && action.requirements.is_empty()
        && action
            .subject
            .iter()
            .filter_map(|id| permanent_in_play(view, id))
            .any(|perm| is_land_card(&perm.card))
}

#[cfg(test)]
mod tests {

    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use rune_protocol::{
        CardView, Permanent, Phase, Prompt, PromptOption, TargetChoice, TargetRequirement,
        ValidAction,
    };

    fn view_with_actions(actions: Vec<ValidAction>) -> GameView {
        GameView {
            you: "p0".into(),
            my_hand: vec![],
            me: rune_protocol::SelfView::default(),
            opponents: vec![],
            battlefield: vec![],
            stack: vec![],
            graveyards: vec![],
            exile: vec![],
            command: vec![],
            phase: Phase::PrecombatMain,
            turn: 1,
            active_player: "p0".into(),
            mana_pool: vec![],
            priority_player: Some("p0".into()),
            valid_actions: actions,
            action_deadline: None,
            result: None,
            log: vec![],
            stops: Vec::new(),
            auto_passed: false,
            action_rejected: false,
            player_names: std::collections::BTreeMap::new(),
            commander_damage: Vec::new(),
            commander_tax: Vec::new(),
            seat_order: Vec::new(),
        }
    }

    fn pass() -> ValidAction {
        ValidAction {
            id: "a0".into(),
            kind: "pass_priority".into(),
            label: "Pass priority".into(),
            subject: vec![],
            ..Default::default()
        }
    }

    // --- Rule-based agent (issue #159) ------------------------------------

    /// A hand card view with a mana cost and type line.
    fn hand_card(id: &str, type_line: &str, cost: &str) -> CardView {
        CardView {
            id: id.into(),
            name: id.into(),
            type_line: type_line.into(),
            mana_cost: (!cost.is_empty()).then(|| cost.to_string()),
            rules_text: String::new(),
            functional_id: String::new(),
            power: None,
            toughness: None,
            keywords: vec![],
        }
    }

    /// A battlefield creature permanent with the given controller and P/T.
    fn creature_perm(id: &str, controller: &str, power: i64, toughness: i64) -> Permanent {
        Permanent {
            id: id.into(),
            controller: controller.into(),
            owner: controller.into(),
            card: CardView {
                id: id.into(),
                name: id.into(),
                type_line: "Creature".into(),
                mana_cost: None,
                rules_text: String::new(),
                functional_id: String::new(),
                power: Some(power.to_string()),
                toughness: Some(toughness.to_string()),
                keywords: vec![],
            },
            tapped: false,
            attacking: false,
            attacking_player: None,
            blocking: None,
            damage: 0,
            attached_to: None,
            counters: vec![],
        }
    }

    fn action(id: &str, kind: &str, subject: Vec<&str>) -> ValidAction {
        ValidAction {
            id: id.into(),
            kind: kind.into(),
            label: kind.into(),
            subject: subject.into_iter().map(str::to_string).collect(),
            token: format!("tok_{id}"),
            ..Default::default()
        }
    }

    fn chosen_for<'a>(targets: &'a [TargetChoice], slot: &str) -> &'a [String] {
        targets
            .iter()
            .find(|t| t.slot == slot)
            .map_or(&[], |t| t.chosen.as_slice())
    }

    #[test]
    fn mana_value_of_sums_generic_and_symbols() {
        assert_eq!(mana_value_of(""), 0);
        assert_eq!(mana_value_of("{G}"), 1);
        assert_eq!(mana_value_of("{2}{G}"), 3);
        assert_eq!(mana_value_of("{4}{G}"), 5);
        assert_eq!(mana_value_of("{1}"), 1);
    }

    #[test]
    fn policy_keeps_at_the_mulligan_decision() {
        let decision = ValidAction {
            prompts: vec![Prompt::Option {
                slot: "decision".into(),
                prompt: "Keep this hand or take a mulligan?".into(),
                options: vec![
                    PromptOption {
                        id: "keep".into(),
                        label: "Keep this hand".into(),
                    },
                    PromptOption {
                        id: "mulligan".into(),
                        label: "Mulligan".into(),
                    },
                ],
            }],
            ..action("a0", "mulligan_decision", vec![])
        };
        let view = view_with_actions(vec![decision.clone(), action("a1", "concede", vec![])]);

        let picked = choose_action(&view).unwrap();
        assert_eq!(picked.kind, "mulligan_decision");
        let answers = fill_answers(&view, picked).unwrap();
        assert_eq!(chosen_for(&answers, "decision"), ["keep"]);
    }

    #[test]
    fn policy_discards_the_highest_mana_value_card() {
        // Over-full hand: a Forest (0), a 1-drop, and a 5-drop; discard the 5-drop.
        let mut view = view_with_actions(vec![
            ValidAction {
                prompts: vec![Prompt::SelectFromZone {
                    slot: "discard".into(),
                    prompt: "Choose a card to discard".into(),
                    zone: "hand".into(),
                    owner: "p0".into(),
                    count: 1,
                    candidates: vec!["card_forest".into(), "card_1".into(), "card_5".into()],
                }],
                ..action("a0", "discard", vec![])
            },
            action("a1", "concede", vec![]),
        ]);
        view.my_hand = vec![
            hand_card("card_forest", "Land", ""),
            hand_card("card_1", "Creature", "{G}"),
            hand_card("card_5", "Creature", "{4}{G}"),
        ];

        let picked = choose_action(&view).unwrap();
        assert_eq!(picked.kind, "discard");
        let answers = fill_answers(&view, picked).unwrap();
        assert_eq!(chosen_for(&answers, "discard"), ["card_5"]);
    }

    #[test]
    fn policy_plays_a_land_before_casting_or_passing() {
        let mut view = view_with_actions(vec![
            pass(),
            action("a1", "play_land", vec!["card_forest"]),
            action("a2", "cast_spell", vec!["card_g"]),
            action("a3", "concede", vec![]),
        ]);
        view.my_hand = vec![
            hand_card("card_forest", "Land", ""),
            hand_card("card_g", "Creature", "{G}"),
        ];
        assert_eq!(choose_action(&view).unwrap().kind, "play_land");
    }

    #[test]
    fn policy_casts_the_highest_mana_value_creature() {
        let mut view = view_with_actions(vec![
            pass(),
            action("a1", "cast_spell", vec!["card_g"]),
            action("a2", "cast_spell", vec!["card_5"]),
            action("a3", "cast_spell", vec!["card_art"]),
            action("a4", "concede", vec![]),
        ]);
        view.my_hand = vec![
            hand_card("card_g", "Creature", "{G}"),
            hand_card("card_5", "Creature", "{4}{G}"),
            hand_card("card_art", "Artifact", "{1}"),
        ];
        // The 5-drop creature outranks the 1-drop creature and the artifact.
        let picked = choose_action(&view).unwrap();
        assert_eq!(picked.subject, vec!["card_5".to_string()]);
    }

    #[test]
    fn policy_casts_a_noncreature_when_no_creature_is_affordable() {
        let mut view = view_with_actions(vec![
            pass(),
            action("a1", "cast_spell", vec!["card_art"]),
            action("a2", "concede", vec![]),
        ]);
        view.my_hand = vec![hand_card("card_art", "Artifact", "{1}")];
        assert_eq!(choose_action(&view).unwrap().kind, "cast_spell");
    }

    #[test]
    fn policy_taps_a_land_for_mana_when_a_creature_is_uncast() {
        // No cast is offered yet (no mana), but a creature is in hand and a Forest
        // can be tapped: activate the mana ability to build toward the cast.
        let mut view = view_with_actions(vec![
            pass(),
            action("a1", "activate_ability", vec!["perm_forest"]),
            action("a2", "concede", vec![]),
        ]);
        view.my_hand = vec![hand_card("card_g", "Creature", "{G}")];
        view.battlefield = vec![Permanent {
            card: CardView {
                type_line: "Land".into(),
                ..creature_perm("perm_forest", "p0", 0, 0).card
            },
            ..creature_perm("perm_forest", "p0", 0, 0)
        }];
        assert_eq!(choose_action(&view).unwrap().kind, "activate_ability");
    }

    #[test]
    fn policy_does_not_tap_mana_with_an_empty_hand() {
        // A land ability is offered but there is nothing to cast: pass instead of
        // tapping pointlessly.
        let view = view_with_actions(vec![
            pass(),
            action("a1", "activate_ability", vec!["perm_forest"]),
            action("a2", "concede", vec![]),
        ]);
        assert_eq!(choose_action(&view).unwrap().kind, "pass_priority");
    }

    #[test]
    fn policy_passes_when_only_pass_and_concede_are_offered() {
        let view = view_with_actions(vec![pass(), action("a1", "concede", vec![])]);
        assert_eq!(choose_action(&view).unwrap().kind, "pass_priority");
    }

    #[test]
    fn policy_never_concedes_while_another_action_exists() {
        // A forced window offering only a declaration and concede: take the
        // declaration, never concede.
        let view = view_with_actions(vec![
            action("a0", "declare_attackers", vec![]),
            action("a1", "concede", vec![]),
        ]);
        assert_eq!(choose_action(&view).unwrap().kind, "declare_attackers");
    }

    #[test]
    fn agent_never_submits_an_unadvertised_id_over_recorded_views() {
        // Property-style: over a battery of representative views, the agent's chosen
        // id is always offered and every filled id is one the slot advertised.
        let views = property_views();
        for view in &views {
            let Some(action) = choose_action(view) else {
                continue;
            };
            assert!(
                crate::is_offered(view, &action.id),
                "chose an unoffered id in view {view:?}"
            );
            let Some(answers) = fill_answers(view, action) else {
                continue;
            };
            for answer in &answers {
                let advertised = advertised_ids(action, &answer.slot);
                for id in &answer.chosen {
                    assert!(
                        advertised.contains(id),
                        "slot {} chose un-advertised id {id} (advertised {advertised:?})",
                        answer.slot
                    );
                }
            }
        }
    }

    /// Every id a slot advertises (requirement candidates, option ids, zone
    /// candidates, or order items), for the property test above.
    fn advertised_ids(action: &ValidAction, slot: &str) -> Vec<String> {
        if let Some(req) = action.requirements.iter().find(|r| r.slot == slot) {
            return req.candidates.clone();
        }
        for prompt in &action.prompts {
            match prompt {
                Prompt::Option {
                    slot: s, options, ..
                } if s == slot => return options.iter().map(|o| o.id.clone()).collect(),
                Prompt::SelectFromZone {
                    slot: s,
                    candidates,
                    ..
                } if s == slot => return candidates.clone(),
                Prompt::Order { slot: s, items, .. } if s == slot => return items.clone(),
                _ => {}
            }
        }
        Vec::new()
    }

    /// A representative battery of views exercising every shape.
    fn property_views() -> Vec<GameView> {
        let mulligan = ValidAction {
            prompts: vec![Prompt::Option {
                slot: "decision".into(),
                prompt: "Keep or mulligan?".into(),
                options: vec![
                    PromptOption {
                        id: "keep".into(),
                        label: "Keep".into(),
                    },
                    PromptOption {
                        id: "mulligan".into(),
                        label: "Mulligan".into(),
                    },
                ],
            }],
            ..action("a0", "mulligan_decision", vec![])
        };
        let discard = ValidAction {
            prompts: vec![Prompt::SelectFromZone {
                slot: "discard".into(),
                prompt: "Choose a card to discard".into(),
                zone: "hand".into(),
                owner: "p0".into(),
                count: 1,
                candidates: vec!["card_1".into(), "card_2".into()],
            }],
            ..action("a0", "discard", vec![])
        };
        let attackers = ValidAction {
            requirements: vec![TargetRequirement {
                slot: "attackers".into(),
                prompt: "Choose which creatures attack".into(),
                candidates: vec!["perm_a".into(), "perm_b".into()],
            }],
            ..action("a0", "declare_attackers", vec![])
        };
        let blockers = ValidAction {
            requirements: vec![TargetRequirement {
                slot: "block_1".into(),
                prompt: "Choose blockers".into(),
                candidates: vec!["perm_x".into()],
            }],
            ..action("a0", "declare_blockers", vec![])
        };

        let mut discard_view = view_with_actions(vec![discard, action("a1", "concede", vec![])]);
        discard_view.my_hand = vec![
            hand_card("card_1", "Creature", "{G}"),
            hand_card("card_2", "Creature", "{4}{G}"),
        ];

        let mut attack_view = view_with_actions(vec![attackers, action("a1", "concede", vec![])]);
        attack_view.you = "p0".into();
        attack_view.battlefield = vec![
            creature_perm("perm_a", "p0", 2, 2),
            creature_perm("perm_b", "p0", 1, 1),
        ];

        let mut block_view = view_with_actions(vec![blockers, action("a1", "concede", vec![])]);
        block_view.you = "p0".into();
        block_view.battlefield = vec![
            Permanent {
                attacking: true,
                ..creature_perm("perm_1", "p1", 2, 2)
            },
            creature_perm("perm_x", "p0", 2, 3),
        ];

        vec![
            view_with_actions(vec![mulligan, action("a1", "concede", vec![])]),
            discard_view,
            attack_view,
            block_view,
            view_with_actions(vec![pass(), action("a1", "concede", vec![])]),
            view_with_actions(vec![]),
        ]
    }

    #[tokio::test]
    async fn rule_based_agent_is_deterministic_for_a_view() {
        let mut view = view_with_actions(vec![
            pass(),
            action("a1", "cast_spell", vec!["card_5"]),
            action("a2", "cast_spell", vec!["card_g"]),
            action("a3", "concede", vec![]),
        ]);
        view.my_hand = vec![
            hand_card("card_5", "Creature", "{4}{G}"),
            hand_card("card_g", "Creature", "{G}"),
        ];
        let a = RuleBasedAgent.choose(&view).await.unwrap();
        let b = RuleBasedAgent.choose(&view).await.unwrap();
        assert_eq!(a, b);
        // The highest-cost creature is the deterministic pick.
        assert_eq!(a, "a1");
    }
}
