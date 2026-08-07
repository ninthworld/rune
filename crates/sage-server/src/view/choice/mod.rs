//! The mid-resolution player choice (issue #604): projecting the question, deciding who
//! may see the cards it is about, and binding the answer back.
//!
//! Everything here is a translation of a decision the engine has already made. The
//! engine says a choice is owed, who must answer it, which cards are pickable, and how
//! many of them a legal answer names; this module puts that on the wire and maps the
//! reply back, deriving no legality of its own.
//!
//! It is also the one place in the whole projection that reads a **hidden** zone — a
//! library, or a hand that is not the receiver's. The `chooser == viewer` gate in
//! [`revealed_to`] is therefore load-bearing in a way nothing else here is: get it
//! wrong and a deck leaks.

use super::*;

/// The slot id of a mid-resolution player choice's card pick (issue #604) — and of the
/// yes-or-no an optional effect asks in the same position (issue #610).
pub(crate) const CHOICE_SLOT: &str = "choice";

/// The option id that accepts an optional effect. Its absence from the offered options
/// is how "you cannot pay for this right now" reaches the client.
pub(crate) const ACCEPT_OPTION: &str = "accept";

/// The option id that declines an optional effect. Always offered — declining is always
/// legal, which is what keeps an unpayable cost from stalling the game.
pub(crate) const DECLINE_OPTION: &str = "decline";

/// The five colors of mana (CR 105.1) as `(option id, label)` pairs — the complete,
/// always-legal answer set of a color choice, in the canonical WUBRG order a player
/// expects to read them in.
const COLOR_OPTIONS: [(&str, &str, Color); 5] = [
    ("white", "White ({W})", Color::White),
    ("blue", "Blue ({U})", Color::Blue),
    ("black", "Black ({B})", Color::Black),
    ("red", "Red ({R})", Color::Red),
    ("green", "Green ({G})", Color::Green),
];

/// The single prompt slot for the mid-resolution player choice the game is waiting on,
/// or an empty list when it is waiting on none.
///
/// Two shapes, one slot: a card selection projects as [`Prompt::SelectFromZone`] (issue
/// #604) and an optional effect's yes-or-no as [`Prompt::Option`] (issue #610). Neither
/// is a new wire shape — the second reuses the prompt the mulligan decision already
/// rides on — so a client that can answer one can answer the other.
///
/// The engine owns everything here: which cards are pickable
/// ([`choice_candidates`](sage_engine::choice_candidates)), how many of them a legal
/// answer names ([`choice_bounds`](sage_engine::choice_bounds), already clamped to what
/// the zone actually holds), and whether an optional cost is payable at this instant
/// ([`confirm_is_payable`](sage_engine::confirm_is_payable)). This projects those and
/// adds words — it derives no legality of its own, and a client picking from this list
/// cannot construct an answer the engine has not already sanctioned.
pub(crate) fn player_choice_prompts(state: &GameState, db: &CardDatabase) -> Vec<Prompt> {
    let Some(pending) = pending_player_choice(state) else {
        return Vec::new();
    };
    match &pending.question {
        ChoiceQuestion::Cards(request) => vec![card_choice_prompt(state, db, request)],
        ChoiceQuestion::Confirm(request) => vec![confirm_prompt(state, db, request)],
        ChoiceQuestion::Color(request) => vec![color_prompt(request, db)],
        ChoiceQuestion::Replacement(_) => vec![replacement_prompt(state, db)],
        ChoiceQuestion::Order(request) => vec![card_order_prompt(state, request)],
        ChoiceQuestion::CardName(request) => vec![card_name_prompt(request, db)],
        ChoiceQuestion::Permanents(request) => vec![permanent_choice_prompt(state, db, request)],
        ChoiceQuestion::Permanent(request) => vec![permanent_prompt(state, db, request)],
        // The offer to play a card poses **no prompt at all**: its answers are real
        // actions the client already knows how to draw — a cast, a land play, and a
        // decline — so a slot here would be a second way to say the same thing
        // (issue #787).
        ChoiceQuestion::PlayCard(_) => Vec::new(),
    }
}

/// The option id that declines an optional permanent choice — `You may have this enter
/// as a copy …`, answered with "no". Offered only when the card wrote the question as
/// optional, which is what keeps a mandatory choice from being dodged.
pub(crate) const DECLINE_COPY_OPTION: &str = "none";

