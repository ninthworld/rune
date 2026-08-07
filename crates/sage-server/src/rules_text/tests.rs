//! Generated rules-text tests: one per IR shape the formatter gives words to.

#![allow(clippy::panic, clippy::unwrap_used)]

use super::*;
use sage_engine::{CardDatabase, CardId, FunctionalId};

/// The bundled catalog, whose definitions cover every IR construct the engine
/// has: the generated text is asserted against real cards, not toy structs.
pub(super) fn bundled() -> CardDatabase {
    CardDatabase::bundled().unwrap()
}

/// An optional effect's mana cost, for the question the offer is worded as.
fn mana_cost(mana: &str) -> OptionalCost {
    OptionalCost::Mana {
        mana: mana.to_string(),
    }
}

/// The generated text of the card with this authored identity.
pub(super) fn text_of(db: &CardDatabase, functional_id: &str) -> String {
    let id = db
        .card_id(&FunctionalId::try_from(functional_id.to_string()).unwrap())
        .unwrap();
    let data = db.card(id).unwrap();
    rules_text(data, sage_engine::scripted_rules_text(&data.functional_id))
}

#[test]
fn a_vanilla_card_has_no_rules_text() {
    // Nothing is invented for a card with no rules: the empty string is the honest
    // answer, and the wire omits the field entirely.
    let db = bundled();
    assert_eq!(text_of(&db, "onakke_ogre"), "");
}

#[test]
fn an_activated_mana_ability_composes_its_cost_and_effect() {
    let db = bundled();
    assert_eq!(text_of(&db, "forest"), "{T}: Add {G}.");
    // Two separate mana abilities are two separate lines — the IR has two, so the
    // text says two, rather than collapsing them into a choice the card cannot make.
    assert_eq!(
        text_of(&db, "tranquil_expanse"),
        "Tranquil Expanse enters the battlefield tapped.\n{T}: Add {G}.\n{T}: Add {W}."
    );
}

#[test]
fn triggered_abilities_name_their_condition_and_effects() {
    let db = bundled();
    // A real ETB trigger from the catalog.
    assert_eq!(
        text_of(&db, "viashino_pyromancer"),
        "When Viashino Pyromancer enters the battlefield, \
         Viashino Pyromancer deals 2 damage to target player or planeswalker."
    );
    // The dies trigger and the ETB-put-counter trigger have no clean M19 card, so
    // they are exercised inline (ADR 0009).
    let inline = CardDatabase::from_json(
        r#"[
            {"schema_version":1,"functional_id":"test_lurker","name":"Test Lurker",
             "types":["creature"],"subtypes":["Horror"],"mana_cost":"{1}{B}","colors":["black"],
             "power":2,"toughness":2,
             "abilities":[{"type":"triggered","event":"self_dies",
               "effects":[{"kind":"draw_card","count":1}]}]},
            {"schema_version":1,"functional_id":"test_sprite","name":"Test Sprite",
             "types":["creature"],"subtypes":["Faerie"],"mana_cost":"{1}{G}","colors":["green"],
             "power":1,"toughness":1,
             "abilities":[{"type":"triggered","event":"self_enters_battlefield",
               "effects":[{"kind":"put_counters","target":"any_creature","counter":"plus_one_plus_one","count":1}]}]}
        ]"#,
    )
    .unwrap();
    assert_eq!(
        text_of(&inline, "test_lurker"),
        "When Test Lurker dies, draw a card."
    );
    assert_eq!(
        text_of(&inline, "test_sprite"),
        "When Test Sprite enters the battlefield, put a +1/+1 counter on target creature."
    );
}

#[test]
fn spell_effects_read_as_sentences() {
    let db = bundled();
    assert_eq!(text_of(&db, "cancel"), "Counter target spell.");
    assert_eq!(text_of(&db, "shock"), "Shock deals 2 damage to any target.");
    assert_eq!(text_of(&db, "murder"), "Destroy target creature.");
    // A two-effect spell reads as two sentences, in order.
    assert_eq!(text_of(&db, "revitalize"), "You gain 3 life.\nDraw a card.");
    assert_eq!(
        text_of(&db, "titanic_growth"),
        "Target creature gets +4/+4 until end of turn."
    );
    // Life loss and a -1/-1 counter have no clean M19 card — exercised inline.
    let inline = CardDatabase::from_json(
        r#"[
            {"schema_version":1,"functional_id":"test_drain","name":"Test Drain",
             "types":["instant"],"mana_cost":"{B}","colors":["black"],
             "spell_effects":[{"kind":"lose_life","player_ref":"controller","amount":2}]},
            {"schema_version":1,"functional_id":"test_wither","name":"Test Wither",
             "types":["sorcery"],"mana_cost":"{B}","colors":["black"],
             "spell_effects":[{"kind":"put_counters","target":"any_creature","counter":"minus_one_minus_one","count":1}]}
        ]"#,
    )
    .unwrap();
    assert_eq!(text_of(&inline, "test_drain"), "You lose 2 life.");
    assert_eq!(
        text_of(&inline, "test_wither"),
        "Put a -1/-1 counter on target creature."
    );
}

#[test]
fn the_widened_ir_reads_as_sentences_on_the_cards_that_use_it() {
    // Every construct added with the M19 batch is rendered from a real bundled
    // card, not a toy struct: a formatter arm that reads wrong is a card that reads
    // wrong, and the exhaustive matches only prove that *some* words exist.
    let db = bundled();

    // Narrowed target classes name themselves rather than falling back to a
    // generic noun, so a player can tell what the spell may legally be aimed at.
    assert_eq!(
        text_of(&db, "plummet"),
        "Destroy target creature with flying."
    );
    assert_eq!(
        text_of(&db, "take_vengeance"),
        "Destroy target tapped creature."
    );
    assert_eq!(text_of(&db, "smelt"), "Destroy target artifact.");
    assert_eq!(
        text_of(&db, "naturalize"),
        "Destroy target artifact or enchantment."
    );
    assert_eq!(
        text_of(&db, "invoke_the_divine"),
        "Destroy target artifact or enchantment.\nYou gain 4 life."
    );
    assert_eq!(
        text_of(&db, "essence_scatter"),
        "Counter target creature spell."
    );
    assert_eq!(
        text_of(&db, "bone_to_ash"),
        "Counter target creature spell.\nDraw a card."
    );
    assert_eq!(
        text_of(&db, "disperse"),
        "Return target nonland permanent to its owner's hand."
    );
    assert_eq!(
        text_of(&db, "exclusion_mage"),
        "When Exclusion Mage enters the battlefield, return target creature an \
         opponent controls to its owner's hand."
    );
    assert_eq!(
        text_of(&db, "vampire_sovereign"),
        "Flying\nWhen Vampire Sovereign enters the battlefield, target opponent loses \
         3 life and you gain 3 life."
    );
    assert_eq!(
        text_of(&db, "tattered_mummy"),
        "When Tattered Mummy dies, each opponent loses 2 life."
    );
    assert_eq!(
        text_of(&db, "arcane_encyclopedia"),
        "{3}, {T}: Draw a card."
    );

    // A targeted player reference conjugates in the third person; the controller
    // reference stays second — one verb, agreement decided in one place.
    assert_eq!(
        text_of(&db, "sovereign_s_bite"),
        "Target player loses 3 life.\nYou gain 3 life."
    );

    // A mana activation cost passes through in the notation it was written in.
    assert_eq!(
        text_of(&db, "millstone"),
        "{2}, {T}: Target player mills two cards."
    );
    assert_eq!(
        text_of(&db, "vampire_neonate"),
        "{2}, {T}: Each opponent loses 1 life and you gain 1 life."
    );

    // Mass modifications name their class as the subject.
    assert_eq!(
        text_of(&db, "inspired_charge"),
        "Creatures you control get +2/+1 until end of turn."
    );
    assert_eq!(
        text_of(&db, "crash_through"),
        "Creatures you control gain trample until end of turn.\nDraw a card."
    );
    assert_eq!(
        text_of(&db, "angel_of_the_dawn"),
        "Flying\nWhen Angel of the Dawn enters the battlefield, creatures you control get \
         +1/+1 until end of turn and creatures you control gain vigilance until end of turn."
    );

    // The attacks trigger, and the two new keywords.
    assert_eq!(
        text_of(&db, "herald_of_faith"),
        "Flying\nWhenever Herald of Faith attacks, you gain 2 life."
    );
    assert_eq!(text_of(&db, "wall_of_mist"), "Defender");
    assert_eq!(text_of(&db, "boggart_brute"), "Menace");

    // A static keyword grant reads as a standing statement, not an event.
    assert_eq!(
        text_of(&db, "aggressive_mammoth"),
        "Trample\nOther creatures you control have trample."
    );

    // Watching triggers name the class they observe as the sentence's subject.
    assert_eq!(
        text_of(&db, "ajani_s_welcome"),
        "Whenever a creature you control enters the battlefield, you gain 1 life."
    );
    assert_eq!(
        text_of(&db, "poison_tip_archer"),
        "Reach, deathtouch\nWhenever another creature dies, each opponent loses 1 life."
    );
    assert_eq!(
        text_of(&db, "epicure_of_blood"),
        "Whenever you gain life, each opponent loses 1 life."
    );
    assert_eq!(
        text_of(&db, "satyr_enchanter"),
        "Whenever you cast an enchantment spell, draw a card."
    );
    // A self-referential effect names its source rather than saying "this".
    assert_eq!(
        text_of(&db, "ajani_s_pridemate"),
        "Whenever you gain life, put a +1/+1 counter on Ajani's Pridemate."
    );
    assert_eq!(
        text_of(&db, "aven_wind_mage"),
        "Flying\nWhenever you cast an instant or sorcery spell, Aven Wind Mage gets \
         +1/+1 until end of turn."
    );

    // A vanilla body still generates nothing, which stays the honest answer.
    assert_eq!(text_of(&db, "thornhide_wolves"), "");
}

