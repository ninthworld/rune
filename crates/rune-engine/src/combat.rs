//! Combat declarations and combat damage: eligibility of attackers and blockers,
//! the turn-structure bookkeeping the declare steps need, and the combat-damage
//! assignment the combat-damage step performs.
//!
//! Declarations (issue #117): who *may* attack (CR 508.1a), who *may* block
//! (CR 509.1a), and which player owes the declaration in each declare step.
//! Attack targets (issue #341): each attacker is declared to attack a chosen
//! defending player — [`Permanent::attacking`] records *whom* (an
//! `Option<PlayerId>`), not a bare boolean. In a two-player game the sole opponent
//! is the only legal defender ([`defender_candidates`] returns exactly it), so
//! combat plays as it always has; with more seats each attacker picks an opponent,
//! blocker eligibility is scoped to attackers attacking that blocker's controller,
//! and combat damage routes to each attacker's own defender. The multi-defender
//! *declaration flow* (issue #344): when attackers are split across several
//! defenders, each attacked player declares blockers for the attackers attacking
//! them, in APNAP order ([`attacked_players`] / [`pending_blocker_declarer`]), and
//! combat damage is computed once after the final declaration.
//! Combat damage (issue #118, extended by #154): the assignment each attacker and
//! blocker makes in a combat-damage step (CR 510.1), gathered so it can be dealt
//! simultaneously (CR 510.2). First strike splits combat into two damage steps
//! (CR 510.5, keyed by [`DamageStep`]); trample (CR 702.19e), deathtouch
//! (CR 702.2b / 510.1e), and lifelink (CR 702.15e) shape the assignment within a
//! step. Double strike and player-chosen damage-assignment order are still out of
//! scope. Every function here is a pure predicate/enumeration over an immutable
//! [`GameState`] — no I/O, no mutation — consistent with the engine's rules.

use crate::card::Keyword;
use crate::card_type::CardType;
use crate::characteristics::characteristics;
use crate::id::{PermanentId, PlayerId};
use crate::phase::Step;
use crate::state::{GameState, Permanent};
use crate::CardDatabase;

/// The players an attacker may legally be declared to attack (CR 508.1a): every
/// opponent still in the game — a seat other than the active (attacking) player
/// that has not lost. In seat order, so the enumeration is deterministic.
///
/// In a two-player game this is exactly the sole opponent, so the only legal
/// assignment for every attacker is that one player and combat plays as it always
/// has. With more seats each attacker chooses among these candidates (issue #341).
/// A player may never attack themselves or an eliminated player, so neither is a
/// candidate.
#[must_use]
pub fn defender_candidates(state: &GameState) -> Vec<PlayerId> {
    state
        .players
        .iter()
        .enumerate()
        .filter(|(seat, player)| PlayerId(*seat) != state.active_player && !player.has_lost)
        .map(|(seat, _)| PlayerId(seat))
        .collect()
}

/// The single defending player of a two-player combat: the one opponent still in
/// the game (CR 508.1). `None` when there is not exactly one eligible defender —
/// on a state with fewer than two seats, or (once multiplayer combat lands) more
/// than one opponent, where there is no *single* defender and callers must consult
/// [`defender_candidates`] / each attacker's own [`Permanent::attacking`] target
/// instead. This keeps every two-player code path (blocker declaration flow,
/// server view binding) working unchanged while the multi-defender flow (#344)
/// builds on the per-attacker targets.
#[must_use]
pub fn defending_player(state: &GameState) -> Option<PlayerId> {
    let candidates = defender_candidates(state);
    match candidates.as_slice() {
        [only] => Some(*only),
        _ => None,
    }
}

/// Whether `perm` has summoning sickness for its controller (CR 302.6): it has
/// **not** been under that player's control continuously since their most recent
/// turn began.
///
/// Derived from [`Permanent::entered_turn`]: a permanent that entered on an
/// earlier turn than the current one was already in play when this turn began, so
/// it is not sick; one that entered this turn is. Written for the active player,
/// whose most recent turn is the current [`GameState::turn`] — the only player
/// who declares attackers in this slice.
#[must_use]
pub(crate) fn has_summoning_sickness(perm: &Permanent, state: &GameState) -> bool {
    perm.entered_turn >= state.turn
}

/// Whether `perm` is a creature by its printed card types. Type-changing
/// continuous effects are future work, so the printed types are authoritative
/// here (as they are in [`crate::resolve::target_is_legal`]).
#[must_use]
fn is_creature(perm: &Permanent, db: &CardDatabase) -> bool {
    db.card(perm.card)
        .is_some_and(|c| c.has_type(CardType::Creature))
}

/// Whether `perm` has printed keyword `keyword` (CR 702). Reads the printed card
/// data; keyword-granting continuous effects are future work, so the printed
/// keywords are authoritative here (as printed types are in [`is_creature`]).
#[must_use]
pub(crate) fn has_keyword(perm: &Permanent, keyword: Keyword, db: &CardDatabase) -> bool {
    db.card(perm.card).is_some_and(|c| c.has_keyword(keyword))
}

/// Whether the creature `blocker` may legally be assigned to block `attacker`
/// given evasion keywords (CR 509.1b): a creature with flying can be blocked only
/// by creatures with flying or reach (CR 702.9c, CR 702.17b). Any creature can
/// block a non-flying attacker.
///
/// Both ids are looked up on the battlefield; a missing permanent (a stale id)
/// yields `false`, so the caller rejects the assignment. This is a per-pair
/// predicate the block-legality check applies on top of the candidate-set
/// membership tests, so partial blocks of mixed flying/ground attackers stay
/// expressible — evasion is enforced in legality, not by hiding candidates.
#[must_use]
pub(crate) fn blocker_can_block_attacker(
    state: &GameState,
    attacker: PermanentId,
    blocker: PermanentId,
    db: &CardDatabase,
) -> bool {
    let Some(atk) = state.battlefield.iter().find(|p| p.id == attacker) else {
        return false;
    };
    // A non-flying attacker imposes no evasion constraint.
    if !has_keyword(atk, Keyword::Flying, db) {
        return true;
    }
    let Some(blk) = state.battlefield.iter().find(|p| p.id == blocker) else {
        return false;
    };
    // CR 702.9c / 702.17b: only flying or reach may block a flyer.
    has_keyword(blk, Keyword::Flying, db) || has_keyword(blk, Keyword::Reach, db)
}

