//! The optional effect's yes-or-no ([`ChoiceQuestion::Confirm`]) and what accepting one
//! costs.
//!
//! Every other question in [`super`] asks a player to *pick* something. This one asks
//! whether they want a thing to happen at all (CR 608.2), and the answer is a decision
//! rather than a selection — which is why it is its own shape, its own action, and its
//! own file.
//!
//! The cost is the half with all the rules in it. A mana payment is charged the moment
//! the answer is given, through the seam a cast and an activation use. A payment the
//! player *picks* — a permanent to sacrifice, cards to discard — cannot be: which one is
//! itself a question, so accepting poses that question and the effects the payment bought
//! wait behind it. Both are gated by the same promise, which is the one thing this module
//! exists to keep: **a cost that cannot be paid is never posed at all**, and is recorded
//! as declined rather than left as a decision nobody could take.

use super::*;

/// The question an optional effect asks: *do you want this, and will you pay for it?*
///
/// Carries what happens on a **yes** and nothing about what happens on a no, because a
/// no is the absence of an event: the effects here are simply not applied, and the rest
/// of the resolution — which rides on [`PendingChoice::resume`], not here — is
/// untouched either way.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfirmRequest {
    /// What accepting charges, or `None` for a free `you may`.
    ///
    /// Mana is paid from the chooser's pool at the moment they accept, through the same
    /// [`ManaPool::pay`](crate::ManaPool::pay) seam a cast uses. A sacrifice or a discard
    /// cannot be: which permanent, which cards is a decision, and a decision is a
    /// question. Accepting one of those poses the ordinary selection
    /// ([`optional_payment_question`]) and hangs the rest of the resolution behind it, so
    /// the cost is paid before the effects it bought and by exactly the machinery a
    /// mandatory sacrifice or discard uses.
    pub cost: Option<OptionalCost>,
    /// The permanent the asking ability came from, resolved when the question was posed
    /// — the `another` of a `sacrifice another creature` cost, and nothing else.
    ///
    /// `None` for a spell, for an emblem, and for every offer whose cost does not ask.
    /// Resolved up front rather than looked up on acceptance because the source may leave
    /// the battlefield while the question sits owed, and an exclusion that quietly
    /// stopped excluding would let a creature eat itself to pump its own corpse.
    pub source: Option<crate::id::PermanentId>,
    /// The effects applied **instead**, when the offer is declined — the `unless` half of
    /// `sacrifice it unless you pay`. Empty for a plain `you may`, where declining is the
    /// end of it.
    ///
    /// Carried on the request rather than left behind in the remainder for
    /// [`Self::effects`]' reason: the two branches belong to the offer, and exactly one of
    /// them is spliced onto the front of what is left.
    pub otherwise: Vec<Effect>,
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

/// Whether the pending yes-or-no can currently be answered **yes**: it is a
/// [`ChoiceQuestion::Confirm`], and its cost (if it has one) is payable as the board
/// stands right now.
///
/// The counterpart of [`choice_bounds`] for a confirmation — the fact a projection needs
/// in order to offer "yes" only when the engine would accept it, and the fact
/// [`crate::apply_action`]'s gate re-derives before charging anyone. `false` when no
/// choice is owed or the owed one is a card selection.
///
/// Mana is read against the pool *now*, deliberately: a chooser owed a payment may still
/// tap lands (CR 605.3a), so this flips from `false` to `true` as they float mana, and
/// the offer follows. A payment the chooser *picks* is read the only way it can be — is
/// there anything to pick — which is the same answer whenever it is asked, so for those
/// two this and [`cost_could_be_paid`] agree by construction.
#[must_use]
pub fn confirm_is_payable(state: &GameState, db: &CardDatabase) -> bool {
    let Some(pending) = pending_player_choice(state) else {
        return false;
    };
    let Some(request) = pending.question.confirm() else {
        return false;
    };
    cost_is_payable_from_pool(state, pending.chooser, request, db)
}

/// Whether `player` can pay `request`'s cost as the board stands. `true` for a free
/// choice, and for a seat that has left the game there is no pool and so no payment.
fn cost_is_payable_from_pool(
    state: &GameState,
    player: PlayerId,
    request: &ConfirmRequest,
    db: &CardDatabase,
) -> bool {
    let Some(cost) = &request.cost else {
        return true;
    };
    match cost.mana() {
        Some(mana) => state
            .players
            .get(player.0)
            .is_some_and(|p| p.mana_pool.can_pay(&crate::mana::parse_mana_cost(mana))),
        None => payment_is_available(state, player, request, db),
    }
}