#[test]
fn issue_611_damage_dealt_to_a_class_reads_as_a_sentence() {
    // Damage to a class drops the word "target" and nothing else: the sentence a
    // player acts on is the same shape as the targeted one, so the two forms can
    // never be told apart by how carefully they were worded.
    let db = bundled();
    assert_eq!(
        text_of(&db, "guttersnipe"),
        "Whenever you cast an instant or sorcery spell, \
         Guttersnipe deals 2 damage to each opponent."
    );
    // The targeted shape every existing burn card uses is unchanged.
    assert_eq!(text_of(&db, "shock"), "Shock deals 2 damage to any target.");

    // No bundled card names a class of *permanents* yet, so the sweeper wordings
    // are asserted from an inline catalog rather than by inventing a card for the
    // formatter's benefit (ADR 0009).
    let inline = CardDatabase::from_json(
        r#"[
            {"schema_version":1,"functional_id":"test_pyroclasm","name":"Test Pyroclasm",
             "types":["sorcery"],"mana_cost":"{1}{R}","colors":["red"],
             "spell_effects":[{"kind":"deal_damage","affects":{"scope":"each_creature"},"amount":2}]},
            {"schema_version":1,"functional_id":"test_slagstorm","name":"Test Slagstorm",
             "types":["sorcery"],"mana_cost":"{2}{R}","colors":["red"],
             "spell_effects":[{"kind":"deal_damage","affects":{"scope":"creatures_your_opponents_control"},"amount":3}]},
            {"schema_version":1,"functional_id":"test_recoil","name":"Test Recoil",
             "types":["sorcery"],"mana_cost":"{R}","colors":["red"],
             "spell_effects":[{"kind":"deal_damage","player_ref":"controller","amount":1}]},
            {"schema_version":1,"functional_id":"test_rally","name":"Test Rally",
             "types":["sorcery"],"mana_cost":"{1}{G}","colors":["green"],
             "spell_effects":[{"kind":"pump_all","affects":{"scope":"each_creature"},"power":1,"toughness":1}]}
        ]"#,
    )
    .unwrap();
    assert_eq!(
        text_of(&inline, "test_pyroclasm"),
        "Test Pyroclasm deals 2 damage to each creature."
    );
    assert_eq!(
        text_of(&inline, "test_slagstorm"),
        "Test Slagstorm deals 3 damage to each creature your opponents control."
    );
    assert_eq!(
        text_of(&inline, "test_recoil"),
        "Test Recoil deals 1 damage to you."
    );
    // The same class as the *subject* of a sentence is a bare plural, which is why
    // the two positions are worded by two functions.
    assert_eq!(
        text_of(&inline, "test_rally"),
        "Creatures get +1/+1 until end of turn."
    );
}

#[test]
fn spells_generate_non_empty_text() {
    // Every card that does something renders real rules text from its IR (ADR 0008 §7).
    let db = bundled();
    assert_eq!(
        text_of(&db, "lightning_strike"),
        "Lightning Strike deals 3 damage to any target."
    );
    assert_eq!(
        text_of(&db, "electrify"),
        "Electrify deals 4 damage to target creature."
    );
    assert_eq!(text_of(&db, "divination"), "Draw two cards.");
    for card in [
        "lightning_strike",
        "electrify",
        "divination",
        "murder",
        "shock",
    ] {
        assert!(!text_of(&db, card).is_empty(), "{card} generated no text");
    }
    // A mana rock: colorless mana reads as {C}, the colorless counterpart of {G}.
    // No M19 card produces {C}, so it is exercised inline (ADR 0009).
    let inline = CardDatabase::from_json(
        r#"[{"schema_version":1,"functional_id":"test_lodestone","name":"Test Lodestone",
            "types":["artifact"],"mana_cost":"{1}","colors":[],
            "abilities":[{"type":"activated","cost":[{"kind":"tap"}],
              "effects":[{"kind":"add_colorless_mana","amount":1}]}]}]"#,
    )
    .unwrap();
    assert_eq!(text_of(&inline, "test_lodestone"), "{T}: Add {C}.");
}

#[test]
fn keywords_join_as_one_clause() {
    let db = bundled();
    assert_eq!(text_of(&db, "snapping_drake"), "Flying");
    // Multiple keywords are one comma list, in printed order.
    assert_eq!(
        text_of(&db, "serra_s_guardian"),
        "Flying, vigilance\nOther creatures you control have vigilance."
    );
    // Trample+deathtouch and lone first strike have no clean M19 card — inline.
    let inline = CardDatabase::from_json(
        r#"[
            {"schema_version":1,"functional_id":"test_baneclaw","name":"Test Baneclaw",
             "types":["creature"],"subtypes":["Beast"],"mana_cost":"{2}{B}{G}","colors":["black","green"],
             "power":4,"toughness":4,"keywords":["trample","deathtouch"]},
            {"schema_version":1,"functional_id":"test_duelist","name":"Test Duelist",
             "types":["creature"],"subtypes":["Human","Knight"],"mana_cost":"{1}{W}","colors":["white"],
             "power":2,"toughness":2,"keywords":["first_strike"]}
        ]"#,
    )
    .unwrap();
    assert_eq!(text_of(&inline, "test_baneclaw"), "Trample, deathtouch");
    assert_eq!(text_of(&inline, "test_duelist"), "First strike");
    // A real double striker in the bundled catalog renders its keyword (CR 702.4).
}

#[test]
fn an_aura_states_its_restriction_and_its_grant() {
    // P/T Auras have no clean M19 card, so they are exercised inline (ADR 0009).
    let db = CardDatabase::from_json(
        r#"[
            {"schema_version":1,"functional_id":"test_aegis","name":"Test Aegis",
             "types":["enchantment"],"subtypes":["Aura"],"mana_cost":"{1}{G}","colors":["green"],
             "attachment":{"kind":"aura","attach_to":"any_creature","power":2,"toughness":2}},
            {"schema_version":1,"functional_id":"test_curse","name":"Test Curse",
             "types":["enchantment"],"subtypes":["Aura"],"mana_cost":"{B}","colors":["black"],
             "attachment":{"kind":"aura","attach_to":"any_creature","power":-2,"toughness":-2}}
        ]"#,
    )
    .unwrap();
    assert_eq!(
        text_of(&db, "test_aegis"),
        "Enchant creature.\nEnchanted creature gets +2/+2."
    );
    // A shrinking Aura reads with its signs intact.
    assert_eq!(
        text_of(&db, "test_curse"),
        "Enchant creature.\nEnchanted creature gets -2/-2."
    );
}

#[test]
fn issue_722_a_counted_grant_and_a_counted_token_state_the_class_they_scale_with() {
    // Blanchwood Armor (bundled): the grant sentence names the rule rather than a number,
    // because the number does not exist until someone reads the board.
    let db = bundled();
    assert_eq!(
        text_of(&db, "blanchwood_armor"),
        "Enchant creature.\nEnchanted creature gets +1/+1 for each Forest you control."
    );

    // The token count has no bundled card yet (see the exclusions), so it is exercised
    // inline (ADR 0009). It reads with the same "for each …" clause every other
    // count-derived amount uses.
    let inline = CardDatabase::from_json(
        r#"[{"schema_version":1,"functional_id":"test_marshal","name":"Test Marshal",
            "types":["creature"],"subtypes":["Human"],"mana_cost":"{1}{W}","colors":["white"],
            "power":1,"toughness":1,
            "abilities":[{"type":"triggered","event":"self_enters_battlefield","effects":[
              {"kind":"create_token","count_of":{"card_type":"creature"},
               "token":{"name":"Soldier","types":["creature"],"subtypes":["Soldier"],
                        "colors":["white"],"power":1,"toughness":1}}]}]}]"#,
    )
    .unwrap();
    assert_eq!(
        text_of(&inline, "test_marshal"),
        "When Test Marshal enters the battlefield, you create a 1/1 white Soldier \
         creature token for each creature you control."
    );
}

#[test]
fn issue_722_an_amount_that_is_not_a_count_names_the_source_it_reads() {
    // Each of the three sources reads as the noun phrase the printed card puts after
    // "where X is" or "equal to" — never as a number, which does not exist until the
    // effect resolves.
    let db = bundled();
    assert_eq!(
        text_of(&db, "one_with_the_machine"),
        "You draw cards equal to the greatest mana value among artifacts you control."
    );
    assert_eq!(
        text_of(&db, "nightmare_s_thirst"),
        "You gain 1 life.\nTarget creature gets -X/-X until end of turn, \
         where X is the amount of life you gained this turn."
    );
    assert_eq!(
        text_of(&db, "patient_rebuilding"),
        "At the beginning of your upkeep, target opponent mills three cards \
         and you draw cards equal to the number of land cards milled this way."
    );
}

#[test]
fn issue_722_a_defined_power_and_the_three_places_a_half_reads() {
    // A characteristic-defining ability is a present-tense statement about the source's
    // own power (CR 604.3), not a trigger and not an effect — and the printed `*` is
    // exactly this sentence.
    let db = bundled();
    assert_eq!(
        text_of(&db, "enigma_drake"),
        "Flying\nEnigma Drake's power is equal to the number of instant or sorcery \
         cards in your graveyard."
    );
    // A halved total names *whose* total it is, so "each player" gets "their" and the
    // rounding trails the phrase where a card prints it.
    assert_eq!(
        text_of(&db, "fraying_omnipotence"),
        "Each player loses half their life, rounded up.\n\
         Each player discards half the cards in their hand, rounded up.\n\
         Each player sacrifices half the creatures they control, rounded up."
    );
    // The **open** sacrifice, whose size is not a number read of anything: it names its own
    // class and prints as the imperative a card writes, with the amount that reads it back
    // ("that many") in the next sentence.
    assert_eq!(
        text_of(&db, "scapeshift"),
        "Sacrifice any number of lands.\n\
         Search your library for up to that many land cards, put them onto the \
         battlefield tapped, then shuffle."
    );
    // "Its" points back at the noun the same sentence just named, which is why the life
    // gain is part of the exile rather than a clause standing on its own.
    assert_eq!(
        text_of(&db, "infernal_reckoning"),
        "Exile target colorless creature. You gain life equal to its power."
    );
}

#[test]
fn issue_728_an_equipment_states_its_grant_and_its_equip_ability() {
    // Marauder's Axe (bundled). The grant sentence names "equipped creature" rather than
    // repeating the equip ability's restriction — an Equipment is only ever on a creature
    // (CR 301.5c), whoever controls it — and there is no "Enchant …" sentence, because an
    // Equipment chooses nothing as it is cast.
    //
    // The equip line is composed from the same derived ability the engine activates and by
    // the same `ability_text` that labels the dock button, so the words on the card and
    // the words a player clicks are one string.
    let db = bundled();
    assert_eq!(
        text_of(&db, "marauder_s_axe"),
        "Equipped creature gets +2/+1.\n\
         {2}: Attach Marauder's Axe to target creature you control."
    );

    // A keyword-granting Equipment has no M19 representative, so that shape is exercised
    // inline (ADR 0009) — and reads exactly as the equivalent Aura does below it.
    let inline = CardDatabase::from_json(
        r#"[{"schema_version":1,"functional_id":"test_wings","name":"Test Wings",
            "types":["artifact"],"subtypes":["Equipment"],"mana_cost":"{1}","colors":[],
            "attachment":{"kind":"equipment","attach_to":"any_creature_you_control",
                          "equip":"{1}","keywords":["flying"]}}]"#,
    )
    .unwrap();
    assert_eq!(
        text_of(&inline, "test_wings"),
        "Equipped creature has flying.\n\
         {1}: Attach Test Wings to target creature you control."
    );
}

