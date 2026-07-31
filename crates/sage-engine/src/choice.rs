//! Mid-resolution player choices: "choose N cards from this set".
//!
//! Every effect before this one resolved without asking anyone anything. A discard, a
//! scry, a look-at-the-top, and a library search all stop in the middle of resolution
//! and hand one named player a decision, and the game must not proceed until they make
//! it. This module is that mechanism, built to the shape the trigger-target choice of
//! issue #602 established:
//!
//! - **The pending choice is derived, never flagged.** A [`PendingChoice`] is queued
//!   state exactly as a [`StackObject`](crate::StackObject) is queued state; whether a
//!   choice is *currently owed* is [`pending_player_choice`], a pure read of the queue
//!   head. Nothing sets a "waiting" bit that could get out of step with the queue.
//! - **The candidates are derived too.** [`choice_candidates`] recomputes the pickable
//!   cards from current state on every call, so an answer is validated against the set
//!   that exists *now*, never against a list snapshotted when the choice was posed. It
//!   can afford to: while a choice is owed nothing else in the game is legal, so the
//!   zone it reads cannot move underneath it.
//! - **Priority goes to the chooser**, who is frequently not the player whose action
//!   caused the choice — a Mind Rot resolving on its caster's turn asks the *other*
//!   seat — and returns to the interrupted holder once the last choice is answered
//!   ([`crate::apply_action`]).
//! - **A choice with no legal answer is never posed.** It is applied immediately with
//!   an empty selection instead, so an empty hand, an empty library, or a look that
//!   turns up nothing matching resolves rather than stalling every seat.
//!
//! ## Hidden information
//!
//! This is the first place the engine shows a player cards no one else may see. The
//! engine itself holds every zone in the clear; the discipline is on the projection
//! ([`crate::PendingChoice::chooser`] is the *only* seat a searched library or a
//! revealed hand may be shown to) and on the log, which records counts and never card
//! identities. See `docs/decisions/0013-mid-resolution-player-choices.md`.

use crate::ability::{CardFilter, Effect, FoundDestination, Target};
use crate::card_type::CardType;
use crate::id::{CardId, CardInstance, CardInstanceId, PermanentId, PlayerId};
use crate::rng::SplitMix64;
use crate::state::{GameEvent, GameState};
use crate::CardDatabase;

/// One player choice an effect has posed and the game is waiting on.
///
/// Queued on [`GameState::pending_choices`] in the order the choices were posed and
/// answered from the front. The whole point of the type is the pair it carries: the
/// *question* ([`request`](Self::request)), which is all the view projection needs, and
/// the *rest of the resolution* ([`resume`](Self::resume)), which is what makes
/// suspending an effect mid-way recoverable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingChoice {
    /// The player who answers. Not necessarily the priority holder, the effect's
    /// controller, or the owner of the zone being chosen from: a coercive discard has
    /// the *controller* choose from the *opponent's* hand. This seat, and no other, may
    /// be shown the cards ([`choice_candidates`]).
    pub chooser: PlayerId,
    /// What is being chosen.
    pub request: ChoiceRequest,
    /// The remainder of the suspended object's resolution, carried on the **last**
    /// choice a single effect posed so the rest of the card happens exactly once. A
    /// choice that is not the last of its effect carries `None`.
    pub resume: Option<Resume>,
}

/// The question one [`PendingChoice`] asks: which cards, from where, how many, and what
/// becomes of them.
///
/// Deliberately free of any *answer* and of any snapshotted candidate list — it names a
/// zone and a class, and [`choice_candidates`] evaluates that against current state, in
/// the same relationship a [`TargetSpec`](crate::TargetSpec) has to
/// [`legal_targets_for_spec`](crate::target_requirements).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChoiceRequest {
    /// The player whose zone is being chosen from. Equal to
    /// [`PendingChoice::chooser`] for every choice except a coercive discard.
    pub subject: PlayerId,
    /// Which of the subject's zones, and how much of it, is on offer.
    pub zone: ChoiceZone,
    /// Which cards of that zone may be picked.
    pub filter: CardFilter,
    /// The printed card [`CardFilter::SameNameAsSource`] compares against — resolved
    /// when the choice was posed, because the source permanent may have left the
    /// battlefield by the time the choice is answered. `None` for a spell (no source
    /// permanent) or a filter that does not need one.
    pub source_card: Option<CardId>,
    /// The fewest cards a legal answer may name, before clamping to what is actually
    /// available ([`choice_bounds`]).
    pub min: u32,
    /// The most cards a legal answer may name, before that same clamping.
    pub max: u32,
    /// What happens to the chosen cards — and to the ones passed over.
    pub outcome: ChoiceOutcome,
}

