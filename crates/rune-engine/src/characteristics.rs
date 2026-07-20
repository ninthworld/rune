//! Computed characteristics: the single pure read path for a permanent's
//! *current* characteristics (CR 613, the layer system).
//!
//! A permanent's current power/toughness, types, and abilities are **not** what
//! is printed on its card — counters, anthems, pump spells, and type-changing
//! effects alter them continuously. Per ADR 0010 the engine never stores these:
//! [`characteristics`] recomputes them fresh on every call from the raw state
//! plus the printed [`CardData`](crate::CardData) seed, caching nothing
//! (consistent with the `GameState` "no cached derivations" invariant in
//! `state.rs`).
//!
//! This module is **slice 3 of 3** (ADR 0010 §3): it seeds current
//! characteristics from printed values, folds `+1/+1` and `-1/-1` counters into
//! power/toughness at CR 613 **layer 7c**, and then applies simple static P/T
//! modifications (anthem-style "+X/+Y" effects) at that same layer **after**
//! counters, in timestamp order. Layers 1–6 (copy, control, text, type, color,
//! ability-adding) remain deferred behind this same function signature, so
//! callers never change as they are filled in.

use crate::ability::Ability;
use crate::card::{abilities_of, CardDatabase, Keyword};
use crate::card_type::{CardType, Supertype};
use crate::id::PermanentId;
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
/// engine pure and undo/replay/resync free (ADR 0010).
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
    /// Current power, for creatures; `None` for non-creatures.
    pub power: Option<i32>,
    /// Current toughness, for creatures; `None` for non-creatures.
    pub toughness: Option<i32>,
    /// The permanent's current ability set, unioning data-driven and scripted
    /// sources via [`abilities_of`].
    pub abilities: Vec<Ability>,
    /// The permanent's *current* keyword abilities (CR 702): its printed
    /// [`CardData::keywords`](crate::CardData::keywords) unioned with any granted by
    /// continuous effects at CR 613 **layer 6** (CR 613.1f) — an attached Aura's
    /// grant, an anthem, or an until-end-of-turn pump. A granted keyword is
    /// indistinguishable from a printed one, and duplicates are collapsed (a keyword
    /// granted twice, or granted atop a printed one, appears once).
    pub keywords: Vec<Keyword>,
}

/// Compute the *current* [`Characteristics`] of the permanent identified by
/// `permanent`, reading its printed [`CardData`](crate::CardData) as the seed.
///
/// This is the one pure read path mandated by ADR 0010: it runs fresh on every
/// call and caches nothing. In this slice the result is the printed values with
/// the permanent's `+1/+1` / `-1/-1` counters folded into power/toughness at CR
/// 613 layer 7c, then any static `+X/+Y` modifiers in force applied at that same
/// layer after the counters, in timestamp order. It takes `&CardDatabase` for
/// the same reason
/// [`apply_action`](crate::apply_action) does (ADR 0007): the printed seed lives
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
    let Some(card) = db.card(perm.card) else {
        return Characteristics::default();
    };
    // CR 613 layer 7c: `+1/+1` and `-1/-1` counters adjust power and toughness
    // by the same signed amount. They only apply to a permanent that has P/T; a
    // permanent with no printed power/toughness (`None`) stays `None`.
    let counter_delta = pt_counter_delta(perm);
    // CR 613 layer 7c (after counters, ADR 0010 §3): static `+X/+Y` modifiers in
    // force apply in timestamp order. `is_creature` gates anthem-style selectors;
    // current type equals printed type until the type layers (1–6) land.
    let is_creature = card.types.contains(&CardType::Creature);
    let (static_power, static_toughness) = static_pt_delta(state, perm, is_creature, db);
    Characteristics {
        supertypes: card.supertypes.clone(),
        types: card.types.clone(),
        subtypes: card.subtypes.clone(),
        mana_cost: card.mana_cost.clone(),
        power: card
            .power
            .map(|p| p.saturating_add(counter_delta).saturating_add(static_power)),
        toughness: card.toughness.map(|t| {
            t.saturating_add(counter_delta)
                .saturating_add(static_toughness)
        }),
        abilities: abilities_of(db, perm.card),
        // CR 613 layer 6 (CR 613.1f): the printed keywords unioned with any granted
        // continuously. Seeded from the printed set so a granted keyword sits beside
        // the printed ones and is read the same way everywhere.
        keywords: current_keywords(state, perm, is_creature, card.keywords.clone(), db),
    }
}

