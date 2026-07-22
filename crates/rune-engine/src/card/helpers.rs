//! Public helper functions for card data.

use super::database::CardDatabase;
use crate::ability::Effect;
use crate::id::CardId;
use crate::scripted::scripted_abilities;
use crate::state::Permanent;

/// All abilities of a card: its data-driven [`super::CardData::abilities`] plus any
/// code-defined ones from [`crate::scripted`].
///
/// Returns an empty list if the id is unknown and has no scripted abilities. This
/// is the single accessor the pipeline uses so both authoring tiers are always
/// considered together.
#[must_use]
pub fn abilities_of(db: &CardDatabase, card: CardId) -> Vec<crate::ability::Ability> {
    let Some(data) = db.card(card) else {
        // An unknown handle has no data tier, and the code tier is keyed on the authored
        // identity this handle would have resolved to — so there is nothing to union.
        return Vec::new();
    };
    let mut abilities = data.abilities.clone();
    abilities.extend(scripted_abilities(&data.functional_id));
    abilities
}

/// The effects a spell of printed card `card` produces on resolution
/// ([`super::CardData::spell_effects`]), or an empty list for an unknown id or a card
/// with no spell ability.
///
/// The spell-side counterpart of [`abilities_of`]: the resolve path reads these
/// to apply a spell's effects (pairing targeting effects with the targets chosen
/// at cast), and [`crate::valid_actions`] reads them to enumerate a targeted
/// cast's requirement slots — the same effect IR, whether it rides an ability or
/// a spell.
#[must_use]
pub(crate) fn spell_effects_of(db: &CardDatabase, card: CardId) -> Vec<Effect> {
    db.card(card)
        .map(|c| c.spell_effects.clone())
        .unwrap_or_default()
}