/// Whether `player` could pay `request`'s cost if they tapped everything they have —
/// their pool plus every point of mana their untapped sources could still add.
///
/// This, not the current pool, is what decides whether an optional cost is *posed*: a
/// player with an empty pool and two untapped Forests can pay `{1}`, and auto-declining
/// them would take away a decision the rules give them. The estimate is the same
/// deliberate over-estimate [`crate::priority_has_no_meaningful_action`] makes — every
/// mana ability of every untapped source, as though one permanent could be tapped for
/// all of them — and errs in the same safe direction: it can only ever *offer* a choice
/// that turns out unpayable, which the chooser simply declines, never withhold one they
/// could have taken.
///
/// A picked payment has no such gap. Tapping a land cannot conjure a creature to
/// sacrifice or a card to discard, so "could they pay" and "can they pay" are one
/// question there, asked of the payment's own candidate set.
/// Whether `player` could pay `cost` right now, by any means available to them —
/// including mana abilities they have not activated yet (CR 605.3a).
///
/// The half of [`cost_could_be_paid`] that asks about a cost rather than about a whole
/// request, for the one caller that has the cost before the request exists: the resolution
/// deciding whether `unless you pay` is a question at all.
#[must_use]
pub(crate) fn optional_cost_could_be_paid(
    state: &GameState,
    player: PlayerId,
    cost: &OptionalCost,
    db: &CardDatabase,
) -> bool {
    cost_could_be_paid(
        state,
        player,
        &ConfirmRequest {
            cost: Some(cost.clone()),
            source: None,
            effects: Vec::new(),
            otherwise: Vec::new(),
            targets: Vec::new(),
        },
        db,
    )
}

pub(super) fn cost_could_be_paid(
    state: &GameState,
    player: PlayerId,
    request: &ConfirmRequest,
    db: &CardDatabase,
) -> bool {
    let Some(cost) = &request.cost else {
        return true;
    };
    match cost.mana() {
        Some(mana) => crate::actions::potential_mana_pool(state, player, db)
            .can_pay(&crate::mana::parse_mana_cost(mana)),
        None => payment_is_available(state, player, request, db),
    }
}

/// The question accepting `request` still owes — the sacrifice or the discard its cost
/// names — or `None` for a cost that is paid on the spot (mana) or is no cost at all.
///
/// **One place decides both halves.** Whether the offer is posed, whether an acceptance
/// is legal, and what the acceptance then asks are all read off this one request, so the
/// class a player is offered to sacrifice can never differ from the class the engine
/// checked before offering them the choice.
fn optional_payment_question(
    chooser: PlayerId,
    request: &ConfirmRequest,
) -> Option<(PlayerId, ChoiceQuestion)> {
    match request.cost.as_ref()? {
        OptionalCost::Mana { .. } => None,
        // CR 701.17b: a player sacrifices only what they control, so the subject is the
        // chooser and there is nothing to author about whose it is.
        OptionalCost::Sacrifice {
            card_type,
            subtype,
            another,
        } => Some((
            chooser,
            ChoiceQuestion::Permanents(PermanentRequest {
                subject: chooser,
                card_type: *card_type,
                subtype: subtype.clone(),
                except: another.then_some(request.source).flatten(),
                min: 1,
                max: 1,
                outcome: PermanentOutcome::Sacrifice,
            }),
        )),
        OptionalCost::Discard { count } => Some((
            chooser,
            ChoiceQuestion::Cards(ChoiceRequest {
                subject: chooser,
                zone: ChoiceZone::Hand,
                filter: CardFilter::Any,
                // A discard to a cost takes any card, so there is no printed identity
                // for a filter to compare against and nothing to resolve here.
                source_card: None,
                min: u32::from(*count),
                max: u32::from(*count),
                outcome: ChoiceOutcome::Discard,
            }),
        )),
    }
}

