//! Fallback rules text: the words a player reads, **generated** from a card's
//! functional definition (ADR 0008 §7).
//!
//! Nothing in the repository stores a card's rules prose. What a client displays is
//! composed here, from the same `Ability`/`Effect` IR the engine executes — so the
//! text a player reads cannot drift from what the card does, because there is nothing
//! for it to drift from.
//!
//! This lives in `sage-server`, not the engine, because generating display prose is
//! presentation: keeping it here is what makes "the engine never depends on display
//! text" true by construction (ADR 0008 §7). It is pure — same definition in, same
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

use sage_engine::{
    Ability, AuraGrant, CardData, Color, Cost, CounterKind, Effect, Keyword, MassAffects,
    PlayerRef, StaticAffects, StaticModification, TargetSpec, TriggerCondition,
};

/// Generate the rules text of one card.
///
/// `scripted` is the card's hand-authored text from `sage_engine::scripted_rules_text`,
/// present exactly when the definition declares `scripted: true` — behavior written in
/// Rust is opaque to this formatter, so a scripted card states in words what its code
/// does (ADR 0008 §7). The engine's catalog loader enforces that pairing in both
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
            let costs: Vec<String> = cost.iter().map(cost_symbol).collect();
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
                TriggerCondition::SelfAttacks => format!("Whenever {source} attacks"),
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
        // A static ability reads as a standing statement about other objects, with no
        // trigger word and no cost — "Other Elves you control get +1/+1." The subject
        // is the affected class, not the source, which is why `source` goes unused here.
        Ability::Static {
            affects,
            modification,
        } => sentence_case(&format!(
            "{} {}.",
            static_subject(affects),
            static_verb(modification)
        )),
    }
}

/// The subject of a static ability's sentence: the class of permanents it affects.
///
/// Composed from the selector rather than authored, so the printed text and the
/// permanents actually modified cannot disagree — the same reason rules text is
/// generated at all (ADR 0008 §6).
fn static_subject(affects: &StaticAffects) -> String {
    match affects {
        StaticAffects::CreaturesYouControl {
            subtype,
            except_this,
        } => {
            // "Other" is what distinguishes a lord from an anthem, and it is a fact
            // about the selector, so it is read off the selector.
            let other = if *except_this { "other " } else { "" };
            match subtype {
                Some(kind) => format!("{other}{} you control", plural(kind)),
                None => format!("{other}creatures you control"),
            }
        }
    }
}

/// The plural of a creature-type noun, for a sentence that speaks about a class of
/// permanents ("other **Elves** you control").
///
/// Subtypes are open-ended strings, so this cannot be a closed table — but naive `+s`
/// is wrong for a lot of Magic's vocabulary (Elf, Dwarf, Wolf), and printing "Elfs"
/// makes the card look broken. The irregulars are listed, then the ordinary English
/// suffix rules, then `+s`. A subtype this gets wrong is fixed by adding a line, and
/// is a cosmetic error rather than a rules one: nothing about which permanents are
/// affected reads this.
fn plural(subtype: &str) -> String {
    // Unchanged in the plural.
    const INVARIANT: [&str; 4] = ["Djinn", "Efreet", "Fish", "Sheep"];
    if INVARIANT.iter().any(|word| word == &subtype) {
        return subtype.to_string();
    }
    match subtype {
        "Ox" => return "Oxen".to_string(),
        "Mouse" => return "Mice".to_string(),
        "Goose" => return "Geese".to_string(),
        "Fungus" => return "Fungi".to_string(),
        _ => {}
    }
    // Elf → Elves, Dwarf → Dwarves, Wolf → Wolves.
    if let Some(stem) = subtype.strip_suffix('f') {
        return format!("{stem}ves");
    }
    // Sphinx → Sphinxes, Leech → Leeches.
    if ["s", "x", "z", "ch", "sh"]
        .iter()
        .any(|suffix| subtype.ends_with(suffix))
    {
        return format!("{subtype}es");
    }
    format!("{subtype}s")
}

