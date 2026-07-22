//! Slot filling for the rule-based agent (issue #159): [`fill_answers`] answers a
//! chosen action's target `requirements` and [`Prompt`] slots — attacker/blocker
//! declarations, mulligan and discard prompts, ability targets — with a legal
//! selection, never an id the server did not advertise. Split from the sibling
//! [`choose_action`](super::choose_action) by size (docs/coding-standards.md, File
//! size); it shares the small view helpers in the parent module. Pure code motion.

use rune_protocol::{GameView, Permanent, Prompt, TargetChoice, TargetRequirement, ValidAction};

use super::{card_in_hand, mana_value_of, permanent_in_play};

/// Fill every choice slot of `action` — its target `requirements` and its
/// [`Prompt`] slots — with a legal selection, one [`TargetChoice`] per slot, per the
/// [`RuleBasedAgent`] policy. `None` only when a **mandatory** slot cannot be filled
/// (an option with no options, or an ability-target slot with no candidates), which
/// the loop turns into a safe pass. A plain action (no slots) yields an empty
/// selection, so plain actions are unchanged.
///
/// Every chosen id is drawn from that slot's advertised candidates/options/items, so
/// the agent never submits an id the server did not enumerate (issue #159 property).
#[must_use]
pub fn fill_answers(view: &GameView, action: &ValidAction) -> Option<Vec<TargetChoice>> {
    let mut out: Vec<TargetChoice> = Vec::with_capacity(action.requirements.len());

    // Combat blocking assigns each blocker to at most one attacker, so the choice for
    // a `block_*` slot depends on which blockers earlier slots already used.
    let mut used_blockers: Vec<String> = Vec::new();

    for req in &action.requirements {
        let chosen: Vec<String> = if req.slot == "attackers" {
            attacker_selection(view, req)
        } else if req.slot.starts_with("defend_") {
            defender_selection(req)
        } else if req.slot.starts_with("block_") {
            block_selection(view, req, &mut used_blockers)
        } else if req.slot == "bottom" {
            // Mulligan bottoming (we never mulligan, so rarely hit): bottom the
            // required count of lowest mana-value cards.
            let count = leading_count(&req.prompt).unwrap_or(req.candidates.len());
            lowest_mana_value_ids(view, &req.candidates, count)
        } else {
            // An ability-target slot (`t0`, `t1`, …): a single mandatory target.
            vec![target_preference(view, req)?]
        };
        out.push(TargetChoice {
            slot: req.slot.clone(),
            chosen,
        });
    }

    for prompt in &action.prompts {
        let choice = match prompt {
            Prompt::Option { slot, options, .. } => {
                let picked = options
                    .iter()
                    .find(|option| {
                        option.id.eq_ignore_ascii_case("keep")
                            || option.label.to_lowercase().contains("keep")
                    })
                    .or_else(|| options.first())?;
                TargetChoice {
                    slot: slot.clone(),
                    chosen: vec![picked.id.clone()],
                }
            }
            Prompt::SelectFromZone {
                slot,
                prompt,
                count,
                candidates,
                ..
            } => {
                let n = *count as usize;
                // Discarding sheds the costliest cards; any other select-from-zone
                // (e.g. bottoming) keeps the cheap cards by shedding the cheapest.
                let ids = if prompt.to_lowercase().contains("discard") {
                    highest_mana_value_ids(view, candidates, n)
                } else {
                    lowest_mana_value_ids(view, candidates, n)
                };
                TargetChoice {
                    slot: slot.clone(),
                    chosen: ids,
                }
            }
            Prompt::Order { slot, items, .. } => TargetChoice {
                slot: slot.clone(),
                // "As given": echo the items in their advertised order.
                chosen: items.clone(),
            },
        };
        out.push(choice);
    }

    Some(out)
}

/// The attackers to declare: every candidate that can deal damage (power ≥ 1). An
/// unblocked attacker only ever deals damage, so attacking with the whole board is
/// the simplest sound aggressive rule; 0-power creatures are left back.
fn attacker_selection(view: &GameView, req: &TargetRequirement) -> Vec<String> {
    req.candidates
        .iter()
        .filter(|id| permanent_in_play(view, id).is_none_or(|perm| power_of(perm) >= 1))
        .cloned()
        .collect()
}

