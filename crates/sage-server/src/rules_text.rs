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
    equip_ability, Ability, ActivatorScope, AdditionalCost, Attachment, AttachmentKind, CardData,
    CardFilter, CardType, Chooser, Color, CombatRestriction, Condition, Cost, CostModification,
    CountScope, CounterKind, DamageCharacteristic, DamageSubject, DerivedAmount, DestroyAffects,
    Effect, EnteringFilter, FoundDestination, GraveyardCardClass, GraveyardScope, Keyword,
    ManaRestriction, MassAffects, ObservedPermanent, ObservedSpell, PermanentCount,
    PlayerModification, PlayerRef, ReplacementEffect, SacrificeCount, SpellMode, SpellTrait,
    StaticAffects, StaticCondition, StaticModification, TargetCount, TargetSpec, TokenData,
    TriggerCondition, TriggerStep, TurnScope,
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
/// Clauses are emitted in a fixed order — keywords, an additional cast cost, spell
/// effects, abilities, the attachment grant (and, for an Equipment, its equip ability),
/// then any scripted text —
/// one per line. A vanilla card generates the empty string: it has no rules, and inventing
/// words for it would be noise.
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
    for restriction in &data.restrictions {
        lines.push(finish(&format!(
            "{source} {}",
            restriction_predicate(restriction)
        )));
    }

    // An additional cast cost is stated *before* what the spell does, because that is
    // the order it happens in: it is paid while the spell is cast (CR 601.2b), and a
    // player who cannot pay it never gets to the sentences below.
    if let Some(cost) = data.additional_cost {
        lines.push(additional_cost_text(cost));
    }

    // What the spell does when it resolves comes before any ability it carries: on the
    // one card that prints both — an instant with a trigger that works from the graveyard
    // it lands in — casting it is what happens first, and the ability is the rider. A
    // permanent has no spell effects, so this changes nothing about every other card.
    for effect in &data.spell_effects {
        lines.push(finish(&effect_clause(source, effect)));
    }

    // A modal spell prints its bullets where a plain one prints its effects, because
    // that is what they are (CR 700.2) — one list instead of a loose sentence.
    if data.is_modal() {
        lines.push(modes_text(source, &data.modes));
    }

    // What is true of the spell on the stack, after what it does. A printed card puts
    // this clause last for the same reason: it is a rider on the sentence above it.
    for declared in &data.spell_traits {
        lines.push(spell_trait_text(source, *declared));
    }

    for ability in &data.abilities {
        lines.push(ability_text(source, ability));
    }

    if let Some(attachment) = &data.attachment {
        lines.extend(attachment_text(data, attachment));
    }

    if let Some(text) = scripted {
        lines.push(text.to_string());
    }

    lines.join("\n")
}

/// The bulleted list a modal spell prints (CR 700.2): a `Choose one —` header and one
/// line per mode, each the mode's own effects as sentences.
///
/// One line per mode is load-bearing beyond looking right: the numbered dock rows a
/// player picks a mode from are drawn from exactly these lines
/// (`docs/client-design.md` §6.7), one row apiece, so the split here *is* the split
/// there.
#[must_use]
pub(crate) fn modes_text(source: &str, modes: &[SpellMode]) -> String {
    let mut lines = vec!["Choose one —".to_string()];
    for mode in modes {
        lines.push(format!("• {}", mode_text(source, mode)));
    }
    lines.join("\n")
}

/// One mode as the sentence its bullet carries — its effects, finished and joined.
///
/// Shared by the card's own text and by the dock row that offers the mode, so a player
/// choosing a mode reads exactly the words the card prints for it.
#[must_use]
pub(crate) fn mode_text(source: &str, mode: &SpellMode) -> String {
    mode.effects
        .iter()
        .map(|effect| finish(&effect_clause(source, effect)))
        .collect::<Vec<_>>()
        .join(" ")
}

