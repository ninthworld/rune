//! Generated rules-text tests: one per IR shape the formatter gives words to.

#![allow(clippy::panic, clippy::unwrap_used)]

use super::*;
use sage_engine::{CardDatabase, CardId, FunctionalId};

/// The bundled catalog, whose definitions cover every IR construct the engine
/// has: the generated text is asserted against real cards, not toy structs.
fn bundled() -> CardDatabase {
    CardDatabase::bundled().unwrap()
}

/// The generated text of the card with this authored identity.
fn text_of(db: &CardDatabase, functional_id: &str) -> String {
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
         Viashino Pyromancer deals 2 damage to target player."
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
             "spell_effects":[{"kind":"deal_damage","affects":"each_creature","amount":2}]},
            {"schema_version":1,"functional_id":"test_slagstorm","name":"Test Slagstorm",
             "types":["sorcery"],"mana_cost":"{2}{R}","colors":["red"],
             "spell_effects":[{"kind":"deal_damage","affects":"creatures_your_opponents_control","amount":3}]},
            {"schema_version":1,"functional_id":"test_recoil","name":"Test Recoil",
             "types":["sorcery"],"mana_cost":"{R}","colors":["red"],
             "spell_effects":[{"kind":"deal_damage","player_ref":"controller","amount":1}]},
            {"schema_version":1,"functional_id":"test_rally","name":"Test Rally",
             "types":["sorcery"],"mana_cost":"{1}{G}","colors":["green"],
             "spell_effects":[{"kind":"pump_all","affects":"each_creature","power":1,"toughness":1}]}
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
             "aura":{"enchant":"any_creature","power":2,"toughness":2}},
            {"schema_version":1,"functional_id":"test_curse","name":"Test Curse",
             "types":["enchantment"],"subtypes":["Aura"],"mana_cost":"{B}","colors":["black"],
             "aura":{"enchant":"any_creature","power":-2,"toughness":-2}}
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
            "aura":{"enchant":"any_creature","keywords":["flying"]}}]"#,
    )
    .unwrap();
    assert_eq!(
        text_of(&inline, "test_flight"),
        "Enchant creature.\nEnchanted creature has flying."
    );
}

#[test]
fn issue_374_a_grant_keyword_spell_reads_as_gaining_the_keyword() {
    // Mighty Leap (bundled): "+2/+2 and gains flying until end of turn", written
    // as the two clauses the IR actually carries.
    let db = bundled();
    assert_eq!(
        text_of(&db, "mighty_leap"),
        "Target creature gets +2/+2 until end of turn.\n\
         Target creature gains flying until end of turn."
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
    // any keyword, combat restriction, ability, spell effect, or Aura grant must
    // produce text — the formatter never silently emits nothing for a card that
    // does something.
    let db = bundled();
    for id in (0..db.len() as u64).map(CardId) {
        let card = db.card(id).unwrap();
        let has_rules = !card.keywords.is_empty()
            || !card.restrictions.is_empty()
            || !card.abilities.is_empty()
            || !card.spell_effects.is_empty()
            || card.aura.is_some();
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
        "Lava Axe deals 5 damage to target player."
    );

    // A negative pump keeps its signs.
    assert_eq!(
        text_of(&db, "strangling_spores"),
        "Target creature gets -3/-3 until end of turn."
    );

    // Two-effect combat tricks render one sentence per effect, in order.
    assert_eq!(
        text_of(&db, "mighty_leap"),
        "Target creature gets +2/+2 until end of turn.\n\
         Target creature gains flying until end of turn."
    );
    assert_eq!(
        text_of(&db, "sure_strike"),
        "Target creature gets +3/+0 until end of turn.\n\
         Target creature gains first strike until end of turn."
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
fn static_text(affects: &str, modification: &str) -> String {
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

    // A spell's effects are one line each, in resolution order, so a player can read
    // that Tormenting Voice discards *before* it draws and that Sift does the reverse.
    assert_eq!(
        text_of(&db, "tormenting_voice"),
        "You discard a card.\nDraw two cards."
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
        "When Militia Bugler enters the battlefield, look at the top four cards of your \
         library, you may put up to one creature card with power 2 or less from among \
         them into your hand, then put the rest on the bottom of your library in a \
         random order."
    );
    assert!(text_of(&db, "elvish_rejuvenator").contains("onto the battlefield tapped"));
    assert_eq!(
        text_of(&db, "elvish_clancaller"),
        "Other Elves you control get +1/+1.\n{3}{G}{G}, {T}: Search your library for up \
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
               "effects":[{"kind":"may","cost":"{1}",
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
fn issue_610_the_optional_question_is_composed_from_the_effects_it_offers() {
    // The words on the button a player answers with come from the same vocabulary as
    // the printed sentence, so the offer and the card can never describe it differently.
    let draw = vec![Effect::DrawCard { count: 1 }];
    assert_eq!(optional_effect_question(None, &draw), "Draw a card?");
    assert_eq!(
        optional_effect_question(Some("{1}"), &draw),
        "Pay {1} to draw a card?"
    );

    let gain = vec![Effect::GainLife {
        player_ref: PlayerRef::Controller,
        amount: 3,
    }];
    assert_eq!(optional_effect_question(None, &gain), "You gain 3 life?");
    assert_eq!(
        optional_effect_question(Some("{W}"), &gain),
        "Pay {W} to gain 3 life?"
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