/// The permanent's *current* keyword set at CR 613 **layer 6** (CR 613.1f):
/// `printed` plus every keyword granted to `perm` by a continuous effect, with
/// duplicates collapsed so a redundant grant is idempotent (CR 702, "having a
/// keyword ability twice is the same as having it once").
///
/// Two sources feed the grants, mirroring [`ordered_pt_modifiers`] (ADR 0010 §4):
/// the stored [`GameState::static_effects`] carrying [`Modification::GrantKeyword`]
/// (anthems and until-end-of-turn pumps) and, synthesized fresh, each Aura attached
/// to `perm` whose [`AuraGrant`](crate::AuraGrant) lists keywords (CR 303.4 /
/// 613.1f). Layer 6 grants are timestamp-independent for a pure keyword grant, so —
/// unlike the layer-7c P/T folds — no ordering is imposed. `is_creature` gates the
/// anthem-style "creatures you control" selector.
fn current_keywords(
    state: &GameState,
    perm: &Permanent,
    is_creature: bool,
    printed: Vec<Keyword>,
    db: &CardDatabase,
) -> Vec<Keyword> {
    let mut keywords = printed;
    let mut add = |keyword: Keyword| {
        if !keywords.contains(&keyword) {
            keywords.push(keyword);
        }
    };
    // Stored continuous grants (anthems, pumps) that apply to this permanent.
    for effect in &state.static_effects {
        if let Modification::GrantKeyword(keyword) = effect.modification {
            if affects(effect, perm, is_creature) {
                add(keyword);
            }
        }
    }
    // CR 303.4 / 613.1f: each Aura attached to `perm` grants its listed keywords
    // while attached. Derived from the attachment, never stored, so it vanishes the
    // instant the Aura leaves (ADR 0010).
    for aura in &state.battlefield {
        if aura.attached_to == Some(perm.id) {
            if let Some(grant) = db.card(aura.card).and_then(|c| c.aura.as_ref()) {
                for &keyword in &grant.keywords {
                    add(keyword);
                }
            }
        }
    }
    keywords
}

/// Whether the permanent identified by `permanent` currently has keyword `keyword`
/// (CR 702) — its printed keywords unioned with any granted at CR 613 layer 6
/// (CR 613.1f). This is the single read path combat, evasion, and combat-damage use,
/// so a granted keyword is indistinguishable from a printed one. Reads fresh through
/// [`characteristics`], caching nothing (ADR 0010); a permanent not on the
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

/// The net power/toughness shift from `perm`'s `+1/+1` and `-1/-1` counters at
/// CR 613 layer 7c: one `+1/+1` counter contributes `+1`, one `-1/-1` counter
/// `-1`, and the kinds sum independently (they do not annihilate here — that is
/// the `+1/+1`/`-1/-1` state-based action, out of this slice's scope).
///
/// Counts are `u32`; conversion saturates at [`i32::MAX`] rather than panic
/// (the engine forbids panicking APIs), which no realistic game ever reaches.
fn pt_counter_delta(perm: &Permanent) -> i32 {
    let plus = i32::try_from(perm.counter_count(CounterKind::PlusOnePlusOne)).unwrap_or(i32::MAX);
    let minus =
        i32::try_from(perm.counter_count(CounterKind::MinusOneMinusOne)).unwrap_or(i32::MAX);
    plus.saturating_sub(minus)
}

