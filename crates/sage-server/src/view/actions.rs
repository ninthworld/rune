//! Projecting the engine's offered actions into wire `ValidAction`s.

use super::*;

/// How a returned answer for a projected wire action is bound back onto a concrete
/// engine [`Action`]. Most wire actions are a 1:1 [`Bind::Standard`] projection of a
/// single engine action; two are *collapsed* projections that fold a combinatorial
/// engine enumeration into one richer-prompt action (issue #156):
/// [`Bind::MulliganDecision`] replaces the separate `Mulligan`/`Keep` actions with a
/// single [`Prompt::Option`], and [`Bind::DiscardFromHand`] replaces the per-card
/// cleanup `Discard` actions with a single [`Prompt::SelectFromZone`].
pub(crate) enum Bind {
    /// A 1:1 projection of this engine action; resolution threads any target
    /// `requirements` back through the per-kind `bind_*` helpers.
    Standard(Action),
    /// The collapsed mulligan keep/take-another decision: an [`Prompt::Option`] plus,
    /// when a bottoming is owed, the [`bottom_requirement`] slot (CR 103.5).
    MulliganDecision,
    /// The collapsed cleanup discard: a single [`Prompt::SelectFromZone`] over the
    /// active player's hand, resolving to one [`Action::Discard`] (CR 514.1).
    DiscardFromHand,
}

/// One projected wire action together with how to bind a returned answer to it.
pub(crate) struct Projected {
    /// The wire action the client sees and answers.
    pub(crate) view: ValidAction,
    /// How [`resolve_action`] maps the answer back onto an engine [`Action`].
    pub(crate) bind: Bind,
}

/// The wire actions the engine currently offers the priority holder, each paired
/// with how a returned answer binds back to the engine.
///
/// The ids are positional (`a0`, `a1`, …), but they are no longer what *binds* a
/// returned answer to an action: each projected [`ValidAction`] also carries a
/// content-binding [`token`](ValidAction::token) hashed from the action's own
/// content (kind + subject + requirements + prompts). [`resolve_action`] verifies
/// that token, so a stale positional id whose action has since changed cannot
/// silently rebind. Empty when no one holds priority.
///
/// Two engine enumerations are *collapsed* into one richer-prompt action apiece
/// (issue #156), deleting the enumeration: the pre-game `Mulligan`/`Keep` pair
/// becomes a single `mulligan_decision` (an [`Prompt::Option`]), and the per-card
/// cleanup `Discard` list becomes a single `discard` (a [`Prompt::SelectFromZone`]).
/// Every other engine action projects 1:1 via [`valid_action_view`].
pub(crate) fn projected_actions(state: &GameState, db: &CardDatabase) -> Vec<Projected> {
    let mut out: Vec<Projected> = Vec::new();
    let mut next = 0usize;
    let mut mulligan_done = false;
    let mut discard_done = false;
    for action in valid_actions(state, db) {
        let projected = match &action {
            // Collapse the keep/take-another pair into one option-bearing action.
            Action::Mulligan | Action::Keep { .. } => {
                if mulligan_done {
                    continue;
                }
                mulligan_done = true;
                build_mulligan_decision(state, next_id(&mut next))
            }
            // Collapse the per-card discard list into one select-from-zone action.
            Action::Discard { .. } => {
                if discard_done {
                    continue;
                }
                discard_done = true;
                build_discard(state, next_id(&mut next))
            }
            _ => Projected {
                view: valid_action_view(next_id(&mut next), &action, state, db),
                bind: Bind::Standard(action),
            },
        };
        out.push(projected);
    }
    out
}

/// Take the next positional wire id (`a0`, `a1`, …), advancing the counter. Only
/// called when an action is actually emitted, so ids stay dense across collapses.
fn next_id(next: &mut usize) -> String {
    let id = format!("a{next}");
    *next += 1;
    id
}

