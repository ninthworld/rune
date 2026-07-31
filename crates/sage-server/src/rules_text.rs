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
    Ability, AuraGrant, CardData, CardFilter, Chooser, Color, CombatRestriction, Cost, CounterKind,
    DamageSubject, Effect, FoundDestination, Keyword, MassAffects, ObservedPermanent,
    ObservedSpell, PlayerRef, StaticAffects, StaticModification, TargetSpec, TokenData,
    TriggerCondition, TriggerStep, TurnScope,
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
    let noun = observes.subtype().unwrap_or("creature");
    match observes {
        ObservedPermanent::CreaturesYouControl { .. } => {
            format!("{article} {noun} you control")
        }
        ObservedPermanent::AnyCreature { .. } => format!("{article} {noun}"),
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
        Effect::DealDamage { subject, amount } => {
            format!(
                "{source} deals {amount} damage to {}",
                damage_recipient(*subject)
            )
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
        // A restriction is already a predicate ("can't be blocked"), so it needs no
        // verb of its own — only the subject and the duration around it.
        Effect::Restrict {
            target,
            restriction,
        } => format!(
            "{} {} this turn",
            target_noun(*target),
            restriction_predicate(*restriction)
        ),
        Effect::RestrictAll {
            affects,
            restriction,
        } => format!(
            "{} {} this turn",
            mass_subject(*affects),
            restriction_predicate(*restriction)
        ),
        // A self-referential effect names the source by name, so the sentence reads
        // the way the card does rather than as an anonymous "this".
        Effect::PumpSelf { power, toughness } => {
            format!("{source} gets {power:+}/{toughness:+} until end of turn")
        }
        Effect::RestrictSelf { restriction } => {
            format!("{source} {} this turn", restriction_predicate(*restriction))
        }
        Effect::PutCountersOnSelf { counter, count } => {
            format!("put {} on {source}", counters(*counter, *count))
        }
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
        // A discard says who chooses when that is not the discarding player, because
        // the difference between "discards a card" and "you choose it" is the whole
        // card for a hand-attack spell.
        Effect::Discard {
            player_ref,
            count,
            chosen_by,
            filter,
        } => {
            let cards = quantity(u32::from(*count), filter);
            match chosen_by {
                Chooser::Owner => format!("{} {cards}", conjugate(*player_ref, "discard")),
                Chooser::Controller => format!(
                    "you look at {}'s hand and choose {cards} from it; {} it",
                    possessive_subject(*player_ref),
                    conjugate(*player_ref, "discard"),
                ),
            }
        }
        // A token is described the way a card prints it: how many, tapped or not, then
        // the token's own characteristics as a noun phrase ("two 1/1 white Soldier
        // creature tokens"). Composed from the same `TokenData` the engine creates the
        // object from, so the sentence and the object cannot disagree.
        Effect::CreateToken {
            token,
            count,
            player_ref,
            tapped,
        } => {
            let tapped = if *tapped { "tapped " } else { "" };
            format!(
                "{} {tapped}{}",
                conjugate(*player_ref, "create"),
                token_noun(token, u32::from(*count))
            )
        }
        Effect::Scry { count } => format!("scry {}", number(u32::from(*count))),
        Effect::LookAtTop {
            count,
            take,
            filter,
            destination,
        } => format!(
            "look at the top {} cards of your library, you may put {} from among them {}, \
             then put the rest on the bottom of your library in a random order",
            number(u32::from(*count)),
            up_to(u32::from(*take), filter),
            destination_phrase(*destination),
        ),
        Effect::SearchLibrary {
            take,
            filter,
            destination,
        } => format!(
            "search your library for {}, put {} {}, then shuffle",
            up_to(u32::from(*take), filter),
            if *take == 1 { "it" } else { "them" },
            destination_phrase(*destination),
        ),
        // An optional effect reads as the card prints it. The costed form is two
        // sentences even inside a larger clause — "you may pay {1}. If you do, draw a
        // card" — because that is how the condition is written on every card that has
        // one, and running it together with "and" would read as though both halves
        // happened.
        Effect::May { cost, effects } => {
            let what = clauses(source, effects);
            match cost {
                Some(cost) => format!("you may pay {cost}. If you do, {what}"),
                None => format!("you may {}", without_you(&what)),
            }
        }
    }
}

