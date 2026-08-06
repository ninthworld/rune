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
        ChoiceQuestion::Confirm(request) => vec![confirm_prompt(state, request)],
        ChoiceQuestion::Color(request) => vec![color_prompt(request, db)],
        ChoiceQuestion::Replacement(_) => vec![replacement_prompt(state, db)],
    }
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
            Ability::EntersWithCounters { counter, count } => format!(
                "it enters with {} on it",
                crate::rules_text::counters(*counter, *count)
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
            restriction: Some(restriction),
        } => format!(
            "Choose a color of mana to add — you may spend it only to {}",
            crate::rules_text::restriction_phrase(restriction)
        ),
        ColorOutcome::AddMana { restriction: None } => "Choose a color of mana to add".to_string(),
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

/// The yes-or-no an optional effect poses, as the `option` slot the client already
/// knows how to answer.
///
/// **Accepting is offered only while the engine would accept it.** A cost the chooser
/// cannot pay from the pool as it stands leaves one option — declining — which is the
/// honest rendering of the position they are in: they may still tap lands (CR 605.3a,
/// offered alongside this action), and the acceptance reappears the moment the mana is
/// there.
fn confirm_prompt(state: &GameState, request: &ConfirmRequest) -> Prompt {
    let mut options = Vec::new();
    if confirm_is_payable(state) {
        options.push(PromptOption {
            id: ACCEPT_OPTION.to_string(),
            label: match &request.cost {
                Some(cost) => format!("Pay {cost}"),
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
        prompt: optional_effect_question(request.cost.as_deref(), &request.effects),
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
    match request.outcome {
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
        ChoiceOutcome::TakeAndBottomRest(_) => {
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
            optional_effect_question(request.cost.as_deref(), &request.effects)
        }
        Some(ChoiceQuestion::Color(request)) => color_question(request, db),
        Some(ChoiceQuestion::Replacement(_)) => replacement_question(),
        None => "Make a choice".to_string(),
    }
}

/// Map a returned answer to the CR 616.1 ordering choice onto
/// [`Action::AnswerReplacement`].
///
/// The same reject-stale discipline the other three answers follow: only an option id
/// the offer itself listed counts, and the engine is asked whether an ordering choice is
/// owed at all before one is built. The id *is* the index, so the parse is the binding;
/// the engine independently re-derives the option list and rejects an index past its end.
pub(crate) fn bind_player_replacement(
    state: &GameState,
    offered: &ValidAction,
    targets: &[TargetChoice],
) -> Option<Action> {
    let options = offered.prompts.iter().find_map(|prompt| match prompt {
        Prompt::Option { slot, options, .. } if slot == CHOICE_SLOT => Some(options),
        _ => None,
    })?;
    pending_player_choice(state)?.question.replacement()?;
    let answer = chosen_for(targets, CHOICE_SLOT).first()?;
    if !options.iter().any(|option| &option.id == answer) {
        return None;
    }
    Some(Action::AnswerReplacement {
        index: answer.parse().ok()?,
    })
}

/// Map a returned answer to the `player_choice` action onto the concrete
/// [`Action::AnswerChoice`] (issue #604): the single `choice`
/// [`Prompt::SelectFromZone`] slot names cards from its freshly recomputed candidates,
/// **in the order the client sent them** — which is the order a scry puts them on the
/// bottom in, so re-sorting here would silently answer a different question.
///
/// An unanswered slot is a legal *empty* selection whenever the prompt's advertised
/// minimum is zero (declining to scry, failing to find); the engine re-checks the whole
/// selection against the choice's own bounds anyway
/// ([`answer_is_legal`](sage_engine::apply_action)), so nothing here is the last word.
/// `None` only when the answer names an id the offer did not.
pub(crate) fn bind_player_choice(
    state: &GameState,
    db: &CardDatabase,
    offered: &ValidAction,
    targets: &[TargetChoice],
) -> Option<Action> {
    let candidates = offered.prompts.iter().find_map(|prompt| match prompt {
        Prompt::SelectFromZone {
            slot, candidates, ..
        } if slot == CHOICE_SLOT => Some(candidates),
        _ => None,
    })?;
    let request = pending_player_choice(state)?.question.cards()?;
    let available = choice_candidates(state, request, db);
    let mut chosen = Vec::new();
    for id in chosen_for(targets, CHOICE_SLOT) {
        if !candidates.contains(id) {
            return None;
        }
        let inst = available
            .iter()
            .find(|inst| card_entity_id(inst.id) == *id)?;
        chosen.push(inst.id);
    }
    Some(Action::AnswerChoice { chosen })
}

/// The cards `viewer` is currently being shown from a hidden zone: the candidates of
/// the mid-resolution choice they are being asked to answer (issue #604), and nothing
/// else, ever.
///
/// Empty unless a choice is owed **and this seat is its chooser** — so a searched
/// library reaches the searcher and no one else, and a hand read by a coercive discard
/// reaches the reader rather than the table. The [`SpectatorView`] has no counterpart
/// field at all: a spectator is structurally incapable of receiving these.
pub(crate) fn revealed_to(state: &GameState, db: &CardDatabase, viewer: PlayerId) -> Vec<CardView> {
    let Some(pending) = pending_player_choice(state) else {
        return Vec::new();
    };
    if pending.chooser != viewer {
        return Vec::new();
    }
    // A yes-or-no is about an effect, not about a zone: it shows nobody anything, so
    // there is nothing here for even its own chooser.
    let Some(request) = pending.question.cards() else {
        return Vec::new();
    };
    choice_candidates(state, request, db)
        .into_iter()
        .map(|inst| card_view(card_entity_id(inst.id), inst.card, db))
        .collect()
}

/// Map a returned answer to a color choice onto [`Action::AnswerColor`].
///
/// The same reject-stale discipline the other two answers follow: only an option id the
/// offer itself listed counts, and the engine is asked whether a color choice is owed at
/// all before one is built. `None` when no color choice is owed or the answer names
/// something the offer did not.
pub(crate) fn bind_player_color(
    state: &GameState,
    offered: &ValidAction,
    targets: &[TargetChoice],
) -> Option<Action> {
    let options = offered.prompts.iter().find_map(|prompt| match prompt {
        Prompt::Option { slot, options, .. } if slot == CHOICE_SLOT => Some(options),
        _ => None,
    })?;
    pending_player_choice(state)?.question.color()?;
    let answer = chosen_for(targets, CHOICE_SLOT).first()?;
    if !options.iter().any(|option| &option.id == answer) {
        return None;
    }
    let (_, _, color) = COLOR_OPTIONS.iter().find(|(id, _, _)| id == answer)?;
    Some(Action::AnswerColor { color: *color })
}

/// Map a returned answer to the yes-or-no of an optional effect onto
/// [`Action::AnswerConfirm`] (issue #610).
///
/// The slot's answer is one option id, and only an id the offer itself listed counts:
/// an answer naming an accept the server did not offer — because the cost was not
/// payable when the view was built — is rejected rather than quietly read as a decline,
/// the same reject-stale discipline the card selection follows. `None` when no yes-or-no
/// is owed or the answer names nothing the offer did.
pub(crate) fn bind_player_confirm(
    state: &GameState,
    offered: &ValidAction,
    targets: &[TargetChoice],
) -> Option<Action> {
    let options = offered.prompts.iter().find_map(|prompt| match prompt {
        Prompt::Option { slot, options, .. } if slot == CHOICE_SLOT => Some(options),
        _ => None,
    })?;
    pending_player_choice(state)?.question.confirm()?;
    let answer = chosen_for(targets, CHOICE_SLOT).first()?;
    if !options.iter().any(|option| &option.id == answer) {
        return None;
    }
    Some(Action::AnswerConfirm {
        accept: answer == ACCEPT_OPTION,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::test_support::fixture;

    /// A two-player state, one turn in, with seat 0 holding priority in its main
    /// phase and both pools stocked so payability decides nothing.
    fn main_phase() -> GameState {
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        state.priority = PlayerId(0);
        for player in &mut state.players {
            for colour in [
                Color::White,
                Color::Blue,
                Color::Black,
                Color::Red,
                Color::Green,
            ] {
                player.mana_pool.add(colour, 10);
            }
            player.mana_pool.add_colorless(10);
        }
        state
    }

    /// Cast `slug` from seat 0's hand at `targets` and let both players pass, so the
    /// spell resolves and any choice it poses is owed.
    fn cast_and_resolve(
        state: &GameState,
        db: &CardDatabase,
        slug: &str,
        targets: Vec<Target>,
    ) -> GameState {
        let mut state = state.clone();
        let instance = state.new_instance(fixture(slug));
        state.players[0].hand.push(instance);
        let state = sage_engine::apply_action(
            &state,
            &Action::CastSpell {
                card: instance,
                mode: None,
                x: None,
                targets,
                payment: Vec::new(),
            },
            db,
        );
        let state = sage_engine::apply_action(&state, &Action::PassPriority, db);
        sage_engine::apply_action(&state, &Action::PassPriority, db)
    }

    /// The `player_choice` action on a seat's view, if it is offered one.
    fn choice_action(view: &GameView) -> Option<&ValidAction> {
        view.valid_actions
            .iter()
            .find(|a| a.kind == "player_choice")
    }

    #[test]
    fn issue_604_the_choice_projects_as_one_select_from_zone_bound_to_its_chooser() {
        // Mind Rot: the targeted seat is offered a single `player_choice` action whose
        // one prompt carries the engine's own candidates and exact count. The caster is
        // offered nothing at all — the engine hands priority to the chooser, and a
        // non-priority seat's view carries no actions.
        let db = CardDatabase::bundled().unwrap();
        let mut state = main_phase();
        for slug in ["forest", "shock", "murder"] {
            let inst = state.new_instance(fixture(slug));
            state.players[1].hand.push(inst);
        }
        let state = cast_and_resolve(&state, &db, "mind_rot", vec![Target::Player(PlayerId(1))]);

        let chooser = personalized_view(&state, &db, PlayerId(1));
        let action = choice_action(&chooser).expect("the targeted seat is asked");
        assert!(!action.token.is_empty(), "a prompt action is token-bound");
        assert_eq!(action.prompts.len(), 1);
        let Prompt::SelectFromZone {
            slot,
            zone,
            owner,
            count,
            min,
            candidates,
            ..
        } = &action.prompts[0]
        else {
            panic!("the choice is a select_from_zone");
        };
        assert_eq!(slot, CHOICE_SLOT);
        assert_eq!(zone, "hand");
        assert_eq!(owner, &player_id(PlayerId(1)));
        assert_eq!(*count, 2);
        assert_eq!(*min, None, "an exact discard states no lower bound");
        assert_eq!(candidates.len(), 3);

        // The caster sees no decision surface while the other seat is answering.
        let caster = personalized_view(&state, &db, PlayerId(0));
        assert!(caster.valid_actions.is_empty());

        // The answer round-trips back to the engine action, preserving the order sent.
        let picked: Vec<String> = vec![candidates[2].clone(), candidates[0].clone()];
        let answer = ChooseAction {
            action_id: action.id.clone(),
            token: action.token.clone(),
            targets: vec![TargetChoice {
                slot: CHOICE_SLOT.to_string(),
                chosen: picked.clone(),
            }],
            ..Default::default()
        };
        let Some(Action::AnswerChoice { chosen }) =
            resolve_action(&state, &db, PlayerId(1), &answer)
        else {
            panic!("the answer resolves to an AnswerChoice");
        };
        assert_eq!(
            chosen
                .iter()
                .map(|id| card_entity_id(*id))
                .collect::<Vec<_>>(),
            picked,
            "the chosen order is carried through, not re-sorted",
        );

        // An id the offer did not name is rejected rather than dropped.
        let forged = ChooseAction {
            action_id: action.id.clone(),
            token: action.token.clone(),
            targets: vec![TargetChoice {
                slot: CHOICE_SLOT.to_string(),
                chosen: vec!["card_999999".to_string()],
            }],
            ..Default::default()
        };
        assert!(resolve_action(&state, &db, PlayerId(1), &forged).is_none());
    }

    #[test]
    fn issue_604_a_searched_library_reaches_the_searcher_and_no_other_seat() {
        // The hidden-information rule, stated as a leak test: seat 0 searches its own
        // library, so seat 0's view carries those cards and seat 1's — and a
        // spectator's — carry no trace of them anywhere in the serialized payload.
        let db = CardDatabase::bundled().unwrap();
        let mut state = main_phase();
        let caller = PermanentId(state.mint_id());
        let instance = state.new_instance(fixture("elvish_clancaller"));
        state.battlefield.push(sage_engine::Permanent {
            id: caller,
            instance: instance.id,
            printed: fixture("elvish_clancaller").into(),
            controller: PlayerId(0),
            tapped: false,
            entered_turn: 0,
            attacking: None,
            blocking: Vec::new(),
            skips_untap: false,
            damage: 0,
            counters: std::collections::BTreeMap::new(),
            attached_to: None,
            chosen_color: None,
        });
        let library: Vec<CardInstance> = ["forest", "elvish_clancaller", "island"]
            .iter()
            .map(|slug| state.new_instance(fixture(slug)))
            .collect();
        state.players[0].library = library.clone();

        let state = sage_engine::apply_action(
            &state,
            &Action::ActivateAbility {
                permanent: caller,
                index: 1,
                targets: Vec::new(),
                payment: Vec::new(),
            },
            &db,
        );
        let state = sage_engine::apply_action(&state, &Action::PassPriority, &db);
        let state = sage_engine::apply_action(&state, &Action::PassPriority, &db);
        assert!(
            pending_player_choice(&state).is_some(),
            "the search is waiting on its controller",
        );

        // The searcher sees the one card the search may find.
        let searcher = personalized_view(&state, &db, PlayerId(0));
        assert_eq!(searcher.revealed.len(), 1);
        assert_eq!(searcher.revealed[0].name, "Elvish Clancaller");

        // Nobody else sees any of it — not the revealed card, and not any other card
        // of that library either.
        let opponent = personalized_view(&state, &db, PlayerId(1));
        assert!(opponent.revealed.is_empty());
        let spectator = serde_json::to_string(&spectator_view(&state, &db)).unwrap();
        let opponent_json = serde_json::to_string(&opponent).unwrap();
        for card in &library {
            let id = card_entity_id(card.id);
            assert!(
                !opponent_json.contains(&id),
                "a searched library card leaked into another seat's view",
            );
            assert!(
                !spectator.contains(&id),
                "a searched library card leaked to a spectator",
            );
        }
    }

    #[test]
    fn issue_604_an_under_fillable_choice_states_its_lower_bound() {
        // A scry may legally bottom nothing, so the prompt carries `min` as well as the
        // maximum — the fact a client needs in order not to block a legal answer.
        let db = CardDatabase::bundled().unwrap();
        let mut state = main_phase();
        state.players[0].library = ["forest", "island", "swamp"]
            .iter()
            .map(|slug| state.new_instance(fixture(slug)))
            .collect();
        let state = cast_and_resolve(&state, &db, "omenspeaker", Vec::new());
        // The ETB trigger targets nothing, so a single pass round resolves it.
        let state = sage_engine::apply_action(&state, &Action::PassPriority, &db);
        let state = sage_engine::apply_action(&state, &Action::PassPriority, &db);

        let view = personalized_view(&state, &db, PlayerId(0));
        let action = choice_action(&view).expect("the scry is owed");
        let Prompt::SelectFromZone {
            count,
            min,
            zone,
            candidates,
            ..
        } = &action.prompts[0]
        else {
            panic!("the choice is a select_from_zone");
        };
        assert_eq!((*count, *min), (2, Some(0)), "any number of the top two");
        assert_eq!(zone, "library");
        assert_eq!(candidates.len(), 2);

        // The looked-at cards are shown to the scrying seat, and to that seat only.
        assert_eq!(view.revealed.len(), 2);
        assert!(personalized_view(&state, &db, PlayerId(1))
            .revealed
            .is_empty());

        // Bottoming nothing is a legal, resolvable answer.
        let decline = ChooseAction {
            action_id: action.id.clone(),
            token: action.token.clone(),
            targets: Vec::new(),
            ..Default::default()
        };
        assert_eq!(
            resolve_action(&state, &db, PlayerId(0), &decline),
            Some(Action::AnswerChoice { chosen: Vec::new() }),
        );
    }

    /// A yes-or-no's `option` prompt, or a panic naming what was found instead.
    fn option_prompt(action: &ValidAction) -> (&str, Vec<&str>) {
        let Prompt::Option {
            prompt, options, ..
        } = &action.prompts[0]
        else {
            panic!("the yes-or-no is an option prompt");
        };
        (prompt, options.iter().map(|o| o.id.as_str()).collect())
    }

    #[test]
    fn issue_610_an_optional_cost_projects_as_an_option_whose_accept_follows_the_pool() {
        // No bundled card poses this yet, so the card is written here. What is under
        // test is the projection: the question reuses the `option` prompt the mulligan
        // already rides on, its accepting choice appears exactly when the engine would
        // accept it, and the mana ability that makes it payable is offered alongside.
        let db = CardDatabase::from_json(
            r#"[
                {"schema_version":1,"functional_id":"test_offer","name":"Test Offer",
                 "types":["sorcery"],"mana_cost":"",
                 "spell_effects":[{"kind":"may","cost":"{G}",
                                   "effects":[{"kind":"draw_card","count":1}]}]},
                {"schema_version":1,"functional_id":"test_wood","name":"Test Wood",
                 "types":["land"],"mana_cost":"",
                 "abilities":[{"type":"activated","cost":[{"kind":"tap"}],
                               "effects":[{"kind":"add_mana","color":"green","amount":1}]}]}
            ]"#,
        )
        .unwrap();
        let card = |slug: &str| {
            db.card_id(&sage_engine::FunctionalId::try_from(slug.to_string()).unwrap())
                .unwrap()
        };

        // An untapped land and an empty pool: the cost is payable in principle, which is
        // why the question is posed at all, and not payable yet.
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        state.priority = PlayerId(0);
        state.consecutive_passes = 0;
        let land = PermanentId(state.mint_id());
        let instance = state.new_instance(card("test_wood"));
        state.battlefield.push(sage_engine::Permanent {
            id: land,
            instance: instance.id,
            printed: card("test_wood").into(),
            controller: PlayerId(0),
            ..Default::default()
        });
        state.players[0].library = vec![state.new_instance(card("test_offer"))];
        let spell = state.new_instance(card("test_offer"));
        state.players[0].hand.push(spell);
        let state = sage_engine::apply_action(
            &state,
            &Action::CastSpell {
                card: spell,
                mode: None,
                x: None,
                targets: Vec::new(),
                payment: Vec::new(),
            },
            &db,
        );
        let state = sage_engine::apply_action(&state, &Action::PassPriority, &db);
        let state = sage_engine::apply_action(&state, &Action::PassPriority, &db);
        assert!(pending_player_choice(&state).is_some(), "the offer is owed");

        let view = personalized_view(&state, &db, PlayerId(0));
        let action = choice_action(&view).expect("the caster is asked");
        assert_eq!(action.label, "Pay {G} to draw a card?");
        assert_eq!(
            option_prompt(action),
            ("Pay {G} to draw a card?", vec![DECLINE_OPTION]),
            "with nothing floating there is only one answer to give",
        );
        assert!(
            view.revealed.is_empty(),
            "a yes-or-no shows nobody any cards",
        );
        assert!(
            view.valid_actions
                .iter()
                .any(|a| a.kind == "activate_ability" && a.mana_ability),
            "the mana ability that would pay for it is offered alongside (CR 605.3a)",
        );
        assert!(
            personalized_view(&state, &db, PlayerId(1))
                .valid_actions
                .is_empty(),
            "and no other seat may act",
        );

        // Declining round-trips; an acceptance the offer did not list is refused rather
        // than quietly read as one.
        let answer = |action: &ValidAction, chosen: &str| ChooseAction {
            action_id: action.id.clone(),
            token: action.token.clone(),
            targets: vec![TargetChoice {
                slot: CHOICE_SLOT.to_string(),
                chosen: vec![chosen.to_string()],
            }],
            ..Default::default()
        };
        assert_eq!(
            resolve_action(&state, &db, PlayerId(0), &answer(action, DECLINE_OPTION)),
            Some(Action::AnswerConfirm { accept: false }),
        );
        assert_eq!(
            resolve_action(&state, &db, PlayerId(0), &answer(action, ACCEPT_OPTION)),
            None,
            "an unpayable acceptance is not on offer, so it is not bound",
        );

        // Float the mana, and the acceptance appears — same question, same slot.
        let floated = sage_engine::apply_action(
            &state,
            &Action::ActivateAbility {
                permanent: land,
                index: 0,
                targets: Vec::new(),
                payment: Vec::new(),
            },
            &db,
        );
        let view = personalized_view(&floated, &db, PlayerId(0));
        let action = choice_action(&view).expect("still asked");
        assert_eq!(
            option_prompt(action).1,
            vec![ACCEPT_OPTION, DECLINE_OPTION],
            "with the mana in the pool, paying is one of the answers",
        );
        assert_eq!(
            resolve_action(&floated, &db, PlayerId(0), &answer(action, ACCEPT_OPTION)),
            Some(Action::AnswerConfirm { accept: true }),
        );
    }

    #[test]
    fn issue_738_the_colour_a_permanent_enters_with_is_asked_by_name() {
        // Diamond Mare's colour is named as it enters (CR 614.12), so the question is
        // owed while the permanent is still on the battlefield's doorstep. Five answers,
        // always — and the sentence names the card, because five identical buttons are
        // all a player has to go on when two things are entering at once.
        let db = CardDatabase::bundled().unwrap();
        let state = cast_and_resolve(&main_phase(), &db, "diamond_mare", Vec::new());

        let view = personalized_view(&state, &db, PlayerId(0));
        let action = choice_action(&view).expect("its controller is asked");
        let (prompt, options) = option_prompt(action);
        assert_eq!(
            prompt,
            "Choose a color as Diamond Mare enters the battlefield"
        );
        assert_eq!(options, vec!["white", "blue", "black", "red", "green"]);
        assert_eq!(
            action.label, prompt,
            "the dock button says what it is about to ask"
        );
        let answer = ChooseAction {
            action_id: action.id.clone(),
            token: action.token.clone(),
            targets: vec![TargetChoice {
                slot: CHOICE_SLOT.to_string(),
                chosen: vec!["red".to_string()],
            }],
            ..Default::default()
        };
        assert_eq!(
            resolve_action(&state, &db, PlayerId(0), &answer),
            Some(Action::AnswerColor { color: Color::Red }),
        );

        // The opponent is offered nothing: the game is frozen on someone else's answer.
        assert!(choice_action(&personalized_view(&state, &db, PlayerId(1))).is_none());
    }

    #[test]
    fn issue_731_the_replacement_ordering_projects_as_an_option_whose_id_is_the_position() {
        // CR 616.1 reaches the client as the `option` prompt it already knows: one entry
        // per applicable replacement, labelled with what it would do, and answered with
        // the position the engine indexes into. No new wire shape, and no seat but the
        // affected permanent's controller is asked.
        let db = CardDatabase::from_json(
            r#"[
            {"schema_version":1,"functional_id":"test_ward","name":"Test Ward",
             "types":["instant"],"mana_cost":"{U}","colors":["blue"],
             "spell_effects":[{"kind":"create_replacement","replacement":{
               "kind":"exile_entering",
               "entering":{"card_type":"creature","nontoken":true,"not_cast":true}}}]},
            {"schema_version":1,"functional_id":"test_wraith","name":"Test Wraith",
             "types":["creature"],"subtypes":["Spirit"],"mana_cost":"{1}{B}","colors":["black"],
             "power":1,"toughness":1,
             "abilities":[
               {"type":"enters_tapped"},
               {"type":"activated","cost":[{"kind":"mana","mana":"{1}"}],
                "effects":[{"kind":"return_self_from_graveyard","destination":"battlefield"}]}]}
        ]"#,
        )
        .unwrap();
        let ward = db
            .card_id(&sage_engine::FunctionalId::try_from("test_ward".to_string()).unwrap())
            .unwrap();
        let wraith = db
            .card_id(&sage_engine::FunctionalId::try_from("test_wraith".to_string()).unwrap())
            .unwrap();

        // Seat 1 arms the replacement; seat 0 brings a wraith back, which is both a
        // creature entering without being cast and a creature that enters tapped.
        let mut state = main_phase();
        state.priority = PlayerId(1);
        let ward_card = state.new_instance(ward);
        state.players[1].hand.push(ward_card);
        let state = sage_engine::apply_action(
            &state,
            &Action::CastSpell {
                card: ward_card,
                mode: None,
                x: None,
                targets: Vec::new(),
                payment: Vec::new(),
            },
            &db,
        );
        let state = sage_engine::apply_action(&state, &Action::PassPriority, &db);
        let mut state = sage_engine::apply_action(&state, &Action::PassPriority, &db);
        assert_eq!(state.replacements.len(), 1);

        let wraith_card = state.new_instance(wraith);
        state.players[0].graveyard.push(wraith_card);
        state.priority = PlayerId(0);
        state.consecutive_passes = 0;
        let state = sage_engine::apply_action(
            &state,
            &Action::ActivateAbilityFromGraveyard {
                card: wraith_card,
                index: 1,
                targets: Vec::new(),
            },
            &db,
        );
        let state = sage_engine::apply_action(&state, &Action::PassPriority, &db);
        let state = sage_engine::apply_action(&state, &Action::PassPriority, &db);

        let view = personalized_view(&state, &db, PlayerId(0));
        let action = choice_action(&view).expect("the affected controller is asked");
        let (prompt, options) = option_prompt(action);
        assert_eq!(prompt, "Choose which replacement effect applies first");
        assert_eq!(options, vec!["0", "1"], "the option id is the position");
        let Prompt::Option { options, .. } = &action.prompts[0] else {
            panic!("an option prompt");
        };
        let labels: Vec<&str> = options.iter().map(|o| o.label.as_str()).collect();
        assert_eq!(labels, vec!["It enters tapped", "Exile it instead"]);

        let answer = ChooseAction {
            action_id: action.id.clone(),
            token: action.token.clone(),
            targets: vec![TargetChoice {
                slot: CHOICE_SLOT.to_string(),
                chosen: vec!["1".to_string()],
            }],
            ..Default::default()
        };
        assert_eq!(
            resolve_action(&state, &db, PlayerId(0), &answer),
            Some(Action::AnswerReplacement { index: 1 }),
        );
        // The seat whose ability created the replacement is not the one asked.
        assert!(choice_action(&personalized_view(&state, &db, PlayerId(1))).is_none());
    }
}