/// The permanents the active player may legally declare as attackers right now
/// (CR 508.1a): creatures they control that are untapped and free of summoning
/// sickness (CR 302.6). In stable battlefield order.
///
/// This is the multi-select candidate set for the declare-attackers action — one
/// O(N) scan of the battlefield, never a product over selections. Haste (CR
/// 702.10b) exempts a creature from the summoning-sickness restriction; defender
/// and "can't attack" restrictions are not modeled yet.
#[must_use]
pub fn attacker_candidates(state: &GameState, db: &CardDatabase) -> Vec<PermanentId> {
    let active = state.active_player;
    state
        .battlefield
        .iter()
        .filter(|perm| {
            perm.controller == active
                && is_creature(perm, db)
                && !perm.tapped
                // CR 302.6, with the CR 702.10b haste exemption: a hasty creature
                // ignores the summoning-sickness attack restriction.
                && (!has_summoning_sickness(perm, state) || has_keyword(perm, Keyword::Haste, db))
        })
        .map(|perm| perm.id)
        .collect()
}

/// The permanents `defender` may legally declare as blockers right now
/// (CR 509.1a): untapped creatures they control (a tapped creature can't block).
/// In stable battlefield order.
///
/// This is the per-defender blocker candidate set: a player may block only with
/// their own creatures, and (enforced in the declaration's legality check, not
/// here) only against attackers attacking *them* (issue #341). The multi-defender
/// declaration flow (#344) calls this once per attacked player.
#[must_use]
pub fn blocker_candidates_for(
    state: &GameState,
    defender: PlayerId,
    db: &CardDatabase,
) -> Vec<PermanentId> {
    state
        .battlefield
        .iter()
        .filter(|perm| perm.controller == defender && is_creature(perm, db) && !perm.tapped)
        .map(|perm| perm.id)
        .collect()
}

/// The permanents the sole defending player of a two-player combat may legally
/// declare as blockers (CR 509.1a). Empty when there is no single defender (see
/// [`defending_player`]). A convenience over [`blocker_candidates_for`] for the
/// two-player declaration flow and server view binding; the multi-defender flow
/// (#344) uses [`blocker_candidates_for`] per attacked player.
#[must_use]
pub fn blocker_candidates(state: &GameState, db: &CardDatabase) -> Vec<PermanentId> {
    let Some(defender) = defending_player(state) else {
        return Vec::new();
    };
    blocker_candidates_for(state, defender, db)
}

/// The permanents currently declared as attackers, in stable battlefield order —
/// the legal set of creatures a blocker may be assigned to block (CR 509.1a).
#[must_use]
pub fn declared_attackers(state: &GameState) -> Vec<PermanentId> {
    state
        .battlefield
        .iter()
        .filter(|perm| perm.attacking.is_some())
        .map(|perm| perm.id)
        .collect()
}

/// Whom the permanent `attacker` is attacking this combat (CR 508.1a), or `None`
/// if it is not on the battlefield or is not an attacker. This is the defending
/// player its combat damage routes to and the player whose creatures may block it.
#[must_use]
pub fn attacking_defender_of(state: &GameState, attacker: PermanentId) -> Option<PlayerId> {
    state
        .battlefield
        .iter()
        .find(|p| p.id == attacker)
        .and_then(|p| p.attacking)
}

/// The players being attacked this combat, in APNAP order (CR 101.4) starting
/// from the active player — the distinct defenders named by the current
/// attackers. Each such player declares blockers for the attackers attacking
/// them (CR 509.1); the order they do so is this order.
///
/// A two-player combat yields at most the sole opponent. Deterministic and
/// reconstructable from state — no hidden iterator (issue #344).
#[must_use]
pub fn attacked_players(state: &GameState) -> Vec<PlayerId> {
    let n = state.players.len();
    if n == 0 {
        return Vec::new();
    }
    let mut ordered = Vec::new();
    for offset in 0..n {
        let seat = PlayerId((state.active_player.0 + offset) % n);
        let is_attacked = state
            .battlefield
            .iter()
            .any(|perm| perm.attacking == Some(seat));
        if is_attacked && !ordered.contains(&seat) {
            ordered.push(seat);
        }
    }
    ordered
}

/// The attacked player who owes the next declare-blockers decision, if any
/// (CR 509.1, APNAP-ordered per CR 101.4). `None` once every attacked player has
/// declared, or when there is nothing to declare.
///
/// Two-player games are unchanged: the sole opponent ([`defending_player`]) owes
/// the one declaration until [`GameState::blockers_declared`] is set — including
/// the empty declaration when no attackers were declared. With attackers split
/// across several defenders (issue #344), each attacked player still in the game
/// declares in turn; a defender already recorded in
/// [`GameState::blockers_declared_by`], or one who has been eliminated, is skipped.
#[must_use]
pub fn pending_blocker_declarer(state: &GameState) -> Option<PlayerId> {
    if state.blockers_declared {
        return None;
    }
    match defending_player(state) {
        // Two-player: the sole opponent owes the (possibly empty) declaration,
        // exactly as before.
        Some(sole) => Some(sole),
        // Multi-defender: the next attacked, still-in-the-game player who has not
        // yet declared, in APNAP order.
        None => attacked_players(state).into_iter().find(|seat| {
            !state.blockers_declared_by.contains(seat)
                && state.players.get(seat.0).is_some_and(|p| !p.has_lost)
        }),
    }
}

/// The player who owes a combat declaration in the current step, if any: the
/// active player during declare-attackers until attackers are declared
/// (CR 508.1), and, during declare-blockers, the next attacked player who owes a
/// blocker declaration (CR 509.1, APNAP-ordered — [`pending_blocker_declarer`]).
/// `None` in every other situation.
///
/// While a declaration is owed it is a turn-based *player choice*, so — like the
/// cleanup discard — only that player acts and the only action offered is the
/// declaration itself. Priority for the step's normal round is handed out only
/// once every owed declaration is made (see [`crate::apply_action`]).
#[must_use]
pub(crate) fn pending_declarer(state: &GameState) -> Option<PlayerId> {
    match state.step {
        Step::DeclareAttackers if !state.attackers_declared => Some(state.active_player),
        // Blockers first, then — once every attacked player has declared — the
        // attacking player's combat-damage assignment order for any multi-blocked
        // attacker (CR 510.1, issue #346).
        Step::DeclareBlockers => {
            pending_blocker_declarer(state).or_else(|| pending_damage_order(state))
        }
        _ => None,
    }
}