/// The CR 614.12 permanent choice, as the same `option` slot every other non-card answer
/// rides on.
///
/// One option per candidate the engine derived, labelled by the permanent's name, with
/// the option **id** being that permanent's entity id — the id the board already draws it
/// under, so a client that wants to answer by clicking the card has everything it needs
/// and one that renders a list of buttons is no worse off. Nothing here filters: the
/// candidate set is the engine's ([`copy_choice_candidates`]).
fn permanent_prompt(state: &GameState, db: &CardDatabase, request: &CopyChoiceRequest) -> Prompt {
    let chooser = pending_player_choice(state).map_or(PlayerId(0), |pending| pending.chooser);
    let mut options: Vec<PromptOption> = copy_choice_candidates(state, request.of, chooser, db)
        .into_iter()
        .filter_map(|id| {
            let perm = state.battlefield.iter().find(|p| p.id == id)?;
            Some(PromptOption {
                id: permanent_entity_id(id),
                label: permanent_name(state, perm, db),
                requires: Vec::new(),
            })
        })
        .collect();
    if request.optional {
        options.push(PromptOption {
            id: DECLINE_COPY_OPTION.to_string(),
            label: "Enter as itself".to_string(),
            requires: Vec::new(),
        });
    }
    Prompt::Option {
        slot: CHOICE_SLOT.to_string(),
        prompt: permanent_question(request),
        options,
    }
}

/// The words a permanent choice is asked in. It says what the answer *does*, because the
/// options are a list of creatures and nothing about them says why they are being read.
fn permanent_question(request: &CopyChoiceRequest) -> String {
    let noun = match request.of {
        CopyClass::AnyCreature => "a creature",
        CopyClass::CreatureYouControl => "a creature you control",
    };
    format!("Choose {noun} to copy")
}

/// The sacrifice, as the same [`Prompt::SelectFromZone`] slot a card pick rides on —
/// over the battlefield rather than a hidden zone.
///
/// Not a new wire shape: the zone string is free-form by design, so a client that can
/// select cards from a hand can select permanents from a board with no new code. The
/// candidates are the engine's
/// ([`permanent_choice_candidates`](sage_engine::permanent_choice_candidates)) and the
/// count is the engine's already-clamped bound, so a player asked to sacrifice two who
/// controls one is offered exactly one and the submit means what it says.
///
/// Nothing here is hidden information: the battlefield is public, so unlike a card pick
/// this prompt reveals nothing to anybody.
fn permanent_choice_prompt(
    state: &GameState,
    db: &CardDatabase,
    request: &PermanentRequest,
) -> Prompt {
    let (min, max) = permanent_choice_bounds(state, request, db);
    let candidates: Vec<String> = permanent_choice_candidates(state, request, db)
        .into_iter()
        .map(permanent_entity_id)
        .collect();
    Prompt::SelectFromZone {
        slot: CHOICE_SLOT.to_string(),
        prompt: permanent_choice_question(request, max),
        zone: "battlefield".to_string(),
        owner: player_id(request.subject),
        count: max,
        min: (min != max).then_some(min),
        candidates,
    }
}

/// The words a sacrifice is asked in, composed from the request rather than from the
/// card that posed it — the discipline [`choice_prompt_text`] follows, for the reason it
/// follows it.
fn permanent_choice_question(request: &PermanentRequest, max: u32) -> String {
    // The class the card names, in the same word its rules text uses, pluralized where
    // English wants it — nothing here invents a noun the printed sentence does not have.
    let noun = if max == 1 {
        crate::rules_text::sacrifice_noun(request.card_type, request.subtype.as_deref())
    } else {
        crate::rules_text::plural_sacrifice_noun(request.card_type, request.subtype.as_deref())
    };
    // The **request's own** floor, not the clamped one, is what says whose decision the
    // size is: every counted sacrifice asks for exactly its count, so a floor of none on a
    // question worth posing is the open form and nothing else. Asking that one for a number
    // would name the board's total as though the card demanded it.
    if request.min == 0 && request.max > 0 {
        return format!("Sacrifice any number of {noun}");
    }
    match (&request.outcome, request.except) {
        // A question that excludes the asking permanent is the `another` of a cost, and
        // reads as the card writes it rather than as a count of one.
        (PermanentOutcome::Sacrifice, Some(_)) if max == 1 => format!("Sacrifice another {noun}"),
        (PermanentOutcome::Sacrifice, _) => format!("Sacrifice {max} {noun}"),
        // The as-enters question names what goes where, in the card's own words, and is
        // always for exactly one permanent.
        (PermanentOutcome::CounterOnEntry { counter, .. }, _) => format!(
            "Put a {} counter on {noun}",
            crate::rules_text::counter_symbol(*counter)
        ),
    }
}

