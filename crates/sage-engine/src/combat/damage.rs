use crate::card::Keyword;
use crate::id::{PermanentId, PlayerId};
use crate::state::GameState;
use crate::CardDatabase;

use super::assigned_combat_damage;
use super::helpers::{
    deals_in_step, has_keyword, lethal_needed, push_permanent_damage, push_player_damage,
};

/// A single combat-damage assignment computed for a combat-damage step
/// (CR 510.1c). Kept as data to apply *after* every assignment is computed, so
/// all combat damage in the step is dealt at once (simultaneously, CR 510.2) — no
/// creature leaves combat partway through the batch.
///
/// **Lifelink rides the assignment rather than sitting beside it.** CR 702.15e gains
/// life equal to the damage that was *dealt*, and how much that is cannot be known until
/// the damage has been through the one seam that deals it — a prevention shield
/// (CR 615.1) can leave nothing of it. A separate `gain N life` entry would have been
/// computed from the damage that was *assigned*, which is a different number the moment
/// anything prevents any of it.
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
        /// The commander designation of the source, if the striking creature is a
        /// commander — its owning [`PlayerId`], the stable tally key (CR 903.10a).
        /// `None` for an ordinary source. When set, the batch application adds this
        /// hit to the CR 903.10a commander-damage tally
        /// ([`GameState::add_commander_damage`]); a bare life change alone would
        /// lose the "which commander dealt it" fact the 21-damage loss needs.
        source_commander: Option<PlayerId>,
        /// The player a lifelink source's damage gains life (CR 702.15e), `None` when
        /// the source has no lifelink.
        lifelink: Option<PlayerId>,
    },
    /// Combat damage a creature deals to another **permanent**: an attacker to its
    /// blockers, a blocker to the attacker it blocks (CR 510.1c), or an attacker to the
    /// planeswalker it is attacking. What the damage *does* is decided at the one
    /// damage seam it is applied through ([`GameState::deal_damage`]) —
    /// marked on a creature (CR 120.3d), loyalty removed from a planeswalker
    /// (CR 120.3c) — so this assignment stays a plain "this much, to that object".
    ToPermanent {
        /// The permanent the damage is dealt to.
        permanent: PermanentId,
        /// How much damage.
        amount: u32,
        /// Whether the source has deathtouch (CR 702.2b): any nonzero such damage
        /// is lethal, so the recipient is flagged for the CR 704.5h state-based
        /// action when the batch is applied.
        deathtouch: bool,
        /// The player a lifelink source's damage gains life (CR 702.15e), `None` when
        /// the source has no lifelink.
        lifelink: Option<PlayerId>,
    },
}

/// Which combat-damage step is being computed (CR 510.5).
///
/// Most creatures deal in exactly one step: first-strikers in the first step,
/// everyone else in the second. A creature with double strike (CR 702.4b) deals in
/// *both* the first-strike and the regular step — the one creature [`deals_in_step`]
/// admits to both.
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

/// Whether any creature currently in combat (attacking or blocking) has first
/// strike **or double strike**, so combat needs the two-step damage sequence
/// (CR 510.5). A double striker deals in the first-strike step too (CR 702.4b), so
/// its mere presence splits combat in two even when no creature has plain first
/// strike. When none qualify, a single [`DamageStep::Only`] step suffices.
#[must_use]
pub(crate) fn combat_has_first_strike(state: &GameState, db: &CardDatabase) -> bool {
    state.battlefield.iter().any(|p| {
        (p.attacking.is_some() || !p.blocking.is_empty())
            && (has_keyword(state, p, Keyword::FirstStrike, db)
                || has_keyword(state, p, Keyword::DoubleStrike, db))
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
            p.attacking.is_some() && state.battlefield.iter().any(|b| b.blocking.contains(&p.id))
        })
        .map(|p| p.id)
        .collect()
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
            .filter(|p| p.blocking.contains(&attacker))
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
                    .any(|p| p.id == *blocker && p.blocking.contains(&attacker))
            })
            .collect(),
        None => battlefield_order(),
    }
}

