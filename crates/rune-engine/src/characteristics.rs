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
//! This module is **slice 1 of 3** (ADR 0010 §3): it seeds current
//! characteristics straight from printed values, with no modifiers. Counters
//! (layer 7c) and static P/T modifications land in later slices behind this same
//! function signature, so callers never change as layers are filled in.

use crate::ability::Ability;
use crate::card::{abilities_of, CardDatabase};
use crate::card_type::{CardType, Supertype};
use crate::id::PermanentId;
use crate::state::GameState;

/// A permanent's *current* characteristics, computed fresh — **never stored on
/// state**.
///
/// This is the value [`characteristics`] returns: what a permanent's types,
/// mana cost, power/toughness, and abilities are *right now*, after the layer
/// system. It is a snapshot produced on demand, not a field on
/// [`GameState`](crate::GameState); recomputing it every query is what keeps the
/// engine pure and undo/replay/resync free (ADR 0010).
///
/// In this first slice the values equal the permanent's printed
/// [`CardData`](crate::CardData). As continuous-effect layers land, the same
/// type carries their results without changing shape.
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
/// call and caches nothing. In this slice the result is exactly the printed
/// values — no counters and no modifiers, which arrive in later slices behind
/// this unchanged signature. It takes `&CardDatabase` for the same reason
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
    Characteristics {
        supertypes: card.supertypes.clone(),
        types: card.types.clone(),
        subtypes: card.subtypes.clone(),
        mana_cost: card.mana_cost.clone(),
        power: card.power,
        toughness: card.toughness,
        abilities: abilities_of(db, perm.card),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::ability::is_mana_ability;
    use crate::id::{CardId, CardInstanceId, PlayerId};
    use crate::state::Permanent;

    /// Put a permanent for `card` on the battlefield and return its id.
    fn place(state: &mut GameState, card: CardId) -> PermanentId {
        let id = PermanentId(state.mint_id());
        state.battlefield.push(Permanent {
            id,
            instance: CardInstanceId(0),
            card,
            controller: PlayerId(0),
            tapped: false,
        });
        id
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
