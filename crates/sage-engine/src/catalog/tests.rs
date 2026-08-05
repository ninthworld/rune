//! The validators, exercised on the definitions they accept and reject.

#![allow(clippy::unwrap_used, clippy::panic)]

use super::*;

/// A minimal valid definition, as parsed JSON, that each test then breaks in one way.
fn definition(extra: &str) -> serde_json::Value {
    let json = format!(
        r#"{{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
             "types": ["creature"], "mana_cost": "{{G}}", "power": 1, "toughness": 1{extra}}}"#
    );
    serde_json::from_str(&json).unwrap()
}

#[test]
fn a_well_formed_definition_validates_and_yields_its_identity() {
    let id = validate_definition(Some("test_card"), &definition("")).unwrap();
    assert_eq!(id, "test_card");
}

#[test]
fn a_definition_must_be_one_object_not_the_old_monolithic_array() {
    let array = serde_json::from_str(r#"[{"functional_id": "test_card"}]"#).unwrap();
    assert_eq!(
        validate_definition(Some("test_card"), &array).unwrap_err(),
        Violation::NotAnObject
    );
}

#[test]
fn an_unrecognized_schema_version_is_rejected() {
    let mut card = definition("");
    card["schema_version"] = serde_json::json!(SCHEMA_VERSION + 1);
    assert_eq!(
        validate_definition(Some("test_card"), &card).unwrap_err(),
        Violation::UnsupportedSchemaVersion {
            functional_id: "test_card".to_string(),
            found: u64::from(SCHEMA_VERSION) + 1,
        }
    );
}

#[test]
fn a_functional_id_that_does_not_match_its_file_name_is_rejected() {
    assert_eq!(
        validate_definition(Some("some_other_file"), &definition("")).unwrap_err(),
        Violation::FileNameMismatch {
            functional_id: "test_card".to_string(),
            file_stem: "some_other_file".to_string(),
        }
    );
}

#[test]
fn a_snapshot_with_no_file_behind_it_skips_the_file_name_check() {
    assert!(validate_definition(None, &definition("")).is_ok());
}

#[test]
fn an_ill_formed_slug_is_rejected() {
    for slug in [
        "Thornback_Boar",
        "thornback boar",
        "9lives",
        "trailing_",
        "double__bar",
    ] {
        let mut card = definition("");
        card["functional_id"] = serde_json::json!(slug);
        assert_eq!(
            validate_definition(None, &card).unwrap_err(),
            Violation::MalformedFunctionalId {
                slug: slug.to_string()
            },
            "expected `{slug}` to be rejected"
        );
    }
}

#[test]
fn well_formed_slugs_are_accepted() {
    for slug in ["forest", "onakke_ogre", "druid_of_the_cowl", "b2_bomber"] {
        assert!(
            is_well_formed_slug(slug),
            "expected `{slug}` to be accepted"
        );
    }
}

#[test]
fn a_creature_without_power_and_toughness_is_rejected() {
    let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                   "types": ["creature"], "mana_cost": "{G}"}"#;
    let card = serde_json::from_str(json).unwrap();
    assert_eq!(
        validate_definition(None, &card).unwrap_err(),
        Violation::PowerToughnessMismatch {
            functional_id: "test_card".to_string(),
            creature: true,
        }
    );
}

#[test]
fn issue_608_loyalty_is_required_on_a_planeswalker_and_forbidden_elsewhere() {
    // CR 306.5b, and both directions of it. A planeswalker with no starting loyalty
    // would enter with no counters and be put straight into its owner's graveyard
    // by CR 704.5i; a loyalty on anything else is a number nothing would read.
    let missing = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                      "supertypes": ["legendary"], "types": ["planeswalker"],
                      "mana_cost": "{2}{W}{W}"}"#;
    assert_eq!(
        validate_definition(None, &serde_json::from_str(missing).unwrap()).unwrap_err(),
        Violation::LoyaltyMismatch {
            functional_id: "test_card".to_string(),
            planeswalker: true,
        }
    );

    let spurious = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                       "types": ["creature"], "mana_cost": "{G}",
                       "power": 1, "toughness": 1, "loyalty": 3}"#;
    assert_eq!(
        validate_definition(None, &serde_json::from_str(spurious).unwrap()).unwrap_err(),
        Violation::LoyaltyMismatch {
            functional_id: "test_card".to_string(),
            planeswalker: false,
        }
    );

    // And the well-formed pair passes: a planeswalker with loyalty and no P/T.
    let good = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                   "supertypes": ["legendary"], "types": ["planeswalker"],
                   "mana_cost": "{2}{W}{W}", "loyalty": 4}"#;
    assert_eq!(
        validate_definition(None, &serde_json::from_str(good).unwrap()).unwrap(),
        "test_card"
    );
}

