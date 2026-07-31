//! Round-trip and targeting tests for the card-effect IR.

#![allow(clippy::unwrap_used)]

use super::*;

#[test]
fn activated_mana_ability_round_trips() {
    let json = r#"{"type":"activated","cost":[{"kind":"tap"}],"effects":[{"kind":"add_mana","color":"green","amount":1}]}"#;
    let ability: Ability = serde_json::from_str(json).unwrap();
    assert_eq!(
        ability,
        Ability::Activated {
            cost: vec![Cost::Tap],
            effects: vec![Effect::AddMana {
                color: Color::Green,
                amount: 1
            }],
        }
    );
    assert!(is_mana_ability(&ability));
}

#[test]
fn issue_256_activated_colorless_mana_ability_round_trips() {
    // A mana rock's {T}: Add {C} — an activated ability whose only effect is
    // colorless mana production. It round-trips and is recognized as a mana ability.
    let json = r#"{"type":"activated","cost":[{"kind":"tap"}],"effects":[{"kind":"add_colorless_mana","amount":1}]}"#;
    let ability: Ability = serde_json::from_str(json).unwrap();
    assert_eq!(
        ability,
        Ability::Activated {
            cost: vec![Cost::Tap],
            effects: vec![Effect::AddColorlessMana { amount: 1 }],
        }
    );
    assert!(is_mana_ability(&ability));
    // Colorless mana production has an implicit subject, so it targets nothing.
    assert_eq!(Effect::AddColorlessMana { amount: 1 }.target_spec(), None);
}

#[test]
fn triggered_etb_draw_round_trips() {
    let json = r#"{"type":"triggered","event":"self_enters_battlefield","effects":[{"kind":"draw_card","count":1}]}"#;
    let ability: Ability = serde_json::from_str(json).unwrap();
    assert_eq!(
        ability,
        Ability::Triggered {
            event: TriggerCondition::SelfEntersBattlefield,
            effects: vec![Effect::DrawCard { count: 1 }],
        }
    );
    assert!(!is_mana_ability(&ability));
}

#[test]
fn issue_151_triggered_dies_draw_round_trips() {
    // The dies trigger authors its condition as the bare `self_dies` tag
    // (CR 700.4 / 603.6c) and reuses the draw effect.
    let json =
        r#"{"type":"triggered","event":"self_dies","effects":[{"kind":"draw_card","count":1}]}"#;
    let ability: Ability = serde_json::from_str(json).unwrap();
    assert_eq!(
        ability,
        Ability::Triggered {
            event: TriggerCondition::SelfDies,
            effects: vec![Effect::DrawCard { count: 1 }],
        }
    );
    assert!(!is_mana_ability(&ability));
}

#[test]
fn issue_607_step_trigger_round_trips_with_its_step_and_scope() {
    // A step condition carries two values, so it authors as the wrapped form the
    // selector-carrying conditions use rather than as a bare tag.
    let json = r#"{"type":"triggered",
        "event":{"beginning_of_step":{"step":"upkeep","whose_turn":"yours"}},
        "effects":[{"kind":"gain_life","player_ref":"controller","amount":1}]}"#;
    let ability: Ability = serde_json::from_str(json).unwrap();
    assert_eq!(
        ability,
        Ability::Triggered {
            event: TriggerCondition::BeginningOfStep {
                step: TriggerStep::Upkeep,
                whose_turn: TurnScope::Yours,
            },
            effects: vec![Effect::GainLife {
                player_ref: PlayerRef::Controller,
                amount: 1
            }],
        }
    );
    assert!(!is_mana_ability(&ability));
}

#[test]
fn issue_607_each_trigger_step_maps_to_its_turn_structure_step() {
    // The vocabulary names four steps and each names exactly one real step. Every
    // one of them grants priority, which is the property that keeps a step trigger
    // answerable in the step it belongs to.
    use crate::phase::Step;
    for (authored, step) in [
        (TriggerStep::Upkeep, Step::Upkeep),
        (TriggerStep::Draw, Step::Draw),
        (TriggerStep::BeginCombat, Step::BeginCombat),
        (TriggerStep::EndStep, Step::End),
    ] {
        assert_eq!(authored.step(), step);
    }
}

