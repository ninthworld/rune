//! Fallback rules text: the words a player reads, **generated** from a card's
//! functional definition (ADR 0018 §7).
//!
//! Nothing in the repository stores a card's rules prose. What a client displays is
//! composed here, from the same `Ability`/`Effect` IR the engine executes — so the
//! text a player reads cannot drift from what the card does, because there is nothing
//! for it to drift from.
//!
//! This lives in `rune-server`, not the engine, because generating display prose is
//! presentation: keeping it here is what makes "the engine never depends on display
//! text" true by construction (ADR 0018 §7). It is pure — same definition in, same
//! string out, no locale, no randomness.
//!
//! **Coverage is compiler-enforced.** Every `match` over the IR is exhaustive with no
//! wildcard arm, so a new `Effect`/`Ability`/`Keyword`/`TargetSpec` variant fails
//! `cargo build` here until it is given words. That is a stronger guarantee than a
//! runtime check, which would only fire once a card using the new variant happened to
//! be loaded — the failure lands on whoever adds the variant, not on a player.
//!
//! Output is written to be **semantically complete for play** — a player can act on
//! it. Reproducing official Oracle wording is explicitly *not* a goal (`docs/brief.md`
//! Legal Considerations).

use rune_engine::{
    Ability, AuraGrant, CardData, Color, Cost, CounterKind, Effect, Keyword, PlayerRef, TargetSpec,
    TriggerCondition,
};

/// Generate the rules text of one card.
///
/// `scripted` is the card's hand-authored text from `rune_engine::scripted_rules_text`,
/// present exactly when the definition declares `scripted: true` — behavior written in
/// Rust is opaque to this formatter, so a scripted card states in words what its code
/// does (ADR 0018 §7). The engine's catalog loader enforces that pairing in both
/// directions, so a scripted card can never reach this function with no text to show.
///
/// Clauses are emitted in a fixed order — keywords, abilities, spell effects, the Aura
/// grant, then any scripted text — one per line. A vanilla card generates the empty
/// string: it has no rules, and inventing words for it would be noise.
#[must_use]
pub(crate) fn rules_text(data: &CardData, scripted: Option<&str>) -> String {
    let source = data.name.as_str();
    let mut lines: Vec<String> = Vec::new();

    if !data.keywords.is_empty() {
        let words: Vec<&str> = data.keywords.iter().map(|&kw| keyword_word(kw)).collect();
        lines.push(sentence_case(&words.join(", ")));
    }

    for ability in &data.abilities {
        lines.push(ability_text(source, ability));
    }

    for effect in &data.spell_effects {
        lines.push(finish(&effect_clause(source, effect)));
    }

    if let Some(aura) = &data.aura {
        lines.extend(aura_text(aura));
    }

    if let Some(text) = scripted {
        lines.push(text.to_string());
    }

    lines.join("\n")
}

/// One ability as a sentence. `source` is the name of the object the ability is on —
/// what a rules sentence calls itself. Also used to label an `activate_ability`
/// action with its own cost-colon-effect line (`view::ability_label`), so the dock
/// button and the printed text can never disagree.
pub(crate) fn ability_text(source: &str, ability: &Ability) -> String {
    match ability {
        Ability::Activated { cost, effects } => {
            let costs: Vec<&str> = cost.iter().map(|c| cost_symbol(c)).collect();
            format!(
                "{}: {}",
                costs.join(", "),
                finish(&clauses(source, effects))
            )
        }
        Ability::Triggered { event, effects } => {
            let trigger = match event {
                TriggerCondition::SelfEntersBattlefield => {
                    format!("When {source} enters the battlefield")
                }
                TriggerCondition::SelfDies => format!("When {source} dies"),
            };
            finish(&format!("{trigger}, {}", clauses(source, effects)))
        }
        // Self-replacements (CR 614.1c) read as statements about entering, not as
        // things that happen afterwards — which is exactly what they are.
        Ability::EntersTapped => format!("{source} enters the battlefield tapped."),
        Ability::EntersWithCounters { counter, count } => format!(
            "{source} enters the battlefield with {} on it.",
            counters(*counter, *count)
        ),
    }
}

/// The Aura's enchant restriction (CR 303.4a) and its static grants — a
/// power/toughness modification (CR 613.7c) and/or granted keywords (CR 613.1f) — as
/// separate sentences. Each grant sentence is omitted when it grants nothing.
fn aura_text(aura: &AuraGrant) -> Vec<String> {
    let mut lines = vec![format!("Enchant {}.", object_noun(aura.enchant))];
    if aura.power != 0 || aura.toughness != 0 {
        lines.push(format!(
            "Enchanted {} gets {:+}/{:+}.",
            object_noun(aura.enchant),
            aura.power,
            aura.toughness
        ));
    }
    if !aura.keywords.is_empty() {
        let words: Vec<&str> = aura.keywords.iter().map(|&kw| keyword_word(kw)).collect();
        lines.push(format!(
            "Enchanted {} has {}.",
            object_noun(aura.enchant),
            words.join(", ")
        ));
    }
    lines
}