/// The net layer-7c power/toughness shift on `perm` from continuous static
/// effects (anthems, pumps), applied **after** counters in timestamp order
/// (CR 613.7, ADR 0010 §3–§4). Returns `(power_delta, toughness_delta)`.
///
/// These modifiers are additive, so their sum is order-independent
/// arithmetically; the engine still folds them in ascending timestamp order so
/// the pipeline is deterministic and stays correct as order-sensitive effects
/// (set P/T, characteristic-defining abilities) land in later slices.
/// `is_creature` gates the anthem-style "creatures you control" selector.
///
/// Overflow saturates rather than panicking, matching
/// [`pt_counter_delta`] and the engine's no-panic rule.
fn static_pt_delta(
    state: &GameState,
    perm: &Permanent,
    is_creature: bool,
    db: &CardDatabase,
) -> (i32, i32) {
    let mut power = 0_i32;
    let mut toughness = 0_i32;
    for effect in ordered_pt_modifiers(state, perm, is_creature, db) {
        // Only layer-7c P/T modifications adjust power/toughness; a layer-6
        // keyword grant that also happens to affect `perm` is skipped here.
        if let Modification::PowerToughness {
            power: dp,
            toughness: dt,
        } = effect.modification
        {
            power = power.saturating_add(dp);
            toughness = toughness.saturating_add(dt);
        }
    }
    (power, toughness)
}

/// The layer-7c static P/T effects that apply to `perm`, sorted by timestamp
/// (ascending [`StaticEffect::timestamp`], i.e. source object id).
///
/// Two sources feed this one list, folded through the same timestamp-ordered path
/// (ADR 0010 §4): the stored [`GameState::static_effects`] (anthems and pumps) and,
/// synthesized fresh, each Aura currently attached to `perm` (CR 303.4 / 613.7c) —
/// see [`aura_pt_effect`]. The Aura contributions are **derived, never stored**: an
/// Aura's P/T grant follows its attachment, so it appears here exactly while the
/// Aura is attached and vanishes the instant it leaves, with nothing to prune
/// (unlike a keyed pump, which the SBA loop must clean up). Object ids are unique,
/// so timestamps do not tie; the sort is stable regardless.
fn ordered_pt_modifiers(
    state: &GameState,
    perm: &Permanent,
    is_creature: bool,
    db: &CardDatabase,
) -> Vec<StaticEffect> {
    let mut effects: Vec<StaticEffect> = state
        .static_effects
        .iter()
        .filter(|effect| affects(effect, perm, is_creature))
        .copied()
        .collect();
    // CR 303.4 / 613.7c: each Aura attached to `perm` contributes its static P/T
    // modifier, timestamped by the Aura's own object id (CR 613.7).
    for aura in &state.battlefield {
        if aura.attached_to == Some(perm.id) {
            if let Some(effect) = aura_pt_effect(aura, db) {
                if affects(&effect, perm, is_creature) {
                    effects.push(effect);
                }
            }
        }
    }
    effects.sort_by_key(StaticEffect::timestamp);
    effects
}

/// The layer-7c power/toughness [`StaticEffect`] a single attached Aura `aura`
/// contributes to its host (CR 303.4 / 613.7c), or `None` if `aura` is not an
/// attached Aura (no host, or its card carries no [`AuraGrant`](crate::AuraGrant)).
///
/// Synthesized on demand rather than stored (ADR 0010): its `source` is the Aura's
/// own object id — a strictly increasing, replayable timestamp (CR 613.7) — and it
/// is keyed to the specific host permanent, so it folds in exactly like a pump
/// keyed to that permanent, and disappears when the Aura leaves.
fn aura_pt_effect(aura: &Permanent, db: &CardDatabase) -> Option<StaticEffect> {
    let host = aura.attached_to?;
    let grant = db.card(aura.card)?.aura.as_ref()?;
    Some(StaticEffect {
        source: aura.id.0,
        affects: EffectAffects::SpecificPermanent(host),
        modification: Modification::PowerToughness {
            power: grant.power,
            toughness: grant.toughness,
        },
        duration: Duration::WhileOnBattlefield,
    })
}