#[test]
fn issue_607_a_step_outside_the_vocabulary_is_a_parse_error() {
    // The closed set is the guard: `cleanup` and `untap` grant no priority, so a
    // trigger owed there could not be answered in its own step. Authoring one must
    // fail loudly rather than parse into a variant nothing implements.
    for step in ["cleanup", "untap", "postcombat_main"] {
        let json = format!(
            r#"{{"type":"triggered",
                "event":{{"beginning_of_step":{{"step":"{step}","whose_turn":"each"}}}},
                "effects":[{{"kind":"draw_card","count":1}}]}}"#
        );
        assert!(
            serde_json::from_str::<Ability>(&json).is_err(),
            "`{step}` is not in the step-trigger vocabulary"
        );
    }
}

#[test]
fn issue_155_enters_tapped_replacement_round_trips() {
    // The "enters tapped" self-replacement (CR 614.1c) authors as the bare
    // `enters_tapped` type tag and is not a mana ability.
    let json = r#"{"type":"enters_tapped"}"#;
    let ability: Ability = serde_json::from_str(json).unwrap();
    assert_eq!(ability, Ability::EntersTapped);
    assert!(!is_mana_ability(&ability));
}

#[test]
fn issue_155_enters_with_counters_replacement_round_trips() {
    // The "enters with N counters" self-replacement (CR 614.12) authors its
    // counter kind under `counter` (the enum reserves `type` for its tag) and
    // its count as data.
    let json = r#"{"type":"enters_with_counters","counter":"plus_one_plus_one","count":2}"#;
    let ability: Ability = serde_json::from_str(json).unwrap();
    assert_eq!(
        ability,
        Ability::EntersWithCounters {
            counter: CounterKind::PlusOnePlusOne,
            count: 2,
        }
    );
    assert!(!is_mana_ability(&ability));
}

#[test]
fn activated_non_mana_ability_is_not_a_mana_ability() {
    let ability = Ability::Activated {
        cost: vec![Cost::Tap],
        effects: vec![Effect::DrawCard { count: 1 }],
    };
    assert!(!is_mana_ability(&ability));
}

#[test]
fn tap_effect_round_trips_with_its_target_spec() {
    // The target spec is authored as a bare string tag on the effect.
    let json = r#"{"kind":"tap","target":"any_creature"}"#;
    let effect: Effect = serde_json::from_str(json).unwrap();
    assert_eq!(
        effect,
        Effect::Tap {
            target: TargetSpec::AnyCreature,
        }
    );
}

