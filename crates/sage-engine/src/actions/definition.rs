//! Action types and core methods: the closed set of legal player choices.

use crate::ability::Target;
use crate::id::{CardId, CardInstance, CardInstanceId, PermanentId};

/// One mana ability a payment activates: a permanent under the payer's control and an
/// index into [`crate::abilities_of_permanent`], addressed exactly as
/// [`Action::ActivateAbility`] addresses the same thing.
///
/// It names an *activation*, not an amount. What that ability produces is the card's,
/// and the engine reads it when the payment is applied — a payment that named a
/// quantity would be a second opinion about a card's text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManaSource {
    /// The permanent whose ability is activated.
    pub permanent: PermanentId,
    /// Index into the permanent's abilities (see [`crate::abilities_of_permanent`]).
    pub index: usize,
}

/// One thing a player does to pay for a cast (CR 601.2f–h).
///
/// A cast's total cost is its mana cost plus every additional cost its text imposes
/// (CR 601.2b), and CR 601.2h pays **all of them at once, as part of casting**. So they
/// travel together in one list rather than mana riding in the action and everything else
/// being asked for afterwards — which is what the engine used to do, and why an
/// additional cost could not be taken back: the question was posed once the spell was
/// already on the stack, at which point there was nothing left to undo.
///
/// Carrying them here is what makes the whole cost assembly free to abandon. A player
/// part-way through choosing what to sacrifice, what to discard, and which lands to tap
/// has sent **nothing**, so putting any of it back costs a click and no message.
///
/// Each variant names a *choice*, never an amount or an effect. What a mana ability
/// produces is the card's business, and how many cards a cost discards is the cost's;
/// this says only which ones the player picked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CostPayment {
    /// A mana ability activated to pay the mana cost (CR 601.2f–g).
    Mana(ManaSource),
    /// One card discarded from hand to pay an additional cost (CR 601.2b).
    ///
    /// The card being cast is already on the stack when costs are paid, so it can never
    /// be discarded to its own cost — a rule the ordering enforces rather than a check.
    Discard(CardInstanceId),
    /// One permanent sacrificed to pay an additional cost (CR 601.2b / 701.17).
    ///
    /// Named on the action for the reason a discard is: a cost paid at announcement has
    /// no resolution to ask during, and once the spell is on the stack there is nothing
    /// left to take back. The permanent must be one the caster controls (CR 701.17b) and
    /// of the type the cost names.
    Sacrifice(PermanentId),
    /// One card exiled from the payer's **graveyard** to pay a cost
    /// (CR 601.2b / 701.19).
    ///
    /// Named on the action for the reason a sacrifice is, and distinct from it for the
    /// reason the cost is: a card in a graveyard has a [`CardInstanceId`] and no
    /// [`PermanentId`], so naming one with the other would be naming a different kind of
    /// object. Nothing dies here — the card moves graveyard→exile — which is why it is not
    /// a sacrifice with a different destination.
    Exile(CardInstanceId),
}

impl CostPayment {
    /// The mana source this entry names, if it names one.
    #[must_use]
    pub fn mana(self) -> Option<ManaSource> {
        match self {
            CostPayment::Mana(source) => Some(source),
            CostPayment::Discard(_) | CostPayment::Sacrifice(_) | CostPayment::Exile(_) => None,
        }
    }

    /// The discarded card this entry names, if it names one.
    #[must_use]
    pub fn discard(self) -> Option<CardInstanceId> {
        match self {
            CostPayment::Discard(card) => Some(card),
            CostPayment::Mana(_) | CostPayment::Sacrifice(_) | CostPayment::Exile(_) => None,
        }
    }

    /// The sacrificed permanent this entry names, if it names one.
    #[must_use]
    pub fn sacrifice(self) -> Option<PermanentId> {
        match self {
            CostPayment::Sacrifice(permanent) => Some(permanent),
            CostPayment::Mana(_) | CostPayment::Discard(_) | CostPayment::Exile(_) => None,
        }
    }

    /// The exiled graveyard card this entry names, if it names one.
    #[must_use]
    pub fn exile(self) -> Option<CardInstanceId> {
        match self {
            CostPayment::Exile(card) => Some(card),
            CostPayment::Mana(_) | CostPayment::Discard(_) | CostPayment::Sacrifice(_) => None,
        }
    }
}

/// The mana sources of a payment, in the order they are activated.
#[must_use]
pub(crate) fn mana_of(payment: &[CostPayment]) -> Vec<ManaSource> {
    payment.iter().filter_map(|entry| entry.mana()).collect()
}

/// The cards a payment discards, in the order the player chose them.
#[must_use]
pub(crate) fn discards_of(payment: &[CostPayment]) -> Vec<CardInstanceId> {
    payment.iter().filter_map(|entry| entry.discard()).collect()
}