#[test]
fn issue_374_a_keyword_granting_aura_states_what_it_grants() {
    // Prodigious Growth (bundled): an Aura that grants both P/T and a keyword
    // reads its enchant restriction and each grant as its own sentence (CR
    // 613.1f). A keyword-only Aura has no M19 representative, so that shape is
    // exercised inline (ADR 0009).
    let db = bundled();
    assert_eq!(
        text_of(&db, "prodigious_growth"),
        "Enchant creature.\nEnchanted creature gets +7/+7.\nEnchanted creature has trample."
    );
    let inline = CardDatabase::from_json(
        r#"[{"schema_version":1,"functional_id":"test_flight","name":"Test Flight",
            "types":["enchantment"],"subtypes":["Aura"],"mana_cost":"{U}","colors":["blue"],
            "attachment":{"kind":"aura","attach_to":"any_creature","keywords":["flying"]}}]"#,
    )
    .unwrap();
    assert_eq!(
        text_of(&inline, "test_flight"),
        "Enchant creature.\nEnchanted creature has flying."
    );
}

#[test]
fn issue_374_a_grant_keyword_spell_reads_as_gaining_the_keyword() {
    // Mighty Leap (bundled): one effect, one target, one sentence — "+2/+2 **and**
    // gains flying", the way the card is printed.
    let db = bundled();
    assert_eq!(
        text_of(&db, "mighty_leap"),
        "Target creature gets +2/+2 and gains flying until end of turn."
    );
}

#[test]
fn issue_607_a_step_trigger_names_the_step_and_whose_turn_it_is() {
    // A step trigger's subject is the step, not the source, and the scope is what
    // the sentence has to get right: "your upkeep" and "each upkeep" are different
    // cards. Combat is the irregular one — English puts the scope after the noun
    // ("combat on your turn"), which is why the phrase is composed rather than
    // prefixed. No bundled card carries a bare step trigger (every printed one also
    // wants an intervening-if or a sacrifice cost), so these are inline.
    let db = CardDatabase::from_json(
        r#"[{"schema_version":1,"functional_id":"test_vigil","name":"Test Vigil",
            "types":["enchantment"],"mana_cost":"{1}{W}","colors":["white"],
            "abilities":[{"type":"triggered",
                "event":{"beginning_of_step":{"step":"upkeep","whose_turn":"yours"}},
                "effects":[{"kind":"gain_life","player_ref":"controller","amount":1}]}]},
           {"schema_version":1,"functional_id":"test_fount","name":"Test Fount",
            "types":["artifact"],"mana_cost":"{1}",
            "abilities":[{"type":"triggered",
                "event":{"beginning_of_step":{"step":"upkeep","whose_turn":"each"}},
                "effects":[{"kind":"gain_life","player_ref":"controller","amount":1}]}]},
           {"schema_version":1,"functional_id":"test_rally","name":"Test Rally",
            "types":["enchantment"],"mana_cost":"{2}{R}","colors":["red"],
            "abilities":[{"type":"triggered",
                "event":{"beginning_of_step":{"step":"begin_combat","whose_turn":"yours"}},
                "effects":[{"kind":"draw_card","count":1}]}]},
           {"schema_version":1,"functional_id":"test_dusk","name":"Test Dusk",
            "types":["enchantment"],"mana_cost":"{2}{B}","colors":["black"],
            "abilities":[{"type":"triggered",
                "event":{"beginning_of_step":{"step":"end_step","whose_turn":"each"}},
                "effects":[{"kind":"lose_life","player_ref":"each_opponent","amount":1}]}]},
           {"schema_version":1,"functional_id":"test_ledger","name":"Test Ledger",
            "types":["artifact"],"mana_cost":"{2}",
            "abilities":[{"type":"triggered",
                "event":{"beginning_of_step":{"step":"draw","whose_turn":"yours"}},
                "effects":[{"kind":"draw_card","count":1}]}]}]"#,
    )
    .unwrap();
    assert_eq!(
        text_of(&db, "test_vigil"),
        "At the beginning of your upkeep, you gain 1 life."
    );
    assert_eq!(
        text_of(&db, "test_fount"),
        "At the beginning of each upkeep, you gain 1 life."
    );
    assert_eq!(
        text_of(&db, "test_rally"),
        "At the beginning of combat on your turn, draw a card."
    );
    assert_eq!(
        text_of(&db, "test_dusk"),
        "At the beginning of each end step, each opponent loses 1 life."
    );
    assert_eq!(
        text_of(&db, "test_ledger"),
        "At the beginning of your draw step, draw a card."
    );
}

#[test]
fn a_replacement_reads_as_a_statement_about_entering() {
    // An enters-with-counters card has no clean M19 representative — inline.
    let db = CardDatabase::from_json(
        r#"[{"schema_version":1,"functional_id":"test_hatchling","name":"Test Hatchling",
            "types":["creature"],"subtypes":["Insect"],"mana_cost":"{1}{G}","colors":["green"],
            "power":0,"toughness":0,
            "abilities":[{"type":"enters_with_counters","counter":"plus_one_plus_one","count":2}]}]"#,
    )
    .unwrap();
    assert_eq!(
        text_of(&db, "test_hatchling"),
        "Test Hatchling enters the battlefield with two +1/+1 counters on it."
    );
}

#[test]
fn every_bundled_card_with_rules_generates_text_for_them() {
    // The completeness claim, checked against the whole catalog: a card that has
    // any keyword, combat restriction, ability, spell effect, or attachment grant must
    // produce text — the formatter never silently emits nothing for a card that
    // does something.
    let db = bundled();
    for id in (0..db.len() as u64).map(CardId) {
        let card = db.card(id).unwrap();
        let has_rules = !card.keywords.is_empty()
            || !card.restrictions.is_empty()
            || !card.abilities.is_empty()
            || !card.spell_effects.is_empty()
            // A modal spell's sentences live in its modes, and a spell trait is a
            // sentence in its own right (issue #733).
            || !card.modes.is_empty()
            || !card.spell_traits.is_empty()
            || card.attachment.is_some();
        let text = rules_text(card, sage_engine::scripted_rules_text(&card.functional_id));
        assert_eq!(
            has_rules,
            !text.is_empty(),
            "{} generated {text:?} for {} rules",
            card.name,
            if has_rules { "its" } else { "no" }
        );
    }
}

#[test]
fn issue_606_evasion_and_restriction_cards_generate_their_rules_text() {
    // The restriction vocabulary reads as sentences on the shipped cards that use
    // it, in each of the four subject positions a restriction can appear in: the
    // card's own name, an Aura's host, a chosen target, and a class.
    let db = bundled();

    // A printed restriction is a sentence about the card, not a keyword word.
    assert_eq!(
        text_of(&db, "bristling_boar"),
        "Bristling Boar can't be blocked by more than one creature."
    );
    // Beside a printed keyword, in the fixed keywords-then-restrictions order.
    assert_eq!(
        text_of(&db, "vine_mare"),
        "Hexproof\nVine Mare can't be blocked by black creatures."
    );

    // An Aura's restrictions become one sentence about the enchanted object.
    assert_eq!(
        text_of(&db, "luminous_bonds"),
        "Enchant creature.\nEnchanted creature can't attack and can't block."
    );
    assert_eq!(
        text_of(&db, "aether_tunnel"),
        "Enchant creature.\nEnchanted creature gets +1/+0.\nEnchanted creature can't be blocked."
    );

    // A targeted, a self-referential, and a class-scoped imposition, each carrying
    // its until-end-of-turn duration.
    assert_eq!(
        text_of(&db, "suspicious_bookcase"),
        "Defender\n{3}, {T}: Target creature can't be blocked this turn."
    );
    assert_eq!(
        text_of(&db, "frilled_sea_serpent"),
        "{5}{U}{U}: Frilled Sea Serpent can't be blocked this turn."
    );
    assert_eq!(
        text_of(&db, "tectonic_rift"),
        "Destroy target land.\nCreatures without flying can't block this turn."
    );

    // The one *permission* in the vocabulary (CR 509.1a, issue #739), beside a keyword
    // exactly as an imposed restriction sits: a "can", not a "can't".
    assert_eq!(
        text_of(&db, "ghastbark_twins"),
        "Trample\nGhastbark Twins can block an additional creature each combat."
    );

    // The one *requirement* in the vocabulary (CR 509.1c, issue #739), riding in the
    // same clause as the count-derived pump because it is granted to the same creature —
    // and reading as neither a "can" nor a "can't".
    assert_eq!(
        text_of(&db, "declare_dominance"),
        "Target creature gets +3/+3 and must be blocked by every creature able to do so \
         until end of turn."
    );

    // The opponents-wide mass scope, on the card that introduced it.
    assert_eq!(
        text_of(&db, "plague_mare"),
        "Plague Mare can't be blocked by white creatures.\n\
         When Plague Mare enters the battlefield, creatures your opponents control \
         get -1/-1 until end of turn."
    );
}

#[test]
fn generation_is_deterministic() {
    // Same definition in, same string out — the property the whole approach rests
    // on, since nothing stores the text to compare against.
    let db = bundled();
    for id in (0..db.len() as u64).map(CardId) {
        let card = db.card(id).unwrap();
        let once = rules_text(card, sage_engine::scripted_rules_text(&card.functional_id));
        let twice = rules_text(card, sage_engine::scripted_rules_text(&card.functional_id));
        assert_eq!(once, twice);
    }
}

#[test]
fn a_scripted_card_shows_its_hand_authored_text() {
    // Behavior written in Rust is opaque to the formatter, so a scripted card
    // supplies its own words (ADR 0008 §7) — and they are what a player sees.
    let db = bundled();
    let ogre = db
        .card_id(&FunctionalId::try_from("onakke_ogre".to_string()).unwrap())
        .unwrap();
    let data = db.card(ogre).unwrap();
    assert_eq!(
        rules_text(data, Some("Whenever this attacks, draw a card.")),
        "Whenever this attacks, draw a card."
    );
}

