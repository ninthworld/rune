//! Mid-resolution player choices: "choose N cards from this set", "do you want this to
//! happen?", "which color of mana?", "name a card", and "in which order?".
//!
//! The questions live here; what an *answer* does lives in [`outcome`].
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
//!   cards from current state on every call, and [`named_card_candidates`] recomputes
//!   the nameable cards from the catalog, so an answer is validated against the set
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
//!   it is declined outright ([`ChoiceQuestion::Confirm`]), and so is an arrangement of
//!   one card or of none ([`ChoiceQuestion::Order`]), which has one answer and so asks
//!   no question.
//!
//! ## Hidden information
//!
//! This is the first place the engine shows a player cards no one else may see. The
//! engine itself holds every zone in the clear; the discipline is on the projection
//! ([`crate::PendingChoice::chooser`] is the *only* seat a searched library or a
//! revealed hand may be shown to) and on the log, which records counts and never card
//! identities. See `docs/decisions/0013-mid-resolution-player-choices.md`.

mod outcome;
mod permanents;
mod posing;

pub(crate) use outcome::{
    apply_card_name_outcome, apply_choice_outcome, apply_color_outcome, apply_order_outcome,
    discard_to_cost, take_confirmed_effects,
};
pub(crate) use permanents::{answer_permanents_is_legal, apply_permanent_choice};
pub use permanents::{
    permanent_choice_bounds, permanent_choice_candidates, PermanentOutcome, PermanentRequest,
};
pub(crate) use posing::{attach_resume, choices_for_effect, pose_choices};

use crate::ability::{BottomOrder, CardFilter, Effect, FoundDestination, Target};
use crate::card_type::{CardType, Supertype};
use crate::id::{CardId, CardInstance, CardInstanceId, PlayerId};
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
    /// Which **color**? — one point's worth of `Add two mana in any combination of
    /// colors`, or the colour a permanent is given as it enters ([`ColorRequest`]).
    /// Answered with [`Action::AnswerColor`](crate::Action).
    ///
    /// The third shape rather than a special case of the first two because the answer
    /// is neither a set of cards nor a yes-or-no: it names one of the five colors
    /// (CR 105.1), and every one of them is always a legal answer. An effect producing
    /// more than one mana poses one of these per point, so the player really is asked
    /// "and the second one?" rather than being made to spend all of it on one color.
    Color(ColorRequest),
    /// Which **replacement effect applies first**? — the CR 616.1 ordering choice, posed
    /// to the affected object's controller when more than one replacement would modify
    /// the same event ([`ReplacementRequest`]). Answered with
    /// [`Action::AnswerReplacement`](crate::Action).
    ///
    /// The fourth shape because the answer is a fourth thing: a *position in a list the
    /// engine derived*, rather than cards, a yes-or-no, or a colour. Everything around
    /// it is the same as the other three — one queue, one chooser, one [`Resume`] — which
    /// is the rule this enum is built on.
    Replacement(ReplacementRequest),
    /// **Name a card** — the CR 614.12 choice a permanent's controller makes as it
    /// enters ([`CardNameRequest`]). Answered with
    /// [`Action::AnswerCardName`](crate::Action).
    ///
    /// The fifth shape because the answer is a fifth thing: a **card identity**, which is
    /// neither a card in a zone (nothing is being moved, and the named card need not be
    /// anywhere in the game) nor a position in a derived list of effects. It is the one
    /// question whose answer set is drawn from the *catalog* rather than from the board
    /// ([`named_card_candidates`]), and that is the whole of the project's legal posture
    /// on naming: the answer is a [`CardId`] chosen from the cards SAGE has defined, so
    /// no prose and no name the catalog does not contain can ever be recorded.
    CardName(CardNameRequest),
    /// In **which order**? — the arrangement a card asks for when it puts more than one
    /// card back on a library *in any order* ([`OrderRequest`]). Answered with
    /// [`Action::AnswerOrder`](crate::Action).
    ///
    /// The fifth shape, and the first whose answer is a **permutation**: not a subset of
    /// the cards on offer but all of them, arranged. That is what keeps it apart from
    /// [`Cards`](Self::Cards), whose answer is a selection and whose bounds are a count —
    /// a question with the same candidates and a completely different legality rule
    /// (every card exactly once, no more and no fewer).
    Order(OrderRequest),
    /// Choose **permanents** on the battlefield — the sacrifice of
    /// [`Effect::Sacrifice`](crate::Effect) ([`PermanentRequest`]). Answered with
    /// [`Action::AnswerPermanents`](crate::Action).
    ///
    /// The fifth shape because the answer names a fifth thing: objects on the
    /// battlefield, which a card selection cannot stand in for. A card selection is a
    /// list of [`CardInstance`](crate::id::CardInstance)s and a token has no card behind
    /// it (CR 111), so a board full of tokens would be a board nobody could sacrifice.
    Permanents(PermanentRequest),
}