/// The permanents a payment sacrifices, in the order the player chose them.
#[must_use]
pub(crate) fn sacrifices_of(payment: &[CostPayment]) -> Vec<PermanentId> {
    payment
        .iter()
        .filter_map(|entry| entry.sacrifice())
        .collect()
}

/// The graveyard cards a payment exiles, in the order the player chose them.
#[must_use]
pub(crate) fn exiles_of(payment: &[CostPayment]) -> Vec<CardInstanceId> {
    payment.iter().filter_map(|entry| entry.exile()).collect()
}

/// An action a player may take. The engine generates the legal set with
/// [`crate::valid_actions`] and validates a chosen action against it in
/// [`crate::apply_action`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Yield priority without taking any other action.
    PassPriority,
    /// Play a land from hand (a special action; lands do not use the stack).
    PlayLand {
        /// The specific land card in the active player's hand to play. Names the
        /// physical copy, so two identical lands in hand are distinguishable.
        card: CardInstance,
    },
    /// Activate an ability of a permanent the priority holder controls.
    ActivateAbility {
        /// The permanent whose ability is activated.
        permanent: PermanentId,
        /// Index into the permanent's abilities (see [`crate::abilities_of`]).
        index: usize,
        /// The targets chosen for this activation, one per target slot the
        /// ability's effects declare (see [`crate::Effect::target_spec`]), in that
        /// order. Empty for an ability that targets nothing.
        ///
        /// This is the **parameterized targeted representation** (ADR 0004
        /// §Enumeration): a single [`Action`] value *carries* the player's target
        /// selection, rather than the generator pre-expanding one variant per
        /// legal target combination. [`crate::valid_actions`] advertises the action once
        /// in its *requirement* form (this field empty); the legal candidates for
        /// each slot come from [`crate::target_requirements`], and a filled-in selection
        /// is validated slot-by-slot in [`crate::apply_action`].
        targets: Vec<Target>,
        /// The parts of the activation cost the player **chose** (CR 601.2b): the
        /// permanents sacrificed to a `Sacrifice another creature`, the cards
        /// discarded to a `Discard a card`, and the cards exiled to an `Exile a creature
        /// card from your graveyard`. Empty for every ability whose cost is
        /// entirely about its own source and the pool, which is almost all of them.
        ///
        /// Carried here for the reason [`Self::CastSpell::payment`] is: a cost is paid as
        /// the ability is activated, so there is no resolution to ask during and nothing
        /// left to take back once the ability is on the stack. A player part-way through
        /// deciding what to feed it has sent **nothing**, and putting it back costs a
        /// click and no message.
        ///
        /// **Mana is not carried here.** An activation pays mana out of its controller's
        /// pool (CR 602.2b), floated by activating mana abilities as actions in their own
        /// right, exactly as it was before this field existed; a
        /// [`CostPayment::Mana`] entry on an activation is refused rather than ignored,
        /// because a payment the engine silently dropped is one the player thought they
        /// had made.
        payment: Vec<CostPayment>,
    },
    /// Activate an ability of a card in the priority holder's **graveyard** that
    /// functions from there (CR 113.6 — [`crate::is_graveyard_ability`]).
    ///
    /// A separate variant from [`Self::ActivateAbility`] because it names a separate
    /// kind of object: a card in a zone has a [`CardInstance`] and no
    /// [`PermanentId`], and every read of the source — its abilities, its cost, its
    /// controller, the effect it resolves — has to go through a path that answers for
    /// a card rather than one that assumes the battlefield. Folding it into the
    /// permanent variant would mean a sentinel id, and a sentinel id is how a stale
    /// lookup becomes a silent no-op instead of an illegal action.
    ///
    /// Everything downstream of the announcement is the ordinary path: the ability goes
    /// on the stack as an ordinary [`crate::StackObject`] with an
    /// [`AbilitySource::GraveyardCard`](crate::AbilitySource), any player may respond,
    /// and it resolves through [`crate::resolve`]. Only where the source *is* differs.
    ActivateAbilityFromGraveyard {
        /// The components of the cost the **player picks** — which cards to exile from
        /// that graveyard, which to discard (CR 601.2b). Empty for an ability whose cost
        /// is mana alone, which is every such ability the catalog held before Bone Dragon
        /// (issue #723).
        ///
        /// The graveyard twin of [`Self::ActivateAbility`]'s field, and exact in the same
        /// way: an ability that asks for nothing accepts no payment, and one that asks is
        /// paid by precisely what it names.
        payment: Vec<CostPayment>,
        /// The specific card in the priority holder's graveyard whose ability is
        /// activated. Names the physical copy, so two identical cards in one graveyard
        /// stay individually addressable — and so the one that comes back is the one
        /// that was paid for.
        card: CardInstance,
        /// Index into the card's abilities (see [`crate::abilities_of`]).
        index: usize,
        /// The targets chosen for this activation, in slot order — the same
        /// parameterized representation [`Self::ActivateAbility`] uses, advertised
        /// empty and validated in [`crate::apply_action`]. Empty for an ability that
        /// targets nothing, which today is every graveyard ability the catalog holds.
        targets: Vec<Target>,
    },
    /// Choose the targets of a **triggered ability already on the stack** (CR 603.3d).
    ///
    /// A trigger is put on the stack by the game, not by a player, so its controller
    /// has had no opportunity to aim it: it arrives with no targets and this action is
    /// the choosing. Until it is answered the game does not proceed — the ability goes
    /// on the stack before any player receives priority (CR 603.3b) — so while one is
    /// owed, [`crate::valid_actions`] offers this and nothing else, to the trigger's
    /// controller rather than to whoever last held priority.
    ///
    /// A trigger with *no* legal choice for one of its slots is never put on the stack
    /// at all (CR 603.3c), so this action is only ever offered when it can be answered.
    ChooseTriggerTargets {
        /// The stack object being aimed — a triggered ability owed targets, as
        /// reported by [`crate::pending_trigger_target_choice`].
        ability: crate::stack::StackId,
        /// One target per slot the ability's effects declare, in that order; the same
        /// parameterized representation [`Self::ActivateAbility`] uses.
        targets: Vec<Target>,
    },
    /// Answer the **mid-resolution yes-or-no** the game is currently waiting on: an
    /// optional effect's `you may …` (CR 608.2 — see [`crate::Effect::May`]).
    ///
    /// The confirmation counterpart of [`Self::AnswerChoice`], routed the same way and
    /// with the same exclusivity — while it is owed, its chooser is offered this and
    /// (when the effect asks for mana) their mana abilities, and every other seat is
    /// offered nothing. The chooser is the offering ability's **controller**, who is
    /// frequently not the priority holder: a creature entering on an opponent's turn
    /// asks its own controller whether to take the trigger's optional effect.
    ///
    /// `accept: true` is legal only while the cost is payable from that seat's pool
    /// right now; declining is always legal, which is why an unpayable cost can never
    /// stall the game. A cost no amount of tapping could pay is never posed at all.
    AnswerConfirm {
        /// Whether to apply the optional effect. `false` skips it and resumes the rest
        /// of the object's resolution untouched — declining is not a fizzle.
        accept: bool,
    },
    /// Answer the **mid-resolution player choice** the game is currently waiting on
    /// (CR 701.8 discard, CR 701.17 scry, CR 701.19 search — see
    /// [`crate::pending_player_choice`]).
    ///
    /// An effect that asks a player to choose cards suspends its object's resolution
    /// and queues the question; this action is the answer, and nothing else can happen
    /// until it arrives. Offered to the choice's *chooser* — who may be neither the
    /// priority holder nor the resolving object's controller, since "target player
    /// discards two cards" asks the targeted seat — and to no one else, so while a
    /// choice is owed [`crate::valid_actions`] offers this and nothing else to that
    /// seat and an empty list to every other.
    ///
    /// A choice with no legal answer is never queued at all (it is applied outright
    /// with an empty selection), so like [`Self::ChooseTriggerTargets`] this action is
    /// only ever offered when it can be answered.
    AnswerChoice {
        /// The cards chosen, in the order they were chosen — which is load-bearing for
        /// a scry, where it is the order they are put on the bottom in. Each names a
        /// card in the choice's freshly recomputed candidate set
        /// ([`crate::choice_candidates`]); the selection size must fall within that
        /// choice's clamped bounds ([`crate::choice_bounds`]). An empty selection is
        /// legal whenever the minimum is zero — declining to scry, or failing to find.
        chosen: Vec<CardInstanceId>,
    },
    /// Answer the **mid-resolution color choice** the game is currently waiting on: one
    /// point of `Add two mana in any combination of colors` (see
    /// [`crate::Effect::AddManaAnyColor`]).
    ///
    /// The third answer shape beside [`Self::AnswerChoice`] and [`Self::AnswerConfirm`],
    /// routed identically — offered to the choice's chooser and to no other seat, and
    /// nothing else happens until it arrives. An effect producing more than one mana
    /// queues one question per point, so this action is taken once per mana and the
    /// second answer may name a different color from the first.
    ///
    /// Every one of the five colors is always a legal answer (CR 105.1), so unlike the
    /// other two this action has no payability or candidate gate of any kind.
    AnswerColor {
        /// The color of the one mana this answer adds to the chooser's pool. It carries
        /// whatever spend restriction the producing effect declared (CR 106.6).
        color: crate::mana::Color,
    },
    /// Answer the **CR 616.1 replacement-ordering choice** the game is currently waiting
    /// on: more than one replacement effect would modify the same event, and the
    /// affected object's controller picks which applies first (see
    /// [`crate::pending_replacement_options`]).
    ///
    /// The fourth answer shape beside the three above, routed identically — offered to
    /// the choice's chooser and to no other seat, and nothing else happens until it
    /// arrives. Applying the named replacement may leave others still applicable, in
    /// which case the question is asked again with a shorter list; it terminates because
    /// a replacement never applies twice to one event (CR 614.5).
    ///
    /// The chooser is frequently **not** the effects' controller. A replacement an
    /// opponent created and a self-replacement printed on the entering card are ordered
    /// by the entering permanent's controller, which is what CR 616.1 says and the whole
    /// reason this is a routed choice rather than a decision the engine makes.
    AnswerReplacement {
        /// Which replacement applies first, as a position in the freshly derived option
        /// list — never an id the client made up, and never an index into a list that
        /// was snapshotted when the question was posed.
        index: u8,
    },
    /// Answer the **CR 614.12 card-naming choice** the game is currently waiting on: a
    /// permanent is entering the battlefield and its controller names a card (see
    /// [`crate::named_card_candidates`]).
    ///
    /// The fifth answer shape beside the four above, routed identically — offered to the
    /// choice's chooser and to no other seat, and nothing else happens until it arrives.
    ///
    /// The answer is a **[`CardId`]: a handle to a card the catalog defines**, validated
    /// against the freshly derived candidate list, and never a name a player typed. That
    /// is not a convenience — it is the legal posture. SAGE ships no card name it has not
    /// itself written down, and an action that carried a string would be the one way a
    /// game in progress could come to hold one.
    AnswerCardName {
        /// The card named, which must be in [`crate::named_card_candidates`] for the
        /// class the entering card's ability declared. Recorded on the permanent that
        /// then enters ([`Permanent::named_card`](crate::Permanent)).
        card: CardId,
    },
    /// Answer the **mid-resolution card-ordering choice** the game is currently waiting
    /// on: the *in any order* of a look that puts what it did not take on the bottom of
    /// the library (see [`crate::order_candidates`]).
    ///
    /// The sixth answer shape, routed identically to the five above — offered to the
    /// choice's chooser and to no other seat, and nothing else happens until it arrives.
    ///
    /// What makes it its own action rather than another [`Self::AnswerChoice`] is the
    /// **legality rule**, not the payload: a selection is *between min and max of these
    /// cards*, and an ordering is *all of these cards, once each*. Sharing a variant
    /// would mean one gate reading a pending question to decide which of two rules it was
    /// enforcing, which is exactly the disagreement the choice queue is built to avoid.
    ///
    /// A remainder of nothing or of one card is never posed — there is no arrangement to
    /// make — so like [`Self::ChooseTriggerTargets`] this action is only ever offered
    /// when it can be answered.
    AnswerOrder {
        /// The cards in the order they are put on the bottom of the library, the **first
        /// named ending up deepest** — the convention every bottoming in the engine
        /// follows. It must be a permutation of the freshly recomputed
        /// [`crate::order_candidates`]: a duplicate, a card the remainder does not
        /// contain, and a short list are each rejected outright rather than tolerated.
        order: Vec<CardInstanceId>,
    },
    /// Answer the **mid-resolution permanent choice** the game is currently waiting on:
    /// which permanents to sacrifice (CR 701.17 — see
    /// [`crate::Effect::Sacrifice`](crate::Effect)).
    ///
    /// The fifth answer shape, routed exactly as the other four are — offered to the
    /// choice's chooser and to no other seat, with nothing else happening until it
    /// arrives. It is a separate action from [`Self::AnswerChoice`] because it names
    /// objects on the battlefield rather than cards in a zone, and a token has no card
    /// to name (CR 111): folding the two together would make a board of tokens
    /// unsacrificeable.
    ///
    /// A choice with no legal answer is never queued at all — a player who controls
    /// nothing of the named class simply sacrifices nothing — so like the others this
    /// action is only ever offered when it can be answered.
    AnswerPermanents {
        /// The permanents chosen. Each names one in the choice's freshly recomputed
        /// candidate set ([`crate::permanent_choice_candidates`]), no id twice, and the
        /// selection size must fall within that choice's clamped bounds
        /// ([`crate::permanent_choice_bounds`]).
        chosen: Vec<crate::id::PermanentId>,
    },
    /// Answer the **CR 614.12 permanent choice** the game is currently waiting on: a card
    /// entering the battlefield names a permanent whose copiable values it (or its host)
    /// takes — `As this Aura enters, choose a creature` (see
    /// [`crate::copy_choice_candidates`]).
    ///
    /// The fifth answer shape beside the four above, routed identically — offered to the
    /// choice's chooser and to no other seat, and nothing else happens until it arrives.
    /// The entering card waits in no zone at all while it is owed, which is what makes a
    /// permanent that enters as a copy (CR 707.5) never briefly a permanent that is not.
    ///
    /// It names a **permanent, not a target** (CR 115.1): nothing is aimed, so hexproof
    /// and shroud have nothing to say about the answer, and it is validated against the
    /// class the card printed rather than against a target spec.
    AnswerPermanent {
        /// The permanent named, or `None` for a decline — `You may have this creature
        /// enter as a copy …`, answered with "no". `None` is legal only for a question
        /// the card wrote as optional.
        chosen: Option<PermanentId>,
    },
    /// Cast a spell from hand, paying its mana cost from the caster's pool.
    CastSpell {
        /// The specific card in the caster's hand to cast. Names the physical
        /// copy, so two identical cards in hand are distinguishable.
        card: CardInstance,
        /// The **mode** chosen for a modal spell (CR 700.2), as an index into
        /// [`CardData::modes`](crate::CardData::modes). `None` for every card that is
        /// not modal, and refused for one that is.
        ///
        /// It rides here for the same reason the targets do — it is chosen as part of
        /// casting, and a player assembling one has sent nothing — but it is chosen
        /// **before** them, and that ordering is a structural fact rather than a
        /// convention. The mode decides which effects the spell has, the effects decide
        /// which target slots exist, and so
        /// [`crate::target_requirements`] cannot answer for a modal cast until this
        /// field is filled: it reads the action, finds no chosen mode, and reports no
        /// slots. An announcement that skipped the choice is therefore not a cast with
        /// zero targets, it is one [`crate::apply_action`] rejects.
        ///
        /// Advertised as `None` in the requirement form; the offered options come from
        /// [`crate::mode_options`], and a submitted index is re-derived against the
        /// card's own list at apply, so a forged mode is refused rather than resolved.
        mode: Option<u8>,
        /// The value announced for **X** (CR 601.2b) on a spell whose mana cost carries
        /// `{X}`. `None` for every card whose cost does not, and refused for one whose
        /// cost does.
        ///
        /// **Announced, then locked.** This single number is what the cost is computed
        /// from ([`cast_cost`](crate::actions::cast_cost)), what the payment is checked
        /// against, what is recorded on the stack object, and what the resolving effect
        /// reads ([`DerivedAmount::AnnouncedX`](crate::DerivedAmount)) — so payment,
        /// resolution, and the text a player is shown cannot disagree about a value none
        /// of them derived independently.
        ///
        /// Advertised as `None` in the requirement form; the legal values, and what each
        /// one costs, come from [`crate::x_options`]. A value the caster's payment
        /// cannot cover is refused at apply, not merely left unoffered.
        x: Option<u32>,
        /// The targets chosen for this cast, one per target slot the card's spell
        /// effects declare (see [`crate::Effect::target_spec`]), in that order. Empty for
        /// a spell that targets nothing.
        ///
        /// The same **parameterized targeted representation** an ability uses (ADR
        /// 0009 §Enumeration): the [`Action`] carries the player's selection (CR
        /// 601.2c — targets are chosen as part of casting), rather than the
        /// generator pre-expanding one variant per legal target combination.
        /// [`crate::valid_actions`] advertises the cast once in its requirement form (this
        /// field empty); per-slot candidates come from [`crate::target_requirements`], and
        /// a filled selection is validated slot-by-slot in [`crate::apply_action`].
        targets: Vec<Target>,
        /// Everything the player does to pay for this cast (CR 601.2f–h) — the mana
        /// abilities activated, in the order they are activated, and the cards discarded
        /// to any additional cost. Empty for a cast paid entirely out of mana already
        /// floating by a card with no additional cost, which is what every cast was
        /// before this field existed.
        ///
        /// **This is what makes the casting process one action.** CR 601.2 walks it in
        /// order — announce, choose modes and targets, determine the total cost,
        /// *activate mana abilities*, pay — and rewinds the whole thing if it cannot be
        /// completed. Carrying the payment here is what lets the engine do that: the
        /// sources are tapped, every cost is paid, and the spell goes on the stack in one
        /// indivisible step, or none of it happens and the state is returned unchanged.
        ///
        /// The consequence upstream is the point of it. A player assembling a payment
        /// has sent *nothing*, so taking a source back out is free and instant, and no
        /// client and no server has to hold a half-made payment that some other event
        /// could invalidate. Floating mana first stays exactly as legal as it was
        /// (CR 605.3), and is still the only way to hold mana across a cast.
        ///
        /// Each mana entry is validated as an activation in its own right, in sequence
        /// and against the state the ones before it produced — so a source named twice is
        /// tapped once and then illegal, exactly as clicking it twice would be, and
        /// naming both halves of a dual land is the same submission as naming it twice.
        payment: Vec<CostPayment>,
    },
    /// Discard one card from hand to satisfy the cleanup step's maximum-hand-size
    /// turn-based action (CR 514.1). Offered — one per card in the active
    /// player's hand, a select-from-zone choice — only while that player is over
    /// [`crate::MAX_HAND_SIZE`] during [`crate::Step::Cleanup`]. Names the
    /// physical copy, so identical cards stay individually addressable.
    Discard {
        /// The specific card in the active player's hand to discard.
        card: CardInstance,
    },
    /// Mulligan the current opening hand during the pre-game London mulligan
    /// (CR 103.5): shuffle it back into the library, draw a fresh hand of the
    /// opening size, and decide again. Offered only in the mulligan phase, to the
    /// deciding seat (see [`crate::mulligan`]).
    Mulligan,
    /// Keep the current opening hand, ending this seat's London-mulligan decisions
    /// (CR 103.5). A seat that has taken `N` mulligans must put `N` cards on the
    /// bottom of its library; `bottom` names those cards, one [`Target::Card`]
    /// per card, chosen from the [bottoming requirement](crate::bottom_requirement)
    /// (empty for a first-hand keep). [`crate::valid_actions`] advertises this in its
    /// *requirement* form (empty `bottom`); a filled-in selection is validated in
    /// [`crate::apply_action`]. Offered only in the mulligan phase, to the deciding
    /// seat.
    Keep {
        /// The chosen cards to put on the bottom of the library, in the order they
        /// are placed there. Exactly one [`Target::Card`] per mulligan taken, each
        /// naming a distinct card currently in the deciding seat's hand.
        bottom: Vec<Target>,
    },
    /// Declare the active player's attackers (CR 508.1), the turn-based player
    /// choice of the declare-attackers step. Each named permanent must be a legal
    /// attacker candidate ([`crate::attacker_candidates`]); an **empty** selection is
    /// legal — declaring no attackers (CR 508.1a). Applying it taps each attacker
    /// (no vigilance yet) and moves the step into its priority round.
    ///
    /// Like [`Action::ActivateAbility`]'s targets, this is a *parameterized*
    /// multi-select: [`crate::valid_actions`] advertises the action once in its empty
    /// requirement form, the legal candidates come from [`crate::attacker_candidates`],
    /// and a filled-in selection is validated against that fresh set in
    /// [`crate::apply_action`] — never pre-expanded into one action per subset.
    DeclareAttackers {
        /// The declared attacks: each names one attacker and the defending player
        /// it attacks (CR 508.1a). In a two-player game the only legal defender is
        /// the sole opponent; with more seats each attacker chooses among the
        /// opponents still in the game ([`crate::defender_candidates`]).
        attackers: Vec<Attack>,
    },
    /// Declare the defending player's blockers (CR 509.1), the turn-based player
    /// choice of the declare-blockers step. Each [`Block`] assigns one eligible
    /// blocker ([`crate::blocker_candidates`]) to one attacking creature
    /// ([`crate::declared_attackers`]); several blockers may share an attacker, but a
    /// blocker is assigned to exactly one (CR 509.1a). An **empty** selection is
    /// legal — declaring no blockers.
    DeclareBlockers {
        /// The blocker→attacker assignments, one per declared blocker.
        blocks: Vec<Block>,
    },
    /// The attacking player's combat-damage assignment order (CR 510.1, issue #346),
    /// the turn-based choice owed once blockers are declared and some attacker is
    /// blocked by two or more creatures. One [`DamageOrder`] per such attacker, each
    /// a permutation of that attacker's blockers; combat damage is then assigned
    /// just-lethal along the chosen order. An attacker with 0–1 blockers is never
    /// ordered. Advertised in its empty requirement form; a filled selection is
    /// validated in [`crate::apply_action`].
    OrderCombatDamage {
        /// One blocker ordering per multi-blocked attacker.
        orders: Vec<DamageOrder>,
    },
    /// Accept the CR 903.9a choice: move the commander from the graveyard or exile
    /// it went to into its owner's command zone instead. Offered only to the
    /// commander's owner while a return decision is pending (the commander sits in
    /// a graveyard or exile awaiting the choice — see [`crate::valid_actions`]). Applying
    /// it removes the card from wherever it is and puts it in the command zone as a
    /// fresh object (it will mint a fresh [`PermanentId`] if recast), and logs the
    /// movement.
    ReturnCommanderToCommandZone {
        /// The commander card to move to the command zone. Names the physical copy
        /// so the owner's commander is unambiguous.
        card: CardInstance,
    },
    /// Decline the CR 903.9a choice: leave the commander where it went (the
    /// graveyard or exile). Offered alongside [`Action::ReturnCommanderToCommandZone`]
    /// while a return decision is pending; applying it simply clears the pending
    /// decision so the commander stays put. This is the decline-compatible default,
    /// so priority automation always has a legal way forward and never stalls.
    DeclineCommanderReturn {
        /// The commander card whose return is declined.
        card: CardInstance,
    },
    /// Concede the game (CR 104.3a). Always offered to the acting seat, in every
    /// phase and step, so a player may leave at any time. Applying it marks the
    /// conceding player as having lost; the game then becomes terminal with the
    /// opponent as the winner (CR 104.2a).
    Concede,
}

