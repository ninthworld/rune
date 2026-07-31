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

/// The slot id of a mid-resolution player choice's card pick (issue #604).
pub(crate) const CHOICE_SLOT: &str = "choice";

/// The single [`Prompt::SelectFromZone`] slot for the mid-resolution player choice the
/// game is waiting on (issue #604), or an empty list when it is waiting on none.
///
/// The engine owns everything here: which cards are pickable
/// ([`choice_candidates`](sage_engine::choice_candidates)) and how many of them a legal
/// answer names ([`choice_bounds`](sage_engine::choice_bounds), already clamped to what
/// the zone actually holds). This projects both and adds words — it derives no legality
/// of its own, and a client picking from this list cannot construct an answer the engine
/// has not already sanctioned.
///
/// `min` rides the wire only when it differs from the maximum, which is exactly when the
/// choice is one a player may legally under-fill: scrying *any number*, taking *up to*
/// one, or failing to find (CR 701.19c). A discard is exact and elides it.
pub(crate) fn player_choice_prompts(state: &GameState, db: &CardDatabase) -> Vec<Prompt> {
    let Some(pending) = pending_player_choice(state) else {
        return Vec::new();
    };
    let request = &pending.request;
    let (min, max) = choice_bounds(state, request, db);
    let candidates: Vec<String> = choice_candidates(state, request, db)
        .into_iter()
        .map(|inst| card_entity_id(inst.id))
        .collect();
    vec![Prompt::SelectFromZone {
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
    }]
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
    match pending_player_choice(state) {
        Some(pending) => {
            let (min, max) = choice_bounds(state, &pending.request, db);
            choice_prompt_text(state, &pending.request, min, max)
        }
        None => "Make a choice".to_string(),
    }
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
    let pending = pending_player_choice(state)?;
    let available = choice_candidates(state, &pending.request, db);
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
    choice_candidates(state, &pending.request, db)
        .into_iter()
        .map(|inst| card_view(card_entity_id(inst.id), inst.card, db))
        .collect()
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
                targets,
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
            card: fixture("elvish_clancaller"),
            controller: PlayerId(0),
            tapped: false,
            entered_turn: 0,
            attacking: None,
            blocking: None,
            damage: 0,
            counters: std::collections::BTreeMap::new(),
            attached_to: None,
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
}
