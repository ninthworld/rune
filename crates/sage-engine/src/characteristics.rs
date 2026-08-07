//! Computed characteristics: the single pure read path for a permanent's
//! *current* characteristics (CR 613, the layer system).
//!
//! A permanent's current power/toughness, types, and abilities are **not** what
//! is printed on its card — counters, anthems, pump spells, and type-changing
//! effects alter them continuously. Per ADR 0005 the engine never stores these:
//! [`characteristics`] recomputes them fresh on every call from the raw state
//! plus the printed [`CardData`](crate::CardData) seed, caching nothing
//! (consistent with the `GameState` "no cached derivations" invariant in
//! `state.rs`).
//!
//! This module is **slice 3 of 3** (ADR 0005 §3): it seeds current
//! characteristics from printed values, lets a characteristic-defining ability
//! (CR 604.3) *replace* the printed power at CR 613 **layer 7a**, folds `+1/+1` and
//! `-1/-1` counters into power/toughness at **layer 7c**, and then applies simple
//! static P/T modifications (anthem-style "+X/+Y" effects) at that same layer
//! **after** counters, in timestamp order. Layers 3–5 (text, type, color) remain deferred
//! behind this same function signature, so callers never change as they are filled in.
//!
//! **Layer 1 — the copiable values — is where the seed comes from.** A copy effect
//! (CR 613.2a, CR 707) does not overwrite a characteristic; it changes which printed face
//! everything else is computed *from*, so it is one call at the top of [`characteristics`]
//! ([`copiable_printed`](crate::copy)) and no stage of its own. That is what puts it ahead
//! of every layer below by construction rather than by ordering, and it is why a copy's
//! counters, damage, and control are untouched by the copying (CR 707.2: they are not
//! copiable values).
//!
//! **Layer 2 — control — is [`controller_of`], not [`characteristics`].** Control is
//! not a characteristic (CR 109.3) and has no place in the [`Characteristics`] value,
//! but it is a continuous effect in a layer, it is ordered by the same CR 613.7
//! timestamps, and every later layer is read against its answer. Keeping it a separate,
//! non-recursive function is what lets the layer-6 and layer-7c selectors ask "does this
//! permanent's controller match?" from inside the very computation they are part of.
//!
//! **A rule modification is in no layer at all**, and is read by its own question in
//! [`rules_modifying`] — [`assigns_combat_damage_by`], [`attacks_as_though_no_defender`].
//! Control is kept out of [`Characteristics`] because CR 109.3 says it is not one; these
//! are kept out because being invisible to every other reader *is* the effect. A creature
//! that assigns combat damage by its toughness has exactly the power it had, and a
//! creature attacking as though it had no defender still has defender — including for the
//! ability that granted the permission.
mod continuous;
mod layer_seven;
mod layer_six;
mod layer_two;
mod rules_modifying;

use continuous::*;
use layer_seven::*;
use layer_six::*;
pub(crate) use layer_six::{current_abilities, stored_abilities};
pub use layer_two::{controller_of, controller_of_id};
pub use rules_modifying::{
    assigns_combat_damage_by, attacks_as_though_no_defender, does_not_untap,
};

use crate::ability::{Ability, StaticAffects, StaticCondition};
use crate::card::{
    abilities_of_permanent, stored_abilities_of_permanent, CardDatabase, CombatRestriction, Keyword,
};
use crate::card_type::{CardType, Supertype};
use crate::id::{PermanentId, PlayerId};
use crate::state::{
    CounterKind, Duration, EffectAffects, GameState, Modification, Permanent, StaticEffect,
};