impl ChoiceQuestion {
    /// The card-selection request this question asks, or `None` when it is not one.
    /// Lets a caller that only handles the selection shape — the candidate projection,
    /// the revealed-cards channel — say so once rather than matching every variant.
    #[must_use]
    pub fn cards(&self) -> Option<&ChoiceRequest> {
        match self {
            ChoiceQuestion::Cards(request) => Some(request),
            ChoiceQuestion::Confirm(_)
            | ChoiceQuestion::Color(_)
            | ChoiceQuestion::CardName(_)
            | ChoiceQuestion::Replacement(_)
            | ChoiceQuestion::Order(_)
            | ChoiceQuestion::Permanents(_) => None,
        }
    }

    /// The yes-or-no request this question asks, or `None` when it is not one.
    #[must_use]
    pub fn confirm(&self) -> Option<&ConfirmRequest> {
        match self {
            ChoiceQuestion::Confirm(request) => Some(request),
            ChoiceQuestion::Cards(_)
            | ChoiceQuestion::Color(_)
            | ChoiceQuestion::CardName(_)
            | ChoiceQuestion::Replacement(_)
            | ChoiceQuestion::Order(_)
            | ChoiceQuestion::Permanents(_) => None,
        }
    }

    /// The mana-color request this question asks, or `None` when it is not one.
    #[must_use]
    pub fn color(&self) -> Option<&ColorRequest> {
        match self {
            ChoiceQuestion::Color(request) => Some(request),
            ChoiceQuestion::Cards(_)
            | ChoiceQuestion::Confirm(_)
            | ChoiceQuestion::CardName(_)
            | ChoiceQuestion::Replacement(_)
            | ChoiceQuestion::Order(_)
            | ChoiceQuestion::Permanents(_) => None,
        }
    }

    /// The card-naming request this question asks, or `None` when it is not one.
    #[must_use]
    pub fn card_name(&self) -> Option<&CardNameRequest> {
        match self {
            ChoiceQuestion::CardName(request) => Some(request),
            ChoiceQuestion::Cards(_)
            | ChoiceQuestion::Confirm(_)
            | ChoiceQuestion::Color(_)
            | ChoiceQuestion::Replacement(_)
            | ChoiceQuestion::Order(_)
            | ChoiceQuestion::Permanents(_) => None,
        }
    }

    /// The ordering request this question asks, or `None` when it is not one.
    #[must_use]
    pub fn order(&self) -> Option<&OrderRequest> {
        match self {
            ChoiceQuestion::Order(request) => Some(request),
            ChoiceQuestion::Cards(_)
            | ChoiceQuestion::Confirm(_)
            | ChoiceQuestion::Color(_)
            | ChoiceQuestion::CardName(_)
            | ChoiceQuestion::Replacement(_)
            | ChoiceQuestion::Permanents(_) => None,
        }
    }

    /// The permanent-selection request this question asks, or `None` when it is not one.
    #[must_use]
    pub fn permanents(&self) -> Option<&PermanentRequest> {
        match self {
            ChoiceQuestion::Permanents(request) => Some(request),
            ChoiceQuestion::Cards(_)
            | ChoiceQuestion::Confirm(_)
            | ChoiceQuestion::Color(_)
            | ChoiceQuestion::CardName(_)
            | ChoiceQuestion::Replacement(_)
            | ChoiceQuestion::Order(_) => None,
        }
    }

    /// The replacement-ordering request this question asks, or `None` when it is not one.
    #[must_use]
    pub fn replacement(&self) -> Option<&ReplacementRequest> {
        match self {
            ChoiceQuestion::Replacement(request) => Some(request),
            ChoiceQuestion::Cards(_)
            | ChoiceQuestion::Confirm(_)
            | ChoiceQuestion::Color(_)
            | ChoiceQuestion::CardName(_)
            | ChoiceQuestion::Order(_)
            | ChoiceQuestion::Permanents(_) => None,
        }
    }
}

