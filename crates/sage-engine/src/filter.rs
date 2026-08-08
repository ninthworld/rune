//! The one predicate that decides whether a permanent is in a class (issue #824).
//!
//! Four selectors used to answer this question and each had its own matcher, so "creatures
//! you control" could mean one set when a spell pumped them, another when a static ability
//! did, and a third when something counted them. There is one body now, and every caller
//! reaches it: a mass effect, a sweeper, a static ability's own selector, and a count.
//!
//! What still differs between callers is [`Reading`], and that difference is a rules fact
//! rather than a leftover — see its documentation.

use crate::ability::{ControllerScope, FilterContext, PermanentFilter, Reading};
use crate::card::CardDatabase;
use crate::id::PermanentId;
use crate::state::{GameState, Permanent};

/// Whether `perm` is in the class `filter` names, for an object described by `ctx`.
///
/// Every field of the filter is conjunctive and an absent field asks nothing, so the
/// empty filter is "every permanent the scope allows". The order below is cheapest-first
/// only where that changes nothing: the computed reads are last because they are the ones
/// that walk the layer system.
#[must_use]
pub(crate) fn permanent_matches(
    state: &GameState,
    db: &CardDatabase,
    filter: &PermanentFilter,
    perm: &Permanent,
    ctx: FilterContext,
) -> bool {
    // CR 613 layer 2, read through the one control path — applied before every layer this
    // predicate reads, and itself unable to recurse, so it is answered the same way from
    // inside the layer walk as from outside it. An anthem stops pumping a creature the
    // moment someone else gains control of it, and starts pumping one it just stole.
    let seat = crate::characteristics::controller_of(state, perm);
    let scope_ok = match filter.scope {
        ControllerScope::YouControl => seat == ctx.controller,
        // A seat that has lost is no longer an opponent (CR 102.1); its permanents are on
        // their way off the battlefield in the same state-based-action pass.
        ControllerScope::OpponentsControl => {
            seat != ctx.controller
                && state
                    .players
                    .get(seat.0)
                    .is_some_and(|player| !player.has_lost)
        }
        ControllerScope::ThatPlayer => ctx.chosen_player == Some(seat),
        ControllerScope::Any => true,
    };
    if !scope_ok {
        return false;
    }

    // "Other …". An object with no source permanent — a spell's own effects, an emblem —
    // excludes nothing, which is the honest reading of a sentence that has no "this".
    if filter.except_this && ctx.source == Some(perm.id) {
        return false;
    }
    // CR 111: a token is not a card. Read off what the permanent is, never inferred from a
    // missing card handle.
    if let Some(wanted) = filter.token {
        if perm.printed.is_token() != wanted {
            return false;
        }
    }
    if filter.attacking && perm.attacking.is_none() {
        return false;
    }
    // A counter is not a characteristic and no layer produces one, so this is the one
    // narrowing that reads the same under both readings and can be asked from anywhere.
    if let Some(kind) = filter.with_counter {
        if perm.counter_count(kind) == 0 {
            return false;
        }
    }
    if filter.with_the_named_card && !bears_the_named_card(state, ctx.source, perm) {
        return false;
    }

    let Some(face) = perm.printed.face(db) else {
        return false;
    };
    // Subtype and colour are printed under both readings. Subtype because CR 613 layer 4
    // is only modelled as *adding* types to one named permanent and a class read is not
    // where that arrives; colour because layer 5 is not modelled on the battlefield at
    // all, so printed colour is current colour here exactly as it is in the blocking
    // restrictions that name one.
    if let Some(wanted) = &filter.subtype {
        if !face.has_subtype(wanted) {
            return false;
        }
    }
    if let Some(wanted) = filter.color {
        if !face.colors().contains(&wanted) {
            return false;
        }
    }

    match ctx.reading {
        // Outside the layer system: every remaining question is answered from the
        // permanent's current characteristics. An artifact animated into a creature is in
        // every class of creatures, which is what makes it die to a sweeper, and a
        // creature *granted* defender is in the class that names one.
        Reading::Computed => {
            let current = crate::characteristics::characteristics(state, perm.id, db);
            if !filter.card_type.is_empty()
                && !filter
                    .card_type
                    .iter()
                    .any(|wanted| current.types.contains(wanted))
            {
                return false;
            }
            if let Some(wanted) = filter.keyword {
                if !current.keywords.contains(&wanted) {
                    return false;
                }
            }
            if let Some(wanted) = filter.without_keyword {
                if current.keywords.contains(&wanted) {
                    return false;
                }
            }
            if let Some(min) = filter.min_power {
                if !current.power.is_some_and(|power| power >= min) {
                    return false;
                }
            }
            if let Some(max) = filter.max_toughness {
                if !current.toughness.is_some_and(|toughness| toughness <= max) {
                    return false;
                }
            }
            // "Less than its power", against a source that may already be gone: the
            // caller read the number before the cost was paid, and no number means an
            // empty class rather than a universal one.
            if filter.below_source_power {
                let below = ctx
                    .source_power
                    .is_some_and(|source| current.power.is_some_and(|power| power < source));
                if !below {
                    return false;
                }
            }
            true
        }
        // Inside the layer walk. Types and keywords come off the printed face, because the
        // computed sets are what the walk is producing. The power and toughness bounds
        // cannot be answered at all here, and the catalog validator refuses them rather
        // than letting this arm quietly say "no" to a card that looked authored.
        Reading::Printed => {
            if !filter.card_type.is_empty()
                && !filter.card_type.iter().any(|&wanted| face.has_type(wanted))
            {
                return false;
            }
            if let Some(wanted) = filter.keyword {
                if !face.keywords().contains(&wanted) {
                    return false;
                }
            }
            if let Some(wanted) = filter.without_keyword {
                if face.keywords().contains(&wanted) {
                    return false;
                }
            }
            true
        }
    }
}

/// Every permanent in the class, in battlefield order.
///
/// The set is whatever the caller's moment says it is: a resolution enumerates once
/// (CR 611.2c) and keeps what it found, a static ability re-derives on every read. This
/// function does not know the difference and does not need to — it is the *caller* that
/// calls it once or calls it again.
#[must_use]
pub(crate) fn permanents_matching(
    state: &GameState,
    db: &CardDatabase,
    filter: &PermanentFilter,
    ctx: FilterContext,
) -> Vec<PermanentId> {
    state
        .battlefield
        .iter()
        .filter(|perm| permanent_matches(state, db, filter, perm, ctx))
        .map(|perm| perm.id)
        .collect()
}

/// Whether `perm`'s printed card is the one `source` named as it entered (CR 614.12).
///
/// Card identity, not a string: two printings of one functional card share a `CardId` and
/// nothing else does. A source that named nothing, or that is not a permanent at all,
/// matches nothing — there is no chosen name, so no permanent has it.
fn bears_the_named_card(state: &GameState, source: Option<PermanentId>, perm: &Permanent) -> bool {
    let named = source
        .and_then(|id| state.battlefield.iter().find(|p| p.id == id))
        .and_then(|source| source.named_card);
    named.is_some() && named == perm.printed.card()
}
