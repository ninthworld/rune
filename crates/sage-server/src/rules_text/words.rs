//! The vocabulary the clauses are built from: mana symbols, keyword and restriction
//! wording, nouns for a target or a token, numbers, and the punctuation and casing that
//! finish a sentence.

use super::*;

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
///
/// Crate-visible because the *prompt* for a cost a player picks the payment for needs the
/// same words the cost line is written in: "Sacrifice another creature" asked as a question
/// and printed on the card is one phrase, and two renderings of one cost would be two things
/// to keep in step.
pub(crate) fn cost_symbol(cost: &Cost) -> String {
    match cost {
        Cost::Tap => "{T}".to_string(),
        // A mana cost is already written in the notation a player reads it in, so it
        // is passed through rather than re-rendered from a parse.
        Cost::Mana { mana } => mana.clone(),
        // A loyalty cost is written as the signed number printed on the card
        // (CR 606.1): `+1`, `0`, `−2`. The minus is the typographic minus sign the
        // card uses, not a hyphen, so the symbol reads as a card rather than as code.
        Cost::Loyalty { amount } => match amount {
            0 => "0".to_string(),
            n if *n > 0 => format!("+{n}"),
            n => format!("\u{2212}{}", n.unsigned_abs()),
        },
        // A sacrifice and a counter removal are words rather than symbols, and are
        // written as the card writes them — in the cost line, beside the symbols.
        Cost::SacrificeThis => "Sacrifice this permanent".to_string(),
        Cost::RemoveCounters { counter, count } => {
            format!("Remove {} from this permanent", counters(*counter, *count))
        }
        // The two costs the *player* picks the payment for read as the card writes them:
        // "Sacrifice another creature", "Sacrifice a Goblin", "Discard a card". The same
        // phrase labels the slot the choice is answered on, so what a player is asked and
        // what the card says are one string.
        Cost::Sacrifice {
            card_type,
            subtype,
            another,
        } => {
            let noun = sacrifice_noun(*card_type, subtype.as_deref());
            let article = if *another {
                "another".to_string()
            } else {
                indefinite_article(&noun).to_string()
            };
            format!("Sacrifice {article} {noun}")
        }
        Cost::Discard { count: 1 } => "Discard a card".to_string(),
        Cost::Discard { count } => {
            format!("Discard {} cards", number(u32::from(*count)))
        }
    }
}

/// The class of permanent a sacrifice cost takes, as the noun a card writes: the
/// `Goblin` of `Sacrifice a Goblin`, the `creature` of `Sacrifice another creature`, and
/// `permanent` for a cost that names neither.
///
/// The same subtype-wins-over-type ordering [`count_noun`](super::effects::count_noun)
/// uses, because a card writes the class the same way wherever it appears.
fn sacrifice_noun(card_type: Option<CardType>, subtype: Option<&str>) -> String {
    match (subtype, card_type) {
        (Some(subtype), Some(card_type)) => format!("{subtype} {}", card_type_word(card_type)),
        (Some(subtype), None) => subtype.to_string(),
        (None, Some(card_type)) => card_type_word(card_type).to_string(),
        (None, None) => "permanent".to_string(),
    }
}

/// `amount` mana pips of `color`, e.g. `{G}{G}` — repeated symbols, as a cost is
/// written, rather than a count a player has to turn back into pips.
pub(super) fn pips(color: Color, amount: u8) -> String {
    color.pip().repeat(usize::from(amount))
}

/// `amount` colorless mana pips, e.g. `{C}{C}` — the colorless counterpart of
/// [`pips`], written the same repeated-symbol way.
pub(super) fn colorless_pips(amount: u8) -> String {
    "{C}".repeat(usize::from(amount))
}

/// `count` counters of `kind`, e.g. `a +1/+1 counter` or `two -1/-1 counters`.
pub(crate) fn counters(kind: CounterKind, count: u32) -> String {
    let symbol = match kind {
        CounterKind::PlusOnePlusOne => "+1/+1",
        CounterKind::MinusOneMinusOne => "-1/-1",
        CounterKind::Loyalty => "loyalty",
        CounterKind::Charge => "charge",
        CounterKind::Gold => "gold",
        CounterKind::Wish => "wish",
        CounterKind::Corpse => "corpse",
    };
    match count {
        1 => format!("a {symbol} counter"),
        n => format!("{} {symbol} counters", number(n)),
    }
}

