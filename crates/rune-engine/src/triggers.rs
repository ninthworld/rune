//! Diff-based trigger collection.
//!
//! Triggers are discovered by comparing the state before and after an action —
//! never via listeners or observers (crate `AGENTS.md`). [`crate::apply_action`]
//! calls [`collect_triggers`] and puts each resulting [`Trigger`] on the stack.

use crate::ability::{Ability, TriggerCondition};
use crate::card::abilities_of;
use crate::id::{CardInstanceId, PermanentId, PlayerId};
use crate::state::{GameState, Permanent};
use crate::{CardDatabase, Effect};

/// A triggered ability that a state transition has caused to trigger.
///
/// Triggers are collected by diffing the state before and after an action (see
/// [`collect_triggers`]) — never via listeners or observers (crate `AGENTS.md`).
/// A collected trigger carries everything needed to put the ability on the stack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trigger {
    /// The permanent whose ability triggered.
    pub source: PermanentId,
    /// The player who controls the triggered ability (its source's controller).
    pub controller: PlayerId,
    /// The effects the ability produces when it resolves.
    pub effects: Vec<Effect>,
}

/// Collect the triggers that should now exist by diffing `before` against
/// `after`. Each triggered ability whose condition ([`condition_met`]) holds
/// across the diff yields one [`Trigger`]. Pure, with no listeners (crate
/// `AGENTS.md`).
///
/// Two directions of zone change are observed. Enter-the-battlefield conditions
/// are checked against the permanents present in `after`; leave-the-battlefield
/// ("dies") conditions are checked against the permanents that were in `before`
/// but are gone from `after` — a dead permanent is no longer on the battlefield,
/// so its ability must be read from the *before* snapshot (last-known information,
/// CR 603.10a in spirit). A permanent that persists across the transition is only
/// visited in the first pass and matches neither condition.
///
/// **Ordering (simultaneous triggers).** Triggers are appended in the order their
/// sources are iterated: `after.battlefield` order for enters, then
/// `before.battlefield` order for deaths. That battlefield-position order is the
/// engine's deterministic default when several abilities trigger at once — the
/// full APNAP each-player-orders-their-own prompt (CR 603.3b / CR 101.4) is later
/// work. [`crate::apply_action`] puts the collected triggers on the stack after
/// the state-based-action loop, i.e. the next time a player would receive
/// priority (CR 603.3b), matching how ETB triggers already reach the stack.
#[must_use]
pub fn collect_triggers(before: &GameState, after: &GameState, db: &CardDatabase) -> Vec<Trigger> {
    let mut triggers = Vec::new();
    // Enter-the-battlefield direction: observe permanents present in `after`.
    for perm in &after.battlefield {
        collect_from(perm, before, after, db, &mut triggers);
    }
    // Leave-the-battlefield ("dies") direction: observe permanents that were in
    // `before` but are gone from `after`. Iterating `before.battlefield` keeps the
    // stack order of simultaneous deaths deterministic (see the ordering note).
    for perm in &before.battlefield {
        if after.battlefield.iter().any(|p| p.id == perm.id) {
            continue;
        }
        collect_from(perm, before, after, db, &mut triggers);
    }
    triggers
}

/// Push a [`Trigger`] for every triggered ability of `perm` whose condition holds
/// across the diff. `perm` is read from whichever snapshot still has it (the
/// `after` battlefield for enters, the `before` battlefield for deaths).
fn collect_from(
    perm: &Permanent,
    before: &GameState,
    after: &GameState,
    db: &CardDatabase,
    out: &mut Vec<Trigger>,
) {
    for ability in abilities_of(db, perm.card) {
        if let Ability::Triggered { event, effects } = ability {
            if condition_met(&event, perm, before, after) {
                out.push(Trigger {
                    source: perm.id,
                    controller: perm.controller,
                    effects,
                });
            }
        }
    }
}