/// The defending player an attacker attacks, for a multiplayer `defend_<id>` slot
/// (issue #341/#345): the **first** advertised defender candidate. The server offers
/// these slots only when the active player has more than one opponent (a two-player
/// game has a sole defender and no slot); the engine lists the candidates in stable
/// seat order, so "first candidate" is a deterministic, always-legal choice — the
/// same simple, sound policy the agent uses elsewhere. `defend_*` slots for creatures
/// the agent does not attack with are ignored by the server, so answering every one is
/// harmless. Empty only if the slot somehow carries no candidate (then the attacker is
/// simply left undeclared).
fn defender_selection(req: &TargetRequirement) -> Vec<String> {
    req.candidates.first().cloned().into_iter().collect()
}

/// The blockers to assign to one attacker's `block_*` slot: at most one *profitable*
/// blocker (one that survives the block or trades up), chosen from the candidates not
/// already assigned to an earlier attacker. `GameView` carries no self-life, so the
/// agent cannot detect lethal to justify a chump; it therefore blocks only when the
/// block is favorable and never throws a creature away for nothing.
fn block_selection(
    view: &GameView,
    req: &TargetRequirement,
    used_blockers: &mut Vec<String>,
) -> Vec<String> {
    // The attacker this slot defends against (`block_<permId>`), for the P/T compare.
    let attacker = req
        .slot
        .strip_prefix("block_")
        .map(|suffix| format!("perm_{suffix}"))
        .and_then(|id| permanent_in_play(view, &id).cloned());
    let (attacker_power, attacker_toughness) = attacker
        .as_ref()
        .map_or((0, 0), |perm| (power_of(perm), toughness_of(perm)));

    let pick = req
        .candidates
        .iter()
        .filter(|id| !used_blockers.contains(*id))
        .find_map(|id| {
            let perm = permanent_in_play(view, id)?;
            // Evasion (CR 509.1b): a flyer can be blocked only by flying or reach. The
            // engine offers every untapped creature as a candidate and enforces
            // evasion only when the declaration is submitted (combat.rs), so the agent
            // must not pick an illegal blocker — resubmitting one would stall combat.
            if !can_legally_block(attacker.as_ref(), perm) {
                return None;
            }
            let survives = toughness_of(perm) > attacker_power;
            let kills = power_of(perm) >= attacker_toughness && attacker_toughness > 0;
            (survives || kills).then(|| id.clone())
        });

    match pick {
        Some(id) => {
            used_blockers.push(id.clone());
            vec![id]
        }
        None => Vec::new(),
    }
}

/// Whether `blocker` may legally be declared to block `attacker` given evasion
/// (CR 509.1b / 702.9c / 702.17b): a flying attacker can be blocked only by a
/// creature with flying or reach; any creature can block a non-flying attacker. The
/// engine does not pre-filter block candidates by evasion (it enforces it only when
/// the declaration is submitted), so the agent applies the same rule to avoid
/// picking — and endlessly resubmitting — an illegal block.
fn can_legally_block(attacker: Option<&Permanent>, blocker: &Permanent) -> bool {
    match attacker {
        Some(atk) if has_keyword(atk, "flying") => {
            has_keyword(blocker, "flying") || has_keyword(blocker, "reach")
        }
        _ => true,
    }
}

/// Whether a permanent has printed keyword `keyword` (matched against the view's
/// wire keyword names, e.g. `"flying"`, `"reach"`).
fn has_keyword(perm: &Permanent, keyword: &str) -> bool {
    perm.card.keywords.iter().any(|k| k == keyword)
}

/// A permanent's power as a number, or `0` when absent/non-numeric (e.g. `"*"`).
fn power_of(perm: &Permanent) -> i64 {
    perm.card
        .power
        .as_deref()
        .and_then(|p| p.parse::<i64>().ok())
        .unwrap_or(0)
}

