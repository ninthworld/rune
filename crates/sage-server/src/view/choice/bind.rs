//! Mapping an answered prompt back onto the engine [`Action`] that carries it.
//!
//! Split out of [`super`] for size (`docs/coding-standards.md`, File size), and cohesive
//! on its own: every function here is the *reverse* of a prompt the sibling module posed,
//! and they all follow one discipline — **only an option the offer itself listed counts**.
//! An answer naming something the server did not advertise is rejected rather than
//! interpreted, and the engine re-derives the candidate set anyway, so nothing here is the
//! last word (ADR 0004, stale-view protection).

use super::*;

/// Map a returned answer to the CR 614.12 permanent choice onto
/// [`Action::AnswerPermanent`].
///
/// The same reject-stale discipline the other answers follow: only an option id the offer
/// itself listed counts, and the engine is asked whether a permanent choice is owed at
/// all before one is built. The engine independently re-derives the candidate set and
/// rejects a permanent that is no longer in it.
pub(crate) fn bind_player_permanent(
    state: &GameState,
    db: &CardDatabase,
    offered: &ValidAction,
    targets: &[TargetChoice],
) -> Option<Action> {
    let options = offered.prompts.iter().find_map(|prompt| match prompt {
        Prompt::Option { slot, options, .. } if slot == CHOICE_SLOT => Some(options),
        _ => None,
    })?;
    let pending = pending_player_choice(state)?;
    let request = pending.question.permanent()?;
    let answer = chosen_for(targets, CHOICE_SLOT).first()?;
    if !options.iter().any(|option| &option.id == answer) {
        return None;
    }
    if answer == DECLINE_COPY_OPTION {
        return Some(Action::AnswerPermanent { chosen: None });
    }
    let chosen = copy_choice_candidates(state, request.of, pending.chooser, db)
        .into_iter()
        .find(|id| permanent_entity_id(*id) == *answer)?;
    Some(Action::AnswerPermanent {
        chosen: Some(chosen),
    })
}

/// Map a returned answer to the CR 616.1 ordering choice onto
/// [`Action::AnswerReplacement`].
///
/// The same reject-stale discipline the other three answers follow: only an option id
/// the offer itself listed counts, and the engine is asked whether an ordering choice is
/// owed at all before one is built. The id *is* the index, so the parse is the binding;
/// the engine independently re-derives the option list and rejects an index past its end.
pub(crate) fn bind_player_replacement(
    state: &GameState,
    offered: &ValidAction,
    targets: &[TargetChoice],
) -> Option<Action> {
    let options = offered.prompts.iter().find_map(|prompt| match prompt {
        Prompt::Option { slot, options, .. } if slot == CHOICE_SLOT => Some(options),
        _ => None,
    })?;
    pending_player_choice(state)?.question.replacement()?;
    let answer = chosen_for(targets, CHOICE_SLOT).first()?;
    if !options.iter().any(|option| &option.id == answer) {
        return None;
    }
    Some(Action::AnswerReplacement {
        index: answer.parse().ok()?,
    })
}

/// Map a returned answer to the `player_choice` action onto the concrete
/// [`Action::AnswerChoice`] (issue #604): the single `choice`
/// [`Prompt::SelectFromZone`] slot names cards from its freshly recomputed candidates,
/// **in the order the client sent them** — which is the order a scry puts them on the
/// bottom in, so re-sorting here would silently answer a different question.
///
/// An unanswered slot is a legal *empty* selection whenever the prompt's advertised
/// minimum is zero (declining to scry, failing to find); the engine re-checks the whole
/// selection against the choice's own bounds anyway
/// ([`answer_is_legal`](sage_engine::apply_action)), so nothing here is the last word.
/// `None` only when the answer names an id the offer did not.
pub(crate) fn bind_player_choice(
    state: &GameState,
    db: &CardDatabase,
    offered: &ValidAction,
    targets: &[TargetChoice],
) -> Option<Action> {
    let candidates = offered.prompts.iter().find_map(|prompt| match prompt {
        Prompt::SelectFromZone {
            slot, candidates, ..
        } if slot == CHOICE_SLOT => Some(candidates),
        _ => None,
    })?;
    let request = pending_player_choice(state)?.question.cards()?;
    let available = choice_candidates(state, request, db);
    let mut chosen = Vec::new();
    for id in chosen_for(targets, CHOICE_SLOT) {
        if !candidates.contains(id) {
            return None;
        }
        let inst = available
            .iter()
            .find(|inst| card_entity_id(inst.id) == *id)?;
        chosen.push(inst.id);
    }
    Some(Action::AnswerChoice { chosen })
}

