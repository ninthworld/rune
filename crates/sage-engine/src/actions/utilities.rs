//! Utility helpers for action validation and generation.

use crate::ability::{is_mana_ability, Ability, Cost, Effect};
use crate::card::abilities_of_permanent;
use crate::card_type::CardType;
use crate::id::{CardId, PlayerId};
use crate::mana::ManaPool;
use crate::state::{GameState, Permanent};
use crate::CardDatabase;

/// Whether `card` is a land, by its structured printed types.
pub(crate) fn is_land(db: &CardDatabase, card: CardId) -> bool {
    db.card(card).is_some_and(|c| c.has_type(CardType::Land))
}

/// Whether `card` may be cast as a spell from hand today (CR 117.1a).
///
/// A land is never cast — it is played as a special action (CR 116.2a) and is
/// offered separately. Every other card type — instant, sorcery, artifact,
/// enchantment (Auras included, since issue #152), creature — is castable, subject
/// to timing and cost checked by the caller. An Aura additionally requires a legal
/// enchant target to be *offered* (CR 303.4c/601.2c); that is enforced by the
/// per-slot candidate check in [`crate::valid_actions`] over [`crate::CardData::cast_target_specs`],
/// not here.
pub(crate) fn is_castable_spell(data: &crate::CardData) -> bool {
    !data.has_type(CardType::Land)
}

/// Whether `data` may be cast **any time its controller has priority** rather than only
/// at sorcery speed — an instant (CR 117.1a), or a card with flash (CR 702.8).
///
/// The single timing predicate every casting road asks: from hand, from a graveyard
/// under a one-turn permission, and from the command zone. Flash says exactly one thing
/// — "cast this as though it were an instant" — so saying it here, once, is what keeps a
/// card with flash from being castable on one road and not another.
#[must_use]
pub(crate) fn castable_at_instant_speed(data: &crate::CardData) -> bool {
    data.has_type(CardType::Instant) || data.keywords.contains(&crate::card::Keyword::Flash)
}

/// Whether every cost in `cost` is payable right now, given the source
/// `permanent`'s state, its controller's mana pool, and — for a cost the player picks
/// what pays it with — whether the board and hand hold anything that could.
///
/// Mana affordability is decided by the same [`ManaPool::can_pay`](crate::ManaPool::can_pay)
/// the cast path uses over the same `{...}` notation, so an ability is offered
/// exactly when [`crate::apply_action`] will succeed in charging for it — the
/// offer and the charge can never disagree about a cost string.
pub(crate) fn cost_payable(
    state: &GameState,
    db: &CardDatabase,
    cost: &[Cost],
    permanent: &Permanent,
) -> bool {
    cost.iter().all(|c| match c {
        Cost::Tap => !permanent.tapped,
        Cost::Mana { mana } => state
            .players
            .get(crate::characteristics::controller_of(state, permanent).0)
            .is_some_and(|player| {
                player
                    .mana_pool
                    .can_pay(&crate::mana::parse_mana_cost(mana))
            }),
        // CR 606.3: a loyalty cost that would remove more counters than the permanent
        // has cannot be paid, so a `−7` ability is simply not offered on a planeswalker
        // at 4. A `+N` or `0` cost is always payable — there is no upper bound on
        // loyalty.
        Cost::Loyalty { amount } => loyalty_cost_is_payable(permanent, *amount),
        // A permanent on the battlefield can always be sacrificed (CR 701.17a), and an
        // ability is only ever offered from one that is.
        Cost::SacrificeThis => true,
        // CR 118.3: a cost that removes counters is payable only out of counters the
        // permanent actually has, which is what makes a three-charge artifact offer its
        // ability three times and then stop.
        Cost::RemoveCounters { counter, count } => permanent.counter_count(*counter) >= *count,
        // CR 601.2b: the costs whose payment the player *picks* are payable only while
        // there is enough to pick, so an ability with nothing to feed it is not offered
        // rather than offered and then found free. The candidate enumeration is the one
        // the action's own slot is posed from, so the offer, the question, and the charge
        // are one answer.
        Cost::Sacrifice { .. } | Cost::Discard { .. } | Cost::ExileFromGraveyard { .. } => {
            crate::actions::chosen_costs_are_payable(state, db, permanent, c)
        }
    })
}