/// What an effect may target, as a noun phrase (CR 115.1).
///
/// Returns an owned `String` because one spec carries a **number**: a mana-value filter
/// names the value the card prints, and no fixed table of borrowed phrasings can hold an
/// arbitrary one. Every other arm is still a constant and merely pays for the copy.
pub(super) fn target_noun(spec: TargetSpec) -> String {
    match spec {
        TargetSpec::AnyPlayer => "target player".to_string(),
        TargetSpec::AnyPlayerOrPlaneswalker => "target player or planeswalker".to_string(),
        TargetSpec::AnyOpponent => "target opponent".to_string(),
        TargetSpec::AnyPermanent => "target permanent".to_string(),
        TargetSpec::AnyNonlandPermanent => "target nonland permanent".to_string(),
        TargetSpec::AnyNonlandPermanentAnOpponentControls => {
            "target nonland permanent an opponent controls".to_string()
        }
        // CR 202.3, as the card prints it: the value itself, not a bound around it.
        TargetSpec::AnyPermanentWithManaValue { mana_value } => {
            format!("target permanent with mana value {mana_value}")
        }
        TargetSpec::AnyArtifactCreatureYouControl => {
            "target artifact creature you control".to_string()
        }
        TargetSpec::AnyCreature => "target creature".to_string(),
        TargetSpec::AnyCreatureYouControl => "target creature you control".to_string(),
        TargetSpec::AnyCreatureAnOpponentControls => {
            "target creature an opponent controls".to_string()
        }
        TargetSpec::AnyCreatureWithFlying => "target creature with flying".to_string(),
        TargetSpec::AnyTappedCreature => "target tapped creature".to_string(),
        TargetSpec::AnyArtifact => "target artifact".to_string(),
        TargetSpec::AnyEnchantment => "target enchantment".to_string(),
        TargetSpec::AnyArtifactOrEnchantment => "target artifact or enchantment".to_string(),
        TargetSpec::AnyLand => "target land".to_string(),
        TargetSpec::SpellOnStack => "target spell".to_string(),
        TargetSpec::CreatureSpellOnStack => "target creature spell".to_string(),
        // CR 115.4: "any target" is the phrase itself, not a class of object.
        TargetSpec::AnyTarget => "any target".to_string(),
        TargetSpec::AnyArtifactEnchantmentOrCreatureWithFlying => {
            "target artifact, enchantment, or creature with flying".to_string()
        }
        TargetSpec::CardInGraveyard { .. } => graveyard_noun(spec, true).to_string(),
    }
}

/// A graveyard target as a noun phrase — the one spec whose wording is a product of
/// three fields rather than a fixed string, so it is composed rather than enumerated.
///
/// The mana-value cap is the printed number, so the values M19 needs read as the cards
/// print them. Returns a `&'static str`, which is what forces the small table below:
/// the phrasings are finite because the fields are, and a table keeps both callers
/// borrowing rather than allocating a string per render.
fn graveyard_noun(spec: TargetSpec, targeted: bool) -> &'static str {
    let TargetSpec::CardInGraveyard {
        scope,
        class,
        max_mana_value,
    } = spec
    else {
        return "card in a graveyard";
    };
    let whose = match scope {
        GraveyardScope::Yours => "your graveyard",
        GraveyardScope::Any => "a graveyard",
    };
    let kind = match class {
        GraveyardCardClass::Any => "card",
        GraveyardCardClass::Creature => "creature card",
        GraveyardCardClass::InstantOrSorcery => "instant or sorcery card",
        GraveyardCardClass::Artifact => "artifact card",
        GraveyardCardClass::Land => "land card",
    };
    match (targeted, whose, kind, max_mana_value) {
        (true, "your graveyard", "creature card", Some(2)) => {
            "target creature card with mana value 2 or less in your graveyard"
        }
        (true, "your graveyard", "creature card", None) => "target creature card in your graveyard",
        (true, "your graveyard", "instant or sorcery card", None) => {
            "target instant or sorcery card in your graveyard"
        }
        (true, "your graveyard", "artifact card", None) => "target artifact card in your graveyard",
        (true, "your graveyard", "land card", None) => "target land card in your graveyard",
        (true, "your graveyard", "card", None) => "target card in your graveyard",
        (true, "a graveyard", "creature card", None) => "target creature card in a graveyard",
        (true, "a graveyard", _, _) => "target card in a graveyard",
        (true, _, _, _) => "target card of a limited mana value in your graveyard",
        (false, "a graveyard", _, _) => "card in a graveyard",
        (false, _, _, _) => "card in your graveyard",
    }
}