/// A permanent's *current* characteristics, computed fresh — **never stored on
/// state**.
///
/// This is the value [`characteristics`] returns: what a permanent's types,
/// mana cost, power/toughness, and abilities are *right now*, after the layer
/// system. It is a snapshot produced on demand, not a field on
/// [`GameState`](crate::GameState); recomputing it every query is what keeps the
/// engine pure and undo/replay/resync free (ADR 0005).
///
/// Its power/toughness are the printed values with any `+1/+1` / `-1/-1`
/// counters folded in and then any applicable static `+X/+Y` modifiers applied
/// (both layer 7c); the remaining fields still equal the printed
/// [`CardData`](crate::CardData). As further continuous-effect layers land, the
/// same type carries their results without changing shape.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Characteristics {
    /// Current supertypes (e.g. [`Supertype::Basic`], [`Supertype::Legendary`]).
    pub supertypes: Vec<Supertype>,
    /// Current card types (e.g. [`CardType::Creature`]). A permanent normally has
    /// at least one; empty only in the unknown-id fallback (see
    /// [`characteristics`]).
    pub types: Vec<CardType>,
    /// Current subtypes (e.g. `"Elf"`, `"Forest"`); open-ended, so strings.
    pub subtypes: Vec<String>,
    /// Current mana cost in curly-brace notation (e.g. `"{2}{G}"`); empty for a
    /// permanent with no mana cost, such as a basic land.
    pub mana_cost: String,
    /// Current colours (CR 105.2), printed unioned with any a CR 613 **layer 5** effect
    /// added. Empty for a colourless permanent.
    ///
    /// A colour is *added*, never removed: every printed card in this catalog says "in
    /// addition to its other colors", and the exclusion list names the replacing form.
    pub colors: Vec<crate::mana::Color>,
    /// Current power, for creatures; `None` for non-creatures.
    pub power: Option<i32>,
    /// Current toughness, for creatures; `None` for non-creatures.
    pub toughness: Option<i32>,
    /// Printed **starting** loyalty, for planeswalkers; `None` otherwise (CR 306.5b).
    ///
    /// A planeswalker's loyalty *characteristic* is the number printed in its corner —
    /// what it enters the battlefield with — and is **not** how much loyalty it has
    /// right now. That is the count of its loyalty counters
    /// ([`Permanent::counter_count`](crate::Permanent::counter_count) of
    /// [`CounterKind::Loyalty`]), which every rule that spends, removes, or checks
    /// loyalty reads. The distinction is the same one power/toughness make between the
    /// printed seed and the computed value, except that nothing modifies this one: no
    /// layer changes printed loyalty, so it is carried through unchanged.
    pub loyalty: Option<u32>,
    /// The permanent's current ability set at CR 613 **layer 6**: the data-driven and
    /// scripted sources [`abilities_of`] unions, plus every ability granted to it — by an
    /// Aura, by an Equipment, or by a spell — and minus everything a loses-all took away
    /// before the grant's timestamp ([`current_abilities`]).
    pub abilities: Vec<Ability>,
    /// The permanent's *current* keyword abilities (CR 702): its printed
    /// [`CardData::keywords`](crate::CardData::keywords) unioned with any granted by
    /// continuous effects at CR 613 **layer 6** (CR 613.1f) — an attached Aura's
    /// grant, an anthem, or an until-end-of-turn pump — and with any **removed** by
    /// one taken back out. A granted keyword is indistinguishable from a printed one,
    /// and duplicates are collapsed (a keyword granted twice, or granted atop a printed
    /// one, appears once).
    ///
    /// The fold runs in timestamp order, so `loses defender` and `gains defender` are
    /// settled by which of the two spoke last — the whole of CR 613.1f.
    pub keywords: Vec<Keyword>,
    /// The permanent's *current* combat restrictions (CR 506.3, CR 509.1b): its printed
    /// [`CardData::restrictions`](crate::CardData::restrictions) unioned with any imposed by
    /// continuous effects at CR 613 **layer 6** (CR 613.1f) — an attached Aura's
    /// "can neither attack nor block", an until-end-of-turn "can't be blocked".
    /// Collapsed the same way [`Self::keywords`] are, so a restriction imposed twice
    /// binds once, and read through the same single path so a granted restriction is
    /// indistinguishable from a printed one.
    pub restrictions: Vec<CombatRestriction>,
}

