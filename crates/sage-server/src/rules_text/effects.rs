//! One effect as a clause, and every phrase that goes into building one: the target it
//! names, the class it affects, the condition it is gated on, and the pronouns that make
//! it read as a sentence.

use super::*;

/// One effect as a lowercase clause with no trailing period, so it can either stand
/// alone as a sentence ([`finish`]) or be embedded after a trigger or a cost.
///
/// Exhaustive by design: a new [`Effect`] variant must be given words here or the
/// workspace does not build (ADR 0008 §7).
pub(super) fn effect_clause(source: &str, effect: &Effect) -> String {
    match effect {
        Effect::AddMana { color, amount } => format!("add {}", pips(*color, *amount)),
        Effect::AddColorlessMana { amount } => format!("add {}", colorless_pips(*amount)),
        Effect::DrawCard { count } => match count {
            1 => "draw a card".to_string(),
            n => format!("draw {} cards", number(u32::from(*n))),
        },
        Effect::Tap { target } => format!("tap {}", target_noun(*target)),
        // Two sentences where a card prints two, joined by the caller: the tap names
        // whose creatures, and the skip refers back to them.
        Effect::TapAll {
            player_ref,
            skip_next_untap,
        } => {
            // "you control", but "target player controls" — English agrees the verb
            // with the subject, and only the second person drops the -s.
            let controls = match player_ref {
                PlayerRef::Controller => "control",
                PlayerRef::EachOpponent | PlayerRef::TargetPlayer | PlayerRef::TargetOpponent => {
                    "controls"
                }
            };
            let tap = format!(
                "tap all creatures {} {controls}",
                subject_pronoun(*player_ref)
            );
            if *skip_next_untap {
                format!(
                    "{tap}. Those creatures don't untap during {} next untap step",
                    possessive_pronoun(*player_ref)
                )
            } else {
                tap
            }
        }
        Effect::CounterSpell { target } => format!("counter {}", target_noun(*target)),
        // A damage source is named, so a player can tell what dealt it (CR 120.3).
        Effect::DealDamage { subject, amount } => {
            format!(
                "{source} deals {amount} damage to {}",
                damage_recipient(subject)
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
        // "on each of up to two target creatures" when the effect may name more than
        // one, "on target creature" when it names exactly one — read off the same count
        // the engine builds the target slots from, so the sentence and the slots cannot
        // disagree.
        Effect::PutCounters {
            target,
            targets,
            counter,
            count,
        } => format!(
            "put {} on {}",
            counters(*counter, *count),
            target_phrase(*target, *targets)
        ),
        // One effect, one target, one sentence: the keywords a pump also grants are
        // granted to the same creature, so they read as a second verb on the same
        // subject rather than as a sentence with a subject of its own.
        Effect::Pump {
            target,
            power,
            toughness,
            keywords,
        } => {
            let numbers = format!("gets {power:+}/{toughness:+}");
            let verbs = if keywords.is_empty() {
                numbers
            } else {
                let words: Vec<&str> = keywords.iter().map(|&kw| keyword_word(kw)).collect();
                format!("{numbers} and gains {}", list_words(&words))
            };
            format!("{} {verbs} until end of turn", target_noun(*target))
        }
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
            mass_subject(affects)
        ),
        Effect::GrantKeywordAll { affects, keyword } => format!(
            "{} gain {} until end of turn",
            mass_subject(affects),
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
            restriction_predicate(restriction)
        ),
        Effect::RestrictAll {
            affects,
            restriction,
        } => format!(
            "{} {} this turn",
            mass_subject(affects),
            restriction_predicate(restriction)
        ),
        // A self-referential effect names the source by name, so the sentence reads
        // the way the card does rather than as an anonymous "this".
        Effect::PumpSelf { power, toughness } => {
            format!("{source} gets {power:+}/{toughness:+} until end of turn")
        }
        Effect::RestrictSelf { restriction } => {
            format!("{source} {} this turn", restriction_predicate(restriction))
        }
        Effect::PutCountersOnSelf { counter, count } => {
            format!("put {} on {source}", counters(*counter, *count))
        }
        Effect::ReturnToHand { target } => {
            format!("return {} to its owner's hand", target_noun(*target))
        }
        // Three sentences where a card prints three, joined here because they are three
        // things done to *one* target: the theft, the untap, and the keywords. Each is
        // stated only when the effect actually does it, so a plain steal reads as one
        // sentence.
        Effect::GainControl {
            target,
            untap,
            keywords,
        } => {
            let mut text = format!("gain control of {} until end of turn", target_noun(*target));
            if *untap {
                text.push_str(". Untap it");
            }
            if !keywords.is_empty() {
                let words: Vec<&str> = keywords.iter().map(|&kw| keyword_word(kw)).collect();
                text.push_str(&format!(
                    ". It gains {} until end of turn",
                    list_words(&words)
                ));
            }
            text
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
        //
        // "Attacking" trails the noun as a relative clause, because that is where a card
        // prints it and because the keywords already occupy the position before it:
        // "two 1/1 white Cat creature tokens with lifelink that are attacking".
        Effect::CreateToken {
            token,
            count,
            count_of,
            player_ref,
            tapped,
            attacking,
        } => {
            let attacking = match (*attacking, *count) {
                (false, _) => "",
                (true, 1) => " that's attacking",
                (true, _) => " that are attacking",
            };
            // A counted number is stated as the *rule* rather than as a number, the way
            // every other count-derived amount is: how many there will be does not exist
            // until the effect resolves.
            let each = match count_of {
                None => String::new(),
                Some(count_of) => format!(" for each {}", count_noun(count_of)),
            };
            format!(
                "{} {}{attacking}{each}",
                conjugate(*player_ref, "create"),
                token_noun(token, u32::from(*count), *tapped)
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
        // An emblem reads as the card prints it: the emblem, then its abilities in
        // quotes. Composed from the abilities the effect actually hands out, so the
        // sentence and the object cannot disagree.
        Effect::CreateEmblem {
            abilities,
            player_ref,
        } => {
            let text: Vec<String> = abilities
                .iter()
                .map(|ability| ability_text("this emblem", ability))
                .collect();
            format!(
                "{} an emblem with \"{}\"",
                conjugate(*player_ref, "get"),
                text.join(" ")
            )
        }
        // The intervening-if clause, written the way a card writes it: the condition
        // first, then what it makes happen, and — when there is an `otherwise` — the
        // fallback as its own sentence.
        Effect::Conditional {
            condition,
            then,
            otherwise,
        } => {
            let head = format!(
                "if {}, {}",
                condition_clause(condition),
                clauses(source, then)
            );
            if otherwise.is_empty() {
                head
            } else {
                format!("{head}. Otherwise, {}", clauses(source, otherwise))
            }
        }
        Effect::ReturnCardToBattlefield { target, tapped } => format!(
            "return {} to the battlefield{}",
            target_noun(*target),
            if *tapped { " tapped" } else { "" }
        ),
        Effect::ReturnCardToHand { target, targets } => format!(
            "return {} to its owner's hand",
            target_phrase(*target, *targets)
        ),
        // A count-derived amount says the *rule* rather than a number, because the
        // number does not exist until the effect resolves.
        Effect::GainLifeByCount {
            player_ref,
            amount_per,
            count_of,
        } => format!(
            "{} {amount_per} life for each {}",
            conjugate(*player_ref, "gain"),
            count_noun(count_of)
        ),
        Effect::DealDamageByCount {
            subject,
            amount_per,
            count_of,
        } => format!(
            "{source} deals {amount_per} damage to {} for each {}",
            damage_recipient(subject),
            count_noun(count_of)
        ),
        Effect::ExileGraveyard { player_ref } => {
            format!("exile {}'s graveyard", possessive_subject(*player_ref))
        }
        Effect::PutOnTopOfLibrary { target } => {
            format!("put {} on top of its owner's library", target_noun(*target))
        }
        // The equip action (CR 702.6b). The subject is the source — an Equipment attaches
        // *itself* — so the sentence names it, which is also what makes the dock button
        // read as an instruction about a specific sword rather than about equipment in
        // general.
        Effect::Attach { target } => {
            format!("attach {source} to {}", target_noun(*target))
        }
        Effect::PumpByCount {
            target,
            power_per,
            toughness_per,
            count_of,
        } => format!(
            "{} gets {}X/{}X until end of turn, where X is the number of {}",
            target_noun(*target),
            sign(*power_per),
            sign(*toughness_per),
            count_subject(count_of),
        ),
        // The colors are the player's, so the sentence says how many mana and leaves
        // the colors to them — exactly what the card says.
        Effect::AddManaAnyColor {
            amount,
            restriction,
        } => {
            let mana = if *amount == 1 {
                "add one mana of any color".to_string()
            } else {
                format!(
                    "add {} mana in any combination of colors",
                    number(u32::from(*amount))
                )
            };
            match restriction {
                Some(restriction) => format!(
                    "{mana}. Spend this mana only to {}",
                    restriction_phrase(restriction)
                ),
                None => mana,
            }
        }
        Effect::AddRestrictedMana {
            color,
            amount,
            restriction,
        } => format!(
            "add {}. Spend this mana only to {}",
            pips(*color, *amount),
            restriction_phrase(restriction)
        ),
        Effect::AllowCastingFromGraveyard { player_ref, filter } => format!(
            "{} may cast {} from {} graveyard this turn",
            subject_pronoun(*player_ref),
            filter_noun(filter, true),
            possessive_pronoun(*player_ref),
        ),
        Effect::IgnoreHexproof { player_ref } => format!(
            "spells and abilities {} control may target as though hexproof were not there \
             this turn",
            subject_pronoun(*player_ref),
        ),
    }
}

/// A target group as the phrase a card writes it in: `target creature` for the ordinary
/// single-target effect, `each of up to two target creatures` for the one that may name
/// fewer than it allows.
fn target_phrase(spec: TargetSpec, count: TargetCount) -> String {
    match count {
        TargetCount::Exactly(1) => target_noun(spec).to_string(),
        TargetCount::Exactly(n) => format!(
            "each of {} {}",
            number(u32::from(n)),
            plural_target_noun(spec)
        ),
        TargetCount::UpTo(n) => format!(
            "each of up to {} {}",
            number(u32::from(n)),
            plural_target_noun(spec)
        ),
    }
}

/// A target spec pluralized — "target creatures" — for a group that names more than one.
fn plural_target_noun(spec: TargetSpec) -> String {
    format!("{}s", target_noun(spec))
}

/// A signed per-unit amount as the `-` or `+` a card prints before its X.
fn sign(amount: i32) -> &'static str {
    if amount < 0 {
        "-"
    } else {
        "+"
    }
}

/// The class a [`PermanentCount`] counts, as the noun phrase an X clause names.
fn count_subject(count: &PermanentCount) -> String {
    let noun = match (&count.subtype, count.card_type) {
        (Some(subtype), _) => format!("{subtype}s"),
        (None, Some(card_type)) => format!("{}s", card_type_word(card_type)),
        (None, None) => "permanents".to_string(),
    };
    let class = match count.scope {
        CountScope::YouControl => format!("{noun} you control"),
        CountScope::OpponentsControl => format!("{noun} your opponents control"),
        CountScope::Any => noun,
    };
    power_clause(&class, count)
}

/// A class with its power bound trailing it, where a card prints it: "creatures you
/// control **with power 4 or greater**". One function so the counted and the
/// distributive forms cannot word the same bound differently.
fn power_clause(class: &str, count: &PermanentCount) -> String {
    match count.min_power {
        None => class.to_string(),
        Some(min) => format!("{class} with power {min} or greater"),
    }
}

/// The same class in the **singular**, for the distributive "for each …" of a
/// count-derived amount: "1 life for each creature you control".
///
/// A separate function from [`count_subject`] for the reason [`mass_recipient`] is
/// separate from [`mass_subject`] — English puts a class in the plural when it is
/// counted and in the singular after "each", and one function per position keeps both
/// exhaustive.
pub(super) fn count_noun(count: &PermanentCount) -> String {
    let mut noun = String::new();
    if let Some(color) = count.color {
        noun.push_str(color.word());
        noun.push(' ');
    }
    match (&count.subtype, count.card_type) {
        (Some(subtype), _) => noun.push_str(subtype),
        (None, Some(card_type)) => noun.push_str(card_type_word(card_type)),
        (None, None) => noun.push_str("permanent"),
    }
    let class = match count.scope {
        CountScope::YouControl => format!("{noun} you control"),
        CountScope::OpponentsControl => format!("{noun} your opponents control"),
        CountScope::Any => noun,
    };
    power_clause(&class, count)
}

/// A card type as the word a rules sentence uses.
pub(crate) fn card_type_word(card_type: CardType) -> &'static str {
    match card_type {
        CardType::Land => "land",
        CardType::Creature => "creature",
        CardType::Artifact => "artifact",
        CardType::Enchantment => "enchantment",
        CardType::Planeswalker => "planeswalker",
        CardType::Battle => "battle",
        CardType::Instant => "instant",
        CardType::Sorcery => "sorcery",
    }
}

/// What a mana restriction allows, as the clause following "spend this mana only to".
///
/// Crate-visible because the *prompt* for a color choice needs the same words the card
/// is written in: a player choosing a color of Dragon-only mana must be told so, and
/// two phrasings of one restriction would be two things to keep in step.
pub(crate) fn restriction_phrase(restriction: &ManaRestriction) -> String {
    match restriction {
        ManaRestriction::SpellsWithSubtype { subtype } => format!("cast {subtype} spells"),
    }
}

/// An intervening-if condition as the clause following the word "if".
fn condition_clause(condition: &Condition) -> String {
    match condition {
        // The same composer the static `as long as …` clause uses. It was worth sharing
        // rather than duplicating: the local phrasing read "you control 1 or more
        // creatures you control", because the count's own scope already says "you
        // control" and this clause said it again.
        Condition::ControlsAtLeast { permanents, count } => {
            format!("you control {}", counted_permanents(permanents, *count))
        }
        Condition::MilledThisWay { filter } => {
            format!(
                "at least one {} was milled this way",
                filter_noun(filter, false)
            )
        }
        Condition::DiscardedThisWay => "a card is discarded this way".to_string(),
        // A threshold of one is the "any" a card leaves unwritten: "if you gained life
        // this turn", never "if you gained 1 or more life this turn".
        Condition::GainedLifeThisTurn { amount: 1 } => "you gained life this turn".to_string(),
        Condition::GainedLifeThisTurn { amount } => {
            format!("you gained {} or more life this turn", number(*amount))
        }
    }
}

/// The subject pronoun a player reference reads as at the start of a clause.
fn subject_pronoun(player_ref: PlayerRef) -> &'static str {
    match player_ref {
        PlayerRef::Controller => "you",
        PlayerRef::EachOpponent => "each opponent",
        PlayerRef::TargetPlayer => "target player",
        PlayerRef::TargetOpponent => "target opponent",
    }
}

/// The possessive a player reference reads as — "your graveyard", "their graveyard".
fn possessive_pronoun(player_ref: PlayerRef) -> &'static str {
    match player_ref {
        PlayerRef::Controller => "your",
        PlayerRef::EachOpponent | PlayerRef::TargetPlayer | PlayerRef::TargetOpponent => "their",
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
/// third-person subject ("target player discards a card") is returned unchanged, which
/// is what an optional effect that targets wants: "you may destroy target artifact"
/// keeps its object and loses nothing.
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
        CardFilter::Creature { max_power, subtype } => {
            let kind = match subtype {
                Some(subtype) => format!("{subtype} creature {card}"),
                None => format!("creature {card}"),
            };
            match max_power {
                Some(cap) => format!("{kind} with power {cap} or less"),
                None => kind,
            }
        }
        CardFilter::CreatureOrLand => format!("creature or land {card}"),
        CardFilter::Permanent => format!("permanent {card}"),
        CardFilter::Subtype { subtype } => format!("{subtype} {card}"),
        CardFilter::NoncreatureNonland => format!("noncreature, nonland {card}"),
        // Deliberately not the card's name: the formatter composes one sentence for a
        // *definition*, and the definition is the one that is searching.
        CardFilter::SameNameAsSource => format!("{card} with this card's name"),
        // Printed colour, as a card writes it: 'a white card'.
        CardFilter::Color { color } => format!("{} {card}", color.word()),
        CardFilter::InstantOrSorcery => format!("instant or sorcery {card}"),
        CardFilter::Artifact => format!("artifact {card}"),
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

/// The class a mass, non-targeting effect names, as the subject of its sentence. A
/// subtype replaces the noun outright — "Dragons you control", never "Dragon creatures
/// you control", which is not how a card is written.
fn mass_subject(affects: &MassAffects) -> String {
    match affects {
        MassAffects::CreaturesYouControl { subtype: None } => "creatures you control".to_string(),
        MassAffects::CreaturesYouControl {
            subtype: Some(subtype),
        } => format!("{subtype}s you control"),
        MassAffects::EachCreature => "creatures".to_string(),
        MassAffects::CreaturesYourOpponentsControl => {
            "creatures your opponents control".to_string()
        }
        MassAffects::CreaturesWithoutFlying => "creatures without flying".to_string(),
        MassAffects::AttackingCreatures => "attacking creatures".to_string(),
    }
}

/// The same class as the **object** of a sentence — what damage is dealt *to*.
///
/// Separate from [`mass_subject`] because English is: a class is a bare plural when it
/// acts ("creatures you control get +2/+1") and a distributive "each" when it is acted
/// on ("deals 2 damage to each creature you control"). One function per position keeps
/// both exhaustive, so a new [`MassAffects`] variant must be given words for each.
fn mass_recipient(affects: &MassAffects) -> String {
    match affects {
        MassAffects::CreaturesYouControl { subtype: None } => {
            "each creature you control".to_string()
        }
        MassAffects::CreaturesYouControl {
            subtype: Some(subtype),
        } => format!("each {subtype} you control"),
        MassAffects::EachCreature => "each creature".to_string(),
        MassAffects::CreaturesYourOpponentsControl => {
            "each creature your opponents control".to_string()
        }
        MassAffects::CreaturesWithoutFlying => "each creature without flying".to_string(),
        MassAffects::AttackingCreatures => "each attacking creature".to_string(),
    }
}

/// Who or what damage is dealt to (CR 120.3), as a noun phrase.
///
/// The three subjects read as one sentence shape — "deals 2 damage to *any target*",
/// "…to *each opponent*", "…to *each creature*" — so a player reads a class-damage
/// effect the same way they read a targeted one, minus the word "target".
fn damage_recipient(subject: &DamageSubject) -> String {
    match subject {
        DamageSubject::Target(spec) => target_noun(*spec).to_string(),
        DamageSubject::Players(player_ref) => player_noun(*player_ref).to_string(),
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