/// A card-ordering question ([`ChoiceQuestion::Order`]): *in which order do these go on
/// the bottom of your library?*
///
/// Carries a **window**, not a list. The cards are the top `count` of the subject's
/// library, recomputed by [`order_candidates`] on every read, in exactly the relationship
/// [`ChoiceRequest`] has to [`choice_candidates`] and for the same reason (ADR 0013 §2):
/// while the question is owed nothing else in the game is legal, so the library cannot
/// move underneath it, and an answer is therefore checked against the cards that are
/// there *now* rather than the ones a client was shown.
///
/// That window is also why the look that poses this leaves its remainder on top of the
/// library rather than picking it up: the cards have to be somewhere derivable, and the
/// place they already are is the honest one.
///
/// `count` is never `0` or `1`. A remainder that short is not a decision, so it is
/// bottomed outright and no question is posed at all — the never-stall rule of ADR 0013
/// §5, applied to a shape whose bounds are not a `(min, max)` pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OrderRequest {
    /// The player whose library's top cards are being arranged, and whose bottom they
    /// are going to. Equal to [`PendingChoice::chooser`]: only the looker orders their
    /// own remainder.
    pub subject: PlayerId,
    /// How many cards from the top are being arranged — the whole of the question, since
    /// a legal answer names every one of them exactly once.
    pub count: u8,
}

/// The CR 616.1 ordering question ([`ChoiceQuestion::Replacement`]): *which of these
/// applicable replacement effects applies first?*
///
/// It carries the **event** and nothing else. The options are recomputed from the event
/// and the state on every read ([`applicable_to_entry`](crate::replacement)), in the same
/// relationship [`ChoiceRequest`] has to [`choice_candidates`]: a snapshotted list is one
/// more thing that can disagree with the game, and while this question is owed nothing
/// else is legal, so the list under it cannot move.
///
/// The event is the whole of what is suspended. There is no [`Resume`] on a choice
/// carrying one — the entry is the last step of a resolution rather than one of its
/// effects — and answering simply hands the (now modified) entry back to the layer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplacementRequest {
    /// The battlefield entry being modified, waiting in no zone at all until the
    /// ordering is decided.
    pub entry: crate::replacement::PendingEntry,
}

/// A colour question ([`ChoiceQuestion::Color`]): *which color?*, and what the answer
/// is then used for.
///
/// Carries nothing about *who* — the chooser is [`PendingChoice::chooser`], which for
/// both of today's uses is the seat the answer belongs to: mana is added to the pool of
/// the controller of the effect producing it (CR 106.4), and an "as this enters" choice
/// is made by the entering permanent's controller (CR 614.12).
///
/// It carries no candidate list either, for the reason [`ChoiceRequest`] carries none:
/// the answer set is the five colors (CR 105.1) and is the same at every table in every
/// game, so there is nothing to compute or to keep fresh.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColorRequest {
    /// What becomes of the colour once it is named.
    pub outcome: ColorOutcome,
}

/// What a named colour is *for* — the counterpart of [`ChoiceOutcome`] for the colour
/// question, and the whole of the difference between its two uses.
///
/// One question with two outcomes rather than two questions, because the *asking* is
/// identical: five answers, all of them always legal, none of them derived from state.
/// Splitting on the outcome instead keeps the queue, the routing, the action, and the
/// priority hand-off single, which is the rule [`ChoiceQuestion`] is built on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ColorOutcome {
    /// Put mana of the chosen colour into the chooser's pool (CR 106.4).
    AddMana {
        /// How many points of it this one answer produces. One for `add N mana in any
        /// combination of colors`, which asks N times; N for `add N mana of any one
        /// color`, which asks once.
        amount: u8,
        /// What the produced mana may be spent on (CR 106.6), or `None` for ordinary
        /// mana. Copied from the effect so the restriction rides each point as it is
        /// chosen.
        restriction: Option<crate::ability::ManaRestriction>,
    },
    /// Record the chosen colour on a permanent that is **entering the battlefield**
    /// (CR 614.12), and hand the entry back to the seam that asked.
    ///
    /// While this is owed the card is on the battlefield's doorstep and in no zone at
    /// all — the same place a spell's card is while its resolution is suspended
    /// ([`SuspendedSpell`]). That is what makes "the permanent is never briefly on the
    /// battlefield without its colour" true by construction rather than by ordering: it
    /// is not there yet. Answering writes the colour onto the event and re-enters
    /// [`begin_battlefield_entry`](crate::GameState), which puts the permanent there with
    /// the colour already on it, so the state-based-action loop, the trigger diff, and
    /// every projection see one arrival, complete.
    RecordOnEntry(crate::replacement::PendingEntry),
}

