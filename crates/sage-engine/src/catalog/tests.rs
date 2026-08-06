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

    // A sacrifice states its count as a structure, and zero of them is no cost either.
    let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                   "types": ["sorcery"], "mana_cost": "{R}",
                   "additional_cost": {"kind": "sacrifice", "card_type": "creature",
                                       "count": {"exactly": 0}}}"#;
    let card = serde_json::from_str(json).unwrap();
    assert!(validate_definition(None, &card).is_err());

    // "Any number" is a real cost even though a payment of none is legal: the player may
    // pay more, and that decision is the whole of what the card asks.
    let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                   "types": ["sorcery"], "mana_cost": "{2}{G}{G}",
                   "additional_cost": {"kind": "sacrifice", "card_type": "land",
                                       "count": "any"},
                   "spell_effects": [{"kind": "draw_card", "count": 1}]}"#;
    let card = serde_json::from_str(json).unwrap();
    assert!(validate_definition(None, &card).is_ok());

    // The shape a real card is authored in passes.
    let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                   "types": ["sorcery"], "mana_cost": "{1}{R}",
                   "additional_cost": {"kind": "discard", "count": 1},
                   "spell_effects": [{"kind": "draw_card", "count": 2}]}"#;
    let card = serde_json::from_str(json).unwrap();
    assert!(validate_definition(None, &card).is_ok());
}

/// An amount read off a cost payment needs a cost that pays it. The engine's honest
/// answer to "how many did the payment sacrifice" when nothing was sacrificed is zero, so
/// a card that reads as throwing a creature and always deals zero damage is caught here
/// rather than shipped (issue #721).
#[test]
fn an_amount_read_off_a_payment_no_cost_makes_is_rejected() {
    let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                   "types": ["sorcery"], "mana_cost": "{R}",
                   "spell_effects": [{"kind": "deal_damage_by_amount", "target": "any_target",
                                      "amount": {"source": "sacrificed_creature_power"}}]}"#;
    let card = serde_json::from_str(json).unwrap();
    assert_eq!(
        validate_definition(None, &card).unwrap_err(),
        Violation::PaymentAmountIsNeverPaid {
            functional_id: "test_card".to_string()
        }
    );

    // With the cost that pays it, the same card is exactly what Thud authors.
    let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                   "types": ["sorcery"], "mana_cost": "{R}",
                   "additional_cost": {"kind": "sacrifice", "card_type": "creature"},
                   "spell_effects": [{"kind": "deal_damage_by_amount", "target": "any_target",
                                      "amount": {"source": "sacrificed_creature_power"}}]}"#;
    let card = serde_json::from_str(json).unwrap();
    assert!(validate_definition(None, &card).is_ok());

    // An activation cost pays it too, and the amount may sit anywhere in the tree — a
    // search's size is the other place it is read.
    let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                   "types": ["creature"], "mana_cost": "{G}", "power": 1, "toughness": 1,
                   "abilities": [{"type": "activated",
                     "cost": [{"kind": "sacrifice", "card_type": "land", "count": "any"}],
                     "effects": [{"kind": "search_library", "take": 0,
                                  "take_amount": {"source": "sacrificed_to_cost"}}]}]}"#;
    let card = serde_json::from_str(json).unwrap();
    assert!(validate_definition(None, &card).is_ok());
}

#[test]
fn an_aura_grant_on_a_card_that_is_not_an_aura_is_rejected() {
    let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                   "types": ["enchantment"], "subtypes": ["Shrine"], "mana_cost": "{G}",
                   "attachment": {"kind": "aura", "attach_to": "any_creature",
                                  "power": 1, "toughness": 1}}"#;
    let card = serde_json::from_str(json).unwrap();
    assert_eq!(
        validate_definition(None, &card).unwrap_err(),
        Violation::AttachmentSubtypeMismatch {
            functional_id: "test_card".to_string(),
            subtype: "Aura",
        }
    );
}