/// The question an optional effect puts to its controller, as the words on the button
/// they answer it with — "Draw a card?", "Pay {1} to draw a card?".
///
/// Composed from the effects themselves rather than authored per card, exactly as the
/// card's own rules text is: one vocabulary, so the prompt and the printed sentence can
/// never describe the same offer two different ways. The source is written as "this"
/// because the question is asked mid-resolution, when the object that asked it may
/// already have left the battlefield.
#[must_use]
pub(crate) fn optional_effect_question(cost: Option<&str>, effects: &[Effect]) -> String {
    let what = clauses("this", effects);
    match cost {
        Some(cost) => format!("Pay {cost} to {}?", without_you(&what)),
        None => format!("{}?", sentence_case(&what)),
    }
}

/// A clause with a leading "you " stripped, for a position whose subject is already
/// stated — "you may **draw a card**", "Pay {1} to **gain 3 life**". A clause with a
/// third-person subject ("target player discards a card") is returned unchanged, and
/// never reaches these positions anyway: an optional effect's contents may not target.
fn without_you(clause: &str) -> &str {
    clause.strip_prefix("you ").unwrap_or(clause)
}

/// `count` cards of the class `filter` names, as a noun phrase — "a card", "two cards",
/// "a creature card with power 2 or less".
fn quantity(count: u32, filter: &CardFilter) -> String {
    if count == 1 {
        format!("a {}", filter_noun(filter, false))
    } else {
        format!("{} {}", number(count), filter_noun(filter, true))
    }
}

/// The same phrase under an "up to" bound, for a take a player may decline.
fn up_to(count: u32, filter: &CardFilter) -> String {
    if count == 1 {
        format!("up to one {}", filter_noun(filter, false))
    } else {
        format!("up to {} {}", number(count), filter_noun(filter, true))
    }
}

/// The noun a [`CardFilter`] names, singular or plural, as the words a card would print
/// — "card" for the unrestricted class, so "discard a **card**" and "up to one **land
/// card**" both read as one sentence rather than needing a special case per call site.
fn filter_noun(filter: &CardFilter, plural: bool) -> String {
    let card = if plural { "cards" } else { "card" };
    match filter {
        CardFilter::Any => card.to_string(),
        CardFilter::Land => format!("land {card}"),
        CardFilter::Creature { max_power: None } => format!("creature {card}"),
        CardFilter::Creature {
            max_power: Some(cap),
        } => format!("creature {card} with power {cap} or less"),
        CardFilter::NoncreatureNonland => format!("noncreature, nonland {card}"),
        // Deliberately not the card's name: the formatter composes one sentence for a
        // *definition*, and the definition is the one that is searching.
        CardFilter::SameNameAsSource => format!("{card} with this card's name"),
    }
}

/// Where a found card goes, as the trailing phrase of a search or a look.
fn destination_phrase(destination: FoundDestination) -> &'static str {
    match destination {
        FoundDestination::Hand => "into your hand",
        FoundDestination::Battlefield => "onto the battlefield",
        FoundDestination::BattlefieldTapped => "onto the battlefield tapped",
    }
}

/// The subject of a [`PlayerRef`] as a possessive noun phrase — "target opponent" in
/// "you look at **target opponent**'s hand". The counterpart of [`conjugate`] for a
/// clause whose verb belongs to someone else.
fn possessive_subject(player_ref: PlayerRef) -> &'static str {
    match player_ref {
        PlayerRef::Controller => "your own",
        PlayerRef::EachOpponent => "each opponent",
        PlayerRef::TargetPlayer => "target player",
        PlayerRef::TargetOpponent => "target opponent",
    }
}

/// The class a mass, non-targeting effect names, as the subject of its sentence.
fn mass_subject(affects: MassAffects) -> &'static str {
    match affects {
        MassAffects::CreaturesYouControl => "creatures you control",
        MassAffects::EachCreature => "creatures",
        MassAffects::CreaturesYourOpponentsControl => "creatures your opponents control",
        MassAffects::CreaturesWithoutFlying => "creatures without flying",
    }
}

/// The same class as the **object** of a sentence — what damage is dealt *to*.
///
/// Separate from [`mass_subject`] because English is: a class is a bare plural when it
/// acts ("creatures you control get +2/+1") and a distributive "each" when it is acted
/// on ("deals 2 damage to each creature you control"). One function per position keeps
/// both exhaustive, so a new [`MassAffects`] variant must be given words for each.
fn mass_recipient(affects: MassAffects) -> &'static str {
    match affects {
        MassAffects::CreaturesYouControl => "each creature you control",
        MassAffects::EachCreature => "each creature",
        MassAffects::CreaturesYourOpponentsControl => "each creature your opponents control",
        MassAffects::CreaturesWithoutFlying => "each creature without flying",
    }
}