/// The collapsed mulligan keep/take-another decision (CR 103.5, London), a real
/// [`Prompt::Option`] projection (issue #156). The two engine actions
/// [`Action::Mulligan`]/[`Action::Keep`] are folded into one `mulligan_decision`
/// action carrying an option slot (`decision`) whose two choices are *keep* and
/// *mulligan*. When a bottoming is owed (the seat has mulliganed), the same action
/// also carries the [`keep_prompts`] bottoming slot, so a keep answer selects which
/// cards to bottom; [`resolve_action`] binds *keep* to [`Action::Keep`] with those
/// cards and *mulligan* to [`Action::Mulligan`].
///
/// Only *keep* owes that bottoming — taking another hand bottoms nothing — so the
/// keep choice names the slot in its [`PromptOption::requires`] (issue #451). That
/// is what lets a client offer both choices honestly: keep only once the exact
/// owed count is picked, mulligan at any time.
fn build_mulligan_decision(state: &GameState, id: String) -> Projected {
    let kind = "mulligan_decision".to_string();
    let subject: Vec<String> = Vec::new();
    let requirements: Vec<TargetRequirement> = Vec::new();
    let bottoming = keep_prompts(state, &Action::Keep { bottom: Vec::new() });
    let keep_requires: Vec<String> = bottoming
        .iter()
        .map(|_| BOTTOM_SLOT.to_string())
        .collect::<Vec<_>>();
    let mut prompts = vec![Prompt::Option {
        slot: "decision".to_string(),
        prompt: "Keep this hand or take a mulligan?".to_string(),
        options: vec![
            PromptOption {
                id: "keep".to_string(),
                label: "Keep this hand".to_string(),
                requires: keep_requires,
            },
            PromptOption {
                id: "mulligan".to_string(),
                label: "Mulligan".to_string(),
                requires: Vec::new(),
            },
        ],
    }];
    prompts.extend(bottoming);
    let token = content_token(&kind, &subject, &requirements, &prompts);
    Projected {
        view: ValidAction {
            id,
            kind,
            mana_ability: false,
            label: "Keep or mulligan".to_string(),
            subject,
            requirements,
            prompts,
            // A pre-game decision is answered in a prompt, not by dragging anything
            // anywhere, so it names no destination (issue #554) — and a client
            // therefore offers no drop target for it.
            destinations: Vec::new(),
            token,
        },
        bind: Bind::MulliganDecision,
    }
}

/// The collapsed cleanup discard-to-maximum choice (CR 514.1), a real
/// [`Prompt::SelectFromZone`] projection (issue #156). The engine offers one
/// [`Action::Discard`] per card in the over-full hand; this folds them into a single
/// `discard` action carrying one select-from-zone slot over the active player's hand
/// (`count: 1` — the engine discards one card per turn-based check, re-offering while
/// still over the limit). [`resolve_action`] binds the chosen id to that
/// [`Action::Discard`].
fn build_discard(state: &GameState, id: String) -> Projected {
    let seat = state.priority;
    let candidates: Vec<String> = state
        .players
        .get(seat.0)
        .map(|player| {
            player
                .hand
                .iter()
                .map(|inst| card_entity_id(inst.id))
                .collect()
        })
        .unwrap_or_default();
    let kind = "discard".to_string();
    let subject: Vec<String> = Vec::new();
    let requirements: Vec<TargetRequirement> = Vec::new();
    let prompts = vec![Prompt::SelectFromZone {
        slot: "discard".to_string(),
        prompt: "Choose a card to discard".to_string(),
        zone: "hand".to_string(),
        owner: player_id(seat),
        count: 1,
        // Exactly one per turn-based check, re-offered while still over the limit.
        min: None,
        candidates,
    }];
    let token = content_token(&kind, &subject, &requirements, &prompts);
    Projected {
        view: ValidAction {
            id,
            kind,
            mana_ability: false,
            label: "Discard a card".to_string(),
            subject,
            requirements,
            prompts,
            // The card to discard is picked in the prompt; the destination that would
            // describe it (the graveyard) is not where the *gesture* goes, so this
            // action names none (issue #554).
            destinations: Vec::new(),
            token,
        },
        bind: Bind::DiscardFromHand,
    }
}

