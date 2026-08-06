//! The rules a **face** must obey, checked once per face a definition authors.
//!
//! Split out of `mod.rs` because a two-faced card asks every one of them twice
//! (CR 712.2): a back face has its own types, its own power/toughness or loyalty, and
//! its own abilities, so a rule stated only about the top-level object would be enforced
//! on the front of a card and silently unenforced on the back. One function, called
//! once per face, is what stops that.
//!
//! Under the same constraint as the rest of `src/catalog/`: `serde_json::Value` and
//! `std` only, never a `crate::` path — `build.rs` compiles this file before the engine
//! exists.

use super::*;

/// Validate one **face** of a definition: everything that is true of a face's printed
/// characteristics and of the effects it authors, whichever side of the card it is on.
///
/// `functional_id` names the *card*, not the face (ADR 0008 §3) — a two-faced card has
/// one identity, so a violation on either side reports against it.
///
/// The card-level rules that are **not** here are the ones a back face cannot have: an
/// additional cast cost and an attachment block are fields only [`CardData`](crate::CardData)
/// carries, so they are checked once, by the caller.
///
/// # Errors
/// Returns the first [`Violation`] found.
pub(super) fn validate_face(
    functional_id: &str,
    object: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), Violation> {
    let types = object
        .get("types")
        .and_then(serde_json::Value::as_array)
        .filter(|types| !types.is_empty())
        .ok_or_else(|| Violation::MalformedField {
            functional_id: functional_id.to_string(),
            field: "types",
        })?;

    // A Creature carries printed power and toughness; nothing else may (ADR 0008 §5).
    // Checked as a pair: half a P/T is as wrong as none at all on a creature.
    let is_creature = types.iter().any(|t| t.as_str() == Some(CREATURE_TYPE));
    let has_power = object.contains_key("power");
    let has_toughness = object.contains_key("toughness");
    if is_creature != (has_power && has_toughness) || has_power != has_toughness {
        return Err(Violation::PowerToughnessMismatch {
            functional_id: functional_id.to_string(),
            creature: is_creature,
        });
    }

    // A Planeswalker carries a printed starting loyalty and nothing else does
    // (CR 306.5b) — the same both-directions pairing the P/T check above makes, and for
    // the same reason: a planeswalker with none would die to CR 704.5i on arrival. It is
    // asked of each face because a face is what a permanent *is*: the back face a
    // creature turns into may be a planeswalker, and it is that face's number the
    // permanent would enter with.
    let is_planeswalker = types.iter().any(|t| t.as_str() == Some(PLANESWALKER_TYPE));
    if is_planeswalker != object.contains_key("loyalty") {
        return Err(Violation::LoyaltyMismatch {
            functional_id: functional_id.to_string(),
            planeswalker: is_planeswalker,
        });
    }

    // A printed combat restriction only ever restricts attacking or blocking, so it
    // belongs only on a creature. An Aura *grants* restrictions to its host through
    // `aura.restrictions` instead, which is why this looks only at the printed list.
    if object.contains_key("restrictions") && !is_creature {
        return Err(Violation::RestrictionsOnNonCreature {
            functional_id: functional_id.to_string(),
        });
    }

    validate_face_effects(functional_id, object)
}

