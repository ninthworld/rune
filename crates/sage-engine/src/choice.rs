//! Mid-resolution player choices: "choose N cards from this set", "do you want this to
//! happen?", and "which color of mana?".
//!
//! Every effect before this one resolved without asking anyone anything. A discard, a
//! scry, a look-at-the-top, a library search, and an optional `you may …` all stop in
//! the middle of resolution and hand one named player a decision, and the game must not
//! proceed until they make it. This module is that mechanism, built to the shape the
//! trigger-target choice of issue #602 established:
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
//!   turns up nothing matching resolves rather than stalling every seat. An optional
//!   cost no amount of tapping could pay is the same rule wearing different clothes:
//!   it is declined outright ([`ChoiceQuestion::Confirm`]).
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
use crate::mana::Color;
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
    /// What is being asked.
    pub question: ChoiceQuestion,
    /// The remainder of the suspended object's resolution, carried on the **last**
    /// choice a single effect posed so the rest of the card happens exactly once. A
    /// choice that is not the last of its effect carries `None`.
    pub resume: Option<Resume>,
}

/// What one [`PendingChoice`] asks — the shapes of question the engine can pose in the
/// middle of a resolution.
///
/// One enum rather than a queue per shape, because everything *around* the question is
/// the same for all of them: the queue, the routing to a chooser, the priority hand-off through
/// the shared [`interrupted_priority`](crate::GameState::interrupted_priority) slot, and
/// the rule that a question with no legal answer is never posed at all. Only the answer
/// differs, so only the answer's shape lives here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChoiceQuestion {
    /// Choose cards from a zone — a discard, a scry, a look, a search
    /// ([`ChoiceRequest`]). Answered with [`Action::AnswerChoice`](crate::Action).
    Cards(ChoiceRequest),
    /// Yes or no, optionally gated on paying a cost — the `you may …` of
    /// [`Effect::May`] ([`ConfirmRequest`]). Answered with
    /// [`Action::AnswerConfirm`](crate::Action).
    Confirm(ConfirmRequest),
    /// Which **color** of mana to add — one point's worth of `Add two mana in any
    /// combination of colors` ([`ColorRequest`]). Answered with
    /// [`Action::AnswerColor`](crate::Action).
    ///
    /// The third shape rather than a special case of the first two because the answer
    /// is neither a set of cards nor a yes-or-no: it names one of the five colors
    /// (CR 105.1), and every one of them is always a legal answer. An effect producing
    /// more than one mana poses one of these per point, so the player really is asked
    /// "and the second one?" rather than being made to spend all of it on one color.
    Color(ColorRequest),
}

impl ChoiceQuestion {
    /// The card-selection request this question asks, or `None` when it is not one.
    /// Lets a caller that only handles the selection shape — the candidate projection,
    /// the revealed-cards channel — say so once rather than matching every variant.
    #[must_use]
    pub fn cards(&self) -> Option<&ChoiceRequest> {
        match self {
            ChoiceQuestion::Cards(request) => Some(request),
            ChoiceQuestion::Confirm(_) | ChoiceQuestion::Color(_) => None,
        }
    }

    /// The yes-or-no request this question asks, or `None` when it is not one.
    #[must_use]
    pub fn confirm(&self) -> Option<&ConfirmRequest> {
        match self {
            ChoiceQuestion::Confirm(request) => Some(request),
            ChoiceQuestion::Cards(_) | ChoiceQuestion::Color(_) => None,
        }
    }

    /// The mana-color request this question asks, or `None` when it is not one.
    #[must_use]
    pub fn color(&self) -> Option<&ColorRequest> {
        match self {
            ChoiceQuestion::Color(request) => Some(request),
            ChoiceQuestion::Cards(_) | ChoiceQuestion::Confirm(_) => None,
        }
    }
}

/// The question one point of "mana in any combination of colors" asks: *which color?*
///
/// Carries nothing about *who* — the chooser is [`PendingChoice::chooser`], and the
/// mana goes into that same seat's pool, because an effect that adds mana adds it to
/// its controller's pool (CR 106.4) and the controller is who is asked.
///
/// It carries no candidate list either, for the reason [`ChoiceRequest`] carries none:
/// the answer set is the five colors (CR 105.1) and is the same at every table in every
/// game, so there is nothing to compute or to keep fresh.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColorRequest {
    /// What the produced mana may be spent on (CR 106.6), or `None` for ordinary mana.
    /// Copied from the effect so the restriction rides each point as it is chosen.
    pub restriction: Option<crate::ability::ManaRestriction>,
}

