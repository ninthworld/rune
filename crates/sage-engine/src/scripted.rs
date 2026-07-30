//! The escape hatch: bespoke abilities for cards the JSON IR cannot express, and
//! the hand-authored rules text that goes with them.
//!
//! A [`FunctionalId`] maps to code-defined [`Ability`] values through a pure `match`.
//! Nothing is stored in [`crate::GameState`] — abilities are re-derived on demand
//! (the same discipline as the layer system), so there are no trait objects or
//! closures on the immutable state and its `Clone`/`Eq` semantics are preserved.
//!
//! **Keyed by [`FunctionalId`], not [`CardId`](crate::CardId).** A `CardId` is interned
//! by `build.rs` as an index into the sorted catalog (ADR 0018 §3), so it shifts the
//! moment a card is added ahead of it alphabetically. A code arm written against an
//! integer handle would rot silently — adding an unrelated card would hand one card's
//! bespoke behavior to another. The authored identity is the only card identity stable
//! across builds, so it is the only sound key for a hand-written table. It also lets the
//! loader check the `scripted` flag without knowing anything about interning.
//!
//! Scripted behavior is opaque Rust, so the server's fallback-text formatter cannot
//! derive a card's rules text from it the way it does for the data IR (ADR 0018 §7).
//! A scripted card therefore supplies that text here, next to the behavior it
//! describes, via [`scripted_rules_text`] — and the catalog loader enforces that the
//! two authoring tiers agree in both directions: a definition declaring
//! `scripted: true` must have a code arm, and a card with a code arm must declare it
//! (`crate::card::CardDatabase::from_json`, ADR 0018 §5).
//!
//! This is the one catalog rule `build.rs` cannot enforce, because the answer lives in
//! compiled Rust that does not exist yet when the build script runs. The loader owns it
//! instead, so a mismatch fails `cargo test` and the server's startup — never a game
//! already in progress.
//!
//! The table is empty today: every bundled card is fully data-expressed. The seam
//! exists so a future card whose behavior the closed [`Effect`](crate::ability::Effect)
//! vocabulary can't capture has a home without weakening the engine's purity.
//! See `docs/decisions/0007-card-effect-ir-hybrid.md`.

use crate::ability::Ability;
use crate::id::FunctionalId;

/// A card that exists only for the tests of the scripted seam itself — the catalog
/// has no such card, so both directions of the loader's `scripted` validation can be
/// exercised while the real table is empty.
#[cfg(test)]
pub(crate) const TEST_SCRIPTED_CARD: &str = "bespoke_thing";

/// Code-defined abilities for a card, or an empty list if it has none.
///
/// Unioned with the card's data-driven abilities by
/// [`abilities_of`](crate::abilities_of).
#[must_use]
pub(crate) fn scripted_abilities(card: &FunctionalId) -> Vec<Ability> {
    // No card needs bespoke abilities yet. When one does, match on its identity:
    //     "chaos_orb" => vec![/* hand-built Ability values */],
    let _ = card;
    Vec::new()
}

/// The hand-authored rules text of a scripted card, or `None` if the card has no
/// code arm (ADR 0018 §7).
///
/// The parallel seam to [`scripted_abilities`]: the server generates a card's rules
/// text from its ability IR, which cannot describe behavior written in Rust, so a
/// scripted card states in words what its code does. This is authored *behavior*
/// documentation, not a card's printed prose — it is written to be semantically
/// complete for play, never to reproduce official wording, and no exact Oracle text
/// belongs here any more than it belongs in the catalog (`docs/brief.md` Legal
/// Considerations).
#[must_use]
pub fn scripted_rules_text(card: &FunctionalId) -> Option<&'static str> {
    // When a card needs a bespoke ability, its text is authored here beside it:
    //     "chaos_orb" => Some("Whenever this attacks, flip a coin. ..."),
    #[cfg(test)]
    if card.as_str() == TEST_SCRIPTED_CARD {
        return Some("Whenever this creature attacks, its controller draws a card.");
    }
    let _ = card;
    None
}

/// Whether `card` has a code arm in this module at all — a scripted ability, scripted
/// rules text, or both.
///
/// This is the predicate the catalog loader validates a definition's `scripted` flag
/// against, in both directions (ADR 0018 §5), so the data tier and the code tier
/// cannot silently disagree about which cards are scripted.
#[must_use]
pub(crate) fn is_scripted(card: &FunctionalId) -> bool {
    !scripted_abilities(card).is_empty() || scripted_rules_text(card).is_some()
}