/// Who receives priority when the turn structure has just settled on a step: the
/// player owing that step's combat declaration if one is pending, otherwise the
/// active player (the ordinary case, CR 117.3a).
#[must_use]
pub(crate) fn priority_after_step_change(state: &GameState) -> PlayerId {
    pending_declarer(state).unwrap_or(state.active_player)
}

/// A single combat-damage assignment computed for a combat-damage step
/// (CR 510.1c). Kept as data to apply *after* every assignment is computed, so
/// all combat damage in the step is dealt at once (simultaneously, CR 510.2) — no
/// creature leaves combat partway through the batch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CombatDamage {
    /// Combat damage a creature deals to a player: an unblocked attacker, or the
    /// trample excess of a blocked one, striking the defending player (CR 510.1c,
    /// CR 702.19e).
    ToPlayer {
        /// The player the damage is dealt to.
        player: PlayerId,
        /// How much damage.
        amount: u32,
    },
    /// Combat damage a creature deals to another creature: an attacker to its
    /// blockers, or a blocker to the attacker it blocks (CR 510.1c). Marked on
    /// the permanent (CR 120.3).
    ToPermanent {
        /// The permanent the damage is marked on.
        permanent: PermanentId,
        /// How much damage.
        amount: u32,
        /// Whether the source has deathtouch (CR 702.2b): any nonzero such damage
        /// is lethal, so the recipient is flagged for the CR 704.5h state-based
        /// action when the batch is applied.
        deathtouch: bool,
    },
    /// Life a lifelink source's controller gains, dealt in the *same* batch as the
    /// damage that caused it so the gain is simultaneous with the damage event
    /// (CR 702.15e).
    GainLife {
        /// The player who gains the life (the damage source's controller).
        player: PlayerId,
        /// How much life is gained (equal to the damage dealt).
        amount: u32,
    },
}

/// Which combat-damage step is being computed (CR 510.5).
///
/// A creature deals its damage in exactly one step in this slice: first-strikers
/// in the first step, everyone else in the second. Double strike — a creature that
/// deals in *both* steps — is out of scope; when it lands it becomes a data
/// addition inside [`deals_in_step`], not a restructuring here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DamageStep {
    /// The single combat-damage step of an ordinary combat (no first strike is
    /// present): every creature in combat deals (CR 510.1).
    Only,
    /// The first-strike combat-damage step: only creatures with first strike deal
    /// (CR 510.5).
    FirstStrike,
    /// The regular combat-damage step that follows a first-strike step: creatures
    /// without first strike deal (CR 510.5).
    Regular,
}

/// Whether `perm` deals its combat damage in `step` (CR 510.5). In an ordinary
/// combat ([`DamageStep::Only`]) every creature deals; when a first-strike step is
/// present, a first-striker deals only in [`DamageStep::FirstStrike`] and every
/// other creature only in [`DamageStep::Regular`].
///
/// Double strike is the one addition this predicate is shaped for: a
/// double-striker would deal in *both* the first-strike and the regular step, so
/// it slots in as `has(FirstStrike) || has(DoubleStrike)` for the first step and
/// `has(DoubleStrike) || !has(FirstStrike)` for the regular one — no caller
/// changes.
#[must_use]
fn deals_in_step(perm: &Permanent, step: DamageStep, db: &CardDatabase) -> bool {
    match step {
        DamageStep::Only => true,
        DamageStep::FirstStrike => has_keyword(perm, Keyword::FirstStrike, db),
        DamageStep::Regular => !has_keyword(perm, Keyword::FirstStrike, db),
    }
}

/// Whether any creature currently in combat (attacking or blocking) has first
/// strike, so combat needs the two-step damage sequence (CR 510.5). When none do,
/// a single [`DamageStep::Only`] step suffices.
#[must_use]
pub(crate) fn combat_has_first_strike(state: &GameState, db: &CardDatabase) -> bool {
    state.battlefield.iter().any(|p| {
        (p.attacking.is_some() || p.blocking.is_some()) && has_keyword(p, Keyword::FirstStrike, db)
    })
}

/// The attackers that are *blocked* this combat — each has at least one creature
/// assigned to block it (CR 509.1h) — captured before any combat damage is dealt.
///
/// A creature stays blocked for the rest of combat even if its blockers later
/// leave (e.g. a first-struck blocker dies before the regular step): a blocked
/// attacker deals no damage to the defending player unless it has trample. This
/// snapshot is what the regular damage step consults so a blocked-but-now-
/// blockerless attacker is not mistaken for an unblocked one.
#[must_use]
pub(crate) fn blocked_attackers(state: &GameState) -> Vec<PermanentId> {
    state
        .battlefield
        .iter()
        .filter(|p| {
            p.attacking.is_some() && state.battlefield.iter().any(|b| b.blocking == Some(p.id))
        })
        .map(|p| p.id)
        .collect()
}

/// The current power of `id` as a non-negative amount of combat damage: a
/// creature assigns combat damage equal to its power (CR 510.1a), and a creature
/// with `0` or negative power (or none at all) assigns none. Reads current
/// power through [`characteristics`], so counters and anthems are folded in.
fn combat_power(state: &GameState, id: PermanentId, db: &CardDatabase) -> u32 {
    let power = characteristics(state, id, db).power.unwrap_or(0);
    u32::try_from(power.max(0)).unwrap_or(0)
}

/// The damage the assigning creature must put on blocker `id` to count as lethal
/// (CR 510.1c — an attacker assigns at least lethal damage to a blocker before the
/// next). Ordinarily this is the blocker's current toughness less any damage
/// already marked, floored at `0`; when the source has **deathtouch** it is just
/// `1` (any nonzero damage is lethal, CR 510.1e / 702.2b). `0` for a creature with
/// no toughness or already at/over lethal.
fn lethal_needed(state: &GameState, id: PermanentId, db: &CardDatabase, deathtouch: bool) -> u32 {
    let toughness = characteristics(state, id, db).toughness.unwrap_or(0);
    let marked = state
        .battlefield
        .iter()
        .find(|p| p.id == id)
        .map_or(0, |p| p.damage);
    let remaining =
        u32::try_from((toughness - i32::try_from(marked).unwrap_or(i32::MAX)).max(0)).unwrap_or(0);
    if deathtouch {
        // CR 510.1e: with deathtouch, 1 damage is lethal — but never assign to a
        // creature that already needs none.
        remaining.min(1)
    } else {
        remaining
    }
}