/// The class of object a target spec names, without the word "target" — what an Aura
/// enchants (CR 303.4a).
///
/// Owned for [`target_noun`]'s reason, and for the same one arm.
pub(super) fn object_noun(spec: TargetSpec) -> String {
    match spec {
        TargetSpec::AnyPlayer => "player".to_string(),
        TargetSpec::AnyPlayerOrPlaneswalker => "player or planeswalker".to_string(),
        TargetSpec::AnyOpponent => "opponent".to_string(),
        TargetSpec::AnyPermanent => "permanent".to_string(),
        TargetSpec::AnyNonlandPermanent => "nonland permanent".to_string(),
        TargetSpec::AnyNonlandPermanentAnOpponentControls => {
            "nonland permanent an opponent controls".to_string()
        }
        TargetSpec::AnyPermanentWithManaValue { mana_value } => {
            format!("permanent with mana value {mana_value}")
        }
        TargetSpec::AnyArtifactCreatureYouControl => "artifact creature you control".to_string(),
        TargetSpec::AnyCreature => "creature".to_string(),
        TargetSpec::AnyCreatureYouControl => "creature you control".to_string(),
        TargetSpec::AnyCreatureAnOpponentControls => "creature an opponent controls".to_string(),
        TargetSpec::AnyCreatureWithFlying => "creature with flying".to_string(),
        TargetSpec::AnyTappedCreature => "tapped creature".to_string(),
        TargetSpec::AnyArtifact => "artifact".to_string(),
        TargetSpec::AnyEnchantment => "enchantment".to_string(),
        TargetSpec::AnyArtifactOrEnchantment => "artifact or enchantment".to_string(),
        TargetSpec::AnyLand => "land".to_string(),
        TargetSpec::SpellOnStack => "spell".to_string(),
        TargetSpec::CreatureSpellOnStack => "creature spell".to_string(),
        TargetSpec::AnyTarget => "any target".to_string(),
        TargetSpec::AnyArtifactEnchantmentOrCreatureWithFlying => {
            "artifact, enchantment, or creature with flying".to_string()
        }
        TargetSpec::CardInGraveyard { .. } => graveyard_noun(spec, false).to_string(),
    }
}