#[test]
fn issue_605_a_token_that_could_not_be_a_permanent_is_rejected() {
    // A token exists only on the battlefield (CR 111.7), so a token that is not a
    // permanent could never exist — the card would author an object with nowhere
    // to go.
    let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                   "types": ["sorcery"], "mana_cost": "{G}",
                   "spell_effects": [{"kind": "create_token",
                     "token": {"name": "Idea", "types": ["instant"]}}]}"#;
    let card = serde_json::from_str(json).unwrap();
    assert_eq!(
        validate_definition(None, &card).unwrap_err(),
        Violation::TokenIsNotAPermanent {
            functional_id: "test_card".to_string(),
        }
    );

    // Naming no types at all is the same failure.
    let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                   "types": ["sorcery"], "mana_cost": "{G}",
                   "spell_effects": [{"kind": "create_token",
                     "token": {"name": "Nothing", "types": []}}]}"#;
    let card = serde_json::from_str(json).unwrap();
    assert!(validate_definition(None, &card).is_err());
}

#[test]
fn issue_605_a_token_needs_power_and_toughness_exactly_when_it_is_a_creature() {
    // The token counterpart of the card rule, and wrong for the same reason.
    let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                   "types": ["sorcery"], "mana_cost": "{G}",
                   "spell_effects": [{"kind": "create_token",
                     "token": {"name": "Goblin", "types": ["creature"]}}]}"#;
    let card = serde_json::from_str(json).unwrap();
    assert_eq!(
        validate_definition(None, &card).unwrap_err(),
        Violation::TokenPowerToughnessMismatch {
            functional_id: "test_card".to_string(),
            creature: true,
        }
    );

    // A noncreature token carrying power/toughness is the other direction.
    let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                   "types": ["sorcery"], "mana_cost": "{G}",
                   "spell_effects": [{"kind": "create_token",
                     "token": {"name": "Treasure", "types": ["artifact"],
                               "power": 1, "toughness": 1}}]}"#;
    let card = serde_json::from_str(json).unwrap();
    assert!(validate_definition(None, &card).is_err());
}

#[test]
fn issue_605_a_token_nested_inside_an_optional_effect_is_still_validated() {
    // The walk is to any depth: a `create_token` inside a `may` is checked too,
    // so nesting is not a way around the rule.
    let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                   "types": ["sorcery"], "mana_cost": "{G}",
                   "spell_effects": [{"kind": "may", "effects": [
                     {"kind": "create_token",
                      "token": {"name": "Goblin", "types": ["creature"]}}]}]}"#;
    let card = serde_json::from_str(json).unwrap();
    assert!(matches!(
        validate_definition(None, &card).unwrap_err(),
        Violation::TokenPowerToughnessMismatch { .. }
    ));
}

#[test]
fn a_creature_with_only_half_a_power_toughness_is_rejected() {
    let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                   "types": ["creature"], "mana_cost": "{G}", "power": 2}"#;
    let card = serde_json::from_str(json).unwrap();
    assert!(validate_definition(None, &card).is_err());
}

#[test]
fn a_non_creature_carrying_power_and_toughness_is_rejected() {
    let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                   "types": ["instant"], "mana_cost": "{R}", "power": 1, "toughness": 1}"#;
    let card = serde_json::from_str(json).unwrap();
    assert_eq!(
        validate_definition(None, &card).unwrap_err(),
        Violation::PowerToughnessMismatch {
            functional_id: "test_card".to_string(),
            creature: false,
        }
    );
}

#[test]
fn an_additional_cost_that_could_never_be_paid_is_rejected() {
    // A land is played, not cast (CR 116.2a), so no cast gate would ever consult
    // its cost — the field would read as a rule and enforce nothing.
    let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                   "types": ["land"], "mana_cost": "",
                   "additional_cost": {"kind": "discard", "count": 1}}"#;
    let card = serde_json::from_str(json).unwrap();
    assert_eq!(
        validate_definition(None, &card).unwrap_err(),
        Violation::AdditionalCostIsUnpayable {
            functional_id: "test_card".to_string()
        }
    );

    // A cost of no cards is not a cost.
    let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                   "types": ["sorcery"], "mana_cost": "{R}",
                   "additional_cost": {"kind": "discard", "count": 0}}"#;
    let card = serde_json::from_str(json).unwrap();
    assert!(validate_definition(None, &card).is_err());

    // The shape a real card is authored in passes.
    let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                   "types": ["sorcery"], "mana_cost": "{1}{R}",
                   "additional_cost": {"kind": "discard", "count": 1},
                   "spell_effects": [{"kind": "draw_card", "count": 2}]}"#;
    let card = serde_json::from_str(json).unwrap();
    assert!(validate_definition(None, &card).is_ok());
}