#[test]
fn issue_728_an_equipment_grant_on_a_card_that_is_not_an_equipment_is_rejected() {
    // The Equipment half of the same rule: the subtype is what makes a card one of
    // these things, and the block only says what it does while attached.
    let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                   "types": ["artifact"], "mana_cost": "{3}",
                   "attachment": {"kind": "equipment", "attach_to": "any_creature_you_control",
                                  "equip": "{2}", "power": 1, "toughness": 1}}"#;
    let card = serde_json::from_str(json).unwrap();
    assert_eq!(
        validate_definition(None, &card).unwrap_err(),
        Violation::AttachmentSubtypeMismatch {
            functional_id: "test_card".to_string(),
            subtype: "Equipment",
        }
    );
}

#[test]
fn issue_728_an_equip_cost_must_agree_with_the_attachment_kind() {
    // Both directions, like the P/T and loyalty pairings: an Equipment with no equip
    // cost could never be attached to anything, and an Aura with one would advertise an
    // ability the rules do not give it.
    let costless = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                   "types": ["artifact"], "subtypes": ["Equipment"], "mana_cost": "{3}",
                   "attachment": {"kind": "equipment", "attach_to": "any_creature_you_control",
                                  "power": 1, "toughness": 1}}"#;
    assert_eq!(
        validate_definition(None, &serde_json::from_str(costless).unwrap()).unwrap_err(),
        Violation::EquipCostMismatch {
            functional_id: "test_card".to_string(),
            equipment: true,
        }
    );

    let paying_aura = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                   "types": ["enchantment"], "subtypes": ["Aura"], "mana_cost": "{G}",
                   "attachment": {"kind": "aura", "attach_to": "any_creature", "equip": "{2}",
                                  "power": 1, "toughness": 1}}"#;
    assert_eq!(
        validate_definition(None, &serde_json::from_str(paying_aura).unwrap()).unwrap_err(),
        Violation::EquipCostMismatch {
            functional_id: "test_card".to_string(),
            equipment: false,
        }
    );
}

#[test]
fn issue_606_printed_restrictions_on_a_non_creature_are_rejected() {
    // A combat restriction restricts attacking or blocking, so on a non-creature it
    // could only ever be inert. An Aura imposes restrictions through `attachment`
    // instead.
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
fn issue_725_a_may_wrapping_one_targeting_effect_is_accepted() {
    // The wrapper forwards that effect's group, so the slot is declared at
    // announcement and filled there. Both authored spellings of "target" go through,
    // in an ability and in a spell effect alike, and a `may` inside a `may` forwards
    // the same single group the whole way up.
    let spec = r#", "abilities": [{"type": "activated", "cost": [],
        "effects": [{"kind": "may", "effects": [{"kind": "tap", "target": "any_creature"}]}]}]"#;
    assert!(validate_definition(None, &definition(spec)).is_ok());

    let player_ref = r#", "abilities": [{"type": "activated", "cost": [],
        "effects": [{"kind": "may", "cost": "{1}",
                     "effects": [{"kind": "may",
                                  "effects": [{"kind": "mill", "player_ref": "target_player",
                                               "count": 2}]}]}]}]"#;
    assert!(validate_definition(None, &definition(player_ref)).is_ok());

    let in_a_spell = r#", "spell_effects": [{"kind": "may",
        "effects": [{"kind": "deal_damage", "target": "any_target", "amount": 2}]}]"#;
    assert!(validate_definition(None, &definition(in_a_spell)).is_ok());

    // A targeting effect *beside* a non-targeting one inside the same `may` is still
    // one group, and still fine.
    let mixed = r#", "spell_effects": [{"kind": "may",
        "effects": [{"kind": "draw_card", "count": 1},
                    {"kind": "destroy", "target": "any_artifact"}]}]"#;
    assert!(validate_definition(None, &definition(mixed)).is_ok());
}

#[test]
fn issue_725_a_may_wrapping_two_targeting_effects_is_rejected() {
    // One forwarding cannot advertise two slots: the flat stored target list would
    // have no way to say which target belongs to which wrapped effect.
    let two = r#", "spell_effects": [{"kind": "may",
        "effects": [{"kind": "destroy", "target": "any_artifact"},
                    {"kind": "tap", "target": "any_creature"}]}]"#;
    assert_eq!(
        validate_definition(None, &definition(two)),
        Err(Violation::TwoTargetsInsideOptional {
            functional_id: "test_card".to_string(),
        }),
    );

    // Nesting does not hide the second one either.
    let nested = r#", "spell_effects": [{"kind": "may",
        "effects": [{"kind": "destroy", "target": "any_artifact"},
                    {"kind": "may",
                     "effects": [{"kind": "mill", "player_ref": "target_player",
                                  "count": 2}]}]}]"#;
    assert!(validate_definition(None, &definition(nested)).is_err());
}