/// The non-targeted subject of an effect (CR 115.1 — no target is chosen), with its
/// verb conjugated to agree with it: `you gain`, but a future third-person subject
/// would read `target player gains`.
///
/// The verb is passed in rather than baked into the subject so agreement is decided in
/// exactly one place; a new [`PlayerRef`] variant cannot pick up the wrong one.
pub(super) fn conjugate(player_ref: PlayerRef, verb: &str) -> String {
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

/// One keyword ability as a card prints it on its own line — the same word the keyword
/// clause is built from, capitalized as the start of a sentence.
///
/// Exposed so a *granted* keyword (CR 613.1f) can be shown in the same words as a printed
/// one: the card the anthem is pumping should say "Trample" exactly as a card that came
/// with it does, and having two spellings of one keyword is how a board starts looking
/// like two games.
#[must_use]
pub(crate) fn keyword_phrase(keyword: Keyword) -> String {
    sentence_case(keyword_word(keyword))
}

/// A keyword ability as the word a player reads (CR 702).
pub(super) fn keyword_word(keyword: Keyword) -> &'static str {
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
        Keyword::Indestructible => "indestructible",
        Keyword::Flash => "flash",
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
pub(super) fn restriction_predicate(restriction: &CombatRestriction) -> String {
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
        CombatRestriction::CantBeBlockedByPowerOrLess(power) => {
            format!("can't be blocked by creatures with power {power} or less")
        }
        CombatRestriction::CantBeBlockedExceptBy(subtype) => {
            format!("can't be blocked except by {subtype}s")
        }
        // The one permission in the vocabulary, and the one predicate here that is not a
        // "can't". "Can" is invariant across singular and plural subjects exactly as
        // "can't" is, so it serves the same four subjects the rest do.
        CombatRestriction::CanBlockAdditional(1) => {
            "can block an additional creature each combat".to_string()
        }
        CombatRestriction::CanBlockAdditional(count) => {
            format!("can block up to {count} additional creatures each combat")
        }
        // The one *requirement* in the vocabulary, and so the one predicate here that is
        // neither a "can't" nor a "can". "Able to do so" rather than "able to block it"
        // for the same reason the rest avoid a subject: a class is a legitimate subject
        // here, and a pronoun would have to agree with it.
        CombatRestriction::MustBeBlockedByAllAble => {
            "must be blocked by every creature able to do so".to_string()
        }
    }
}

/// A token as the noun phrase a card prints it as: `"a 1/1 red Goblin creature
/// token"`, `"two 1/1 white Soldier creature tokens with lifelink"`.
///
/// Assembled in the order a real card states it — count, "tapped", power/toughness,
/// colors, subtypes, card types, the word "token", then any keywords — from the same
/// [`TokenData`] the engine builds the object from. A token with abilities beyond
/// keywords says so without reciting them: the object's own rules text
/// ([`token_rules_text`]) is what a player reads once it is on the battlefield, and
/// repeating it inside the creating card's sentence would be a second place for the
/// same words to drift.
///
/// `tapped` is an adjective inside the phrase rather than a word before it, because
/// that is where a card puts it: *create two **tapped** 1/1 white Cat creature tokens*.
pub(super) fn token_noun(token: &TokenData, count: u32, tapped: bool) -> String {
    let mut parts: Vec<String> = Vec::new();
    if tapped {
        parts.push("tapped".to_string());
    }
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

/// An additional cast cost as its own sentence (CR 601.2b). Exhaustive, so a new
/// [`AdditionalCost`] variant must be given words here.
pub(super) fn additional_cost_text(cost: AdditionalCost) -> String {
    match cost {
        AdditionalCost::Discard { count: 1 } => {
            "As an additional cost to cast this spell, discard a card.".to_string()
        }
        AdditionalCost::Discard { count } => format!(
            "As an additional cost to cast this spell, discard {} cards.",
            number(u32::from(count))
        ),
        AdditionalCost::Sacrifice { card_type } => format!(
            "As an additional cost to cast this spell, sacrifice {} {}.",
            indefinite_article(card_type_word(card_type)),
            card_type_word(card_type)
        ),
    }
}

/// A short list of words as English reads it: `"a"`, `"a and b"`, `"a, b, and c"`.
pub(super) fn list_words(words: &[&str]) -> String {
    match words {
        [] => String::new(),
        [one] => (*one).to_string(),
        [first, second] => format!("{first} and {second}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
    }
}

/// Small counts read as words, as a card writes them; larger ones stay numeric.
pub(super) fn number(count: u32) -> String {
    match count {
        2 => "two".to_string(),
        3 => "three".to_string(),
        4 => "four".to_string(),
        5 => "five".to_string(),
        n => n.to_string(),
    }
}

/// A clause promoted to a sentence: capitalized, with a period.
pub(super) fn finish(clause: &str) -> String {
    format!("{}.", sentence_case(clause))
}

/// The clause with its first character uppercased. ASCII-only by construction — every
/// clause above starts with an English word or a card's name.
pub(crate) fn sentence_case(clause: &str) -> String {
    let mut chars = clause.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}