/// The CR 614.12 card-naming question, as the same `option` slot every other
/// non-selection question rides on.
///
/// One option per card the engine says may be named
/// ([`named_card_candidates`](sage_engine::named_card_candidates)), in the order it
/// derived them. **The option id is the card's authored `functional_id`** — the stable
/// identity a card is named by everywhere in this project (ADR 0008 §3) — and the label
/// is the card's own name from the catalog. Neither is composed here and neither can be
/// anything else: the client picks from this list, so it never sends a name, and the
/// engine re-derives the list and re-checks the answer against it.
fn card_name_prompt(request: &CardNameRequest, db: &CardDatabase) -> Prompt {
    Prompt::Option {
        slot: CHOICE_SLOT.to_string(),
        prompt: card_name_question(request, db),
        options: named_card_candidates(db, request.class)
            .into_iter()
            .filter_map(|card| {
                Some(PromptOption {
                    id: db.card(card)?.functional_id.to_string(),
                    label: card_name(card, db),
                    requires: Vec::new(),
                })
            })
            .collect(),
    }
}

/// The words a card-naming question is asked in. It names the entering card, for the
/// reason the colour question does: a player who has just cast two things needs to know
/// which of them is asking, and the answer is a lasting property of the permanent that
/// is about to arrive.
fn card_name_question(request: &CardNameRequest, db: &CardDatabase) -> String {
    match request.entry.object.card() {
        Some(card) => format!(
            "Choose a card name as {} enters the battlefield",
            card_name(card.card, db)
        ),
        // A token is never asked (`create_token` consults no naming question), so this
        // arm is unreachable in play; naming no card is the honest rendering.
        None => "Choose a card name as this permanent enters the battlefield".to_string(),
    }
}

/// The *in any order* of a look, as the `order` prompt the combat-damage assignment
/// already rides on (CR 510.1) — a permutation of the items, and no new wire shape.
///
/// Its items are the engine's own remainder ([`order_candidates`]), recomputed now, and
/// the cards behind those ids reach the chooser on the same view's `revealed` array and
/// no other seat's — the same channel a searched library uses, for the same reason: this
/// is the top of somebody's deck.
fn card_order_prompt(state: &GameState, request: &OrderRequest) -> Prompt {
    Prompt::Order {
        slot: CHOICE_SLOT.to_string(),
        prompt: card_order_question(),
        items: order_candidates(state, request)
            .into_iter()
            .map(|inst| card_entity_id(inst.id))
            .collect(),
    }
}

/// The words the card-ordering question is asked in. It states **which end is which**,
/// because a permutation the player cannot orient is a coin flip: the first card named is
/// the deepest, the convention every bottoming in the engine follows.
fn card_order_question() -> String {
    "Choose the order these go on the bottom of your library, deepest first".to_string()
}

/// The CR 616.1 ordering question, as the same `option` slot the yes-or-no and the
/// colour choice already ride on.
///
/// One option per applicable replacement, in the order the engine derived them, labelled
/// with the words the card itself is written in. The option **id is the position** —
/// which is what the engine's answer names ([`Action::AnswerReplacement`]) — so the list
/// the player is shown and the list the answer indexes into are the same list by
/// construction.
fn replacement_prompt(state: &GameState, db: &CardDatabase) -> Prompt {
    let options = pending_replacement_options(state, db)
        .iter()
        .enumerate()
        .map(|(index, offered)| PromptOption {
            id: index.to_string(),
            label: offered_replacement_label(offered),
            requires: Vec::new(),
        })
        .collect();
    Prompt::Option {
        slot: CHOICE_SLOT.to_string(),
        prompt: replacement_question(),
        options,
    }
}

/// The words the ordering question is asked in. It names no card: what the player is
/// deciding between is *the effects*, and those are the option labels.
fn replacement_question() -> String {
    "Choose which replacement effect applies first".to_string()
}

