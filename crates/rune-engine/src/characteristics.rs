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
use crate::card::{abilities_of, CardDatabase};
use crate::card_type::{CardType, Supertype};
use crate::id::PermanentId;
use crate::state::{CounterKind, EffectAffects, GameState, Modification, Permanent, StaticEffect};

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
    let (static_power, static_toughness) = static_pt_delta(state, perm, is_creature);
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
    }
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
fn static_pt_delta(state: &GameState, perm: &Permanent, is_creature: bool) -> (i32, i32) {
    let mut power = 0_i32;
    let mut toughness = 0_i32;
    for effect in ordered_pt_modifiers(state, perm, is_creature) {
        let Modification::PowerToughness {
            power: dp,
            toughness: dt,
        } = effect.modification;
        power = power.saturating_add(dp);
        toughness = toughness.saturating_add(dt);
    }
    (power, toughness)
}

/// The layer-7c static P/T effects that apply to `perm`, sorted by timestamp
/// (ascending [`StaticEffect::timestamp`], i.e. source object id).
///
/// Isolating the selection and ordering here keeps the "timestamp order"
/// guarantee explicit and directly testable (ADR 0010 §4). Object ids are
/// unique, so timestamps do not tie; the sort is stable regardless.
fn ordered_pt_modifiers<'a>(
    state: &'a GameState,
    perm: &Permanent,
    is_creature: bool,
) -> Vec<&'a StaticEffect> {
    let mut effects: Vec<&StaticEffect> = state
        .static_effects
        .iter()
        .filter(|effect| affects(effect, perm, is_creature))
        .collect();
    effects.sort_by_key(|effect| effect.timestamp());
    effects
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
            attacking: false,
            blocking: None,
            damage: 0,
            counters: BTreeMap::new(),
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
        // A "+3/+3 until end of turn" pump aimed at one Boar makes it a 6/5 and
        // leaves a second, unpumped Boar at its printed 3/2 (the effect is keyed
        // to a specific permanent id, not a controller-wide selector).
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        let pumped = place(&mut state, CardId(1));
        let bystander = place(&mut state, CardId(1));
        add_pump(&mut state, 100, pumped, 3, 3);

        let ch = characteristics(&state, pumped, &db);
        assert_eq!(ch.power, Some(6));
        assert_eq!(ch.toughness, Some(5));
        let other = characteristics(&state, bystander, &db);
        assert_eq!(other.power, Some(3));
        assert_eq!(other.toughness, Some(2));
    }

    #[test]
    fn issue_150_two_pumps_on_one_target_stack_in_timestamp_order() {
        // Two pumps on the same Boar sum (they are additive) and fold in ascending
        // timestamp order (CR 613.7) regardless of insertion order — printed 3/2 +
        // (+2/+0) + (+1/+2) = 6/4.
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        let boar = place(&mut state, CardId(1));
        add_pump(&mut state, 200, boar, 1, 2); // later timestamp, inserted first
        add_pump(&mut state, 100, boar, 2, 0);

        let ch = characteristics(&state, boar, &db);
        assert_eq!(ch.power, Some(6));
        assert_eq!(ch.toughness, Some(4));

        let perm = state.battlefield.iter().find(|p| p.id == boar).unwrap();
        let ordered: Vec<u64> = ordered_pt_modifiers(&state, perm, true)
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
        let forest = place(&mut state, CardId(5));
        add_pump(&mut state, 100, forest, 3, 3);

        let ch = characteristics(&state, forest, &db);
        assert_eq!(ch.power, None);
        assert_eq!(ch.toughness, None);
    }

    #[test]
    fn vanilla_creature_current_pt_and_types_equal_printed() {
        // Thornback Boar (CardId 1): a 3/2 Creature — Boar with no modifiers, so
        // its current characteristics are exactly its printed ones.
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        let boar = place(&mut state, CardId(1));

        let ch = characteristics(&state, boar, &db);
        let printed = db.card(CardId(1)).unwrap();
        assert_eq!(ch.power, Some(3));
        assert_eq!(ch.toughness, Some(2));
        assert_eq!(ch.types, vec![CardType::Creature]);
        assert_eq!(ch.subtypes, vec!["Boar".to_string()]);
        assert_eq!(ch.mana_cost, "{2}{G}");
        // Every field mirrors the printed seed in this slice.
        assert_eq!(ch.supertypes, printed.supertypes);
        assert_eq!(ch.power, printed.power);
        assert_eq!(ch.toughness, printed.toughness);
        assert_eq!(ch.types, printed.types);
    }

    #[test]
    fn plus_one_counters_add_to_printed_power_and_toughness() {
        // Thornback Boar is a printed 3/2. Three +1/+1 counters make it a 6/5.
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        let boar = place(&mut state, CardId(1));
        set_counters(&mut state, boar, CounterKind::PlusOnePlusOne, 3);

        let ch = characteristics(&state, boar, &db);
        assert_eq!(ch.power, Some(6));
        assert_eq!(ch.toughness, Some(5));
        // Only P/T shifts; the printed types are untouched by counters.
        assert_eq!(ch.types, vec![CardType::Creature]);
    }

    #[test]
    fn mixed_plus_and_minus_counters_net_correctly() {
        // 3/2 Boar with two +1/+1 and one -1/-1 nets +1/+1 overall -> 4/3.
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        let boar = place(&mut state, CardId(1));
        set_counters(&mut state, boar, CounterKind::PlusOnePlusOne, 2);
        set_counters(&mut state, boar, CounterKind::MinusOneMinusOne, 1);

        let ch = characteristics(&state, boar, &db);
        assert_eq!(ch.power, Some(4));
        assert_eq!(ch.toughness, Some(3));
    }

    #[test]
    fn minus_counters_can_drive_power_and_toughness_negative() {
        // Counters are folded verbatim; SBAs (a 0-or-less-toughness creature
        // dying, annihilation of +1/+1 vs -1/-1) are not this slice's concern,
        // so three -1/-1 on a 3/2 computes a raw 0/-1.
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        let boar = place(&mut state, CardId(1));
        set_counters(&mut state, boar, CounterKind::MinusOneMinusOne, 3);

        let ch = characteristics(&state, boar, &db);
        assert_eq!(ch.power, Some(0));
        assert_eq!(ch.toughness, Some(-1));
    }

    #[test]
    fn counters_on_a_permanent_without_pt_leave_it_without_pt() {
        // A Forest has no printed P/T; a stray +1/+1 counter does not conjure any
        // (layer 7c only adjusts an existing power/toughness).
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        let forest = place(&mut state, CardId(5));
        set_counters(&mut state, forest, CounterKind::PlusOnePlusOne, 2);

        let ch = characteristics(&state, forest, &db);
        assert_eq!(ch.power, None);
        assert_eq!(ch.toughness, None);
    }

    #[test]
    fn counter_count_defaults_to_zero_and_reports_stored_counts() {
        let mut state = GameState::new_two_player();
        let boar = place(&mut state, CardId(1));
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
        // Forest (CardId 5): a Basic Land with a mana ability and no P/T. Abilities
        // route through abilities_of, so the land's {T}: Add {G} is present.
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        let forest = place(&mut state, CardId(5));

        let ch = characteristics(&state, forest, &db);
        assert_eq!(ch.types, vec![CardType::Land]);
        assert_eq!(ch.supertypes, vec![Supertype::Basic]);
        assert_eq!(ch.power, None);
        assert_eq!(ch.toughness, None);
        assert_eq!(ch.mana_cost, "");
        assert_eq!(ch.abilities, abilities_of(&db, CardId(5)));
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
        // Thornback Boar is a printed 3/2. One +1/+1 counter and one static
        // +2/+2 anthem controlled by its controller compute 3+1+2 / 2+1+2 = 6/5,
        // exercising "printed + counters + modifier" together (ADR 0010 §3).
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        let boar = place(&mut state, CardId(1));
        set_counters(&mut state, boar, CounterKind::PlusOnePlusOne, 1);
        add_anthem(&mut state, 100, PlayerId(0), 2, 2);

        let ch = characteristics(&state, boar, &db);
        assert_eq!(ch.power, Some(6));
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
        let boar = place(&mut state, CardId(1));
        // Inserted later-timestamp first to prove the pipeline sorts, not reads
        // insertion order.
        add_anthem(&mut state, 200, PlayerId(0), 0, 3); // +0/+3
        add_anthem(&mut state, 100, PlayerId(0), 4, 0); // +4/+0

        // Printed 3/2 + (+4/+0) + (+0/+3) = 7/5.
        let ch = characteristics(&state, boar, &db);
        assert_eq!(ch.power, Some(7));
        assert_eq!(ch.toughness, Some(5));

        // The ordering is deterministic: ascending timestamp (source id), not the
        // Vec's insertion order.
        let perm = state.battlefield.iter().find(|p| p.id == boar).unwrap();
        let ordered: Vec<u64> = ordered_pt_modifiers(&state, perm, true)
            .iter()
            .map(|effect| effect.timestamp())
            .collect();
        assert_eq!(ordered, vec![100, 200]);
    }

    #[test]
    fn removing_the_source_reverts_the_computed_value() {
        // With the anthem in force the Boar is a 5/4; dropping the effect (its
        // source leaving) reverts to the printed 3/2 with nothing cached to stale.
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        let boar = place(&mut state, CardId(1));
        add_anthem(&mut state, 100, PlayerId(0), 2, 2);

        let boosted = characteristics(&state, boar, &db);
        assert_eq!(boosted.power, Some(5));
        assert_eq!(boosted.toughness, Some(4));

        state.static_effects.clear();
        let reverted = characteristics(&state, boar, &db);
        assert_eq!(reverted.power, Some(3));
        assert_eq!(reverted.toughness, Some(2));
    }

    #[test]
    fn anthem_only_affects_matching_controllers_creatures() {
        // An anthem for player 1 does not touch player 0's creature.
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        let boar = place(&mut state, CardId(1)); // controlled by player 0
        add_anthem(&mut state, 100, PlayerId(1), 5, 5);

        let ch = characteristics(&state, boar, &db);
        assert_eq!(ch.power, Some(3));
        assert_eq!(ch.toughness, Some(2));
    }

    #[test]
    fn anthem_does_not_grant_pt_to_a_noncreature() {
        // A Forest is not a creature, so a "creatures you control" anthem leaves
        // it without power/toughness (layer 7c only adjusts an existing P/T).
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        let forest = place(&mut state, CardId(5));
        add_anthem(&mut state, 100, PlayerId(0), 2, 2);

        let ch = characteristics(&state, forest, &db);
        assert_eq!(ch.power, None);
        assert_eq!(ch.toughness, None);
    }

    #[test]
    fn recomputes_fresh_and_never_mutates_state() {
        // Two calls agree and the state is untouched — the function is a pure query.
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        let boar = place(&mut state, CardId(1));
        let before = state.clone();

        let first = characteristics(&state, boar, &db);
        let second = characteristics(&state, boar, &db);
        assert_eq!(first, second);
        assert_eq!(state, before);
    }
}