/// A card-naming question ([`ChoiceQuestion::CardName`]): *name a card*, as a permanent
/// enters the battlefield (CR 614.12).
///
/// It carries the **class** of card that may be named and the **event** that is waiting,
/// and no candidate list: [`named_card_candidates`] derives the nameable cards from the
/// catalog on every read, in the same relationship [`ChoiceRequest`] has to
/// [`choice_candidates`]. Like a [`ReplacementRequest`] it needs no [`Resume`] — the
/// entry is the last step of a resolution rather than one of its effects.
///
/// One outcome today, so there is no outcome enum: a second use adds one, exactly as
/// [`ColorOutcome`] appeared when the colour question grew its second.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardNameRequest {
    /// Which cards may be named — the "nonbasic land card" of a card that narrows it.
    pub class: NamedCardClass,
    /// The battlefield entry waiting on the answer, in no zone at all until it comes.
    pub entry: crate::replacement::PendingEntry,
}

/// The class of card a [`CardNameRequest`] may name — the words a card puts between
/// "name a" and "card".
///
/// A deliberately small closed set that grows by adding a variant when a card needs one,
/// like every other authored selector. It is **not** a [`CardFilter`]: that vocabulary
/// classifies a card sitting in a zone, and this one classifies an entry in the catalog,
/// which is a different question about a different kind of thing — nothing being named
/// here need be in the game at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NamedCardClass {
    /// A **nonbasic land** card — a land without the basic supertype (CR 205.4a).
    NonbasicLand,
}

impl NamedCardClass {
    /// Whether the printed card `card` belongs to this class.
    ///
    /// Reads printed characteristics, which is the only reading available and the right
    /// one: a card name is named in the abstract, off no object on any battlefield.
    #[must_use]
    fn admits(self, data: &crate::CardData) -> bool {
        match self {
            NamedCardClass::NonbasicLand => {
                data.has_type(CardType::Land) && !data.supertypes.contains(&Supertype::Basic)
            }
        }
    }
}