/// Record `amount` combat damage an attacker deals to whatever it is attacking
/// (CR 508.1a): a player, or a planeswalker.
///
/// The one place the two attack targets diverge, and the divergence is small on purpose
/// — a player takes it as life loss with the CR 903.10a commander tally attached, a
/// planeswalker takes it as a permanent through the same assignment a blocker's damage
/// uses. Lifelink rides either (CR 702.15e), which is why both arms go through the
/// existing push helpers rather than pushing directly.
///
/// A planeswalker that has already left the battlefield produces no assignment: the
/// `defending_player` lookup fails, and there is nothing to deal damage to.
fn push_attack_target_damage(
    out: &mut Vec<CombatDamage>,
    state: &GameState,
    target: super::AttackTarget,
    amount: u32,
    deathtouch: bool,
    lifelink: Option<PlayerId>,
    source_commander: Option<PlayerId>,
) {
    match target {
        super::AttackTarget::Player(player) => {
            push_player_damage(out, player, amount, lifelink, source_commander);
        }
        super::AttackTarget::Planeswalker(id) => {
            // Only a planeswalker still on the battlefield takes anything; one that has
            // died mid-combat leaves its attacker dealing damage nowhere (CR 506.4).
            if state.battlefield.iter().any(|p| p.id == id) {
                push_permanent_damage(out, id, amount, deathtouch, lifelink);
            }
        }
    }
}

/// Compute all combat damage for the combat-damage step `step` (CR 510.1): every
/// attacking and blocking creature that deals in this step assigns
/// [`assigned_combat_damage`] — its power (CR 510.1a), or the characteristic a
/// continuous effect substitutes for it — gathered here so [`crate::apply_action`] can
/// apply the whole batch at once (simultaneously, CR 510.2).
///
/// The substitution is read once, at the top of each creature's arm, and everything below
/// is about the amount rather than about where it came from. That is why trample's excess
/// and the marked damage the lethal-damage state-based action reads both follow the
/// override with no clause of their own.
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
/// - Each surviving blocker assigns its combat damage among the attackers it blocks
///   (CR 510.1c), carrying its own deathtouch/lifelink. Blocking one attacker — the
///   ordinary case — is the whole of its assignment to that attacker; blocking several
///   (CR 509.1a, the "block an additional creature" permission) spreads it along the
///   blocker's own damage assignment order, which is the order its declaration named
///   them in ([`Permanent::blocking`](crate::Permanent::blocking), CR 509.3).
///
/// Deathtouch is recorded on each [`CombatDamage::ToPermanent`] so the CR 704.5h
/// state-based action can destroy a creature dealt any nonzero deathtouch damage.
/// Pure over the immutable state.
#[cfg(test)]
pub(crate) fn combat_damage(
    state: &GameState,
    db: &CardDatabase,
    step: DamageStep,
    blocked: &[PermanentId],
) -> Vec<CombatDamage> {
    combat_damage_and_dealers(state, db, step, blocked).0
}