/// The question an optional effect asks: *do you want this, and will you pay for it?*
///
/// Carries what happens on a **yes** and nothing about what happens on a no, because a
/// no is the absence of an event: the effects here are simply not applied, and the rest
/// of the resolution — which rides on [`PendingChoice::resume`], not here — is
/// untouched either way.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfirmRequest {
    /// The mana cost accepting charges, in `{...}` notation, or `None` for a free
    /// `you may`. Paid from the chooser's pool at the moment they accept, through the
    /// same [`ManaPool::pay`](crate::ManaPool::pay) seam a cast uses.
    pub cost: Option<String>,
    /// The effects applied on acceptance, in order. They are spliced onto the front of
    /// the suspended remainder rather than applied here, so accepting resumes down
    /// exactly one code path and an accepted effect that poses a *further* choice
    /// suspends again without any special case.
    pub effects: Vec<Effect>,
}

/// A card-selection question ([`ChoiceQuestion::Cards`]): which cards, from where, how
/// many, and what becomes of them.
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
    /// The log sequence this resolution began at — the window an intervening condition
    /// about what *this* resolution has done reads over
    /// ([`Condition::MilledThisWay`](crate::Condition)).
    ///
    /// Carried through the suspension for the same reason the remaining effects are: a
    /// discard-then-draw asks its question, and the `if a card is discarded this way`
    /// that follows must still be measured from where the resolution started, not from
    /// where it woke up.
    pub resolution_start: u64,
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
        .filter(|inst| card_matches_filter(db, inst.card, &request.filter, request.source_card))
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
    // A yes-or-no is not answered with cards, so a card selection aimed at one is not
    // a wrong answer to it — it is an answer to a question nobody asked.
    let Some(request) = pending_player_choice(state).and_then(|p| p.question.cards()) else {
        return false;
    };
    let (min, max) = choice_bounds(state, request, db);
    let count = u32::try_from(chosen.len()).unwrap_or(u32::MAX);
    if count < min || count > max {
        return false;
    }
    let candidates = choice_candidates(state, request, db);
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

/// Whether the pending yes-or-no can currently be answered **yes**: it is a
/// [`ChoiceQuestion::Confirm`], and its cost (if it has one) is payable from the
/// chooser's mana pool as it stands right now.
///
/// The counterpart of [`choice_bounds`] for a confirmation — the fact a projection needs
/// in order to offer "yes" only when the engine would accept it, and the fact
/// [`crate::apply_action`]'s gate re-derives before charging anyone. `false` when no
/// choice is owed or the owed one is a card selection.
///
/// Read against the pool *now*, deliberately: a chooser owed a payment may still tap
/// lands (CR 605.3a), so this flips from `false` to `true` as they float mana, and the
/// offer follows.
#[must_use]
pub fn confirm_is_payable(state: &GameState) -> bool {
    let Some(pending) = pending_player_choice(state) else {
        return false;
    };
    let Some(request) = pending.question.confirm() else {
        return false;
    };
    cost_is_payable_from_pool(state, pending.chooser, request.cost.as_deref())
}

/// Whether `player`'s pool covers `cost` as it stands. `true` for a free choice
/// (`None`), and for a seat that has left the game there is no pool and so no payment.
fn cost_is_payable_from_pool(state: &GameState, player: PlayerId, cost: Option<&str>) -> bool {
    let Some(cost) = cost else {
        return true;
    };
    state
        .players
        .get(player.0)
        .is_some_and(|p| p.mana_pool.can_pay(&crate::mana::parse_mana_cost(cost)))
}

/// Whether `player` could pay `cost` if they tapped everything they have — their pool
/// plus every point of mana their untapped sources could still add.
///
/// This, not the current pool, is what decides whether an optional cost is *posed*: a
/// player with an empty pool and two untapped Forests can pay `{1}`, and auto-declining
/// them would take away a decision the rules give them. The estimate is the same
/// deliberate over-estimate [`crate::priority_has_no_meaningful_action`] makes — every
/// mana ability of every untapped source, as though one permanent could be tapped for
/// all of them — and errs in the same safe direction: it can only ever *offer* a choice
/// that turns out unpayable, which the chooser simply declines, never withhold one they
/// could have taken.
fn cost_could_be_paid(
    state: &GameState,
    player: PlayerId,
    cost: Option<&str>,
    db: &CardDatabase,
) -> bool {
    let Some(cost) = cost else {
        return true;
    };
    crate::actions::potential_mana_pool(state, player, db)
        .can_pay(&crate::mana::parse_mana_cost(cost))
}