/// Compute the *current* [`Characteristics`] of the permanent identified by
/// `permanent`, reading its printed [`CardData`](crate::CardData) as the seed.
///
/// This is the one pure read path mandated by ADR 0005: it runs fresh on every
/// call and caches nothing. In this slice the result is the printed values with
/// the permanent's `+1/+1` / `-1/-1` counters folded into power/toughness at CR
/// 613 layer 7c, then any static `+X/+Y` modifiers in force applied at that same
/// layer after the counters, in timestamp order. It takes `&CardDatabase` for
/// the same reason
/// [`apply_action`](crate::apply_action) does (ADR 0003): the printed seed lives
/// in the database, which is kept out of [`GameState`](crate::GameState) to
/// preserve that type's `Eq`/purity.
///
/// # Fallback
/// Returns [`Characteristics::default`] — an empty characteristics with no
/// types, no mana cost, and `None` power/toughness — when `permanent` is not on
/// the battlefield, or when its card is absent from `db`. Both are unknown-id
/// cases with no answer to compute; the engine forbids panicking APIs, so the
/// empty value is surfaced rather than panicked on.
#[must_use]
pub fn characteristics(
    state: &GameState,
    permanent: PermanentId,
    db: &CardDatabase,
) -> Characteristics {
    let Some(perm) = state.battlefield.iter().find(|p| p.id == permanent) else {
        return Characteristics::default();
    };
    // CR 613 layer 1 (CR 613.2a): copy effects are applied first, and they are applied by
    // choosing a different printed seed. Everything below — the counters at 7c, the
    // grants at 6, the anthems at 7c — then runs on the copied values, which is what
    // makes a copied 2/2 with two `+1/+1` counters a 4/4 rather than the copying card's
    // printed 0/0 plus two.
    let Some(face) = crate::copy::copiable_printed(state, perm).face(db) else {
        return Characteristics::default();
    };
    // CR 613 layer 7c: `+1/+1` and `-1/-1` counters adjust power and toughness
    // by the same signed amount. They only apply to a permanent that has P/T; a
    // permanent with no printed power/toughness (`None`) stays `None`. Counters are
    // **not** a copiable value (CR 707.2), so they are the copying permanent's own and are
    // folded onto the copied power here, at layer 7c, well after layer 1.
    let counter_delta = pt_counter_delta(perm);
    // CR 613 **layer 4**: types and subtypes a continuous effect adds, folded before
    // anything below asks what this permanent *is*. That ordering is the whole content of
    // the layer: an artifact animated into a creature is inside an anthem's class at 7c,
    // can be declared as an attacker, and dies to a creature sweeper.
    //
    // Gathered from the stored effects and the attachments alone — the same two sources
    // layer 6 bottoms out on — because the third, a printed static ability, is collected
    // by reading each source permanent's abilities, and asking for those from inside this
    // computation would not terminate.
    let (added_types, added_subtypes, added_colors) = added_types(state, perm, db);
    let mut types = face.types().to_vec();
    for card_type in added_types {
        if !types.contains(&card_type) {
            types.push(card_type);
        }
    }
    let mut subtypes = face.subtypes().to_vec();
    for subtype in added_subtypes {
        if !subtypes.contains(&subtype) {
            subtypes.push(subtype);
        }
    }
    let mut colors = face.colors().to_vec();
    for color in added_colors {
        if !colors.contains(&color) {
            colors.push(color);
        }
    }
    // CR 613 layer 7c (after counters, ADR 0005 §3): static `+X/+Y` modifiers in
    // force apply in timestamp order. `is_creature` gates anthem-style selectors, and it
    // is the **current** type — layer 4 above has already run, which is what puts an
    // animated artifact inside an anthem's class.
    let is_creature = types.contains(&CardType::Creature);
    let (static_power, static_toughness) = static_pt_delta(state, perm, is_creature, db);
    // CR 613 layer 6, the non-keyword half: the accessor answers an empty list for a
    // permanent that has lost all its abilities. Computed before layer 7 because 7a
    // reads it — a defining ability that layer 6 removed does not define anything.
    let abilities = abilities_of_permanent(state, db, perm);
    // CR 613 layer 7a (CR 604.3): a characteristic-defining ability replaces the printed
    // power outright, ahead of the counters and modifiers below, which then apply to
    // *its* answer. Absent on all but a handful of cards, and then the printed seed
    // stands.
    let base_power = defined_power(state, perm, &abilities, db);
    // CR 613 **layer 7b**: a base power and toughness a continuous effect *sets*, after
    // the characteristic-defining abilities of 7a and before the counters and modifiers
    // of 7c. It also gives P/T to a permanent that printed none, which is the whole point
    // on an animated artifact — without it, "becomes a creature" would make something
    // that attacks for nothing.
    let set_base = set_base_pt(state, perm, db);
    let seed_power = set_base
        .map(|(power, _)| power)
        .or(base_power)
        .or(face.power());
    let seed_toughness = set_base
        .map(|(_, toughness)| toughness)
        .or(face.toughness());
    Characteristics {
        supertypes: face.supertypes().to_vec(),
        types,
        subtypes,
        mana_cost: face.mana_cost().to_string(),
        colors,
        power: seed_power.map(|base| {
            base.saturating_add(counter_delta)
                .saturating_add(static_power)
        }),
        toughness: seed_toughness.map(|base| {
            base.saturating_add(counter_delta)
                .saturating_add(static_toughness)
        }),
        // Printed starting loyalty (CR 306.5b), carried through untouched: no layer
        // modifies it, and *current* loyalty is the counter count, not this.
        loyalty: face.loyalty(),
        abilities,
        // CR 613 layer 6 (CR 613.1f): the printed keywords unioned with any granted
        // continuously. Seeded from the printed set so a granted keyword sits beside
        // the printed ones and is read the same way everywhere.
        keywords: current_keywords(state, perm, is_creature, face.keywords().to_vec(), db),
        // CR 613 layer 6 as well: the non-keyworded half of the same layer.
        restrictions: current_restrictions(
            state,
            perm,
            is_creature,
            face.restrictions().to_vec(),
            db,
        ),
    }
}