#[test]
fn target_spec_variants_deserialize_from_bare_strings() {
    assert_eq!(
        serde_json::from_str::<TargetSpec>(r#""any_player""#).unwrap(),
        TargetSpec::AnyPlayer
    );
    assert_eq!(
        serde_json::from_str::<TargetSpec>(r#""any_permanent""#).unwrap(),
        TargetSpec::AnyPermanent
    );
}

#[test]
fn only_targeting_effects_report_a_target_spec() {
    // A targeting effect exposes its spec; implicit-subject effects do not.
    assert_eq!(
        Effect::Tap {
            target: TargetSpec::AnyPermanent,
        }
        .target_spec(),
        Some(TargetSpec::AnyPermanent)
    );
    assert_eq!(Effect::DrawCard { count: 1 }.target_spec(), None);
    assert_eq!(
        Effect::AddMana {
            color: Color::Green,
            amount: 1
        }
        .target_spec(),
        None
    );
}

#[test]
fn counter_spell_effect_round_trips_with_its_target_spec() {
    // The counterspell effect authors its spec as a bare string tag, and only
    // it (a targeting effect) reports a spec (CR 701.5).
    let json = r#"{"kind":"counter_spell","target":"spell_on_stack"}"#;
    let effect: Effect = serde_json::from_str(json).unwrap();
    assert_eq!(
        effect,
        Effect::CounterSpell {
            target: TargetSpec::SpellOnStack,
        }
    );
    assert_eq!(effect.target_spec(), Some(TargetSpec::SpellOnStack));
    assert_eq!(
        serde_json::from_str::<TargetSpec>(r#""spell_on_stack""#).unwrap(),
        TargetSpec::SpellOnStack
    );
}

#[test]
fn a_tap_effect_is_not_a_mana_ability() {
    let ability = Ability::Activated {
        cost: vec![Cost::Tap],
        effects: vec![Effect::Tap {
            target: TargetSpec::AnyCreature,
        }],
    };
    assert!(!is_mana_ability(&ability));
}

#[test]
fn issue_149_deal_damage_round_trips_with_its_target_spec() {
    let json = r#"{"kind":"deal_damage","target":"any_target","amount":2}"#;
    let effect: Effect = serde_json::from_str(json).unwrap();
    assert_eq!(
        effect,
        Effect::DealDamage {
            subject: DamageSubject::Target(TargetSpec::AnyTarget),
            amount: 2,
        }
    );
    // A targeting effect reports its spec; the "any target" spec deserializes
    // from its bare string tag.
    assert_eq!(effect.target_spec(), Some(TargetSpec::AnyTarget));
    assert_eq!(
        serde_json::from_str::<TargetSpec>(r#""any_target""#).unwrap(),
        TargetSpec::AnyTarget
    );
}

#[test]
fn issue_611_deal_damage_round_trips_with_a_class_of_players_or_permanents() {
    // The subject is flattened into the effect, so each shape is authored with the
    // key the rest of the vocabulary already uses — and every existing burn card,
    // written with `target`, keeps parsing byte-for-byte as it did (above).
    let players: Effect =
        serde_json::from_str(r#"{"kind":"deal_damage","player_ref":"each_opponent","amount":2}"#)
            .unwrap();
    assert_eq!(
        players,
        Effect::DealDamage {
            subject: DamageSubject::Players(PlayerRef::EachOpponent),
            amount: 2,
        }
    );
    let permanents: Effect =
        serde_json::from_str(r#"{"kind":"deal_damage","affects":"each_creature","amount":1}"#)
            .unwrap();
    assert_eq!(
        permanents,
        Effect::DealDamage {
            subject: DamageSubject::Permanents(MassAffects::EachCreature),
            amount: 1,
        }
    );
    // Neither class fills a target slot (CR 115.1), so neither can fizzle.
    assert_eq!(players.target_spec(), None);
    assert_eq!(permanents.target_spec(), None);
    // The subject answers the targeting question in one place, so a *targeting*
    // player reference reports the slot it fills just as `lose_life` does.
    assert_eq!(
        DamageSubject::Players(PlayerRef::TargetOpponent).target_spec(),
        Some(TargetSpec::AnyOpponent)
    );
    assert_eq!(
        serde_json::from_str::<MassAffects>(r#""creatures_your_opponents_control""#).unwrap(),
        MassAffects::CreaturesYourOpponentsControl
    );
}

#[test]
fn issue_149_destroy_round_trips_with_its_target_spec() {
    let json = r#"{"kind":"destroy","target":"any_creature"}"#;
    let effect: Effect = serde_json::from_str(json).unwrap();
    assert_eq!(
        effect,
        Effect::Destroy {
            target: TargetSpec::AnyCreature,
        }
    );
    assert_eq!(effect.target_spec(), Some(TargetSpec::AnyCreature));
}

#[test]
fn issue_149_put_counters_round_trips_with_both_kinds() {
    // The counter kind is authored under `counter` (the enum reserves `kind`
    // for its own tag) and deserializes from a snake_case string.
    let plus = r#"{"kind":"put_counters","target":"any_creature","counter":"plus_one_plus_one","count":1}"#;
    assert_eq!(
        serde_json::from_str::<Effect>(plus).unwrap(),
        Effect::PutCounters {
            target: TargetSpec::AnyCreature,
            counter: CounterKind::PlusOnePlusOne,
            count: 1,
        }
    );
    let minus = r#"{"kind":"put_counters","target":"any_creature","counter":"minus_one_minus_one","count":2}"#;
    assert_eq!(
        serde_json::from_str::<Effect>(minus).unwrap(),
        Effect::PutCounters {
            target: TargetSpec::AnyCreature,
            counter: CounterKind::MinusOneMinusOne,
            count: 2,
        }
    );
}

#[test]
fn issue_150_pump_round_trips_with_its_target_spec() {
    // The pump verb authors its target spec and signed P/T amounts as card
    // data, and (a targeting effect) reports its spec.
    let json = r#"{"kind":"pump","target":"any_creature","power":3,"toughness":3}"#;
    let effect: Effect = serde_json::from_str(json).unwrap();
    assert_eq!(
        effect,
        Effect::Pump {
            target: TargetSpec::AnyCreature,
            power: 3,
            toughness: 3,
        }
    );
    assert_eq!(effect.target_spec(), Some(TargetSpec::AnyCreature));
}

#[test]
fn issue_374_grant_keyword_round_trips_with_its_target_spec() {
    // The keyword-granting pump verb authors its target spec and the keyword it
    // grants as card data, and (a targeting effect) reports its spec.
    use crate::card::Keyword;
    let json = r#"{"kind":"grant_keyword","target":"any_creature","keyword":"trample"}"#;
    let effect: Effect = serde_json::from_str(json).unwrap();
    assert_eq!(
        effect,
        Effect::GrantKeyword {
            target: TargetSpec::AnyCreature,
            keyword: Keyword::Trample,
        }
    );
    assert_eq!(effect.target_spec(), Some(TargetSpec::AnyCreature));
}

#[test]
fn a_mana_activation_cost_round_trips_as_the_string_it_was_written_in() {
    // The authored card keeps its `{...}` notation; parsing happens on demand, so
    // the JSON a card is written in never has to mirror an internal cost shape.
    let json = r#"{"type":"activated","cost":[{"kind":"mana","mana":"{1}{R}"},{"kind":"tap"}],"effects":[{"kind":"draw_card","count":1}]}"#;
    let ability: Ability = serde_json::from_str(json).unwrap();
    assert_eq!(
        ability,
        Ability::Activated {
            cost: vec![
                Cost::Mana {
                    mana: "{1}{R}".to_string()
                },
                Cost::Tap
            ],
            effects: vec![Effect::DrawCard { count: 1 }],
        }
    );
    // CR 605.1a is about the *effects*, not the cost: a mana cost does not stop an
    // ability being a mana ability, and a non-mana effect still does.
    assert!(!is_mana_ability(&ability));
    let rock: Ability = serde_json::from_str(
        r#"{"type":"activated","cost":[{"kind":"mana","mana":"{1}"},{"kind":"tap"}],"effects":[{"kind":"add_colorless_mana","amount":1}]}"#,
    )
    .unwrap();
    assert!(is_mana_ability(&rock));
}

#[test]
fn a_player_reference_decides_for_itself_whether_it_targets() {
    // The fizzle rule follows from the reference, so every player-subject effect
    // inherits one consistent answer instead of restating it.
    assert_eq!(PlayerRef::Controller.target_spec(), None);
    assert_eq!(PlayerRef::EachOpponent.target_spec(), None);
    assert_eq!(
        PlayerRef::TargetPlayer.target_spec(),
        Some(TargetSpec::AnyPlayer)
    );
    assert_eq!(
        PlayerRef::TargetOpponent.target_spec(),
        Some(TargetSpec::AnyOpponent)
    );

    // …and the effects defer to it, rather than each hard-coding a slot.
    let drain = Effect::LoseLife {
        player_ref: PlayerRef::TargetOpponent,
        amount: 2,
    };
    assert_eq!(drain.target_spec(), Some(TargetSpec::AnyOpponent));
    let symmetric = Effect::LoseLife {
        player_ref: PlayerRef::EachOpponent,
        amount: 2,
    };
    assert_eq!(symmetric.target_spec(), None);
    let mill = Effect::Mill {
        player_ref: PlayerRef::TargetPlayer,
        count: 2,
    };
    assert_eq!(mill.target_spec(), Some(TargetSpec::AnyPlayer));
}

#[test]
fn the_new_effect_verbs_round_trip_with_their_target_or_class() {
    let bounce = r#"{"kind":"return_to_hand","target":"any_creature"}"#;
    let bounce: Effect = serde_json::from_str(bounce).unwrap();
    assert_eq!(
        bounce,
        Effect::ReturnToHand {
            target: TargetSpec::AnyCreature
        }
    );
    assert_eq!(bounce.target_spec(), Some(TargetSpec::AnyCreature));

    let mill = r#"{"kind":"mill","player_ref":"each_opponent","count":2}"#;
    assert_eq!(
        serde_json::from_str::<Effect>(mill).unwrap(),
        Effect::Mill {
            player_ref: PlayerRef::EachOpponent,
            count: 2,
        }
    );

    // A mass modification names a class, which is not a target (CR 115.1).
    let pump = r#"{"kind":"pump_all","affects":"creatures_you_control","power":2,"toughness":1}"#;
    let pump: Effect = serde_json::from_str(pump).unwrap();
    assert_eq!(
        pump,
        Effect::PumpAll {
            affects: MassAffects::CreaturesYouControl,
            power: 2,
            toughness: 1,
        }
    );
    assert_eq!(pump.target_spec(), None);

    let grant =
        r#"{"kind":"grant_keyword_all","affects":"creatures_you_control","keyword":"trample"}"#;
    let grant: Effect = serde_json::from_str(grant).unwrap();
    assert_eq!(
        grant,
        Effect::GrantKeywordAll {
            affects: MassAffects::CreaturesYouControl,
            keyword: Keyword::Trample,
        }
    );
    assert_eq!(grant.target_spec(), None);
}

#[test]
fn the_new_target_specs_deserialize_from_their_bare_string_tags() {
    for (tag, spec) in [
        ("any_opponent", TargetSpec::AnyOpponent),
        ("any_nonland_permanent", TargetSpec::AnyNonlandPermanent),
        (
            "any_creature_you_control",
            TargetSpec::AnyCreatureYouControl,
        ),
        (
            "any_creature_an_opponent_controls",
            TargetSpec::AnyCreatureAnOpponentControls,
        ),
        (
            "any_creature_with_flying",
            TargetSpec::AnyCreatureWithFlying,
        ),
        ("any_tapped_creature", TargetSpec::AnyTappedCreature),
        ("any_artifact", TargetSpec::AnyArtifact),
        ("any_enchantment", TargetSpec::AnyEnchantment),
        (
            "any_artifact_or_enchantment",
            TargetSpec::AnyArtifactOrEnchantment,
        ),
        ("any_land", TargetSpec::AnyLand),
        ("creature_spell_on_stack", TargetSpec::CreatureSpellOnStack),
    ] {
        let json = format!("\"{tag}\"");
        assert_eq!(
            serde_json::from_str::<TargetSpec>(&json).unwrap(),
            spec,
            "{tag}"
        );
    }
}

#[test]
fn the_attacks_trigger_authors_its_condition_as_a_bare_tag() {
    let json = r#"{"type":"triggered","event":"self_attacks","effects":[{"kind":"gain_life","player_ref":"controller","amount":2}]}"#;
    let ability: Ability = serde_json::from_str(json).unwrap();
    assert_eq!(
        ability,
        Ability::Triggered {
            event: TriggerCondition::SelfAttacks,
            effects: vec![Effect::GainLife {
                player_ref: PlayerRef::Controller,
                amount: 2
            }],
        }
    );
}

#[test]
fn issue_604_the_choice_effects_round_trip_with_their_defaults() {
    // The three optional axes — who chooses, which cards, where a found card goes —
    // all default, so the common shapes stay short to author.
    let plain = r#"{"kind":"discard","player_ref":"target_player","count":2}"#;
    let plain: Effect = serde_json::from_str(plain).unwrap();
    assert_eq!(
        plain,
        Effect::Discard {
            player_ref: PlayerRef::TargetPlayer,
            count: 2,
            chosen_by: Chooser::Owner,
            filter: CardFilter::Any,
        }
    );
    // A discard targets exactly when its player reference does, like every other
    // player-subject effect.
    assert_eq!(plain.target_spec(), Some(TargetSpec::AnyPlayer));
    assert_eq!(
        Effect::Discard {
            player_ref: PlayerRef::Controller,
            count: 1,
            chosen_by: Chooser::Owner,
            filter: CardFilter::Any,
        }
        .target_spec(),
        None
    );

    // The coercive shape names its chooser and its class explicitly.
    let coercive = r#"{"kind":"discard","player_ref":"target_opponent","count":1,"chosen_by":"controller","filter":{"kind":"noncreature_nonland"}}"#;
    assert_eq!(
        serde_json::from_str::<Effect>(coercive).unwrap(),
        Effect::Discard {
            player_ref: PlayerRef::TargetOpponent,
            count: 1,
            chosen_by: Chooser::Controller,
            filter: CardFilter::NoncreatureNonland,
        }
    );

    let scry: Effect = serde_json::from_str(r#"{"kind":"scry","count":2}"#).unwrap();
    assert_eq!(scry, Effect::Scry { count: 2 });

    let look = r#"{"kind":"look_at_top","count":4,"take":1,"filter":{"kind":"creature","max_power":2},"destination":"hand"}"#;
    let look: Effect = serde_json::from_str(look).unwrap();
    assert_eq!(
        look,
        Effect::LookAtTop {
            count: 4,
            take: 1,
            filter: CardFilter::Creature { max_power: Some(2) },
            destination: FoundDestination::Hand,
        }
    );

    let search = r#"{"kind":"search_library","take":1,"filter":{"kind":"same_name_as_source"},"destination":"battlefield"}"#;
    assert_eq!(
        serde_json::from_str::<Effect>(search).unwrap(),
        Effect::SearchLibrary {
            take: 1,
            filter: CardFilter::SameNameAsSource,
            destination: FoundDestination::Battlefield,
        }
    );

    // A choice over the controller's own library is never a target (CR 115.1), so
    // none of these three can fizzle.
    for effect in [scry, look] {
        assert_eq!(effect.target_spec(), None);
    }

    // An unconstrained creature filter and the default destination both elide.
    let bare = r#"{"kind":"look_at_top","count":3,"take":1,"filter":{"kind":"creature"}}"#;
    assert_eq!(
        serde_json::from_str::<Effect>(bare).unwrap(),
        Effect::LookAtTop {
            count: 3,
            take: 1,
            filter: CardFilter::Creature { max_power: None },
            destination: FoundDestination::Hand,
        }
    );
}

#[test]
fn issue_149_life_effects_round_trip_and_target_nothing() {
    let gain = r#"{"kind":"gain_life","player_ref":"controller","amount":3}"#;
    let gain: Effect = serde_json::from_str(gain).unwrap();
    assert_eq!(
        gain,
        Effect::GainLife {
            player_ref: PlayerRef::Controller,
            amount: 3,
        }
    );
    let lose = r#"{"kind":"lose_life","player_ref":"controller","amount":2}"#;
    let lose: Effect = serde_json::from_str(lose).unwrap();
    assert_eq!(
        lose,
        Effect::LoseLife {
            player_ref: PlayerRef::Controller,
            amount: 2,
        }
    );
    // Life gain/loss have an implicit subject, so they choose no target.
    assert_eq!(gain.target_spec(), None);
    assert_eq!(lose.target_spec(), None);
}
