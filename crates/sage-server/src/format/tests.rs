//! Format registry and deck legality, per format.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::test_support::fixture;

/// The bundled database. Forest is its only basic land.
fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

/// The five non-basic cards these deck tests build with.
const NON_BASICS: [&str; 5] = [
    "onakke_ogre",
    "snapping_drake",
    "fire_elemental",
    "giant_spider",
    "walking_corpse",
];

/// A legal 40-card starter deck: four copies each of the five non-basics plus
/// twenty basic Forests.
fn legal_deck() -> Vec<CardId> {
    let mut deck = Vec::new();
    for slug in NON_BASICS {
        for _ in 0..4 {
            deck.push(fixture(slug));
        }
    }
    for _ in 0..20 {
        deck.push(fixture("forest"));
    }
    deck
}

#[test]
fn registry_resolves_seeded_ids_and_rejects_unknown() {
    let registry = FormatRegistry::with_defaults();
    assert!(registry.get("starter-1v1").is_some());
    assert!(registry.get("standard_2p").is_some());
    // The ids the web client's create form actually sends must resolve, or
    // `create_room` would reject every real room (regression guard for the
    // real-server e2e).
    assert!(registry.get("1v1").is_some());
    assert!(registry.get("ffa-4").is_some());
    assert!(registry.get("no-such-format").is_none());
}

#[test]
fn issue_349_ffa_format_seats_three_to_four_and_duels_seat_two() {
    let registry = FormatRegistry::with_defaults();
    // The free-for-all format seats 3–4 players; a duel format seats exactly two.
    let ffa = registry
        .get("standard_ffa")
        .expect("standard_ffa is registered");
    assert_eq!(ffa.seats, 3..=4);
    assert!(!ffa.seats.contains(&2) && ffa.seats.contains(&3) && ffa.seats.contains(&4));
    // The FFA format imposes no deck rules (permissive, like the open default).
    assert_eq!(ffa.deck_rules, Format::open().deck_rules);
    // The seeded competitive starter is a 1v1 duel.
    assert_eq!(registry.get("starter-1v1").unwrap().seats, 2..=2);
}

#[test]
fn seeded_format_yields_a_game_setup_with_its_parameters() {
    let format = FormatRegistry::with_defaults()
        .get("starter-1v1")
        .unwrap()
        .clone();
    let setup = format.game_setup(vec![PlayerSetup::new(legal_deck())], 7);
    assert_eq!(setup.starting_life, sage_engine::DEFAULT_STARTING_LIFE);
    assert_eq!(
        setup.starting_hand_size,
        sage_engine::DEFAULT_STARTING_HAND_SIZE
    );
    assert_eq!(setup.rng_seed, 7);
    assert_eq!(setup.players.len(), 1);
}

#[test]
fn a_legal_deck_including_many_basics_is_accepted() {
    // Twenty basic Forests far exceed the four-copy limit, yet are exempt.
    let format = Format::starter();
    assert_eq!(format.validate_deck(&legal_deck(), None, &db()), Ok(()));
}

#[test]
fn a_deck_under_the_minimum_size_is_rejected() {
    let format = Format::starter();
    let small = vec![fixture("forest"); 39];
    assert_eq!(
        format.validate_deck(&small, None, &db()),
        Err(DeckError::BelowMinimum { have: 39, min: 40 }),
    );
}

#[test]
fn over_the_copy_limit_for_a_non_basic_is_rejected() {
    // Five copies of one non-basic with an otherwise legal 40-card deck.
    let mut deck = vec![fixture("onakke_ogre"); 5];
    for slug in &NON_BASICS[1..] {
        for _ in 0..4 {
            deck.push(fixture(slug));
        }
    }
    for _ in 0..19 {
        deck.push(fixture("forest"));
    }
    assert_eq!(deck.len(), 40);
    assert_eq!(
        Format::starter().validate_deck(&deck, None, &db()),
        Err(DeckError::CopyLimit {
            card: fixture("onakke_ogre"),
            count: 5,
            limit: 4,
        }),
    );
}