/// Project one engine [`Action`] onto its wire [`ValidAction`], attaching the
/// subject entity so the client can render the action on the card/permanent it
/// belongs to, the ordered target `requirements` it must fill, and the
/// content-binding `token` (see [`content_token`]) the client echoes back.
///
/// Every subject/candidate names a *specific* game object by its per-instance id
/// ([`card_entity_id`]/[`permanent_entity_id`]/[`player_id`], issue #51), never a
/// bare printed card, so a targeted answer is unambiguous.
///
/// Multi-select and targeted actions carry their engine candidate sets in
/// `requirements`, projected from the freshly computed legal sets (issue #140):
/// the mulligan [`Action::Keep`] bottoming ([`bottom_requirement`]), the combat
/// [`Action::DeclareAttackers`]/[`Action::DeclareBlockers`] declarations
/// ([`attacker_candidates`]/[`blocker_candidates`]), and ability targets
/// ([`target_requirements`], ADR 0004 deferral #73). The token binds those
/// requirements automatically (see [`content_token`]), and [`resolve_action`] maps
/// a returned selection back onto the concrete engine action. An action with
/// nothing to choose projects empty `requirements` and stays a plain action.
fn valid_action_view(
    id: String,
    action: &Action,
    state: &GameState,
    db: &CardDatabase,
) -> ValidAction {
    let (kind, label, subject, requirements): (
        String,
        String,
        Vec<String>,
        Vec<TargetRequirement>,
    ) = match action {
        Action::PassPriority => (
            "pass_priority".to_string(),
            pass_priority_label(state),
            Vec::new(),
            Vec::new(),
        ),
        Action::PlayLand { card } => (
            "play_land".to_string(),
            format!("Play {}", card_name(card.card, db)),
            vec![card_entity_id(card.id)],
            Vec::new(),
        ),
        // A cast's target requirements (CR 601.2c) come from the same per-slot
        // enumeration abilities use ([`target_requirements`]); an untargeted spell
        // projects none. Wiring the returned selection back into a targeted cast is
        // a later server slice (ADR 0004 §Client / #73) — the engine already
        // records and re-checks the targets.
        Action::CastSpell { card, .. } => (
            "cast_spell".to_string(),
            format!("Cast {}", card_name(card.card, db)),
            vec![card_entity_id(card.id)],
            ability_requirements(state, db, action),
        ),
        // A triggered ability the game put on the stack, waiting for its controller
        // to aim it (CR 603.3d). Labeled with the ability's own sentence so the
        // player is choosing for something they can read, and subject-bound to its
        // source permanent so the board highlights what is asking.
        Action::ChooseTriggerTargets { ability, .. } => (
            "choose_targets".to_string(),
            trigger_label(state, db, *ability),
            trigger_subject(state, *ability),
            ability_requirements(state, db, action),
        ),
        Action::Discard { card } => (
            "discard".to_string(),
            format!("Discard {}", card_name(card.card, db)),
            vec![card_entity_id(card.id)],
            Vec::new(),
        ),
        // The mid-resolution player choice the game is waiting on (issue #604). The
        // pick itself rides as a `select_from_zone` prompt (built below) rather than a
        // target requirement: it names cards in a hidden zone, which is a different
        // thing from an object on the battlefield, and it carries a count.
        // Every shape of mid-resolution choice rides the same action kind: from the
        // client's side they are one thing — the question the game is waiting on — and
        // the prompt below says which shape it is. A color choice reuses the same
        // `option` prompt the yes-or-no does, so no client learns a new wire shape.
        Action::AnswerChoice { .. } | Action::AnswerConfirm { .. } | Action::AnswerColor { .. } => {
            (
                "player_choice".to_string(),
                player_choice_label(state, db),
                Vec::new(),
                Vec::new(),
            )
        }
        // Labeled with the ability's own rules sentence ("{T}: Add {G}.", ADR 0008
        // text generation), so a permanent offering several activations renders
        // *distinguishable* dock buttons — a generic "Activate ability" collapses
        // them into identical choices the player cannot tell apart.
        Action::ActivateAbility {
            permanent, index, ..
        } => (
            "activate_ability".to_string(),
            ability_label(state, db, *permanent, *index),
            vec![permanent_entity_id(*permanent)],
            ability_requirements(state, db, action),
        ),
        // The same category over a card in a graveyard (CR 113.6): from the client's
        // side it is the one thing it already knows how to render — an ability offered
        // on the object that has it — so it keeps the `activate_ability` category and
        // differs only in that its subject is a *card* entity rather than a permanent.
        // That entity is the one the graveyard pile already projects, so the offer lands
        // on the card the player is looking at, with no new wire shape and no rules
        // knowledge on the client.
        Action::ActivateAbilityFromGraveyard { card, index, .. } => (
            "activate_ability".to_string(),
            graveyard_ability_label(db, *card, *index),
            vec![card_entity_id(card.id)],
            ability_requirements(state, db, action),
        ),
        // Pre-game London mulligan decisions (CR 103.5). Subject-less, so the
        // client renders them in the action bar. A `Mulligan` has no
        // sub-choice; a `Keep` carries the bottoming select-from-zone slot
        // (candidates = the deciding seat's hand card entity ids, count = mulligans
        // taken) when one is owed, and nothing for a first-hand keep.
        Action::Mulligan => (
            "mulligan".to_string(),
            "Mulligan".to_string(),
            Vec::new(),
            Vec::new(),
        ),
        Action::Keep { .. } => (
            "keep".to_string(),
            "Keep hand".to_string(),
            Vec::new(),
            Vec::new(),
        ),
        // Combat declarations (CR 508/509) are subject-less choices offered to the
        // priority holder, carrying their multi-select candidate `requirements` from
        // the engine's freshly computed legal sets: attacker candidates for the
        // active player, and one blocker slot per declared attacker for the
        // defender. Empty when there is nothing to declare, so the empty (token-less)
        // form still round-trips as a "no attackers/blockers" declaration.
        Action::DeclareAttackers { .. } => (
            "declare_attackers".to_string(),
            "Declare attackers".to_string(),
            Vec::new(),
            attacker_requirements(state, db),
        ),
        Action::DeclareBlockers { .. } => (
            "declare_blockers".to_string(),
            "Declare blockers".to_string(),
            Vec::new(),
            blocker_requirements(state, db),
        ),
        // Combat-damage assignment order (CR 510.1, issue #346): the choice rides as
        // one `order` prompt per multi-blocked attacker (built below), not a target
        // requirement.
        Action::OrderCombatDamage { .. } => (
            "order_combat_damage".to_string(),
            "Order combat damage".to_string(),
            Vec::new(),
            Vec::new(),
        ),
        // Commander return decisions (CR 903.9a): the owner may move a commander
        // that went to a graveyard or exile into their command zone, or decline.
        // Subject is the commander card so the client can render it on that card.
        Action::ReturnCommanderToCommandZone { card } => (
            "return_commander_to_command_zone".to_string(),
            format!("Move {} to the command zone", card_name(card.card, db)),
            vec![card_entity_id(card.id)],
            Vec::new(),
        ),
        Action::DeclineCommanderReturn { card } => (
            "decline_commander_return".to_string(),
            format!("Leave {} where it is", card_name(card.card, db)),
            vec![card_entity_id(card.id)],
            Vec::new(),
        ),
        // Concede (CR 104.3a): a subject-less action always offered to the acting
        // seat, rendered in the action bar.
        Action::Concede => (
            "concede".to_string(),
            "Concede".to_string(),
            Vec::new(),
            Vec::new(),
        ),
    };
    // Most 1:1 engine-action projections carry no `prompts`; the combat-damage
    // ordering action (issue #346) carries one `order` prompt per multi-blocked
    // attacker, each a permutation over that attacker's blockers, a mulligan `Keep`
    // carries its owed bottoming as a select-from-zone slot (CR 103.5), and a cast
    // carries its unpaid cost one pip at a time.
    let prompts: Vec<Prompt> = match action {
        Action::OrderCombatDamage { .. } => damage_order_prompts(state, db),
        Action::Keep { .. } => keep_prompts(state, action),
        Action::AnswerChoice { .. } | Action::AnswerConfirm { .. } | Action::AnswerColor { .. } => {
            player_choice_prompts(state, db)
        }
        // A cast carries one `pay_mana` slot per unit of cost it still owes (CR 601.2f–g)
        // — none at all when the pool already covers it. This is what lets a client offer
        // the card first and the payment second, and take the payment back apart without
        // having sent anything.
        Action::CastSpell { card, .. } => cast_payment_prompts(state, db, *card),
        // An activation carries the parts of its cost the player *picks* the payment for
        // (CR 601.2b) — a sacrifice or a discard — on the same select-from-zone slots a
        // cast poses them on. No pips: an activation's mana comes from the pool, floated by
        // activating mana abilities as actions in their own right.
        Action::ActivateAbility {
            permanent, index, ..
        } => activation_payment_prompts(state, db, *permanent, *index),
        _ => Vec::new(),
    };
    // One-gesture mana: mark the activation of a mana ability
    // (CR 605.1a) so a client may offer a lighter gesture for exactly these
    // actions. Computed by the engine's classifier — clients never inspect
    // abilities themselves.
    let mana_ability = match action {
        Action::ActivateAbility {
            permanent, index, ..
        } => state
            .battlefield
            .iter()
            .find(|perm| perm.id == *permanent)
            .and_then(|perm| {
                abilities_of_permanent(state, db, perm)
                    .get(*index)
                    .map(is_mana_ability)
            })
            .unwrap_or(false),
        _ => false,
    };
    let destinations = action_destinations(state, action, mana_ability);
    let token = content_token(&kind, &subject, &requirements, &prompts);
    ValidAction {
        id,
        kind,
        label,
        subject,
        mana_ability,
        requirements,
        prompts,
        destinations,
        token,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::test_support::{fixture, id_in};
    use crate::view::test_support::{answer, cleanup_over_hand_limit, put_permanent};

    /// Two copies of the same printed card in one hand must project to distinct
    /// entity ids and independently routable actions (issue #51). Before
    /// per-instance identity both copies shared `card_5`, so a returned action
    /// resolved against "the first matching copy".
    #[test]
    fn issue_51_duplicate_hand_cards_get_distinct_entities_and_actions() {
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let forest_a = state.new_instance(fixture("forest"));
        let forest_b = state.new_instance(fixture("forest"));
        state.players[0].hand = vec![forest_a, forest_b];

        let view = personalized_view(&state, &db, PlayerId(0));

        // Each physical copy gets its own hand entity id.
        let hand_ids: Vec<&str> = view.my_hand.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(hand_ids.len(), 2);
        assert_ne!(hand_ids[0], hand_ids[1]);
        assert!(hand_ids.contains(&card_entity_id(forest_a.id).as_str()));
        assert!(hand_ids.contains(&card_entity_id(forest_b.id).as_str()));

        // Two land actions, each carrying its own copy's entity id as subject.
        let land_actions: Vec<&ValidAction> = view
            .valid_actions
            .iter()
            .filter(|a| a.kind == "play_land")
            .collect();
        assert_eq!(land_actions.len(), 2);
        let subjects: Vec<&str> = land_actions.iter().map(|a| a.subject[0].as_str()).collect();
        assert_ne!(subjects[0], subjects[1]);

        // Each action id routes back to a PlayLand naming the exact instance its
        // subject entity referenced — no ambiguity, no "first matching copy".
        for action in &land_actions {
            let resolved = resolve_action(&state, &db, PlayerId(0), &answer(action)).unwrap();
            let Action::PlayLand { card } = resolved else {
                panic!("play_land action must resolve to a PlayLand");
            };
            assert_eq!(action.subject[0], card_entity_id(card.id));
        }

        // The two actions route to two different instances between them.
        let routed: Vec<CardInstance> = land_actions
            .iter()
            .map(
                |a| match resolve_action(&state, &db, PlayerId(0), &answer(a)).unwrap() {
                    Action::PlayLand { card } => card,
                    other => panic!("expected PlayLand, got {other:?}"),
                },
            )
            .collect();
        assert_ne!(routed[0].id, routed[1].id);
        assert!(routed.contains(&forest_a));
        assert!(routed.contains(&forest_b));
    }

    /// During the pre-game London mulligan (CR 103.5) the view projects the deciding
    /// seat's keep/take-another choice as a single `mulligan_decision` action
    /// carrying an [`Prompt::Option`] (issue #156, the real `option` projection),
    /// token-bound, while hand redaction is unaffected: the viewer sees its own hand
    /// in full and only the *size* of the opponent's.
    #[test]
    fn mulligan_decision_projects_an_option_prompt_and_redaction_holds() {
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        // Enter the mulligan phase with seat 0 deciding; give both seats hands.
        state.players[0].hand = vec![state.new_instance(fixture("forest"))];
        state.players[1].hand = vec![
            state.new_instance(fixture("walking_corpse")),
            state.new_instance(fixture("onakke_ogre")),
        ];
        state.mulligan = Some(sage_engine::MulliganState::new(2, 7));

        let view = personalized_view(&state, &db, PlayerId(0));

        // Redaction is unaffected: the viewer sees its own hand in full, and the
        // opponent is reduced to a hand *size* with no card contents leaked.
        assert_eq!(view.my_hand.len(), 1);
        assert_eq!(view.opponents.len(), 1);
        assert_eq!(view.opponents[0].hand_size, 2);

        // The two engine actions collapse into ONE token-bound `mulligan_decision`
        // (plus the always-available concede) — the keep/mulligan enumeration is gone.
        assert!(
            view.valid_actions.iter().all(|a| !a.token.is_empty()),
            "every action carries a content-binding token (ADR 0004)",
        );
        assert!(view.valid_actions.iter().all(|a| a.kind != "keep"));
        assert!(view.valid_actions.iter().all(|a| a.kind != "mulligan"));
        let decision = view
            .valid_actions
            .iter()
            .find(|a| a.kind == "mulligan_decision")
            .expect("the deciding seat is offered a single mulligan decision");

        // It carries exactly one `option` prompt whose choices are keep + mulligan.
        assert_eq!(decision.prompts.len(), 1);
        let Prompt::Option { slot, options, .. } = &decision.prompts[0] else {
            panic!("the mulligan decision is an option prompt");
        };
        assert_eq!(slot, "decision");
        assert_eq!(
            options.iter().map(|o| o.id.as_str()).collect::<Vec<_>>(),
            vec!["keep", "mulligan"],
        );
        // A first-hand keep owes no bottoming, so there is no select-from-zone slot.
        assert!(decision.requirements.is_empty());

        // Both options resolve back to the concrete engine actions.
        let keep = ChooseAction {
            submission: String::new(),
            action_id: decision.id.clone(),
            token: decision.token.clone(),
            targets: vec![TargetChoice {
                slot: "decision".to_string(),
                chosen: vec!["keep".to_string()],
            }],
        };
        assert_eq!(
            resolve_action(&state, &db, PlayerId(0), &keep),
            Some(Action::Keep { bottom: Vec::new() }),
        );
        let mull = ChooseAction {
            submission: String::new(),
            action_id: decision.id.clone(),
            token: decision.token.clone(),
            targets: vec![TargetChoice {
                slot: "decision".to_string(),
                chosen: vec!["mulligan".to_string()],
            }],
        };
        assert_eq!(
            resolve_action(&state, &db, PlayerId(0), &mull),
            Some(Action::Mulligan),
        );
        // An unknown option id is rejected.
        let bogus = ChooseAction {
            submission: String::new(),
            action_id: decision.id.clone(),
            token: decision.token.clone(),
            targets: vec![TargetChoice {
                slot: "decision".to_string(),
                chosen: vec!["scoop".to_string()],
            }],
        };
        assert!(resolve_action(&state, &db, PlayerId(0), &bogus).is_none());

        // The non-deciding seat is offered nothing (actions are redacted to the
        // priority holder) and still only sees the opponent's hand size.
        let other = personalized_view(&state, &db, PlayerId(1));
        assert!(other.valid_actions.is_empty());
        assert_eq!(other.opponents[0].hand_size, 1);
    }

    /// A mulligan decision taken after a mulligan carries, alongside its `option`
    /// slot, the London bottoming (CR 103.5) as a `select_from_zone` prompt over the
    /// hand's cards (issue #156, the `select_from_zone` projection reusing #140's
    /// bottoming), and a keep answer naming the owed cards resolves to a `Keep`
    /// bottoming exactly those cards.
    #[test]
    fn mulligan_decision_keep_projects_bottoming_as_select_from_zone_and_resolves() {
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        let c0 = state.new_instance(fixture("forest"));
        let c1 = state.new_instance(fixture("walking_corpse"));
        state.players[0].hand = vec![c0, c1];
        state.players[1].hand = vec![state.new_instance(fixture("onakke_ogre"))];
        // Seat 0 has taken one mulligan, so a keep now owes one bottomed card.
        let mut mull = sage_engine::MulliganState::new(2, 7);
        mull.decisions[0].taken = 1;
        state.mulligan = Some(mull);

        let view = personalized_view(&state, &db, PlayerId(0));
        let decision = view
            .valid_actions
            .iter()
            .find(|a| a.kind == "mulligan_decision")
            .expect("the deciding seat is offered a mulligan decision");

        // The bottoming rides a `select_from_zone` prompt over the hand carrying the
        // exact owed count (issue #451 — it used to ride a countless `requirements`
        // slot, leaving a client to guess "how many" from the prompt text).
        assert!(decision.requirements.is_empty(), "no target requirement");
        let Prompt::SelectFromZone {
            slot,
            zone,
            owner,
            count,
            candidates,
            ..
        } = &decision.prompts[1]
        else {
            panic!("the bottoming is a select_from_zone prompt");
        };
        assert_eq!(slot, "bottom");
        assert_eq!(zone, "hand");
        assert_eq!(owner, &player_id(PlayerId(0)));
        assert_eq!(*count, 1, "one mulligan taken owes one bottomed card");
        assert_eq!(
            candidates,
            &vec![card_entity_id(c0.id), card_entity_id(c1.id)],
        );

        // Only *keep* owes that slot, so the option says so and *mulligan* does not
        // (issue #451): this is what lets a client gate keep on the exact count while
        // leaving take-another available at zero.
        let Prompt::Option { options, .. } = &decision.prompts[0] else {
            panic!("the decision is an option prompt");
        };
        assert_eq!(options[0].id, "keep");
        assert_eq!(options[0].requires, vec!["bottom".to_string()]);
        assert_eq!(options[1].id, "mulligan");
        assert!(options[1].requires.is_empty());

        // A keep naming one card to bottom resolves to a Keep bottoming exactly it.
        let choose = ChooseAction {
            submission: String::new(),
            action_id: decision.id.clone(),
            token: decision.token.clone(),
            targets: vec![
                TargetChoice {
                    slot: "decision".to_string(),
                    chosen: vec!["keep".to_string()],
                },
                TargetChoice {
                    slot: "bottom".to_string(),
                    chosen: vec![card_entity_id(c0.id)],
                },
            ],
        };
        let resolved =
            resolve_action(&state, &db, PlayerId(0), &choose).expect("the selection resolves");
        assert_eq!(
            resolved,
            Action::Keep {
                bottom: vec![Target::Card(c0.id)],
            },
        );

        // A keep that omits the owed bottoming is rejected (the mandatory slot is
        // unfilled), so a stale/empty answer cannot bottom nothing when one is owed.
        let empty_keep = ChooseAction {
            submission: String::new(),
            action_id: decision.id.clone(),
            token: decision.token.clone(),
            targets: vec![TargetChoice {
                slot: "decision".to_string(),
                chosen: vec!["keep".to_string()],
            }],
        };
        assert!(resolve_action(&state, &db, PlayerId(0), &empty_keep).is_none());

        // A keep naming *more* cards than the owed count is rejected just as firmly
        // (issue #451): the wire now advertises the count, so a client can keep this
        // answer from ever being sent, and the server still refuses it if one does.
        let over_keep = ChooseAction {
            submission: String::new(),
            action_id: decision.id.clone(),
            token: decision.token.clone(),
            targets: vec![
                TargetChoice {
                    slot: "decision".to_string(),
                    chosen: vec!["keep".to_string()],
                },
                TargetChoice {
                    slot: "bottom".to_string(),
                    chosen: vec![card_entity_id(c0.id), card_entity_id(c1.id)],
                },
            ],
        };
        assert!(resolve_action(&state, &db, PlayerId(0), &over_keep).is_none());
    }

    /// A first-hand mulligan decision owes no bottoming, so it carries the option
    /// slot alone and its *keep* choice requires nothing (issue #451): keeping the
    /// opening seven must never be gated behind a selection that was never asked for.
    #[test]
    fn first_hand_mulligan_decision_owes_no_bottoming_and_keep_requires_nothing() {
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        let c0 = state.new_instance(fixture("forest"));
        state.players[0].hand = vec![c0];
        state.mulligan = Some(sage_engine::MulliganState::new(2, 7));

        let view = personalized_view(&state, &db, PlayerId(0));
        let decision = view
            .valid_actions
            .iter()
            .find(|a| a.kind == "mulligan_decision")
            .expect("the deciding seat is offered a mulligan decision");

        assert_eq!(decision.prompts.len(), 1, "the option slot alone");
        let Prompt::Option { options, .. } = &decision.prompts[0] else {
            panic!("the decision is an option prompt");
        };
        assert!(options.iter().all(|option| option.requires.is_empty()));

        // And that keep resolves with no bottoming at all.
        let choose = ChooseAction {
            submission: String::new(),
            action_id: decision.id.clone(),
            token: decision.token.clone(),
            targets: vec![TargetChoice {
                slot: "decision".to_string(),
                chosen: vec!["keep".to_string()],
            }],
        };
        assert_eq!(
            resolve_action(&state, &db, PlayerId(0), &choose),
            Some(Action::Keep { bottom: vec![] }),
        );
    }

    /// The cleanup discard-to-maximum (CR 514.1) collapses the engine's per-card
    /// `Discard` list into ONE `discard` action carrying a single `select_from_zone`
    /// prompt over the hand (issue #156, the flagship `select_from_zone` projection);
    /// a chosen card resolves to that concrete [`Action::Discard`].
    #[test]
    fn cleanup_discard_projects_select_from_zone_and_a_selection_resolves() {
        let db = CardDatabase::bundled().unwrap();
        let (state, hand) = cleanup_over_hand_limit();

        let view = personalized_view(&state, &db, state.active_player);

        // Exactly one `discard` action (the N per-card actions are gone), token-bound.
        let discards: Vec<&ValidAction> = view
            .valid_actions
            .iter()
            .filter(|a| a.kind == "discard")
            .collect();
        assert_eq!(discards.len(), 1, "one collapsed discard, not one per card");
        let discard = discards[0];
        assert!(!discard.token.is_empty());

        // It carries one select-from-zone slot over the hand (count 1, all cards).
        assert_eq!(discard.prompts.len(), 1);
        let Prompt::SelectFromZone {
            slot,
            zone,
            owner,
            count,
            candidates,
            ..
        } = &discard.prompts[0]
        else {
            panic!("the discard is a select-from-zone prompt");
        };
        assert_eq!(slot, "discard");
        assert_eq!(zone, "hand");
        assert_eq!(owner, &player_id(state.active_player));
        assert_eq!(*count, 1);
        assert_eq!(
            *candidates,
            hand.iter()
                .map(|c| card_entity_id(c.id))
                .collect::<Vec<_>>(),
        );

        // Choosing one card resolves to a Discard of exactly that instance.
        let choose = ChooseAction {
            submission: String::new(),
            action_id: discard.id.clone(),
            token: discard.token.clone(),
            targets: vec![TargetChoice {
                slot: "discard".to_string(),
                chosen: vec![card_entity_id(hand[3].id)],
            }],
        };
        assert_eq!(
            resolve_action(&state, &db, state.active_player, &choose),
            Some(Action::Discard { card: hand[3] }),
        );

        // A card not among the candidates (never in hand) is rejected.
        let foreign = ChooseAction {
            submission: String::new(),
            action_id: discard.id.clone(),
            token: discard.token.clone(),
            targets: vec![TargetChoice {
                slot: "discard".to_string(),
                chosen: vec!["card_99999".to_string()],
            }],
        };
        assert!(resolve_action(&state, &db, state.active_player, &foreign).is_none());
    }

    #[test]
    fn cr_605_mana_ability_activation_carries_the_wire_flag() {
        // The projection flags exactly the mana-ability activation
        // (CR 605.1a — all effects add mana, no stack, no targets) so a client
        // can offer the one-gesture tap-for-mana; the targeted tap ability of
        // the same permanent stays unflagged, as does every other action kind.
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
        let flags: Vec<bool> = view
            .valid_actions
            .iter()
            .filter(|a| a.kind == "activate_ability")
            .map(|a| a.mana_ability)
            .collect();
        assert_eq!(flags, vec![true, false], "only the mana ability is flagged");
        assert!(
            view.valid_actions
                .iter()
                .filter(|a| a.kind != "activate_ability")
                .all(|a| !a.mana_ability),
            "no other action kind carries the flag",
        );
    }

    #[test]
    fn issue_723_a_graveyard_activation_is_offered_on_the_card_it_belongs_to() {
        // CR 113.6: an ability that functions from a graveyard is projected as the same
        // `activate_ability` category every other activation uses — a client has no new
        // wire shape to learn — with the *card* entity as its subject, which is the id
        // the graveyard pile already carries. It is labelled with the ability's own
        // generated sentence, so a player reads what they are clicking.
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        state.priority = PlayerId(0);
        let skeleton = state.new_instance(fixture("reassembling_skeleton"));
        state.players[0].graveyard.push(skeleton);
        state.players[0].mana_pool.add(sage_engine::Color::Black, 2);

        let view = personalized_view(&state, &db, PlayerId(0));
        let offered: Vec<&ValidAction> = view
            .valid_actions
            .iter()
            .filter(|a| a.kind == "activate_ability")
            .collect();
        assert_eq!(offered.len(), 1, "one activation, from the graveyard");
        assert_eq!(offered[0].subject, vec![card_entity_id(skeleton.id)]);
        assert_eq!(
            offered[0].label,
            "{1}{B}: Return Reassembling Skeleton from your graveyard to the battlefield tapped."
        );
        assert!(!offered[0].mana_ability, "it uses the stack");

        // The graveyard pile projects that same entity, so the offer lands on the card
        // the player is looking at.
        assert!(view
            .graveyards
            .iter()
            .flat_map(|pile| pile.cards.iter())
            .any(|card| card.id == card_entity_id(skeleton.id)));

        // And the action id routes back to the engine action naming that exact copy.
        let resolved = resolve_action(&state, &db, PlayerId(0), &answer(offered[0]));
        assert_eq!(
            resolved,
            Some(Action::ActivateAbilityFromGraveyard {
                card: skeleton,
                index: 0,
                targets: Vec::new(),
            })
        );
    }

    // ----- Game-log projection (issue #259) -----
}
