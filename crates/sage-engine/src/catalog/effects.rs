//! Walking a definition's authored effects: finding every effect in a definition —
//! including the ones nested inside an optional or conditional wrapper — and the
//! shape checks that read one.
//!
//! Split out of `catalog.rs` for size (issue #711), and under the same constraint:
//! `serde_json::Value` and `std` only, never a `crate::` path.

use super::*;

/// Validate the `token` block of a `create_token` effect, authored by `functional_id`.
///
/// The token analogue of the type and power/toughness rules [`validate_definition`]
/// applies to a card, and deliberately the same two: an object that is not a permanent
/// could not be on the battlefield, and a creature without printed power and toughness
/// is not a creature anyone can play with. Everything else a token may not have — a
/// `functional_id`, a mana cost, a `scripted` flag — is unrepresentable in
/// [`TokenData`](crate::TokenData) rather than checked here, so it is a parse error
/// instead of a validation one.
pub(super) fn validate_token(
    functional_id: &str,
    token: Option<&serde_json::Value>,
) -> Result<(), Violation> {
    // A missing or malformed `token` is a parse error on the typed side; there is
    // nothing to validate here.
    let Some(token) = token.and_then(serde_json::Value::as_object) else {
        return Ok(());
    };
    let types = token
        .get("types")
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let is_permanent = !types.is_empty()
        && types.iter().all(|t| {
            t.as_str()
                .is_some_and(|name| PERMANENT_TYPES.contains(&name))
        });
    if !is_permanent {
        return Err(Violation::TokenIsNotAPermanent {
            functional_id: functional_id.to_string(),
        });
    }
    let is_creature = types.iter().any(|t| t.as_str() == Some(CREATURE_TYPE));
    let has_power = token.contains_key("power");
    let has_toughness = token.contains_key("toughness");
    if is_creature != (has_power && has_toughness) || has_power != has_toughness {
        return Err(Violation::TokenPowerToughnessMismatch {
            functional_id: functional_id.to_string(),
            creature: is_creature,
        });
    }
    Ok(())
}

/// Every effect a definition authors, at any nesting depth — the top-level lists
/// [`authored_effects`] yields, plus everything nested inside them (the contents of a
/// `may`).
///
/// Used by rules that are about an effect wherever it appears, rather than about the
/// shape of the list it sits in.
pub(super) fn every_effect(
    object: &serde_json::Map<String, serde_json::Value>,
) -> Vec<&serde_json::Value> {
    fn walk<'a>(effect: &'a serde_json::Value, out: &mut Vec<&'a serde_json::Value>) {
        out.push(effect);
        for nested in nested_effects(effect) {
            walk(nested, out);
        }
    }
    let mut out = Vec::new();
    for effect in authored_effects(object) {
        walk(effect, &mut out);
    }
    out
}

/// The effects nested inside `effect`, whatever key they hang off: a `may`'s `effects`,
/// a `conditional`'s `then` and `otherwise`, and the effect lists of the abilities a
/// `create_emblem` hands out.
///
/// One function so a rule stated about "every effect a definition authors" cannot be
/// true of one wrapper and quietly false of the next one added.
pub(super) fn nested_effects(effect: &serde_json::Value) -> Vec<&serde_json::Value> {
    let mut out = Vec::new();
    for key in ["effects", "then", "otherwise"] {
        out.extend(
            effect
                .get(key)
                .and_then(serde_json::Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_default(),
        );
    }
    for ability in effect
        .get("abilities")
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        out.extend(
            ability
                .get("effects")
                .and_then(serde_json::Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_default(),
        );
    }
    out
}

/// Every **list** of effects a definition authors as one announcement's worth: each
/// ability's `effects`, the card's `spell_effects`, and each ability an emblem is created
/// with.
///
/// Distinct from [`every_effect`], which flattens: the variable-arity rule is about what
/// one *object on the stack* declares together, so it has to see the lists rather than
/// the effects.
pub(super) fn effect_lists(
    object: &serde_json::Map<String, serde_json::Value>,
) -> Vec<Vec<&serde_json::Value>> {
    let mut lists: Vec<Vec<&serde_json::Value>> = Vec::new();
    for ability in object
        .get("abilities")
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        if let Some(effects) = ability.get("effects").and_then(serde_json::Value::as_array) {
            lists.push(effects.iter().collect());
        }
    }
    if let Some(effects) = object
        .get("spell_effects")
        .and_then(serde_json::Value::as_array)
    {
        lists.push(effects.iter().collect());
    }
    for effect in every_effect(object) {
        for ability in effect
            .get("abilities")
            .and_then(serde_json::Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
        {
            if let Some(effects) = ability.get("effects").and_then(serde_json::Value::as_array) {
                lists.push(effects.iter().collect());
            }
        }
    }
    lists
}

/// How many of `effects` declare an `"up_to"` target count — the variable-arity groups
/// one announcement would have to split its flat target list between.
pub(super) fn variable_target_groups(effects: &[&serde_json::Value]) -> usize {
    effects
        .iter()
        .filter(|effect| declares_variable_group(effect))
        .count()
}

/// Whether `effect` declares an `"up_to"` target count, looking **through** a `may` —
/// which forwards the group of the effect it wraps, so an optional "return up to two
/// target cards" is as variable-arity as a bare one.
fn declares_variable_group(effect: &serde_json::Value) -> bool {
    if effect
        .get("targets")
        .and_then(serde_json::Value::as_object)
        .is_some_and(|count| count.contains_key("up_to"))
    {
        return true;
    }
    is_optional(effect)
        && effect
            .get("effects")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|nested| nested.iter().any(declares_variable_group))
}