/// Whether `effect` applies to `perm`, given whether `perm` is currently a
/// creature. Encodes the [`EffectAffects`] selector semantics in one place.
fn affects(effect: &StaticEffect, perm: &Permanent, is_creature: bool) -> bool {
    match effect.affects {
        EffectAffects::CreaturesControlledBy(player) => is_creature && perm.controller == player,
        // A pump targets one specific permanent by its battlefield identity
        // (CR 601.2c). Layer 7c only adjusts an existing power/toughness, so a
        // pump landed on a non-creature (which has none) is folded into `None`
        // and has no visible effect — no `is_creature` gate is needed here.
        EffectAffects::SpecificPermanent(id) => perm.id == id,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::ability::is_mana_ability;
    use crate::fixtures::fixture;
    use crate::id::{CardId, CardInstanceId, PlayerId};
    use crate::state::{Duration, Permanent};
    use std::collections::BTreeMap;

    /// Put a permanent for `card` on the battlefield and return its id.
    fn place(state: &mut GameState, card: CardId) -> PermanentId {
        let id = PermanentId(state.mint_id());
        state.battlefield.push(Permanent {
            id,
            instance: CardInstanceId(0),
            card,
            controller: PlayerId(0),
            tapped: false,
            entered_turn: 0,
            attacking: None,
            blocking: None,
            damage: 0,
            counters: BTreeMap::new(),
            attached_to: None,
        });
        id
    }

    /// Set the count of `kind` counters on the permanent identified by `id`.
    fn set_counters(state: &mut GameState, id: PermanentId, kind: CounterKind, count: u32) {
        let perm = state.battlefield.iter_mut().find(|p| p.id == id).unwrap();
        perm.counters.insert(kind, count);
    }

    /// Add a static anthem "+`power`/+`toughness` to creatures `controller`
    /// controls" from a source with the given `source` object id (its timestamp).
    fn add_anthem(
        state: &mut GameState,
        source: u64,
        controller: PlayerId,
        power: i32,
        toughness: i32,
    ) {
        state.static_effects.push(StaticEffect {
            source,
            affects: EffectAffects::CreaturesControlledBy(controller),
            modification: Modification::PowerToughness { power, toughness },
            duration: Duration::WhileOnBattlefield,
        });
    }

    /// Add an "until end of turn" pump of +`power`/+`toughness` aimed at the
    /// single permanent `target`, timestamped by `source` (its object id).
    fn add_pump(
        state: &mut GameState,
        source: u64,
        target: PermanentId,
        power: i32,
        toughness: i32,
    ) {
        state.static_effects.push(StaticEffect {
            source,
            affects: EffectAffects::SpecificPermanent(target),
            modification: Modification::PowerToughness { power, toughness },
            duration: Duration::UntilEndOfTurn,
        });
    }

    #[test]
    fn issue_150_pump_boosts_only_its_specific_target() {
        // A "+3/+3 until end of turn" pump aimed at one Ogre makes it a 7/5 and
        // leaves a second, unpumped Ogre at its printed 4/2 (the effect is keyed
        // to a specific permanent id, not a controller-wide selector).
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        let pumped = place(&mut state, fixture("onakke_ogre"));
        let bystander = place(&mut state, fixture("onakke_ogre"));
        add_pump(&mut state, 100, pumped, 3, 3);

        let ch = characteristics(&state, pumped, &db);
        assert_eq!(ch.power, Some(7));
        assert_eq!(ch.toughness, Some(5));
        let other = characteristics(&state, bystander, &db);
        assert_eq!(other.power, Some(4));
        assert_eq!(other.toughness, Some(2));
    }

    #[test]
    fn issue_150_two_pumps_on_one_target_stack_in_timestamp_order() {
        // Two pumps on the same Ogre sum (they are additive) and fold in ascending
        // timestamp order (CR 613.7) regardless of insertion order — printed 4/2 +
        // (+2/+0) + (+1/+2) = 7/4.
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        let boar = place(&mut state, fixture("onakke_ogre"));
        add_pump(&mut state, 200, boar, 1, 2); // later timestamp, inserted first
        add_pump(&mut state, 100, boar, 2, 0);

        let ch = characteristics(&state, boar, &db);
        assert_eq!(ch.power, Some(7));
        assert_eq!(ch.toughness, Some(4));

        let perm = state.battlefield.iter().find(|p| p.id == boar).unwrap();
        let ordered: Vec<u64> = ordered_pt_modifiers(&state, perm, true, &db)
            .iter()
            .map(|effect| effect.timestamp())
            .collect();
        assert_eq!(ordered, vec![100, 200]);
    }

    #[test]
    fn issue_150_pump_on_a_noncreature_has_no_visible_effect() {
        // Layer 7c only adjusts an existing power/toughness: a pump keyed to a
        // Forest (no printed P/T) leaves it without any.
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        let forest = place(&mut state, fixture("forest"));
        add_pump(&mut state, 100, forest, 3, 3);

        let ch = characteristics(&state, forest, &db);
        assert_eq!(ch.power, None);
        assert_eq!(ch.toughness, None);
    }

    #[test]
    fn vanilla_creature_current_pt_and_types_equal_printed() {
        // Onakke Ogre: a 4/2 Creature — Ogre Warrior with no modifiers, so its
        // current characteristics are exactly its printed ones.
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        let boar = place(&mut state, fixture("onakke_ogre"));

        let ch = characteristics(&state, boar, &db);
        let printed = db.card(fixture("onakke_ogre")).unwrap();
        assert_eq!(ch.power, Some(4));
        assert_eq!(ch.toughness, Some(2));
        assert_eq!(ch.types, vec![CardType::Creature]);
        assert_eq!(ch.subtypes, vec!["Ogre".to_string(), "Warrior".to_string()]);
        assert_eq!(ch.mana_cost, "{2}{R}");
        // Every field mirrors the printed seed in this slice.
        assert_eq!(ch.supertypes, printed.supertypes);
        assert_eq!(ch.power, printed.power);
        assert_eq!(ch.toughness, printed.toughness);
        assert_eq!(ch.types, printed.types);
    }

    #[test]
    fn plus_one_counters_add_to_printed_power_and_toughness() {
        // Onakke Ogre is a printed 4/2. Three +1/+1 counters make it a 7/5.
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        let boar = place(&mut state, fixture("onakke_ogre"));
        set_counters(&mut state, boar, CounterKind::PlusOnePlusOne, 3);

        let ch = characteristics(&state, boar, &db);
        assert_eq!(ch.power, Some(7));
        assert_eq!(ch.toughness, Some(5));
        // Only P/T shifts; the printed types are untouched by counters.
        assert_eq!(ch.types, vec![CardType::Creature]);
    }

    #[test]
    fn mixed_plus_and_minus_counters_net_correctly() {
        // 4/2 Ogre with two +1/+1 and one -1/-1 nets +1/+1 overall -> 5/3.
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        let boar = place(&mut state, fixture("onakke_ogre"));
        set_counters(&mut state, boar, CounterKind::PlusOnePlusOne, 2);
        set_counters(&mut state, boar, CounterKind::MinusOneMinusOne, 1);

        let ch = characteristics(&state, boar, &db);
        assert_eq!(ch.power, Some(5));
        assert_eq!(ch.toughness, Some(3));
    }

    #[test]
    fn minus_counters_can_drive_power_and_toughness_negative() {
        // Counters are folded verbatim; SBAs (a 0-or-less-toughness creature
        // dying, annihilation of +1/+1 vs -1/-1) are not this slice's concern,
        // so three -1/-1 on a 4/2 computes a raw 1/-1.
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        let boar = place(&mut state, fixture("onakke_ogre"));
        set_counters(&mut state, boar, CounterKind::MinusOneMinusOne, 3);

        let ch = characteristics(&state, boar, &db);
        assert_eq!(ch.power, Some(1));
        assert_eq!(ch.toughness, Some(-1));
    }

    #[test]
    fn counters_on_a_permanent_without_pt_leave_it_without_pt() {
        // A Forest has no printed P/T; a stray +1/+1 counter does not conjure any
        // (layer 7c only adjusts an existing power/toughness).
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        let forest = place(&mut state, fixture("forest"));
        set_counters(&mut state, forest, CounterKind::PlusOnePlusOne, 2);

        let ch = characteristics(&state, forest, &db);
        assert_eq!(ch.power, None);
        assert_eq!(ch.toughness, None);
    }

    #[test]
    fn counter_count_defaults_to_zero_and_reports_stored_counts() {
        let mut state = GameState::new_two_player();
        let boar = place(&mut state, fixture("onakke_ogre"));
        let find = |state: &GameState| {
            state
                .battlefield
                .iter()
                .find(|p| p.id == boar)
                .unwrap()
                .clone()
        };
        assert_eq!(find(&state).counter_count(CounterKind::PlusOnePlusOne), 0);
        set_counters(&mut state, boar, CounterKind::PlusOnePlusOne, 4);
        assert_eq!(find(&state).counter_count(CounterKind::PlusOnePlusOne), 4);
        assert_eq!(find(&state).counter_count(CounterKind::MinusOneMinusOne), 0);
    }

    #[test]
    fn basic_land_has_no_power_or_toughness_and_keeps_its_ability() {
        // Forest: a Basic Land with a mana ability and no P/T. Abilities
        // route through abilities_of, so the land's {T}: Add {G} is present.
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        let forest = place(&mut state, fixture("forest"));

        let ch = characteristics(&state, forest, &db);
        assert_eq!(ch.types, vec![CardType::Land]);
        assert_eq!(ch.supertypes, vec![Supertype::Basic]);
        assert_eq!(ch.power, None);
        assert_eq!(ch.toughness, None);
        assert_eq!(ch.mana_cost, "");
        assert_eq!(ch.abilities, abilities_of(&db, fixture("forest")));
        assert_eq!(ch.abilities.len(), 1);
        assert!(is_mana_ability(&ch.abilities[0]));
    }

    #[test]
    fn unknown_permanent_id_follows_the_default_fallback() {
        // No permanent with this id is on the battlefield.
        let db = CardDatabase::bundled().unwrap();
        let state = GameState::new_two_player();
        assert!(state.battlefield.is_empty());

        assert_eq!(
            characteristics(&state, PermanentId(42), &db),
            Characteristics::default()
        );
    }

    #[test]
    fn permanent_whose_card_is_absent_from_db_follows_the_default_fallback() {
        // The permanent exists on the battlefield, but its CardId is not in the db.
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        let ghost = place(&mut state, CardId(9999));
        assert!(db.card(CardId(9999)).is_none());

        assert_eq!(
            characteristics(&state, ghost, &db),
            Characteristics::default()
        );
    }

    #[test]
    fn single_static_modifier_stacks_on_printed_pt_and_counters() {
        // Onakke Ogre is a printed 4/2. One +1/+1 counter and one static
        // +2/+2 anthem controlled by its controller compute 4+1+2 / 2+1+2 = 7/5,
        // exercising "printed + counters + modifier" together (ADR 0010 §3).
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        let boar = place(&mut state, fixture("onakke_ogre"));
        set_counters(&mut state, boar, CounterKind::PlusOnePlusOne, 1);
        add_anthem(&mut state, 100, PlayerId(0), 2, 2);

        let ch = characteristics(&state, boar, &db);
        assert_eq!(ch.power, Some(7));
        assert_eq!(ch.toughness, Some(5));
        // Only P/T shifts; static modifiers never touch the printed types.
        assert_eq!(ch.types, vec![CardType::Creature]);
    }

    #[test]
    fn two_static_modifiers_apply_in_timestamp_order_and_sum() {
        // Two anthems whose sources were minted out of order in the state vector.
        // The result is their sum (they are additive), and the read path folds
        // them in ascending-timestamp order regardless of insertion order.
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        let boar = place(&mut state, fixture("onakke_ogre"));
        // Inserted later-timestamp first to prove the pipeline sorts, not reads
        // insertion order.
        add_anthem(&mut state, 200, PlayerId(0), 0, 3); // +0/+3
        add_anthem(&mut state, 100, PlayerId(0), 4, 0); // +4/+0

        // Printed 4/2 + (+4/+0) + (+0/+3) = 8/5.
        let ch = characteristics(&state, boar, &db);
        assert_eq!(ch.power, Some(8));
        assert_eq!(ch.toughness, Some(5));

        // The ordering is deterministic: ascending timestamp (source id), not the
        // Vec's insertion order.
        let perm = state.battlefield.iter().find(|p| p.id == boar).unwrap();
        let ordered: Vec<u64> = ordered_pt_modifiers(&state, perm, true, &db)
            .iter()
            .map(|effect| effect.timestamp())
            .collect();
        assert_eq!(ordered, vec![100, 200]);
    }

    #[test]
    fn removing_the_source_reverts_the_computed_value() {
        // With the anthem in force the Ogre is a 6/4; dropping the effect (its
        // source leaving) reverts to the printed 4/2 with nothing cached to stale.
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        let boar = place(&mut state, fixture("onakke_ogre"));
        add_anthem(&mut state, 100, PlayerId(0), 2, 2);

        let boosted = characteristics(&state, boar, &db);
        assert_eq!(boosted.power, Some(6));
        assert_eq!(boosted.toughness, Some(4));

        state.static_effects.clear();
        let reverted = characteristics(&state, boar, &db);
        assert_eq!(reverted.power, Some(4));
        assert_eq!(reverted.toughness, Some(2));
    }

    #[test]
    fn anthem_only_affects_matching_controllers_creatures() {
        // An anthem for player 1 does not touch player 0's creature.
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        let boar = place(&mut state, fixture("onakke_ogre")); // controlled by player 0
        add_anthem(&mut state, 100, PlayerId(1), 5, 5);

        let ch = characteristics(&state, boar, &db);
        assert_eq!(ch.power, Some(4));
        assert_eq!(ch.toughness, Some(2));
    }

    #[test]
    fn anthem_does_not_grant_pt_to_a_noncreature() {
        // A Forest is not a creature, so a "creatures you control" anthem leaves
        // it without power/toughness (layer 7c only adjusts an existing P/T).
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        let forest = place(&mut state, fixture("forest"));
        add_anthem(&mut state, 100, PlayerId(0), 2, 2);

        let ch = characteristics(&state, forest, &db);
        assert_eq!(ch.power, None);
        assert_eq!(ch.toughness, None);
    }

    /// Add a static "grant `keyword` to the single permanent `target`" continuous
    /// effect timestamped by `source`, with the given `duration`.
    fn add_keyword_grant(
        state: &mut GameState,
        source: u64,
        target: PermanentId,
        keyword: Keyword,
        duration: Duration,
    ) {
        state.static_effects.push(StaticEffect {
            source,
            affects: EffectAffects::SpecificPermanent(target),
            modification: Modification::GrantKeyword(keyword),
            duration,
        });
    }

    /// Attach the permanent `aura` to `host` (set its `attached_to`), the way a
    /// resolving Aura enters (CR 303.4d).
    fn attach(state: &mut GameState, aura: PermanentId, host: PermanentId) {
        let aura = state.battlefield.iter_mut().find(|p| p.id == aura).unwrap();
        aura.attached_to = Some(host);
    }

    #[test]
    fn issue_374_aura_grants_flying_folds_into_computed_keywords_cr_613_1f() {
        // CR 613.1f: an Aura granting flying puts flying into the host's computed
        // keyword set, indistinguishable from a printed keyword. A bystander creature
        // with no Aura has none.
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        let host = place(&mut state, fixture("onakke_ogre"));
        let bystander = place(&mut state, fixture("onakke_ogre"));
        let aura = place(&mut state, fixture("flight"));
        attach(&mut state, aura, host);

        assert!(characteristics(&state, host, &db)
            .keywords
            .contains(&Keyword::Flying));
        assert!(!characteristics(&state, bystander, &db)
            .keywords
            .contains(&Keyword::Flying));
    }

    #[test]
    fn issue_374_aura_grant_vanishes_when_the_aura_leaves() {
        // The grant is derived from the attachment (ADR 0010): detach the Aura and
        // the host's computed keyword set reverts with nothing to prune.
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        let host = place(&mut state, fixture("onakke_ogre"));
        let aura = place(&mut state, fixture("flight"));
        attach(&mut state, aura, host);
        assert!(characteristics(&state, host, &db)
            .keywords
            .contains(&Keyword::Flying));

        // The Aura leaves the battlefield entirely.
        state.battlefield.retain(|p| p.id != aura);
        assert!(!characteristics(&state, host, &db)
            .keywords
            .contains(&Keyword::Flying));
    }

    #[test]
    fn issue_374_specific_permanent_grant_folds_into_computed_keywords() {
        // A pump-style "target creature gains trample" grant keyed to one permanent
        // folds into that permanent's keyword set and no other's.
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        let pumped = place(&mut state, fixture("onakke_ogre"));
        let bystander = place(&mut state, fixture("onakke_ogre"));
        add_keyword_grant(
            &mut state,
            100,
            pumped,
            Keyword::Trample,
            Duration::UntilEndOfTurn,
        );

        assert!(characteristics(&state, pumped, &db)
            .keywords
            .contains(&Keyword::Trample));
        assert!(!characteristics(&state, bystander, &db)
            .keywords
            .contains(&Keyword::Trample));
    }

    #[test]
    fn issue_374_anthem_grant_affects_only_matching_controllers_creatures() {
        // A "creatures you control have vigilance" grant applies to a matching
        // controller's creature and not to an opponent's.
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        let mine = place(&mut state, fixture("onakke_ogre")); // controller PlayerId(0)
        state.static_effects.push(StaticEffect {
            source: 100,
            affects: EffectAffects::CreaturesControlledBy(PlayerId(0)),
            modification: Modification::GrantKeyword(Keyword::Vigilance),
            duration: Duration::WhileOnBattlefield,
        });
        assert!(characteristics(&state, mine, &db)
            .keywords
            .contains(&Keyword::Vigilance));

        // A creature the effect's controller does not control is untouched.
        state.static_effects[0].affects = EffectAffects::CreaturesControlledBy(PlayerId(1));
        assert!(!characteristics(&state, mine, &db)
            .keywords
            .contains(&Keyword::Vigilance));
    }

    #[test]
    fn issue_374_duplicate_keyword_grants_are_redundant_not_stacking() {
        // CR 702: having a keyword twice is the same as having it once. A printed
        // flier (Snapping Drake) also granted flying twice appears with flying
        // exactly once — the grants collapse.
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        let drake = place(&mut state, fixture("snapping_drake")); // printed flying
        add_keyword_grant(
            &mut state,
            100,
            drake,
            Keyword::Flying,
            Duration::UntilEndOfTurn,
        );
        add_keyword_grant(
            &mut state,
            200,
            drake,
            Keyword::Flying,
            Duration::WhileOnBattlefield,
        );

        let ch = characteristics(&state, drake, &db);
        assert_eq!(
            ch.keywords
                .iter()
                .filter(|&&kw| kw == Keyword::Flying)
                .count(),
            1,
            "flying is present once despite a printed copy and two grants"
        );
    }

    #[test]
    fn recomputes_fresh_and_never_mutates_state() {
        // Two calls agree and the state is untouched — the function is a pure query.
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        let boar = place(&mut state, fixture("onakke_ogre"));
        let before = state.clone();

        let first = characteristics(&state, boar, &db);
        let second = characteristics(&state, boar, &db);
        assert_eq!(first, second);
        assert_eq!(state, before);
    }
}
