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
                PlayerRef::EachOpponent
                | PlayerRef::EachPlayer
                | PlayerRef::TargetPlayer
                | PlayerRef::TargetOpponent
                | PlayerRef::ThatPlayer => "controls",
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
                damage_recipient(source, subject)
            )
        }
        // Two sentences where the card prints two: what goes down, and what they are once
        // they are down.
        Effect::PutHandOntoBattlefieldFaceDown { values } => format!(
            "put any number of cards from your hand onto the battlefield face down. \
             They're {}",
            face_down_noun(values)
        ),
        Effect::Destroy { target, targets } => {
            format!("destroy {}", target_phrase(*target, *targets))
        }
        // "It", because the sentence before it named the host: an Aura's ability is about
        // the creature it is already on, and the card prints a pronoun rather than a
        // second choice.
        Effect::DestroyAttached => "destroy it".to_string(),
        Effect::DestroyAll { affects } => {
            format!("destroy all {}", mass_subject(source, affects))
        }
        // The derived-amount damage verb, in the two shapes English gives it. An
        // announced X reads the way a printed card writes it: the letter itself, in
        // quantity position, not the number it turned out to be — what a *particular*
        // cast announced is a fact about that object on the stack, and the stack entry
        // says so. Every other source has no letter of its own, so the sentence names it
        // after "damage equal to". Spelled out rather than wildcarded, so a new amount
        // has to be put in one shape or the other here.
        Effect::DealDamageByAmount { subject, amount } => match amount {
            DerivedAmount::AnnouncedX => {
                format!(
                    "{source} deals X damage to {}",
                    damage_recipient(source, subject)
                )
            }
            DerivedAmount::LifeGainedThisTurn
            | DerivedAmount::YourLifeTotal
            | DerivedAmount::MilledThisWay { .. }
            | DerivedAmount::GreatestManaValue { .. }
            | DerivedAmount::SacrificedThisWay
            | DerivedAmount::SacrificedCreaturePower
            | DerivedAmount::HalfRoundedUp { .. } => format!(
                "{source} deals damage equal to {} to {}",
                amount_noun(amount, PlayerRef::Controller),
                damage_recipient(source, subject),
            ),
        },
        // The life rider is a second sentence, where the card prints it, and "its" points
        // back at the noun this one just named — which is the whole reason the two are
        // one effect rather than two.
        Effect::Exile { target, gain_life } => {
            let exile = format!("exile {}", target_noun(*target));
            match gain_life {
                None => exile,
                Some(PermanentAmount::Power) => {
                    format!("{exile}. You gain life equal to its power")
                }
            }
        }
        // The one clause with two target nouns in it (CR 701.12). The mutual form is the
        // printed verb *fights*, which says the power reading on its own; the one-sided
        // form has to spell it out, and "its" refers back to the first noun the sentence
        // named — which is why the two targets are one clause rather than two.
        Effect::Fight {
            dealer,
            dealt_to,
            mutual,
        } => {
            if *mutual {
                format!("{} fights {}", target_noun(*dealer), target_noun(*dealt_to))
            } else {
                format!(
                    "{} deals damage equal to its power to {}",
                    target_noun(*dealer),
                    target_noun(*dealt_to)
                )
            }
        }
        // The one clause whose *number* of target nouns comes from the table, so it names
        // none of them individually: "for each player" is the whole of who is being asked,
        // and repeating the noun once per seat would make the card's text depend on how
        // many people are playing.
        Effect::SacrificeChosenPerPlayer { reveal_top } => {
            let sacrifice = "for each player, choose target permanent that player controls. \
                 Those players sacrifice those permanents"
                .to_string();
            if *reveal_top {
                format!(
                    "{sacrifice}. Each player who sacrificed a permanent this way reveals \
                     the top card of their library, then puts it onto the battlefield if \
                     it's a permanent card"
                )
            } else {
                sacrifice
            }
        }
        // Two sentences on the printed card, and two here: the removal, then the
        // prohibition that outlives it. `for as long as` names the source's stay on the
        // battlefield, which is what the duration measures.
        Effect::RemoveAllCounters {
            target,
            then_forbid,
        } => {
            let removal = format!("remove all counters from {}", target_noun(*target));
            if *then_forbid {
                format!(
                    "{removal}. It can't have counters put on it for as long as \
                     {source} remains on the battlefield"
                )
            } else {
                removal
            }
        }
        Effect::PlayerLosesAllCounters {
            player_ref,
            then_forbid,
        } => {
            let removal = format!("{} all counters", conjugate(*player_ref, "lose"));
            if *then_forbid {
                format!(
                    "{removal}. That player can't get counters for as long as \
                     {source} remains on the battlefield"
                )
            } else {
                removal
            }
        }
        Effect::GainLife { player_ref, amount } => {
            format!("{} {amount} life", conjugate(*player_ref, "gain"))
        }
        Effect::LoseLife { player_ref, amount } => {
            format!("{} {amount} life", conjugate(*player_ref, "lose"))
        }
        // CR 720.1, in the card's own words. "After this one" is not a fact about the
        // effect a player chooses — every extra turn is taken after the current one — so
        // it is printed rather than authored.
        Effect::TakeExtraTurn { player_ref } => {
            // "You" is dropped where the subject is the controller, exactly as it is for
            // every other clause that reads as an instruction: the card says "take an
            // extra turn after this one", never "you take" one.
            let clause = format!(
                "{} an extra turn after this one",
                conjugate(*player_ref, "take")
            );
            match player_ref {
                PlayerRef::Controller => without_you(&clause).to_string(),
                _ => clause,
            }
        }
        // The shortest sentence a card can end on, and it takes the same conjugation
        // every other player-subject clause does: "you win the game".
        Effect::WinTheGame { player_ref } => {
            format!("{} the game", conjugate(*player_ref, "win"))
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
            count_amount,
            keywords,
        } => {
            // "X +1/+1 counters, where X is your life total" — the letter in quantity
            // position and the source named after it, which is how a card prints an
            // amount it does not know when it is printed.
            let what = match count_amount {
                None => counters(*counter, *count),
                Some(_) => format!("X {} counters", crate::rules_text::counter_symbol(*counter)),
            };
            let clause = format!("put {what} on {}", target_phrase(*target, *targets));
            let clause = match count_amount {
                None => clause,
                Some(amount) => format!(
                    "{clause}, where X is {}",
                    amount_noun(amount, PlayerRef::Controller)
                ),
            };
            // A keyword the same sentence grants goes to the same creature, and the card
            // says so with a pronoun rather than by naming the class twice — "and **that
            // creature** gains flying until end of turn". The pronoun is what the second
            // clause is for; repeating the noun would read as a second target, which is
            // precisely the thing the one-effect shape exists to prevent.
            if keywords.is_empty() {
                return clause;
            }
            let words: Vec<&str> = keywords.iter().map(|&kw| keyword_word(kw)).collect();
            format!(
                "{clause}, and {} gains {} until end of turn",
                counter_pronoun(*targets),
                list_words(&words)
            )
        }
        // One effect, one target, one sentence: the keywords a pump also grants are
        // granted to the same creature, so they read as a second verb on the same
        // subject rather than as a sentence with a subject of its own.
        Effect::Pump {
            target,
            power,
            toughness,
            keywords,
            abilities,
            restrictions,
        } => {
            // One subject, one duration, and as many clauses as the card prints between
            // them: the numbers, then any keywords gained, then any written-out ability,
            // then any combat restriction imposed. Each is a predicate the target noun
            // and "until end of turn" wrap, which is why they join as a list rather than
            // as separate sentences.
            let mut clauses = vec![format!("gets {power:+}/{toughness:+}")];
            if !keywords.is_empty() {
                let words: Vec<&str> = keywords.iter().map(|&kw| keyword_word(kw)).collect();
                clauses.push(format!("gains {}", list_words(&words)));
            }
            // Quoted, because the words inside a granted ability belong to the *host*:
            // "this creature" in them names the creature that gained it, not the spell
            // that handed it over.
            clauses.extend(abilities.iter().map(|ability| {
                format!(
                    "gains \"{}\"",
                    ability_text(granted_subject(*target), ability)
                )
            }));
            clauses.extend(restrictions.iter().map(restriction_predicate));
            let verbs = list_words(&clauses.iter().map(String::as_str).collect::<Vec<_>>());
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
            mass_subject(source, affects)
        ),
        Effect::GrantKeywordAll { affects, keyword } => format!(
            "{} gain {} until end of turn",
            mass_subject(source, affects),
            keyword_word(*keyword)
        ),
        // A restriction is already a predicate ("can't be blocked"), so it needs no
        // verb of its own — only the subject and the duration around it. The subject is
        // where a variable-arity restriction differs from a variable-arity counter: the
        // targets are what the sentence is *about*, so they read as "up to two target
        // creatures" rather than as the "each of …" an object position takes.
        Effect::Restrict {
            target,
            targets,
            restriction,
        } => format!(
            "{} {} this turn",
            target_subject(*target, *targets),
            restriction_predicate(restriction)
        ),
        Effect::RestrictAll {
            affects,
            restriction,
        } => format!(
            "{} {} this turn",
            mass_subject(source, affects),
            restriction_predicate(restriction)
        ),
        // A self-referential effect names the source by name, so the sentence reads
        // the way the card does rather than as an anonymous "this".
        // The self row of what `Effect::Pump` says for a target: one subject, one
        // duration, and the keywords the same sentence grants read as a second verb on
        // it rather than as a sentence of their own.
        Effect::PumpSelf {
            power,
            toughness,
            keywords,
        } => {
            let mut clauses = vec![format!("gets {power:+}/{toughness:+}")];
            if !keywords.is_empty() {
                let words: Vec<&str> = keywords.iter().map(|&kw| keyword_word(kw)).collect();
                clauses.push(format!("gains {}", list_words(&words)));
            }
            let verbs = list_words(&clauses.iter().map(String::as_str).collect::<Vec<_>>());
            format!("{source} {verbs} until end of turn")
        }
        Effect::RestrictSelf { restriction } => {
            format!("{source} {} this turn", restriction_predicate(restriction))
        }
        // One printed sentence, so one clause: what is lost, then what is gained, in
        // the order the effect applies them. A clause that only loses or only gains
        // reads as the single half it is.
        Effect::AlterAbilitiesSelf {
            lose_all,
            lose,
            gain,
        } => {
            let mut parts: Vec<String> = Vec::new();
            if *lose_all {
                parts.push("loses all abilities".to_string());
            }
            if !lose.is_empty() {
                let words: Vec<&str> = lose.iter().map(|&kw| keyword_word(kw)).collect();
                parts.push(format!("loses {}", list_words(&words)));
            }
            if !gain.is_empty() {
                let words: Vec<&str> = gain.iter().map(|&kw| keyword_word(kw)).collect();
                parts.push(format!("gains {}", list_words(&words)));
            }
            // Nothing lost and nothing gained is not a sentence; the catalog validator
            // keeps it unauthorable, and saying so beats printing a bare name.
            if parts.is_empty() {
                format!("{source} is unchanged until end of turn")
            } else {
                format!("{source} {} until end of turn", parts.join(" and "))
            }
        }
        Effect::PutCountersOnSelf {
            counter,
            count,
            that_many,
        } => {
            // "that many" is the card's own words for a number the trigger event fixes;
            // the printed sentence has no value to show, and by the time one is written
            // in, this text has already been read.
            if *that_many {
                format!(
                    "put that many {} counters on {source}",
                    counter_symbol(*counter)
                )
            } else {
                format!("put {} on {source}", counters(*counter, *count))
            }
        }
        // The self-referential return: the source names itself, and the graveyard it comes
        // out of is the one the ability functions in (CR 113.6) — so both halves of the
        // sentence are facts about the source rather than anything an author chose.
        Effect::ReturnSelfFromGraveyard { destination } => format!(
            "return {source} from your graveyard to {}",
            match destination {
                FoundDestination::Hand => "your hand",
                FoundDestination::Battlefield => "the battlefield",
                FoundDestination::BattlefieldTapped => "the battlefield tapped",
            }
        ),
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
            take_min,
            filter,
            destination,
            bottom_order,
        } => format!(
            "look at the top {} cards of your library, {} {} from among them {}, \
             then put the rest on the bottom of your library in {}",
            number(u32::from(*count)),
            // "You may put up to one" and "put one" are the two takes, and the words have
            // to follow the floor: a card that reads "you may" where the rules make the
            // player take one is a card the player will misplay.
            if *take_min == 0 { "you may put" } else { "put" },
            take_phrase(u32::from(*take_min), u32::from(*take), filter),
            destination_phrase(*destination),
            // The card's own two wordings, and they are different rules: one is the
            // game's roll and the other is the looker's arrangement.
            match bottom_order {
                BottomOrder::Random => "a random order",
                BottomOrder::Chosen => "any order",
            },
        ),
        // The open form first, because it names no number at all: "any number of Dragon
        // creature cards" is what the card prints where the others print a ceiling.
        Effect::SearchLibrary {
            filter,
            destination,
            any_number: true,
            ..
        } => format!(
            "search your library for any number of {}, put them {}, then shuffle",
            filter_noun(filter, true),
            destination_phrase(*destination),
        ),
        Effect::SearchLibrary {
            take,
            take_amount,
            filter,
            destination,
            ..
        } => match take_amount {
            // A card whose search size is a derived number says "up to that many" and
            // never a figure, so the phrase names the amount rather than a count.
            Some(amount) => format!(
                "search your library for up to {} {}, put them {}, then shuffle",
                amount_noun(amount, PlayerRef::Controller),
                filter_noun(filter, true),
                destination_phrase(*destination),
            ),
            None => format!(
                "search your library for {}, put {} {}, then shuffle",
                up_to(u32::from(*take), filter),
                if *take == 1 { "it" } else { "them" },
                destination_phrase(*destination),
            ),
        },
        // An optional effect reads as the card prints it. The costed form is two
        // sentences even inside a larger clause — "you may pay {1}. If you do, draw a
        // card", "you may sacrifice another creature. If you do, …" — because that is how
        // the condition is written on every card that has one, and running it together
        // with "and" would read as though both halves happened.
        Effect::May {
            chooser,
            cost,
            effects,
            otherwise,
        } => {
            let what = clauses(source, effects);
            // With a consequence attached, the card prints it the other way round —
            // "sacrifice it unless you pay {1}" — because the consequence is what
            // happens and the payment is what avoids it. Cards that print the pair
            // this way never also print a "if you do" half.
            if !otherwise.is_empty() {
                let cost = cost
                    .as_ref()
                    .map_or_else(|| "does".to_string(), optional_cost_phrase);
                // Who is paying decides the subject and the agreement: "unless you pay
                // {1}", "unless that player sacrifices a creature". The verb is the first
                // word of the cost phrase, and every one of them is regular.
                let clause = match chooser {
                    PlayerRef::Controller => format!("you {cost}"),
                    other => match cost.split_once(' ') {
                        Some((verb, rest)) => format!("{} {rest}", conjugate(*other, verb)),
                        None => conjugate(*other, &cost),
                    },
                };
                return format!("{} unless {clause}", clauses(source, otherwise));
            }
            match cost {
                Some(cost) => format!("you may {}. If you do, {what}", optional_cost_phrase(cost)),
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
        Effect::ReturnCardToBattlefield {
            target,
            targets,
            tapped,
            types,
            subtypes,
            colors,
            counters,
            exile_on_leaving,
        } => {
            // The counters ride the same sentence the card prints them in: "…onto the
            // battlefield with a corpse counter on it".
            let with_counters = match counters.as_slice() {
                [] => String::new(),
                entries => format!(
                    " with {}",
                    entries
                        .iter()
                        // "a corpse counter", not "1 corpse counter" — a card writes the
                        // article for one and the number for more.
                        .map(|(kind, count)| format!(
                            "{} {} counter{}",
                            if *count == 1 {
                                "a".to_string()
                            } else {
                                number(*count)
                            },
                            counter_symbol(*kind),
                            if *count == 1 { "" } else { "s" }
                        ))
                        .collect::<Vec<_>>()
                        .join(" and ")
                ),
            };
            let on_it = if counters.is_empty() { "" } else { " on it" };
            let return_it = format!(
                "put {} onto the battlefield under your control{}{with_counters}{on_it}",
                target_phrase(*target, *targets),
                if *tapped { " tapped" } else { "" }
            );
            // The replacement is its own sentence, where the card prints it.
            let return_it = if *exile_on_leaving {
                format!(
                    "{return_it}. If that creature would leave the battlefield, exile it \
                     instead of putting it anywhere else"
                )
            } else {
                return_it
            };
            // The card prints the continuous half as its own sentence about "that
            // creature", because by then the permanent exists and can be spoken of.
            let mut what: Vec<String> = colors.iter().map(|c| c.word().to_string()).collect();
            what.extend(subtypes.iter().cloned());
            what.extend(types.iter().map(|kind| card_type_word(*kind).to_string()));
            if what.is_empty() {
                return return_it;
            }
            let named = what.join(" ");
            format!(
                "{return_it}. That creature is {} {named} in addition to its other colors \
                 and types",
                super::indefinite_article(&named)
            )
        }
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
            damage_recipient(source, subject),
            count_noun(count_of)
        ),
        // The three derived-number verbs of a symmetric sweeper, each one sentence in the
        // order the card prints them. The amount carries its own "rounded up", and it is
        // rendered against the *named* player, so "each player" gets "their" and a
        // controller-only form would get "your".
        Effect::LoseLifeByAmount { player_ref, amount } => format!(
            "{} {}",
            conjugate(*player_ref, "lose"),
            amount_noun(amount, *player_ref)
        ),
        Effect::DiscardByAmount { player_ref, amount } => format!(
            "{} {}",
            conjugate(*player_ref, "discard"),
            amount_noun(amount, *player_ref)
        ),
        // With a counted amount the class (`card_type`) is deliberately not restated: the
        // amount's own phrase already names it — "half the creatures they control" — and
        // printing the class twice would be two ways to say one printed clause. The **open**
        // form has no such phrase, because "any number" is not a number read of anything,
        // so that one names the class itself and prints as the imperative a card writes
        // — `Sacrifice any number of lands` — exactly as the draw beside it does.
        Effect::Sacrifice {
            player_ref,
            amount,
            card_type,
        } => match amount {
            Some(amount) => format!(
                "{} {}",
                conjugate(*player_ref, "sacrifice"),
                amount_noun(amount, *player_ref)
            ),
            None => {
                let clause = format!(
                    "{} any number of {}",
                    conjugate(*player_ref, "sacrifice"),
                    plural_sacrifice_noun(*card_type, None)
                );
                without_you(&clause).to_string()
            }
        },
        Effect::ExileGraveyard { player_ref } => {
            format!("exile {}'s graveyard", possessive_subject(*player_ref))
        }
        Effect::ExileLibraryExceptBottom { target } => format!(
            "exile all but the bottom card of {}'s library",
            possessive_subject(*target)
        ),
        // CR 701.28a. The permanent turns over and is the same object, so the sentence is
        // about the source and nothing else — there is no zone to name and nothing to
        // say about what survives, because everything does.
        Effect::TransformSelf => format!("transform {source}"),
        // Two zone changes rather than a turn-over, and the sentence says so: a player
        // reading it needs to know that what comes back is a new object, which is the
        // difference between keeping a +1/+1 counter and losing it.
        Effect::ExileSelfAndReturnTransformed => format!(
            "exile {source}, then return it to the battlefield transformed under its \
             owner's control"
        ),
        Effect::PutOnTopOfLibrary { target } => {
            format!("put {} on top of its owner's library", target_noun(*target))
        }
        // The self-referential sibling of the line above, and the reason it names its
        // source rather than a target: nothing was chosen, so the sentence has to say
        // which permanent goes back — and "its owner's" is the card's own owner, whoever
        // currently controls it.
        Effect::ShuffleSelfIntoLibrary => {
            format!("shuffle {source} into its owner's library")
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
        // The same X clause with the source spelled out by the amount itself, which
        // already reads as the noun phrase a card puts after "where X is".
        Effect::PumpByAmount {
            target,
            power_per,
            toughness_per,
            amount,
        } => format!(
            "{} gets {}X/{}X until end of turn, where X is {}",
            target_noun(*target),
            sign(*power_per),
            sign(*toughness_per),
            amount_noun(amount, PlayerRef::Controller),
        ),
        // Named subject, unlike the fixed [`Effect::DrawCard`] beside it: a derived draw
        // is printed as the *second* clause of a sentence whose first one belongs to
        // somebody else ("target opponent mills three cards, then **you** draw …"), and a
        // bare "draw" there reads as an instruction to the opponent.
        Effect::DrawCardsByAmount { amount } => {
            format!(
                "you draw cards equal to {}",
                amount_noun(amount, PlayerRef::Controller)
            )
        }
        // The colors are the player's, so the sentence says how many mana and leaves
        // the colors to them — exactly what the card says. Which of the two phrasings it
        // is is the difference between one decision and several.
        Effect::AddManaAnyColor {
            amount,
            same_color,
            restriction,
        } => {
            let mana = if *amount == 1 {
                "add one mana of any color".to_string()
            } else if *same_color {
                format!("add {} mana of any one color", number(u32::from(*amount)))
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
        // Two sentences because the card prints two, and in its order: what moves, then
        // what you may do with it. "That card" and "those cards" is the permission naming
        // exactly what this effect exiled (CR 116.2a — *play*, since a land among them is
        // played rather than cast).
        // Three sentences where the card prints three, in its order: what is revealed,
        // what you may do with it, and what happens if you don't.
        // Two sentences, in the card's order: what the digging does, and where it stops.
        Effect::ExileFromLibraryUntil { player_ref, class } => {
            let noun = super::words::graveyard_class_noun(*class)
                .map_or_else(|| "card".to_string(), |class| format!("{class} card"));
            format!(
                "{} {} cards from the top of {} library until {} {noun}",
                subject_pronoun(*player_ref),
                conjugate(*player_ref, "exile"),
                possessive_pronoun(*player_ref),
                conjugate(*player_ref, "exile"),
            )
        }
        // The offer, and what becomes of everything it passed over.
        Effect::MayCastExiledThisWay { free, .. } => {
            let price = if *free {
                " without paying its mana cost"
            } else {
                ""
            };
            format!(
                "you may cast that card{price}. Then put the exiled cards that weren't cast \
                 this way on the bottom of that library in a random order"
            )
        }
        Effect::RevealTopAndMayPlay { free } => {
            let price = if *free {
                " without paying its mana cost"
            } else {
                ""
            };
            format!(
                "reveal the top card of your library. You may play that card{price}. \
                 If you don't, exile it"
            )
        }
        Effect::ExileTopForPlay { count, cast_only } => {
            // Two printed sentences, and the difference is whether a land among them may
            // be played: *you may play that card* against *you may cast spells from among
            // them* (CR 116.2a).
            let permission = if *cast_only {
                "you may cast spells from among them".to_string()
            } else if *count == 1 {
                "you may play that card".to_string()
            } else {
                "you may play those cards".to_string()
            };
            if *count == 1 {
                format!("exile the top card of your library. Until end of turn, {permission}")
            } else {
                format!(
                    "exile the top {} cards of your library. Until end of turn, {permission}",
                    number(u32::from(*count)),
                )
            }
        }
        Effect::IgnoreHexproof { player_ref } => format!(
            "spells and abilities {} control may target as though hexproof were not there \
             this turn",
            subject_pronoun(*player_ref),
        ),
        // CR 614.1b, in the order a card prints it: the event, then the duration the
        // one-shot lasts, then any qualifier on the event, then what happens instead.
        Effect::CreateReplacement { replacement } => match replacement {
            ReplacementEffect::ExileEntering { entering } => format!(
                "the next time {} would enter the battlefield this turn{}, {}",
                entering_noun(entering),
                if entering.not_cast {
                    " without being cast"
                } else {
                    ""
                },
                replacement_phrase(replacement),
            ),
        },
        // CR 615.1, in the order a card prints it: the verb, the class of damage, and
        // the duration — `prevent all combat damage that would be dealt this turn`.
        Effect::PreventDamage { damage } => format!(
            "prevent all {}damage that would be dealt this turn",
            if damage.combat_only { "combat " } else { "" },
        ),
        // CR 603.7, in the order a card prints it: the event it waits for, then what
        // happens when it comes. The `next` and the `this turn` are both facts about the
        // ability rather than authored words, so the sentence states them.
        Effect::CreateDelayedTrigger { trigger } => {
            let DelayedCondition::NextSpellCast(spell) = trigger.event;
            format!(
                "when you next cast {} this turn, {}",
                observed_spell_noun(spell),
                clauses("that spell", &trigger.effects),
            )
        }
        // CR 603.11 with a payment as its condition, printed as the two sentences every
        // card that has it prints: the offer, then what buying it does.
        Effect::MayPayForTrigger { cost, effects } => format!(
            "you may {}. When you do, {}",
            optional_cost_phrase(cost),
            clauses(source, effects)
        ),
        // CR 603.11: the `when …, …` a resolution says about what it just did. The
        // subject of the effects it creates is the permanent that arrived, so they are
        // composed against "it" — the pronoun the printed card uses, and the only name
        // available for a card nobody has chosen yet.
        Effect::CreateReflexiveTrigger { trigger } => {
            let sage_engine::ReflexiveCondition::CreaturePutOntoBattlefieldThisWay = trigger.event;
            format!(
                "when a creature is put onto the battlefield this way, {}",
                clauses("it", &trigger.effects),
            )
        }
        // CR 613 layers 4 and 7b, in the order the card prints them: what it becomes,
        // then how big, then for how long.
        Effect::Animate {
            target,
            types,
            subtypes,
            colors,
            power,
            toughness,
            until_end_of_turn,
            until_your_next_turn,
        } => {
            // Colour first, then subtype, then type — the order a card prints them in:
            // "a black Zombie", "an artifact creature".
            let mut what: Vec<String> = colors.iter().map(|c| c.word().to_string()).collect();
            what.extend(subtypes.iter().cloned());
            what.extend(types.iter().map(|kind| card_type_word(*kind).to_string()));
            let becomes = if what.is_empty() {
                String::new()
            } else {
                format!(
                    " becomes {} {}",
                    super::indefinite_article(&what[0]),
                    what.join(" ")
                )
            };
            let size = match (power, toughness) {
                (Some(power), Some(toughness)) => {
                    format!(" with base power and toughness {power}/{toughness}")
                }
                _ => String::new(),
            };
            let how_long = if *until_your_next_turn {
                " until your next turn".to_string()
            } else if *until_end_of_turn {
                " until end of turn".to_string()
            } else {
                format!(" for as long as {source} remains on the battlefield")
            };
            format!("{}{becomes}{size}{how_long}", target_noun(*target))
        }
        // The self-referential animation: the source is the subject, so the sentence says
        // what it becomes and nothing about what it is now.
        Effect::AnimateSelf {
            types,
            subtypes,
            colors,
            power,
            toughness,
            ..
        } => {
            let mut what: Vec<String> = colors.iter().map(|c| c.word().to_string()).collect();
            what.extend(subtypes.iter().cloned());
            what.extend(types.iter().map(|kind| card_type_word(*kind).to_string()));
            let named = what.join(" ");
            let size = match (power, toughness) {
                (Some(power), Some(toughness)) => {
                    format!(" with base power and toughness {power}/{toughness}")
                }
                _ => String::new(),
            };
            format!(
                "{source} becomes {} {named}{size} until end of turn",
                super::indefinite_article(&named)
            )
        }
        // CR 701.10, as the card prints it: one sentence about the pair.
        Effect::ExchangeControl { first, .. } => {
            // The card says "two **target** creatures": the word belongs in the sentence,
            // and the plural is of the class rather than of the phrase.
            format!(
                "exchange control of two target {}",
                plural(&object_noun(*first))
            )
        }
        // CR 610.3: the return is a sentence about *this* card, so the source names itself.
        Effect::ExileUntilSourceLeaves { target } => format!(
            "exile {} until {source} leaves the battlefield",
            target_noun(*target)
        ),
        // CR 701.17: the source names itself, and a card that has just told you what it
        // is says "it" rather than repeating its own name.
        Effect::SacrificeSelf => "sacrifice it".to_string(),
        // CR 303.4: the Aura chose its host at cast, so the sentence says which creature
        // without naming a target.
        Effect::TapAttached => "tap enchanted creature".to_string(),
        // CR 609.7: the dealer is the ability's own source, so the sentence says "it" and
        // the amount is not a number the card prints.
        Effect::SelfDealsDamage { target } => {
            format!(
                "{source} deals damage equal to its power to {}",
                target_noun(*target)
            )
        }
        // CR 707.10. The copy's targets are the second sentence a card prints, not a
        // clause of the first, because they are a separate permission.
        Effect::CopySpell { new_targets, .. } => {
            let mut clause = "copy that spell".to_string();
            if *new_targets {
                clause.push_str(". You may choose new targets for the copy");
            }
            clause
        }
    }
}

/// What a replacement effect does **instead** of the event it watches (CR 614.1a).
///
/// The half of the clause that is about the substitution rather than about the event,
/// split out because the CR 616.1 ordering prompt asks a player to choose *between
/// substitutions* — they already know what is entering. One formatter either way, so the
/// question and the card's rules text can never describe the same effect two ways.
#[must_use]
pub(crate) fn replacement_phrase(replacement: &ReplacementEffect) -> String {
    match replacement {
        ReplacementEffect::ExileEntering { .. } => "exile it instead".to_string(),
    }
}

/// The class of entering permanent a replacement's filter names, as a noun phrase with
/// its article: `a nontoken creature`, `a permanent`.
fn entering_noun(filter: &EnteringFilter) -> String {
    let noun = match filter.card_type {
        Some(card_type) => card_type_word(card_type),
        None => "permanent",
    };
    if filter.nontoken {
        format!("a nontoken {noun}")
    } else {
        format!("a {noun}")
    }
}

/// A target group as the phrase a card writes it in: `target creature` for the ordinary
/// single-target effect, `each of up to two target creatures` for the one that may name
/// fewer than it allows.
/// The pronoun a counter-and-grant sentence refers back to its own target with — `that
/// creature` for one, `those creatures` for a group.
///
/// The whole point of the pronoun is that it is **not** the noun: a card that says "put a
/// +1/+1 counter on another target creature you control, and *that creature* gains
/// flying" names one creature and then points at it. Writing the class out twice would
/// read as a second target, which is the misauthoring
/// [`Violation::TwoTargetsOfOneClass`](sage_engine::Violation) exists to refuse.
fn counter_pronoun(count: TargetCount) -> &'static str {
    match count {
        TargetCount::Exactly(1) | TargetCount::UpTo(1) => "that creature",
        _ => "those creatures",
    }
}

fn target_phrase(spec: TargetSpec, count: TargetCount) -> String {
    match count {
        TargetCount::Exactly(1) => target_noun(spec),
        TargetCount::Exactly(n) => format!(
            "each of {} {}",
            number(u32::from(n)),
            plural_target_noun(spec)
        ),
        // "up to one target creature", singular and with no "each of" in front of it:
        // there is no *each* about a group that names at most one, and the noun agrees
        // with the number the way a printed card writes it.
        TargetCount::UpTo(1) => format!("up to one {}", target_noun(spec)),
        TargetCount::UpTo(n) => format!(
            "each of up to {} {}",
            number(u32::from(n)),
            plural_target_noun(spec)
        ),
    }
}

/// The same target group in **subject** position: `target creature`, `up to two target
/// creatures`.
///
/// A separate function from [`target_phrase`] for the reason [`count_noun`] is separate
/// from [`count_subject`] — English puts the group after "each of" when the effect acts
/// *on* it and bare at the head of the sentence when the group is what the sentence is
/// about. One function per position keeps both exhaustive over the count.
fn target_subject(spec: TargetSpec, count: TargetCount) -> String {
    match count {
        TargetCount::Exactly(1) => target_noun(spec),
        TargetCount::Exactly(n) => {
            format!("{} {}", number(u32::from(n)), plural_target_noun(spec))
        }
        TargetCount::UpTo(1) => format!("up to one {}", target_noun(spec)),
        TargetCount::UpTo(n) => {
            format!(
                "up to {} {}",
                number(u32::from(n)),
                plural_target_noun(spec)
            )
        }
    }
}

/// A target spec pluralized — "target creatures" — for a group that names more than one.
///
/// English pluralizes the **head** noun, which is not always the last word: a card in a
/// graveyard is "target creature card**s** in a graveyard", never "…in a graveyard**s**".
/// The head runs up to the first preposition, and every phrase this vocabulary produces
/// has at most one.
fn plural_target_noun(spec: TargetSpec) -> String {
    let noun = target_noun(spec);
    match noun.find(" in ") {
        Some(at) => format!("{}s{}", &noun[..at], &noun[at..]),
        None => format!("{noun}s"),
    }
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

/// A [`DerivedAmount`] as the noun phrase a card puts after "where X is" or "equal to".
///
/// One phrasing per source, in the words the printed card uses, so the two positions the
/// amount can appear in read as one sentence apiece — "gets -X/-X …, where X is **the
/// amount of life you gained this turn**", "draw cards equal to **the greatest mana value
/// among artifacts you control**". Exhaustive, so a new source has to say how it reads.
fn amount_noun(amount: &DerivedAmount, subject: PlayerRef) -> String {
    match amount {
        DerivedAmount::LifeGainedThisTurn => "the amount of life you gained this turn".to_string(),
        DerivedAmount::YourLifeTotal => "your life total".to_string(),
        // "milled this way" is the wording the intervening-if clause already uses for the
        // same window; one phrase, so the yes-or-no and the count cannot describe the
        // same mill two ways.
        DerivedAmount::MilledThisWay { filter } => format!(
            "the number of {} milled this way",
            filter_noun(filter, true)
        ),
        DerivedAmount::GreatestManaValue { among } => {
            format!("the greatest mana value among {}", count_subject(among))
        }
        // An announced X has no "where X is" clause at all: the card writes the letter and
        // the player supplies the value as they cast it (CR 601.2b). Reaching this
        // position would mean a card said "where X is X", so it says the plain letter.
        DerivedAmount::AnnouncedX => "X".to_string(),
        // The two amounts a sacrifice leaves behind. A card writes the first as the "that
        // many" of a sentence whose previous clause did the sacrificing, and the second as
        // a possessive naming the creature the cost ate.
        DerivedAmount::SacrificedThisWay => "that many".to_string(),
        DerivedAmount::SacrificedCreaturePower => "the sacrificed creature's power".to_string(),
        // The one source whose phrase is about the *named player* rather than about the
        // controller, so it is the one that takes the subject: "each player loses half
        // **their** life", "you lose half **your** life". The rounding trails the phrase
        // exactly where a card prints it.
        DerivedAmount::HalfRoundedUp { of } => {
            let total = match of {
                HalvedTotal::LifeTotal => {
                    format!("half {} life", possessive_pronoun(subject))
                }
                HalvedTotal::HandSize => {
                    format!("half the cards in {} hand", possessive_pronoun(subject))
                }
                HalvedTotal::CreaturesControlled => {
                    format!("half the creatures {} control", relative_subject(subject))
                }
            };
            format!("{total}, rounded up")
        }
    }
}

/// The subject of a relative clause hanging off a noun — "the creatures **they**
/// control". The third of the player-reference renderings beside [`subject_pronoun`] and
/// [`possessive_pronoun`], and separate for the same reason those two are: English wants
/// a different word in each position, and one function per position keeps all three
/// exhaustive.
fn relative_subject(player_ref: PlayerRef) -> &'static str {
    match player_ref {
        PlayerRef::Controller => "you",
        PlayerRef::EachOpponent
        | PlayerRef::EachPlayer
        | PlayerRef::TargetPlayer
        | PlayerRef::TargetOpponent
        | PlayerRef::ThatPlayer => "they",
    }
}

/// The cards a [`GraveyardCount`] counts, as the noun phrase after "the number of" —
/// "instant or sorcery cards in your graveyard".
pub(super) fn graveyard_count_noun(count: &GraveyardCount) -> String {
    let zone = match count.scope {
        GraveyardScope::Yours => "in your graveyard",
        GraveyardScope::Any => "in all graveyards",
    };
    format!("{} {zone}", filter_noun(&count.filter, true))
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
    // Printed ahead of the colour, the way a card prints it: "nontoken creature", and
    // "nontoken white creature" if a card ever names both (CR 111).
    if count.nontoken {
        noun.push_str("nontoken ");
    }
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
pub(super) fn condition_clause(condition: &Condition) -> String {
    match condition {
        // The same composer the static `as long as …` clause uses. It was worth sharing
        // rather than duplicating: the local phrasing read "you control 1 or more
        // creatures you control", because the count's own scope already says "you
        // control" and this clause said it again.
        Condition::ControlsAtLeast { permanents, count } => {
            format!("you control {}", counted_permanents(permanents, *count))
        }
        // The negative form says "no", where the counted one says a number — and it is
        // the plural class rather than one of them, which is what "no permanents" means.
        Condition::ControlsNone { permanents } => {
            format!("you control no {}", counted_permanents_plural(permanents))
        }
        // The one condition about the resolving object rather than about the board.
        Condition::CastFromHand => "this spell was cast from your hand".to_string(),
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
        // The one condition whose subject is the source rather than its controller, so
        // the clause says "this creature" where the others say "you".
        Condition::AttackedOrBlockedThisTurn => {
            "this creature attacked or blocked this turn".to_string()
        }
    }
}

/// The subject pronoun a player reference reads as at the start of a clause.
fn subject_pronoun(player_ref: PlayerRef) -> &'static str {
    match player_ref {
        PlayerRef::Controller => "you",
        PlayerRef::EachOpponent => "each opponent",
        PlayerRef::EachPlayer => "each player",
        PlayerRef::TargetPlayer => "target player",
        PlayerRef::TargetOpponent => "target opponent",
        // The card's own words for a player a sentence before this one chose: "its
        // controller", where a destroy came first, reads as "that player" everywhere the
        // engine has to name them in the abstract.
        PlayerRef::ThatPlayer => "that player",
    }
}

/// The possessive a player reference reads as — "your graveyard", "their graveyard".
fn possessive_pronoun(player_ref: PlayerRef) -> &'static str {
    match player_ref {
        PlayerRef::Controller => "your",
        PlayerRef::EachOpponent
        | PlayerRef::EachPlayer
        | PlayerRef::TargetPlayer
        | PlayerRef::TargetOpponent
        | PlayerRef::ThatPlayer => "their",
    }
}