/// Which cards of a player's zone a [`ChoiceRequest`] draws from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChoiceZone {
    /// The subject's hand.
    Hand,
    /// The top N cards of the subject's library, in top-first order — the "look at the
    /// top four" of a scry or a look-and-take. Fewer when the library is shorter.
    LibraryTop(u8),
    /// The subject's whole library — a search (CR 701.19).
    Library,
}

/// What becomes of the chosen cards once a [`PendingChoice`] is answered, and of the
/// cards it looked at but did not choose.
///
/// The aftermath belongs to the *request* rather than to the answer because it happens
/// whether or not anything was chosen: a search that finds nothing still shuffles
/// (CR 701.19c), and a look that takes nothing still bottoms what it looked at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChoiceOutcome {
    /// The chosen cards go from the subject's hand to their graveyard (CR 701.8).
    Discard,
    /// The chosen cards go to the bottom of the subject's library in the order they
    /// were chosen (the first chosen ends up deepest); every looked-at card not chosen
    /// stays on top in the order it was already in. This is scry (CR 701.17).
    BottomChosen,
    /// The chosen cards go to `destination`; every **other looked-at** card goes to the
    /// bottom of the library in a random order drawn from the seeded RNG.
    TakeAndBottomRest(FoundDestination),
    /// The chosen cards go to `destination`; the subject's library is then shuffled
    /// (CR 701.19c). This is a search.
    TakeAndShuffle(FoundDestination),
}

/// The rest of a suspended object's resolution, held until the choice that interrupted
/// it is answered.
///
/// Everything [`crate::resolve`] would have gone on to do: the effects it had not
/// reached yet, the stored targets those effects still owe, and — for a spell — the
/// card that must still leave the stack for its final zone (CR 608.3). Without that
/// last part a Tormenting Voice would discard, draw, and then never reach the
/// graveyard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resume {
    /// The suspended object's controller, the frame of reference every remaining
    /// effect is applied in.
    pub controller: PlayerId,
    /// The ability's source permanent, for a self-referential remaining effect.
    /// `None` for a spell.
    pub source: Option<PermanentId>,
    /// The effects not yet applied, in order.
    pub effects: Vec<Effect>,
    /// The stored targets those effects have not yet consumed, in slot order.
    pub targets: Vec<Target>,
    /// The spell whose card still has to reach its final zone, `None` for an ability.
    pub spell: Option<SuspendedSpell>,
}

/// A spell whose resolution was suspended, and which must still be put into its final
/// zone once the suspended effects finish (CR 608.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuspendedSpell {
    /// The physical card on the stack.
    pub card: CardInstance,
    /// The spell's chosen targets, in slot order — read by the Aura attachment path.
    pub targets: Vec<Target>,
}

/// The choice the game is currently waiting on, or `None` when it is waiting on none.
///
/// **Derived, never stored.** Choices are answered in the order they were posed, so
/// this is the head of the queue and nothing else; there is no "a choice is pending"
/// flag to set, clear, or forget. While this is `Some`, [`crate::valid_actions`] offers
/// the chooser exactly one thing and every other seat nothing, which is what makes
/// "the game does not proceed until it is answered" true by construction rather than by
/// convention.
#[must_use]
pub fn pending_player_choice(state: &GameState) -> Option<&PendingChoice> {
    state.pending_choices.first()
}

/// The cards `request` currently offers, in the order the chooser sees them.
///
/// Recomputed from `state` on every call — the counterpart of
/// [`legal_targets_for_spec`](crate::target_requirements) for a card in a hidden zone.
/// Empty for a request whose zone is gone (an eliminated seat) or whose filter nothing
/// matches, which is exactly the "no legal answer" case
/// [`ChoiceOutcome`] resolves without stalling.
///
/// **A library's cards are visible only to [`PendingChoice::chooser`].** This function
/// happily returns them for any caller; keeping them out of another seat's view is the
/// projection's job.
#[must_use]
pub fn choice_candidates(
    state: &GameState,
    request: &ChoiceRequest,
    db: &CardDatabase,
) -> Vec<CardInstance> {
    let Some(player) = state.players.get(request.subject.0) else {
        return Vec::new();
    };
    let pool: Vec<CardInstance> = match request.zone {
        ChoiceZone::Hand => player.hand.clone(),
        // The top of a library is its last element, so the top N read top-first is the
        // tail reversed.
        ChoiceZone::LibraryTop(count) => player
            .library
            .iter()
            .rev()
            .take(count as usize)
            .copied()
            .collect(),
        ChoiceZone::Library => player.library.clone(),
    };
    pool.into_iter()
        .filter(|inst| card_matches(db, inst.card, &request.filter, request.source_card))
        .collect()
}

