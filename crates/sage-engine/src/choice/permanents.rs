//! The mid-resolution question whose answer is **permanents** rather than cards: the
//! sacrifice of [`Effect::Sacrifice`](crate::Effect).
//!
//! Every other selection in [`super`] picks cards out of a pile — a hand, a library —
//! where only printed characteristics exist and a [`CardInstance`](crate::id::CardInstance)
//! is the whole identity of what was picked. This one picks objects that are *on the
//! battlefield*, and the difference is not cosmetic: a token has no card behind it
//! (CR 111), so it could never appear in a card-shaped candidate list, and a player who
//! could not sacrifice their tokens would be playing a different game.
//!
//! Everything around the question is the shared machinery — one queue, one chooser, one
//! [`Resume`](super::Resume), and the rule that a question with no legal answer is
//! applied rather than posed. Only the answer's shape lives here.

use super::*;
use crate::id::PermanentId;

/// A permanent-selection question ([`ChoiceQuestion::Permanents`]): whose permanents,
/// which of them, how many, and what becomes of the ones picked.
///
/// Deliberately free of any snapshotted candidate list, exactly as [`ChoiceRequest`] is:
/// it names a class, and [`permanent_choice_candidates`] evaluates that against current
/// state every time it is asked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PermanentRequest {
    /// The player whose permanents are on offer.
    ///
    /// Always equal to [`PendingChoice::chooser`], and there is no card in the game for
    /// which it is not: CR 701.17b lets a player sacrifice only permanents they control,
    /// so a coercive form — one seat picking from another's board — is a thing the rules
    /// do not have. It is still a field rather than an assumption because the *class* is
    /// read relative to it, and reading a class relative to "whoever is answering" would
    /// be a second way to say the same thing.
    pub subject: PlayerId,
    /// Restrict the offer to permanents with this **printed** card type — the `creatures`
    /// of "sacrifices half the creatures they control". Absent offers every permanent
    /// they control.
    pub card_type: Option<CardType>,
    /// The fewest permanents a legal answer may name, before clamping to what is
    /// actually there ([`permanent_choice_bounds`]).
    pub min: u32,
    /// The most a legal answer may name, before that same clamping.
    pub max: u32,
    /// What happens to the ones picked.
    pub outcome: PermanentOutcome,
}

/// What becomes of the permanents a [`PermanentRequest`] picked.
///
/// One member today. It is an enum rather than an implied sacrifice for
/// [`ChoiceOutcome`]'s reason: the aftermath belongs to the *request*, and a future
/// "choose a creature and tap it" would be the same question with a different ending
/// rather than a second queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermanentOutcome {
    /// They are **sacrificed** (CR 701.17): each goes to its owner's graveyard as a real
    /// death, so a creature among them fires the dies triggers and a token among them
    /// ceases to exist (CR 111.7).
    Sacrifice,
}

/// The permanents `request` currently offers, in battlefield order.
///
/// Recomputed from live state on every call, for the reason [`choice_candidates`] is:
/// an answer is validated against the set that exists *now*, never against one
/// snapshotted when the question was posed.
///
/// Control is read through the one CR 613 layer-2 path, so a creature the subject has
/// gained control of is theirs to sacrifice and one they have lost is not. The type is
/// read off the printed face, consistent with every other type test in the engine.
#[must_use]
pub fn permanent_choice_candidates(
    state: &GameState,
    request: &PermanentRequest,
    db: &CardDatabase,
) -> Vec<PermanentId> {
    state
        .battlefield
        .iter()
        .filter(|perm| crate::characteristics::controller_of(state, perm) == request.subject)
        .filter(|perm| {
            request.card_type.is_none_or(|card_type| {
                perm.printed
                    .face(db)
                    .is_some_and(|face| face.has_type(card_type))
            })
        })
        .map(|perm| perm.id)
        .collect()
}

/// How many permanents a legal answer to `request` must name, clamped to what is
/// actually there: `(min, max)`, inclusive.
///
/// The permanent counterpart of [`choice_bounds`], and it carries the same guarantee: a
/// player told to sacrifice two creatures who controls one sacrifices the one, and a
/// clamped maximum of zero is the single uniform signal that the question must not be
/// posed at all.
#[must_use]
pub fn permanent_choice_bounds(
    state: &GameState,
    request: &PermanentRequest,
    db: &CardDatabase,
) -> (u32, u32) {
    let available =
        u32::try_from(permanent_choice_candidates(state, request, db).len()).unwrap_or(u32::MAX);
    (request.min.min(available), request.max.min(available))
}

/// Whether `chosen` is a legal answer to the permanent choice currently owed: the right
/// number of distinct permanents, every one of them in the freshly recomputed candidate
/// set.
///
/// The same regenerate-and-check discipline [`answer_is_legal`] applies to cards, so a
/// stale or forged [`PermanentId`] can never survive.
#[must_use]
pub(crate) fn answer_permanents_is_legal(
    state: &GameState,
    chosen: &[PermanentId],
    db: &CardDatabase,
) -> bool {
    let Some(request) = pending_player_choice(state).and_then(|p| p.question.permanents()) else {
        return false;
    };
    let (min, max) = permanent_choice_bounds(state, request, db);
    let count = u32::try_from(chosen.len()).unwrap_or(u32::MAX);
    if count < min || count > max {
        return false;
    }
    let candidates = permanent_choice_candidates(state, request, db);
    chosen
        .iter()
        .enumerate()
        .all(|(i, id)| !chosen[..i].contains(id) && candidates.contains(id))
}

/// Carry out `request` for the permanents just picked.
///
/// The caller has already established that `chosen` is a legal answer; this moves
/// permanents and decides nothing.
///
/// A sacrifice goes through the **death** seam rather than a bare zone move, which is
/// what makes it a real death (CR 701.17b, CR 700.4): a creature among the chosen fires
/// the dies triggers the diff collector picks up, and a token among them leaves a
/// recorded `PermanentDied` behind — the only trace a token's death ever has, since
/// CR 111.7 leaves no card in any zone.
pub(crate) fn apply_permanent_choice(
    state: &mut GameState,
    request: &PermanentRequest,
    chosen: &[PermanentId],
    db: &CardDatabase,
) {
    match request.outcome {
        PermanentOutcome::Sacrifice => {
            for &id in chosen {
                state.destroy_permanent(id, db);
            }
        }
    }
}