/// A permanent's toughness as a number, or `0` when absent/non-numeric.
fn toughness_of(perm: &Permanent) -> i64 {
    perm.card
        .toughness
        .as_deref()
        .and_then(|t| t.parse::<i64>().ok())
        .unwrap_or(0)
}

/// The preferred single target for an ability slot: an opponent (player or their
/// permanent) when the slot targets damage-style, otherwise the first advertised
/// candidate. `None` when the slot has no candidates (a mandatory slot the agent then
/// cannot answer).
fn target_preference(view: &GameView, req: &TargetRequirement) -> Option<String> {
    if req.candidates.is_empty() {
        return None;
    }
    // Prefer aiming at an opponent (their player id or a permanent they control),
    // which is the right default for the damage/tap abilities the engine models.
    let opponent_target = req
        .candidates
        .iter()
        .find(|id| is_opponent_target(view, id));
    opponent_target.or_else(|| req.candidates.first()).cloned()
}

/// Whether an entity id names an opponent — an opposing player, or a permanent an
/// opponent controls.
fn is_opponent_target(view: &GameView, id: &str) -> bool {
    view.opponents.iter().any(|opp| opp.player_id == id)
        || permanent_in_play(view, id).is_some_and(|perm| perm.controller != view.you)
}

/// The `count` candidate ids with the greatest hand mana value (ties broken toward
/// the advertised order), for shedding the costliest cards on a discard.
fn highest_mana_value_ids(view: &GameView, candidates: &[String], count: usize) -> Vec<String> {
    sorted_by_mana_value(view, candidates, true)
        .into_iter()
        .take(count)
        .collect()
}

/// The `count` candidate ids with the least hand mana value (ties broken toward the
/// advertised order), for bottoming/keeping the cheap cards.
fn lowest_mana_value_ids(view: &GameView, candidates: &[String], count: usize) -> Vec<String> {
    sorted_by_mana_value(view, candidates, false)
        .into_iter()
        .take(count)
        .collect()
}

/// `candidates` ordered by the mana value of the hand card each names — descending
/// when `descending`, ascending otherwise. A **stable** sort, so equal-value ids keep
/// their advertised order and the result is deterministic.
fn sorted_by_mana_value(view: &GameView, candidates: &[String], descending: bool) -> Vec<String> {
    let mut ids: Vec<String> = candidates.to_vec();
    ids.sort_by_key(|id| {
        let value = card_in_hand(view, id)
            .and_then(|card| card.mana_cost.as_deref())
            .map_or(0, mana_value_of);
        if descending {
            // Negate for a descending order under an ascending stable sort.
            -(i64::from(value))
        } else {
            i64::from(value)
        }
    });
    ids
}