#[test]
fn an_aura_grant_on_a_card_that_is_not_an_aura_is_rejected() {
    let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                   "types": ["enchantment"], "subtypes": ["Shrine"], "mana_cost": "{G}",
                   "aura": {"enchant": "any_creature", "power": 1, "toughness": 1}}"#;
    let card = serde_json::from_str(json).unwrap();
    assert_eq!(
        validate_definition(None, &card).unwrap_err(),
        Violation::AuraOnNonAura {
            functional_id: "test_card".to_string()
        }
    );
}

#[test]
fn issue_606_printed_restrictions_on_a_non_creature_are_rejected() {
    // A combat restriction restricts attacking or blocking, so on a non-creature it
    // could only ever be inert. An Aura imposes restrictions through `aura` instead.
    let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                   "types": ["enchantment"], "mana_cost": "{1}",
                   "restrictions": ["cant_block"]}"#;
    assert_eq!(
        validate_definition(None, &serde_json::from_str(json).unwrap()).unwrap_err(),
        Violation::RestrictionsOnNonCreature {
            functional_id: "test_card".to_string()
        }
    );
}

#[test]
fn issue_610_a_may_wrapping_a_targeting_effect_is_rejected() {
    // A wrapper cannot declare the target slot of what it wraps, so the target
    // would never be chosen and the effect would silently do nothing. Both authored
    // spellings of "target" are caught, in an ability and in a spell effect alike,
    // and nesting does not hide either.
    let spec = r#", "abilities": [{"type": "activated", "cost": [],
        "effects": [{"kind": "may", "effects": [{"kind": "tap", "target": "any_creature"}]}]}]"#;
    assert_eq!(
        validate_definition(None, &definition(spec)),
        Err(Violation::TargetInsideOptional {
            functional_id: "test_card".to_string(),
        }),
    );

    let player_ref = r#", "abilities": [{"type": "activated", "cost": [],
        "effects": [{"kind": "may", "cost": "{1}",
                     "effects": [{"kind": "may",
                                  "effects": [{"kind": "mill", "player_ref": "target_player",
                                               "count": 2}]}]}]}]"#;
    assert!(validate_definition(None, &definition(player_ref)).is_err());

    let nested_in_a_spell = r#", "spell_effects": [{"kind": "may",
        "effects": [{"kind": "deal_damage", "target": "any_target", "amount": 2}]}]"#;
    assert!(validate_definition(None, &definition(nested_in_a_spell)).is_err());
}

#[test]
fn issue_610_targets_outside_an_optional_effect_are_untouched() {
    // The rule is about what a `may` *wraps*, not about the card: a targeting
    // effect beside an optional one is ordinary and stays authorable, and so does a
    // non-targeting effect inside the optional one.
    let json = r#", "spell_effects": [{"kind": "deal_damage", "target": "any_target",
                                        "amount": 2},
                                      {"kind": "may", "cost": "{1}",
                                       "effects": [{"kind": "draw_card", "count": 1}]}]"#;
    assert!(validate_definition(None, &definition(json)).is_ok());
}

#[test]
fn issue_606_printed_restrictions_on_a_creature_are_accepted() {
    let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                   "types": ["creature"], "mana_cost": "{1}", "power": 1, "toughness": 1,
                   "restrictions": ["cant_be_blocked_by_more_than_one"]}"#;
    assert!(validate_definition(None, &serde_json::from_str(json).unwrap()).is_ok());
}

#[test]
fn an_aura_grant_on_an_aura_is_accepted() {
    let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                   "types": ["enchantment"], "subtypes": ["Aura"], "mana_cost": "{G}",
                   "aura": {"enchant": "any_creature", "power": 1, "toughness": 1}}"#;
    let card = serde_json::from_str(json).unwrap();
    assert!(validate_definition(None, &card).is_ok());
}

#[test]
fn a_definition_with_no_types_is_rejected() {
    let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                   "types": [], "mana_cost": "{G}"}"#;
    let card = serde_json::from_str(json).unwrap();
    assert_eq!(
        validate_definition(None, &card).unwrap_err(),
        Violation::MalformedField {
            functional_id: "test_card".to_string(),
            field: "types",
        }
    );
}

#[test]
fn duplicate_collector_numbers_in_one_set_are_rejected() {
    assert!(check_printings("FIX", ["1", "2", "3"]).is_ok());
    assert_eq!(
        check_printings("FIX", ["1", "2", "1"]).unwrap_err(),
        Violation::DuplicatePrinting {
            set_code: "FIX".to_string(),
            collector_number: "1".to_string(),
        }
    );
}