/// The blockers assigned to `attacker`, in the order combat damage is assigned
/// across them (see [`combat_damage`]): the attacking player's chosen
/// damage-assignment order (CR 510.1, issue #346) when one has been recorded for
/// this attacker, otherwise stable battlefield order. A chosen order is filtered to
/// the attacker's *current* blockers, so a blocker that has since left combat is
/// simply skipped and the rest keep their chosen sequence.
fn blockers_of(state: &GameState, attacker: PermanentId) -> Vec<PermanentId> {
    let battlefield_order = || -> Vec<PermanentId> {
        state
            .battlefield
            .iter()
            .filter(|p| p.blocking == Some(attacker))
            .map(|p| p.id)
            .collect()
    };
    match state.damage_orders.iter().find(|(atk, _)| *atk == attacker) {
        Some((_, order)) => order
            .iter()
            .copied()
            .filter(|blocker| {
                state
                    .battlefield
                    .iter()
                    .any(|p| p.id == *blocker && p.blocking == Some(attacker))
            })
            .collect(),
        None => battlefield_order(),
    }
}

/// The attackers whose controller still owes a combat-damage assignment order
/// (CR 510.1, issue #346): those blocked by two or more creatures whose order has
/// not yet been chosen (they are absent from [`GameState::damage_orders`]). An
/// attacker with zero or one blocker has no choice to make and never appears here.
/// In stable battlefield order.
#[must_use]
pub fn attackers_needing_damage_order(state: &GameState) -> Vec<PermanentId> {
    state
        .battlefield
        .iter()
        .filter(|p| p.attacking.is_some())
        .map(|p| p.id)
        .filter(|&atk| {
            blockers_of_unordered(state, atk).len() >= 2
                && !state.damage_orders.iter().any(|(a, _)| *a == atk)
        })
        .collect()
}

/// The attacker's blockers in bare battlefield order, ignoring any chosen order —
/// used to *count* blockers when deciding whether an ordering choice is owed
/// (independent of whether one has been made).
fn blockers_of_unordered(state: &GameState, attacker: PermanentId) -> Vec<PermanentId> {
    state
        .battlefield
        .iter()
        .filter(|p| p.blocking == Some(attacker))
        .map(|p| p.id)
        .collect()
}

/// The player who owes a combat-damage assignment order, if any (CR 510.1, issue
/// #346): the attacking (active) player, once every blocker declaration is in and at
/// least one attacker is multi-blocked without a chosen order. `None` once every
/// such attacker has been ordered, or when none is multi-blocked.
#[must_use]
pub fn pending_damage_order(state: &GameState) -> Option<PlayerId> {
    if state.blockers_declared && !attackers_needing_damage_order(state).is_empty() {
        Some(state.active_player)
    } else {
        None
    }
}

/// Record `amount` combat damage a `source_controller`'s creature deals to
/// `player`, plus the simultaneous lifelink life gain if the source has it
/// (CR 702.15e).
fn push_player_damage(
    out: &mut Vec<CombatDamage>,
    player: PlayerId,
    amount: u32,
    source_controller: PlayerId,
    lifelink: bool,
) {
    out.push(CombatDamage::ToPlayer { player, amount });
    if lifelink && amount > 0 {
        out.push(CombatDamage::GainLife {
            player: source_controller,
            amount,
        });
    }
}

/// Record `amount` combat damage a `source_controller`'s creature deals to
/// `permanent`, carrying the source's deathtouch flag (CR 702.2b) and adding the
/// simultaneous lifelink life gain if the source has it (CR 702.15e).
fn push_permanent_damage(
    out: &mut Vec<CombatDamage>,
    permanent: PermanentId,
    amount: u32,
    deathtouch: bool,
    source_controller: PlayerId,
    lifelink: bool,
) {
    out.push(CombatDamage::ToPermanent {
        permanent,
        amount,
        deathtouch,
    });
    if lifelink && amount > 0 {
        out.push(CombatDamage::GainLife {
            player: source_controller,
            amount,
        });
    }
}