/// The predicate of a static ability's sentence — what the affected permanents get.
fn static_verb(modification: &StaticModification) -> String {
    match modification {
        StaticModification::PowerToughness { power, toughness } => {
            format!("get {power:+}/{toughness:+}")
        }
        // "have", not "gain": a static ability is continuously true, where a spell's
        // grant is an event. The distinction is the whole difference between an anthem
        // and a pump.
        StaticModification::GrantKeyword { keyword } => {
            format!("have {}", keyword_word(*keyword))
        }
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
/// workspace does not build (ADR 0008 §7).
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
        Effect::Exile { target } => format!("exile {}", target_noun(*target)),
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
        Effect::PumpAll {
            affects,
            power,
            toughness,
        } => format!(
            "{} get {power:+}/{toughness:+} until end of turn",
            mass_subject(*affects)
        ),
        Effect::GrantKeywordAll { affects, keyword } => format!(
            "{} gain {} until end of turn",
            mass_subject(*affects),
            keyword_word(*keyword)
        ),
        Effect::ReturnToHand { target } => {
            format!("return {} to its owner's hand", target_noun(*target))
        }
        Effect::Mill { player_ref, count } => match count {
            1 => conjugate(*player_ref, "mill") + " a card",
            n => format!(
                "{} {} cards",
                conjugate(*player_ref, "mill"),
                number(u32::from(*n))
            ),
        },
    }
}

/// The class a mass, non-targeting effect names, as the subject of its sentence.
fn mass_subject(affects: MassAffects) -> &'static str {
    match affects {
        MassAffects::CreaturesYouControl => "creatures you control",
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
fn cost_symbol(cost: &Cost) -> String {
    match cost {
        Cost::Tap => "{T}".to_string(),
        // A mana cost is already written in the notation a player reads it in, so it
        // is passed through rather than re-rendered from a parse.
        Cost::Mana { mana } => mana.clone(),
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
        TargetSpec::AnyOpponent => "target opponent",
        TargetSpec::AnyPermanent => "target permanent",
        TargetSpec::AnyNonlandPermanent => "target nonland permanent",
        TargetSpec::AnyCreature => "target creature",
        TargetSpec::AnyCreatureYouControl => "target creature you control",
        TargetSpec::AnyCreatureAnOpponentControls => "target creature an opponent controls",
        TargetSpec::AnyCreatureWithFlying => "target creature with flying",
        TargetSpec::AnyTappedCreature => "target tapped creature",
        TargetSpec::AnyArtifact => "target artifact",
        TargetSpec::AnyEnchantment => "target enchantment",
        TargetSpec::AnyArtifactOrEnchantment => "target artifact or enchantment",
        TargetSpec::AnyLand => "target land",
        TargetSpec::SpellOnStack => "target spell",
        TargetSpec::CreatureSpellOnStack => "target creature spell",
        // CR 115.4: "any target" is the phrase itself, not a class of object.
        TargetSpec::AnyTarget => "any target",
    }
}

/// The class of object a target spec names, without the word "target" — what an Aura
/// enchants (CR 303.4a).
fn object_noun(spec: TargetSpec) -> &'static str {
    match spec {
        TargetSpec::AnyPlayer => "player",
        TargetSpec::AnyOpponent => "opponent",
        TargetSpec::AnyPermanent => "permanent",
        TargetSpec::AnyNonlandPermanent => "nonland permanent",
        TargetSpec::AnyCreature => "creature",
        TargetSpec::AnyCreatureYouControl => "creature you control",
        TargetSpec::AnyCreatureAnOpponentControls => "creature an opponent controls",
        TargetSpec::AnyCreatureWithFlying => "creature with flying",
        TargetSpec::AnyTappedCreature => "tapped creature",
        TargetSpec::AnyArtifact => "artifact",
        TargetSpec::AnyEnchantment => "enchantment",
        TargetSpec::AnyArtifactOrEnchantment => "artifact or enchantment",
        TargetSpec::AnyLand => "land",
        TargetSpec::SpellOnStack => "spell",
        TargetSpec::CreatureSpellOnStack => "creature spell",
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
        // Every third-person subject here is grammatically singular ("each opponent
        // loses", not "lose"), and every verb this is called with is regular, so the
        // agreement is one suffix rather than a table.
        PlayerRef::EachOpponent => format!("each opponent {verb}s"),
        PlayerRef::TargetPlayer => format!("target player {verb}s"),
        PlayerRef::TargetOpponent => format!("target opponent {verb}s"),
    }
}

/// A keyword ability as the word a player reads (CR 702).
fn keyword_word(keyword: Keyword) -> &'static str {
    match keyword {
        Keyword::Flying => "flying",
        Keyword::Reach => "reach",
        Keyword::Vigilance => "vigilance",
        Keyword::Haste => "haste",
        Keyword::Defender => "defender",
        Keyword::Menace => "menace",
        Keyword::FirstStrike => "first strike",
        Keyword::Trample => "trample",
        Keyword::Deathtouch => "deathtouch",
        Keyword::Lifelink => "lifelink",
        Keyword::DoubleStrike => "double strike",
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

        // A vanilla body still generates nothing, which stays the honest answer.
        assert_eq!(text_of(&db, "thornhide_wolves"), "");
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
}