/// The rules about the *effects* a face authors — everything reached through its
/// abilities and, on a front face, its spell ability.
///
/// # Errors
/// Returns the first [`Violation`] found.
fn validate_face_effects(
    functional_id: &str,
    object: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), Violation> {
    let named = || functional_id.to_string();

    // An optional effect forwards the target group of the one effect it wraps, so it may
    // wrap one targeting effect and no more (see [`Violation::TwoTargetsInsideOptional`]).
    // Walked to any depth, so a `may` inside a `may` is counted too.
    if every_effect(object)
        .into_iter()
        .any(optional_declares_two_targets)
    {
        return Err(Violation::TwoTargetsInsideOptional {
            functional_id: named(),
        });
    }

    // A conditional's branches get no such forwarding: two branches share one flat
    // target list, so a group named in either could not be paired back onto the branch
    // that was taken.
    if every_effect(object)
        .into_iter()
        .any(conditional_wraps_a_target)
    {
        return Err(Violation::TargetInsideConditional {
            functional_id: named(),
        });
    }

    // A power bound is the one selector field read from computed characteristics, which
    // the layer system cannot ask for from inside itself without recursing. Two sites are
    // evaluated there: a static ability's condition, and an attachment's counted grant.
    if static_condition_counts_by_power(object) {
        return Err(Violation::PowerInStaticCondition {
            functional_id: named(),
        });
    }
    if attachment_counts_by_power(object) {
        return Err(Violation::PowerInAttachmentCount {
            functional_id: named(),
        });
    }

    // CR 114.1: an emblem has no characteristics but its abilities, and only the two
    // kinds that need neither an activation nor an entry event can function on one.
    if every_effect(object).into_iter().any(emblem_ability_is_bad) {
        return Err(Violation::EmblemAbilityIsNotStaticOrTriggered {
            functional_id: named(),
        });
    }

    // A layer-6 change that neither subtracts nor adds is no effect at all, and every
    // field of one defaults — so the empty clause is the shape a typo lands on.
    if every_effect(object)
        .into_iter()
        .any(ability_change_is_empty)
    {
        return Err(Violation::AbilityChangeIsEmpty {
            functional_id: named(),
        });
    }

    // CR 113.6: an ability functions from a graveyard *because* it returns its own card
    // from there, so the effect belongs on an activated ability and nowhere else — and
    // that ability may charge only mana, a card in a zone having nothing to tap,
    // sacrifice, or spend counters from.
    if graveyard_ability_is_bad(object) {
        return Err(Violation::GraveyardAbilityCannotFunction {
            functional_id: named(),
        });
    }

    // At most one "up to N" target group per ability or spell, so the flat stored target
    // list pairs back onto effects unambiguously.
    if effect_lists(object)
        .into_iter()
        .any(|effects| variable_target_groups(&effects) > 1)
    {
        return Err(Violation::TwoVariableTargetGroups {
            functional_id: named(),
        });
    }

    // Every token a definition creates must be an object that could exist: a permanent
    // (CR 110.1/111.7), with power and toughness exactly when it is a creature. Walked
    // to any depth, so a `create_token` nested inside a `may` is checked too.
    for effect in every_effect(object) {
        if effect.get("kind").and_then(serde_json::Value::as_str) != Some("create_token") {
            continue;
        }
        validate_token(functional_id, effect.get("token"))?;
    }

    // "A spell of the chosen color" is the one trigger selector whose meaning comes from
    // elsewhere on the same card (CR 614.12). A card that watches it without naming a
    // colour as it enters has written a trigger that can never fire, and the engine's
    // answer to it — notice nothing — is silent by design, so it is caught here instead.
    if watches_the_chosen_color(object) && !object_chooses_a_color(object) {
        return Err(Violation::ChosenColorIsNeverNamed {
            functional_id: named(),
        });
    }

    Ok(())
}

/// Validate a definition's `back_face` block (CR 712.2), if it has one.
///
/// Two rules on top of [`validate_face`], and both are about what a back face *is*:
///
/// - **It has no mana cost and can never be cast** (CR 712.4a). The field exists on
///   [`BackFace`](crate::BackFace) so this can be enforced rather than assumed — an
///   author who writes one is told which card is wrong, instead of meeting a
///   `deny_unknown_fields` parse error that names a line number.
/// - **It transforms into something.** A `back_face` that is not an object is a parse
///   error on the typed side; here it is simply not walked.
///
/// # Errors
/// Returns the first [`Violation`] found.
pub(super) fn validate_back_face(
    functional_id: &str,
    object: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), Violation> {
    let Some(back) = object
        .get("back_face")
        .and_then(serde_json::Value::as_object)
    else {
        return Ok(());
    };
    let cost = back
        .get("mana_cost")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if !cost.is_empty() {
        return Err(Violation::BackFaceHasManaCost {
            functional_id: functional_id.to_string(),
        });
    }
    validate_face(functional_id, back)
}

/// Whether `object` authors an effect that turns its own source over — either road.
///
/// Walked over every effect at any depth, so a transform inside a `may` counts: an
/// ability that only sometimes turns the permanent over still needs a face to turn to.
pub(super) fn transforms_itself(object: &serde_json::Map<String, serde_json::Value>) -> bool {
    every_effect(object).into_iter().any(|effect| {
        matches!(
            effect.get("kind").and_then(serde_json::Value::as_str),
            Some("transform_self" | "exile_self_and_return_transformed")
        )
    })
}