/// Compute all combat damage for the combat-damage step `step` (CR 510.1): every
/// attacking and blocking creature that deals in this step assigns its power as
/// combat damage, gathered here so [`crate::apply_action`] can apply the whole
/// batch at once (simultaneously, CR 510.2).
///
/// `blocked` is the set of attackers blocked this combat ([`blocked_attackers`]),
/// captured before any damage so a blocked attacker whose blockers have since died
/// is still treated as blocked (CR 509.1h). `step` gates which creatures deal
/// (first strike splits combat in two, CR 510.5 — see [`deals_in_step`]).
///
/// - An **unblocked** attacker assigns its combat damage to the player it is
///   attacking — its own chosen defender (CR 510.1c / 508.1a), not a single global
///   defender, so split attacks route to the right seats. Lifelink gains its
///   controller that much life (CR 702.15e).
/// - A **blocked** attacker assigns its combat damage among its *surviving*
///   blockers in battlefield order, each just-lethal before the next
///   (deathtouch-aware, CR 510.1e); with **trample** any remainder is assigned to
///   the player it is attacking (CR 702.19e), otherwise it is left undealt.
///   Player-chosen damage-assignment order is still deferred.
/// - Each surviving blocker assigns its combat damage to the attacker it blocks
///   (CR 510.1c), carrying its own deathtouch/lifelink.
///
/// Deathtouch is recorded on each [`CombatDamage::ToPermanent`] so the CR 704.5h
/// state-based action can destroy a creature dealt any nonzero deathtouch damage.
/// Pure over the immutable state.
pub(crate) fn combat_damage(
    state: &GameState,
    db: &CardDatabase,
    step: DamageStep,
    blocked: &[PermanentId],
) -> Vec<CombatDamage> {
    let mut out = Vec::new();
    for attacker in state.battlefield.iter().filter(|p| p.attacking.is_some()) {
        // The player this attacker is attacking (CR 508.1a): its damage and any
        // trample overflow route here, not to a single global defender.
        let defender = attacker.attacking;
        let blockers = blockers_of(state, attacker.id);
        // The attacker's own strike, if it deals in this step.
        if deals_in_step(attacker, step, db) {
            let power = combat_power(state, attacker.id, db);
            let deathtouch = has_keyword(attacker, Keyword::Deathtouch, db);
            let lifelink = has_keyword(attacker, Keyword::Lifelink, db);
            let controller = attacker.controller;
            if !blocked.contains(&attacker.id) {
                // Unblocked: the attacker's damage goes to the player it attacks.
                if power > 0 {
                    if let Some(player) = defender {
                        push_player_damage(&mut out, player, power, controller, lifelink);
                    }
                }
            } else {
                // Blocked: spread across surviving blockers, lethal-per-blocker
                // (deathtouch-aware); trample sends the remainder to the player.
                let mut remaining = power;
                for blocker in &blockers {
                    if remaining == 0 {
                        break;
                    }
                    let assign = remaining.min(lethal_needed(state, *blocker, db, deathtouch));
                    if assign > 0 {
                        push_permanent_damage(
                            &mut out, *blocker, assign, deathtouch, controller, lifelink,
                        );
                        remaining -= assign;
                    }
                }
                // CR 702.19e: a trampler assigns its leftover to the defending
                // player; without trample a blocked creature deals it nowhere.
                if remaining > 0 && has_keyword(attacker, Keyword::Trample, db) {
                    if let Some(player) = defender {
                        push_player_damage(&mut out, player, remaining, controller, lifelink);
                    }
                }
            }
        }
        // Each surviving blocker deals its power back to the attacker, if it deals
        // in this step (CR 510.1c).
        for blocker in &blockers {
            let Some(bperm) = state.battlefield.iter().find(|p| p.id == *blocker) else {
                continue;
            };
            if !deals_in_step(bperm, step, db) {
                continue;
            }
            let bp = combat_power(state, *blocker, db);
            if bp > 0 {
                push_permanent_damage(
                    &mut out,
                    attacker.id,
                    bp,
                    has_keyword(bperm, Keyword::Deathtouch, db),
                    bperm.controller,
                    has_keyword(bperm, Keyword::Lifelink, db),
                );
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::fixtures::{fixture, id_in};
    use crate::id::CardId;
    use crate::state::Permanent;

    /// A first-strike attacker and a plain blocker/attacker, as an inline catalog —
    /// first strike and deathtouch have no clean M19 representative, so the combat
    /// tests that need those keywords build their own definitions (ADR 0026).
    fn keyword_db() -> CardDatabase {
        let json = r#"[
            {"schema_version":1,"functional_id":"test_duelist","name":"Test Duelist",
             "types":["creature"],"subtypes":["Human","Knight"],"mana_cost":"{1}{W}","colors":["white"],
             "power":2,"toughness":2,"keywords":["first_strike"]},
            {"schema_version":1,"functional_id":"test_adder","name":"Test Adder",
             "types":["creature"],"subtypes":["Snake"],"mana_cost":"{G}","colors":["green"],
             "power":1,"toughness":1,"keywords":["deathtouch"]},
            {"schema_version":1,"functional_id":"test_basilisk","name":"Test Basilisk",
             "types":["creature"],"subtypes":["Basilisk"],"mana_cost":"{4}{G}","colors":["green"],
             "power":4,"toughness":5},
            {"schema_version":1,"functional_id":"test_boar","name":"Test Boar",
             "types":["creature"],"subtypes":["Boar"],"mana_cost":"{2}{G}","colors":["green"],
             "power":3,"toughness":2}
        ]"#;
        CardDatabase::from_json(json).unwrap()
    }

    /// The bundled card database, for tests that need oracle data.
    fn db() -> CardDatabase {
        CardDatabase::bundled().unwrap()
    }

    /// Put a creature (Walking Corpse, a vanilla 2/2 with no combat keyword) on the
    /// battlefield under `controller` with the given tapped state, having entered on
    /// turn `entered_turn`.
    fn creature(
        state: &mut GameState,
        controller: PlayerId,
        tapped: bool,
        entered_turn: u32,
    ) -> PermanentId {
        let inst = state.new_instance(fixture("walking_corpse"));
        let id = PermanentId(state.mint_id());
        state.battlefield.push(Permanent {
            id,
            instance: inst.id,
            card: fixture("walking_corpse"),
            controller,
            tapped,
            entered_turn,
            attacking: None,
            blocking: None,
            damage: 0,
            counters: Default::default(),
            attached_to: None,
        });
        id
    }

    #[test]
    fn summoning_sickness_is_by_entry_turn_cr_302_6() {
        // CR 302.6: a creature that entered this turn is sick; one that entered a
        // previous turn is not.
        let mut state = GameState::new_two_player();
        state.turn = 3;
        let fresh = creature(&mut state, PlayerId(0), false, 3);
        let seasoned = creature(&mut state, PlayerId(0), false, 1);
        let fresh = state.battlefield.iter().find(|p| p.id == fresh).unwrap();
        let seasoned = state.battlefield.iter().find(|p| p.id == seasoned).unwrap();
        assert!(has_summoning_sickness(fresh, &state));
        assert!(!has_summoning_sickness(seasoned, &state));
    }

    #[test]
    fn attacker_candidates_exclude_sick_and_tapped_creatures_cr_508_1a() {
        // CR 508.1a / 302.6: only the active player's untapped, non-sick creatures
        // are eligible attackers.
        let mut state = GameState::new_two_player();
        state.turn = 2;
        let eligible = creature(&mut state, PlayerId(0), false, 1);
        let _sick = creature(&mut state, PlayerId(0), false, 2);
        let _tapped = creature(&mut state, PlayerId(0), true, 1);
        let _opponents = creature(&mut state, PlayerId(1), false, 1);

        assert_eq!(attacker_candidates(&state, &db()), vec![eligible]);
    }

    #[test]
    fn blocker_candidates_exclude_tapped_creatures_cr_509_1a() {
        // CR 509.1a: a tapped creature can't block. Only the defender's untapped
        // creatures are eligible; summoning sickness does not stop blocking.
        let mut state = GameState::new_two_player();
        state.turn = 2;
        let eligible = creature(&mut state, PlayerId(1), false, 2); // sick but can block
        let _tapped = creature(&mut state, PlayerId(1), true, 1);
        let _attackers_creature = creature(&mut state, PlayerId(0), false, 1);

        assert_eq!(blocker_candidates(&state, &db()), vec![eligible]);
    }

    #[test]
    fn defender_is_the_sole_opponent() {
        let state = GameState::new_two_player();
        assert_eq!(defending_player(&state), Some(PlayerId(1)));
        assert_eq!(defending_player(&GameState::default()), None);
    }

    /// Put a creature of printed card `card` on the battlefield under `controller`,
    /// untapped, entered on turn `entered_turn`; returns its fresh id. Used to
    /// place the keyword-bearing real cards (flying, reach, vigilance, haste).
    fn creature_card(
        state: &mut GameState,
        card: CardId,
        controller: PlayerId,
        entered_turn: u32,
    ) -> PermanentId {
        let inst = state.new_instance(card);
        let id = PermanentId(state.mint_id());
        state.battlefield.push(Permanent {
            id,
            instance: inst.id,
            card,
            controller,
            tapped: false,
            entered_turn,
            attacking: None,
            blocking: None,
            damage: 0,
            counters: Default::default(),
            attached_to: None,
        });
        id
    }

    #[test]
    fn issue_153_flying_can_be_blocked_only_by_flying_or_reach_cr_702_9c() {
        // CR 702.9c / 702.17b: a flyer can be blocked only by flying or reach.
        // Tested both directions: a ground creature cannot, flying and reach can.
        let db = db();
        let mut state = GameState::new_two_player();
        let flyer = creature_card(&mut state, fixture("snapping_drake"), PlayerId(0), 0); // flying
        let ground = creature_card(&mut state, fixture("walking_corpse"), PlayerId(1), 0); // no keyword
        let other_flyer = creature_card(&mut state, fixture("snapping_drake"), PlayerId(1), 0);
        let reacher = creature_card(&mut state, fixture("giant_spider"), PlayerId(1), 0); // reach

        assert!(
            !blocker_can_block_attacker(&state, flyer, ground, &db),
            "a ground creature cannot block a flyer (CR 702.9c)"
        );
        assert!(
            blocker_can_block_attacker(&state, flyer, other_flyer, &db),
            "a flyer can block a flyer (CR 702.9c)"
        );
        assert!(
            blocker_can_block_attacker(&state, flyer, reacher, &db),
            "a reach creature can block a flyer (CR 702.17b)"
        );

        // A non-flying attacker imposes no evasion constraint: the ground creature
        // can block a ground attacker.
        let ground_attacker = creature_card(&mut state, fixture("walking_corpse"), PlayerId(0), 0);
        assert!(blocker_can_block_attacker(
            &state,
            ground_attacker,
            ground,
            &db
        ));
    }

    #[test]
    fn issue_153_haste_creature_is_an_attacker_candidate_cr_702_10b() {
        // CR 702.10b: haste exempts a creature from the summoning-sickness attack
        // restriction, so one that entered this very turn may still attack. A
        // vanilla creature that entered this turn stays ineligible (CR 302.6).
        let db = db();
        let mut state = GameState::new_two_player();
        state.turn = 2;
        let hasty = creature_card(&mut state, fixture("volcanic_dragon"), PlayerId(0), 2); // haste, entered this turn
        let sick = creature_card(&mut state, fixture("walking_corpse"), PlayerId(0), 2); // entered this turn

        let candidates = attacker_candidates(&state, &db);
        assert!(
            candidates.contains(&hasty),
            "a hasty creature attacks the turn it enters (CR 702.10b)"
        );
        assert!(
            !candidates.contains(&sick),
            "a non-hasty creature that entered this turn cannot attack (CR 302.6)"
        );
    }

    // ----- Combat II: first strike / trample / deathtouch / lifelink (issue #154) -----

    /// Place an attacking creature of `card` under `controller` attacking the sole
    /// opponent (the two-player default); returns its id.
    fn attacker(state: &mut GameState, card: CardId, controller: PlayerId) -> PermanentId {
        let defender = defending_player(state).unwrap_or(PlayerId(1));
        attacker_of(state, card, controller, defender)
    }

    /// Place an attacking creature of `card` under `controller` attacking
    /// `defender`; returns its id. Used by the multi-defender combat tests.
    fn attacker_of(
        state: &mut GameState,
        card: CardId,
        controller: PlayerId,
        defender: PlayerId,
    ) -> PermanentId {
        let id = creature_card(state, card, controller, 0);
        if let Some(perm) = state.battlefield.iter_mut().find(|p| p.id == id) {
            perm.attacking = Some(defender);
        }
        id
    }

    /// Place a creature of `card` under `controller` blocking `blocks`; returns its id.
    fn blocker(
        state: &mut GameState,
        card: CardId,
        controller: PlayerId,
        blocks: PermanentId,
    ) -> PermanentId {
        let id = creature_card(state, card, controller, 0);
        if let Some(perm) = state.battlefield.iter_mut().find(|p| p.id == id) {
            perm.blocking = Some(blocks);
        }
        id
    }

    // Real M19 keyword bodies: flying/reach/vigilance/haste ride catalog cards;
    // first strike and deathtouch come from `keyword_db()` inline definitions.

    #[test]
    fn issue_154_first_strike_present_needs_two_damage_steps_cr_510_5() {
        // CR 510.5: any first striker in combat means two damage steps; without one
        // a single step suffices.
        let db = keyword_db();
        let mut state = GameState::new_two_player();
        let atk = attacker(&mut state, id_in(&db, "test_duelist"), PlayerId(0)); // first strike
        let _blk = blocker(&mut state, id_in(&db, "test_boar"), PlayerId(1), atk);
        assert!(combat_has_first_strike(&state, &db));

        let mut plain = GameState::new_two_player();
        let a = attacker(&mut plain, id_in(&db, "test_boar"), PlayerId(0));
        let _b = blocker(&mut plain, id_in(&db, "test_boar"), PlayerId(1), a);
        assert!(!combat_has_first_strike(&plain, &db));
    }

    #[test]
    fn issue_154_first_striker_deals_only_in_the_first_step_cr_510_5() {
        // CR 510.5: a first-strike attacker deals in the first-strike step; its
        // vanilla blocker deals in the regular step. `deals_in_step` gates each.
        let db = keyword_db();
        let mut state = GameState::new_two_player();
        let atk = attacker(&mut state, id_in(&db, "test_duelist"), PlayerId(0)); // first strike 2/2
        let blk = blocker(&mut state, id_in(&db, "test_boar"), PlayerId(1), atk); // vanilla 3/2
        let blocked = blocked_attackers(&state);

        // First-strike step: only the attacker deals (2 to the blocker).
        let first = combat_damage(&state, &db, DamageStep::FirstStrike, &blocked);
        assert_eq!(
            first,
            vec![CombatDamage::ToPermanent {
                permanent: blk,
                amount: 2,
                deathtouch: false,
            }]
        );
        // Regular step: only the (still-present, in this pure call) blocker deals.
        let regular = combat_damage(&state, &db, DamageStep::Regular, &blocked);
        assert_eq!(
            regular,
            vec![CombatDamage::ToPermanent {
                permanent: atk,
                amount: 3,
                deathtouch: false,
            }]
        );
    }

    #[test]
    fn issue_154_deathtouch_makes_one_damage_lethal_for_assignment_cr_510_1e() {
        // CR 510.1e / 702.2b: a deathtouch source needs assign only 1 to a blocker
        // to count as lethal. A 1/1 deathtouch attacker assigns 1 to a 4/5 blocker,
        // flagged deathtouch; the assignment records the deathtouch flag.
        let db = keyword_db();
        let mut state = GameState::new_two_player();
        let atk = attacker(&mut state, id_in(&db, "test_adder"), PlayerId(0)); // deathtouch 1/1
        let blk = blocker(&mut state, id_in(&db, "test_basilisk"), PlayerId(1), atk); // 4/5
        let blocked = blocked_attackers(&state);

        let batch = combat_damage(&state, &db, DamageStep::Only, &blocked);
        assert!(batch.contains(&CombatDamage::ToPermanent {
            permanent: blk,
            amount: 1,
            deathtouch: true,
        }));
        // The blocker deals its 4 back to the 1/1 attacker.
        assert!(batch.contains(&CombatDamage::ToPermanent {
            permanent: atk,
            amount: 4,
            deathtouch: false,
        }));
    }

    #[test]
    fn issue_154_trample_assigns_lethal_then_excess_to_the_player_cr_702_19e() {
        // CR 702.19e: a blocked trampler assigns just-lethal to its blocker, the
        // rest to the defending player. A 6/6 trampler over a 4/2 Ogre assigns 2
        // (lethal) to the Ogre and 4 to player 1.
        let db = db();
        let mut state = GameState::new_two_player();
        let atk = attacker(&mut state, fixture("colossal_dreadmaw"), PlayerId(0)); // trample 6/6
        let blk = blocker(&mut state, fixture("onakke_ogre"), PlayerId(1), atk); // 4/2
        let blocked = blocked_attackers(&state);

        let batch = combat_damage(&state, &db, DamageStep::Only, &blocked);
        assert!(batch.contains(&CombatDamage::ToPermanent {
            permanent: blk,
            amount: 2,
            deathtouch: false,
        }));
        assert!(batch.contains(&CombatDamage::ToPlayer {
            player: PlayerId(1),
            amount: 4,
        }));
    }

    #[test]
    fn issue_154_deathtouch_trample_assigns_one_per_blocker_rest_to_player() {
        // CR 510.1e + 702.19e together: a deathtouch trampler needs assign only 1
        // per blocker before the rest tramples over. The assignment math is verified
        // by exercising `lethal_needed` directly against a blocker — deathtouch makes
        // 1 lethal, otherwise its full toughness is.
        let db = db();
        let mut state = GameState::new_two_player();
        let blk = creature_card(&mut state, fixture("giant_spider"), PlayerId(1), 0); // 2/4
        assert_eq!(
            lethal_needed(&state, blk, &db, true),
            1,
            "deathtouch: 1 is lethal"
        );
        assert_eq!(
            lethal_needed(&state, blk, &db, false),
            4,
            "without deathtouch: full toughness is lethal"
        );
    }

    #[test]
    fn issue_154_lifelink_gains_its_controller_life_in_the_same_batch_cr_702_15e() {
        // CR 702.15e: a lifelink source's controller gains life equal to the damage,
        // recorded in the same batch (so it is simultaneous when applied). An
        // unblocked 2/1 lifelinker attacking player 1 hits for 2 and gains 2.
        let db = db();
        let mut state = GameState::new_two_player();
        let _atk = attacker(&mut state, fixture("child_of_night"), PlayerId(0)); // lifelink 2/1
        let blocked = blocked_attackers(&state);

        let batch = combat_damage(&state, &db, DamageStep::Only, &blocked);
        assert!(batch.contains(&CombatDamage::ToPlayer {
            player: PlayerId(1),
            amount: 2,
        }));
        assert!(batch.contains(&CombatDamage::GainLife {
            player: PlayerId(0),
            amount: 2,
        }));
    }

    #[test]
    fn issue_154_blocked_attacker_stays_blocked_when_its_blockers_leave() {
        // CR 509.1h: an attacker recorded as blocked deals no player damage even
        // once its blockers are gone (no trample). Removing the blocker after
        // capturing the blocked set leaves the attacker dealing nothing.
        let db = db();
        let mut state = GameState::new_two_player();
        let atk = attacker(&mut state, fixture("onakke_ogre"), PlayerId(0)); // vanilla 3/2, no trample
        let blk = blocker(&mut state, fixture("onakke_ogre"), PlayerId(1), atk);
        let blocked = blocked_attackers(&state);
        assert_eq!(blocked, vec![atk]);

        // The blocker dies before damage: remove it, keep the blocked snapshot.
        state.battlefield.retain(|p| p.id != blk);
        let batch = combat_damage(&state, &db, DamageStep::Only, &blocked);
        assert!(
            batch.is_empty(),
            "a blocked non-trampler with no surviving blockers deals nothing"
        );
    }

    // ----- Multiplayer combat: per-attacker attack targets (issue #341) -----

    #[test]
    fn issue_341_defender_candidates_are_every_living_opponent_cr_508_1a() {
        // CR 508.1a: an attacker may be declared to attack any opponent still in the
        // game — never the active player, never an eliminated one.
        let mut state = GameState::new_multiplayer(3);
        state.active_player = PlayerId(0);
        assert_eq!(
            defender_candidates(&state),
            vec![PlayerId(1), PlayerId(2)],
            "both opponents of the active player are candidates"
        );
        // A two-player game has exactly one defender candidate — the sole opponent —
        // so `defending_player` resolves and combat plays as it always has.
        let two = GameState::new_two_player();
        assert_eq!(defender_candidates(&two), vec![PlayerId(1)]);
        assert_eq!(defending_player(&two), Some(PlayerId(1)));
        // With more than one opponent there is no single defender.
        assert_eq!(defending_player(&state), None);

        // An eliminated opponent drops out of the candidate set.
        state.players[1].has_lost = true;
        assert_eq!(defender_candidates(&state), vec![PlayerId(2)]);
    }

    #[test]
    fn issue_341_split_attacks_route_damage_to_each_chosen_defender() {
        // CR 510.1c: with attackers split across two defenders, each unblocked
        // attacker's damage goes to *its own* chosen defender, not one global one.
        let db = db();
        let mut state = GameState::new_multiplayer(3);
        // Seat 0 attacks: a 4/2 at seat 1 and a 4/2 at seat 2, both unblocked.
        let _at1 = attacker_of(&mut state, fixture("onakke_ogre"), PlayerId(0), PlayerId(1));
        let _at2 = attacker_of(&mut state, fixture("onakke_ogre"), PlayerId(0), PlayerId(2));
        let blocked = blocked_attackers(&state);
        assert!(blocked.is_empty());

        let batch = combat_damage(&state, &db, DamageStep::Only, &blocked);
        assert!(
            batch.contains(&CombatDamage::ToPlayer {
                player: PlayerId(1),
                amount: 4,
            }),
            "the attacker assigned to seat 1 hits seat 1"
        );
        assert!(
            batch.contains(&CombatDamage::ToPlayer {
                player: PlayerId(2),
                amount: 4,
            }),
            "the attacker assigned to seat 2 hits seat 2"
        );
    }

    #[test]
    fn issue_341_trample_overflow_routes_to_the_attackers_own_defender() {
        // CR 702.19e: a blocked trampler's overflow goes to the player it is
        // attacking. A 6/6 trampler at seat 2, blocked by seat 2's 4/2, assigns 2
        // (lethal) to the blocker and tramples 4 to seat 2 — never seat 1.
        let db = db();
        let mut state = GameState::new_multiplayer(3);
        let atk = attacker_of(
            &mut state,
            fixture("colossal_dreadmaw"),
            PlayerId(0),
            PlayerId(2),
        ); // trample 6/6
        let blk = blocker(&mut state, fixture("onakke_ogre"), PlayerId(2), atk); // 4/2
        let blocked = blocked_attackers(&state);

        let batch = combat_damage(&state, &db, DamageStep::Only, &blocked);
        assert!(batch.contains(&CombatDamage::ToPermanent {
            permanent: blk,
            amount: 2,
            deathtouch: false,
        }));
        assert!(
            batch.contains(&CombatDamage::ToPlayer {
                player: PlayerId(2),
                amount: 4,
            }),
            "trample overflow hits the attacker's own defender (seat 2)"
        );
        assert!(
            !batch.contains(&CombatDamage::ToPlayer {
                player: PlayerId(1),
                amount: 4,
            }),
            "no damage leaks to the other opponent (seat 1)"
        );
    }

    #[test]
    fn issue_341_blocker_candidates_are_per_defender() {
        // Blocker candidates for a defending player include only that player's own
        // untapped creatures (issue #341); the per-attacker scoping is enforced in
        // the declaration's legality check.
        let db = db();
        let mut state = GameState::new_multiplayer(3);
        let seat1_creature = creature_card(&mut state, fixture("walking_corpse"), PlayerId(1), 0);
        let seat2_creature = creature_card(&mut state, fixture("walking_corpse"), PlayerId(2), 0);

        assert_eq!(
            blocker_candidates_for(&state, PlayerId(1), &db),
            vec![seat1_creature]
        );
        assert_eq!(
            blocker_candidates_for(&state, PlayerId(2), &db),
            vec![seat2_creature]
        );
    }

    // ----- Multi-defender declaration flow (issue #344) -----

    #[test]
    fn issue_344_attacked_players_are_in_apnap_order() {
        // CR 101.4: attacked players declare in APNAP order from the active player.
        // Seat 0 attacks seats 2 and 1 (declared in that order); APNAP order is 1
        // then 2, regardless of declaration order.
        let mut state = GameState::new_multiplayer(4);
        state.active_player = PlayerId(0);
        attacker_of(&mut state, fixture("onakke_ogre"), PlayerId(0), PlayerId(2));
        attacker_of(&mut state, fixture("onakke_ogre"), PlayerId(0), PlayerId(1));
        assert_eq!(attacked_players(&state), vec![PlayerId(1), PlayerId(2)]);

        // Seat 3 is not attacked, so it is not in the list.
        assert!(!attacked_players(&state).contains(&PlayerId(3)));
    }

    #[test]
    fn issue_344_pending_declarer_walks_each_attacked_player_then_finishes() {
        let mut state = GameState::new_multiplayer(3);
        state.step = Step::DeclareBlockers;
        state.active_player = PlayerId(0);
        state.attackers_declared = true;
        attacker_of(&mut state, fixture("onakke_ogre"), PlayerId(0), PlayerId(1));
        attacker_of(&mut state, fixture("onakke_ogre"), PlayerId(0), PlayerId(2));

        // Seat 1 owes the first declaration (APNAP), then seat 2, then none.
        assert_eq!(pending_blocker_declarer(&state), Some(PlayerId(1)));
        state.blockers_declared_by.push(PlayerId(1));
        assert_eq!(pending_blocker_declarer(&state), Some(PlayerId(2)));
        state.blockers_declared_by.push(PlayerId(2));
        assert_eq!(pending_blocker_declarer(&state), None);
    }

    #[test]
    fn issue_344_defender_eliminated_before_declaring_is_skipped() {
        // A defender eliminated in the declare window is skipped without stalling
        // combat: the next attacked player owes the declaration instead.
        let mut state = GameState::new_multiplayer(3);
        state.step = Step::DeclareBlockers;
        state.active_player = PlayerId(0);
        state.attackers_declared = true;
        attacker_of(&mut state, fixture("onakke_ogre"), PlayerId(0), PlayerId(1));
        attacker_of(&mut state, fixture("onakke_ogre"), PlayerId(0), PlayerId(2));

        state.players[1].has_lost = true; // seat 1 dies in the declare window
        assert_eq!(
            pending_blocker_declarer(&state),
            Some(PlayerId(2)),
            "the eliminated defender is skipped; seat 2 declares"
        );
    }
}
