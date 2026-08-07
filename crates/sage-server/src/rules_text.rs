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
    equip_ability, Ability, ActivationTiming, ActivatorScope, AdditionalCost, Attachment,
    AttachmentKind, BackFace, BottomOrder, CardData, CardFilter, CardType, Chooser, Color,
    CombatRestriction, Condition, CopyClass, CopySubject, Cost, CostModification, CountScope,
    CounterKind, DamageCharacteristic, DamageSubject, DelayedCondition, DerivedAmount,
    DestroyAffects, Effect, EnteringFilter, FoundDestination, GraveyardCardClass, GraveyardCount,
    GraveyardScope, HalvedTotal, Keyword, ManaRestriction, MassAffects, NamedCardClass,
    ObservedPermanent, ObservedSpell, OptionalCost, PermanentAmount, PermanentCount,
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
    // The keyword line and the printed restriction sentences come first, exactly as they
    // do for a token and for a back face — one builder, so the three cannot drift.
    let mut lines: Vec<String> = face_lines(source, &data.keywords, &data.restrictions, &[]);

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

/// Generate the rules text of a card's **back face** (CR 712.2), from the same clause
/// builders and in the same clause order its front face's text is composed with.
///
/// Shorter than [`rules_text`] for structural reasons rather than by choice, exactly as
/// [`token_rules_text`] is: a back face is never cast, so it has no additional cast cost
/// and no spell ability to describe; it carries no attachment block; and the scripted
/// tier is keyed to the card rather than to a face, so a scripted card states its text
/// once, on the front.
#[must_use]
pub(crate) fn back_face_rules_text(face: &BackFace) -> String {
    face_lines(
        &face.name,
        &face.keywords,
        &face.restrictions,
        &face.abilities,
    )
    .join("\n")
}

/// The three clauses every face has — the keyword line, one sentence per printed combat
/// restriction, and one per ability — in the order a card prints them.
///
/// Written once because three callers need exactly it: a card's front face, its back
/// face, and a token. Each of those then adds whatever else it has, and none of them can
/// disagree with the others about how a keyword line or an ability sentence is built.
fn face_lines(
    source: &str,
    keywords: &[Keyword],
    restrictions: &[CombatRestriction],
    abilities: &[Ability],
) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    if !keywords.is_empty() {
        let words: Vec<&str> = keywords.iter().map(|&kw| keyword_word(kw)).collect();
        lines.push(sentence_case(&words.join(", ")));
    }
    for restriction in restrictions {
        lines.push(finish(&format!(
            "{source} {}",
            restriction_predicate(restriction)
        )));
    }
    for ability in abilities {
        lines.push(ability_text(source, ability));
    }
    lines
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
    face_lines(
        &token.name,
        &token.keywords,
        &token.restrictions,
        &token.abilities,
    )
    .join("\n")
}

/// The class a copy choice names, as the noun a card writes it with (CR 707).
fn copy_class_noun(class: CopyClass) -> &'static str {
    match class {
        CopyClass::AnyCreature => "a creature",
        CopyClass::CreatureYouControl => "a creature you control",
    }
}