/// The question an optional effect puts to its controller, as the words on the button
/// they answer it with — "Draw a card?", "Pay {1}? If you do, draw a card", "Sacrifice
/// another creature? If you do, this gets +2/+2 until end of turn".
///
/// Composed from the effects themselves rather than authored per card, exactly as the
/// card's own rules text is: one vocabulary, so the prompt and the printed sentence can
/// never describe the same offer two different ways. The source is written as "this"
/// because the question is asked mid-resolution, when the object that asked it may
/// already have left the battlefield.
///
/// The costed form asks the *cost* and states what it buys, in the card's own two
/// sentences, rather than folding both into one "pay X to Y?". Only the first shape
/// survives a clause whose subject is not the player: "Sacrifice another creature to this
/// gets +2/+2" is not English, and a card whose optional effect targets would read the
/// same way.
#[must_use]
pub(crate) fn optional_effect_question(cost: Option<&OptionalCost>, effects: &[Effect]) -> String {
    let what = clauses("this", effects);
    match cost {
        Some(cost) => format!(
            "{}? If you do, {what}",
            sentence_case(&optional_cost_phrase(cost))
        ),
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

/// How many cards a look takes, as the card would print it: *up to one creature card*
/// where the take is optional, and a bare *one of them* where it is not.
///
/// A floor equal to the ceiling is the printed *put one of them into your hand*, and the
/// "up to" has to go with it — the phrase is what tells a player whether they may decline,
/// and it is the one thing on the card that the new bound changes.
fn take_phrase(min: u32, max: u32, filter: &CardFilter) -> String {
    if min == 0 {
        return up_to(max, filter);
    }
    if max == 1 {
        format!("one {}", filter_noun(filter, false))
    } else {
        format!("{} {}", number(max), filter_noun(filter, true))
    }
}

/// The noun a [`CardFilter`] names, singular or plural, as the words a card would print
/// — "card" for the unrestricted class, so "discard a **card**" and "up to one **land
/// card**" both read as one sentence rather than needing a special case per call site.
pub(super) fn filter_noun(filter: &CardFilter, plural: bool) -> String {
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
        CardFilter::ColorOrArtifact { color } => format!("{} or artifact {card}", color.word()),
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
        PlayerRef::EachPlayer => "each player",
        PlayerRef::TargetPlayer => "target player",
        PlayerRef::TargetOpponent => "target opponent",
        PlayerRef::ThatPlayer => "that player",
    }
}

/// The class a [`PermanentFilter`] names, as a noun phrase.
///
/// One generator for a vocabulary that used to be four enums of named classes (issue
/// #824), and the reason it is one: a class is a product of axes, and English composes
/// those axes in a fixed order — `other attacking blue Dragons you control with power 4 or
/// greater`. A phrase per pairing would have to be written once per card.
///
/// `plural` picks the position. A class is a bare plural when it *acts* ("creatures you
/// control get +2/+1") and a distributive singular when it is *acted on* ("deals 2 damage
/// to each creature you control"), which is a fact about English rather than about the
/// class, so both come out of one body.
///
/// It does **not** try to reproduce printed wording. A card that prints "each creature and
/// planeswalker they control" after naming its opponents gets "…your opponents control"
/// here: the pronoun is a discourse feature of the sentence before it, not a property of
/// the class, and this formatter states behaviour rather than reproducing Oracle text
/// (`AGENTS.md`, Legal Considerations).
fn class_noun(source: &str, filter: &PermanentFilter, plural: bool) -> String {
    let mut words: Vec<String> = Vec::new();
    // "Other …" leads, because that is where a card puts it.
    if filter.except_this {
        words.push("other".to_string());
    }
    if filter.attacking {
        words.push("attacking".to_string());
    }
    if let Some(color) = filter.color {
        words.push(color.word().to_string());
    }
    // A subtype **replaces** the type noun — "Dragons you control", never "Dragon
    // creatures you control", which is not how a card is written.
    let mut nouns: Vec<String> = match (&filter.subtype, filter.card_type.as_slice()) {
        (Some(subtype), _) => vec![subtype.clone()],
        (None, []) => vec!["permanent".to_string()],
        (None, types) => types
            .iter()
            .map(|&kind| card_type_word(kind).to_string())
            .collect(),
    };
    // "Nontoken" is an adjective on the class; "token" is a noun the class qualifies —
    // "creature tokens" — so the two halves of one field land in different places.
    match filter.token {
        Some(false) => words.push("nontoken".to_string()),
        Some(true) => nouns = vec![format!("{} token", list_words_owned(&nouns))],
        None => {}
    }
    // Every noun in a disjunction agrees with the number, not just the last: "artifacts
    // and enchantments", never "artifact and enchantments".
    if plural {
        for noun in &mut nouns {
            noun.push('s');
        }
    }
    words.push(list_words_owned(&nouns));
    let mut phrase = words.join(" ");
    phrase.push_str(match filter.scope {
        ControllerScope::YouControl => " you control",
        ControllerScope::OpponentsControl => " your opponents control",
        ControllerScope::ThatPlayer => " that player controls",
        ControllerScope::Any => "",
    });
    // The qualifiers a card prints after the class, in the order it prints them.
    if let Some(keyword) = filter.keyword {
        phrase.push_str(&format!(" with {}", keyword_word(keyword)));
    }
    if let Some(keyword) = filter.without_keyword {
        phrase.push_str(&format!(" without {}", keyword_word(keyword)));
    }
    if let Some(min) = filter.min_power {
        phrase.push_str(&format!(" with power {min} or greater"));
    }
    if let Some(max) = filter.max_toughness {
        phrase.push_str(&format!(" with toughness {max} or less"));
    }
    // The source is named, because a card naming itself in its own text uses its name
    // (CR 201.4) — and the reader needs to know which creature the comparison is against.
    if filter.below_source_power {
        phrase.push_str(&format!(" with power less than {source}'s power"));
    }
    if let Some(counter) = filter.with_counter {
        phrase.push_str(&format!(
            " with a {} counter on {}",
            crate::rules_text::counter_symbol(counter),
            if plural { "them" } else { "it" }
        ));
    }
    if filter.with_the_named_card {
        phrase.push_str(" with the chosen name");
    }
    phrase
}

/// [`list_words`] over owned strings — the class nouns are built rather than borrowed.
fn list_words_owned(words: &[String]) -> String {
    list_words(&words.iter().map(String::as_str).collect::<Vec<_>>())
}

/// The class as the **subject** of its sentence — a bare plural.
fn mass_subject(source: &str, affects: &PermanentFilter) -> String {
    class_noun(source, affects, true)
}

/// The same class as the **object** of a sentence — what damage is dealt *to*, or what a
/// sweeper destroys. Distributive: "each creature you control".
fn mass_recipient(source: &str, affects: &PermanentFilter) -> String {
    format!("each {}", class_noun(source, affects, false))
}

/// Who or what damage is dealt to (CR 120.3), as a noun phrase.
///
/// The three subjects read as one sentence shape — "deals 2 damage to *any target*",
/// "…to *each opponent*", "…to *each creature*" — so a player reads a class-damage
/// effect the same way they read a targeted one, minus the word "target".
fn damage_recipient(source: &str, subject: &DamageSubject) -> String {
    match subject {
        DamageSubject::Target(spec) => target_noun(*spec).to_string(),
        DamageSubject::Players(player_ref) => player_noun(*player_ref).to_string(),
        DamageSubject::Permanents(affects) => mass_recipient(source, affects),
    }
}

/// A [`PlayerRef`] as a bare noun phrase, for an effect that acts *on* the player
/// rather than conjugating a verb after them ([`conjugate`] covers that position).
fn player_noun(player_ref: PlayerRef) -> &'static str {
    match player_ref {
        PlayerRef::Controller => "you",
        PlayerRef::EachOpponent => "each opponent",
        PlayerRef::EachPlayer => "each player",
        PlayerRef::TargetPlayer => "target player",
        PlayerRef::TargetOpponent => "target opponent",
        PlayerRef::ThatPlayer => "that player",
    }
}
