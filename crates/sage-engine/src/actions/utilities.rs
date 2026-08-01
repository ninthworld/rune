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

/// Whether every cost in `cost` is payable right now, given the source
/// `permanent`'s state and its controller's mana pool.
///
/// Mana affordability is decided by the same [`ManaPool::can_pay`](crate::ManaPool::can_pay)
/// the cast path uses over the same `{...}` notation, so an ability is offered
/// exactly when [`crate::apply_action`] will succeed in charging for it — the
/// offer and the charge can never disagree about a cost string.
pub(crate) fn cost_payable(state: &GameState, cost: &[Cost], permanent: &Permanent) -> bool {
    cost.iter().all(|c| match c {
        Cost::Tap => !permanent.tapped,
        Cost::Mana { mana } => state
            .players
            .get(permanent.controller.0)
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
    let sorcery_speed = permanent.controller == state.active_player
        && matches!(
            state.step,
            crate::phase::Step::PrecombatMain | crate::phase::Step::PostcombatMain
        )
        && state.stack.is_empty();
    sorcery_speed && !state.loyalty_activations.contains(&permanent.id)
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
        if perm.controller != player || perm.tapped {
            continue;
        }
        for ability in abilities_of_permanent(db, perm) {
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