/// One attacker→defender assignment of a [`Action::DeclareAttackers`] declaration
/// (CR 508.1a): the `attacker` is declared to attack `defender`, a player or a
/// planeswalker.
///
/// In a two-player game with no planeswalker to attack, every attack's `defender` is
/// the sole opponent and the declaration is choice-free; otherwise each attacker
/// records what it attacks, which is what blocker eligibility and combat damage follow
/// (issues #341 and #608). Plain `Copy`/`Eq` data, mirroring [`Block`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Attack {
    /// The creature declared as an attacker (an [`crate::attacker_candidates`] member).
    pub attacker: PermanentId,
    /// What it attacks — a player or a planeswalker they control (a
    /// [`crate::defender_candidates`] member).
    pub defender: crate::combat::AttackTarget,
}

/// One attacker's combat-damage assignment order (CR 510.1, issue #346): the
/// `attacker`'s blockers listed in the order its controller chose to assign lethal
/// damage along. `blockers` is a permutation of exactly that attacker's declared
/// blockers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DamageOrder {
    /// The multi-blocked attacker whose damage order this is.
    pub attacker: PermanentId,
    /// Its blockers, in the chosen assignment order.
    pub blockers: Vec<PermanentId>,
}

/// One blocker→attacker assignment of a [`Action::DeclareBlockers`] declaration
/// (CR 509.1a): the `blocker` is declared to block the attacking `attacker`.
///
/// Plain `Copy`/`Eq` data (no closures), so an [`Action`] stays a value the
/// engine can compare and the state machine can carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Block {
    /// The creature declared as a blocker (a [`crate::blocker_candidates`] member).
    pub blocker: PermanentId,
    /// The attacking creature it is assigned to block (a [`crate::declared_attackers`]
    /// member).
    pub attacker: PermanentId,
}