/// Several effects as one clause: `draw a card and you gain 3 life`.
fn clauses(source: &str, effects: &[Effect]) -> String {
    let parts: Vec<String> = effects.iter().map(|e| effect_clause(source, e)).collect();
    parts.join(" and ")
}

/// One effect as a lowercase clause with no trailing period, so it can either stand
/// alone as a sentence ([`finish`]) or be embedded after a trigger or a cost.
///
/// Exhaustive by design: a new [`Effect`] variant must be given words here or the
/// workspace does not build (ADR 0018 §7).
fn effect_clause(source: &str, effect: &Effect) -> String {
    match effect {
        Effect::AddMana { color, amount } => format!("add {}", pips(*color, *amount)),
        Effect::AddColorlessMana { amount } => format!("add {}", colorless_pips(*amount)),
        Effect::DrawCard { count } => match count {
            1 => "draw a card".to_string(),
            n => format!("draw {} cards", number(u32::from(*n))),
        },
        Effect::Tap { target } => format!("tap {}", target_noun(*target)),
        Effect::CounterSpell { target } => format!("counter {}", target_noun(*target)),
        // A damage source is named, so a player can tell what dealt it (CR 120.3).
        Effect::DealDamage { target, amount } => {
            format!("{source} deals {amount} damage to {}", target_noun(*target))
        }
        Effect::Destroy { target } => format!("destroy {}", target_noun(*target)),
        Effect::GainLife { player_ref, amount } => {
            format!("{} {amount} life", conjugate(*player_ref, "gain"))
        }
        Effect::LoseLife { player_ref, amount } => {
            format!("{} {amount} life", conjugate(*player_ref, "lose"))
        }
        Effect::PutCounters {
            target,
            counter,
            count,
        } => format!(
            "put {} on {}",
            counters(*counter, *count),
            target_noun(*target)
        ),
        Effect::Pump {
            target,
            power,
            toughness,
        } => format!(
            "{} gets {power:+}/{toughness:+} until end of turn",
            target_noun(*target)
        ),
        Effect::GrantKeyword { target, keyword } => format!(
            "{} gains {} until end of turn",
            target_noun(*target),
            keyword_word(*keyword)
        ),
    }
}

/// A short label for an ability on the stack: its effects, as a sentence.
///
/// The stack shows what an ability *will do*, drawn from the same vocabulary as the
/// card's rules text — one formatter, so a spell and its stack entry can never
/// describe the same effect two different ways.
#[must_use]
pub(crate) fn effects_description(source: &str, effects: &[Effect]) -> String {
    if effects.is_empty() {
        return "Ability".to_string();
    }
    finish(&clauses(source, effects))
}

/// The cost symbol paid to activate an ability.
fn cost_symbol(cost: &Cost) -> &'static str {
    match cost {
        Cost::Tap => "{T}",
    }
}

/// `amount` mana pips of `color`, e.g. `{G}{G}` — repeated symbols, as a cost is
/// written, rather than a count a player has to turn back into pips.
fn pips(color: Color, amount: u8) -> String {
    color.pip().repeat(usize::from(amount))
}

/// `amount` colorless mana pips, e.g. `{C}{C}` — the colorless counterpart of
/// [`pips`], written the same repeated-symbol way.
fn colorless_pips(amount: u8) -> String {
    "{C}".repeat(usize::from(amount))
}

/// `count` counters of `kind`, e.g. `a +1/+1 counter` or `two -1/-1 counters`.
fn counters(kind: CounterKind, count: u32) -> String {
    let symbol = match kind {
        CounterKind::PlusOnePlusOne => "+1/+1",
        CounterKind::MinusOneMinusOne => "-1/-1",
    };
    match count {
        1 => format!("a {symbol} counter"),
        n => format!("{} {symbol} counters", number(n)),
    }
}

/// What an effect may target, as a noun phrase (CR 115.1).
fn target_noun(spec: TargetSpec) -> &'static str {
    match spec {
        TargetSpec::AnyPlayer => "target player",
        TargetSpec::AnyPermanent => "target permanent",
        TargetSpec::AnyCreature => "target creature",
        TargetSpec::SpellOnStack => "target spell",
        // CR 115.4: "any target" is the phrase itself, not a class of object.
        TargetSpec::AnyTarget => "any target",
    }
}

