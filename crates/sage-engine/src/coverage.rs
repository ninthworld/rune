//! The deterministic test-coverage audit for the catalog (issue #774).
//!
//! A component test can pass for every effect a card is built from while the *card* is
//! not playable: the ordering is wrong, the second effect asks a question the first left
//! it unable to answer, or the whole composition was never driven end to end. Nothing in
//! the gate noticed, because nothing was looking at cards — it was looking at effects.
//!
//! This module renders the artifact that looks at cards. [`render_report`] turns the
//! interned catalog plus the text of the workspace's Rust sources into the Markdown
//! committed at `docs/generated/test-coverage.md`: which definitions no test so much as
//! names, split by the tier of test each one deserves.
//!
//! **It is a review aid, not a gate.** A card being unnamed is a question worth asking,
//! not a defect — and being *named* proves almost nothing, since a slug can appear in a
//! test that asserts something else entirely. The report says exactly that in its own
//! header, so nobody reads a green column as a guarantee. What *is* gated is drift: the
//! committed copy must match what the sources render, the same forcing function
//! [`crate::compat`] uses, so this cannot rot into the hand-maintained ledger that
//! preceded it (#252).
//!
//! **This does not weaken "zero I/O in the engine."** Rendering is a pure function of its
//! inputs: the caller reads the sources and hands them over. Only the generator binary
//! (`src/bin/gen-coverage.rs`) and the freshness test touch the filesystem, and neither
//! ships in the running engine.

use std::fmt::Write as _;

use crate::card::{CardData, CardDatabase};
use crate::id::CardId;

/// The directories whose Rust sources count as "a test names this card", relative to the
/// repository root.
///
/// The **engine** only, deliberately. A card named in the server's rules-text tests is
/// named by a test that asserts its *wording*, and a card named in the CLI's is named by
/// one that plays a whole game past it; neither is somebody driving that card's state
/// transition, which is the thing this report is asking about. Ten definitions change
/// column on that distinction, which is exactly why it is drawn here rather than left to
/// whoever reads the table.
///
/// Both the generator and the freshness test walk these, and they walk them separately —
/// each does its own I/O, because this crate does none. The list is here so the two
/// cannot disagree about what was scanned.
pub const SCANNED_ROOTS: &[&str] = &["crates/sage-engine/tests", "crates/sage-engine/src"];

/// What kind of test a definition deserves, decided by what the definition *is*.
///
/// The distinction matters because the two tiers fail differently. A card with abilities
/// has a state transition that can be wrong in ways no schema check sees, so it wants a
/// test that drives one. A vanilla creature has none: its printed characteristics are
/// the whole card, and a test that cast one would be exercising the casting pipeline
/// rather than the card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
    /// The card does something — an ability, a spell effect, an attachment, a cost, or a
    /// trait. It wants a test that drives the real pipeline and asserts a transition.
    Behavioral,
    /// The card is its printed characteristics and nothing else. It wants those
    /// characteristics asserted, and nothing more.
    Vanilla,
}

impl Tier {
    /// Which tier `data` falls in — read off the definition, never guessed from a name.
    #[must_use]
    pub fn of(data: &CardData) -> Self {
        let does_something = !data.abilities.is_empty()
            || !data.spell_effects.is_empty()
            || !data.keywords.is_empty()
            || !data.spell_traits.is_empty()
            || data.attachment.is_some()
            || data.additional_cost.is_some();
        if does_something {
            Self::Behavioral
        } else {
            Self::Vanilla
        }
    }

    /// The word the report uses for it.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Behavioral => "behavioral",
            Self::Vanilla => "vanilla",
        }
    }
}

/// Render the audit for `db` against `sources` — the full text of every scanned Rust
/// file, in any order.
///
/// A definition counts as *named* when its authored identity appears anywhere in that
/// text. That is a deliberately weak test, and the report says so: it is a cheap lower
/// bound on attention, not a measure of coverage. Nothing here reads the filesystem.
#[must_use]
pub fn render_report(db: &CardDatabase, sources: &[String]) -> String {
    let mut unnamed: Vec<(&str, &str, Tier)> = Vec::new();
    let mut named = 0usize;
    let mut total = 0usize;
    for index in 0..db.len() as u64 {
        let Some(data) = db.card(CardId(index)) else {
            continue;
        };
        total += 1;
        let slug = data.functional_id.as_str();
        if sources.iter().any(|text| text.contains(slug)) {
            named += 1;
        } else {
            unnamed.push((slug, data.name.as_str(), Tier::of(data)));
        }
    }
    // By identity, which is the order a reader looks one up in.
    unnamed.sort_unstable_by_key(|(slug, _, _)| *slug);

    let mut out = String::new();
    out.push_str("# Test coverage of the card catalog\n\n");
    out.push_str(
        "<!-- Generated by `make compat`. Do not edit by hand; edit the catalog or the tests. -->\n\n",
    );
    out.push_str(
        "A **review aid, not a gate** (issue #774). A card listed here is a question worth asking \
         before a release, not a defect. A card *not* listed here is not thereby covered: this \
         report asks only whether an engine source mentions the card's authored identity, which a \
         test asserting something else entirely also satisfies.\n\n",
    );
    out.push_str(
        "What it does catch is the failure that motivated it: a card composed entirely of \
         well-tested effects, shipped without anyone driving the composition. Every effect passed; \
         the card was never played.\n\n",
    );
    let _ = writeln!(
        out,
        "Scanned: {}.\n",
        SCANNED_ROOTS
            .iter()
            .map(|root| format!("`{root}`"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let _ = writeln!(out, "## Named by some test ({named} of {total})\n");
    if unnamed.is_empty() {
        out.push_str("Every definition in the catalog is named somewhere.\n");
        return out;
    }
    let _ = writeln!(
        out,
        "## Named by nothing ({})\n\n\
         The tier says which kind of test the definition would want: a **behavioral** card has a \
         state transition to drive, a **vanilla** one has only its printed characteristics to \
         assert.\n",
        unnamed.len()
    );
    out.push_str("| Identity | Card | Tier |\n| --- | --- | --- |\n");
    for (slug, name, tier) in unnamed {
        let _ = writeln!(out, "| `{slug}` | {name} | {} |", tier.word());
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn generation_is_deterministic() {
        let db = CardDatabase::bundled().unwrap();
        let sources = vec!["nothing at all".to_string()];
        assert_eq!(
            render_report(&db, &sources),
            render_report(&db, &sources),
            "the report must be byte-identical across runs"
        );
    }

    #[test]
    fn a_named_card_leaves_the_list_and_an_unnamed_one_stays() {
        let db = CardDatabase::bundled().unwrap();
        let empty = render_report(&db, &[]);
        assert!(empty.contains("| `forest` |"), "nothing named anything");

        let named = render_report(&db, &["forest".to_string()]);
        assert!(
            !named.contains("| `forest` |"),
            "naming it anywhere is enough to drop it from the list"
        );
    }

    #[test]
    fn the_tier_is_read_off_the_definition() {
        let db = CardDatabase::bundled().unwrap();
        let vanilla = db.card(
            db.card_id(&"onakke_ogre".to_string().try_into().unwrap())
                .unwrap(),
        );
        assert_eq!(Tier::of(vanilla.unwrap()), Tier::Vanilla);
        let behavioral = db.card(
            db.card_id(&"pendulum_of_patterns".to_string().try_into().unwrap())
                .unwrap(),
        );
        assert_eq!(Tier::of(behavioral.unwrap()), Tier::Behavioral);
    }
}