/// One offered replacement as a label, drawn from the same formatter the card's rules
/// text uses so the option and the card read the same way.
fn offered_replacement_label(offered: &OfferedReplacement) -> String {
    let clause = match offered {
        OfferedReplacement::SelfReplacement(ability) => match ability {
            Ability::EntersTapped => "it enters tapped".to_string(),
            Ability::EntersWithCounters {
                counter,
                count,
                from_announced_x,
            } => format!(
                "it enters with {} on it",
                crate::rules_text::entering_counters(*counter, *count, *from_announced_x)
            ),
            // Every other ability shape is filtered out before it reaches here; a
            // generic label is the right amount of damage for one that somehow did.
            _ => "a replacement effect".to_string(),
        },
        OfferedReplacement::Created(effect) => crate::rules_text::replacement_phrase(effect),
    };
    crate::rules_text::sentence_case(&clause)
}

/// The color question, as the same `option` slot the yes-or-no already rides on.
///
/// All five colors, always, in WUBRG order: there is no state to consult, because a
/// color is legal by being a color (CR 105.1). The prompt says what the mana may be
/// spent on when the effect restricted it, since that is the only thing that makes one
/// answer better than another.
fn color_prompt(request: &ColorRequest, db: &CardDatabase) -> Prompt {
    Prompt::Option {
        slot: CHOICE_SLOT.to_string(),
        prompt: color_question(request, db),
        options: COLOR_OPTIONS
            .iter()
            .map(|(id, label, _)| PromptOption {
                id: (*id).to_string(),
                label: (*label).to_string(),
                requires: Vec::new(),
            })
            .collect(),
    }
}

/// The words a color choice is asked in — five identical answers make the *question*
/// the only thing that tells a player what they are deciding.
///
/// Three sentences for two outcomes: adding mana, adding mana that may only be spent
/// somewhere ("choose a color" and "choose a color you may only spend on Dragons" are
/// different decisions), and naming the colour a permanent enters with. The last one
/// names the card, because a player who has just cast two things needs to know which of
/// them is asking, and because that colour is a lasting property of a permanent rather
/// than a point of mana about to be spent.
fn color_question(request: &ColorRequest, db: &CardDatabase) -> String {
    match &request.outcome {
        ColorOutcome::AddMana {
            amount,
            restriction: Some(restriction),
        } => format!(
            "Choose a color of mana to add{} — you may spend it only to {}",
            color_mana_count(*amount),
            crate::rules_text::restriction_phrase(restriction)
        ),
        ColorOutcome::AddMana {
            amount,
            restriction: None,
        } => format!("Choose a color of mana to add{}", color_mana_count(*amount)),
        ColorOutcome::RecordOnEntry(entry) => match entry.object.card() {
            Some(card) => format!(
                "Choose a color as {} enters the battlefield",
                card_name(card.card, db)
            ),
            // A token is never asked (`create_token` consults no colour question), so
            // this arm is unreachable in play; naming no card is the honest rendering.
            None => "Choose a color as this permanent enters the battlefield".to_string(),
        },
    }
}

/// How much mana one colour answer produces, for the question that produces more than
/// one point of it — `Add two mana of any one color` is a single decision worth two, and
/// a player choosing has to be told which.
///
/// Empty for the ordinary one-point answer, so every question that existed before a
/// single-colour clause did reads exactly as it did.
fn color_mana_count(amount: u8) -> String {
    if amount <= 1 {
        String::new()
    } else {
        format!(" ({amount} mana of that color)")
    }
}

/// The yes-or-no an optional effect poses, as the `option` slot the client already
/// knows how to answer.
///
/// **Accepting is offered only while the engine would accept it.** A cost the chooser
/// cannot pay from the pool as it stands leaves one option — declining — which is the
/// honest rendering of the position they are in: they may still tap lands (CR 605.3a,
/// offered alongside this action), and the acceptance reappears the moment the mana is
/// there.
fn confirm_prompt(state: &GameState, db: &CardDatabase, request: &ConfirmRequest) -> Prompt {
    let mut options = Vec::new();
    if confirm_is_payable(state, db) {
        options.push(PromptOption {
            id: ACCEPT_OPTION.to_string(),
            // The payment in the words the card writes it in — "Pay {1}", "Sacrifice
            // another creature" — so the button and the printed sentence are one phrase.
            label: match &request.cost {
                Some(cost) => {
                    crate::rules_text::sentence_case(&crate::rules_text::optional_cost_phrase(cost))
                }
                None => "Yes".to_string(),
            },
            requires: Vec::new(),
        });
    }
    options.push(PromptOption {
        id: DECLINE_OPTION.to_string(),
        label: "Decline".to_string(),
        requires: Vec::new(),
    });
    Prompt::Option {
        slot: CHOICE_SLOT.to_string(),
        prompt: optional_effect_question(request.cost.as_ref(), &request.effects),
        options,
    }
}