/// A [`SpellTrait`] as the sentence a card prints for it, with its X threshold where the
/// card puts one.
///
/// Both members read as a fact about the spell rather than as something it does, which
/// is why they are their own clause and not an effect: "**Banefire** can't be countered"
/// is true while it is on the stack, before anything resolves.
fn spell_trait_text(source: &str, declared: SpellTrait) -> String {
    let (threshold, clause) = match declared {
        SpellTrait::CantBeCountered { if_x_at_least } => {
            (if_x_at_least, format!("{source} can't be countered"))
        }
        SpellTrait::DamageCantBePrevented { if_x_at_least } => (
            if_x_at_least,
            format!("the damage {source} deals can't be prevented"),
        ),
    };
    match threshold {
        None => finish(&clause),
        Some(least) => finish(&format!("if X is {least} or more, {clause}")),
    }
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
    for restriction in &token.restrictions {
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
                TriggerCondition::PermanentAttacks(observes) => {
                    format!("Whenever {} attacks", observed_subject(observes))
                }
                TriggerCondition::YouGainLife => "Whenever you gain life".to_string(),
                TriggerCondition::YouDrawCard => "Whenever you draw a card".to_string(),
                // "Nonmana" is the engine's own word for the CR 605.3a exclusion the
                // condition enforces structurally: an ability that uses the stack.
                TriggerCondition::AbilityActivated(observes) => format!(
                    "Whenever {} activates a nonmana ability{}",
                    activator_subject(observes.activator),
                    activated_source_clause(&observes.source_types)
                ),
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
        // "As …" rather than "When …", and the distinction is the rule (CR 614.12): the
        // colour is named as part of entering, not by an ability that goes on the stack
        // once the permanent is already there. Every ability of the card that reads the
        // answer calls it "the chosen color", so this sentence is what gives that phrase
        // its referent.
        Ability::EntersChoosingColor => {
            format!("As {source} enters the battlefield, choose a color.")
        }
        // A player-subject static says what is true of *you*, so the sentence has no
        // object at all — the shortest ability the formatter composes, and the only one
        // whose subject is a person.
        Ability::PlayerStatic { modification } => match modification {
            PlayerModification::NoMaximumHandSize => "You have no maximum hand size.".to_string(),
            PlayerModification::PlayLandsFromGraveyard => {
                "You may play lands from your graveyard.".to_string()
            }
        },
        // A cost modifier's subject is a class of *spells*, and the sentence names the
        // caster because the ability does: it reaches its controller's casts and nobody
        // else's. "Cost … to cast" rather than "cost … " because the modification applies
        // while the spell is being cast (CR 601.2f) and not to anything the card does
        // later.
        Ability::CostModifier {
            spells,
            modification,
        } => {
            let (direction, generic) = match modification {
                CostModification::Reduce { generic } => ("less", generic),
                CostModification::Increase { generic } => ("more", generic),
            };
            sentence_case(&format!(
                "{} you cast cost {{{generic}}} {direction} to cast.",
                observed_spell_class(*spells),
            ))
        }
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
                static_verb(modification, subject_is_plural(affects))
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
    // The qualifiers trail the whole class, where a card prints them: "another creature
    // you control with power 2 or less", "a creature with flying". Both at once join
    // with "and" rather than repeating the preposition, which is how a card would say
    // it if one ever asked for both.
    let mut qualifiers: Vec<String> = Vec::new();
    if let Some(keyword) = observes.keyword() {
        qualifiers.push(keyword_word(keyword).to_string());
    }
    if let Some(max) = observes.max_power() {
        qualifiers.push(format!("power {max} or less"));
    }
    if qualifiers.is_empty() {
        class
    } else {
        format!("{class} with {}", qualifiers.join(" and "))
    }
}

/// The subject of an activation-watching trigger's sentence: the player whose
/// activations it notices.
fn activator_subject(activator: ActivatorScope) -> &'static str {
    match activator {
        ActivatorScope::Any => "a player",
        ActivatorScope::Opponents => "an opponent",
    }
}