/// Map a returned answer to a color choice onto [`Action::AnswerColor`].
///
/// The same reject-stale discipline the other two answers follow: only an option id the
/// offer itself listed counts, and the engine is asked whether a color choice is owed at
/// all before one is built. `None` when no color choice is owed or the answer names
/// something the offer did not.
pub(crate) fn bind_player_color(
    state: &GameState,
    offered: &ValidAction,
    targets: &[TargetChoice],
) -> Option<Action> {
    let options = offered.prompts.iter().find_map(|prompt| match prompt {
        Prompt::Option { slot, options, .. } if slot == CHOICE_SLOT => Some(options),
        _ => None,
    })?;
    pending_player_choice(state)?.question.color()?;
    let answer = chosen_for(targets, CHOICE_SLOT).first()?;
    if !options.iter().any(|option| &option.id == answer) {
        return None;
    }
    let (_, _, color) = COLOR_OPTIONS.iter().find(|(id, _, _)| id == answer)?;
    Some(Action::AnswerColor { color: *color })
}

/// Map a returned answer to an **amount** onto [`Action::AnswerNumber`].
///
/// The same reject-stale discipline as its siblings, in the shape a number takes: the
/// answer must be one of the values the offer itself listed, so a value that was reachable
/// when the view was built but is not now — a pool that shrank, or a forged number — is
/// refused rather than clamped. The engine's gate re-checks the bounds regardless; this is
/// what stops a stale answer becoming a *different* legal answer.
pub(crate) fn bind_player_number(
    state: &GameState,
    offered: &ValidAction,
    targets: &[TargetChoice],
) -> Option<Action> {
    let values = offered.prompts.iter().find_map(|prompt| match prompt {
        Prompt::Number { slot, values, .. } if slot == CHOICE_SLOT => Some(values),
        _ => None,
    })?;
    if !matches!(
        pending_player_choice(state)?.question,
        sage_engine::ChoiceQuestion::Number(_)
    ) {
        return None;
    }
    let answer: u32 = chosen_for(targets, CHOICE_SLOT).first()?.parse().ok()?;
    if !values.iter().any(|value| value.value == answer) {
        return None;
    }
    Some(Action::AnswerNumber { value: answer })
}

/// Map a returned answer to the yes-or-no of an optional effect onto
/// [`Action::AnswerConfirm`] (issue #610).
///
/// The slot's answer is one option id, and only an id the offer itself listed counts:
/// an answer naming an accept the server did not offer — because the cost was not
/// payable when the view was built — is rejected rather than quietly read as a decline,
/// the same reject-stale discipline the card selection follows. `None` when no yes-or-no
/// is owed or the answer names nothing the offer did.
pub(crate) fn bind_player_confirm(
    state: &GameState,
    offered: &ValidAction,
    targets: &[TargetChoice],
) -> Option<Action> {
    let options = offered.prompts.iter().find_map(|prompt| match prompt {
        Prompt::Option { slot, options, .. } if slot == CHOICE_SLOT => Some(options),
        _ => None,
    })?;
    pending_player_choice(state)?.question.confirm()?;
    let answer = chosen_for(targets, CHOICE_SLOT).first()?;
    if !options.iter().any(|option| &option.id == answer) {
        return None;
    }
    Some(Action::AnswerConfirm {
        accept: answer == ACCEPT_OPTION,
    })
}