/// The leading integer of a prompt like `"Put 2 card(s) on the bottom …"` (→ `2`),
/// or `None` if the prompt opens with no number. Lets the agent honor a bottoming
/// count the wire requirement does not carry as a field.
fn leading_count(prompt: &str) -> Option<usize> {
    prompt
        .split_whitespace()
        .find_map(|word| word.parse::<usize>().ok())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::choose_action;
    use rune_protocol::{
        CardView, OpponentView, Permanent, Phase, PromptOption, TargetRequirement, ValidAction,
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

    fn chosen_for<'a>(targets: &'a [TargetChoice], slot: &str) -> &'a [String] {
        targets
            .iter()
            .find(|t| t.slot == slot)
            .map_or(&[], |t| t.chosen.as_slice())
    }

    #[test]
    fn attackers_selection_declares_every_damaging_creature() {
        let mut view = view_with_actions(vec![ValidAction {
            requirements: vec![TargetRequirement {
                slot: "attackers".into(),
                prompt: "Choose which creatures attack".into(),
                candidates: vec!["perm_a".into(), "perm_b".into(), "perm_wall".into()],
            }],
            ..action("a0", "declare_attackers", vec![])
        }]);
        view.you = "p0".into();
        view.battlefield = vec![
            creature_perm("perm_a", "p0", 2, 2),
            creature_perm("perm_b", "p0", 3, 3),
            creature_perm("perm_wall", "p0", 0, 4), // 0 power: held back
        ];
        let picked = choose_action(&view).unwrap();
        let answers = fill_answers(&view, picked).unwrap();
        let attackers = chosen_for(&answers, "attackers");
        assert!(attackers.contains(&"perm_a".to_string()));
        assert!(attackers.contains(&"perm_b".to_string()));
        assert!(!attackers.contains(&"perm_wall".to_string()));
    }

    #[test]
    fn blockers_selection_blocks_only_profitably_and_never_double_assigns() {
        // Two attackers; our two blockers: one survives/trades, one only chumps.
        let attacker_big = "perm_atk_big"; // 3/3
        let attacker_small = "perm_atk_small"; // 2/2
        let mut view = view_with_actions(vec![ValidAction {
            requirements: vec![
                TargetRequirement {
                    slot: format!("block_{}", "atk_big"),
                    prompt: "Choose blockers for Big".into(),
                    candidates: vec!["perm_blk_good".into(), "perm_blk_weak".into()],
                },
                TargetRequirement {
                    slot: format!("block_{}", "atk_small"),
                    prompt: "Choose blockers for Small".into(),
                    candidates: vec!["perm_blk_good".into(), "perm_blk_weak".into()],
                },
            ],
            ..action("a0", "declare_blockers", vec![])
        }]);
        view.you = "p0".into();
        view.battlefield = vec![
            Permanent {
                attacking: true,
                ..creature_perm(&format!("perm_{}", "atk_big"), "p1", 3, 3)
            },
            Permanent {
                attacking: true,
                ..creature_perm(&format!("perm_{}", "atk_small"), "p1", 2, 2)
            },
            creature_perm("perm_blk_good", "p0", 2, 4), // survives either attacker
            creature_perm("perm_blk_weak", "p0", 1, 1), // chump only — never used
        ];
        let _ = (attacker_big, attacker_small);

        let picked = choose_action(&view).unwrap();
        let answers = fill_answers(&view, picked).unwrap();
        let big = chosen_for(&answers, "block_atk_big");
        let small = chosen_for(&answers, "block_atk_small");
        // The good blocker is assigned to exactly one attacker; the weak one is
        // never thrown away, and no blocker is double-assigned.
        let all: Vec<&String> = big.iter().chain(small.iter()).collect();
        assert_eq!(all.iter().filter(|id| **id == "perm_blk_good").count(), 1);
        assert!(!all.iter().any(|id| **id == "perm_blk_weak"));
    }

    #[test]
    fn blockers_selection_respects_flying_evasion() {
        // CR 509.1b: a flying attacker can be blocked only by flying or reach. The
        // engine offers every untapped creature as a block candidate and enforces
        // evasion only on submission, so the agent must skip a ground blocker of a
        // flyer — declaring it would be illegal and re-offered forever.
        let with_keyword = |id: &str, controller: &str, p: i64, t: i64, kw: &str| Permanent {
            card: CardView {
                keywords: vec![kw.to_string()],
                ..creature_perm(id, controller, p, t).card
            },
            ..creature_perm(id, controller, p, t)
        };
        let flyer = Permanent {
            attacking: true,
            ..with_keyword("perm_atk", "p1", 4, 4, "flying")
        };
        // A 6/6 ground creature would survive the flyer (profitable by P/T) but cannot
        // legally block it.
        let ground = creature_perm("perm_ground", "p0", 6, 6);

        let block_action = |candidates: Vec<&str>| ValidAction {
            requirements: vec![TargetRequirement {
                slot: "block_atk".into(),
                prompt: "Choose blockers".into(),
                candidates: candidates.into_iter().map(str::to_string).collect(),
            }],
            ..action("a0", "declare_blockers", vec![])
        };

        // Only the ground blocker is a candidate: no legal block, so declare none.
        let mut view = view_with_actions(vec![block_action(vec!["perm_ground"])]);
        view.you = "p0".into();
        view.battlefield = vec![flyer.clone(), ground.clone()];
        let picked = choose_action(&view).unwrap();
        let answers = fill_answers(&view, picked).unwrap();
        assert!(
            chosen_for(&answers, "block_atk").is_empty(),
            "a ground creature cannot block a flyer"
        );

        // Add a profitable reacher: it is the legal, chosen blocker.
        let reacher = with_keyword("perm_reach", "p0", 5, 5, "reach"); // survives the 4/4 flyer
        let mut view2 = view_with_actions(vec![block_action(vec!["perm_ground", "perm_reach"])]);
        view2.you = "p0".into();
        view2.battlefield = vec![flyer, ground, reacher];
        let picked2 = choose_action(&view2).unwrap();
        let answers2 = fill_answers(&view2, picked2).unwrap();
        assert_eq!(
            chosen_for(&answers2, "block_atk"),
            ["perm_reach"],
            "a reach creature can block the flyer"
        );
    }

    #[test]
    fn fill_answers_option_prefers_keep_then_first() {
        let keep_first = Prompt::Option {
            slot: "s".into(),
            prompt: "?".into(),
            options: vec![
                PromptOption {
                    id: "mulligan".into(),
                    label: "Mulligan".into(),
                },
                PromptOption {
                    id: "keep".into(),
                    label: "Keep".into(),
                },
            ],
        };
        let view = view_with_actions(vec![]);
        let act = ValidAction {
            prompts: vec![keep_first],
            ..action("a0", "mulligan_decision", vec![])
        };
        let answers = fill_answers(&view, &act).unwrap();
        assert_eq!(chosen_for(&answers, "s"), ["keep"]);

        // With no "keep", the first option is the default.
        let act2 = ValidAction {
            prompts: vec![Prompt::Option {
                slot: "s".into(),
                prompt: "?".into(),
                options: vec![
                    PromptOption {
                        id: "a".into(),
                        label: "A".into(),
                    },
                    PromptOption {
                        id: "b".into(),
                        label: "B".into(),
                    },
                ],
            }],
            ..action("a0", "x", vec![])
        };
        assert_eq!(chosen_for(&fill_answers(&view, &act2).unwrap(), "s"), ["a"]);
    }

    #[test]
    fn fill_answers_order_keeps_items_as_given() {
        let view = view_with_actions(vec![]);
        let act = ValidAction {
            prompts: vec![Prompt::Order {
                slot: "ord".into(),
                prompt: "Order".into(),
                items: vec!["s1".into(), "s2".into(), "s3".into()],
            }],
            ..action("a0", "order", vec![])
        };
        let answers = fill_answers(&view, &act).unwrap();
        assert_eq!(chosen_for(&answers, "ord"), ["s1", "s2", "s3"]);
    }

    #[test]
    fn fill_answers_ability_target_prefers_an_opponent() {
        let mut view = view_with_actions(vec![]);
        view.you = "p0".into();
        view.opponents = vec![OpponentView {
            player_id: "p1".into(),
            hand_size: 0,
            life: 20,
            library_size: 0,
            graveyard_size: 0,
            statuses: vec![],
            eliminated: false,
        }];
        let act = ValidAction {
            requirements: vec![TargetRequirement {
                slot: "t0".into(),
                prompt: "Choose target player".into(),
                candidates: vec!["p0".into(), "p1".into()],
            }],
            ..action("a0", "activate_ability", vec!["perm_x"])
        };
        let answers = fill_answers(&view, &act).unwrap();
        assert_eq!(chosen_for(&answers, "t0"), ["p1"]);
    }

    #[test]
    fn fill_answers_returns_none_when_a_mandatory_target_has_no_candidate() {
        let view = view_with_actions(vec![]);
        let act = ValidAction {
            requirements: vec![TargetRequirement {
                slot: "t0".into(),
                prompt: "Choose target creature".into(),
                candidates: vec![],
            }],
            ..action("a0", "activate_ability", vec!["perm_x"])
        };
        assert!(fill_answers(&view, &act).is_none());
    }
}