/// The activated ability `index` of `card`, if that card is in `seat`'s graveyard right
/// now **and** the ability functions from there (CR 113.6,
/// [`is_graveyard_ability`]).
///
/// The one lookup every part of the graveyard-activation seam goes through — the offer,
/// the apply-time re-check, the target-group enumeration, and the announcement — so
/// "which ability is this action naming" is answered once and the four can never disagree
/// about it. Three separate `None`s, each of which is a way an action can be stale rather
/// than merely unpayable: the card has left the graveyard, the index names nothing (or
/// names a non-activated ability), or the ability is an ordinary one that only works on
/// the battlefield.
///
/// It reads the *graveyard*, never the battlefield: a card in a zone has no
/// [`Permanent`] to look up, and asking for one is the mistake this exists to make
/// impossible.
pub(crate) fn graveyard_ability(
    state: &GameState,
    db: &CardDatabase,
    seat: PlayerId,
    card: crate::id::CardInstance,
    index: usize,
) -> Option<Ability> {
    let player = state.players.get(seat.0)?;
    if !player.graveyard.iter().any(|c| c.id == card.id) {
        return None;
    }
    let ability = crate::card::abilities_of(db, card.card)
        .into_iter()
        .nth(index)?;
    if !matches!(ability, Ability::Activated { .. })
        || !crate::ability::is_graveyard_ability(&ability)
    {
        return None;
    }
    Some(ability)
}

/// Whether `seat` can pay `cost` for an ability activated from their graveyard.
///
/// The graveyard counterpart of [`cost_payable`], and deliberately narrower: a card in a
/// graveyard is not a permanent, so there is nothing to tap, nothing to sacrifice, and no
/// counters to remove. Mana is the only component such a cost can have, and **every other
/// component is unpayable** rather than ignored — an ability authored with a `{T}` it
/// cannot pay is not offered at all, instead of being offered for free. The catalog
/// validator rejects that authoring outright ([`crate::Violation`]); this is the second,
/// independent gate that holds for a database assembled in a test.
///
/// Mana affordability goes through the same [`ManaPool::can_pay`] the battlefield path
/// uses over the same `{...}` notation, so the offer and the charge cannot disagree.
pub(crate) fn graveyard_cost_payable(state: &GameState, seat: PlayerId, cost: &[Cost]) -> bool {
    cost.iter().all(|c| match c {
        Cost::Mana { mana } => state.players.get(seat.0).is_some_and(|player| {
            player
                .mana_pool
                .can_pay(&crate::mana::parse_mana_cost(mana))
        }),
        Cost::Tap
        | Cost::Loyalty { .. }
        | Cost::SacrificeThis
        | Cost::RemoveCounters { .. }
        // A chosen sacrifice, discard, or graveyard exile is refused here for the same
        // reason as the rest: the catalog validator lets a graveyard ability charge mana
        // and nothing else, and this is the second, independent gate that holds for a
        // database assembled in a test.
        | Cost::Sacrifice { .. }
        | Cost::ExileFromGraveyard { .. }
        | Cost::Discard { .. } => false,
    })
}

/// Whether `permanent` currently has enough loyalty counters to pay a loyalty cost of
/// `amount` (CR 606.3). Trivially true for a zero or positive amount; for a negative
/// one the permanent must have at least that many counters.
///
/// The single expression of the rule, shared by the offer ([`cost_payable`]) and the
/// independent apply-time gate ([`crate::actions::legality`]), so an ability is offered
/// exactly when it can be paid for.
pub(crate) fn loyalty_cost_is_payable(permanent: &Permanent, amount: i32) -> bool {
    match u32::try_from(-i64::from(amount)) {
        // A negative cost: it must not take the permanent below zero.
        Ok(spent) => permanent.counter_count(crate::state::CounterKind::Loyalty) >= spent,
        // A zero or positive cost costs no counters at all.
        Err(_) => true,
    }
}

/// Whether the CR 606.3 timing rules allow `permanent`'s controller to activate a
/// **loyalty** ability right now: at sorcery speed on their own turn, and only if no
/// loyalty ability of that same permanent has already been activated this turn.
///
/// Sorcery speed here is the same three conditions [`crate::valid_actions`] applies to
/// a sorcery — the permanent's controller is the active player, the game is in a main
/// phase, and the stack is empty — measured from the *controller* rather than from
/// whoever holds priority, so an opponent holding priority in a main phase can never
/// activate their own planeswalker's ability through a window that is not theirs.
///
/// `false` for a permanent that is not on the battlefield, since there is no source to
/// activate. Non-loyalty abilities never consult this: an ordinary activated ability of
/// a planeswalker (there are none in the catalog, but the IR permits one) is bound only
/// by the ordinary gates.
pub(crate) fn loyalty_timing_allows(state: &GameState, permanent: &Permanent) -> bool {
    sorcery_speed_for(state, permanent) && !state.loyalty_activations.contains(&permanent.id)
}

