//! Room-layer policy for decision timers and priority automation (issues #263,
//! #264), plus the conservative default action a timeout takes.
//!
//! The engine is pure and both timer- and automation-free (ADR 0002); these
//! policies and the [`timeout_default_action`] helper live in the room layer, which
//! already owns tokio time and the settle loop. Pure code motion out of the room
//! module root (issue #427) — no behavior change.

use std::time::Duration;

use rune_engine::{
    attackers_needing_damage_order, valid_actions, Action, CardDatabase, DamageOrder, GameState,
};

/// A room's decision-timer policy (issue #263).
///
/// The engine is pure and timer-free (ADR 0002); deadline policy and enforcement
/// live here in the room layer, which already owns tokio time. Timers are **off by
/// default** — an off policy reproduces exactly the pre-timer behavior, so existing
/// flows and tests are unchanged — and, when on, apply only to in-game decisions;
/// the lobby/deck-submission phase is explicitly out of scope (a room only exists
/// once a game has been constructed).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TimerPolicy {
    /// No decision clock: a seat may take as long as it likes (the default, and the
    /// behavior before timers existed).
    #[default]
    Off,
    /// Each in-game decision must be answered within `limit`; on expiry the room
    /// takes a conservative default action on the deciding player's behalf (see
    /// [`timeout_default_action`]).
    PerDecision {
        /// How long the deciding player has before the default action fires.
        limit: Duration,
    },
}

/// The conservative default action the room takes when a decision times out
/// (issue #263). This is deliberately a *safe no-op-ish* choice, never a
/// game-losing one — a single missed prompt must not concede (CR 104.3a is reserved
/// for an explicit concession or a future idle-escalation policy):
///
/// - In an ordinary priority window, **pass priority** — the universal safe default.
/// - For a forced combat declaration, declare **no** attackers/blockers (CR 508.1a /
///   509.1a both allow the empty declaration).
/// - Any other forced decision (mulligan keep/mulligan, cleanup discard) has no safe
///   auto-answer, so the timer does not force it — the room stops the clock for that
///   decision rather than guess (idle-escalation is future work). Returns `None`.
///
/// All legality is still enforced by [`apply_action`](rune_engine::apply_action);
/// this only picks *which* offered action to take, reading the engine's own
/// [`valid_actions`].
pub(super) fn timeout_default_action(state: &GameState, db: &CardDatabase) -> Option<Action> {
    let actions = valid_actions(state, db);
    if actions.iter().any(|a| matches!(a, Action::PassPriority)) {
        return Some(Action::PassPriority);
    }
    if actions
        .iter()
        .any(|a| matches!(a, Action::DeclareAttackers { .. }))
    {
        return Some(Action::DeclareAttackers {
            attackers: Vec::new(),
        });
    }
    if actions
        .iter()
        .any(|a| matches!(a, Action::DeclareBlockers { .. }))
    {
        return Some(Action::DeclareBlockers { blocks: Vec::new() });
    }
    if actions
        .iter()
        .any(|a| matches!(a, Action::OrderCombatDamage { .. }))
    {
        // Combat-damage assignment order (issue #346): resolve to the deterministic
        // battlefield-order default — the exact assignment used before player choice
        // existed — so an unattended game never stalls and never concedes.
        let orders = attackers_needing_damage_order(state)
            .into_iter()
            .map(|attacker| DamageOrder {
                attacker,
                blockers: state
                    .battlefield
                    .iter()
                    .filter(|p| p.blocking == Some(attacker))
                    .map(|p| p.id)
                    .collect(),
            })
            .collect();
        return Some(Action::OrderCombatDamage { orders });
    }
    None
}

/// A room's basic priority-automation policy (issue #264, ADR 0020).
///
/// Like [`TimerPolicy`], automation is a room-layer concern layered over the pure,
/// automation-free engine: the engine only *reports* (via
/// [`priority_has_no_meaningful_action`](rune_engine::priority_has_no_meaningful_action))
/// whether the priority holder has a meaningful action; the room owns the loop that
/// acts on it. **Off by default** — an off policy reproduces exactly the
/// pre-automation behavior, so every existing flow and test is unchanged — and, when
/// on, auto-passes a seat's priority while the engine says it is idle and the seat
/// has not opted to stop at the current step (its `set_stops` preferences, held per
/// seat on the room).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AutoPassPolicy {
    /// No automation: every priority pass is manual (the default, and the behavior
    /// before automation existed).
    #[default]
    Off,
    /// Auto-pass an idle seat's priority (per its stop preferences).
    On,
}