/// Whether the printed card `card` satisfies `filter`.
///
/// Reads **printed** characteristics only. A card in a library or a hand is not on the
/// battlefield, so it has no computed power and no continuous effects applying to it;
/// asking [`characteristics`](crate::characteristics::characteristics) about it would be
/// asking about a permanent that does not exist.
pub(crate) fn card_matches_filter(
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
        CardFilter::Creature { max_power, subtype } => {
            data.has_type(CardType::Creature)
                && max_power.is_none_or(|cap| data.power.is_some_and(|power| power <= cap))
                && subtype
                    .as_deref()
                    .is_none_or(|wanted| data.has_subtype(wanted))
        }
        CardFilter::NoncreatureNonland => {
            !data.has_type(CardType::Creature) && !data.has_type(CardType::Land)
        }
        CardFilter::CreatureOrLand => {
            data.has_type(CardType::Creature) || data.has_type(CardType::Land)
        }
        // CR 110.1: a card that would enter the battlefield. The class a search that puts
        // its find straight onto the battlefield names, because nothing else could go
        // there.
        CardFilter::Permanent => data.is_permanent(),
        // A card with the subtype, whatever its card type — "a Zombie card" is not the
        // same class as "a Zombie creature card".
        CardFilter::Subtype { subtype } => data.has_subtype(subtype),
        // Same printed identity, not same name string: two copies of one printing share
        // a `CardId`, and nothing else does.
        CardFilter::SameNameAsSource => source_card == Some(card),
        // Printed colour (CR 105.2), read off the colour indicator rather than the mana
        // cost: a colourless card matches no colour and a gold card matches each of
        // its own.
        CardFilter::Color { color } => data.colors.contains(color),
        // One class as a card writes it, not two types.
        CardFilter::InstantOrSorcery => {
            data.has_type(CardType::Instant) || data.has_type(CardType::Sorcery)
        }
        CardFilter::Artifact => data.has_type(CardType::Artifact),
    }
}

/// Pose `choices`, or settle the ones that have no legal answer outright.
///
/// Returns whether anything was actually queued — i.e. whether the caller must suspend.
/// A question that is not a decision is answered here instead of being asked, which is
/// the whole of the "a choice with no legal answer resolves without stalling" guarantee,
/// in one place rather than per effect:
///
/// - a card selection whose clamped maximum is zero is applied with an empty selection
///   (which still shuffles a searched library and still bottoms a looked-at pile);
/// - an optional cost no amount of tapping could pay is declined, and *recorded* as
///   declined, so the log never quietly omits a decision the player was entitled to.
pub(crate) fn pose_choices(
    state: &mut GameState,
    choices: Vec<(PlayerId, ChoiceQuestion)>,
    db: &CardDatabase,
) -> bool {
    let mut queued = false;
    for (chooser, question) in choices {
        match &question {
            ChoiceQuestion::Cards(request) => {
                let (_, max) = choice_bounds(state, request, db);
                if max == 0 {
                    apply_choice_outcome(state, request, &[], db);
                    continue;
                }
            }
            ChoiceQuestion::Confirm(request) => {
                if !cost_could_be_paid(state, chooser, request.cost.as_deref(), db) {
                    state.record_event(GameEvent::OptionalDeclined { player: chooser });
                    continue;
                }
            }
            // A color question always has five legal answers, so it is always posed —
            // the one shape with no "nothing to ask" case at all.
            ChoiceQuestion::Color(_) => {}
        }
        state.pending_choices.push(PendingChoice {
            chooser,
            question,
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

/// Discard cards to pay an additional cast cost (CR 601.2b).
///
/// The same move as any other discard, deliberately: a card discarded to a cost is
/// discarded (CR 701.8), and routing it here rather than writing a second hand-to-
/// graveyard move is what keeps that true as discards grow triggers and replacements.
/// It exists only because the cost path has already *chosen* the cards — they arrive in
/// the payment — so it needs the second half of a choice without the first.
pub(crate) fn discard_to_cost(state: &mut GameState, subject: PlayerId, chosen: &[CardInstanceId]) {
    discard_chosen(state, subject, chosen);
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
) -> Option<Vec<(PlayerId, ChoiceQuestion)>> {
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
                            ChoiceQuestion::Cards(ChoiceRequest {
                                subject,
                                zone: ChoiceZone::Hand,
                                filter: filter.clone(),
                                source_card,
                                min: u32::from(*count),
                                max: u32::from(*count),
                                outcome: ChoiceOutcome::Discard,
                            }),
                        )
                    })
                    .collect(),
            )
        }
        Effect::Scry { count } => Some(vec![(
            controller,
            ChoiceQuestion::Cards(ChoiceRequest {
                subject: controller,
                zone: ChoiceZone::LibraryTop(*count),
                filter: CardFilter::Any,
                source_card,
                // Any number, including none (CR 701.17).
                min: 0,
                max: u32::from(*count),
                outcome: ChoiceOutcome::BottomChosen,
            }),
        )]),
        Effect::LookAtTop {
            count,
            take,
            filter,
            destination,
        } => Some(vec![(
            controller,
            ChoiceQuestion::Cards(ChoiceRequest {
                subject: controller,
                zone: ChoiceZone::LibraryTop(*count),
                filter: filter.clone(),
                source_card,
                // Taking is optional ("you may reveal…"), so nothing forces a pick.
                min: 0,
                max: u32::from(*take),
                outcome: ChoiceOutcome::TakeAndBottomRest(*destination),
            }),
        )]),
        Effect::SearchLibrary {
            take,
            filter,
            destination,
        } => Some(vec![(
            controller,
            ChoiceQuestion::Cards(ChoiceRequest {
                subject: controller,
                zone: ChoiceZone::Library,
                filter: filter.clone(),
                source_card,
                // A player may always fail to find (CR 701.19c).
                min: 0,
                max: u32::from(*take),
                outcome: ChoiceOutcome::TakeAndShuffle(*destination),
            }),
        )]),
        // One question per point of mana, so a player producing two really is asked
        // twice and may answer differently each time. They are separate queue entries
        // rather than one multi-answer question because that is what makes the second
        // question askable *after* seeing the first answered, and because the resume
        // machinery already attaches the rest of the resolution to the last of them.
        Effect::AddManaAnyColor {
            amount,
            restriction,
        } => Some(
            (0..*amount)
                .map(|_| {
                    (
                        controller,
                        ChoiceQuestion::Color(ColorRequest {
                            restriction: restriction.clone(),
                        }),
                    )
                })
                .collect(),
        ),
        // The one question the *controller* always answers, whoever else the ability
        // names: an optional effect is theirs to take or leave (CR 608.2).
        Effect::May { cost, effects } => Some(vec![(
            controller,
            ChoiceQuestion::Confirm(ConfirmRequest {
                cost: cost.clone(),
                effects: effects.clone(),
            }),
        )]),
        _ => None,
    }
}