#[test]
fn issue_610_targets_outside_an_optional_effect_are_untouched() {
    // A targeting effect beside an optional one is two ordinary fixed groups, and a
    // non-targeting effect inside the optional one declares none at all.
    let json = r#", "spell_effects": [{"kind": "deal_damage", "target": "any_target",
                                        "amount": 2},
                                      {"kind": "may", "cost": "{1}",
                                       "effects": [{"kind": "draw_card", "count": 1}]}]"#;
    assert!(validate_definition(None, &definition(json)).is_ok());
}

#[test]
fn issue_725_an_optional_variable_arity_group_is_still_counted() {
    // The "at most one up-to-N group" invariant looks through the wrapper too: an
    // optional "return up to two" is as variable-arity as a bare one, and pairing two
    // of them back onto effects would be a guess.
    let json = r#", "spell_effects": [
        {"kind": "may", "effects": [{"kind": "return_card_to_hand",
                                     "target": {"card_in_graveyard": {"class": "creature"}},
                                     "targets": {"up_to": 2}}]},
        {"kind": "put_counters", "target": "any_creature", "targets": {"up_to": 2},
         "counter": "plus_one_plus_one", "count": 1}]"#;
    assert_eq!(
        validate_definition(None, &definition(json)),
        Err(Violation::TwoVariableTargetGroups {
            functional_id: "test_card".to_string(),
        }),
    );
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
                   "attachment": {"kind": "aura", "attach_to": "any_creature",
                                  "power": 1, "toughness": 1}}"#;
    let card = serde_json::from_str(json).unwrap();
    assert!(validate_definition(None, &card).is_ok());
}

#[test]
fn issue_728_an_equipment_grant_on_an_equipment_is_accepted() {
    let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                   "types": ["artifact"], "subtypes": ["Equipment"], "mana_cost": "{3}",
                   "attachment": {"kind": "equipment", "attach_to": "any_creature_you_control",
                                  "equip": "{2}", "power": 2, "toughness": 1}}"#;
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

#[test]
fn a_static_condition_may_not_count_by_power() {
    // A power bound is read through the computed characteristics, and a static
    // ability's condition is evaluated *inside* the computation of a permanent's
    // characteristics. Asking there would ask again, forever. Rejected at build time
    // so the failure is a sentence rather than a stack overflow.
    let statik = r#", "power": 1, "toughness": 1, "types": ["creature"],
        "abilities": [{"type": "static", "affects": {"scope": "source"},
            "modification": {"kind": "power_toughness", "power": 1, "toughness": 0},
            "condition": {"kind": "controls_at_least",
                          "permanents": {"card_type": "creature", "min_power": 4}}}]"#;
    assert_eq!(
        validate_definition(None, &definition(statik)),
        Err(Violation::PowerInStaticCondition {
            functional_id: "test_card".to_string(),
        }),
    );

    // The same condition shape on an `Effect::Conditional` is evaluated during a
    // resolution, where nothing recurses, and stays authorable. This is the whole
    // reason the check names `type: "static"` rather than the condition itself.
    let intervening_if = r#", "spell_effects": [{"kind": "conditional",
        "condition": {"kind": "controls_at_least",
                      "permanents": {"card_type": "creature", "min_power": 4}, "count": 1},
        "then": [{"kind": "draw_card", "count": 1}]}]"#;
    assert!(validate_definition(None, &definition(intervening_if)).is_ok());
}

