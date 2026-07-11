//! Diff-based trigger collection.
//!
//! Triggers are discovered by comparing the state before and after an action —
//! never via listeners or observers (crate `AGENTS.md`). [`crate::apply_action`]
//! calls [`collect_triggers`] and puts each resulting [`Trigger`] on the stack.

use crate::ability::{Ability, TriggerCondition};
use crate::card::abilities_of;
use crate::id::{PermanentId, PlayerId};
use crate::state::GameState;
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
/// `after`. For every permanent, each triggered ability whose condition
/// ([`condition_met`]) holds across the diff yields one [`Trigger`]. Pure, with
/// no listeners (crate `AGENTS.md`).
#[must_use]
pub fn collect_triggers(before: &GameState, after: &GameState, db: &CardDatabase) -> Vec<Trigger> {
    let mut triggers = Vec::new();
    for perm in &after.battlefield {
        for ability in abilities_of(db, perm.card) {
            if let Ability::Triggered { event, effects } = ability {
                if condition_met(&event, perm.id, before, after) {
                    triggers.push(Trigger {
                        source: perm.id,
                        controller: perm.controller,
                        effects,
                    });
                }
            }
        }
    }
    triggers
}

/// Evaluate a trigger condition as a pure predicate over the before/after states.
fn condition_met(
    condition: &TriggerCondition,
    source: PermanentId,
    before: &GameState,
    after: &GameState,
) -> bool {
    match condition {
        TriggerCondition::SelfEntersBattlefield => {
            after.battlefield.iter().any(|p| p.id == source)
                && !before.battlefield.iter().any(|p| p.id == source)
        }
    }
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
}