/// Whether `permanent`'s controller could cast a sorcery right now — the timing an
/// **equip** ability is bound by (CR 702.6b), the timing an ability that prints
/// `Activate only as a sorcery.` declares (CR 602.5d), and the half of CR 606.3
/// [`loyalty_timing_allows`] shares.
///
/// One expression of "sorcery speed, measured from the permanent's controller", so the
/// three cannot disagree about when that is. Measured from the *controller* rather than
/// from whoever holds priority for the reason the loyalty gate is: an opponent holding
/// priority in a main phase must not be able to act through a window that is not theirs.
///
/// Unlike a loyalty ability there is no per-turn limit: an Equipment may be moved, and a
/// sorcery-speed ability activated, as many times in a main phase as its controller can
/// pay for.
pub(crate) fn sorcery_timing_allows(state: &GameState, permanent: &Permanent) -> bool {
    sorcery_speed_for(state, permanent)
}

/// The three conditions [`crate::valid_actions`] applies to a sorcery — the permanent's
/// controller is the active player, the game is in a main phase, and the stack is empty
/// (CR 117.1a).
///
/// The controller is the **computed** one (CR 613 layer 2), not the stored field: an
/// Equipment whose control has changed is equipped on its new controller's turn, not its
/// owner's.
fn sorcery_speed_for(state: &GameState, permanent: &Permanent) -> bool {
    crate::characteristics::controller_of(state, permanent) == state.active_player
        && matches!(
            state.step,
            crate::phase::Step::PrecombatMain | crate::phase::Step::PostcombatMain
        )
        && state.stack.is_empty()
}

