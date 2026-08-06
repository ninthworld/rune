//! Projecting the engine's offered actions into wire `ValidAction`s.

use super::*;

mod announcement;

pub(crate) use announcement::*;

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
            // Neither collapsed action is a cast, so neither states a mana cost.
            cost: None,
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
            cost: None,
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
        Action::AnswerChoice { .. }
        | Action::AnswerConfirm { .. }
        | Action::AnswerColor { .. }
        | Action::AnswerReplacement { .. }
        | Action::AnswerCardName { .. }
        | Action::AnswerOrder { .. }
        | Action::AnswerPermanents { .. }
        | Action::AnswerPermanent { .. } => (
            "player_choice".to_string(),
            player_choice_label(state, db),
            Vec::new(),
            Vec::new(),
        ),
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
        Action::AnswerChoice { .. }
        | Action::AnswerConfirm { .. }
        | Action::AnswerColor { .. }
        | Action::AnswerReplacement { .. }
        | Action::AnswerCardName { .. }
        | Action::AnswerOrder { .. }
        | Action::AnswerPermanents { .. }
        | Action::AnswerPermanent { .. } => player_choice_prompts(state, db),
        // A cast carries one `pay_mana` slot per unit of cost it still owes (CR 601.2f–g)
        // — none at all when the pool already covers it. This is what lets a client offer
        // the card first and the payment second, and take the payment back apart without
        // having sent anything.
        // A cast poses its announcement choices (CR 601.2b) **before** its cost, in the
        // order the rules make them: the mode first, because it decides which target
        // slots exist, then X, because it decides what the spell costs. The pips come
        // last and are posed for the base cost — a value of X above zero adds generic
        // mana the server pays on the player's behalf (ADR 0010), exactly as it pays for
        // any slot a client leaves unanswered.
        Action::CastSpell { card, .. } => {
            let mut prompts = announcement_prompts(state, db, action);
            prompts.extend(cast_payment_prompts(state, db, *card, None));
            prompts
        }
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
        cost: cast_cost_view(state, db, action),
        destinations,
        token,
    }
}

/// What a cast costs, printed and as the game has it (CR 601.2f) — `None` for every
/// action that is not a cast, none of which has a mana cost to state.
///
/// The modified half comes from [`sage_engine::total_cast_cost`], which is the same
/// answer the offer was gated on and the charge will take. Asking the engine rather than
/// composing it here is the whole rule: a server that reduced a cost of its own would be
/// a second implementation of CR 601.2f, and the two would disagree the first time either
/// changed.
fn cast_cost_view(
    state: &GameState,
    db: &CardDatabase,
    action: &Action,
) -> Option<sage_protocol::ActionCost> {
    let Action::CastSpell { card, .. } = action else {
        return None;
    };
    Some(sage_protocol::ActionCost {
        printed: db
            .card(card.card)
            .map(|data| data.mana_cost.clone())
            .unwrap_or_default(),
        modified: total_cast_cost(state, db, *card)?.text(),
    })
}

#[cfg(test)]
mod tests;