/// Apply `perm`'s own **enters-the-battlefield self-replacement effects**
/// (CR 614.1c) to the freshly built [`Permanent`] as it enters, *before* it is
/// placed on the battlefield.
///
/// This is the replacement seam for [`Ability::EntersTapped`](crate::ability::Ability::EntersTapped) and
/// [`Ability::EntersWithCounters`](crate::ability::Ability::EntersWithCounters): because a replacement modifies the entry
/// *event* rather than acting after it (CR 614.12), the tapped state and counters
/// must already be on `perm` at the moment it joins the battlefield — before the
/// state-based-action loop (so a 0/0 entering with two `+1/+1` counters is a 2/2
/// and survives CR 704.5f) and before any enters-the-battlefield trigger is
/// collected (so the trigger observes the replaced state). It is therefore called
/// at every battlefield-entry site (a land played, [`crate::apply_action`]; a
/// permanent spell resolving, [`crate::resolve::resolve_stack_object`]), never as
/// a post-action pipeline stage. Only the permanent's *own* replacements apply
/// here (CR 614.13 ordering among multiple external replacements is out of scope).
/// Both authoring tiers are honored via [`abilities_of`]; non-replacement
/// abilities are ignored.
pub(crate) fn apply_enters_replacements(db: &CardDatabase, perm: &mut Permanent) {
    for ability in abilities_of(db, perm.card) {
        match ability {
            crate::ability::Ability::EntersTapped => perm.tapped = true,
            crate::ability::Ability::EntersWithCounters { counter, count } => {
                *perm.counters.entry(counter).or_insert(0) += count;
            }
            crate::ability::Ability::Activated { .. }
            | crate::ability::Ability::Triggered { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]

    use super::*;
    use crate::ability::{Ability, Effect, TriggerCondition};

    #[test]
    fn forest_has_one_activated_mana_ability() {
        let db = CardDatabase::bundled().unwrap();
        let forest = crate::card::tests::card_named(&db, "forest");
        assert_eq!(forest.abilities.len(), 1);
        assert!(crate::ability::is_mana_ability(&forest.abilities[0]));
    }

    #[test]
    fn skyscanner_has_an_etb_draw_trigger() {
        let db = CardDatabase::bundled().unwrap();
        let skyscanner = crate::card::tests::card_named(&db, "skyscanner");
        assert_eq!(
            skyscanner.abilities,
            vec![Ability::Triggered {
                event: TriggerCondition::SelfEntersBattlefield,
                effects: vec![Effect::DrawCard { count: 1 }],
            }]
        );
    }

    #[test]
    fn issue_155_etb_replacement_fixtures_carry_their_self_replacements() {
        // The bundled tapland authors an `enters_tapped` self-replacement (CR 614.1c)
        // alongside its two mana abilities. The `enters_with_counters` self-replacement
        // (CR 614.12) has no clean representative in the real M19 catalog, so it is
        // exercised through an inline definition — the sanctioned pattern for an IR
        // shape the shipped set does not use (ADR 0026).
        use crate::card_type::CardType;
        use crate::state::CounterKind;
        let db = CardDatabase::bundled().unwrap();

        let land = crate::card::tests::card_named(&db, "tranquil_expanse");
        assert_eq!(land.name, "Tranquil Expanse");
        assert_eq!(land.types, vec![CardType::Land]);
        assert_eq!(
            land.abilities
                .iter()
                .filter(|a| matches!(a, Ability::EntersTapped))
                .count(),
            1,
            "the tapland enters tapped (CR 614.1c)"
        );
        // Its two tap-for-mana abilities are still present and activatable.
        assert_eq!(
            land.abilities
                .iter()
                .filter(|a| crate::ability::is_mana_ability(a))
                .count(),
            2
        );

        let json = r#"[{"schema_version":1,"functional_id":"test_broodling","name":"Test Broodling",
            "types":["creature"],"subtypes":["Insect"],"mana_cost":"{1}{G}","colors":["green"],
            "power":0,"toughness":0,
            "abilities":[{"type":"enters_with_counters","counter":"plus_one_plus_one","count":2}]}]"#;
        let inline = CardDatabase::from_json(json).unwrap();
        let broodling = crate::card::tests::card_named(&inline, "test_broodling");
        assert_eq!(broodling.power, Some(0));
        assert_eq!(broodling.toughness, Some(0));
        assert_eq!(
            broodling.abilities,
            vec![Ability::EntersWithCounters {
                counter: CounterKind::PlusOnePlusOne,
                count: 2,
            }]
        );
    }

    #[test]
    fn abilities_of_unions_data_and_scripted_sources() {
        let db = CardDatabase::bundled().unwrap();
        // Forest's ability comes from data; no scripted card is registered, so
        // the accessor returns exactly the data-driven ability.
        let forest = crate::card::tests::id_of(&db, "forest");
        assert_eq!(
            abilities_of(&db, forest),
            db.card(forest).unwrap().abilities
        );
        // An unknown id with no scripted abilities yields nothing.
        assert!(abilities_of(&db, CardId(9999)).is_empty());
    }

    #[test]
    fn issue_149_effect_ir_wave_fixtures_carry_their_verbs() {
        use crate::ability::{PlayerRef, TargetSpec};
        use crate::state::CounterKind;
        let db = CardDatabase::bundled().unwrap();

        // A burn instant: deal 2 to any target.
        let shock = crate::card::tests::card_named(&db, "shock");
        assert_eq!(shock.name, "Shock");
        assert_eq!(
            shock.spell_effects,
            vec![Effect::DealDamage {
                target: TargetSpec::AnyTarget,
                amount: 2
            }]
        );
        // A burn instant restricted to a creature: deal 4 to target creature.
        assert_eq!(
            crate::card::tests::card_named(&db, "electrify").spell_effects,
            vec![Effect::DealDamage {
                target: TargetSpec::AnyCreature,
                amount: 4
            }]
        );
        // A destroy instant.
        let murder = crate::card::tests::card_named(&db, "murder");
        assert_eq!(
            murder.spell_effects,
            vec![Effect::Destroy {
                target: TargetSpec::AnyCreature
            }]
        );
        // A two-effect spell: gain life, then draw.
        assert_eq!(
            crate::card::tests::card_named(&db, "revitalize").spell_effects,
            vec![
                Effect::GainLife {
                    player_ref: PlayerRef::Controller,
                    amount: 3
                },
                Effect::DrawCard { count: 1 },
            ]
        );

        // Effects the real M19 catalog does not use — a +1/+1 ETB counter, life loss,
        // and a -1/-1 counter — are exercised inline (ADR 0026).
        let json = r#"[
            {"schema_version":1,"functional_id":"test_sprite","name":"Test Sprite",
             "types":["creature"],"subtypes":["Faerie"],"mana_cost":"{1}{G}","colors":["green"],
             "power":1,"toughness":1,
             "abilities":[{"type":"triggered","event":"self_enters_battlefield",
               "effects":[{"kind":"put_counters","target":"any_creature","counter":"plus_one_plus_one","count":1}]}]},
            {"schema_version":1,"functional_id":"test_drain","name":"Test Drain",
             "types":["instant"],"mana_cost":"{B}","colors":["black"],
             "spell_effects":[{"kind":"lose_life","player_ref":"controller","amount":2}]},
            {"schema_version":1,"functional_id":"test_wither","name":"Test Wither",
             "types":["sorcery"],"mana_cost":"{B}","colors":["black"],
             "spell_effects":[{"kind":"put_counters","target":"any_creature","counter":"minus_one_minus_one","count":1}]}
        ]"#;
        let inline = CardDatabase::from_json(json).unwrap();
        assert_eq!(
            crate::card::tests::card_named(&inline, "test_sprite").abilities,
            vec![Ability::Triggered {
                event: TriggerCondition::SelfEntersBattlefield,
                effects: vec![Effect::PutCounters {
                    target: TargetSpec::AnyCreature,
                    counter: CounterKind::PlusOnePlusOne,
                    count: 1,
                }],
            }]
        );
        assert_eq!(
            crate::card::tests::card_named(&inline, "test_drain").spell_effects,
            vec![Effect::LoseLife {
                player_ref: PlayerRef::Controller,
                amount: 2
            }]
        );
        assert_eq!(
            crate::card::tests::card_named(&inline, "test_wither").spell_effects,
            vec![Effect::PutCounters {
                target: TargetSpec::AnyCreature,
                counter: CounterKind::MinusOneMinusOne,
                count: 1,
            }]
        );
    }

    #[test]
    fn bundled_spells_carry_their_functions() {
        use crate::ability::Cost;
        use crate::card_type::CardType;
        use crate::mana::Color;

        let db = CardDatabase::bundled().unwrap();

        // Lightning Strike: a {1}{R} bolt dealing 3 to any target — distinct from
        // Shock's 2, so it is its own definition rather than a reprint of one identity.
        let strike = crate::card::tests::card_named(&db, "lightning_strike");
        assert_eq!(strike.types, vec![CardType::Instant]);
        assert_eq!(
            strike.spell_effects,
            vec![Effect::DealDamage {
                target: crate::ability::TargetSpec::AnyTarget,
                amount: 3,
            }]
        );
        assert_ne!(
            strike.spell_effects,
            crate::card::tests::card_named(&db, "shock").spell_effects,
            "a byte-identical twin should be a reprint, not a second definition"
        );

        // Divination: a {2}{U} sorcery drawing two.
        let divination = crate::card::tests::card_named(&db, "divination");
        assert_eq!(divination.types, vec![CardType::Sorcery]);
        assert_eq!(
            divination.spell_effects,
            vec![Effect::DrawCard { count: 2 }]
        );

        // Viashino Pyromancer: a creature whose ETB trigger deals 2 to a target player.
        let pyromancer = crate::card::tests::card_named(&db, "viashino_pyromancer");
        assert_eq!(
            pyromancer.abilities,
            vec![Ability::Triggered {
                event: TriggerCondition::SelfEntersBattlefield,
                effects: vec![Effect::DealDamage {
                    target: crate::ability::TargetSpec::AnyPlayer,
                    amount: 2,
                }],
            }]
        );

        // A mana dork: {T}: Add {G} is a mana ability (CR 605.1a).
        let elves = crate::card::tests::card_named(&db, "llanowar_elves");
        assert_eq!(
            elves.abilities,
            vec![Ability::Activated {
                cost: vec![Cost::Tap],
                effects: vec![Effect::AddMana {
                    color: Color::Green,
                    amount: 1,
                }],
            }]
        );
        assert!(crate::ability::is_mana_ability(&elves.abilities[0]));

        // The colorless-mana verb ({T}: Add {C}) has no clean M19 representative,
        // so a mana rock is exercised inline (ADR 0026).
        let json = r#"[{"schema_version":1,"functional_id":"test_lodestone","name":"Test Lodestone",
            "types":["artifact"],"mana_cost":"{1}","colors":[],
            "abilities":[{"type":"activated","cost":[{"kind":"tap"}],
              "effects":[{"kind":"add_colorless_mana","amount":1}]}]}]"#;
        let inline = CardDatabase::from_json(json).unwrap();
        let lodestone = crate::card::tests::card_named(&inline, "test_lodestone");
        assert_eq!(lodestone.types, vec![CardType::Artifact]);
        assert_eq!(
            lodestone.abilities,
            vec![Ability::Activated {
                cost: vec![Cost::Tap],
                effects: vec![Effect::AddColorlessMana { amount: 1 }],
            }]
        );
        assert!(crate::ability::is_mana_ability(&lodestone.abilities[0]));
    }
}
