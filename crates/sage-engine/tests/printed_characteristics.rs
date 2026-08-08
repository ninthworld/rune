//! The bundled catalog against the card that was actually printed (issue #822).
//!
//! `build.rs` validates a definition's *shape* — the file stem matches the identity, a
//! creature carries both power and toughness, a planeswalker carries loyalty and nothing
//! else does. None of that can know a number is **wrong**, and the behavioural suites
//! assert against whatever the definition says, so a mis-transcribed mana cost is
//! invisible at every gate and then cemented by the test written over it. Fifteen cards
//! shipped that way (#819, #820); twelve of them differed in a field a machine can check.
//!
//! This is that machine. It reads a fixture transcribed **from the printed set** and
//! compares it to `CardDatabase::bundled()` — the same embedded, validated catalog the
//! game is played with, not the JSON on disk.
//!
//! ## What the fixture may hold, and why
//!
//! Name, mana cost, supertypes, types, subtypes, power, toughness, loyalty, colours: the
//! functional characteristics ADR 0009 already sources for the catalog itself, and
//! nothing else. No rules text, no flavour text, no artist, no image, no set symbol — the
//! project ships none of those (`AGENTS.md`, Legal Considerations), and a fixture is not a
//! loophole in that rule.
//!
//! Rules text is therefore **out of scope for this gate**, and the limit is real: of the
//! fifteen wrong cards, this catches twelve. A missing `{R}` in a cost list, a `gain_life`
//! the card does not print, a token created tapped — those stay a human-review problem,
//! and the tests in `m19_activation_costs.rs` and `m19_attacking_tokens.rs` are where they
//! are held.
//!
//! ## The one rule that keeps this honest
//!
//! `crates/sage-engine/tests/fixtures/printed_characteristics.json` is transcribed from
//! the printed set by `scripts/printed-characteristics.py`, which reads a third-party set
//! file and **never** reads `data/catalog/`. Regenerating it the other way round would
//! leave this file green forever while proving nothing, which is the failure mode to
//! design against rather than to discover.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use sage_engine::{Ability, CardData, CardDatabase, CardType, Color, Supertype};
use serde::Deserialize;

/// One printed **face**. Every field is optional in the source the way it is optional on
/// a card — a land has no mana cost, an instant has no power — and absent means the
/// catalog must not carry one either.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Face {
    name: String,
    mana_cost: String,
    types: Vec<CardType>,
    colors: Vec<Color>,
    #[serde(default)]
    supertypes: Vec<Supertype>,
    #[serde(default)]
    subtypes: Vec<String>,
    /// Kept as a **string**, because a printed power is not always a number: Enigma
    /// Drake's is `*`, which the catalog says as a base of 0 plus a `defined_power`
    /// ability, and a numeric field could not tell that from a card that really is a 0.
    #[serde(default)]
    power: Option<String>,
    #[serde(default)]
    toughness: Option<String>,
    #[serde(default)]
    loyalty: Option<String>,
}

/// One card as it was printed: its front face, the identity the catalog files it under,
/// and the second face when it has one (CR 712).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Printed {
    functional_id: String,
    #[serde(flatten)]
    front: Face,
    #[serde(default)]
    back_face: Option<Face>,
}

const FIXTURE: &str = include_str!("fixtures/printed_characteristics.json");

fn printed() -> Vec<Printed> {
    serde_json::from_str(FIXTURE).expect("the fixture parses")
}

fn catalog() -> BTreeMap<String, &'static CardData> {
    // Leaked once, so the map can borrow for the whole test: a `CardDatabase` owns its
    // definitions and every test here wants them by identity rather than by handle.
    let db: &'static CardDatabase = Box::leak(Box::new(
        CardDatabase::bundled().expect("the bundled catalog loads"),
    ));
    db.all()
        .into_iter()
        .map(|(_, data)| (data.functional_id.as_str().to_string(), data))
        .collect()
}

/// Whether `card` sets its own power from something it counts — the shape a printed `*`
/// takes in this catalog.
fn power_is_defined(card: &CardData) -> bool {
    card.abilities
        .iter()
        .any(|ability| matches!(ability, Ability::DefinedPower { .. }))
}

// ----- the two directions of the correspondence -----------------------------

#[test]
fn issue_822_every_catalog_card_has_a_printed_card_behind_it() {
    let fixture: Vec<String> = printed()
        .into_iter()
        .map(|card| card.functional_id)
        .collect();
    let catalog = catalog();
    let unbacked: Vec<&String> = catalog.keys().filter(|id| !fixture.contains(id)).collect();
    assert!(
        unbacked.is_empty(),
        "a definition with no printed card behind it is a card nobody can check: {unbacked:?}"
    );
}

#[test]
fn issue_822_every_printed_card_in_the_fixture_is_in_the_catalog() {
    let catalog = catalog();
    let orphans: Vec<String> = printed()
        .into_iter()
        .map(|card| card.functional_id)
        .filter(|id| !catalog.contains_key(id))
        .collect();
    assert!(
        orphans.is_empty(),
        "a fixture row naming no definition is a row nothing checks: {orphans:?}"
    );
}

// ----- the characteristics themselves ----------------------------------------