/// Evaluate a trigger condition as a pure predicate over the before/after states,
/// for the candidate permanent `perm` (its id and physical instance).
fn condition_met(
    condition: &TriggerCondition,
    perm: &Permanent,
    before: &GameState,
    after: &GameState,
) -> bool {
    match condition {
        TriggerCondition::SelfEntersBattlefield => {
            after.battlefield.iter().any(|p| p.id == perm.id)
                && !before.battlefield.iter().any(|p| p.id == perm.id)
        }
        // CR 700.4 / 603.6c: the permanent died — it left the battlefield for a
        // graveyard. Observed purely by diff: its id is gone from the battlefield
        // and its physical instance is now in some graveyard where it was not
        // before. Requiring the *graveyard* destination is what stops a leave to a
        // non-graveyard zone (a future bounce or exile) from firing this.
        TriggerCondition::SelfDies => {
            let left = before.battlefield.iter().any(|p| p.id == perm.id)
                && !after.battlefield.iter().any(|p| p.id == perm.id);
            left && in_graveyard(after, perm.instance) && !in_graveyard(before, perm.instance)
        }
    }
}

/// Whether the physical card `instance` is in any player's graveyard in `state`.
fn in_graveyard(state: &GameState, instance: CardInstanceId) -> bool {
    state
        .players
        .iter()
        .any(|p| p.graveyard.iter().any(|c| c.id == instance))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::id::{CardId, CardInstanceId};
    use crate::state::Permanent;

    /// The bundled card database, for tests that need oracle data.
    fn db() -> CardDatabase {
        CardDatabase::bundled().unwrap()
    }

    #[test]
    fn trigger_diff_yields_nothing_for_a_plain_transition() {
        let before = GameState::new_two_player();
        let after = before.advance();
        assert!(collect_triggers(&before, &after, &db()).is_empty());
    }

    #[test]
    fn collect_triggers_detects_etb_by_permanent_id_diff() {
        let db = db();
        let before = GameState::new_two_player();
        let mut after = before.clone();
        after.battlefield.push(Permanent {
            id: PermanentId(1),
            instance: CardInstanceId(1),
            card: CardId(6),
            controller: PlayerId(0),
            tapped: false,
            entered_turn: 0,
            attacking: false,
            blocking: None,
            damage: 0,
            counters: Default::default(),
        });
        let triggers = collect_triggers(&before, &after, &db);
        assert_eq!(triggers.len(), 1);
        assert_eq!(triggers[0].source, PermanentId(1));
        assert_eq!(triggers[0].effects, vec![Effect::DrawCard { count: 1 }]);
    }

    /// A battlefield holding the dies fixture (Cryptvine Lurker, id 28) as a lone
    /// permanent under player 0. Returns the `before` state and the permanent's id
    /// and instance so a test can craft the matching `after`.
    fn before_with_lurker() -> (GameState, PermanentId, CardInstanceId) {
        let mut before = GameState::new_two_player();
        let instance = CardInstanceId(77);
        let id = PermanentId(1);
        before.battlefield.push(Permanent {
            id,
            instance,
            card: CardId(28),
            controller: PlayerId(0),
            tapped: false,
            entered_turn: 0,
            attacking: false,
            blocking: None,
            damage: 0,
            counters: Default::default(),
        });
        (before, id, instance)
    }

    #[test]
    fn issue_151_collect_triggers_detects_a_death_by_battlefield_to_graveyard_diff() {
        // CR 700.4 / 603.6c: the permanent left the battlefield and its instance is
        // now in a graveyard — the diff observes the death and yields the dies
        // trigger, its source the (now-gone) permanent id.
        let db = db();
        let (before, id, instance) = before_with_lurker();
        let mut after = before.clone();
        after.battlefield.clear();
        after.players[0].graveyard.push(crate::id::CardInstance {
            id: instance,
            card: CardId(28),
        });

        let triggers = collect_triggers(&before, &after, &db);
        assert_eq!(triggers.len(), 1);
        assert_eq!(triggers[0].source, id);
        assert_eq!(triggers[0].controller, PlayerId(0));
        assert_eq!(triggers[0].effects, vec![Effect::DrawCard { count: 1 }]);
    }

    #[test]
    fn issue_151_leaving_to_a_non_graveyard_zone_does_not_fire_dies() {
        // Future-proofing (CR 603.6c): a permanent that leaves the battlefield but
        // is *not* put into a graveyard (e.g. a bounce or a countered permanent)
        // does not satisfy `SelfDies`. The permanent is simply gone from `after`
        // with nothing in any graveyard.
        let db = db();
        let (before, _id, _instance) = before_with_lurker();
        let mut after = before.clone();
        after.battlefield.clear();

        assert!(
            collect_triggers(&before, &after, &db).is_empty(),
            "a leave to a non-graveyard zone must not fire SelfDies"
        );
    }
}