#[test]
fn basics_are_only_exempt_when_the_rule_says_so() {
    // Same twenty Forests, but a format that does not exempt basics rejects them.
    let strict = Format {
        starting_life: 20,
        starting_hand_size: 7,
        seats: 2..=2,
        deck_rules: DeckRules {
            min_size: 40,
            max_size: None,
            max_copies: 4,
            basic_land_exempt: false,
            require_commander: false,
            enforce_color_identity: false,
        },
    };
    assert_eq!(
        strict.validate_deck(&legal_deck(), None, &db()),
        Err(DeckError::CopyLimit {
            card: fixture("forest"),
            count: 20,
            limit: 4,
        }),
    );
}

#[test]
fn a_deck_over_the_maximum_size_is_rejected() {
    let capped = Format {
        starting_life: 20,
        starting_hand_size: 7,
        seats: 2..=2,
        deck_rules: DeckRules {
            min_size: 40,
            max_size: Some(60),
            max_copies: 4,
            basic_land_exempt: true,
            require_commander: false,
            enforce_color_identity: false,
        },
    };
    let big = vec![fixture("forest"); 61];
    assert_eq!(
        capped.validate_deck(&big, None, &db()),
        Err(DeckError::AboveMaximum { have: 61, max: 60 }),
    );
}

// ----------------------------------------------------------------------
// Commander format (issue #372): 100-card singleton, color identity, 40 life.
// ----------------------------------------------------------------------

/// A legal 100-card mono-green commander deck for the bundled catalog: Jedit
/// Ojanen (a green legendary creature) as the commander, the catalog's unique
/// green (and colorless) non-basics, and Forests to fill to 100. Every card is
/// within Lathliss's red color identity, and every non-basic is a singleton — so
/// this is the acceptance-path deck the rejection tests each perturb one way.
fn commander_deck() -> Vec<CardId> {
    // Lathliss (the commander) plus the catalog's other in-identity non-basics:
    // mono-red cards and the colorless Skyscanner (empty identity ⊆ red). Each
    // appears exactly once (singleton, CR 903.5b).
    let non_basics = [
        "lathliss_dragon_queen",
        "volcanic_dragon",
        "viashino_pyromancer",
        "siegebreaker_giant",
        "lightning_strike",
        "shock",
        "sure_strike",
        "skyscanner",
    ];
    let mut deck: Vec<CardId> = non_basics.iter().map(|slug| fixture(slug)).collect();
    // Fill to exactly 100 with basic Mountains (singleton-exempt, in-identity).
    while deck.len() < 100 {
        deck.push(fixture("mountain"));
    }
    assert_eq!(deck.len(), 100);
    deck
}

/// The commander (Lathliss, Dragon Queen) of [`commander_deck`].
fn commander() -> CardId {
    fixture("lathliss_dragon_queen")
}

#[test]
fn issue_372_a_legal_commander_deck_is_accepted() {
    // The acceptance path: exactly 100 cards, singleton non-basics, a legendary
    // creature commander, every card within its color identity.
    assert_eq!(
        Format::commander().validate_deck(&commander_deck(), Some(commander()), &db()),
        Ok(())
    );
}

#[test]
fn issue_372_commander_format_starts_at_forty_life_and_seats_two_to_four() {
    let commander = FormatRegistry::with_defaults()
        .get("commander")
        .expect("commander format is registered")
        .clone();
    assert_eq!(commander.starting_life, COMMANDER_STARTING_LIFE);
    assert_eq!(commander.starting_life, 40);
    assert_eq!(commander.seats, 2..=4);
    // The engine `GameSetup` it builds carries the 40-life total.
    let setup = commander.game_setup(vec![PlayerSetup::new(commander_deck())], 1);
    assert_eq!(setup.starting_life, 40);
}

#[test]
fn issue_372_a_missing_commander_is_rejected() {
    assert_eq!(
        Format::commander().validate_deck(&commander_deck(), None, &db()),
        Err(DeckError::MissingCommander)
    );
}

#[test]
fn issue_372_a_non_legendary_creature_commander_is_rejected() {
    // Volcanic Dragon is a red creature but not legendary, so it cannot be the
    // commander (CR 903.5a). It is already one of the deck's cards.
    let not_legendary = fixture("volcanic_dragon");
    assert_eq!(
        Format::commander().validate_deck(&commander_deck(), Some(not_legendary), &db()),
        Err(DeckError::CommanderNotLegendaryCreature {
            card: not_legendary
        })
    );
}

