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
    Ability, AdditionalCost, AuraGrant, CardData, CardFilter, CardType, Chooser, Color,
    CombatRestriction, Condition, Cost, CountScope, CounterKind, DamageSubject, Effect,
    FoundDestination, GraveyardCardClass, GraveyardScope, Keyword, ManaRestriction, MassAffects,
    ObservedPermanent, ObservedSpell, PermanentCount, PlayerModification, PlayerRef, StaticAffects,
    StaticCondition, StaticModification, TargetCount, TargetSpec, TokenData, TriggerCondition,
    TriggerStep, TurnScope,
};

mod effects;
mod words;

pub(crate) use effects::*;
pub(crate) use words::*;

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

    // A printed combat restriction is not a keyword, so it gets its own sentence about
    // the card rather than a word in the keyword line: "Bristling Boar can't be blocked
    // by more than one creature."
    for &restriction in &data.restrictions {
        lines.push(finish(&format!(
            "{source} {}",
            restriction_predicate(restriction)
        )));
    }

    for ability in &data.abilities {
        lines.push(ability_text(source, ability));
    }

    // An additional cast cost is stated *before* what the spell does, because that is
    // the order it happens in: it is paid while the spell is cast (CR 601.2b), and a
    // player who cannot pay it never gets to the sentences below.
    if let Some(cost) = data.additional_cost {
        lines.push(additional_cost_text(cost));
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

/// Generate the rules text of a **token** (CR 111.3), from the characteristics the
/// effect that created it gave it.
///
/// The token counterpart of [`rules_text`], sharing its clause builders and its clause
/// order so a token's words are composed exactly as a card's are. It is shorter for
/// structural reasons rather than by choice: a token has no spell ability to describe,
/// no Aura grant, and no scripted tier to fall back on (all three are keyed to things
/// a token has not got), so only the keyword line, the restriction sentences, and the
/// abilities remain.
#[must_use]
pub(crate) fn token_rules_text(token: &TokenData) -> String {
    let source = token.name.as_str();
    let mut lines: Vec<String> = Vec::new();

    if !token.keywords.is_empty() {
        let words: Vec<&str> = token.keywords.iter().map(|&kw| keyword_word(kw)).collect();
        lines.push(sentence_case(&words.join(", ")));
    }
    for &restriction in &token.restrictions {
        lines.push(finish(&format!(
            "{source} {}",
            restriction_predicate(restriction)
        )));
    }
    for ability in &token.abilities {
        lines.push(ability_text(source, ability));
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
                // A watching condition's subject is the class it observes, not the
                // source — "whenever another creature dies", not "whenever this does".
                TriggerCondition::PermanentEnters(observes) => {
                    format!(
                        "Whenever {} enters the battlefield",
                        observed_subject(observes)
                    )
                }
                TriggerCondition::PermanentDies(observes) => {
                    format!("Whenever {} dies", observed_subject(observes))
                }
                TriggerCondition::YouGainLife => "Whenever you gain life".to_string(),
                TriggerCondition::YouCastSpell(spell) => {
                    format!("Whenever you cast {}", observed_spell_noun(*spell))
                }
                // A step trigger's subject is the step, not the source — "at the
                // beginning of your upkeep", never "when this …".
                TriggerCondition::BeginningOfStep { step, whose_turn } => {
                    format!("At the beginning of {}", step_phrase(*step, *whose_turn))
                }
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
        // A player-subject static says what is true of *you*, so the sentence has no
        // object at all — the shortest ability the formatter composes, and the only one
        // whose subject is a person.
        Ability::PlayerStatic { modification } => match modification {
            PlayerModification::NoMaximumHandSize => "You have no maximum hand size.".to_string(),
        },
        // A static ability reads as a standing statement about other objects, with no
        // trigger word and no cost — "Other Elves you control get +1/+1." The subject
        // is the affected class, not the source, which is why `source` goes unused here.
        Ability::Static {
            affects,
            modification,
            condition,
        } => {
            let statement = format!(
                "{} {}",
                static_subject(affects, source),
                static_verb(modification)
            );
            // The `as long as …` clause trails the statement, where a card prints it.
            match condition {
                None => sentence_case(&format!("{statement}.")),
                Some(condition) => sentence_case(&format!(
                    "{statement} as long as {}.",
                    static_condition_clause(condition)
                )),
            }
        }
    }
}

/// The subject of a watching trigger's sentence: the class of permanents it observes,
/// as an indefinite noun phrase ("another creature", "a creature you control").
///
/// Composed from the selector rather than authored, the same reason
/// [`static_subject`] is: the sentence and the events actually noticed cannot disagree
/// if only one of them exists.
fn observed_subject(observes: &ObservedPermanent) -> String {
    let article = if observes.excludes_source() {
        "another"
    } else {
        "a"
    };
    // "nontoken" is an adjective on the noun, exactly where a card prints it:
    // "another nontoken Dragon you control".
    let noun = match (observes.nontoken_only(), observes.subtype()) {
        (false, Some(subtype)) => subtype.to_string(),
        (false, None) => "creature".to_string(),
        (true, Some(subtype)) => format!("nontoken {subtype}"),
        (true, None) => "nontoken creature".to_string(),
    };
    let class = match observes {
        ObservedPermanent::CreaturesYouControl { .. } => {
            format!("{article} {noun} you control")
        }
        ObservedPermanent::AnyCreature { .. } => format!("{article} {noun}"),
    };
    // A power bound trails the whole class, where a card prints it: "another creature
    // you control with power 2 or less".
    match observes.max_power() {
        None => class,
        Some(max) => format!("{class} with power {max} or less"),
    }
}

/// The step a step trigger watches, as the noun phrase that follows "at the beginning
/// of" — "your upkeep", "each end step", "combat on your turn".
///
/// The scope is folded into the phrase rather than prefixed, because English does not
/// put it in one place: an upkeep takes a possessive determiner ("your upkeep") and the
/// combat step takes a trailing qualifier ("combat on your turn"), and "each combat"
/// drops the qualifier entirely. Composed from the same two values the engine matches
/// on, so the sentence and the step actually watched cannot disagree.
fn step_phrase(step: TriggerStep, whose_turn: TurnScope) -> String {
    let noun = match step {
        TriggerStep::Upkeep => "upkeep",
        TriggerStep::Draw => "draw step",
        TriggerStep::BeginCombat => "combat",
        TriggerStep::EndStep => "end step",
    };
    match (step, whose_turn) {
        (TriggerStep::BeginCombat, TurnScope::Yours) => "combat on your turn".to_string(),
        (_, TurnScope::Yours) => format!("your {noun}"),
        (_, TurnScope::Each) => format!("each {noun}"),
    }
}

/// The class of spell a cast-watching trigger notices, as a noun phrase.
fn observed_spell_noun(spell: ObservedSpell) -> &'static str {
    match spell {
        ObservedSpell::Enchantment => "an enchantment spell",
        ObservedSpell::InstantOrSorcery => "an instant or sorcery spell",
    }
}