#[test]
fn an_attachment_s_counted_grant_may_not_count_by_power() {
    // The second site evaluated from inside the layer system, refused for the reason a
    // static ability's condition is: an Aura's grant is read while its host's
    // characteristics are being computed, so a *computed* power there would ask the layer
    // system for the answer it is producing — and two mutually enchanted creatures would
    // ask each other forever.
    let aura = |count_of: &str| {
        let json = format!(
            r#"{{"schema_version": 1, "functional_id": "test_aura", "name": "Test Aura",
                 "types": ["enchantment"], "subtypes": ["Aura"], "mana_cost": "{{G}}",
                 "attachment": {{"kind": "aura", "attach_to": "any_creature",
                                 "power": 1, "toughness": 1, "count_of": {count_of}}}}}"#
        );
        serde_json::from_str::<serde_json::Value>(&json).unwrap()
    };
    assert_eq!(
        validate_definition(None, &aura(r#"{"subtype": "Forest", "min_power": 4}"#)),
        Err(Violation::PowerInAttachmentCount {
            functional_id: "test_aura".to_string(),
        }),
    );
    // The same count without the power bound reads printed characteristics only, which is
    // safe from inside the layer system and is what a counted Aura actually needs.
    assert!(validate_definition(None, &aura(r#"{"subtype": "Forest"}"#)).is_ok());
}

#[test]
fn issue_726_a_layer_six_change_that_changes_nothing_is_rejected() {
    // Every field of `alter_abilities_self` defaults, so the clause that says nothing is
    // exactly the one a typo lands on — and it would mint a timestamp, sit in the stored
    // effects until cleanup, and describe itself as "unchanged until end of turn".
    let empty = r#", "abilities": [{"type": "activated", "cost": [],
        "effects": [{"kind": "alter_abilities_self"}]}]"#;
    assert_eq!(
        validate_definition(None, &definition(empty)),
        Err(Violation::AbilityChangeIsEmpty {
            functional_id: "test_card".to_string(),
        }),
    );

    // Empty lists are the same nothing spelled out longhand.
    let empty_lists = r#", "abilities": [{"type": "activated", "cost": [],
        "effects": [{"kind": "alter_abilities_self", "lose": [], "gain": []}]}]"#;
    assert_eq!(
        validate_definition(None, &definition(empty_lists)),
        Err(Violation::AbilityChangeIsEmpty {
            functional_id: "test_card".to_string(),
        }),
    );

    // Any one of the three fields carrying something makes it a clause.
    for spec in [
        r#"{"kind": "alter_abilities_self", "lose": ["defender"]}"#,
        r#"{"kind": "alter_abilities_self", "gain": ["flying"]}"#,
        r#"{"kind": "alter_abilities_self", "lose_all": true}"#,
    ] {
        let ability =
            format!(r#", "abilities": [{{"type": "activated", "cost": [], "effects": [{spec}]}}]"#);
        assert!(validate_definition(None, &definition(&ability)).is_ok());
    }
}

#[test]
fn a_return_self_from_graveyard_must_sit_on_an_ability_that_could_reach_it() {
    // CR 113.6: the effect is what makes an ability function from a graveyard, so it is
    // only honest where such an ability can actually be reached from there. The two
    // shapes M19 prints both validate — Reassembling Skeleton's activated ability whose
    // cost is mana, and Spit Flame's trigger, whose return is the payoff of an optional
    // cost nested inside it.
    for good in [
        r#", "abilities": [{"type": "activated",
             "cost": [{"kind": "mana", "mana": "{1}{B}"}],
             "effects": [{"kind": "return_self_from_graveyard",
                          "destination": "battlefield_tapped"}]}]"#,
        r#", "abilities": [{"type": "triggered", "event": "self_dies",
             "effects": [{"kind": "may", "cost": "{R}", "effects": [
                 {"kind": "return_self_from_graveyard", "destination": "hand"}]}]}]"#,
    ] {
        assert!(
            validate_definition(None, &definition(good)).is_ok(),
            "expected `{good}` to validate"
        );
    }

    // A card in a graveyard is not a permanent: there is nothing to tap. An ability
    // authored this way would simply never be offered — a dead ability, caught here.
    let tapped = r#", "abilities": [{"type": "activated", "cost": [{"kind": "tap"}],
        "effects": [{"kind": "return_self_from_graveyard", "destination": "hand"}]}]"#;
    assert_eq!(
        validate_definition(None, &definition(tapped)),
        Err(Violation::GraveyardAbilityCannotFunction {
            functional_id: "test_card".to_string(),
        }),
    );

    // And outside an activated or triggered ability there is no card in a graveyard for
    // the return to act on at all — a spell's own effects resolve from the stack, and an
    // emblem is in no zone and has no card anywhere.
    for elsewhere in [
        r#", "spell_effects": [{"kind": "return_self_from_graveyard",
             "destination": "hand"}]"#,
        r#", "abilities": [{"type": "triggered", "event": "self_dies",
             "effects": [{"kind": "create_emblem", "abilities": [
                 {"type": "triggered", "event": "upkeep", "effects": [
                     {"kind": "return_self_from_graveyard", "destination": "hand"}]}]}]}]"#,
    ] {
        assert_eq!(
            validate_definition(None, &definition(elsewhere)),
            Err(Violation::GraveyardAbilityCannotFunction {
                functional_id: "test_card".to_string(),
            }),
            "expected `{elsewhere}` to be rejected"
        );
    }
}