impl Action {
    /// The chosen targets carried by this action, in slot order; empty for an
    /// action that carries none.
    pub(super) fn targets(&self) -> &[Target] {
        match self {
            Action::ActivateAbility { targets, .. }
            | Action::ActivateAbilityFromGraveyard { targets, .. }
            | Action::CastSpell { targets, .. }
            | Action::ChooseTriggerTargets { targets, .. } => targets,
            // `Keep::bottom` is a mulligan sub-choice and `AnswerChoice::chosen` a
            // mid-resolution one; both name cards in a hidden zone rather than
            // targets, and are validated through their own paths, never this one.
            Action::PassPriority
            | Action::AnswerChoice { .. }
            | Action::AnswerConfirm { .. }
            | Action::AnswerColor { .. }
            | Action::AnswerReplacement { .. }
            | Action::AnswerCardName { .. }
            | Action::AnswerOrder { .. }
            | Action::AnswerPermanents { .. }
            | Action::AnswerPermanent { .. }
            | Action::PlayLand { .. }
            | Action::Discard { .. }
            | Action::Mulligan
            | Action::Keep { .. }
            // Combat declarations carry permanent selections, not `Target`s, so
            // they hold none of the ability-targeting vocabulary; their selection
            // is validated separately in `action_is_legal`.
            | Action::DeclareAttackers { .. }
            | Action::DeclareBlockers { .. }
            | Action::OrderCombatDamage { .. }
            // The commander-return decisions carry only a card, no target slots.
            | Action::ReturnCommanderToCommandZone { .. }
            | Action::DeclineCommanderReturn { .. }
            // Concede carries no selection.
            | Action::Concede => &[],
        }
    }

