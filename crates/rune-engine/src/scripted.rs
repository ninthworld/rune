//! The escape hatch: bespoke abilities for cards the JSON IR cannot express.
//!
//! A [`CardId`] maps to code-defined [`Ability`] values through a pure `match`.
//! Nothing is stored in [`crate::GameState`] — abilities are re-derived on demand
//! (the same discipline as the layer system), so there are no trait objects or
//! closures on the immutable state and its `Clone`/`Eq` semantics are preserved.
//!
//! The table is empty today: every bundled card is fully data-expressed. The seam
//! exists so a future card whose behavior the closed [`Effect`](crate::ability::Effect)
//! vocabulary can't capture has a home without weakening the engine's purity.
//! See `docs/decisions/0007-card-effect-ir-hybrid.md`.

use crate::ability::Ability;
use crate::id::CardId;

/// Code-defined abilities for a card, or an empty list if it has none.
///
/// Unioned with the card's data-driven abilities by
/// [`crate::card::abilities_of`].
#[must_use]
pub(crate) fn scripted_abilities(card: CardId) -> Vec<Ability> {
    // No card needs bespoke abilities yet. When one does, match on `card`:
    //     CardId(999) => vec![/* hand-built Ability values */],
    let _ = card;
    Vec::new()
}