#[test]
fn issue_372_a_commander_not_in_the_deck_is_rejected() {
    // Designate a legendary creature the deck does not contain. Build a 100-card
    // deck of Mountains only (so the designation, not size, is what is wrong).
    let deck = vec![fixture("mountain"); 100];
    assert_eq!(
        Format::commander().validate_deck(&deck, Some(commander()), &db()),
        Err(DeckError::CommanderNotInDeck { card: commander() })
    );
}

#[test]
fn issue_372_a_duplicate_non_basic_is_rejected() {
    // Two copies of a non-basic breaks the singleton limit (CR 903.5b). Drop one
    // Mountain and add a second Volcanic Dragon so the deck is still 100 cards.
    let mut deck = commander_deck();
    let land_pos = deck
        .iter()
        .rposition(|&c| c == fixture("mountain"))
        .expect("deck has mountains");
    deck[land_pos] = fixture("volcanic_dragon");
    assert_eq!(deck.len(), 100);
    assert_eq!(
        Format::commander().validate_deck(&deck, Some(commander()), &db()),
        Err(DeckError::CopyLimit {
            card: fixture("volcanic_dragon"),
            count: 2,
            limit: 1,
        })
    );
}

#[test]
fn issue_372_an_out_of_identity_card_is_rejected() {
    // Swap a Mountain for a blue card (Snapping Drake): its blue color identity is
    // not contained in the commander's red identity (CR 903.4). Deck stays 100.
    let mut deck = commander_deck();
    let land_pos = deck
        .iter()
        .rposition(|&c| c == fixture("mountain"))
        .expect("deck has mountains");
    deck[land_pos] = fixture("snapping_drake");
    assert_eq!(deck.len(), 100);
    assert_eq!(
        Format::commander().validate_deck(&deck, Some(commander()), &db()),
        Err(DeckError::OutOfIdentity {
            card: fixture("snapping_drake"),
        })
    );
}

#[test]
fn issue_372_a_wrong_size_commander_deck_is_rejected() {
    // 99 cards is below the exact-100 requirement (a closed size range).
    let mut deck = commander_deck();
    deck.pop();
    assert_eq!(deck.len(), 99);
    assert_eq!(
        Format::commander().validate_deck(&deck, Some(commander()), &db()),
        Err(DeckError::BelowMinimum { have: 99, min: 100 })
    );
    // 101 cards is above it.
    let mut over = commander_deck();
    over.push(fixture("forest"));
    assert_eq!(
        Format::commander().validate_deck(&over, Some(commander()), &db()),
        Err(DeckError::AboveMaximum {
            have: 101,
            max: 100
        })
    );
}

#[test]
fn issue_372_a_forest_is_green_identity_and_colorless_is_identityless() {
    // Color identity is computed from structured data: a basic Forest is green
    // (its intrinsic `{T}: Add {G}` ability, CR 903.4), while a colorless artifact
    // that taps for {C} has empty identity (colorless is not a color, CR 105.1).
    let database = db();
    let forest = color_identity(&database, fixture("forest"));
    assert!(forest.contains(&Color::Green) && forest.len() == 1);
    let skyscanner = color_identity(&database, fixture("skyscanner"));
    assert!(skyscanner.is_empty());
    // Jedit Ojanen is green (its colored mana-cost pips).
    assert_eq!(
        color_identity(&database, commander()),
        HashSet::from([Color::Red])
    );
}

#[test]
fn issue_372_lathliss_is_a_legendary_creature() {
    // The commander eligibility predicate reads structured supertype + type.
    assert!(is_legendary_creature(&db(), commander()));
    // A non-legendary creature and a legendary-less card are both ineligible.
    assert!(!is_legendary_creature(&db(), fixture("volcanic_dragon")));
    assert!(!is_legendary_creature(&db(), fixture("forest")));
}

// ----------------------------------------------------------------------
// Structured rejection reasons reaching the wire (issue #395).
// ----------------------------------------------------------------------

/// The display name the bundled database gives `slug`, used to assert a rejection
/// reason names the offending card by name rather than a raw interned id.
fn name_of(slug: &str) -> String {
    db().card(fixture(slug)).expect("bundled card").name.clone()
}

