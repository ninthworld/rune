//! Shared test utilities and fixtures.

#![cfg(test)]
#![allow(clippy::unwrap_used, clippy::panic)]

use super::card_data::CardData;
use super::database::CardDatabase;
use crate::id::{CardId, FunctionalId};

/// The number of functional definitions in `data/catalog/`.
pub(crate) const CATALOG_SIZE: usize = 61;

/// Every handle the bundled catalog interned: `CardId(0..n)` (ADR 0018 §3).
pub(crate) fn every_id() -> impl Iterator<Item = CardId> {
    (0..CATALOG_SIZE as u64).map(CardId)
}

/// Resolve a card by its **authored identity**, the way every caller should.
///
/// Tests name cards by `functional_id` rather than by handle because the handle is
/// interned at build time and shifts whenever the catalog changes — hard-coding one
/// would make an unrelated new card break this file (ADR 0018 §3).
pub(crate) fn id_of(db: &CardDatabase, slug: &str) -> CardId {
    let functional_id = FunctionalId::try_from(slug.to_string()).unwrap();
    db.card_id(&functional_id)
        .unwrap_or_else(|| panic!("{slug} is not in the bundled catalog"))
}

/// The bundled definition of the card authored under `slug`.
pub(crate) fn card_named<'a>(db: &'a CardDatabase, slug: &str) -> &'a CardData {
    db.card(id_of(db, slug)).unwrap()
}

#[test]
fn catalog_parsing_meets_its_startup_budget_at_catalog_scale() {
    use std::fmt::Write as _;
    use std::time::{Duration, Instant};

    // ADR 0018 §6: `CardDatabase::bundled()` must parse a 10,000-card catalog well
    // under 200ms on CI hardware. The bundled catalog is a few dozen cards, far too
    // small to measure that, so the budget is exercised against a synthetic catalog of the
    // size the target actually names.
    //
    // Measured when this landed: 53ms for 10,000 definitions in release, 206ms
    // unoptimized — so the §6 target holds with roughly 4x of headroom.
    //
    // What the assertion guards, though, is the loader's *shape*, not that number.
    // Tests run unoptimized and CI machines are noisy and shared, so the ceiling is
    // deliberately loose: an accidentally quadratic loader (a linear scan per insert
    // — easy to introduce, since interning and duplicate detection both want a
    // lookup) overshoots it by orders of magnitude at this size, while ordinary
    // machine-to-machine variance never comes close.
    const CARDS: usize = 10_000;
    const CEILING: Duration = Duration::from_secs(10);

    let mut json = String::from("[");
    for i in 0..CARDS {
        if i > 0 {
            json.push(',');
        }
        // Shaped like a real definition: a creature with a keyword and an ETB trigger.
        let _ = write!(
            json,
            r#"{{"schema_version":1,"functional_id":"synthetic_card_{i}",
                     "name":"Synthetic Card {i}","types":["creature"],"subtypes":["Spirit"],
                     "mana_cost":"{{2}}{{G}}","colors":["green"],"power":2,"toughness":2,
                     "keywords":["flying"],
                     "abilities":[{{"type":"triggered","event":"self_enters_battlefield",
                                   "effects":[{{"kind":"draw_card","count":1}}]}}]}}"#
        );
    }
    json.push(']');

    let start = Instant::now();
    let db = CardDatabase::from_json(&json).unwrap();
    let elapsed = start.elapsed();

    assert_eq!(db.len(), CARDS);
    assert!(
        elapsed < CEILING,
        "parsing {CARDS} definitions took {elapsed:?}, over the {CEILING:?} ceiling — \
         the loader is likely no longer linear in catalog size"
    );

    // And the catalog the engine actually ships stays trivial to load.
    let start = Instant::now();
    CardDatabase::bundled().unwrap();
    let bundled = start.elapsed();
    assert!(
        bundled < Duration::from_secs(1),
        "the {CATALOG_SIZE}-card bundled catalog took {bundled:?} to load"
    );
}

#[test]
fn issue_256_no_bundled_card_is_a_functionless_shell() {
    use crate::card::abilities_of;
    use crate::card_type::CardType;

    // A castable *spell* that resolves doing nothing renders as blank generated
    // rules text (ADR 0018 §7) — the exact failure mode issue #256 fixed. This guard
    // keeps every bundled card meaningful and fails on any future functionless one:
    //
    // - a land must have an ability (its mana ability);
    // - an instant, sorcery, or noncreature permanent (artifact/enchantment) must
    //   have at least one of spell_effects / abilities / aura;
    // - a creature is inherently functional — a body with power/toughness — so a
    //   vanilla creature is not a shell, and needs no IR to prove it.
    //
    // The `scripted` escape hatch (behavior in code) also counts as function.
    let db = CardDatabase::bundled().unwrap();
    for id in every_id() {
        let card = db.card(id).unwrap();
        let has_ir = !card.spell_effects.is_empty()
            || !abilities_of(&db, id).is_empty()
            || card.aura.is_some()
            || !card.keywords.is_empty()
            || card.scripted;

        if card.has_type(CardType::Land) {
            assert!(
                !abilities_of(&db, id).is_empty(),
                "the land {} has no ability",
                card.name
            );
        } else if !card.has_type(CardType::Creature) {
            assert!(
                has_ir,
                "{} is a functionless shell — no spell effect, ability, or aura",
                card.name
            );
        }
    }
}