/// Whether the payment `request` would owe on acceptance has enough to answer it: the
/// clamped maximum still covers what the cost demands.
///
/// The picked-payment counterpart of "can the pool pay", expressed in the one currency
/// both selection shapes already have — a question whose bounds clamp below its minimum
/// is a question with no legal answer, which is the engine's single signal for "there is
/// nothing here".
fn payment_is_available(
    state: &GameState,
    chooser: PlayerId,
    request: &ConfirmRequest,
    db: &CardDatabase,
) -> bool {
    match optional_payment_question(chooser, request) {
        Some((_, ChoiceQuestion::Permanents(payment))) => {
            permanent_choice_bounds(state, &payment, db).1 >= payment.min
        }
        Some((_, ChoiceQuestion::Cards(payment))) => {
            choice_bounds(state, &payment, db).1 >= payment.min
        }
        // A cost that owes no question is paid on the spot or is not a cost; neither is
        // this function's business, and both are handled by its callers.
        _ => true,
    }
}

/// An accepted offer: the effects to splice onto the front of the suspended remainder,
/// and the payment the acceptance has not made yet.
pub(crate) struct Accepted {
    /// The branch that was taken, in printed order — the wrapped effects on an
    /// acceptance, the `unless` effects on a decline that has one.
    pub effects: Vec<Effect>,
    /// The targets that branch carries. The accepted branch takes the offer's, chosen at
    /// announcement (CR 601.2c); a declined one carries none, and may not target.
    pub targets: Vec<Target>,
    /// The selection the cost still owes — a sacrifice or a discard — or `None` when the
    /// cost was mana (already charged) or there was no cost. The caller poses it and
    /// hangs the remainder behind it, so the payment happens before the effects it bought.
    pub payment: Option<(PlayerId, ChoiceQuestion)>,
}

/// Answer the pending yes-or-no: hand the branch that was taken back to be spliced onto
/// the front of the suspended remainder, or `None` when nothing was.
///
/// Charging for the acceptance happens here too, because the charge and the answer are
/// one act — a `yes` that could not pay would be a `no` that had already moved cards.
/// For mana that is a payment; for a cost whose payment the player *picks* it is the
/// question that payment will be made through, handed back rather than posed here so the
/// caller can hang the rest of the resolution behind it in one place.
///
/// The caller has established payability ([`confirm_is_payable`]); an unpayable cost
/// reaching here is treated as a decline rather than granting a free effect, which is why
/// the picked payment is built *before* the acceptance is recorded.
pub(crate) fn take_confirmed_effects(
    state: &mut GameState,
    chooser: PlayerId,
    request: &ConfirmRequest,
    accept: bool,
    db: &CardDatabase,
) -> Option<Accepted> {
    if !accept {
        state.record_event(GameEvent::OptionalDeclined { player: chooser });
        return declined(request);
    }
    let payment = optional_payment_question(chooser, request);
    let paid = match (&request.cost, &payment) {
        (None, _) => true,
        // A picked payment is not made here; it is owed. It counts as payable exactly
        // when there is something to pick, which is the check the offer was gated on.
        (Some(_), Some(_)) => payment_is_available(state, chooser, request, db),
        (Some(cost), None) => charge_mana(state, chooser, cost),
    };
    if !paid {
        state.record_event(GameEvent::OptionalDeclined { player: chooser });
        return declined(request);
    }
    state.record_event(GameEvent::OptionalApplied { player: chooser });
    Some(Accepted {
        effects: request.effects.clone(),
        targets: request.targets.clone(),
        payment,
    })
}

/// What a **declined** offer leaves behind: its `unless` branch, or nothing at all.
///
/// The branch carries no targets. The offer's targets were chosen for the effect it
/// wraps (CR 601.2c) and go with it, so a declined offer drops them — an `unless` branch
/// that could aim would be a second announcement nobody made.
fn declined(request: &ConfirmRequest) -> Option<Accepted> {
    if request.otherwise.is_empty() {
        return None;
    }
    Some(Accepted {
        effects: request.otherwise.clone(),
        targets: Vec::new(),
        payment: None,
    })
}

/// Spend `cost` from `chooser`'s pool, answering whether it was there to spend.
///
/// The one place mana moves outside the cast and activation paths, through the same
/// [`ManaPool::pay`](crate::ManaPool::pay) seam both of those use.
fn charge_mana(state: &mut GameState, chooser: PlayerId, cost: &OptionalCost) -> bool {
    let Some(mana) = cost.mana() else {
        return false;
    };
    let paid = state
        .players
        .get(chooser.0)
        .and_then(|player| player.mana_pool.pay(&crate::mana::parse_mana_cost(mana)));
    match (paid, state.players.get_mut(chooser.0)) {
        (Some(pool), Some(player)) => {
            player.mana_pool = pool;
            true
        }
        _ => false,
    }
}