/// One ability as a sentence. `source` is the name of the object the ability is on —
/// what a rules sentence calls itself. Also used to label an `activate_ability`
/// action with its own cost-colon-effect line (`view::ability_label`), so the dock
/// button and the printed text can never disagree.
pub(crate) fn ability_text(source: &str, ability: &Ability) -> String {
    match ability {
        Ability::Activated {
            cost,
            effects,
            timing,
            once_each_turn,
        } => {
            let costs: Vec<String> = cost.iter().map(cost_symbol).collect();
            // CR 602.5d and CR 602.5f: the restrictions are a second sentence on the same
            // line, exactly as a printed card sets them — the cost and effect first, then
            // the window it may be used in and how often.
            let restriction = match (timing, once_each_turn) {
                (ActivationTiming::AnyTime, false) => "",
                (ActivationTiming::AnyTime, true) => " Activate only once each turn.",
                (ActivationTiming::SorcerySpeed, false) => " Activate only as a sorcery.",
                (ActivationTiming::SorcerySpeed, true) => {
                    " Activate only as a sorcery and only once each turn."
                }
            };
            format!(
                "{}: {}{restriction}",
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
                // CR 609.7. The subject is the source or its host, and the two narrowings
                // trail it where the card prints them.
                TriggerCondition::DealsDamage(observes) => {
                    let who = if observes.by_attached {
                        "equipped creature".to_string()
                    } else {
                        source.to_string()
                    };
                    let kind = if observes.combat_only {
                        "combat damage"
                    } else {
                        "damage"
                    };
                    let whom = if observes.to_opponent {
                        " to an opponent"
                    } else if observes.to_player {
                        " to a player"
                    } else {
                        ""
                    };
                    format!("Whenever {who} deals {kind}{whom}")
                }
                // "One or more" is the card's own words for a condition that fires once
                // however many left.
                TriggerCondition::CardsLeaveGraveyard(observes) => format!(
                    "Whenever one or more {} leave {} graveyard",
                    plural(&filter_noun(&observes.filter, true)),
                    if observes.yours_only { "your" } else { "a" }
                ),
                TriggerCondition::PlayerDiscards(observes) => format!(
                    "Whenever {} discards a card",
                    if observes.opponents_only {
                        "an opponent"
                    } else {
                        "a player"
                    }
                ),
                // CR 603.6e. Both narrowings are printed where the card prints them:
                // the class of object first, then whose it has to be.
                TriggerCondition::SelfBecomesTarget(observes) => {
                    let what = if observes.spells_only {
                        "a spell"
                    } else {
                        "a spell or ability"
                    };
                    let whose = if observes.opponents_only {
                        " an opponent controls"
                    } else {
                        ""
                    };
                    format!("Whenever {source} becomes the target of {what}{whose}")
                }
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
        // The card-naming counterpart, worded the same way and for the same reason: the
        // abilities that read the answer call it "the chosen name", and this sentence is
        // what gives that phrase its referent.
        Ability::EntersNamingCard { class } => format!(
            "As {source} enters the battlefield, choose a {} card name.",
            named_card_class_noun(*class)
        ),
        // A characteristic-defining ability is a statement about *this* object's power,
        // in the present tense and with no trigger word — which is exactly what CR 604.3
        // makes it. It names the source because the card does: the printed corner says
        // `*`, and this sentence is what that asterisk means.
        Ability::DefinedPower { count_of } => format!(
            "{source}'s power is equal to the number of {}.",
            graveyard_count_noun(count_of)
        ),
        // The copy question, in the two shapes real cards print it in (CR 707). The
        // subject decides which sentence gets written: `You may have this enter as a copy
        // of …` says the whole thing in one clause, while an Aura says the choice and the
        // continuous effect as the two sentences the card prints.
        Ability::EntersAsCopy {
            of,
            subject,
            optional,
        } => match subject {
            CopySubject::This => {
                let opening = if *optional {
                    format!("You may have {source} enter the battlefield")
                } else {
                    format!("{source} enters the battlefield")
                };
                format!("{opening} as a copy of {}.", copy_class_noun(*of))
            }
            CopySubject::Attached => format!(
                "As {source} enters the battlefield, choose {}. Enchanted creature is a copy of the chosen creature.",
                copy_class_noun(*of)
            ),
        }
        // A player-subject static says what is true of *you*, so the sentence has no
        // object at all — the shortest ability the formatter composes, and the only one
        // whose subject is a person.
        Ability::PlayerStatic { modification } => match modification {
            PlayerModification::NoMaximumHandSize => "You have no maximum hand size.".to_string(),
            PlayerModification::PlayLandsFromGraveyard => {
                "You may play lands from your graveyard.".to_string()
            }
            PlayerModification::CastFromHandWithoutPaying => {
                "You may cast spells from your hand without paying their mana costs.".to_string()
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
                None => sentence_case(&stop(&statement)),
                Some(condition) => sentence_case(&stop(&format!(
                    "{statement} as long as {}",
                    static_condition_clause(condition)
                ))),
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
        ObservedSpell::Creature {
            min_power,
            max_power,
        } => format!(
            "a creature spell{}",
            spell_power_clause(min_power, max_power)
        ),
        ObservedSpell::Aura => "an Aura spell".to_string(),
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
        ObservedSpell::Creature {
            min_power,
            max_power,
        } => format!(
            "creature spells{}",
            spell_power_clause(min_power, max_power)
        ),
        ObservedSpell::Aura => "Aura spells".to_string(),
        ObservedSpell::ChosenColor => "spells of the chosen color".to_string(),
    }
}

/// The " with power 4 or greater" that trails a spell class, or nothing when the class
/// names no bound. Written once so the singular and the plural phrasings cannot drift.
fn spell_power_clause(min_power: Option<i32>, max_power: Option<i32>) -> String {
    match (min_power, max_power) {
        (None, None) => String::new(),
        (Some(min), None) => format!(" with power {min} or greater"),
        (None, Some(max)) => format!(" with power {max} or less"),
        // A range is printed as the numbers themselves — "power 4, 5, or 6" — because
        // that is how the card that has one says it, and "between 4 and 6" is not a
        // phrase Magic uses. Two apart is the only span any card prints; a wider one
        // would read as a list and is not worth pre-empting.
        (Some(min), Some(max)) => {
            let values: Vec<String> = (min..=max).map(|value| value.to_string()).collect();
            match values.split_last() {
                Some((last, rest)) if !rest.is_empty() => {
                    format!(" with power {}, or {last}", rest.join(", "))
                }
                _ => format!(" with power {min}"),
            }
        }
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
        // The other class of one, and it is *not* the card's name: an attachment's
        // sentence about its host names the host by what it is to the attachment
        // (CR 303.4). Only an Aura prints this today; an Equipment would say "equipped
        // creature", which is a fact about the attachment kind rather than the selector.
        StaticAffects::AttachedTo => "enchanted creature".to_string(),
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
        // "Lands your opponents control with the chosen name" — composed from the same
        // selector the engine evaluates on every read, so the sentence and the class it
        // describes cannot come apart.
        StaticAffects::PermanentsYourOpponentsControl {
            card_type,
            with_the_named_card,
        } => {
            let noun = plural(permanent_noun(*card_type));
            let named = if *with_the_named_card {
                " with the chosen name"
            } else {
                ""
            };
            format!("{noun} your opponents control{named}")
        }
    }
}

/// `statement` with a full stop, unless it already ends in one — which it does when the
/// statement ends in a **quoted ability**, whose own full stop sits inside the closing
/// quote where a printed card puts it. Writing `…any color.".` is the only thing this
/// exists to prevent.
fn stop(statement: &str) -> String {
    if statement.ends_with(".\"") {
        statement.to_string()
    } else {
        format!("{statement}.")
    }
}

/// The noun a card type reads as in a class of permanents — "land", "creature", or the
/// catch-all "permanent" for a class that names no type at all.
///
/// Shared with [`counted_permanents`] so a `land` in a static ability's selector and a
/// `land` in an intervening if are the same word.
fn permanent_noun(card_type: Option<CardType>) -> &'static str {
    match card_type {
        Some(CardType::Creature) => "creature",
        Some(CardType::Artifact) => "artifact",
        Some(CardType::Enchantment) => "enchantment",
        Some(CardType::Land) => "land",
        Some(CardType::Planeswalker) => "planeswalker",
        Some(CardType::Instant) | Some(CardType::Sorcery) | Some(CardType::Battle) | None => {
            "permanent"
        }
    }
}

/// The words that go between "name a" and "card" for a [`NamedCardClass`] — the class of
/// card a permanent's controller may name as it enters (CR 614.12).
fn named_card_class_noun(class: NamedCardClass) -> &'static str {
    match class {
        NamedCardClass::NonbasicLand => "nonbasic land",
    }
}

/// Whether a static ability's subject is **plural** — a class of permanents rather than
/// the single permanent that printed the ability.
///
/// Every verb that has to agree with its subject reads it. The `source` scope is a class
/// of **one**, and it is the card's own name — "Palladia-Mors, the Ruiner **has** hexproof",
/// never "have" — while every other scope names a plural class; and an as-though clause
/// carries a pronoun back to the subject ("as though **it** didn't have defender"), where
/// there is no wording right for both numbers.
fn subject_is_plural(affects: &StaticAffects) -> bool {
    match affects {
        // Both classes of one are singular: one card's name, one enchanted creature.
        StaticAffects::Source | StaticAffects::AttachedTo => false,
        StaticAffects::CreaturesYouControl { .. }
        | StaticAffects::PermanentsYourOpponentsControl { .. } => true,
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
        // "Yet" is the whole clause: the window reaches back to the moment the permanent
        // entered, so the sentence says so rather than naming a turn.
        StaticCondition::SourceHasNotDealtDamage => "it hasn't dealt damage yet".to_string(),
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
        // A subtype with no card type beside it **is** the noun: a card says "four or
        // more Demons", never "four or more Demon permanents". With a type it stays an
        // adjective, which is how a card prints that too — "an Ajani planeswalker".
        if permanents.card_type.is_some() {
            noun.push(' ');
            noun.push_str(permanent_noun(permanents.card_type));
        }
    } else {
        noun.push_str(permanent_noun(permanents.card_type));
    }
    let phrase = match count {
        1 => format!("{} {noun}", indefinite_article(&noun)),
        n => format!("{} or more {}", number(n), plural(&noun)),
    };
    // A power bound trails the noun, where a card prints it: "a creature with power 4
    // or greater".
    let phrase = match permanents.min_power {
        None => phrase,
        Some(min) => format!("{phrase} with power {min} or greater"),
    };
    // And the names clause trails that, where a card prints it: "four or more Demons
    // with different names". A count of one cannot have different names and does not
    // claim to.
    if permanents.distinct_names && count > 1 {
        format!("{phrase} with different names")
    } else {
        phrase
    }
}

/// "a" or "an" for `noun` — read off its first letter, which is right for every noun
/// this composer can produce.
pub(super) fn indefinite_article(noun: &str) -> &'static str {
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
/// ability ([`subject_is_plural`]). A card that modifies itself prints its own name as the
/// subject, and "Grasping Scoundrel get +1/+0" is not a sentence; the as-though clause
/// reads it for a second reason, being the only one that refers back to its subject with a
/// pronoun. Taken as a flag rather than read off the selector here so this function stays
/// about the predicate; the caller owns the subject and therefore owns its number.
fn static_verb(modification: &StaticModification, plural: bool) -> String {
    match modification {
        StaticModification::PowerToughness { power, toughness } => {
            let verb = if plural { "get" } else { "gets" };
            format!("{verb} {power:+}/{toughness:+}")
        }
        // "have", not "gain": a static ability is continuously true, where a spell's
        // grant is an event. The distinction is the whole difference between an anthem
        // and a pump.
        StaticModification::GrantKeyword { keyword } => {
            let verb = if plural { "have" } else { "has" };
            format!("{verb} {}", keyword_word(*keyword))
        }
        // The card's own words, and they name the step: a permanent under this is not
        // "tapped" by anything, it simply is not untapped by the one rule that would have.
        StaticModification::DoesNotUntap => {
            let (verb, possessive) = if plural {
                ("don't", "their")
            } else {
                ("doesn't", "its")
            };
            format!("{verb} untap during {possessive} controller's untap step")
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
        StaticModification::LoseAllAbilities => "lose all abilities".to_string(),
        // The granted ability is quoted, which is how a printed card writes one — and the
        // words inside the quotes come from the same composer that writes it when the
        // permanent has it, so the promise and the ability read identically. It speaks of
        // itself as "this permanent": the sentence is about a class, so there is no one
        // card name it could use.
        StaticModification::GrantAbility { ability } => {
            format!("have \"{}\"", ability_text("this permanent", ability))
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
    if !attachment.types.is_empty() || !attachment.subtypes.is_empty() {
        // CR 613 layer 4, and the card's own words for it: the type is *added*, which is
        // the whole difference between this and a card that replaces what its host is.
        let mut what: Vec<String> = attachment.subtypes.clone();
        what.extend(
            attachment
                .types
                .iter()
                .map(|&kind| card_type_word(kind).to_string()),
        );
        let named = what.join(" ");
        lines.push(sentence_case(&format!(
            "{host} is {} {named} in addition to its other types.",
            indefinite_article(&named)
        )));
    }
    if !attachment.abilities.is_empty() {
        // A granted ability is quoted, because the words inside it are the host's: it is
        // the *land* that taps for mana and the *creature* that dies, so the ability is
        // worded against that object and set apart from the sentence granting it. "Has",
        // not "gains", for the reason the keyword line uses it — an attachment's grant is
        // continuously true rather than an event.
        for ability in &attachment.abilities {
            lines.push(sentence_case(&format!(
                "{host} has \"{}\"",
                ability_text(granted_subject(attachment.attach_to), ability)
            )));
        }
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