/// The subject of a static ability's sentence: the class of permanents it affects.
///
/// Composed from the selector rather than authored, so the printed text and the
/// permanents actually modified cannot disagree — the same reason rules text is
/// generated at all (ADR 0008 §6).
fn static_subject(affects: &StaticAffects, source: &str) -> String {
    match affects {
        // A class of one reads as the card's own name, which is how a printed card
        // refers to itself in a standing statement.
        StaticAffects::Source => source.to_string(),
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

/// The `as long as …` clause of a conditional static ability, as the words that follow
/// "as long as".
///
/// Composed from the same selector the engine re-asks on every read, so the sentence and
/// the condition cannot disagree.
fn static_condition_clause(condition: &StaticCondition) -> String {
    match condition {
        StaticCondition::ControlsAtLeast { permanents, count } => {
            format!("you control {}", counted_permanents(permanents, *count))
        }
        StaticCondition::SourceIsAttacking => "it's attacking".to_string(),
    }
}

/// A permanent count as a noun phrase — "an artifact", "three or more artifacts", "an
/// Ajani planeswalker", "a blue creature", "a creature with power 4 or greater".
///
/// Shared with the intervening if in [`effects`], which asks the same question of the
/// same selector: one composer, so the static and the resolution-time readings of one
/// `controls_at_least` cannot be worded differently.
pub(super) fn counted_permanents(permanents: &PermanentCount, count: u32) -> String {
    let mut noun = String::new();
    if let Some(color) = permanents.color {
        noun.push_str(color.word());
        noun.push(' ');
    }
    if let Some(subtype) = &permanents.subtype {
        noun.push_str(subtype);
        noun.push(' ');
    }
    noun.push_str(match permanents.card_type {
        Some(CardType::Creature) => "creature",
        Some(CardType::Artifact) => "artifact",
        Some(CardType::Enchantment) => "enchantment",
        Some(CardType::Land) => "land",
        Some(CardType::Planeswalker) => "planeswalker",
        Some(CardType::Instant) | Some(CardType::Sorcery) | Some(CardType::Battle) | None => {
            "permanent"
        }
    });
    let phrase = match count {
        1 => format!("{} {noun}", indefinite_article(&noun)),
        n => format!("{} or more {}", number(n), plural(&noun)),
    };
    // A power bound trails the noun, where a card prints it: "a creature with power 4
    // or greater".
    match permanents.min_power {
        None => phrase,
        Some(min) => format!("{phrase} with power {min} or greater"),
    }
}

/// "a" or "an" for `noun` — read off its first letter, which is right for every noun
/// this composer can produce.
fn indefinite_article(noun: &str) -> &'static str {
    match noun.chars().next() {
        Some('a' | 'e' | 'i' | 'o' | 'u' | 'A' | 'E' | 'I' | 'O' | 'U') => "an",
        _ => "a",
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
    if !aura.restrictions.is_empty() {
        // Restrictions are predicates rather than nouns, so they are joined into one
        // sentence about the host — "enchanted creature can't attack and can't block".
        let clauses: Vec<String> = aura
            .restrictions
            .iter()
            .map(|&r| restriction_predicate(r))
            .collect();
        lines.push(finish(&format!(
            "enchanted {} {}",
            object_noun(aura.enchant),
            clauses.join(" and ")
        )));
    }
    lines
}

/// Several effects as one clause: `draw a card and you gain 3 life`.
fn clauses(source: &str, effects: &[Effect]) -> String {
    let parts: Vec<String> = effects.iter().map(|e| effect_clause(source, e)).collect();
    parts.join(" and ")
}

#[cfg(test)]
mod tests;