#[test]
fn issue_740_a_granted_ability_is_an_ability_position_like_any_other() {
    // An ability a card hands away is still an ability of a real object once it is
    // granted, so the CR 113.6 rule above has to see inside the two places a card writes
    // one: an `attachment` block's grant, and the `abilities` a pump gives its target.
    // A `create_emblem`'s stay excluded — an emblem is in no zone and has no card.
    for granted in [
        r#", "spell_effects": [{"kind": "pump", "target": "any_creature",
             "power": 2, "toughness": 0, "abilities": [
                 {"type": "triggered", "event": "self_dies", "effects": [
                     {"kind": "return_self_from_graveyard",
                      "destination": "battlefield_tapped"}]}]}]"#,
        r#", "subtypes": ["Aura"],
            "attachment": {"kind": "aura", "attach_to": "any_creature", "abilities": [
                {"type": "activated", "cost": [{"kind": "mana", "mana": "{B}"}],
                 "effects": [{"kind": "return_self_from_graveyard",
                              "destination": "hand"}]}]}"#,
    ] {
        assert!(
            validate_definition(None, &definition(granted)).is_ok(),
            "expected `{granted}` to validate"
        );
    }

    // And the cost rule reaches a granted ability too: a card in a graveyard has nothing
    // to tap, whoever granted the ability that would tap it.
    let tapped = r#", "subtypes": ["Aura"],
        "attachment": {"kind": "aura", "attach_to": "any_creature", "abilities": [
            {"type": "activated", "cost": [{"kind": "tap"}],
             "effects": [{"kind": "return_self_from_graveyard", "destination": "hand"}]}]}"#;
    assert_eq!(
        validate_definition(None, &definition(tapped)),
        Err(Violation::GraveyardAbilityCannotFunction {
            functional_id: "test_card".to_string(),
        }),
    );
}

#[test]
fn issue_738_the_chosen_color_must_be_chosen_somewhere_on_the_card() {
    // "A spell of the chosen color" is the one trigger selector that refers to the rest
    // of its own card. Without an `enters_choosing_color` there is no colour to refer to
    // and the trigger cannot fire once all game — and the engine's answer to a permanent
    // with no recorded colour is *silence*, which is why this is caught at authoring
    // time instead.
    let unnamed = r#", "abilities": [{"type": "triggered",
        "event": {"you_cast_spell": "chosen_color"},
        "effects": [{"kind": "gain_life", "player_ref": "controller", "amount": 1}]}]"#;
    assert_eq!(
        validate_definition(None, &definition(unnamed)),
        Err(Violation::ChosenColorIsNeverNamed {
            functional_id: "test_card".to_string(),
        }),
    );

    // With the choice declared, the same trigger is exactly what Diamond Mare authors.
    let named = r#", "abilities": [{"type": "enters_choosing_color"},
        {"type": "triggered", "event": {"you_cast_spell": "chosen_color"},
         "effects": [{"kind": "gain_life", "player_ref": "controller", "amount": 1}]}]"#;
    assert!(validate_definition(None, &definition(named)).is_ok());

    // A card that names a colour and never reads it back is not an error: an entry
    // choice is a fact about the permanent, and nothing says an ability has to consult
    // it. Only the dangling reference is wrong.
    let choice_only = r#", "abilities": [{"type": "enters_choosing_color"}]"#;
    assert!(validate_definition(None, &definition(choice_only)).is_ok());

    // The other spellings of the same trigger are untouched.
    let ordinary = r#", "abilities": [{"type": "triggered",
        "event": {"you_cast_spell": "instant_or_sorcery"},
        "effects": [{"kind": "draw_card", "count": 1}]}]"#;
    assert!(validate_definition(None, &definition(ordinary)).is_ok());
}