/// How many cards a legal answer to `request` must name, clamped to what is actually
/// there: `(min, max)`, inclusive.
///
/// Clamping is what makes "discard two cards" mean "discard the one card you have"
/// rather than deadlock, and what makes a `max` of `0` the single, uniform signal that
/// a choice has no legal answer and must not be posed.
#[must_use]
pub fn choice_bounds(state: &GameState, request: &ChoiceRequest, db: &CardDatabase) -> (u32, u32) {
    let available = u32::try_from(choice_candidates(state, request, db).len()).unwrap_or(u32::MAX);
    (request.min.min(available), request.max.min(available))
}

/// Whether `chosen` is a legal answer to the choice currently owed (CR-style
/// regenerate-and-check): the right number of distinct cards, every one of them in the
/// freshly recomputed candidate set.
///
/// The same discipline [`action_is_legal`](crate::apply_action) applies to targets — a
/// stale or forged card id can never survive, because membership is tested against the
/// set that exists now rather than the one the client was shown.
#[must_use]
pub(crate) fn answer_is_legal(
    state: &GameState,
    chosen: &[CardInstanceId],
    db: &CardDatabase,
) -> bool {
    let Some(pending) = pending_player_choice(state) else {
        return false;
    };
    let (min, max) = choice_bounds(state, &pending.request, db);
    let count = u32::try_from(chosen.len()).unwrap_or(u32::MAX);
    if count < min || count > max {
        return false;
    }
    let candidates = choice_candidates(state, &pending.request, db);
    // No card may be named twice: a hand of one card is not two discards.
    let distinct = chosen
        .iter()
        .enumerate()
        .all(|(index, id)| !chosen[..index].contains(id));
    distinct
        && chosen
            .iter()
            .all(|id| candidates.iter().any(|inst| inst.id == *id))
}

/// Whether the printed card `card` satisfies `filter`.
///
/// Reads **printed** characteristics only. A card in a library or a hand is not on the
/// battlefield, so it has no computed power and no continuous effects applying to it;
/// asking [`characteristics`](crate::characteristics::characteristics) about it would be
/// asking about a permanent that does not exist.
fn card_matches(
    db: &CardDatabase,
    card: CardId,
    filter: &CardFilter,
    source_card: Option<CardId>,
) -> bool {
    let Some(data) = db.card(card) else {
        return false;
    };
    match filter {
        CardFilter::Any => true,
        CardFilter::Land => data.has_type(CardType::Land),
        CardFilter::Creature { max_power } => {
            data.has_type(CardType::Creature)
                && match max_power {
                    Some(cap) => data.power.is_some_and(|power| power <= *cap),
                    None => true,
                }
        }
        CardFilter::NoncreatureNonland => {
            !data.has_type(CardType::Creature) && !data.has_type(CardType::Land)
        }
        // Same printed identity, not same name string: two copies of one printing share
        // a `CardId`, and nothing else does.
        CardFilter::SameNameAsSource => source_card == Some(card),
    }
}

/// Pose `choices`, or apply the ones that have no legal answer outright.
///
/// Returns whether anything was actually queued — i.e. whether the caller must suspend.
/// A choice whose clamped maximum is zero is not a decision at all, so it is applied
/// immediately with an empty selection (which still shuffles a searched library and
/// still bottoms a looked-at pile) and the caller carries on. This is the whole of the
/// "a choice with no legal answer resolves without stalling" guarantee, in one place
/// rather than per effect.
pub(crate) fn pose_choices(
    state: &mut GameState,
    choices: Vec<(PlayerId, ChoiceRequest)>,
    db: &CardDatabase,
) -> bool {
    let mut queued = false;
    for (chooser, request) in choices {
        let (_, max) = choice_bounds(state, &request, db);
        if max == 0 {
            apply_choice_outcome(state, &request, &[], db);
            continue;
        }
        state.pending_choices.push(PendingChoice {
            chooser,
            request,
            resume: None,
        });
        queued = true;
    }
    queued
}

/// Attach `resume` to the most recently queued choice, so the rest of the suspended
/// object's resolution happens once, after the *last* choice its effect posed.
///
/// Called only when [`pose_choices`] queued something, so the queue is non-empty.
pub(crate) fn attach_resume(state: &mut GameState, resume: Resume) {
    if let Some(last) = state.pending_choices.last_mut() {
        last.resume = Some(resume);
    }
}