#[test]
fn issue_401_new_m19_cards_generate_their_rules_text() {
    // The M19 slice of issue #401 puts several IR shapes onto *shipped* cards
    // that previously only had inline `test_*` coverage: a P/T Aura, a
    // P/T-and-keyword Aura, damage aimed only at a player, a negative pump, and
    // multi-effect pump/grant spells. Each renders from its structured data.
    let db = bundled();

    // A single-keyword body and a bare lifelink body.
    assert_eq!(text_of(&db, "daybreak_chaplain"), "Lifelink");

    // The first shipped P/T Auras (previously only `test_aegis`/`test_curse`).
    assert_eq!(
        text_of(&db, "knight_s_pledge"),
        "Enchant creature.\nEnchanted creature gets +2/+2."
    );
    assert_eq!(
        text_of(&db, "oakenform"),
        "Enchant creature.\nEnchanted creature gets +3/+3."
    );
    // A P/T *and* keyword Aura in one.
    assert_eq!(
        text_of(&db, "prodigious_growth"),
        "Enchant creature.\nEnchanted creature gets +7/+7.\nEnchanted creature has trample."
    );

    // Burn aimed only at a player exercises the `target player` damage phrase.
    assert_eq!(
        text_of(&db, "lava_axe"),
        "Lava Axe deals 5 damage to target player or planeswalker."
    );

    // A negative pump keeps its signs.
    assert_eq!(
        text_of(&db, "strangling_spores"),
        "Target creature gets -3/-3 until end of turn."
    );

    // A combat trick that pumps *and* grants is one sentence about one creature,
    // because it is one effect with one target slot.
    assert_eq!(
        text_of(&db, "mighty_leap"),
        "Target creature gets +2/+2 and gains flying until end of turn."
    );
    assert_eq!(
        text_of(&db, "sure_strike"),
        "Target creature gets +3/+0 and gains first strike until end of turn."
    );

    // A destroy-and-gain sorcery, and the two trigger shapes on shipped bodies.
    assert_eq!(
        text_of(&db, "lich_s_caress"),
        "Destroy target creature.\nYou gain 3 life."
    );
    assert_eq!(
        text_of(&db, "highland_game"),
        "When Highland Game dies, you gain 2 life."
    );
    assert_eq!(
        text_of(&db, "skeleton_archer"),
        "When Skeleton Archer enters the battlefield, \
         Skeleton Archer deals 1 damage to any target."
    );
    assert_eq!(
        text_of(&db, "pelakka_wurm"),
        "Trample\n\
         When Pelakka Wurm enters the battlefield, you gain 7 life.\n\
         When Pelakka Wurm dies, draw a card."
    );
}

#[test]
fn the_stack_description_speaks_the_same_vocabulary() {
    let db = bundled();
    let forest = db
        .card_id(&FunctionalId::try_from("forest".to_string()).unwrap())
        .unwrap();
    let data = db.card(forest).unwrap();
    let Some(Ability::Activated { effects, .. }) = data.abilities.first() else {
        panic!("the Forest fixture has one activated ability");
    };
    assert_eq!(effects_description(&data.name, effects), "Add {G}.");
    assert_eq!(effects_description(&data.name, &[]), "Ability");
}

/// A card whose only rules are a static ability, built inline: the wording has to
/// be asserted from the IR shape, and the catalog carries whichever shapes real
/// cards happen to use.
pub(super) fn static_text(affects: &str, modification: &str) -> String {
    let json = format!(
        r#"[{{"schema_version":1,"functional_id":"test_lord","name":"Test Lord",
            "types":["creature"],"subtypes":["Elf"],"mana_cost":"{{G}}","colors":["green"],
            "power":1,"toughness":1,
            "abilities":[{{"type":"static","affects":{affects},"modification":{modification}}}]}}]"#
    );
    let db = CardDatabase::from_json(&json).unwrap();
    text_of(&db, "test_lord")
}

#[test]
fn an_anthem_states_what_it_modifies_and_by_how_much() {
    assert_eq!(
        static_text(
            r#"{"scope":"creatures_you_control"}"#,
            r#"{"kind":"power_toughness","power":1,"toughness":1}"#
        ),
        "Creatures you control get +1/+1."
    );
}

#[test]
fn a_lord_says_other_and_names_its_subtype() {
    // "Other" and the subtype both come off the selector, so the sentence cannot
    // claim a scope different from the one the engine applies.
    assert_eq!(
        static_text(
            r#"{"scope":"creatures_you_control","subtype":"Elf","except_this":true}"#,
            r#"{"kind":"power_toughness","power":1,"toughness":1}"#
        ),
        "Other Elves you control get +1/+1."
    );
}

#[test]
fn a_shrinking_static_ability_reads_as_negative() {
    assert_eq!(
        static_text(
            r#"{"scope":"creatures_you_control"}"#,
            r#"{"kind":"power_toughness","power":-1,"toughness":-1}"#
        ),
        "Creatures you control get -1/-1."
    );
}

#[test]
fn a_static_keyword_grant_says_have_not_gains() {
    // A static ability is continuously true; a spell's grant is an event. "Gains"
    // would describe the wrong thing happening.
    assert_eq!(
        static_text(
            r#"{"scope":"creatures_you_control"}"#,
            r#"{"kind":"grant_keyword","keyword":"vigilance"}"#
        ),
        "Creatures you control have vigilance."
    );
}

#[test]
fn irregular_subtype_plurals_read_as_english() {
    // Naive `+s` prints "Elfs", which makes a card look broken. The scope a lord
    // actually applies is unaffected by this — it is the sentence that has to be
    // right, and a wrong plural is the kind of thing nobody notices until a player
    // reads the card.
    assert_eq!(plural("Elf"), "Elves");
    assert_eq!(plural("Dwarf"), "Dwarves");
    assert_eq!(plural("Sphinx"), "Sphinxes");
    assert_eq!(plural("Ox"), "Oxen");
    assert_eq!(plural("Djinn"), "Djinn");
    // The ordinary case still just takes an s.
    assert_eq!(plural("Goblin"), "Goblins");
    assert_eq!(plural("Knight"), "Knights");
}

#[test]
fn issue_604_the_choice_effects_say_what_they_ask_and_what_follows() {
    // Each of the four choice shapes gets words describing the *question*, not just
    // the outcome — a player has to know they will be asked before they cast.
    let db = bundled();

    // Who chooses is stated only when it is not the discarding player, because that
    // is the whole difference between Mind Rot and a hand attack.
    assert_eq!(
        text_of(&db, "mind_rot"),
        "Target player discards two cards."
    );
    assert_eq!(
        text_of(&db, "duress"),
        "You look at target opponent's hand and choose a noncreature, nonland card from \
         it; target opponent discards it."
    );

    // An additional cast cost is stated first, because it is paid first — before the
    // spell is even on the stack (CR 601.2b) — and Sift's discard, which really is an
    // effect, reads as one and comes after its draws.
    assert_eq!(
        text_of(&db, "tormenting_voice"),
        "As an additional cost to cast this spell, discard a card.\nDraw two cards."
    );
    assert_eq!(
        text_of(&db, "sift"),
        "Draw three cards.\nYou discard a card."
    );

    // Scry, look-and-take, and search each name their aftermath, including the
    // random bottom order and the shuffle — the parts a player cannot see coming.
    assert_eq!(
        text_of(&db, "omenspeaker"),
        "When Omenspeaker enters the battlefield, scry two."
    );
    assert_eq!(
        text_of(&db, "militia_bugler"),
        "Vigilance\n\
         When Militia Bugler enters the battlefield, look at the top four cards of your \
         library, you may put up to one creature card with power 2 or less from among \
         them into your hand, then put the rest on the bottom of your library in a \
         random order."
    );
    assert!(text_of(&db, "elvish_rejuvenator").contains("onto the battlefield tapped"));
    assert_eq!(
        text_of(&db, "elvish_clancaller"),
        "Other Elves you control get +1/+1.\n{4}{G}{G}, {T}: Search your library for up \
         to one card with this card's name, put it onto the battlefield, then shuffle."
    );
}

#[test]
fn issue_610_an_optional_effect_reads_as_the_card_prints_it() {
    // Both forms, and the sentence around them. No bundled card uses this effect yet —
    // the three that will each need one more primitive besides — so the shapes are
    // exercised inline (ADR 0009).
    let db = CardDatabase::from_json(
        r#"[
            {"schema_version":1,"functional_id":"test_seer","name":"Test Seer",
             "types":["creature"],"mana_cost":"{2}{U}","power":2,"toughness":2,
             "abilities":[{"type":"triggered","event":"self_attacks",
               "effects":[{"kind":"may","effects":[{"kind":"draw_card","count":1}]}]}]},
            {"schema_version":1,"functional_id":"test_mentor","name":"Test Mentor",
             "types":["creature"],"mana_cost":"{2}{W}","power":2,"toughness":2,
             "abilities":[{"type":"triggered","event":"self_enters_battlefield",
               "effects":[{"kind":"may","cost":{"kind":"mana","mana":"{1}"},
                           "effects":[{"kind":"draw_card","count":1}]},
                          {"kind":"gain_life","player_ref":"controller","amount":2}]}]},
            {"schema_version":1,"functional_id":"test_almsgiver","name":"Test Almsgiver",
             "types":["sorcery"],"mana_cost":"{W}",
             "spell_effects":[{"kind":"may",
                               "effects":[{"kind":"gain_life","player_ref":"controller",
                                           "amount":3}]}]}
        ]"#,
    )
    .unwrap();

    assert_eq!(
        text_of(&db, "test_seer"),
        "Whenever Test Seer attacks, you may draw a card."
    );
    // The costed form is two sentences even mid-clause, because that is how every card
    // with one writes it — and the mandatory effect after it is still joined with "and",
    // so a reader can tell what the payment does and does not buy.
    assert_eq!(
        text_of(&db, "test_mentor"),
        "When Test Mentor enters the battlefield, you may pay {1}. \
         If you do, draw a card and you gain 2 life."
    );
    // A clause whose subject is already "you" is not doubled: "you may gain 3 life",
    // never "you may you gain 3 life".
    assert_eq!(text_of(&db, "test_almsgiver"), "You may gain 3 life.");
}