    /// This action with its chosen targets cleared — its *requirement* form, the
    /// shape [`crate::valid_actions`] advertises. Target-carrying variants drop their
    /// selection; every other variant is returned unchanged.
    pub(super) fn without_targets(&self) -> Action {
        match self {
            // An activation drops its target selection *and* the chosen half of its cost,
            // for the reason a cast drops both: CR 602.2 announces the ability first and
            // chooses targets and pays costs as steps within it, so the requirement form
            // is the bare announcement.
            Action::ActivateAbility {
                permanent, index, ..
            } => Action::ActivateAbility {
                permanent: *permanent,
                index: *index,
                targets: Vec::new(),
                payment: Vec::new(),
            },
            Action::ActivateAbilityFromGraveyard { card, index, .. } => {
                Action::ActivateAbilityFromGraveyard {
                    card: *card,
                    index: *index,
                    targets: Vec::new(),
                    payment: Vec::new(),
                }
            }
            // A cast drops its **announcement choices**, its target selection, and its
            // payment to its requirement form, the shape `valid_actions` advertises. All
            // of them are filled in later and by the same rule (CR 601.2): the process
            // announces the spell first, and chooses modes, then targets, then activates
            // mana abilities as steps within it. The bare announcement is the card.
            Action::CastSpell { card, .. } => Action::CastSpell {
                card: *card,
                mode: None,
                x: None,
                targets: Vec::new(),
                payment: Vec::new(),
            },
            Action::ChooseTriggerTargets { ability, .. } => Action::ChooseTriggerTargets {
                ability: *ability,
                targets: Vec::new(),
            },
            // The mulligan keep's bottom selection is cleared the same way, so its
            // requirement form matches what [`valid_actions`] advertises — as is a
            // mid-resolution choice's, which is likewise advertised empty and filled
            // in by the answer.
            Action::Keep { .. } => Action::Keep { bottom: Vec::new() },
            Action::AnswerChoice { .. } => Action::AnswerChoice { chosen: Vec::new() },
            // A yes-or-no is advertised as the bare question; the answer rides in the
            // submitted action, so its requirement form is the declining default —
            // never a second offer per possible reply.
            Action::AnswerConfirm { .. } => Action::AnswerConfirm { accept: false },
            // A color choice is advertised the same way: one bare question, whose
            // answer names a color in the submitted action rather than five separate
            // offers. White is the requirement form's arbitrary stand-in, never a
            // default anyone is charged for.
            Action::AnswerColor { .. } => Action::AnswerColor {
                color: crate::mana::Color::White,
            },
            // And a replacement-ordering answer the same way: one bare question, whose
            // answer names a position in the submitted action. Zero is the requirement
            // form's stand-in, never a default anyone is held to.
            Action::AnswerReplacement { .. } => Action::AnswerReplacement { index: 0 },
            // And a card-naming answer the same way: one bare question, whose answer
            // names a card in the submitted action. `CardId(0)` is the requirement
            // form's stand-in — the same role White plays for a colour — and never a
            // card anyone is held to having named; the legality gate checks the
            // submitted handle against the freshly derived candidates.
            Action::AnswerCardName { .. } => Action::AnswerCardName { card: CardId(0) },
            // A card ordering is advertised as the bare question too, and its answer is
            // the permutation the submitted action carries — never one offer per
            // arrangement, which for five cards would be a hundred and twenty of them.
            Action::AnswerOrder { .. } => Action::AnswerOrder { order: Vec::new() },
            // And a permanent selection the same way, for the reason a card selection
            // is: the offer is the bare question and the chosen ids ride in the
            // submitted action.
            Action::AnswerPermanents { .. } => Action::AnswerPermanents { chosen: Vec::new() },
            // And a permanent choice the same way: one bare question, whose answer names
            // a permanent — or names none — in the submitted action. The declining form
            // is the stand-in, never a default anyone is held to.
            Action::AnswerPermanent { .. } => Action::AnswerPermanent { chosen: None },
            // The requirement form of a combat declaration is the empty selection —
            // exactly what `valid_actions` advertises during the declare window.
            Action::DeclareAttackers { .. } => Action::DeclareAttackers {
                attackers: Vec::new(),
            },
            Action::DeclareBlockers { .. } => Action::DeclareBlockers { blocks: Vec::new() },
            Action::OrderCombatDamage { .. } => Action::OrderCombatDamage { orders: Vec::new() },
            other => other.clone(),
        }
    }
}

/// One target slot of a targeted [`Action`]: the [`crate::TargetSpec`] that constrains
/// the slot together with the *set* of [`Target`]s currently legal for it.
///
/// This is the unit [`crate::target_requirements`] advertises per slot — the "target
/// requirement plus the set of legal targets" of ADR 0004 §Enumeration. The
/// candidate set is O(N) in that slot's candidate count; see the combinatorial
/// guard on [`crate::target_requirements`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetRequirement {
    /// What the slot may target.
    pub spec: crate::TargetSpec,
    /// Whether the slot may be left **empty** — the "up to" of "up to two target
    /// creatures" (CR 601.2c). `false` for every slot of an ordinary effect, which must
    /// be filled or the announcement is illegal.
    ///
    /// A group that takes up to N targets is advertised as N slots, of which the ones
    /// past its minimum carry this. The alternative — one slot with a count — would make
    /// every existing client and every existing binding path learn about counts to
    /// express something only one card needs.
    pub optional: bool,
    /// Every [`Target`] legal for the slot against current state, in a stable
    /// board order. A single O(N) scan of the relevant candidate universe.
    pub candidates: Vec<Target>,
}