/// Carry out `request` for the chosen cards (and, where the outcome says so, for the
/// ones passed over).
///
/// The caller has already established that `chosen` is a legal answer; this moves cards
/// and records the log event, and decides nothing.
pub(crate) fn apply_choice_outcome(
    state: &mut GameState,
    request: &ChoiceRequest,
    chosen: &[CardInstanceId],
    db: &CardDatabase,
) {
    match request.outcome {
        ChoiceOutcome::Discard => discard_chosen(state, request.subject, chosen),
        ChoiceOutcome::BottomChosen => bottom_chosen(state, request.subject, chosen),
        ChoiceOutcome::TakeAndBottomRest(destination) => {
            take_and_bottom_rest(state, request, chosen, destination, db);
        }
        ChoiceOutcome::TakeAndShuffle(destination) => {
            take_and_shuffle(state, request, chosen, destination, db);
        }
    }
}

/// Move the chosen cards from the subject's hand to their graveyard (CR 701.8) and log
/// how many moved — never which, since a hand is hidden and a count is all the other
/// seats are entitled to. (The graveyard itself is public, so the cards become visible
/// there on their own.)
fn discard_chosen(state: &mut GameState, subject: PlayerId, chosen: &[CardInstanceId]) {
    let mut discarded = 0u32;
    for id in chosen {
        let Some(player) = state.players.get_mut(subject.0) else {
            break;
        };
        let Some(pos) = player.hand.iter().position(|card| card.id == *id) else {
            continue;
        };
        let card = player.hand.remove(pos);
        player.graveyard.push(card);
        discarded += 1;
    }
    if discarded > 0 {
        state.record_event(GameEvent::CardsDiscarded {
            player: subject,
            count: discarded,
        });
    }
}

/// Put the chosen cards on the bottom of the subject's library in the order they were
/// chosen — scry's half that moves anything (CR 701.17). The looked-at cards left
/// unchosen are already on top in their existing order, so they need no work.
fn bottom_chosen(state: &mut GameState, subject: PlayerId, chosen: &[CardInstanceId]) {
    let Some(player) = state.players.get_mut(subject.0) else {
        return;
    };
    let mut moved = Vec::with_capacity(chosen.len());
    for id in chosen {
        if let Some(pos) = player.library.iter().position(|card| card.id == *id) {
            moved.push(player.library.remove(pos));
        }
    }
    // The bottom of the library is index 0, and the first card chosen ends up deepest.
    moved.reverse();
    for card in moved {
        player.library.insert(0, card);
    }
}

/// Take the chosen cards to `destination` and bottom every *other* card this choice
/// looked at, in a random order drawn from the seeded RNG.
///
/// The looked-at set is the request's own window onto the library
/// ([`ChoiceZone::LibraryTop`]) rather than the filtered candidate list: a look at the
/// top four bottoms all four, not only the ones that could have been taken.
fn take_and_bottom_rest(
    state: &mut GameState,
    request: &ChoiceRequest,
    chosen: &[CardInstanceId],
    destination: FoundDestination,
    db: &CardDatabase,
) {
    let ChoiceZone::LibraryTop(count) = request.zone else {
        return;
    };
    let looked_at: Vec<CardInstance> = state
        .players
        .get(request.subject.0)
        .map(|player| {
            player
                .library
                .iter()
                .rev()
                .take(count as usize)
                .copied()
                .collect()
        })
        .unwrap_or_default();
    // Every looked-at card leaves the library first, so nothing below can find a card
    // in two places at once; each is then put where the answer says.
    if let Some(player) = state.players.get_mut(request.subject.0) {
        player
            .library
            .retain(|card| !looked_at.iter().any(|seen| seen.id == card.id));
    }
    let (taken, mut rest): (Vec<CardInstance>, Vec<CardInstance>) = looked_at
        .into_iter()
        .partition(|card| chosen.contains(&card.id));
    for card in taken {
        place_card(state, request.subject, card, destination, db);
    }
    // "in a random order" — the seeded stream, so the same seed replays the same
    // bottom order and the chooser learns nothing about their future draws.
    let mut rng = SplitMix64::new(state.rng_seed);
    rng.shuffle(&mut rest);
    state.rng_seed = rng.state();
    if let Some(player) = state.players.get_mut(request.subject.0) {
        for card in rest.into_iter().rev() {
            player.library.insert(0, card);
        }
    }
}

