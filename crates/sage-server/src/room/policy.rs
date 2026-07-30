//! Room-layer policy for decision timers and priority automation (issues #263,
//! #264), plus the conservative default action a timeout takes.
//!
//! The engine is pure and both timer- and automation-free (ADR 0002); these
//! policies and the [`timeout_default_action`] helper live in the room layer, which
//! already owns tokio time and the settle loop. Pure code motion out of the room
//! module root (issue #427) — no behavior change.

use std::time::Duration;

use sage_engine::{
    attackers_needing_damage_order, valid_actions, Action, CardDatabase, DamageOrder, GameState,
};
use sage_protocol::Phase;

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
/// All legality is still enforced by [`apply_action`](sage_engine::apply_action);
/// this only picks *which* offered action to take, reading the engine's own
/// [`valid_actions`].
///
/// Not to be confused with the settle loop's
/// [`forced_declaration_without_choice`](sage_engine::forced_declaration_without_choice)
/// (issue #453), which produces the same empty declarations for a deliberately
/// different reason. This function answers "the player did not respond in time, so
/// take the safest of the moves they *could* have made"; that predicate fires only
/// when the engine can prove there was no non-empty move to make. The two stay
/// separate because their preconditions differ: a timeout must resolve a real
/// choice, and automation must never resolve one.
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
/// [`priority_has_no_meaningful_action`](sage_engine::priority_has_no_meaningful_action)
/// and [`forced_declaration_without_choice`](sage_engine::forced_declaration_without_choice))
/// whether the priority holder has a meaningful action, and whether a declaration it
/// owes has any legal non-empty answer; the room owns the loop that acts on both.
/// **Off by default** — an off policy reproduces exactly the pre-automation behavior,
/// so every existing flow and test is unchanged — and, when on, auto-passes a seat's
/// priority while the engine says it is idle, auto-submits a choiceless combat
/// declaration, and does neither where the seat has opted to stop at the current step
/// (its `set_stops` preferences, held per seat on the room).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AutoPassPolicy {
    /// No automation: every priority pass is manual (the default, and the behavior
    /// before automation existed).
    #[default]
    Off,
    /// Auto-pass an idle seat's priority, and auto-submit a forced combat
    /// declaration the engine reports has no legal non-empty answer (issue #453) —
    /// both per the seat's stop preferences.
    On,
}

/// The steps a human seat stops at by default under
/// [`StopPolicy::HumanMainPhases`] (issue #455): **its own main phases**.
///
/// These are the two windows a turn's owner acts in at sorcery speed, so they are
/// the two the settle loop must not carry them past — CR 505.5b makes a main phase
/// the only place a land drop, a sorcery, or a creature can be played, which is
/// precisely the decision a fast-forward would take away. Every other step keeps
/// ADR 0020's pacing: a seat with a real instant-speed play is non-idle and is
/// never auto-passed anywhere, and a seat with nothing to do sails through the
/// other ten steps as before.
pub(super) const DEFAULT_HUMAN_OWN_TURN_STOPS: [Phase; 2] =
    [Phase::PrecombatMain, Phase::PostcombatMain];

/// A room's **default-stop** policy (issue #455): the stop preferences a seat that
/// has never sent `set_stops` starts with.
///
/// ADR 0020 chose an empty default deliberately, on the grounds that automation only
/// ever passes a seat whose *sole* meaningful move is a pass — so an empty default
/// never skips a real decision. Issue #455 is the playtest evidence that the
/// argument, while true about decisions, is false about **comprehension**: a human
/// whose own turn contains nothing castable watches the settle run the whole turn,
/// and both their main phases, between two broadcasts. Nothing was decided for them
/// and they still lost the turn.
///
/// So the room seeds a *default*, not a rule. It is the ordinary
/// [`AutoPassPolicy`]/[`TimerPolicy`] shape — **off by default**, so every existing
/// construction (and every AI-only or headless game that never opts in) is
/// bit-for-bit unchanged — and the lobby turns it on for real games. It is only a
/// starting value: the first `set_stops` a seat sends replaces it wholesale, so a
/// player who wants ADR 0020's original pacing back clears it in one message and the
/// room never re-seeds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum StopPolicy {
    /// Every seat starts with no stops at all — ADR 0020's original default, and the
    /// behavior before default stops existed.
    #[default]
    None,
    /// Every **human** seat starts stopped at its own main phases
    /// ([`DEFAULT_HUMAN_OWN_TURN_STOPS`]); AI seats start with no stops, so AI-only
    /// and mixed games keep exactly the throughput they had. Humanness is the room's
    /// existing `ai_seats` knowledge (issue #415) — the same fact every seat's view
    /// already reports — so no new configuration is introduced to express it.
    HumanMainPhases,
}

impl StopPolicy {
    /// The stop preference a seat starts with under this policy, given whether that
    /// seat is AI-controlled. Only ever consulted for a seat that has never sent
    /// `set_stops` — the first one it sends replaces this wholesale.
    pub(super) fn seed(self, ai: bool) -> SeatStops {
        match self {
            Self::None => SeatStops::default(),
            Self::HumanMainPhases if ai => SeatStops::default(),
            Self::HumanMainPhases => SeatStops {
                any_turn: Vec::new(),
                own_turn: DEFAULT_HUMAN_OWN_TURN_STOPS.to_vec(),
            },
        }
    }
}

/// One seat's **priority-stop preference**: the steps at which it wants priority
/// even when the engine reports it idle (issues #264 and #455).
///
/// Two lists, because a stop answers two different questions. `any_turn` is ADR
/// 0020's original set — "hand me priority here whoever's turn it is", the escape
/// hatch for wanting to act at an opponent's end step. `own_turn` is issue #455's
/// narrower claim — "hand me priority here **while the turn is mine**", which is
/// what a main-phase stop has to mean: stopping a human in every opponent main
/// phase would reintroduce exactly the per-step click ADR 0020 removed, for a window
/// in which they have nothing to do that they could not already do at instant speed.
///
/// A step in both stops on every turn: `any_turn` is the wider claim and subsumes
/// the narrower one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct SeatStops {
    /// Steps to stop at on any player's turn.
    pub(super) any_turn: Vec<Phase>,
    /// Steps to stop at only while this seat is the active player.
    pub(super) own_turn: Vec<Phase>,
}

impl SeatStops {
    /// Whether this preference asks for priority at `here`, given whether the seat
    /// holding it is the active player.
    pub(super) fn stops_at(&self, here: Phase, own_turn: bool) -> bool {
        self.any_turn.contains(&here) || (own_turn && self.own_turn.contains(&here))
    }
}