/// The "of a creature or land" that follows "activates a nonmana ability", or nothing
/// at all when the selector names no types and every permanent's abilities count.
fn activated_source_clause(source_types: &[CardType]) -> String {
    if source_types.is_empty() {
        return String::new();
    }
    // "A creature or land": the article rides on the first noun only, exactly as a card
    // prints it, and agrees with that noun alone.
    let nouns: Vec<&str> = source_types
        .iter()
        .map(|&kind| card_type_word(kind))
        .collect();
    let article = if nouns[0].starts_with(['a', 'e', 'i', 'o', 'u']) {
        "an"
    } else {
        "a"
    };
    format!(" of {article} {}", nouns.join(" or "))
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

/// The class of spell a cast-watching trigger notices, as a **singular** noun phrase —
/// the object of "whenever you cast …".
fn observed_spell_noun(spell: ObservedSpell) -> String {
    match spell {
        ObservedSpell::Enchantment => "an enchantment spell".to_string(),
        ObservedSpell::Artifact => "an artifact spell".to_string(),
        ObservedSpell::InstantOrSorcery => "an instant or sorcery spell".to_string(),
        ObservedSpell::Creature { min_power } => {
            format!("a creature spell{}", spell_power_clause(min_power))
        }
        // Named, not spelled out: the sentence is printed on a card that has not
        // entered the battlefield yet, where the colour is genuinely unknown. What the
        // *permanent* chose is on the board, in its own field, rather than baked into
        // text a hand and a battlefield would then disagree about.
        ObservedSpell::ChosenColor => "a spell of the chosen color".to_string(),
    }
}

/// The same class as a **plural** noun phrase — the subject of "… you cast cost {2} less
/// to cast".
///
/// One function per position, exactly as [`mass_subject`]/[`mass_recipient`] are, because
/// English is: a class is singular where a trigger names one cast and plural where a cost
/// modifier names all of them. Both are exhaustive, so a new [`ObservedSpell`] variant
/// must be given words in each.
fn observed_spell_class(spell: ObservedSpell) -> String {
    match spell {
        ObservedSpell::Enchantment => "enchantment spells".to_string(),
        ObservedSpell::Artifact => "artifact spells".to_string(),
        ObservedSpell::InstantOrSorcery => "instant and sorcery spells".to_string(),
        ObservedSpell::Creature { min_power } => {
            format!("creature spells{}", spell_power_clause(min_power))
        }
        ObservedSpell::ChosenColor => "spells of the chosen color".to_string(),
    }
}

/// The " with power 4 or greater" that trails a spell class, or nothing when the class
/// names no bound. Written once so the singular and the plural phrasings cannot drift.
fn spell_power_clause(min_power: Option<i32>) -> String {
    match min_power {
        None => String::new(),
        Some(min) => format!(" with power {min} or greater"),
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
            keyword,
        } => {
            // "Other" is what distinguishes a lord from an anthem, and it is a fact
            // about the selector, so it is read off the selector.
            let other = if *except_this { "other " } else { "" };
            let class = match subtype {
                Some(kind) => format!("{other}{} you control", plural(kind)),
                None => format!("{other}creatures you control"),
            };
            // A keyword filter trails the class, where a card prints it: "each creature
            // you control **with defender**". The same place the observer's counterpart
            // puts it, because it is the same phrase.
            match keyword {
                None => class,
                Some(keyword) => format!("{class} with {}", keyword_word(*keyword)),
            }
        }
    }
}