/// Take the chosen cards to `destination` and shuffle the searched library
/// (CR 701.19c) — including when nothing was found, since the shuffle is what stops a
/// failed search from being a free look at the deck.
fn take_and_shuffle(
    state: &mut GameState,
    request: &ChoiceRequest,
    chosen: &[CardInstanceId],
    destination: FoundDestination,
    db: &CardDatabase,
) {
    for id in chosen {
        let Some(player) = state.players.get_mut(request.subject.0) else {
            break;
        };
        let Some(pos) = player.library.iter().position(|card| card.id == *id) else {
            continue;
        };
        let card = player.library.remove(pos);
        place_card(state, request.subject, card, destination, db);
    }
    let mut library = state
        .players
        .get(request.subject.0)
        .map(|player| player.library.clone())
        .unwrap_or_default();
    let mut rng = SplitMix64::new(state.rng_seed);
    rng.shuffle(&mut library);
    state.rng_seed = rng.state();
    if let Some(player) = state.players.get_mut(request.subject.0) {
        player.library = library;
    }
    state.record_event(GameEvent::LibrarySearched {
        player: request.subject,
    });
}

/// Put one card that has already been removed from its zone into `destination`.
///
/// A card headed for the battlefield goes through the one battlefield-entry seam
/// ([`GameState::put_card_onto_battlefield`]), so it mints a fresh
/// [`PermanentId`](crate::PermanentId), applies its own CR 614 enters-the-battlefield
/// replacements, and is picked up by the trigger diff exactly as a resolving permanent
/// spell is.
fn place_card(
    state: &mut GameState,
    subject: PlayerId,
    card: CardInstance,
    destination: FoundDestination,
    db: &CardDatabase,
) {
    match destination {
        FoundDestination::Hand => {
            if let Some(player) = state.players.get_mut(subject.0) {
                player.hand.push(card);
            }
        }
        FoundDestination::Battlefield => {
            state.put_card_onto_battlefield(card, subject, false, None, db);
        }
        FoundDestination::BattlefieldTapped => {
            state.put_card_onto_battlefield(card, subject, true, None, db);
        }
    }
}

/// The choices `effect` poses, if it poses any: `(chooser, request)` pairs in the order
/// they must be answered. `None` for every effect that resolves without asking.
///
/// `target` is the effect's chosen target when it has one, so a "target player
/// discards" reaches the seat the caster aimed at rather than a seat derived here.
pub(crate) fn choices_for_effect(
    state: &GameState,
    effect: &Effect,
    controller: PlayerId,
    source_card: Option<CardId>,
    target: Option<Target>,
) -> Option<Vec<(PlayerId, ChoiceRequest)>> {
    match effect {
        Effect::Discard {
            player_ref,
            count,
            chosen_by,
            filter,
        } => {
            let subjects = match target {
                // A targeting reference names the one seat that was aimed at.
                Some(Target::Player(seat)) => vec![seat],
                _ => crate::apply::non_targeting_subjects(state, *player_ref, controller),
            };
            Some(
                subjects
                    .into_iter()
                    .map(|subject| {
                        let chooser = match chosen_by {
                            crate::ability::Chooser::Owner => subject,
                            crate::ability::Chooser::Controller => controller,
                        };
                        (
                            chooser,
                            ChoiceRequest {
                                subject,
                                zone: ChoiceZone::Hand,
                                filter: filter.clone(),
                                source_card,
                                min: u32::from(*count),
                                max: u32::from(*count),
                                outcome: ChoiceOutcome::Discard,
                            },
                        )
                    })
                    .collect(),
            )
        }
        Effect::Scry { count } => Some(vec![(
            controller,
            ChoiceRequest {
                subject: controller,
                zone: ChoiceZone::LibraryTop(*count),
                filter: CardFilter::Any,
                source_card,
                // Any number, including none (CR 701.17).
                min: 0,
                max: u32::from(*count),
                outcome: ChoiceOutcome::BottomChosen,
            },
        )]),
        Effect::LookAtTop {
            count,
            take,
            filter,
            destination,
        } => Some(vec![(
            controller,
            ChoiceRequest {
                subject: controller,
                zone: ChoiceZone::LibraryTop(*count),
                filter: filter.clone(),
                source_card,
                // Taking is optional ("you may reveal…"), so nothing forces a pick.
                min: 0,
                max: u32::from(*take),
                outcome: ChoiceOutcome::TakeAndBottomRest(*destination),
            },
        )]),
        Effect::SearchLibrary {
            take,
            filter,
            destination,
        } => Some(vec![(
            controller,
            ChoiceRequest {
                subject: controller,
                zone: ChoiceZone::Library,
                filter: filter.clone(),
                source_card,
                // A player may always fail to find (CR 701.19c).
                min: 0,
                max: u32::from(*take),
                outcome: ChoiceOutcome::TakeAndShuffle(*destination),
            },
        )]),
        _ => None,
    }
}