#[test]
fn issue_395_size_rejection_names_no_card_and_carries_a_code() {
    // A below-minimum rejection is not about any one card: the wire reason has the
    // stable class code and the human sentence, and no `card`.
    let rejection = DeckError::BelowMinimum { have: 39, min: 40 }.to_rejection(&db());
    assert_eq!(rejection.code, "below_minimum");
    assert_eq!(rejection.card, None);
    assert_eq!(
        rejection.reason,
        "deck has 39 cards, below the 40-card minimum"
    );

    let over = DeckError::AboveMaximum {
        have: 101,
        max: 100,
    }
    .to_rejection(&db());
    assert_eq!(over.code, "above_maximum");
    assert_eq!(over.card, None);
}

#[test]
fn issue_395_copy_limit_rejection_names_the_offending_card_by_name_and_identity() {
    // The wire reason names the card by its display name (never the raw CardId),
    // and `card` carries its stable functional_id — both drawn from the sender's
    // own submission.
    let rejection = DeckError::CopyLimit {
        card: fixture("onakke_ogre"),
        count: 5,
        limit: 4,
    }
    .to_rejection(&db());
    assert_eq!(rejection.code, "copy_limit");
    assert_eq!(rejection.card.as_deref(), Some("onakke_ogre"));
    assert_eq!(
        rejection.reason,
        format!(
            "{} appears 5 times, above the 4-copy limit",
            name_of("onakke_ogre")
        )
    );
    // Never the internal interned id.
    assert!(!rejection
        .reason
        .contains(&format!("card {}", fixture("onakke_ogre").0)));
}

#[test]
fn issue_395_commander_rejections_carry_class_codes_and_the_named_card() {
    let db = db();
    // Missing commander: a required designation, about no card.
    let missing = DeckError::MissingCommander.to_rejection(&db);
    assert_eq!(missing.code, "missing_commander");
    assert_eq!(missing.card, None);

    // Not a legendary creature: names the illegal designation (Llanowar Elves).
    let not_legendary = DeckError::CommanderNotLegendaryCreature {
        card: fixture("llanowar_elves"),
    }
    .to_rejection(&db);
    assert_eq!(not_legendary.code, "commander_not_legendary_creature");
    assert_eq!(not_legendary.card.as_deref(), Some("llanowar_elves"));
    assert!(not_legendary.reason.contains(&name_of("llanowar_elves")));

    // Not in the deck: names the designated commander (Jedit Ojanen).
    let not_in_deck = DeckError::CommanderNotInDeck {
        card: fixture("lathliss_dragon_queen"),
    }
    .to_rejection(&db);
    assert_eq!(not_in_deck.code, "commander_not_in_deck");
    assert_eq!(not_in_deck.card.as_deref(), Some("lathliss_dragon_queen"));

    // Out of identity: names the offending deck card (Snapping Drake).
    let out = DeckError::OutOfIdentity {
        card: fixture("snapping_drake"),
    }
    .to_rejection(&db);
    assert_eq!(out.code, "out_of_identity");
    assert_eq!(out.card.as_deref(), Some("snapping_drake"));
    assert!(out.reason.contains(&name_of("snapping_drake")));
}

#[test]
fn issue_395_display_for_logs_is_unchanged_and_uses_the_raw_id() {
    // The `Display` impl (used only for server logs) still labels a card by its raw
    // CardId, so the wire naming refactor did not disturb the log wording.
    let display = DeckError::CopyLimit {
        card: fixture("onakke_ogre"),
        count: 5,
        limit: 4,
    }
    .to_string();
    assert_eq!(
        display,
        format!(
            "card {} appears 5 times, above the 4-copy limit",
            fixture("onakke_ogre").0
        )
    );
    assert_eq!(
        DeckError::BelowMinimum { have: 39, min: 40 }.to_string(),
        "deck has 39 cards, below the 40-card minimum"
    );
}

#[test]
fn issue_372_existing_formats_keep_default_life_and_no_commander_rules() {
    // Regression guard: the non-commander formats are unchanged — 20 life, no
    // commander requirement, no color-identity enforcement.
    for id in ["starter-1v1", "standard_2p", "1v1", "standard_ffa", "ffa-4"] {
        let format = FormatRegistry::with_defaults().get(id).unwrap().clone();
        assert_eq!(format.starting_life, sage_engine::DEFAULT_STARTING_LIFE);
        assert!(!format.deck_rules.require_commander);
        assert!(!format.deck_rules.enforce_color_identity);
    }
}