#[test]
fn issue_744_an_optional_cost_that_is_not_mana_reads_as_the_card_prints_it() {
    // The printed sentence, from a bundled card: the payment is a verb the player does
    // rather than a symbol they pay, and it takes the same "If you do" the mana form
    // takes because that is how the card writes it.
    let db = bundled();
    assert_eq!(
        text_of(&db, "brawl_bash_ogre"),
        "Menace\nWhenever Brawl-Bash Ogre attacks, you may sacrifice another creature. \
         If you do, Brawl-Bash Ogre gets +2/+2 until end of turn."
    );
}

#[test]
fn issue_610_the_optional_question_is_composed_from_the_effects_it_offers() {
    // The words on the button a player answers with come from the same vocabulary as
    // the printed sentence, so the offer and the card can never describe it differently.
    let draw = vec![Effect::DrawCard { count: 1 }];
    assert_eq!(optional_effect_question(None, &draw), "Draw a card?");
    assert_eq!(
        optional_effect_question(Some(&mana_cost("{1}")), &draw),
        "Pay {1}? If you do, draw a card"
    );

    let gain = vec![Effect::GainLife {
        player_ref: PlayerRef::Controller,
        amount: 3,
    }];
    assert_eq!(optional_effect_question(None, &gain), "You gain 3 life?");
    assert_eq!(
        optional_effect_question(Some(&mana_cost("{W}")), &gain),
        "Pay {W}? If you do, you gain 3 life"
    );
}

#[test]
fn issue_725_an_optional_effect_that_targets_reads_as_the_card_prints_it() {
    // The target is named in the printed sentence exactly as a mandatory effect names
    // it, because it is chosen the same way — at announcement — and only the yes-or-no
    // waits for resolution.
    let db = bundled();
    assert_eq!(
        text_of(&db, "gravedigger"),
        "When Gravedigger enters the battlefield, you may return target creature card \
         in your graveyard to its owner's hand."
    );
    assert_eq!(
        text_of(&db, "reclamation_sage"),
        "When Reclamation Sage enters the battlefield, you may destroy target artifact \
         or enchantment."
    );

    // And the question the player answers uses the same words, so the offer and the
    // card cannot describe the same thing two ways.
    assert_eq!(
        optional_effect_question(
            None,
            &[Effect::Destroy {
                target: TargetSpec::AnyArtifactOrEnchantment,
            }]
        ),
        "Destroy target artifact or enchantment?"
    );
}

#[test]
fn issue_605_a_token_creation_reads_as_the_card_prints_it() {
    let db = bundled();

    // The plain case: count, P/T, colour, subtype, card type, "token".
    assert_eq!(
        text_of(&db, "goblin_instigator"),
        "When Goblin Instigator enters the battlefield, you create a 1/1 red Goblin \
         creature token."
    );
    // A colourless token names no colour, and its keywords ride a "with" clause.
    assert_eq!(
        text_of(&db, "aviation_pioneer"),
        "When Aviation Pioneer enters the battlefield, you create a 1/1 Thopter \
         artifact creature token with flying."
    );
    // A plural count, and a token creation sitting beside other effects in one
    // sentence — the spell says all three things it does, in order.
    assert_eq!(
        text_of(&db, "heroic_reinforcements"),
        "You create two 1/1 white Soldier creature tokens.\n\
         Creatures you control get +1/+1 until end of turn.\n\
         Creatures you control gain haste until end of turn."
    );
    // A death trigger creating one.
    assert_eq!(
        text_of(&db, "doomed_dissenter"),
        "When Doomed Dissenter dies, you create a 2/2 black Zombie creature token."
    );
}

#[test]
fn issue_734_a_token_created_attacking_says_so_after_the_noun() {
    let db = bundled();

    // Both entry states at once, each where a card prints it: "tapped" ahead of the
    // noun, "that are attacking" trailing it, after the keyword clause.
    assert_eq!(
        text_of(&db, "leonin_warleader"),
        "Whenever Leonin Warleader attacks, you create two tapped 1/1 white Cat \
         creature tokens with lifelink that are attacking."
    );

    // The singular agrees, and attacking is independent of tapped.
    let inline = CardDatabase::from_json(
        r#"[{"schema_version":1,"functional_id":"test_outrider","name":"Test Outrider",
             "types":["creature"],"subtypes":["Cat"],"mana_cost":"{1}{W}","colors":["white"],
             "power":2,"toughness":2,
             "abilities":[{"type":"triggered","event":"self_attacks","effects":[
               {"kind":"create_token","attacking":true,
                "token":{"name":"Cat","types":["creature"],"subtypes":["Cat"],
                         "colors":["white"],"power":1,"toughness":1}}]}]}]"#,
    )
    .unwrap();
    assert_eq!(
        text_of(&inline, "test_outrider"),
        "Whenever Test Outrider attacks, you create a 1/1 white Cat creature token \
         that's attacking."
    );
}

#[test]
fn issue_608_a_loyalty_ability_reads_as_the_signed_number_the_card_prints() {
    // CR 606.1: a loyalty cost is written as the number in the ability's symbol — `+1`,
    // `0`, `−2` — with the typographic minus a card uses rather than a hyphen. No
    // planeswalker is authorable in the bundled set (every M19 one needs an emblem), so
    // the shape is exercised inline, as ADR 0009 prescribes.
    let inline = CardDatabase::from_json(
        r#"[
            {"schema_version":1,"functional_id":"test_warden","name":"Test Warden",
             "supertypes":["legendary"],"types":["planeswalker"],"subtypes":["Warden"],
             "mana_cost":"{2}{W}{W}","colors":["white"],"loyalty":4,
             "abilities":[
               {"type":"activated","cost":[{"kind":"loyalty","amount":1}],
                "effects":[{"kind":"gain_life","player_ref":"controller","amount":2}]},
               {"type":"activated","cost":[{"kind":"loyalty","amount":0}],
                "effects":[{"kind":"draw_card","count":1}]},
               {"type":"activated","cost":[{"kind":"loyalty","amount":-2}],
                "effects":[{"kind":"deal_damage","target":"any_target","amount":2}]}]}
        ]"#,
    )
    .unwrap();

    assert_eq!(
        text_of(&inline, "test_warden"),
        "+1: You gain 2 life.\n\
         0: Draw a card.\n\
         \u{2212}2: Test Warden deals 2 damage to any target."
    );
}

#[test]
fn a_power_bound_is_spoken_by_every_selector_that_carries_one() {
    // Three selectors learned a power threshold at once, and each has to say so in the
    // position a printed card puts it: trailing the class it narrows. Asserted against
    // the real cards, so the sentence and the selector the engine applies cannot drift.
    let db = bundled();
    assert_eq!(
        text_of(&db, "mentor_of_the_meek"),
        "Whenever another creature you control with power 2 or less enters the \
         battlefield, you may pay {1}. If you do, draw a card."
    );
    assert_eq!(
        text_of(&db, "colossal_majesty"),
        "At the beginning of your upkeep, if you control a creature with power 4 or \
         greater, draw a card."
    );
    assert_eq!(
        text_of(&db, "ghirapur_guide"),
        "{2}{G}: Target creature you control can't be blocked by creatures with \
         power 2 or less this turn."
    );
}

#[test]
fn a_mass_tap_names_whose_creatures_and_the_step_they_sit_out() {
    // Two sentences because the card prints two, and the verb agrees with the subject the
    // `player_ref` names — "target player controls", never "target player control".
    let db = bundled();
    assert_eq!(
        text_of(&db, "sleep"),
        "Tap all creatures target player controls. Those creatures don't untap during \
         their next untap step."
    );
}

#[test]
fn a_control_change_says_the_theft_the_untap_and_the_keyword_it_grants() {
    // Three sentences from one effect, because the card does three things to one
    // creature. Each is stated only when the effect actually does it, so the sentence and
    // the single target group stay the same shape.
    let db = bundled();
    assert_eq!(
        text_of(&db, "act_of_treason"),
        "Gain control of target creature until end of turn. Untap it. \
         It gains haste until end of turn."
    );
}

#[test]
fn a_player_subject_static_says_what_is_true_of_you() {
    // The only ability the formatter composes whose subject is a person rather than an
    // object, so it is the only one with no source name in it at all.
    let db = bundled();
    assert_eq!(
        text_of(&db, "reliquary_tower"),
        "You have no maximum hand size.\n{T}: Add {C}."
    );
    // The second of the shape, and the one that is a permission rather than a limit
    // lifted — still one sentence about a person, with no source name in it.
    assert_eq!(
        text_of(&db, "crucible_of_worlds"),
        "You may play lands from your graveyard."
    );
}

#[test]
fn an_additional_cost_states_what_it_takes_before_what_the_spell_does() {
    // A cost is stated first because that is the order it happens in: it is paid while
    // the spell is cast (CR 601.2b), and a player who cannot pay never reaches the rest.
    let db = bundled();
    assert_eq!(
        text_of(&db, "blood_divination"),
        "As an additional cost to cast this spell, sacrifice a creature.\nDraw three cards."
    );
    // On a creature spell the keyword line still leads, and the cost follows it — the
    // clause order is the card's, not the effect list's.
    assert_eq!(
        text_of(&db, "demon_of_catastrophes"),
        "Flying, trample\nAs an additional cost to cast this spell, sacrifice a creature."
    );
}

#[test]
fn a_life_gained_condition_states_its_threshold_only_when_there_is_one() {
    // "if you gained life this turn" and "if you gained five or more life this turn"
    // are one condition with a number, and the number is written only when it is more
    // than the "any" a card leaves unsaid.
    let db = bundled();
    assert_eq!(
        text_of(&db, "regal_bloodlord"),
        "Flying\nAt the beginning of your end step, if you gained life this turn, \
         you create a 2/2 black Bat creature token with flying."
    );
    assert_eq!(
        text_of(&db, "resplendent_angel"),
        "Flying\nAt the beginning of each end step, if you gained five or more life \
         this turn, you create a 4/4 white Angel creature token with flying and vigilance."
    );
}

#[test]
fn issue_727_a_condition_about_the_source_names_the_creature_not_its_controller() {
    // Every other intervening if says "you"; this one is about the permanent the ability
    // is on, so the sentence has to say so — and the effect it gates names the card,
    // because a self-referential effect chose no target to name instead.
    let db = bundled();
    assert_eq!(
        text_of(&db, "inferno_hellion"),
        "Trample\nAt the beginning of each end step, if this creature attacked or \
         blocked this turn, shuffle Inferno Hellion into its owner's library."
    );
}