/// Answer a pending color question: put one mana of `color` into the chooser's pool,
/// carrying whatever restriction the effect attached to it (CR 106.6).
///
/// The whole of the answer's consequence — the queue entry is popped by the caller,
/// exactly as a card selection's is, and the rest of the resolution rides on the
/// [`Resume`] attached to the last question of the effect.
pub(crate) fn add_chosen_color(
    state: &mut GameState,
    chooser: PlayerId,
    request: &ColorRequest,
    color: Color,
) {
    let Some(player) = state.players.get_mut(chooser.0) else {
        return;
    };
    match &request.restriction {
        Some(restriction) => player
            .mana_pool
            .add_restricted(color, 1, restriction.clone()),
        None => player.mana_pool.add(color, 1),
    }
}

/// Answer the pending yes-or-no: hand the accepted effects back to be spliced onto the
/// front of the suspended remainder, or `None` for a decline.
///
/// Charging for the acceptance happens here too, because the charge and the answer are
/// one act — a `yes` that could not pay would be a `no` that had already moved cards.
/// The caller has established payability ([`confirm_is_payable`]); an unpayable cost
/// reaching here is treated as a decline rather than granting a free effect.
pub(crate) fn take_confirmed_effects(
    state: &mut GameState,
    chooser: PlayerId,
    request: &ConfirmRequest,
    accept: bool,
) -> Option<Vec<Effect>> {
    if !accept {
        state.record_event(GameEvent::OptionalDeclined { player: chooser });
        return None;
    }
    if let Some(cost) = &request.cost {
        let paid = state
            .players
            .get(chooser.0)
            .and_then(|player| player.mana_pool.pay(&crate::mana::parse_mana_cost(cost)));
        match (paid, state.players.get_mut(chooser.0)) {
            (Some(pool), Some(player)) => player.mana_pool = pool,
            _ => {
                state.record_event(GameEvent::OptionalDeclined { player: chooser });
                return None;
            }
        }
    }
    state.record_event(GameEvent::OptionalApplied { player: chooser });
    Some(request.effects.clone())
}