/// Whether the permanent identified by `permanent` currently has keyword `keyword`
/// (CR 702) — its printed keywords unioned with any granted at CR 613 layer 6
/// (CR 613.1f). This is the single read path combat, evasion, and combat-damage use,
/// so a granted keyword is indistinguishable from a printed one. Reads fresh through
/// [`characteristics`], caching nothing (ADR 0005); a permanent not on the
/// battlefield has no keywords.
#[must_use]
pub(crate) fn permanent_has_keyword(
    state: &GameState,
    permanent: PermanentId,
    keyword: Keyword,
    db: &CardDatabase,
) -> bool {
    characteristics(state, permanent, db)
        .keywords
        .contains(&keyword)
}

/// The permanent identified by `permanent`'s *current* combat restrictions (CR 506.3,
/// CR 509.1b) — its printed set unioned with any imposed at CR 613 layer 6.
///
/// The restriction counterpart of [`permanent_has_keyword`], and the single read path
/// every declaration gate uses. It returns the whole list rather than answering one
/// membership question because a parameterized restriction
/// ([`CombatRestriction::CantBeBlockedBy`]) is asked "which colour?", not "is this exact
/// value present?". Empty for a permanent that is not on the battlefield.
#[must_use]
pub(crate) fn permanent_restrictions(
    state: &GameState,
    permanent: PermanentId,
    db: &CardDatabase,
) -> Vec<CombatRestriction> {
    characteristics(state, permanent, db).restrictions
}

/// Whether the permanent identified by `permanent` currently has the exact combat
/// restriction `restriction`. The convenience over [`permanent_restrictions`] for the
/// unit-shaped restrictions, which *are* answered by a membership test.
#[must_use]
pub(crate) fn permanent_has_restriction(
    state: &GameState,
    permanent: PermanentId,
    restriction: CombatRestriction,
    db: &CardDatabase,
) -> bool {
    permanent_restrictions(state, permanent, db).contains(&restriction)
}

#[cfg(test)]
mod tests;