#[test]
fn issue_727_a_static_condition_about_the_source_reads_as_a_standing_statement() {
    // The `as long as …` clause of a continuous ability, and — because the subject is the
    // card's own name rather than a class — a verb that agrees with it in the singular.
    let db = bundled();
    assert_eq!(
        text_of(&db, "palladia_mors_the_ruiner"),
        "Flying, vigilance, trample\nPalladia-Mors, the Ruiner has hexproof as long as \
         it hasn't dealt damage yet."
    );
    assert_eq!(
        text_of(&db, "grasping_scoundrel"),
        "Grasping Scoundrel gets +1/+0 as long as it's attacking."
    );
}

#[test]
fn the_watched_draw_attack_and_activation_each_have_words() {
    // A draw and an activation, from the two real cards that watch them. The activation
    // sentence states the CR 605.3a exclusion the condition enforces structurally,
    // because a player reading the card has to know that a tapped land will give the
    // watcher nothing.
    let db = bundled();
    assert_eq!(
        text_of(&db, "psychic_corrosion"),
        "Whenever you draw a card, each opponent mills two cards."
    );
    assert_eq!(
        text_of(&db, "runic_armasaur"),
        "Whenever an opponent activates a nonmana ability of a creature or land, \
         you may draw a card."
    );

    // The watching attack condition and the keyword filter have no authorable card
    // yet — the M19 cards that want them need vocabulary that does not exist — so
    // their words are asserted from an inline definition rather than by inventing a
    // card for the catalog.
    let inline = CardDatabase::from_json(
        r#"[
        {"schema_version":1,"functional_id":"test_sky_watch","name":"Test Sky Watch",
         "types":["enchantment"],"mana_cost":"{2}{U}","colors":["blue"],
         "abilities":[{"type":"triggered",
            "event":{"permanent_attacks":{"scope":"any_creature","keyword":"flying"}},
            "effects":[{"kind":"draw_card","count":1}]}]},
        {"schema_version":1,"functional_id":"test_small_guard","name":"Test Small Guard",
         "types":["enchantment"],"mana_cost":"{1}{W}","colors":["white"],
         "abilities":[{"type":"triggered",
            "event":{"permanent_enters":{"scope":"creatures_you_control","except_this":true,
                "keyword":"defender","max_power":2}},
            "effects":[{"kind":"gain_life","player_ref":"controller","amount":1}]}]},
        {"schema_version":1,"functional_id":"test_watch_all","name":"Test Watch All",
         "types":["enchantment"],"mana_cost":"{W}","colors":["white"],
         "abilities":[{"type":"triggered",
            "event":{"ability_activated":{}},
            "effects":[{"kind":"gain_life","player_ref":"controller","amount":1}]}]}
        ]"#,
    )
    .unwrap();

    assert_eq!(
        text_of(&inline, "test_sky_watch"),
        "Whenever a creature with flying attacks, draw a card."
    );
    // Two qualifiers on one class join rather than repeating the preposition.
    assert_eq!(
        text_of(&inline, "test_small_guard"),
        "Whenever another creature you control with defender and power 2 or less \
         enters the battlefield, you gain 1 life."
    );
    // An activation watcher that names no types and no seat drops both clauses rather
    // than inventing them.
    assert_eq!(
        text_of(&inline, "test_watch_all"),
        "Whenever a player activates a nonmana ability, you gain 1 life."
    );
}

#[test]
fn issue_742_a_subtype_evasion_reads_as_a_permission() {
    // The one restriction stated backwards: what it names is what may block, so the
    // sentence says "except by" rather than listing what may not. No bundled card
    // prints it yet, so the predicate is asked directly — the same string every subject
    // position composes around.
    assert_eq!(
        restriction_predicate(&CombatRestriction::CantBeBlockedExceptBy(
            "Spirit".to_string()
        )),
        "can't be blocked except by Spirits"
    );
}

#[test]
fn issue_742_detection_tower_states_its_mana_and_its_permission() {
    let db = bundled();
    assert_eq!(
        text_of(&db, "detection_tower"),
        "{T}: Add {C}.\n{1}, {T}: Spells and abilities you control may target as though \
         hexproof were not there this turn."
    );
}

#[test]
fn a_layer_six_change_says_what_is_lost_before_what_is_gained() {
    // One clause for one printed sentence, in the order the effect applies it. Gargoyle
    // Sentinel is the whole shape on a real card; the halves M19 does not print — losing
    // all abilities, and losing without gaining — are exercised inline (ADR 0009).
    let db = bundled();
    assert_eq!(
        text_of(&db, "gargoyle_sentinel"),
        "Defender\n{3}: Gargoyle Sentinel loses defender and gains flying until end of turn."
    );

    let inline = CardDatabase::from_json(
        r#"[
            {"schema_version":1,"functional_id":"test_blank","name":"Test Blank",
             "types":["creature"],"subtypes":["Shapeshifter"],"mana_cost":"{2}","colors":["blue"],
             "power":2,"toughness":2,
             "abilities":[{"type":"activated","cost":[{"kind":"mana","mana":"{1}"}],
               "effects":[{"kind":"alter_abilities_self","lose_all":true,"gain":["hexproof"]}]}]},
            {"schema_version":1,"functional_id":"test_grounded","name":"Test Grounded",
             "types":["creature"],"subtypes":["Bird"],"mana_cost":"{1}","colors":["white"],
             "power":1,"toughness":1,"keywords":["flying","vigilance"],
             "abilities":[{"type":"activated","cost":[{"kind":"mana","mana":"{1}"}],
               "effects":[{"kind":"alter_abilities_self","lose":["flying","vigilance"]}]}]}
        ]"#,
    )
    .unwrap();
    assert_eq!(
        text_of(&inline, "test_blank"),
        "{1}: Test Blank loses all abilities and gains hexproof until end of turn."
    );
    // Two keywords lost read as a list, and a clause that only subtracts says only that.
    assert_eq!(
        text_of(&inline, "test_grounded"),
        "Flying, vigilance\n{1}: Test Grounded loses flying and vigilance until end of turn."
    );
}

#[test]
fn issue_723_a_graveyard_ability_names_its_own_card_and_the_zone_it_works_in() {
    // Reassembling Skeleton (bundled). Both halves of the sentence are facts about the
    // source rather than anything an author chose: the card names itself, and the zone is
    // the one the ability functions in (CR 113.6). The same string labels the dock button
    // the player clicks, so the card and the offer cannot describe it differently.
    let db = bundled();
    assert_eq!(
        text_of(&db, "reassembling_skeleton"),
        "{1}{B}: Return Reassembling Skeleton from your graveyard to the battlefield tapped."
    );

    // The graveyard→hand destination, on an Aura whose grant is stated beneath it: the
    // ability is about the card in the graveyard, the attachment about the creature it
    // enchants once the card is back and cast.
    assert_eq!(
        text_of(&db, "talons_of_wildwood"),
        "{2}{G}: Return Talons of Wildwood from your graveyard to your hand.\n\
         Enchant creature.\n\
         Enchanted creature gets +1/+1.\n\
         Enchanted creature has trample."
    );
}

/// A colour named as a permanent enters reads as an "As …" statement about entering
/// (CR 614.12) — never "When …", which would describe a trigger that goes on the stack —
/// and the ability that reads the answer back calls it "the chosen color" rather than
/// naming a colour the printed card cannot know.
#[test]
fn issue_738_an_entry_choice_and_the_ability_that_reads_it_back() {
    let db = bundled();
    assert_eq!(
        text_of(&db, "diamond_mare"),
        "As Diamond Mare enters the battlefield, choose a color.\n\
         Whenever you cast a spell of the chosen color, you gain 1 life."
    );
}

#[test]
fn issue_723_a_spell_states_what_it_does_before_the_ability_it_carries() {
    // The one card that prints both an instant's own effect and an ability — a trigger
    // that functions from the graveyard the instant lands in. Casting it is what happens
    // first, so it is said first, and the ability that watches from the graveyard reads
    // as the rider it is.
    let db = bundled();
    assert_eq!(
        text_of(&db, "spit_flame"),
        "Spit Flame deals 4 damage to target creature.\n\
         Whenever a Dragon you control enters the battlefield, you may pay {R}. \
         If you do, return Spit Flame from your graveyard to your hand."
    );
}

/// An effect whose two target slots do not share a spec reads as one sentence naming
/// both, in slot order (CR 701.12) — the two nouns are exactly what the player will be
/// asked for, so a sentence naming only one of them would describe a different card.
#[test]
fn issue_737_a_two_target_effect_names_both_of_its_targets() {
    let db = bundled();
    assert_eq!(
        text_of(&db, "rabid_bite"),
        "Target creature you control deals damage equal to its power to \
         target creature an opponent controls."
    );

    // The mutual form is the printed verb, which says the power reading on its own. No
    // M19 card prints it, so it is exercised inline (ADR 0009).
    let inline = CardDatabase::from_json(
        r#"[{"schema_version":1,"functional_id":"test_pounce","name":"Test Pounce",
            "types":["instant"],"mana_cost":"{1}{G}","colors":["green"],
            "spell_effects":[{"kind":"fight","dealer":"any_creature_you_control",
              "dealt_to":"any_creature_an_opponent_controls","mutual":true}]}]"#,
    )
    .unwrap();
    assert_eq!(
        text_of(&inline, "test_pounce"),
        "Target creature you control fights target creature an opponent controls."
    );
}

/// An activation cost the player picks the payment for is written in the cost line, as
/// the card writes it — `Sacrifice another creature`, `Sacrifice a Goblin`, `Discard a
/// card` — and the same words label the slot the choice is answered on, so a player is
/// asked the question the card poses rather than a paraphrase of it.
#[test]
fn issue_721_an_activation_cost_states_what_the_player_must_spend() {
    let db = bundled();
    assert_eq!(
        text_of(&db, "ravenous_harpy"),
        "Flying\n{B}, Sacrifice another creature: Put a +1/+1 counter on Ravenous Harpy \
         and you gain 1 life."
    );
    // A subtype names the class on its own: a Goblin is a Goblin whatever else it is, and
    // the Trashmaster is one, so with no *another* it is a legal payment for its own cost.
    assert_eq!(
        text_of(&db, "goblin_trashmaster"),
        "Other Goblins you control get +1/+1.\n\
         {1}{R}, Sacrifice a Goblin: Destroy target artifact."
    );
    assert_eq!(
        text_of(&db, "dismissive_pyromancer"),
        "{T}, Discard a card: Draw a card.\n\
         {2}{R}, Sacrifice this permanent: Dismissive Pyromancer deals 4 damage to \
         target creature."
    );
}