#[test]
fn issue_738_the_chosen_name_must_be_named_somewhere_on_the_card() {
    // The selector counterpart of the colour rule above, and wrong for the same reason:
    // "permanents with the chosen name" refers to the rest of its own card, and without
    // an `enters_naming_card` the class could never contain one permanent all game.
    let unnamed = r#", "abilities": [{"type": "static",
        "affects": {"scope": "permanents_your_opponents_control", "card_type": "land",
                    "with_the_named_card": true},
        "modification": {"kind": "lose_all_abilities"}}]"#;
    assert_eq!(
        validate_definition(None, &definition(unnamed)),
        Err(Violation::ChosenNameIsNeverNamed {
            functional_id: "test_card".to_string(),
        }),
    );

    // With the naming declared, the same selector is exactly what Alpine Moon authors.
    let named = r#", "abilities": [
        {"type": "enters_naming_card", "class": "nonbasic_land"},
        {"type": "static",
         "affects": {"scope": "permanents_your_opponents_control", "card_type": "land",
                     "with_the_named_card": true},
         "modification": {"kind": "lose_all_abilities"}}]"#;
    assert!(validate_definition(None, &definition(named)).is_ok());

    // Naming a card and never reading it back is not an error, for the reason choosing a
    // colour and never reading it back is not: only the dangling reference is wrong.
    let choice_only = r#", "abilities": [
        {"type": "enters_naming_card", "class": "nonbasic_land"}]"#;
    assert!(validate_definition(None, &definition(choice_only)).is_ok());

    // The same class without the name filter refers to nothing outside itself.
    let unfiltered = r#", "abilities": [{"type": "static",
        "affects": {"scope": "permanents_your_opponents_control", "card_type": "land"},
        "modification": {"kind": "lose_all_abilities"}}]"#;
    assert!(validate_definition(None, &definition(unfiltered)).is_ok());
}

#[test]
fn issue_737_a_two_slot_effect_counts_as_two_target_groups() {
    // A fight names two slots in two fields, so every rule stated in *groups* sees two
    // of them. A `may` forwards the groups of the one effect it wraps, and two is one
    // too many for the flat stored target list to pair back.
    let optional_fight = r#", "spell_effects": [{"kind": "may",
        "effects": [{"kind": "fight", "dealer": "any_creature_you_control",
                     "dealt_to": "any_creature_an_opponent_controls"}]}]"#;
    assert_eq!(
        validate_definition(None, &definition(optional_fight)),
        Err(Violation::TwoTargetsInsideOptional {
            functional_id: "test_card".to_string(),
        }),
    );

    // A conditional branch may choose no target at all, however the branch is written —
    // and both of a fight's slots are targets.
    let conditional_fight = r#", "spell_effects": [{"kind": "conditional",
        "condition": {"kind": "controls_at_least",
                      "permanents": {"card_type": "creature"}, "count": 2},
        "then": [{"kind": "fight", "dealer": "any_creature_you_control",
                  "dealt_to": "any_creature_an_opponent_controls"}]}]"#;
    assert_eq!(
        validate_definition(None, &definition(conditional_fight)),
        Err(Violation::TargetInsideConditional {
            functional_id: "test_card".to_string(),
        }),
    );

    // On its own it is exactly what Rabid Bite authors, and it validates.
    let bare = r#", "spell_effects": [{"kind": "fight", "dealer": "any_creature_you_control",
        "dealt_to": "any_creature_an_opponent_controls"}]"#;
    assert!(validate_definition(None, &definition(bare)).is_ok());
}

/// The mode list of a sorcery with `count` identical, harmless modes.
fn modal(count: usize) -> serde_json::Value {
    let mode = serde_json::json!({"effects": [{"kind": "draw_card", "count": 1}]});
    serde_json::json!({
        "schema_version": 1,
        "functional_id": "test_card",
        "name": "Test Card",
        "types": ["sorcery"],
        "mana_cost": "{G}",
        "modes": vec![mode; count],
    })
}