/// Map a returned answer to a card-naming choice onto [`Action::AnswerCardName`].
///
/// The same reject-stale discipline every other answer follows, with one extra step: the
/// option id is an authored `functional_id`, so it is resolved back to a
/// [`CardId`](sage_engine::CardId) through the database rather than parsed as a handle.
/// A handle is a per-build integer and must never travel on the wire (ADR 0008 §3); an
/// identity is stable, and one the catalog does not know resolves to nothing, which is a
/// rejection rather than a guess. `None` when no card-naming choice is owed or the answer
/// names something the offer did not.
pub(crate) fn bind_player_card_name(
    state: &GameState,
    db: &CardDatabase,
    offered: &ValidAction,
    targets: &[TargetChoice],
) -> Option<Action> {
    let options = offered.prompts.iter().find_map(|prompt| match prompt {
        Prompt::Option { slot, options, .. } if slot == CHOICE_SLOT => Some(options),
        _ => None,
    })?;
    pending_player_choice(state)?.question.card_name()?;
    let answer = chosen_for(targets, CHOICE_SLOT).first()?;
    if !options.iter().any(|option| &option.id == answer) {
        return None;
    }
    let functional_id = FunctionalId::try_from((*answer).clone()).ok()?;
    Some(Action::AnswerCardName {
        card: db.card_id(&functional_id)?,
    })
}

/// Map a returned answer to a sacrifice onto [`Action::AnswerPermanents`].
///
/// The permanent counterpart of [`bind_player_choice`], and the same reject-stale
/// discipline: only ids the offer itself listed count, the candidate list is re-derived
/// from the engine rather than trusted from the prompt, and the engine re-checks the
/// whole selection against its own bounds afterwards.
pub(crate) fn bind_player_permanents(
    state: &GameState,
    db: &CardDatabase,
    offered: &ValidAction,
    targets: &[TargetChoice],
) -> Option<Action> {
    let candidates = offered.prompts.iter().find_map(|prompt| match prompt {
        Prompt::SelectFromZone {
            slot, candidates, ..
        } if slot == CHOICE_SLOT => Some(candidates),
        _ => None,
    })?;
    let request = pending_player_choice(state)?.question.permanents()?;
    let available = permanent_choice_candidates(state, request, db);
    let mut chosen = Vec::new();
    for id in chosen_for(targets, CHOICE_SLOT) {
        if !candidates.contains(id) {
            return None;
        }
        let permanent = available
            .iter()
            .copied()
            .find(|perm| permanent_entity_id(*perm) == *id)?;
        chosen.push(permanent);
    }
    Some(Action::AnswerPermanents { chosen })
}

/// Map a returned answer to the card ordering onto [`Action::AnswerOrder`].
///
/// The same reject-stale discipline the other answers follow: only ids the offer itself
/// listed count, and the engine is asked whether an ordering is owed at all before one is
/// built. **The submitted order is carried through untouched** — it is the whole answer,
/// so re-sorting here would silently bottom the cards somewhere else. The engine
/// independently re-derives the remainder and rejects anything that is not a permutation
/// of it, so nothing here is the last word.
pub(crate) fn bind_player_order(
    state: &GameState,
    offered: &ValidAction,
    targets: &[TargetChoice],
) -> Option<Action> {
    let items = offered.prompts.iter().find_map(|prompt| match prompt {
        Prompt::Order { slot, items, .. } if slot == CHOICE_SLOT => Some(items),
        _ => None,
    })?;
    let request = pending_player_choice(state)?.question.order()?;
    let available = order_candidates(state, request);
    let mut order = Vec::new();
    for id in chosen_for(targets, CHOICE_SLOT) {
        if !items.contains(id) {
            return None;
        }
        let inst = available
            .iter()
            .find(|inst| card_entity_id(inst.id) == *id)?;
        order.push(inst.id);
    }
    Some(Action::AnswerOrder { order })
}