/// The class of object a target spec names, without the word "target" — what an Aura
/// enchants (CR 303.4a).
fn object_noun(spec: TargetSpec) -> &'static str {
    match spec {
        TargetSpec::AnyPlayer => "player",
        TargetSpec::AnyPermanent => "permanent",
        TargetSpec::AnyCreature => "creature",
        TargetSpec::SpellOnStack => "spell",
        TargetSpec::AnyTarget => "any target",
    }
}

/// The non-targeted subject of an effect (CR 115.1 — no target is chosen), with its
/// verb conjugated to agree with it: `you gain`, but a future third-person subject
/// would read `target player gains`.
///
/// The verb is passed in rather than baked into the subject so agreement is decided in
/// exactly one place; a new [`PlayerRef`] variant cannot pick up the wrong one.
fn conjugate(player_ref: PlayerRef, verb: &str) -> String {
    match player_ref {
        // Second person takes the bare verb.
        PlayerRef::Controller => format!("you {verb}"),
    }
}

/// A keyword ability as the word a player reads (CR 702).
fn keyword_word(keyword: Keyword) -> &'static str {
    match keyword {
        Keyword::Flying => "flying",
        Keyword::Reach => "reach",
        Keyword::Vigilance => "vigilance",
        Keyword::Haste => "haste",
        Keyword::FirstStrike => "first strike",
        Keyword::Trample => "trample",
        Keyword::Deathtouch => "deathtouch",
        Keyword::Lifelink => "lifelink",
    }
}

/// Small counts read as words, as a card writes them; larger ones stay numeric.
fn number(count: u32) -> String {
    match count {
        2 => "two".to_string(),
        3 => "three".to_string(),
        4 => "four".to_string(),
        5 => "five".to_string(),
        n => n.to_string(),
    }
}

/// A clause promoted to a sentence: capitalized, with a period.
fn finish(clause: &str) -> String {
    format!("{}.", sentence_case(clause))
}

/// The clause with its first character uppercased. ASCII-only by construction — every
/// clause above starts with an English word or a card's name.
fn sentence_case(clause: &str) -> String {
    let mut chars = clause.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::panic, clippy::unwrap_used)]

    use super::*;
    use rune_engine::{CardDatabase, CardId, FunctionalId};

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
        rules_text(data, rune_engine::scripted_rules_text(&data.functional_id))
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
        // they are exercised inline (ADR 0026).
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
    fn spells_generate_non_empty_text() {
        // Every card that does something renders real rules text from its IR (ADR 0018 §7).
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
        // No M19 card produces {C}, so it is exercised inline (ADR 0026).
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
        assert_eq!(text_of(&db, "serra_angel"), "Flying, vigilance");
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
    }

    #[test]
    fn an_aura_states_its_restriction_and_its_grant() {
        // P/T Auras have no clean M19 card, so they are exercised inline (ADR 0026).
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
        // Flight (bundled): an Aura whose only grant is a keyword reads its enchant
        // restriction and the keyword it grants (CR 613.1f).
        let db = bundled();
        assert_eq!(
            text_of(&db, "flight"),
            "Enchant creature.\nEnchanted creature has flying."
        );
    }

    #[test]
    fn issue_374_a_grant_keyword_spell_reads_as_gaining_the_keyword() {
        // Jump (bundled): "Target creature gains flying until end of turn."
        let db = bundled();
        assert_eq!(
            text_of(&db, "jump"),
            "Target creature gains flying until end of turn."
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
        // any keyword, ability, spell effect, or Aura grant must produce text — the
        // formatter never silently emits nothing for a card that does something.
        let db = bundled();
        for id in (0..db.len() as u64).map(CardId) {
            let card = db.card(id).unwrap();
            let has_rules = !card.keywords.is_empty()
                || !card.abilities.is_empty()
                || !card.spell_effects.is_empty()
                || card.aura.is_some();
            let text = rules_text(card, rune_engine::scripted_rules_text(&card.functional_id));
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
    fn generation_is_deterministic() {
        // Same definition in, same string out — the property the whole approach rests
        // on, since nothing stores the text to compare against.
        let db = bundled();
        for id in (0..db.len() as u64).map(CardId) {
            let card = db.card(id).unwrap();
            let once = rules_text(card, rune_engine::scripted_rules_text(&card.functional_id));
            let twice = rules_text(card, rune_engine::scripted_rules_text(&card.functional_id));
            assert_eq!(once, twice);
        }
    }

    #[test]
    fn a_scripted_card_shows_its_hand_authored_text() {
        // Behavior written in Rust is opaque to the formatter, so a scripted card
        // supplies its own words (ADR 0018 §7) — and they are what a player sees.
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
}