/// Whether a static ability's subject is **plural** — a class of permanents rather than
/// the single permanent that printed the ability.
///
/// Read by the one verb that has to agree with its subject: an as-though clause carries a
/// pronoun back to it ("as though **it** didn't have defender"), and there is no wording
/// that is right for both numbers. The other verbs need no such agreement — "get +1/+1"
/// and "have flying" read the same after either subject — so only that one asks.
fn subject_is_plural(affects: &StaticAffects) -> bool {
    match affects {
        StaticAffects::Source => false,
        StaticAffects::CreaturesYouControl { .. } => true,
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
        StaticCondition::SourceIsEnchantedOrEquipped => "it's enchanted or equipped".to_string(),
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
///
/// `plural` says whether the subject is a class or the one permanent that printed the
/// ability ([`subject_is_plural`]). Only the as-though clause reads it, because only it
/// refers back to its own subject with a pronoun.
fn static_verb(modification: &StaticModification, plural: bool) -> String {
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
        // "Rather than their power" is stated even though it is implied, because it is
        // the whole content of the ability: without it the sentence claims a creature
        // assigns damage equal to a characteristic, which every creature already does.
        StaticModification::AssignsCombatDamageBy { characteristic } => {
            let (verb, possessive) = if plural {
                ("assign", "their")
            } else {
                ("assigns", "its")
            };
            match characteristic {
                DamageCharacteristic::Toughness => format!(
                    "{verb} combat damage equal to {possessive} toughness rather than \
                     {possessive} power"
                ),
                // The unmodified rule, said out loud. No card prints it, but the
                // vocabulary can say it and a sentence that silently omitted it would be
                // the one kind of text this formatter must never produce.
                DamageCharacteristic::Power => {
                    format!("{verb} combat damage equal to {possessive} power")
                }
            }
        }
        // "As though", not "doesn't have": the keyword is still there, and a player
        // reading this needs to know that their own defender-counting cards still see it.
        StaticModification::AttacksAsThoughNoDefender => {
            let pronoun = if plural { "they didn't" } else { "it didn't" };
            format!("can attack as though {pronoun} have defender")
        }
    }
}

/// An attachment's restriction and its static grants — a power/toughness modification
/// (CR 613.7c) and/or granted keywords and combat restrictions (CR 613.1f) — as separate
/// sentences. Each grant sentence is omitted when it grants nothing.
///
/// The two kinds differ only in how the host is named. An Aura states its enchant
/// restriction as a sentence of its own (CR 303.4a) and then speaks of "enchanted
/// creature"; an Equipment has no such sentence — its restriction is a target on the
/// equip ability, printed with that ability — and speaks of "equipped creature", which is
/// the only thing an Equipment can ever be attached to (CR 301.5c) whatever its equip
/// ability may target.
fn attachment_text(data: &CardData, attachment: &Attachment) -> Vec<String> {
    // The word a grant sentence calls the host, and — for an Aura only — the sentence
    // that says what may be enchanted at all.
    let (host, mut lines) = match attachment.kind {
        AttachmentKind::Aura => (
            format!("enchanted {}", object_noun(attachment.attach_to)),
            vec![format!("Enchant {}.", object_noun(attachment.attach_to))],
        ),
        AttachmentKind::Equipment => ("equipped creature".to_string(), Vec::new()),
    };
    if attachment.power != 0 || attachment.toughness != 0 {
        // A counted grant states the class it scales with, exactly where the card prints
        // it: "enchanted creature gets +1/+1 for each Forest you control".
        let each = match &attachment.count_of {
            None => String::new(),
            Some(count_of) => format!(" for each {}", count_noun(count_of)),
        };
        lines.push(sentence_case(&format!(
            "{host} gets {:+}/{:+}{each}.",
            attachment.power, attachment.toughness
        )));
    }
    if !attachment.keywords.is_empty() {
        let words: Vec<&str> = attachment
            .keywords
            .iter()
            .map(|&kw| keyword_word(kw))
            .collect();
        lines.push(sentence_case(&format!("{host} has {}.", words.join(", "))));
    }
    if !attachment.restrictions.is_empty() {
        // Restrictions are predicates rather than nouns, so they are joined into one
        // sentence about the host — "enchanted creature can't attack and can't block".
        let clauses: Vec<String> = attachment
            .restrictions
            .iter()
            .map(restriction_predicate)
            .collect();
        lines.push(sentence_case(&finish(&format!(
            "{host} {}",
            clauses.join(" and ")
        ))));
    }
    // The equip ability, composed from the same value the engine activates
    // ([`equip_ability`]) and worded by the same [`ability_text`] that labels the dock
    // button — so the line a player reads on the card and the line they click are one
    // string, not two that agree today.
    lines.extend(equip_ability(data).map(|ability| ability_text(&data.name, &ability)));
    lines
}

/// Several effects as one clause: `draw a card and you gain 3 life`.
fn clauses(source: &str, effects: &[Effect]) -> String {
    let parts: Vec<String> = effects.iter().map(|e| effect_clause(source, e)).collect();
    parts.join(" and ")
}

#[cfg(test)]
mod rule_modification_tests;
#[cfg(test)]
mod tests;