#[test]
fn issue_822_the_catalog_matches_the_printed_characteristics() {
    let catalog = catalog();
    let mut wrong: Vec<String> = Vec::new();

    for card in printed() {
        let Some(data) = catalog.get(&card.functional_id) else {
            continue; // reported by the correspondence test above
        };
        let mut note = |field: &str, found: String, want: String| {
            wrong.push(format!(
                "{}: {field} is {found}, printed {want}",
                card.functional_id
            ));
        };

        if data.name != card.front.name {
            note(
                "name",
                format!("{:?}", data.name),
                format!("{:?}", card.front.name),
            );
        }
        if data.mana_cost != card.front.mana_cost {
            note(
                "mana_cost",
                format!("{:?}", data.mana_cost),
                format!("{:?}", card.front.mana_cost),
            );
        }
        if data.types != card.front.types {
            note(
                "types",
                format!("{:?}", data.types),
                format!("{:?}", card.front.types),
            );
        }
        if data.supertypes != card.front.supertypes {
            note(
                "supertypes",
                format!("{:?}", data.supertypes),
                format!("{:?}", card.front.supertypes),
            );
        }
        if data.subtypes != card.front.subtypes {
            note(
                "subtypes",
                format!("{:?}", data.subtypes),
                format!("{:?}", card.front.subtypes),
            );
        }
        if data.colors != card.front.colors {
            note(
                "colors",
                format!("{:?}", data.colors),
                format!("{:?}", card.front.colors),
            );
        }

        // A `*` is not a number the catalog stores: the card says "equal to the number
        // of…", and the definition says that with an ability. What is checked is that
        // the ability is there — a base of 0 with nothing to define it is a 0/4 wearing
        // a `*`'s clothes.
        match card.front.power.as_deref() {
            Some("*") => {
                if !power_is_defined(data) {
                    note(
                        "power",
                        format!("{:?} with no defined_power ability", data.power),
                        "* (a defined power)".to_string(),
                    );
                }
            }
            other => {
                let found = data.power.map(|value| value.to_string());
                if found.as_deref() != other {
                    note("power", format!("{found:?}"), format!("{other:?}"));
                }
            }
        }
        let toughness = data.toughness.map(|value| value.to_string());
        if toughness.as_deref() != card.front.toughness.as_deref() {
            note(
                "toughness",
                format!("{toughness:?}"),
                format!("{:?}", card.front.toughness),
            );
        }
        let loyalty = data.loyalty.map(|value| value.to_string());
        if loyalty.as_deref() != card.front.loyalty.as_deref() {
            note(
                "loyalty",
                format!("{loyalty:?}"),
                format!("{:?}", card.front.loyalty),
            );
        }
    }

    assert!(
        wrong.is_empty(),
        "the catalog disagrees with the card that was printed:\n  {}",
        wrong.join("\n  ")
    );
}

#[test]
fn issue_822_a_second_face_matches_the_face_that_was_printed() {
    let catalog = catalog();
    let mut wrong: Vec<String> = Vec::new();

    for card in printed() {
        let Some(data) = catalog.get(&card.functional_id) else {
            continue;
        };
        match (&card.back_face, &data.back_face) {
            (None, None) => {}
            (Some(printed), Some(face)) => {
                if face.name != printed.name
                    || face.types != printed.types
                    || face.supertypes != printed.supertypes
                    || face.subtypes != printed.subtypes
                    || face.colors != printed.colors
                    || face.power.map(|value| value.to_string()).as_deref()
                        != printed.power.as_deref()
                    || face.toughness.map(|value| value.to_string()).as_deref()
                        != printed.toughness.as_deref()
                    || face.loyalty.map(|value| value.to_string()).as_deref()
                        != printed.loyalty.as_deref()
                {
                    wrong.push(format!("{}: back face is {face:?}", card.functional_id));
                }
            }
            (Some(_), None) => wrong.push(format!(
                "{}: the printed card transforms and the definition has no back face",
                card.functional_id
            )),
            (None, Some(_)) => wrong.push(format!(
                "{}: the definition has a back face and the printed card has none",
                card.functional_id
            )),
        }
    }

    assert!(
        wrong.is_empty(),
        "a second face disagrees with the face that was printed:\n  {}",
        wrong.join("\n  ")
    );
}

// ----- the licensing constraint, enforced rather than remembered -------------

#[test]
fn issue_822_the_fixture_carries_nothing_but_printed_characteristics() {
    // `deny_unknown_fields` on `Printed` already refuses a field this test does not know
    // about, so a future regeneration cannot smuggle rules text past it silently. This
    // states the allowed set outright as well, because the reason for the limit lives in
    // `AGENTS.md` rather than in serde, and a reviewer should not have to infer it.
    const ALLOWED: [&str; 11] = [
        "functional_id",
        "name",
        "mana_cost",
        "types",
        "colors",
        "supertypes",
        "subtypes",
        "power",
        "toughness",
        "loyalty",
        "back_face",
    ];
    let rows: Vec<BTreeMap<String, serde_json::Value>> =
        serde_json::from_str(FIXTURE).expect("the fixture parses");
    assert!(!rows.is_empty(), "the fixture is not empty");
    for row in &rows {
        for key in row.keys() {
            assert!(
                ALLOWED.contains(&key.as_str()),
                "{key:?} is not a printed characteristic — this project ships no rules \
                 text, flavour text, art, or branding, and a test fixture is not an \
                 exception (AGENTS.md)"
            );
        }
    }
}