/// The card pick, as a [`Prompt::SelectFromZone`].
///
/// `min` rides the wire only when it differs from the maximum, which is exactly when the
/// choice is one a player may legally under-fill: scrying *any number*, taking *up to*
/// one, or failing to find (CR 701.19c). A discard is exact and elides it.
fn card_choice_prompt(state: &GameState, db: &CardDatabase, request: &ChoiceRequest) -> Prompt {
    let (min, max) = choice_bounds(state, request, db);
    let candidates: Vec<String> = choice_candidates(state, request, db)
        .into_iter()
        .map(|inst| card_entity_id(inst.id))
        .collect();
    Prompt::SelectFromZone {
        slot: CHOICE_SLOT.to_string(),
        prompt: choice_prompt_text(state, request, min, max),
        zone: match request.zone {
            ChoiceZone::Hand => "hand".to_string(),
            ChoiceZone::LibraryTop(_) | ChoiceZone::Library => "library".to_string(),
        },
        owner: player_id(request.subject),
        count: max,
        min: (min != max).then_some(min),
        candidates,
    }
}

/// The words a mid-resolution choice is asked in, composed from the request the engine
/// posed rather than from the card that posed it — so a new card asking the same shape
/// of question reads correctly with no extra prose to write.
fn choice_prompt_text(state: &GameState, request: &ChoiceRequest, min: u32, max: u32) -> String {
    let cards = if max == 1 { "card" } else { "cards" };
    let how_many = if min == max {
        format!("{max} {cards}")
    } else {
        format!("up to {max} {cards}")
    };
    match &request.outcome {
        // What they will be is worth saying, because the cards stop being what they are:
        // a player choosing which to turn down is choosing what to lose sight of.
        ChoiceOutcome::PutOntoBattlefieldFaceDown(values) => format!(
            "Choose {how_many} to put onto the battlefield face down as {}",
            crate::rules_text::face_down_noun(values)
        ),
        ChoiceOutcome::Discard => {
            // Whose hand it is matters here and nowhere else: this is the one choice a
            // player can be asked to make about a zone that is not theirs.
            if request.subject == state.priority {
                format!("Discard {how_many}")
            } else {
                format!(
                    "Choose {how_many} from {}'s hand to discard",
                    player_id(request.subject)
                )
            }
        }
        ChoiceOutcome::BottomChosen => {
            format!("Choose {how_many} to put on the bottom of your library, in that order")
        }
        // Whether the rest are bottomed at random or in an order the player is about to
        // be asked for is left out on purpose: this sentence is about *this* question,
        // and the arrangement gets a question, and a sentence, of its own.
        ChoiceOutcome::TakeAndBottomRest { .. } => {
            format!("Choose {how_many} to keep; the rest go to the bottom of your library")
        }
        ChoiceOutcome::TakeAndShuffle(_) => {
            format!("Search your library and choose {how_many}; it is then shuffled")
        }
    }
}

/// The dock label for the `player_choice` action: the same sentence its prompt asks,
/// so the button a player clicks says what it is about to ask rather than a generic
/// "Choose". Falls back to a bare label when no choice is owed, which the projection
/// never reaches (the action is only offered while one is).
pub(crate) fn player_choice_label(state: &GameState, db: &CardDatabase) -> String {
    match pending_player_choice(state).map(|pending| &pending.question) {
        Some(ChoiceQuestion::Cards(request)) => {
            let (min, max) = choice_bounds(state, request, db);
            choice_prompt_text(state, request, min, max)
        }
        Some(ChoiceQuestion::Confirm(request)) => {
            optional_effect_question(request.cost.as_ref(), &request.effects)
        }
        Some(ChoiceQuestion::Color(request)) => color_question(request, db),
        Some(ChoiceQuestion::Replacement(_)) => replacement_question(),
        Some(ChoiceQuestion::PlayCard(request)) => {
            let name = card_name(request.card.card, db);
            if request.free {
                format!("You may play {name} without paying its mana cost")
            } else {
                format!("You may play {name}")
            }
        }
        Some(ChoiceQuestion::Order(_)) => card_order_question(),
        Some(ChoiceQuestion::Permanent(request)) => permanent_question(request),
        Some(ChoiceQuestion::Permanents(request)) => {
            let (_, max) = permanent_choice_bounds(state, request, db);
            permanent_choice_question(request, max)
        }
        Some(ChoiceQuestion::CardName(request)) => card_name_question(request, db),
        None => "Make a choice".to_string(),
    }
}