/// The two cost shapes issue #721 finishes each state what they take, in the card's own
/// words — and the amount that reads a payment names it where a card names it.
#[test]
fn issue_721_a_costs_size_and_the_amount_that_reads_it_are_both_stated() {
    let db = bundled();
    // Exiling from a graveyard is a cost like any other, written in the cost line beside
    // the mana symbols.
    assert_eq!(
        text_of(&db, "graveyard_marshal"),
        "{2}{B}, Exile a creature card from your graveyard: \
         You create a tapped 2/2 black Zombie creature token."
    );
    // A count greater than one reads as the card writes it — "two artifacts", not two
    // sentences each taking one — and the trigger names the class of spell it watches.
    assert_eq!(
        text_of(&db, "sai_master_thopterist"),
        "Whenever you cast an artifact spell, you create a 1/1 Thopter artifact creature \
         token with flying.\n\
         {1}{U}, Sacrifice two artifacts: Draw a card."
    );
    // And the payment amount, named as a possessive about a creature that is gone by the
    // time the sentence takes effect.
    assert_eq!(
        text_of(&db, "thud"),
        "As an additional cost to cast this spell, sacrifice a creature.\n\
         Thud deals damage equal to the sacrificed creature's power to any target."
    );
}

/// A created replacement reads as the sentence a card prints it in: the event, the turn
/// it lasts, the qualifier on the event, and what happens instead (CR 614.1b). The
/// keyword line above it is the flash the card is held up with (CR 702.8).
#[test]
fn issue_731_a_created_replacement_reads_as_the_next_time_this_turn() {
    let db = bundled();
    assert_eq!(
        text_of(&db, "mistcaller"),
        "Flash\n\
         Sacrifice this permanent: The next time a nontoken creature would enter the \
         battlefield this turn without being cast, exile it instead."
    );
}

/// The three M19 mechanics of issue #748, each stated in the words its card prints.
///
/// A keyword line on its own is the whole of a vanilla creature with flash; a
/// variable-arity restriction reads its group in **subject** position rather than after
/// the "each of" an object position takes; and a mana-value filter names the number,
/// because a spec that carries one cannot be described by a fixed class name.
#[test]
fn issue_748_flash_variable_arity_and_a_mana_value_filter_read_as_their_cards() {
    let db = bundled();
    assert_eq!(text_of(&db, "hired_blade"), "Flash");
    assert_eq!(
        text_of(&db, "ghostform"),
        "Up to two target creatures can't be blocked this turn."
    );
    assert_eq!(
        text_of(&db, "isolate"),
        "Exile target permanent with mana value 1."
    );
    // The same effect at its default count still reads as one creature, so the field
    // that made the sentence plural changed no card that leaves it out.
    assert_eq!(
        text_of(&db, "suspicious_bookcase"),
        "Defender\n{3}, {T}: Target creature can't be blocked this turn."
    );
}

/// A prevention shield reads as the one sentence Root Snare prints (CR 615.1): the verb,
/// the class of damage, and the turn it lasts. An unfiltered shield drops the word that
/// narrows it, exercised inline because M19 prints no `prevent all damage` (ADR 0009).
#[test]
fn issue_736_a_prevention_shield_reads_as_prevent_all_damage_this_turn() {
    let db = bundled();
    assert_eq!(
        text_of(&db, "root_snare"),
        "Prevent all combat damage that would be dealt this turn."
    );
    let inline = CardDatabase::from_json(
        r#"[
            {"schema_version":1,"functional_id":"test_ward","name":"Test Ward",
             "types":["instant"],"mana_cost":"{W}","colors":["white"],
             "spell_effects":[{"kind":"prevent_damage"}]}
        ]"#,
    )
    .unwrap();
    assert_eq!(
        text_of(&inline, "test_ward"),
        "Prevent all damage that would be dealt this turn."
    );
}

/// **A modal spell renders as a printed card writes it** (CR 700.2, issue #733): a
/// `Choose one —` header and one bulleted line per mode, each line the mode's own
/// sentences.
///
/// The split into lines is the same split the dock's numbered rows use, so these are
/// literally the words a player picks between.
#[test]
fn issue_733_a_modal_spell_prints_its_bullets() {
    let db = bundled();
    assert_eq!(
        text_of(&db, "cleansing_nova"),
        "Choose one —\n• Destroy all creatures.\n• Destroy all artifacts and enchantments."
    );
}

/// **X renders as X** (issue #733). The card prints the letter; the value belongs to a
/// particular cast and is stated on the stack entry, not here. The threshold clauses
/// follow the sentence they qualify, where the printed card puts them.
#[test]
fn issue_733_x_renders_as_x_with_its_threshold_clauses() {
    let db = bundled();
    assert_eq!(
        text_of(&db, "banefire"),
        "Banefire deals X damage to any target.\n\
         If X is 5 or more, Banefire can't be countered.\n\
         If X is 5 or more, the damage Banefire deals can't be prevented."
    );
}

/// A cost modifier states the class of spell, whose casts it reaches, and which way the
/// number goes (CR 601.2f). The power bound trails the class, where a card prints it —
/// the same place the mass effect on the line below puts its own.
#[test]
fn issue_735_a_cost_modifier_states_the_class_and_the_amount() {
    let db = bundled();
    assert_eq!(
        text_of(&db, "goreclaw_terror_of_qal_sisma"),
        "Creature spells with power 4 or greater you cast cost {2} less to cast.\n\
         Whenever Goreclaw, Terror of Qal Sisma attacks, creatures you control with \
         power 4 or greater get +1/+1 until end of turn and creatures you control with \
         power 4 or greater gain trample until end of turn."
    );
}

#[test]
fn issue_740_a_granted_ability_is_quoted_on_the_card_that_grants_it() {
    // CR 613.1f, three ways. An Aura on a land handing over an activated ability, an Aura
    // on a creature handing over a triggered one, and a spell doing the same for a turn.
    //
    // The granted ability is in quotation marks and worded against **its host** — "this
    // creature", not the name of the card granting it — because that is whose ability it
    // becomes. It is composed by the same `ability_text` that words a printed ability and
    // labels the dock button, so a granted activation and a printed one are one string.
    let db = bundled();
    assert_eq!(
        text_of(&db, "gift_of_paradise"),
        "When Gift of Paradise enters the battlefield, you gain 2 life.\n\
         Enchant land.\n\
         Enchanted land has \"{T}: Add two mana of any one color.\""
    );
    assert_eq!(
        text_of(&db, "infernal_scarring"),
        "Enchant creature.\n\
         Enchanted creature gets +2/+0.\n\
         Enchanted creature has \"When this creature dies, draw a card.\""
    );
    // One sentence, because it is one effect on one target: the numbers and the ability
    // are granted together or not at all.
    assert_eq!(
        text_of(&db, "abnormal_endurance"),
        "Target creature gets +2/+0 and gains \"When this creature dies, return this \
         creature from your graveyard to the battlefield tapped.\" until end of turn."
    );

    // The two phrasings of a chosen-colour mana clause are different decisions, so they
    // are different sentences: Manalith asks once per point, the Aura above asks once.
    assert_eq!(text_of(&db, "manalith"), "{T}: Add one mana of any color.");
}

/// Alpine Moon (issue #738/#743): the naming clause gives "the chosen name" its referent,
/// and both halves of the continuous effect are composed from the same selector the
/// engine evaluates — including the quoted ability it hands out, written by the same
/// composer that writes it once the land has it.
#[test]
fn a_named_card_and_a_static_that_reaches_an_opponents_lands() {
    let db = bundled();
    assert_eq!(
        text_of(&db, "alpine_moon"),
        "As Alpine Moon enters the battlefield, choose a nonbasic land card name.\n\
         Lands your opponents control with the chosen name lose all abilities.\n\
         Lands your opponents control with the chosen name have \"{T}: Add one mana of \
         any color.\""
    );
}

/// A card with **two faces** (CR 712) generates a sentence per face, from that face's
/// own ability IR. The front's transform line carries the authored timing restriction
/// (CR 602.5d), and the back's four loyalty abilities come off a face that has no
/// keywords, no spell ability, and no cost of its own.
#[test]
fn issue_747_both_faces_of_a_transforming_card_generate_their_own_text() {
    let db = bundled();
    let front = text_of(&db, "nicol_bolas_the_ravager");
    assert!(front.starts_with("Flying\n"), "{front}");
    assert!(front.contains("each opponent discards a card."), "{front}");
    assert!(
        front.ends_with("Activate only as a sorcery."),
        "the authored timing restriction is stated: {front}"
    );

    let data = db
        .card(
            db.card_id(&FunctionalId::try_from("nicol_bolas_the_ravager".to_string()).unwrap())
                .unwrap(),
        )
        .unwrap();
    let back = back_face_rules_text(data.back_face.as_deref().unwrap());
    assert_eq!(
        back.lines().count(),
        4,
        "one line per loyalty ability: {back}"
    );
    assert!(back.starts_with("+2: Draw two cards."), "{back}");
    assert!(
        back.contains("target creature or planeswalker"),
        "the new target spec has words: {back}"
    );
    assert!(
        back.contains("bottom card of target player's library"),
        "{back}"
    );
    // The back face's sentences name *it*, not the card's front face.
    assert!(back.contains("Nicol Bolas, the Arisen"), "{back}");
    assert!(!back.contains("the Ravager"), "{back}");
}
#[test]
fn issue_734_a_copy_reads_as_the_copy_it_is() {
    let db = bundled();
    // CR 707.5, the "you may" and all: one sentence, and the class it may name.
    assert_eq!(
        text_of(&db, "mirror_image"),
        "You may have Mirror Image enter the battlefield as a copy of a creature you control."
    );
    // CR 614.12 + CR 707.2c: the choice and the continuous effect are the two sentences
    // the card prints, in that order — the enchant line follows, as it does on every Aura.
    assert_eq!(
        text_of(&db, "metamorphic_alteration"),
        "As Metamorphic Alteration enters the battlefield, choose a creature. Enchanted \
         creature is a copy of the chosen creature.\nEnchant creature."
    );
    // CR 603.7 + CR 707.10c: the `next` and the `this turn` are facts about the delayed
    // ability rather than authored words, and the re-target is its own sentence.
    assert_eq!(
        text_of(&db, "doublecast"),
        "When you next cast an instant or sorcery spell this turn, copy that spell. You \
         may choose new targets for the copy."
    );
}