/// **The dock's band is four rows wide, so a fifth mode is a card the catalog refuses**
/// (`docs/client-design.md` §6.7). The alternative is truncating a mode a player has to
/// read before they can choose it, which §6 forbids outright — so the limit lands on the
/// person authoring the card rather than on the person playing it.
#[test]
fn a_fifth_mode_is_refused() {
    for count in [2, 3, 4] {
        assert!(
            validate_definition(Some("test_card"), &modal(count)).is_ok(),
            "{count} modes fit the band"
        );
    }
    assert_eq!(
        validate_definition(Some("test_card"), &modal(MAX_MODES + 1)).unwrap_err(),
        Violation::MalformedModes {
            functional_id: "test_card".to_string(),
            modes: MAX_MODES + 1,
        }
    );
}

/// One mode is a question with a single answer, and no modes at all in a `modes` key is
/// a typo rather than a card.
#[test]
fn fewer_than_two_modes_is_not_a_choice() {
    for count in [0, 1] {
        assert_eq!(
            validate_definition(Some("test_card"), &modal(count)).unwrap_err(),
            Violation::MalformedModes {
                functional_id: "test_card".to_string(),
                modes: count,
            }
        );
    }
}

/// A modal card's effects live in its modes. Loose `spell_effects` beside them would
/// resolve whichever mode was chosen — a sentence the player never picked.
#[test]
fn a_modal_card_carries_no_loose_spell_effects() {
    let mut card = modal(2);
    card["spell_effects"] = serde_json::json!([{"kind": "draw_card", "count": 1}]);
    assert_eq!(
        validate_definition(Some("test_card"), &card).unwrap_err(),
        Violation::MalformedModes {
            functional_id: "test_card".to_string(),
            modes: 2,
        }
    );
}

/// A mode that does nothing is a bullet with no sentence under it.
#[test]
fn a_mode_that_does_nothing_is_refused() {
    let mut card = modal(2);
    card["modes"][1] = serde_json::json!({"effects": []});
    assert!(validate_definition(Some("test_card"), &card).is_err());
}

/// CR 107.3: X is announced as a spell is cast. An activation pays out of a pool with no
/// announcement step to fix a value in, so an `{X}` there would simply be ignored — and
/// the ability activated for nothing.
#[test]
fn an_x_in_an_activation_cost_is_refused() {
    let card = definition(
        r#", "abilities": [{"type": "activated",
                            "cost": [{"kind": "mana", "mana": "{X}{R}"}],
                            "effects": [{"kind": "draw_card", "count": 1}]}]"#,
    );
    assert_eq!(
        validate_definition(Some("test_card"), &card).unwrap_err(),
        Violation::XOutsideAManaCost {
            functional_id: "test_card".to_string(),
            cost: "{X}{R}".to_string(),
        }
    );
}

/// "If X is 5 or more" on a card whose cost prints no `{X}` is a clause that could never
/// be true.
#[test]
fn a_spell_trait_conditional_on_x_needs_an_x_to_announce() {
    let with_x = serde_json::json!({
        "schema_version": 1,
        "functional_id": "test_card",
        "name": "Test Card",
        "types": ["sorcery"],
        "mana_cost": "{X}{R}",
        "spell_effects": [{"kind": "draw_card", "count": 1}],
        "spell_traits": [{"kind": "cant_be_countered", "if_x_at_least": 5}],
    });
    assert!(validate_definition(Some("test_card"), &with_x).is_ok());

    let mut without_x = with_x.clone();
    without_x["mana_cost"] = serde_json::json!("{2}{R}");
    assert_eq!(
        validate_definition(Some("test_card"), &without_x).unwrap_err(),
        Violation::SpellTraitNeedsX {
            functional_id: "test_card".to_string(),
        }
    );

    // An unconditional trait needs no X at all — a card that simply can't be countered
    // is sayable on a fixed cost.
    without_x["spell_traits"] = serde_json::json!([{"kind": "cant_be_countered"}]);
    assert!(validate_definition(Some("test_card"), &without_x).is_ok());
}