/// The cards `viewer` is currently being shown from a hidden zone: the candidates of
/// the mid-resolution choice they are being asked to answer (issue #604), and nothing
/// else, ever.
///
/// Empty unless a choice is owed **and this seat is its chooser** — so a searched
/// library reaches the searcher and no one else, and a hand read by a coercive discard
/// reaches the reader rather than the table. The [`SpectatorView`] has no counterpart
/// field at all: a spectator is structurally incapable of receiving these.
///
/// Two questions are about cards and both are covered: the selection, and the *ordering*
/// of what a look did not take. The second is not optional — its prompt names those cards
/// by id, and a client with an id and no card has nothing to draw.
pub(crate) fn revealed_to(state: &GameState, db: &CardDatabase, viewer: PlayerId) -> Vec<CardView> {
    let Some(pending) = pending_player_choice(state) else {
        return Vec::new();
    };
    // **A reveal is public** (CR 701.16a, issue #787). An offer to play a revealed card
    // shows that card to *every* seat, not only to the player deciding — the card said
    // "reveal", and a reveal only its chooser can see is a look wearing the wrong word.
    // This is the one question here whose cards are not private, which is why it is
    // answered before the chooser test rather than inside the match below.
    if let Some(request) = pending.question.play_card() {
        return state
            .players
            .get(request.subject.0)
            .and_then(|player| {
                player
                    .library
                    .iter()
                    .find(|card| card.id == request.card.id)
            })
            .map(|card| vec![card_view(card_entity_id(card.id), card.card, db)])
            .unwrap_or_default();
    }
    if pending.chooser != viewer {
        return Vec::new();
    }
    // A yes-or-no, a colour, a card name, and a replacement ordering are about an effect
    // rather than a zone: they show nobody anything, so there is nothing here for even
    // their own chooser.
    let shown: Vec<CardInstance> = match &pending.question {
        // **A look shows what it looks at, not what it lets you take** (issue #773). The two
        // are different sets whenever the card names a class: *look at the top five … you may
        // put a black card from among them into your hand* shows five cards and offers some
        // of them. Projecting the candidates showed a player only the black ones and called
        // that a look at five. The answer a client may submit is still bound to
        // [`choice_candidates`](sage_engine::choice_candidates) — this widens what is *seen*,
        // never what is answerable, and it is still sent to the chooser and to nobody else.
        //
        // **Only a look at the top of a library.** The count is printed on the card and seeing
        // those cards is what the card grants, so showing all of them is the card being
        // honoured. A **search** (`ChoiceZone::Library`) is a different verb: CR 701.19a does
        // say the searcher looks at every card in that library, so today's projection — only
        // the cards that match — understates it, but the surface for pouring a whole library
        // into a view is a design question of its own and not this fix's to answer. A hand is
        // not affected either way: its owner already holds it.
        ChoiceQuestion::Cards(request) => match request.zone {
            ChoiceZone::LibraryTop(_) => choice_looked_at(state, request),
            ChoiceZone::Library | ChoiceZone::Hand => choice_candidates(state, request, db),
        },
        ChoiceQuestion::Order(request) => order_candidates(state, request),
        ChoiceQuestion::Confirm(_)
        | ChoiceQuestion::Color(_)
        | ChoiceQuestion::CardName(_)
        | ChoiceQuestion::Replacement(_)
        // The battlefield is public: a sacrifice, and a permanent named as a card
        // enters, reveal nothing to anybody.
        | ChoiceQuestion::Permanents(_)
        // Answered above, before the chooser test, because its card is public.
        | ChoiceQuestion::PlayCard(_)
        | ChoiceQuestion::Permanent(_) => Vec::new(),
    };
    shown
        .into_iter()
        .map(|inst| card_view(card_entity_id(inst.id), inst.card, db))
        .collect()
}

mod bind;
pub(crate) use bind::*;

#[cfg(test)]
mod tests;