#[test]
fn issue_727_a_count_of_names_and_the_shortest_payoff_a_card_can_print() {
    let db = bundled();
    // Two sentences, and the second is the whole reason the card exists. "Demons" is the
    // subtype standing alone as its own noun — a card says "four or more Demons", never
    // "four or more Demon permanents" — and "with different names" trails it where the
    // printed clause sits.
    assert_eq!(
        text_of(&db, "liliana_s_contract"),
        "When Liliana's Contract enters the battlefield, draw four cards and you lose 4 \
         life.\nAt the beginning of your upkeep, if you control four or more Demons with \
         different names, you win the game."
    );
    // The same composer, one permanent and no names clause: a count of one cannot have
    // different names and does not claim to.
    assert_eq!(
        text_of(&db, "kargan_dragonrider"),
        "Kargan Dragonrider has flying as long as you control a Dragon."
    );
}

#[test]
fn issue_722_a_reflexive_trigger_reads_as_the_sentence_it_is() {
    let db = bundled();
    // CR 603.11: the last sentence is an ability this resolution creates, and it is
    // composed against "it" — the pronoun the printed card uses for a card nobody has
    // chosen yet.
    assert_eq!(
        text_of(&db, "vivien_s_invocation"),
        "Look at the top seven cards of your library, you may put up to one creature card \
         from among them onto the battlefield, then put the rest on the bottom of your \
         library in a random order.\nWhen a creature is put onto the battlefield this way, \
         it deals damage equal to its power to target creature an opponent controls."
    );
}

#[test]
fn issue_706_two_cards_the_vocabulary_could_already_say() {
    let db = bundled();
    // An optional effect that targets: the class belongs to the effect the `may` wraps,
    // and the sentence says so in the order the card prints it.
    assert_eq!(
        text_of(&db, "riddlemaster_sphinx"),
        "Flying\nWhen Riddlemaster Sphinx enters the battlefield, you may return target \
         creature an opponent controls to its owner's hand."
    );
    // A watcher of the whole board's attackers, not of its controller's.
    assert_eq!(
        text_of(&db, "windreader_sphinx"),
        "Flying\nWhenever a creature with flying attacks, you may draw a card."
    );
}

#[test]
fn issue_706_a_target_relative_to_the_attacker_says_what_it_is_relative_to() {
    let db = bundled();
    // "Another" goes before the word target, where the card prints it.
    assert_eq!(
        text_of(&db, "pegasus_courser"),
        "Flying\nWhenever Pegasus Courser attacks, another target attacking creature \
         gains flying until end of turn."
    );
    assert_eq!(
        text_of(&db, "star_crowned_stag"),
        "Whenever Star-Crowned Stag attacks, tap target creature defending player controls."
    );
}

#[test]
fn issue_706_an_aura_that_taps_its_host_and_keeps_it_there() {
    let db = bundled();
    // The enchant line, the trigger that names its host without targeting it, and the
    // rule the Aura modifies — three sentences, in the order the card prints them.
    assert_eq!(
        text_of(&db, "waterknot"),
        "When Waterknot enters the battlefield, tap enchanted creature.\nEnchanted \
         creature doesn't untap during its controller's untap step.\nEnchant creature."
    );
}

#[test]
fn issue_706_a_toll_reads_as_the_consequence_and_the_way_out_of_it() {
    let db = bundled();
    // The card prints the consequence first and the payment as the way to avoid it,
    // which is the other order from "you may pay {1}. If you do, …".
    assert_eq!(
        text_of(&db, "rupture_spire"),
        "Rupture Spire enters the battlefield tapped.\nWhen Rupture Spire enters the \
         battlefield, sacrifice it unless you pay {1}.\n{T}: Add one mana of any color."
    );
}

#[test]
fn issue_706_becoming_the_target_says_what_kind_of_object_and_whose() {
    let db = bundled();
    // "A spell or ability an opponent controls" — both narrowings, in the order a card
    // prints them.
    assert_eq!(
        text_of(&db, "thorn_lieutenant"),
        "Whenever Thorn Lieutenant becomes the target of a spell or ability an opponent \
         controls, you create a 1/1 green Elf Warrior creature token.\n{5}{G}: Thorn \
         Lieutenant gets +4/+4 until end of turn."
    );
    // "A spell", and nobody's in particular: the drawback the card is priced for.
    assert_eq!(
        text_of(&db, "departed_deckhand"),
        "Departed Deckhand can't be blocked except by Spirits.\nWhenever Departed Deckhand \
         becomes the target of a spell, sacrifice it.\n{3}{U}: Another target creature \
         you control can't be blocked except by Spirits this turn."
    );
}

#[test]
fn issue_706_animating_reads_as_what_it_becomes_and_for_how_long() {
    let db = bundled();
    assert_eq!(
        text_of(&db, "skilled_animator"),
        "When Skilled Animator enters the battlefield, target artifact you control \
         becomes a creature with base power and toughness 5/5 for as long as Skilled \
         Animator remains on the battlefield."
    );
    // An Equipment's grants are one sentence, and the type sits among them.
    assert_eq!(
        text_of(&db, "sigiled_sword_of_valeron"),
        "Equipped creature gets +2/+0.\nEquipped creature has vigilance.\nEquipped \
         creature is a Knight in addition to its other types.\nEquipped creature has \
         \"Whenever this creature attacks, you create a tapped 2/2 white Knight creature \
         token with vigilance that's attacking.\"\n{3}: Attach Sigiled Sword of Valeron \
         to target creature you control."
    );
}

#[test]
fn issue_706_an_extra_turn_reads_as_the_card_prints_it() {
    let db = bundled();
    assert_eq!(
        text_of(&db, "magistrate_s_scepter"),
        "{4}, {T}: Put a charge counter on Magistrate's Scepter.\n{T}, Remove three \
         charge counters from this permanent: Take an extra turn after this one."
    );
}

#[test]
fn issue_706_reanimation_reads_as_two_sentences() {
    let db = bundled();
    // The card prints the continuous half as its own sentence about "that creature",
    // because by then the permanent exists and can be spoken of.
    assert_eq!(
        text_of(&db, "rise_from_the_grave"),
        "Put target creature card in a graveyard onto the battlefield under your control. \
         That creature is a black Zombie in addition to its other colors and types."
    );
}

#[test]
fn issue_706_becoming_something_else_and_the_once_a_turn_line() {
    let db = bundled();
    assert_eq!(
        text_of(&db, "ursine_champion"),
        "{5}{G}: Ursine Champion gets +3/+3 until end of turn and Ursine Champion \
         becomes a Bear Berserker until end of turn. Activate only once each turn."
    );
    assert_eq!(
        text_of(&db, "chromium_the_mutable"),
        "Flash, flying\nChromium, the Mutable can't be countered.\nDiscard a card: \
         Chromium, the Mutable becomes a Human with base power and toughness 1/1 until \
         end of turn and Chromium, the Mutable loses all abilities and gains hexproof \
         until end of turn and Chromium, the Mutable can't be blocked this turn."
    );
}

#[test]
fn issue_706_an_exile_that_comes_back_and_an_exchange() {
    let db = bundled();
    assert_eq!(
        text_of(&db, "hieromancer_s_cage"),
        "When Hieromancer's Cage enters the battlefield, exile target nonland permanent \
         an opponent controls until Hieromancer's Cage leaves the battlefield."
    );
    assert_eq!(
        text_of(&db, "switcheroo"),
        "Exchange control of two target creatures."
    );
}

#[test]
fn issue_706_a_sentence_about_a_choice_the_one_before_it_made() {
    let db = bundled();
    assert_eq!(
        text_of(&db, "radiating_lightning"),
        "Radiating Lightning deals 3 damage to target player.\nRadiating Lightning deals \
         1 damage to each creature that player controls."
    );
}

#[test]
fn issue_706_the_x_it_was_cast_for_and_the_damage_it_survives() {
    let db = bundled();
    assert_eq!(
        text_of(&db, "hungering_hydra"),
        "Hungering Hydra can't be blocked by more than one creature.\n\
         Hungering Hydra enters the battlefield with X +1/+1 counters on it.\n\
         Whenever Hungering Hydra is dealt damage, put that many +1/+1 counters on \
         Hungering Hydra."
    );
}

#[test]
fn issue_706_an_unless_somebody_else_answers() {
    let db = bundled();
    assert_eq!(
        text_of(&db, "demanding_dragon"),
        "Flying\nWhen Demanding Dragon enters the battlefield, Demanding Dragon \
         deals 5 damage to target opponent unless that player sacrifices a creature."
    );
}

#[test]
fn issue_706_a_counter_placed_as_it_enters_and_the_state_that_undoes_it() {
    let db = bundled();
    assert_eq!(
        text_of(&db, "phylactery_lich"),
        "Indestructible\nAs Phylactery Lich enters the battlefield, put a phylactery \
         counter on an artifact you control.\nWhen you control no permanents with \
         phylactery counters on them, sacrifice it."
    );
}

#[test]
fn issue_706_a_permission_and_a_block_read_as_the_cards_print_them() {
    let db = bundled();
    assert_eq!(
        text_of(&db, "vivien_s_jaguar"),
        "Reach\n{2}{G}: Return Vivien's Jaguar from your graveyard to your hand. \
         Activate only if you control a Vivien planeswalker."
    );
    assert_eq!(
        text_of(&db, "dwindle"),
        "Whenever enchanted creature blocks, destroy it.\nEnchant creature.\n\
         Enchanted creature gets -6/+0."
    );
}

#[test]
fn issue_706_a_class_named_as_one_choice_and_a_spell_that_stands() {
    let db = bundled();
    assert_eq!(
        text_of(&db, "tezzeret_s_gatebreaker"),
        "When Tezzeret's Gatebreaker enters the battlefield, look at the top five cards \
         of your library, you may put up to one blue or artifact card from among them \
         into your hand, then put the rest on the bottom of your library in a random \
         order.\n{5}{U}, {T}, Sacrifice this permanent: Creatures you control can't be \
         blocked this turn."
    );
    assert_eq!(
        text_of(&db, "lightning_mare"),
        "Lightning Mare can't be blocked by blue creatures.\n\
         Lightning Mare can't be countered.\n\
         {1}{R}: Lightning Mare gets +1/+0 until end of turn."
    );
}