/// Who or what damage is dealt to (CR 120.3), as a noun phrase.
///
/// The three subjects read as one sentence shape — "deals 2 damage to *any target*",
/// "…to *each opponent*", "…to *each creature*" — so a player reads a class-damage
/// effect the same way they read a targeted one, minus the word "target".
fn damage_recipient(subject: DamageSubject) -> &'static str {
    match subject {
        DamageSubject::Target(spec) => target_noun(spec),
        DamageSubject::Players(player_ref) => player_noun(player_ref),
        DamageSubject::Permanents(affects) => mass_recipient(affects),
    }
}

/// A [`PlayerRef`] as a bare noun phrase, for an effect that acts *on* the player
/// rather than conjugating a verb after them ([`conjugate`] covers that position).
fn player_noun(player_ref: PlayerRef) -> &'static str {
    match player_ref {
        PlayerRef::Controller => "you",
        PlayerRef::EachOpponent => "each opponent",
        PlayerRef::TargetPlayer => "target player",
        PlayerRef::TargetOpponent => "target opponent",
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
        Keyword::Hexproof => "hexproof",
    }
}

/// A combat restriction as the **predicate** of a sentence — the words that follow a
/// subject, with no subject, verb agreement, or duration of their own.
///
/// Predicates rather than sentences because the same restriction has to read correctly
/// after four different subjects: a card's own name (`Bristling Boar can't be blocked
/// by more than one creature.`), an Aura's host (`Enchanted creature can't block.`), a
/// chosen target (`Target creature can't be blocked this turn.`), and a class
/// (`Creatures without flying can't block this turn.`). "Can't" is invariant across
/// singular and plural subjects, so one string serves all four.
fn restriction_predicate(restriction: CombatRestriction) -> String {
    match restriction {
        CombatRestriction::CantAttack => "can't attack".to_string(),
        CombatRestriction::CantBlock => "can't block".to_string(),
        CombatRestriction::CantBeBlocked => "can't be blocked".to_string(),
        CombatRestriction::CantBeBlockedBy(color) => {
            format!("can't be blocked by {} creatures", color.word())
        }
        CombatRestriction::CantBeBlockedByMoreThanOne => {
            "can't be blocked by more than one creature".to_string()
        }
    }
}

/// A token as the noun phrase a card prints it as: `"a 1/1 red Goblin creature
/// token"`, `"two 1/1 white Soldier creature tokens with lifelink"`.
///
/// Assembled in the order a real card states it — count, power/toughness, colors,
/// subtypes, card types, the word "token", then any keywords — from the same
/// [`TokenData`] the engine builds the object from. A token with abilities beyond
/// keywords says so without reciting them: the object's own rules text
/// ([`token_rules_text`]) is what a player reads once it is on the battlefield, and
/// repeating it inside the creating card's sentence would be a second place for the
/// same words to drift.
fn token_noun(token: &TokenData, count: u32) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let (Some(power), Some(toughness)) = (token.power, token.toughness) {
        parts.push(format!("{power}/{toughness}"));
    }
    parts.extend(token.colors.iter().map(|color| color.word().to_string()));
    parts.extend(token.subtypes.iter().cloned());
    parts.extend(token.types.iter().map(|t| t.display().to_lowercase()));
    parts.push(if count == 1 { "token" } else { "tokens" }.to_string());
    let mut noun = format!(
        "{} {}",
        if count == 1 {
            "a".to_string()
        } else {
            number(count)
        },
        parts.join(" ")
    );
    if !token.keywords.is_empty() {
        let words: Vec<&str> = token.keywords.iter().map(|&kw| keyword_word(kw)).collect();
        noun.push_str(&format!(" with {}", list_words(&words)));
    }
    if !token.abilities.is_empty() {
        noun.push_str(" with an ability");
    }
    noun
}

/// A short list of words as English reads it: `"a"`, `"a and b"`, `"a, b, and c"`.
fn list_words(words: &[&str]) -> String {
    match words {
        [] => String::new(),
        [one] => (*one).to_string(),
        [first, second] => format!("{first} and {second}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
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
mod tests;