/// The cards `class` currently admits, in ascending [`CardId`] order — the complete,
/// always-legal answer set of a card-naming question.
///
/// **Derived from the catalog on every read**, exactly as [`choice_candidates`] is
/// derived from a zone. Nothing snapshots it, so an answer is checked against the set
/// that exists now.
///
/// It is also the whole of the project's legal posture on naming a card: a player names
/// one of the cards SAGE has *defined*, never a string they typed, so the answer that
/// reaches [`Permanent::named_card`](crate::Permanent) is a functional identity and the
/// engine can never come to hold a card name it did not already ship.
#[must_use]
pub fn named_card_candidates(db: &CardDatabase, class: NamedCardClass) -> Vec<CardId> {
    db.all()
        .into_iter()
        .filter(|(_, data)| class.admits(data))
        .map(|(id, _)| id)
        .collect()
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
    /// The targets the announcement chose for those effects (CR 601.2c), in slot
    /// order — a `may` declares the group of the one effect it wraps
    /// ([`Effect::target_group`]), so "you may destroy target artifact" arrives here
    /// already aimed.
    ///
    /// They ride the request rather than the [`Resume`] because they belong to the
    /// offer: accepting splices them onto the front of the remaining targets exactly
    /// as it splices the effects, and declining drops both together. Leaving them in
    /// the remainder would hand a declined offer's target to whatever effect came
    /// next.
    pub targets: Vec<Target>,
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
    /// bottom of the library, in the order `order` names.
    ///
    /// [`BottomOrder::Chosen`] is the one outcome that leaves a *second* question owed:
    /// the remainder stays on top of the library and an [`OrderRequest`] over it is
    /// queued behind this one, unless the remainder is too short to be a decision.
    TakeAndBottomRest {
        /// Where a taken card goes.
        destination: FoundDestination,
        /// How the cards not taken reach the bottom — the game's roll, or the looker's
        /// arrangement.
        order: BottomOrder,
    },
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
    /// What the suspended ability came from (CR 113.3), for a self-referential remaining
    /// effect. `None` for a spell, which has no source object of its own.
    pub source: Option<crate::stack::AbilitySource>,
    /// The effects not yet applied, in order.
    pub effects: Vec<Effect>,
    /// The stored targets those effects have not yet consumed, in slot order.
    pub targets: Vec<Target>,
    /// The spell whose card still has to reach its final zone, `None` for an ability.
    pub spell: Option<SuspendedSpell>,
    /// What the suspended resolution knows about itself — the window an intervening
    /// condition reads over, the X its object announced, whether its damage can be
    /// prevented, and what paying for the object recorded (see
    /// [`Resolution`](crate::Resolution)).
    ///
    /// Carried through the suspension for the same reason the remaining effects are: a
    /// discard-then-draw asks its question, and the `if a card is discarded this way`
    /// that follows must still be measured from where the resolution started, not from
    /// where it woke up. The same is true of an announced X — a spell that stops to ask
    /// something resumes with the value it was cast for, not with none — and the payment
    /// travels for a stronger version of the same reason: it could not be recovered from
    /// anywhere at all once the question is answered.
    pub resolution: crate::resolve::Resolution,
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
/// a choice has no legal answer and must not be posed. It is also what lets a card state
/// a **mandatory** take — *put one of them into your hand* — without that floor ever
/// becoming a hang: a library that cannot supply one lowers the floor to nothing, and the
/// choice settles with an empty selection.
///
/// The returned pair is always satisfiable. The floor is held below the ceiling as well
/// as below what is available, so a card authored with a minimum larger than its maximum
/// is a wrong effect rather than a game nobody can continue — the direction ADR 0013 says
/// to be wrong in.
#[must_use]
pub fn choice_bounds(state: &GameState, request: &ChoiceRequest, db: &CardDatabase) -> (u32, u32) {
    let available = u32::try_from(choice_candidates(state, request, db).len()).unwrap_or(u32::MAX);
    let max = request.max.min(available);
    (request.min.min(available).min(max), max)
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

/// Whether `named` is a legal answer to the card-naming question currently owed: a
/// card-name question *is* owed, and the card is in its freshly derived candidate set
/// ([`named_card_candidates`]).
///
/// The same regenerate-and-check discipline [`answer_is_legal`] applies, and the enforced
/// half of the legal posture: an answer naming a card the catalog does not contain — or
/// one outside the class the card narrowed the choice to — is refused rather than
/// recorded, so nothing a client sends can put an undefined name into the game state.
#[must_use]
pub(crate) fn named_card_is_legal(state: &GameState, named: CardId, db: &CardDatabase) -> bool {
    let Some(request) = pending_player_choice(state).and_then(|p| p.question.card_name()) else {
        return false;
    };
    named_card_candidates(db, request.class).contains(&named)
}

/// The cards `request` currently asks to be arranged, top of the library first.
///
/// The [`choice_candidates`] of the ordering question, and derived on every call for the
/// same reason: a snapshot is one more thing that can disagree with the game. Shorter
/// than `count` only if a library somehow shrank while the question was owed, which no
/// legal transition can do — nothing but the answer is legal while it is pending.
///
/// **Visible only to [`PendingChoice::chooser`].** Like [`choice_candidates`], this
/// returns the cards to any caller; the projection is what keeps them off another seat's
/// wire.
#[must_use]
pub fn order_candidates(state: &GameState, request: &OrderRequest) -> Vec<CardInstance> {
    state
        .players
        .get(request.subject.0)
        .map(|player| {
            player
                .library
                .iter()
                .rev()
                .take(usize::from(request.count))
                .copied()
                .collect()
        })
        .unwrap_or_default()
}

/// Whether `order` is a legal answer to the ordering currently owed: a **permutation** of
/// exactly the cards [`order_candidates`] names right now.
///
/// Three ways to be wrong and all three are rejected rather than tolerated, because each
/// would silently mean something the player did not say: a card named twice would bottom
/// one card and lose another, a card the remainder does not contain would move a card
/// nobody looked at, and a short answer would leave cards on top that the effect said go
/// underneath. There is no clamping counterpart to [`choice_bounds`] here — an ordering
/// has one legal size, and it is the whole list.
#[must_use]
pub(crate) fn order_answer_is_legal(state: &GameState, order: &[CardInstanceId]) -> bool {
    // An ordering aimed at some other shape of question is an answer to a question
    // nobody asked.
    let Some(request) = pending_player_choice(state).and_then(|p| p.question.order()) else {
        return false;
    };
    let cards = order_candidates(state, request);
    if order.len() != cards.len() {
        return false;
    }
    let distinct = order
        .iter()
        .enumerate()
        .all(|(index, id)| !order[..index].contains(id));
    distinct && order.iter().all(|id| cards.iter().any(|c| c.id == *id))
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