/// Whether `effect` is a `conditional` whose branches would choose a target.
pub(super) fn conditional_wraps_a_target(effect: &serde_json::Value) -> bool {
    if effect.get("kind").and_then(serde_json::Value::as_str) != Some("conditional") {
        return false;
    }
    ["then", "otherwise"].into_iter().any(|key| {
        effect
            .get(key)
            .and_then(serde_json::Value::as_array)
            .is_some_and(|branch| branch.iter().any(effect_chooses_a_target))
    })
}

/// Whether any **static** ability of `object` gates itself on a power bound.
///
/// A power bound is read through the computed characteristics, which is correct in a
/// resolution and non-terminating inside the layer system — see
/// [`Violation::PowerInStaticCondition`](super::Violation::PowerInStaticCondition).
/// Only `type: "static"` is checked: the same condition shape appears on an
/// `Effect::Conditional`, where it is evaluated during a resolution and is fine.
pub(super) fn static_condition_counts_by_power(
    object: &serde_json::Map<String, serde_json::Value>,
) -> bool {
    object
        .get("abilities")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter(|ability| ability.get("type").and_then(serde_json::Value::as_str) == Some("static"))
        .any(|ability| {
            ability
                .get("condition")
                .and_then(|condition| condition.get("permanents"))
                .and_then(|permanents| permanents.get("min_power"))
                .is_some()
        })
}

/// Whether `effect` is a `create_emblem` handing out an ability an emblem cannot carry
/// (CR 114.1) — anything but `static` or `triggered`.
pub(super) fn emblem_ability_is_bad(effect: &serde_json::Value) -> bool {
    if effect.get("kind").and_then(serde_json::Value::as_str) != Some("create_emblem") {
        return false;
    }
    effect
        .get("abilities")
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .any(|ability| {
            !matches!(
                ability.get("type").and_then(serde_json::Value::as_str),
                Some("static" | "triggered")
            )
        })
}

/// Every effect a definition authors at the top level of an ability or of its spell
/// effects, in file order.
///
/// Shallow on purpose: the nested contents of an effect are the business of whatever
/// walks *that* effect ([`declared_target_groups`] recurses into its own).
pub(super) fn authored_effects(
    object: &serde_json::Map<String, serde_json::Value>,
) -> impl Iterator<Item = &serde_json::Value> {
    let abilities = object
        .get("abilities")
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|ability| ability.get("effects"))
        .filter_map(serde_json::Value::as_array)
        .flatten();
    let spell = object
        .get("spell_effects")
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter();
    abilities.chain(spell)
}

/// Whether `effect` is a `may` wrapping **more than one** targeting effect.
///
/// A `may` forwards the target group of the one effect it wraps, so the slot is
/// declared at announcement and filled there; two of them would need two slots off one
/// wrapper, and the flat stored target list has no way to say which is which.
pub(super) fn optional_declares_two_targets(effect: &serde_json::Value) -> bool {
    is_optional(effect) && declared_target_groups(effect) > 1
}

/// Whether `effect` is an optional wrapper (`{"kind":"may"}`).
fn is_optional(effect: &serde_json::Value) -> bool {
    effect.get("kind").and_then(serde_json::Value::as_str) == Some("may")
}

/// How many target groups `effect` declares — the JSON mirror of
/// [`Effect::target_group`](crate::Effect::target_group), which every announcement path
/// reads.
///
/// One for an effect that names a target of its own; for a `may`, the sum over what it
/// wraps, because that is precisely what the wrapper forwards. A `conditional` declares
/// none, however its branches are written — [`conditional_wraps_a_target`] is what
/// rejects a branch that tries.
fn declared_target_groups(effect: &serde_json::Value) -> usize {
    if is_optional(effect) {
        return effect
            .get("effects")
            .and_then(serde_json::Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .map(declared_target_groups)
            .sum();
    }
    if effect.get("kind").and_then(serde_json::Value::as_str) == Some("conditional") {
        return 0;
    }
    usize::from(names_a_target(effect))
}

/// Whether `effect` itself names a target, in either of the two authored spellings: a
/// `target` spec, or a `player_ref` naming a targeted seat.
fn names_a_target(effect: &serde_json::Value) -> bool {
    effect.get("target").is_some()
        || matches!(
            effect.get("player_ref").and_then(serde_json::Value::as_str),
            Some("target_player" | "target_opponent")
        )
}

/// Whether `effect`, or anything nested inside it, chooses a target (CR 115.1).
///
/// Two authored spellings say "target", and both count: a `target` spec on the effect
/// itself, and a `player_ref` naming a targeted seat. Kept here rather than in the typed
/// IR because `build.rs` validates JSON before the IR exists (ADR 0008 §5).
pub(super) fn effect_chooses_a_target(effect: &serde_json::Value) -> bool {
    if effect.get("target").is_some() {
        return true;
    }
    if matches!(
        effect.get("player_ref").and_then(serde_json::Value::as_str),
        Some("target_player" | "target_opponent")
    ) {
        return true;
    }
    nested_effects(effect)
        .into_iter()
        .any(effect_chooses_a_target)
}
