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
    // Each mode is its own list, because each is announced on its own: one mode's
    // variable-arity group has nothing to do with another's, and the two are never
    // resolved together (CR 700.2).
    for mode in object
        .get("modes")
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        if let Some(effects) = mode.get("effects").and_then(serde_json::Value::as_array) {
            lists.push(effects.iter().collect());
        }
    }
    // An ability an attachment grants is one object's worth of announcement once it is
    // on its host, so its effect list is its own list here.
    for ability in granted_abilities(object) {
        if let Some(effects) = ability.get("effects").and_then(serde_json::Value::as_array) {
            lists.push(effects.iter().collect());
        }
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

/// Whether `object`'s `attachment` block scales its grant by a power bound.
///
/// The attachment counterpart of [`static_condition_counts_by_power`], and refused for
/// exactly the same reason — see
/// [`Violation::PowerInAttachmentCount`](super::Violation::PowerInAttachmentCount).
pub(super) fn attachment_counts_by_power(
    object: &serde_json::Map<String, serde_json::Value>,
) -> bool {
    object
        .get("attachment")
        .and_then(|attachment| attachment.get("count_of"))
        .and_then(|count_of| count_of.get("min_power"))
        .is_some()
}

/// The effect kind that makes an ability function from a graveyard (CR 113.6): the
/// source moves *itself* out of the graveyard it is in.
const RETURN_SELF_FROM_GRAVEYARD: &str = "return_self_from_graveyard";

/// Whether `object` authors a `return_self_from_graveyard` anywhere it could not work,
/// or on an activated ability whose cost a card in a graveyard could not pay.
///
/// Two failures, one rule — "where the ability functions is derived from what it does"
/// only holds if the effect appears where that derivation is true:
///
/// - **Anywhere but an activated or a triggered ability.** A spell effect or an ability
///   handed to an emblem has no source in a graveyard for the return to act on, so the
///   effect would sit in a card that reads as recursive and never is. An ability a card
///   **grants** — an Aura's, or a pump's — counts as one of those positions: it is an
///   ability, it will sit on an object that can die, and where it happens to be authored
///   says nothing about whether it works.
/// - **Beside a cost a card in a graveyard cannot pay.** A card in a zone is not a
///   permanent: it cannot be tapped, sacrificed, or have counters removed. Mana is the
///   only cost component such an ability can charge, and one that charged anything else
///   would simply never be offered — a dead ability, caught here rather than shipped.
///
/// A triggered ability has no activation cost, so only the first rule applies to it; the
/// `you may pay` a printed one wraps the return in is an [`Effect::May`](crate::Effect),
/// whose cost is mana by construction.
pub(super) fn graveyard_ability_is_bad(
    object: &serde_json::Map<String, serde_json::Value>,
) -> bool {
    fn returns_self(effect: &serde_json::Value) -> bool {
        effect.get("kind").and_then(serde_json::Value::as_str) == Some(RETURN_SELF_FROM_GRAVEYARD)
    }
    /// Occurrences in one ability's own effect tree — its list, and the `may` and
    /// `conditional` wrappers inside it. Deliberately **not** [`nested_effects`], which
    /// also descends into the abilities an effect hands out: those are counted by walking
    /// the abilities themselves below, and an emblem's — which are in no zone and have no
    /// card in a graveyard — are counted nowhere and fall out as a mismatch.
    fn count_in(effects: &[serde_json::Value]) -> usize {
        effects
            .iter()
            .map(|effect| {
                usize::from(returns_self(effect))
                    + ["effects", "then", "otherwise"]
                        .into_iter()
                        .filter_map(|key| effect.get(key).and_then(serde_json::Value::as_array))
                        .map(|nested| count_in(nested))
                        .sum::<usize>()
            })
            .sum()
    }
    // Every ability position that becomes a real ability of a real object: the card's
    // own, and the ones it grants — through an `attachment` block, or through the
    // `abilities` an effect like a pump hands to the creature it targets. An emblem's are
    // deliberately absent, which is what makes one authored there a mismatch.
    let mut abilities: Vec<&serde_json::Value> = object
        .get("abilities")
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .collect();
    abilities.extend(granted_abilities(object));
    for effect in every_effect(object) {
        if effect.get("kind").and_then(serde_json::Value::as_str) == Some("pump") {
            abilities.extend(
                effect
                    .get("abilities")
                    .and_then(serde_json::Value::as_array)
                    .map(Vec::as_slice)
                    .unwrap_or_default(),
            );
        }
    }
    // Every authored occurrence, at any depth and in any list.
    let total = every_effect(object)
        .into_iter()
        .filter(|effect| returns_self(effect))
        .count();
    // The ones inside an activated or triggered ability, where they work.
    let mut on_abilities = 0;
    for ability in abilities {
        let kind = ability.get("type").and_then(serde_json::Value::as_str);
        if !matches!(kind, Some("activated" | "triggered")) {
            continue;
        }
        let effects = ability
            .get("effects")
            .and_then(serde_json::Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let here = count_in(effects);
        if here == 0 {
            continue;
        }
        on_abilities += here;
        let payable = ability
            .get("cost")
            .and_then(serde_json::Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .all(|component| {
                component.get("kind").and_then(serde_json::Value::as_str) == Some("mana")
            });
        if !payable {
            return true;
        }
    }
    total != on_abilities
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

/// Whether `effect` is an `alter_abilities_self` that changes nothing — no `lose_all`,
/// no keyword lost, no keyword gained. Every field defaults, so the empty clause is
/// exactly the one a typo produces.
pub(super) fn ability_change_is_empty(effect: &serde_json::Value) -> bool {
    if effect.get("kind").and_then(serde_json::Value::as_str) != Some("alter_abilities_self") {
        return false;
    }
    let names_none = |key: &str| {
        effect
            .get(key)
            .and_then(serde_json::Value::as_array)
            .is_none_or(Vec::is_empty)
    };
    effect.get("lose_all").and_then(serde_json::Value::as_bool) != Some(true)
        && names_none("lose")
        && names_none("gain")
}

/// The abilities a definition **grants** rather than has: the ones an `attachment` block
/// hands to whatever it is attached to (CR 613.1f).
///
/// A third ability position beside the card's own `abilities` and the ones a
/// `create_emblem` effect writes, and it has to be walked like the other two: an ability
/// a card gives away is still an ability this catalog authored, so every rule stated
/// about "every effect a definition authors" has to be able to see inside it.
///
/// The abilities a **pump** grants need no entry here — they hang off an effect, and
/// [`nested_effects`] already descends into any effect's `abilities`.
pub(super) fn granted_abilities(
    object: &serde_json::Map<String, serde_json::Value>,
) -> &[serde_json::Value] {
    object
        .get("attachment")
        .and_then(|attachment| attachment.get("abilities"))
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
}

/// Every effect a definition authors at the top level of an ability or of its spell
/// effects, in file order.
///
/// Shallow on purpose: the nested contents of an effect are the business of whatever
/// walks *that* effect ([`declared_target_groups`] recurses into its own).
pub(super) fn authored_effects(
    object: &serde_json::Map<String, serde_json::Value>,
) -> impl Iterator<Item = &serde_json::Value> {
    fn effects_of(abilities: &[serde_json::Value]) -> impl Iterator<Item = &serde_json::Value> {
        abilities
            .iter()
            .filter_map(|ability| ability.get("effects"))
            .filter_map(serde_json::Value::as_array)
            .flatten()
    }
    let abilities = effects_of(
        object
            .get("abilities")
            .and_then(serde_json::Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default(),
    );
    let spell = object
        .get("spell_effects")
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter();
    // A modal spell's effects hang off its modes instead (CR 700.2), and every rule
    // stated about "every effect a definition authors" has to reach them — otherwise a
    // check that holds for a plain spell would quietly not hold once the same text was
    // written as one bullet of two. An attachment's granted abilities carry effects for
    // the same reason.
    let modes = mode_effects(object).into_iter();
    abilities
        .chain(spell)
        .chain(modes)
        .chain(effects_of(granted_abilities(object)))
}

/// The effects of every mode a definition declares, flattened (CR 700.2).
pub(super) fn mode_effects(
    object: &serde_json::Map<String, serde_json::Value>,
) -> Vec<&serde_json::Value> {
    object
        .get("modes")
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|mode| mode.get("effects"))
        .filter_map(serde_json::Value::as_array)
        .flatten()
        .collect()
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

/// The authored keys that carry a target spec — the JSON spelling of one target slot.
///
/// Nearly every targeting effect names its one slot `target`. A `fight` names two, and
/// names them separately *because* they do not share a spec (CR 701.12). Keeping the set
/// as a list is what makes counting slots one rule rather than one rule plus a special
/// case: the next effect with slots of its own adds a key here.
const TARGET_SPEC_FIELDS: [&str; 3] = ["target", "dealer", "dealt_to"];

/// How many target groups `effect` declares — the JSON mirror of
/// [`Effect::target_groups`](crate::Effect::target_groups), which every announcement path
/// reads.
///
/// One per target-spec field the effect names, plus one for a `player_ref` naming a
/// targeted seat; for a `may`, the sum over what it wraps, because that is precisely what
/// the wrapper forwards. A `conditional` declares none, however its branches are written
/// — [`conditional_wraps_a_target`] is what rejects a branch that tries.
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
    target_spec_fields(effect) + usize::from(names_a_targeted_seat(effect))
}

/// How many of `effect`'s own fields carry a target spec ([`TARGET_SPEC_FIELDS`]).
fn target_spec_fields(effect: &serde_json::Value) -> usize {
    TARGET_SPEC_FIELDS
        .iter()
        .filter(|key| effect.get(*key).is_some())
        .count()
}

/// Whether `effect` names a **targeted seat** through its `player_ref` — the spelling a
/// life-, mill-, or discard-shaped effect uses instead of a target spec.
fn names_a_targeted_seat(effect: &serde_json::Value) -> bool {
    matches!(
        effect.get("player_ref").and_then(serde_json::Value::as_str),
        Some("target_player" | "target_opponent")
    )
}

/// Whether `effect` itself names a target, in either of the two authored spellings: a
/// target spec field, or a `player_ref` naming a targeted seat.
fn names_a_target(effect: &serde_json::Value) -> bool {
    target_spec_fields(effect) > 0 || names_a_targeted_seat(effect)
}

/// Whether `effect`, or anything nested inside it, chooses a target (CR 115.1).
///
/// Two authored spellings say "target", and both count: a target spec field on the
/// effect itself, and a `player_ref` naming a targeted seat. Kept here rather than in the
/// typed IR because `build.rs` validates JSON before the IR exists (ADR 0008 §5).
pub(super) fn effect_chooses_a_target(effect: &serde_json::Value) -> bool {
    names_a_target(effect)
        || nested_effects(effect)
            .into_iter()
            .any(effect_chooses_a_target)
}

/// Whether any ability of `object` watches "a spell of the **chosen color**" — the one
/// trigger selector whose meaning comes from elsewhere on the same card (CR 614.12).
///
/// Reads the authored `event` shape directly: `{"you_cast_spell": "chosen_color"}`. Like
/// everything else here it works on JSON, because `build.rs` validates a definition
/// before the typed IR exists (ADR 0008 §5).
pub(super) fn watches_the_chosen_color(
    object: &serde_json::Map<String, serde_json::Value>,
) -> bool {
    abilities_of(object).iter().any(|ability| {
        ability
            .get("event")
            .and_then(|event| event.get("you_cast_spell"))
            .and_then(serde_json::Value::as_str)
            == Some("chosen_color")
    })
}

/// The two [`DerivedAmount`](crate::DerivedAmount) sources that read an object's own cost
/// payment rather than the game (CR 601.2h).
const PAYMENT_AMOUNTS: [&str; 2] = ["sacrificed_to_cost", "sacrificed_creature_power"];

/// Whether `object` reads an amount off a cost payment its own cost never makes — see
/// [`Violation::PaymentAmountIsNeverPaid`](super::Violation::PaymentAmountIsNeverPaid).
///
/// The pair rule that matches [`watches_the_chosen_color`]'s: one half of the card names a
/// number, the other half has to be the thing that produces it. Reads the authored JSON,
/// because `build.rs` validates a definition before the typed IR exists (ADR 0008 §5).
pub(super) fn reads_an_unpaid_amount(object: &serde_json::Map<String, serde_json::Value>) -> bool {
    let reads = every_effect(object).into_iter().any(|effect| {
        ["amount", "take_amount"].into_iter().any(|key| {
            effect
                .get(key)
                .and_then(|amount| amount.get("source"))
                .and_then(serde_json::Value::as_str)
                .is_some_and(|source| PAYMENT_AMOUNTS.contains(&source))
        })
    });
    reads && !sacrifices_something(object)
}

/// Whether any cost of `object` sacrifices a permanent the payer picks — an additional
/// cast cost, or a component of an activation cost.
fn sacrifices_something(object: &serde_json::Map<String, serde_json::Value>) -> bool {
    let kind_is_sacrifice = |value: &serde_json::Value| {
        value.get("kind").and_then(serde_json::Value::as_str) == Some("sacrifice")
    };
    if object.get("additional_cost").is_some_and(kind_is_sacrifice) {
        return true;
    }
    abilities_of(object).iter().any(|ability| {
        ability
            .get("cost")
            .and_then(serde_json::Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .any(kind_is_sacrifice)
    })
}

/// Whether `object` declares the `enters_choosing_color` ability — whether it names a
/// colour as it enters (CR 614.12), and so whether "the chosen color" refers to anything.
pub(super) fn object_chooses_a_color(object: &serde_json::Map<String, serde_json::Value>) -> bool {
    abilities_of(object).iter().any(|ability| {
        ability.get("type").and_then(serde_json::Value::as_str) == Some("enters_choosing_color")
    })
}

/// A definition's authored `abilities` array, or an empty slice when it has none.
fn abilities_of(object: &serde_json::Map<String, serde_json::Value>) -> &[serde_json::Value] {
    object
        .get("abilities")
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
}