/// [`combat_damage`], paired with **who assigned each entry of it** — the attribution
/// `Palladia-Mors, the Ruiner`'s "hasn't dealt damage yet" reads
/// ([`Permanent::dealt_damage`](crate::Permanent)).
///
/// A second return value rather than a `source` field on every [`CombatDamage`] because
/// the assignment is about what one object *receives*: three creatures blocking one
/// attacker take three separate assignments, and who dealt them is a fact about the batch,
/// not about any one of its entries. Each pair is a creature and the **index** of an
/// assignment pushed on its behalf, so a blocked non-trampler whose blockers all died —
/// which assigns its damage nowhere — appears not at all.
///
/// The index is what makes the answer survive CR 615.1: an assignment is not yet damage
/// dealt, and only the applier knows which of them a prevention shield let through
/// (`apply.rs :: apply_step`). Listing a creature here says it assigned something, never
/// that it dealt it.
pub(crate) fn combat_damage_and_dealers(
    state: &GameState,
    db: &CardDatabase,
    step: DamageStep,
    blocked: &[PermanentId],
) -> (Vec<CombatDamage>, Vec<(PermanentId, usize)>) {
    let mut out = Vec::new();
    let mut dealers: Vec<(PermanentId, usize)> = Vec::new();
    let note = |dealers: &mut Vec<(PermanentId, usize)>, id: PermanentId, assignment: usize| {
        dealers.push((id, assignment));
    };
    for attacker in state.battlefield.iter().filter(|p| p.attacking.is_some()) {
        // What this attacker is attacking (CR 508.1a): its damage and any trample
        // overflow route here, not to a single global defender. A planeswalker that has
        // since left the battlefield leaves nothing to route to, and the assignment is
        // simply not made — an attacker with no target deals its damage nowhere.
        let defender = attacker.attacking;
        let blockers = blockers_of(state, attacker.id);
        // The attacker's own strike, if it deals in this step.
        if deals_in_step(state, attacker, step, db) {
            let assigned = assigned_combat_damage(state, attacker.id, db);
            let deathtouch = has_keyword(state, attacker, Keyword::Deathtouch, db);
            let controller = crate::characteristics::controller_of(state, attacker);
            // CR 702.15e gains the *source's controller* the life, so the keyword and
            // the seat travel together — one `Some(seat)` rather than a flag beside a
            // player the applier would have to pair back up.
            let lifelink =
                has_keyword(state, attacker, Keyword::Lifelink, db).then_some(controller);
            // CR 903.10a: whether this attacker is a commander (identified by its
            // stable instance → designation), so its damage to a player counts
            // toward the 21-combat-damage loss. `None` for an ordinary creature.
            let source_commander = state.commander_owner_of(attacker.instance);
            if !blocked.contains(&attacker.id) {
                // Unblocked: the attacker's damage goes to what it attacks.
                if assigned > 0 {
                    if let Some(target) = defender {
                        let before = out.len();
                        push_attack_target_damage(
                            &mut out,
                            state,
                            target,
                            assigned,
                            deathtouch,
                            lifelink,
                            source_commander,
                        );
                        // A planeswalker that has already left takes nothing and no
                        // assignment is pushed, so the attacker assigned nothing — which
                        // is exactly what the length says.
                        if out.len() > before {
                            note(&mut dealers, attacker.id, before);
                        }
                    }
                }
            } else {
                // Blocked: spread across surviving blockers, lethal-per-blocker
                // (deathtouch-aware); trample sends the remainder to the player.
                let mut remaining = assigned;
                for blocker in &blockers {
                    if remaining == 0 {
                        break;
                    }
                    let assign = remaining.min(lethal_needed(state, *blocker, db, deathtouch));
                    if assign > 0 {
                        push_permanent_damage(&mut out, *blocker, assign, deathtouch, lifelink);
                        note(&mut dealers, attacker.id, out.len() - 1);
                        remaining -= assign;
                    }
                }
                // CR 702.19e: a trampler assigns its leftover to whatever it is
                // attacking — the defending player, or the planeswalker's loyalty;
                // without trample a blocked creature deals it nowhere. A trampling
                // commander's overflow still counts toward CR 903.10a, but only when it
                // reaches a player: loyalty is not life.
                if remaining > 0 && has_keyword(state, attacker, Keyword::Trample, db) {
                    if let Some(target) = defender {
                        let before = out.len();
                        push_attack_target_damage(
                            &mut out,
                            state,
                            target,
                            remaining,
                            deathtouch,
                            lifelink,
                            source_commander,
                        );
                        if out.len() > before {
                            note(&mut dealers, attacker.id, before);
                        }
                    }
                }
            }
        }
    }
    // Each surviving blocker assigns its combat damage among the attackers it blocks
    // (CR 510.1c). Driven from the blocker rather than from inside the attacker loop
    // because a blocker may block more than one attacker (CR 509.1a) and its assignment
    // is one pool spread across all of them — an attacker-driven loop would hand the same
    // pool out once per attacker.
    for blocker in state.battlefield.iter().filter(|p| !p.blocking.is_empty()) {
        if !deals_in_step(state, blocker, step, db) {
            continue;
        }
        let assigned = assigned_combat_damage(state, blocker.id, db);
        if assigned == 0 {
            continue;
        }
        let deathtouch = has_keyword(state, blocker, Keyword::Deathtouch, db);
        let lifelink = has_keyword(state, blocker, Keyword::Lifelink, db)
            .then_some(crate::characteristics::controller_of(state, blocker));
        // The attackers it blocks, in its own damage assignment order (CR 509.3 — the
        // order its declaration named them in), minus any that have left the
        // battlefield: an attacker that died to first strike takes nothing further.
        let attackers: Vec<PermanentId> = blocker
            .blocking
            .iter()
            .copied()
            .filter(|atk| state.battlefield.iter().any(|p| p.id == *atk))
            .collect();
        let mut remaining = assigned;
        for (index, attacker) in attackers.iter().enumerate() {
            if remaining == 0 {
                break;
            }
            // Just-lethal to each attacker before the next (CR 510.1e, deathtouch-aware),
            // and everything still unassigned to the last one in the order — which is
            // what CR 510.1d permits once every creature ahead of it has lethal, and what
            // makes the single-attacker case a plain full-assignment hit, exactly as it was
            // before a blocker could block two.
            let assign = if index + 1 == attackers.len() {
                remaining
            } else {
                remaining.min(lethal_needed(state, *attacker, db, deathtouch))
            };
            if assign > 0 {
                push_permanent_damage(&mut out, *attacker, assign, deathtouch, lifelink);
                note(&mut dealers, blocker.id, out.len() - 1);
                remaining -= assign;
            }
        }
    }
    (out, dealers)
}

#[cfg(test)]
pub(crate) mod tests;