/// `player`'s pool **plus every unit of mana their untapped permanents could still
/// produce** (CR 605.1) — the mana they have if they tap out.
///
/// A deliberate *over*-estimate: it sums the output of every mana ability of every
/// untapped source, ignoring that one permanent can only be tapped for one of them, and
/// credits a summoning-sick creature's mana ability (CR 302.6) that
/// [`valid_actions`](crate::valid_actions) would not offer this turn. Both errors point
/// the same way — toward crediting mana the seat may not actually be able to make — and
/// both callers want that direction:
///
/// - [`priority_has_no_meaningful_action`](crate::priority_has_no_meaningful_action)
///   uses it to decide a seat is idle, and an over-estimate only ever keeps a seat
///   *non*-idle, so nobody is auto-passed past a play they had;
/// - the optional-cost gate ([`crate::confirm_is_payable`]) uses it to decide whether to
///   pose a payment at all, and an over-estimate only ever *offers* a choice that turns
///   out unpayable — which the chooser declines — instead of silently taking one away.
///
/// One estimate rather than two, so the two can never disagree about what a board could
/// pay for.
pub(crate) fn potential_mana_pool(
    state: &GameState,
    player: PlayerId,
    db: &CardDatabase,
) -> ManaPool {
    let mut pool = state
        .players
        .get(player.0)
        .map(|p| p.mana_pool.clone())
        .unwrap_or_default();
    for perm in &state.battlefield {
        if crate::characteristics::controller_of(state, perm) != player || perm.tapped {
            continue;
        }
        for ability in abilities_of_permanent(state, db, perm) {
            if !is_mana_ability(&ability) {
                continue;
            }
            let Ability::Activated { effects, .. } = ability else {
                continue;
            };
            for effect in &effects {
                match effect {
                    Effect::AddMana { color, amount } => pool.add(*color, *amount),
                    Effect::AddColorlessMana { amount } => pool.add_colorless(*amount),
                    // Restricted mana (CR 106.6) is credited *with its restriction*, not
                    // as ordinary mana: it is real mana a seat could still make, so
                    // omitting it would under-estimate and could auto-pass a player who
                    // still had a Dragon to cast — the one direction this estimate must
                    // never err in. Carrying the restriction is what stops the opposite
                    // mistake, of crediting it toward a spell it can never pay for.
                    Effect::AddRestrictedMana {
                        color,
                        amount,
                        restriction,
                    } => pool.add_restricted(*color, *amount, restriction.clone()),
                    // The colors are not decided until the ability resolves, so the
                    // estimate credits the seat with `amount` of *every* color. That
                    // over-counts on purpose, in the direction this estimate is allowed
                    // to err: it can only ever offer a decision that turns out
                    // unaffordable, never withhold one the player could have taken.
                    // Whether the colors must all match is exactly the constraint this
                    // over-count already ignores, so it changes nothing here.
                    Effect::AddManaAnyColor {
                        amount,
                        same_color: _,
                        restriction,
                    } => {
                        for color in crate::mana::Color::ALL {
                            match restriction {
                                Some(restriction) => {
                                    pool.add_restricted(color, *amount, restriction.clone());
                                }
                                None => pool.add(color, *amount),
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    pool
}

/// Whether `cost` contains the tap symbol `{T}` (CR 118.3f) — the cost component
/// CR 302.6 forbids a summoning-sick creature from paying.
///
/// NOTE: [`Cost::Tap`] is the only cost the effect IR models today. When the untap
/// symbol `{Q}` (CR 118.3g) is added it belongs in this predicate too: CR 302.6
/// restricts *both* symbols on a summoning-sick creature, and this is the one seam
/// that gate runs through.
pub(crate) fn cost_requires_tapping(cost: &[Cost]) -> bool {
    cost.contains(&Cost::Tap)
}

/// Whether CR 302.6 forbids activating an ability of `permanent` whose activation
/// cost is `cost`: the cost includes `{T}` and the permanent is a creature still
/// affected by summoning sickness (see
/// [`summoning_sickness_restricts`](crate::combat::summoning_sickness_restricts),
/// which applies the CR 702.10b haste exemption).
///
/// CR 605.3a exempts nothing: a mana ability with `{T}` in its cost is gated
/// exactly like any other activated ability. Non-creature permanents are never
/// summoning sick, so a land played this turn still taps for mana.
pub(crate) fn tap_cost_is_summoning_sick(
    state: &GameState,
    permanent: &Permanent,
    cost: &[Cost],
    db: &CardDatabase,
) -> bool {
    cost_requires_tapping(cost) && crate::combat::summoning_sickness_restricts(state, permanent, db)
}

/// Whether every element of `ids` is distinct. O(n²), which is fine for the
/// handful of creatures a combat declaration ever names and keeps the engine free
/// of a hashing dependency for a tiny list.
pub(crate) fn all_unique<T: PartialEq>(ids: &[T]) -> bool {
    ids.iter().enumerate().all(|(i, id)| !ids[..i].contains(id))
}

/// The total mana cost of casting `card` right now, and the printed subtypes a
/// restricted-mana check reads (CR 106.6).
///
/// One answer, in one place, because every road that touches a cast's price goes down
/// it and they must not be able to disagree: the generator deciding whether to offer the
/// cast, the pip enumeration posing what is still owed, the payment search, the legality
/// gate deciding whether a payment covers it, [`crate::apply_action`] charging it, and
/// the view telling the client what the spell costs. The idle predicate joins them by
/// construction — it asks [`crate::valid_actions`] of a board with its mana floated
/// rather than reading a cost of its own.
///
/// Two things happen to the printed cost here, in the order CR 601.2 puts them. The
/// commander tax (CR 903.8) is an **additional cost**, part of the total rather than a
/// surcharge applied later, which is exactly why it cannot live only in the apply path.
/// Cost modification (CR 601.2f) then applies to that total — see
/// [`crate::cost_modification`].
///
/// `None` for a card the database does not hold — the same defensive absence every other
/// lookup here returns rather than a zero cost that would read as free.
///
/// `x` is the value announced for X (CR 601.2b), and folding it in here is what makes
/// the announcement binding: a `{X}{R}` announced at 3 costs `{3}{R}`, and every road
/// that asks what the spell costs asks this one, so no road can price it differently. An
/// unannounced X contributes nothing (CR 202.3b), which is also exactly right for the
/// offer gate — the cheapest a spell with X can be is X = 0.
pub(crate) fn cast_cost(
    state: &GameState,
    db: &CardDatabase,
    card: crate::id::CardInstance,
    x: Option<u32>,
) -> Option<(crate::mana::ManaCost, Vec<String>)> {
    let data = db.card(card.card)?;
    let mut base = crate::mana::parse_mana_cost(&data.mana_cost);
    // Each `{X}` in the printed cost demands the announced value in generic mana
    // (CR 107.3). Saturating rather than wrapping: the enumeration never offers a value
    // this could overflow, and a cost that silently wrapped would read as free.
    if let Some(announced) = x {
        let pips = u32::from(data.x_pips());
        let added = u8::try_from(pips.saturating_mul(announced)).unwrap_or(u8::MAX);
        base.generic = base.generic.saturating_add(added);
    }
    let caster = state.priority;
    let player = state.players.get(caster.0)?;
    // A commander cast from the command zone carries the tax; the same card cast from
    // hand does not, so *where it is* decides the cost (CR 903.8).
    let from_command = player.command.iter().any(|c| c.id == card.id);
    let total = if from_command {
        let casts = player.commander.as_ref().map_or(0, |c| c.casts);
        crate::commander::commander_tax_cost(&base, casts)
    } else {
        base
    };
    let cost = crate::cost_modification::modified_cast_cost(state, db, caster, card.card, total);
    // **Without paying its mana cost** (CR 601.2b) — Omniscience. An alternative cost is
    // applied last, after the tax and after every modification, because it *replaces* the
    // mana component rather than adjusting it: a reduction applied afterwards would be
    // arithmetic on a cost nobody is paying.
    //
    // Here rather than at each caller for the reason every other cost rule is here: the
    // offer, the payment search, the legality gate, the charge, the pips, and the view all
    // ask this one function, so a spell cannot be advertised free and then charged. That is
    // the same guarantee `total_cast_cost` documents, and it is why this is a *cost* answer
    // and not a flag every caller would have to remember to check.
    //
    // **The mana component only.** An additional cost the card names (CR 601.2b — a
    // discard, a sacrifice) is still paid, and so is every non-mana cost; those are asked
    // elsewhere and this does not reach them. An `{X}` in the cost is `0` when nothing is
    // paid for it (CR 107.3b), which falls out of the same emptying.
    if crate::player::casts_from_hand_without_paying(state, caster, db)
        && player.hand.iter().any(|c| c.id == card.id)
    {
        return Some((crate::mana::ManaCost::default(), data.subtypes.clone()));
    }
    Some((cost, data.subtypes.clone()))
}

/// What casting `card` costs the priority holder **right now** — its printed cost, plus
/// the commander tax where one applies, after every cost modification (CR 601.2f).
///
/// The public face of [`cast_cost`], and the reason it is public: the client renders what
/// a spell costs and computes no cost of its own, so the number a view carries has to be
/// the very one the offer was gated on and the charge will take. Asking the same function
/// is what makes that true rather than merely likely.
///
/// `None` for a card the database does not hold. An unannounced X contributes nothing
/// (CR 202.3b), so a spell with `{X}` prices here at X = 0 — the same floor the offer
/// gate uses, with each announceable value's own price enumerated by
/// [`crate::x_options`].
///
/// **It does not take an `x`, and that is the answer rather than an omission**
/// (issue #776). The question was whether this should be parameterised by an announced
/// value, since every road that reads a cast's price comes through here. It should not:
/// X is announced *as part of casting* (CR 601.2b), so at the moment each of these
/// callers asks, no value exists to pass.
///
/// - The **offer** and the **legality gate** ask whether the spell is castable at all,
///   which is the X = 0 floor — a caster who can afford more announces more, and a
///   caster who cannot afford the floor cannot cast it at any value.
/// - The **payment search** and the **charge** work from the announced action, which
///   carries its own X, and price it through [`crate::x_options`] — the one place a
///   value and a price are paired.
/// - The **view** and the **pips** render the printed cost, which is what the card says.
///
/// So a value belongs to an *announcement*, and a price for one belongs to
/// [`crate::x_options`]; this function answers the question that has no announcement
/// behind it. What the player is *shown* while paying an announced X is a separate,
/// still-open surface question — issue #776 item 5, in the client's §6.7 — and not a
/// reason to widen this signature.
#[must_use]
pub fn total_cast_cost(
    state: &GameState,
    db: &CardDatabase,
    card: crate::id::CardInstance,
) -> Option<crate::mana::ManaCost> {
    cast_cost(state, db, card, None).map(|(cost, _)| cost)
}
