//! Diff-based trigger collection.
//!
//! Triggers are discovered by comparing the state before and after an action —
//! never via listeners or observers (crate `AGENTS.md`). [`crate::apply_action`]
//! calls [`collect_triggers`] and puts each resulting [`Trigger`] on the stack.

use crate::ability::{Ability, ObservedPermanent, ObservedSpell, TriggerCondition, TurnScope};
use crate::card::abilities_of;
use crate::card_type::CardType;
use crate::id::{CardInstanceId, PermanentId, PlayerId};
use crate::stack::{StackId, StackObjectKind};
use crate::state::{GameEvent, GameState, Permanent};
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
/// CR 603.10a in spirit). A permanent that persists across the transition is visited
/// only in the first pass, where it can still match a condition about something that
/// happened *to it in place* — being declared as an attacker (CR 603.6d).
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

/// The topmost triggered ability on the stack that is still owed targets
/// (CR 603.3d), or `None` when nothing is owed.
///
/// **Derived, never stored.** A triggered ability reaches the stack with no targets
/// (its controller has not chosen yet), so "owes targets" is exactly *declares more
/// target slots than it carries* — a fact about the object, readable at any time,
/// with no flag to set or clear. Spells and activated abilities are never in this
/// state: both choose their targets as part of the action that put them there
/// (CR 601.2c), so they always arrive full.
///
/// Topmost first, because triggers are chosen for in the order they will be answered
/// and the last one put on the stack is the first a player sees.
#[must_use]
pub fn pending_trigger_target_choice(state: &GameState) -> Option<StackId> {
    state
        .stack
        .iter()
        .rev()
        .find(|object| match &object.kind {
            StackObjectKind::Ability { effects, .. } => {
                effects.iter().filter_map(Effect::target_spec).count() > object.targets.len()
            }
            StackObjectKind::Spell { .. } => false,
        })
        .map(|object| object.id)
}

/// The controller of the stack object `id` — the player who chooses its targets.
#[must_use]
pub(crate) fn controller_of_stack_object(state: &GameState, id: StackId) -> Option<PlayerId> {
    state
        .stack
        .iter()
        .find(|o| o.id == id)
        .map(|o| o.controller)
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
            // A condition reports *how many times* it was met, not whether: an ability
            // watching the rest of the board sees one event per qualifying object, and
            // two creatures dying at once must trigger it twice (CR 603.2). The
            // self-conditions can only ever answer 0 or 1.
            for _ in 0..fire_count(&event, perm, before, after, db) {
                out.push(Trigger {
                    source: perm.id,
                    controller: perm.controller,
                    effects: effects.clone(),
                });
            }
        }
    }
}

/// The permanents that entered the battlefield across the diff: present in `after`,
/// absent from `before`.
fn entered<'a>(before: &GameState, after: &'a GameState) -> impl Iterator<Item = &'a Permanent> {
    let ids: Vec<PermanentId> = before.battlefield.iter().map(|p| p.id).collect();
    after
        .battlefield
        .iter()
        .filter(move |p| !ids.contains(&p.id))
}

/// The permanents that **died** across the diff — the [`TriggerCondition::SelfDies`]
/// test applied to the whole board, so "died" means exactly the same thing whether an
/// ability is watching itself or its neighbours.
fn died<'a>(before: &'a GameState, after: &GameState) -> Vec<&'a Permanent> {
    before
        .battlefield
        .iter()
        .filter(|p| {
            !after.battlefield.iter().any(|q| q.id == p.id)
                && in_graveyard(after, p.instance)
                && !in_graveyard(before, p.instance)
        })
        .collect()
}

/// The events recorded **by this transition** — the entries `after` has that `before`
/// did not.
///
/// This is still a diff, over the one part of the state that records *what happened*
/// rather than what is (ADR 0007). Some conditions are about events, not about board
/// positions, and cannot be recovered from a snapshot comparison at all: gaining three
/// life and losing three leaves every total unchanged, but a card that triggers on
/// gaining life has triggered. Reading the recorded events is what makes those
/// conditions expressible without adding a listener.
///
/// The log is a bounded window, so a transition recording more events than the window
/// holds would lose its earliest ones. A single transition records a handful.
fn events_in<'a>(before: &GameState, after: &'a GameState) -> impl Iterator<Item = &'a GameEvent> {
    let from = before.next_log_sequence;
    after
        .log
        .iter()
        .filter(move |entry| entry.sequence >= from)
        .map(|entry| &entry.event)
}

/// Whether `candidate` is one of the permanents `observes` watches, for an ability on
/// `source`. Evaluated relative to the source, exactly as [`crate::StaticAffects`] is:
/// "you" is the source's controller and "another" excludes the source itself.
fn observed_matches(
    observes: &ObservedPermanent,
    candidate: &Permanent,
    source: &Permanent,
    db: &CardDatabase,
) -> bool {
    if observes.excludes_source() && candidate.id == source.id {
        return false;
    }
    let Some(card) = db.card(candidate.card) else {
        return false;
    };
    if !card.has_type(CardType::Creature) {
        return false;
    }
    if let Some(subtype) = observes.subtype() {
        if !card.has_subtype(subtype) {
            return false;
        }
    }
    match observes {
        ObservedPermanent::CreaturesYouControl { .. } => candidate.controller == source.controller,
        ObservedPermanent::AnyCreature { .. } => true,
    }
}

/// Whether the spell `card` is one `observes` notices.
fn observed_spell_matches(
    observes: ObservedSpell,
    card: crate::id::CardId,
    db: &CardDatabase,
) -> bool {
    let Some(data) = db.card(card) else {
        return false;
    };
    match observes {
        ObservedSpell::Enchantment => data.has_type(CardType::Enchantment),
        ObservedSpell::InstantOrSorcery => {
            data.has_type(CardType::Instant) || data.has_type(CardType::Sorcery)
        }
    }
}

/// How many times `condition` was met across the transition, for an ability on `perm`.
///
/// A pure function of the two snapshots — never an event listener. The self-conditions
/// answer 0 or 1 because they are about one object; a condition watching the board
/// answers once per qualifying event, which is what makes a board wipe trigger a
/// death-watcher once per creature rather than once.
fn fire_count(
    condition: &TriggerCondition,
    perm: &Permanent,
    before: &GameState,
    after: &GameState,
    db: &CardDatabase,
) -> usize {
    usize::from(match condition {
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
        // CR 508.1 / 603.6d: the permanent was declared as an attacker this
        // transition. Observed by diff on the one field the declaration writes —
        // `attacking` is set after and was not before — so it fires once, from the
        // declare-attackers action, and never from a creature that was merely tapped
        // or that is still attacking from an earlier check.
        TriggerCondition::SelfAttacks => {
            let attacking_now = after
                .battlefield
                .iter()
                .any(|p| p.id == perm.id && p.attacking.is_some());
            let attacking_before = before
                .battlefield
                .iter()
                .any(|p| p.id == perm.id && p.attacking.is_some());
            attacking_now && !attacking_before
        }
        // The watching conditions count rather than answer, so each returns early.
        TriggerCondition::PermanentEnters(observes) => {
            // An ability that is no longer on the battlefield is not watching it. A
            // death-watcher is the deliberate exception below: a creature that died
            // alongside another still observed that death.
            if !after.battlefield.iter().any(|p| p.id == perm.id) {
                return 0;
            }
            return entered(before, after)
                .filter(|candidate| observed_matches(observes, candidate, perm, db))
                .count();
        }
        TriggerCondition::PermanentDies(observes) => {
            return died(before, after)
                .into_iter()
                .filter(|candidate| observed_matches(observes, candidate, perm, db))
                .count();
        }
        TriggerCondition::YouGainLife => {
            if !after.battlefield.iter().any(|p| p.id == perm.id) {
                return 0;
            }
            // A *gain*, not a net change: the recorded delta is signed, and only a
            // positive one is life gained (CR 118.3). Damage to a player is recorded as
            // damage rather than as a life change, so it never reaches here.
            return events_in(before, after)
                .filter(|event| {
                    matches!(event, GameEvent::LifeChanged { player, amount }
                        if *player == perm.controller && *amount > 0)
                })
                .count();
        }
        // CR 603.6a: the ability triggers as the step begins. Counted over the
        // `StepChanged` entries this transition recorded rather than by comparing
        // `before.step` with `after.step` — one pass of priority can walk through
        // several steps at once, and a step recurs every turn, so the snapshot
        // comparison both misses crossings and fires on transitions that crossed
        // nothing. One event per crossing makes this exactly once per crossing.
        TriggerCondition::BeginningOfStep { step, whose_turn } => {
            if !after.battlefield.iter().any(|p| p.id == perm.id) {
                return 0;
            }
            let watched = step.step();
            return events_in(before, after)
                .filter(|event| {
                    matches!(event, GameEvent::StepChanged { step, active_player, .. }
                    if *step == watched
                        && match whose_turn {
                            TurnScope::Yours => *active_player == perm.controller,
                            TurnScope::Each => true,
                        })
                })
                .count();
        }
        TriggerCondition::YouCastSpell(spell) => {
            if !after.battlefield.iter().any(|p| p.id == perm.id) {
                return 0;
            }
            return events_in(before, after)
                .filter(|event| {
                    matches!(event, GameEvent::SpellCast { player, card }
                        if *player == perm.controller
                            && observed_spell_matches(*spell, card.card, db))
                })
                .count();
        }
    })
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
    use crate::fixtures::{fixture, id_in};
    use crate::id::CardInstanceId;
    use crate::state::Permanent;

    /// The bundled card database, for tests that need oracle data.
    fn db() -> CardDatabase {
        CardDatabase::bundled().unwrap()
    }

    /// An inline catalog with a dies-draw creature. No clean M19 card carries a bare
    /// "when this dies, draw a card", so the dies-trigger tests build their own
    /// definition (ADR 0009).
    fn lurker_db() -> CardDatabase {
        let json = r#"[{"schema_version":1,"functional_id":"test_lurker","name":"Test Lurker",
            "types":["creature"],"subtypes":["Horror"],"mana_cost":"{1}{B}","colors":["black"],
            "power":2,"toughness":2,
            "abilities":[{"type":"triggered","event":"self_dies","effects":[{"kind":"draw_card","count":1}]}]}]"#;
        CardDatabase::from_json(json).unwrap()
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
            card: fixture("skyscanner"),
            controller: PlayerId(0),
            tapped: false,
            entered_turn: 0,
            attacking: None,
            blocking: None,
            damage: 0,
            counters: Default::default(),
            attached_to: None,
        });
        let triggers = collect_triggers(&before, &after, &db);
        assert_eq!(triggers.len(), 1);
        assert_eq!(triggers[0].source, PermanentId(1));
        assert_eq!(triggers[0].effects, vec![Effect::DrawCard { count: 1 }]);
    }

    /// A battlefield holding the dies-draw creature (`test_lurker`) as a lone
    /// permanent under player 0. Returns the `before` state and the permanent's id
    /// and instance so a test can craft the matching `after`.
    fn before_with_lurker(db: &CardDatabase) -> (GameState, PermanentId, CardInstanceId) {
        let mut before = GameState::new_two_player();
        let instance = CardInstanceId(77);
        let id = PermanentId(1);
        before.battlefield.push(Permanent {
            id,
            instance,
            card: id_in(db, "test_lurker"),
            controller: PlayerId(0),
            tapped: false,
            entered_turn: 0,
            attacking: None,
            blocking: None,
            damage: 0,
            counters: Default::default(),
            attached_to: None,
        });
        (before, id, instance)
    }

    #[test]
    fn issue_151_collect_triggers_detects_a_death_by_battlefield_to_graveyard_diff() {
        // CR 700.4 / 603.6c: the permanent left the battlefield and its instance is
        // now in a graveyard — the diff observes the death and yields the dies
        // trigger, its source the (now-gone) permanent id.
        let db = lurker_db();
        let (before, id, instance) = before_with_lurker(&db);
        let mut after = before.clone();
        after.battlefield.clear();
        after.players[0].graveyard.push(crate::id::CardInstance {
            id: instance,
            card: id_in(&db, "test_lurker"),
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
        let db = lurker_db();
        let (before, _id, _instance) = before_with_lurker(&db);
        let mut after = before.clone();
        after.battlefield.clear();

        assert!(
            collect_triggers(&before, &after, &db).is_empty(),
            "a leave to a non-graveyard zone must not fire SelfDies"
        );
    }

    // ----- CR 603.6a: triggers at the beginning of a step (issue #607) -----

    /// An inline catalog of step-triggered permanents, one per scope/step pairing the
    /// vocabulary can express. No M19 card carries a bare step trigger with nothing
    /// attached to it, so these tests build their own definitions (ADR 0009).
    ///
    /// The life amounts are deliberately distinct (1/2/4/8) so a single life total
    /// says exactly which of them fired, and how often.
    fn step_db() -> CardDatabase {
        let json = r#"[
            {"schema_version":1,"functional_id":"test_your_upkeep","name":"Test Your Upkeep",
             "types":["enchantment"],"mana_cost":"{1}{W}","colors":["white"],
             "abilities":[{"type":"triggered",
                "event":{"beginning_of_step":{"step":"upkeep","whose_turn":"yours"}},
                "effects":[{"kind":"gain_life","player_ref":"controller","amount":1}]}]},
            {"schema_version":1,"functional_id":"test_each_upkeep","name":"Test Each Upkeep",
             "types":["artifact"],"mana_cost":"{1}",
             "abilities":[{"type":"triggered",
                "event":{"beginning_of_step":{"step":"upkeep","whose_turn":"each"}},
                "effects":[{"kind":"gain_life","player_ref":"controller","amount":2}]}]},
            {"schema_version":1,"functional_id":"test_each_end_step","name":"Test Each End Step",
             "types":["enchantment"],"mana_cost":"{2}{B}","colors":["black"],
             "abilities":[{"type":"triggered",
                "event":{"beginning_of_step":{"step":"end_step","whose_turn":"each"}},
                "effects":[{"kind":"gain_life","player_ref":"controller","amount":4}]}]},
            {"schema_version":1,"functional_id":"test_your_combat","name":"Test Your Combat",
             "types":["creature"],"subtypes":["Dwarf"],"mana_cost":"{3}{W}","colors":["white"],
             "power":3,"toughness":3,
             "abilities":[{"type":"triggered",
                "event":{"beginning_of_step":{"step":"begin_combat","whose_turn":"yours"}},
                "effects":[{"kind":"gain_life","player_ref":"controller","amount":8}]}]}
        ]"#;
        CardDatabase::from_json(json).unwrap()
    }

    /// A two-player state with `functional_id` on the battlefield under `controller`,
    /// plus the `after` snapshot in which the turn structure has entered `step` on
    /// `active_player`'s turn `turn` — the recorded crossing the condition reads.
    fn crossing(
        db: &CardDatabase,
        functional_id: &str,
        controller: PlayerId,
        turn: u32,
        active_player: PlayerId,
        step: crate::phase::Step,
    ) -> (GameState, GameState) {
        let mut before = GameState::new_two_player();
        before.battlefield.push(Permanent {
            id: PermanentId(1),
            instance: CardInstanceId(1),
            card: id_in(db, functional_id),
            controller,
            tapped: false,
            entered_turn: 0,
            attacking: None,
            blocking: None,
            damage: 0,
            counters: Default::default(),
            attached_to: None,
        });
        let mut after = before.clone();
        after.turn = turn;
        after.active_player = active_player;
        after.step = step;
        after.record_event(GameEvent::StepChanged {
            turn,
            active_player,
            step,
        });
        (before, after)
    }

    #[test]
    fn issue_607_your_upkeep_fires_once_on_its_controllers_upkeep() {
        // CR 603.6a with a "your" scope: seat 0's own upkeep, and exactly one trigger
        // for the one crossing.
        let db = step_db();
        let (before, after) = crossing(
            &db,
            "test_your_upkeep",
            PlayerId(0),
            1,
            PlayerId(0),
            crate::phase::Step::Upkeep,
        );
        let triggers = collect_triggers(&before, &after, &db);
        assert_eq!(triggers.len(), 1);
        assert_eq!(triggers[0].controller, PlayerId(0));
        assert_eq!(
            triggers[0].effects,
            vec![Effect::GainLife {
                player_ref: crate::ability::PlayerRef::Controller,
                amount: 1
            }]
        );
    }

    #[test]
    fn issue_607_your_upkeep_does_not_fire_on_an_opponents_upkeep() {
        // The whole point of the scope: seat 0's ability is silent on seat 1's turn.
        let db = step_db();
        let (before, after) = crossing(
            &db,
            "test_your_upkeep",
            PlayerId(0),
            2,
            PlayerId(1),
            crate::phase::Step::Upkeep,
        );
        assert!(collect_triggers(&before, &after, &db).is_empty());
    }

    #[test]
    fn issue_607_each_upkeep_fires_on_every_players_upkeep() {
        // The "each" scope ignores whose turn it is: seat 0's artifact triggers on
        // seat 1's upkeep just as it does on its own.
        let db = step_db();
        for active in [PlayerId(0), PlayerId(1)] {
            let (before, after) = crossing(
                &db,
                "test_each_upkeep",
                PlayerId(0),
                2,
                active,
                crate::phase::Step::Upkeep,
            );
            let triggers = collect_triggers(&before, &after, &db);
            assert_eq!(
                triggers.len(),
                1,
                "an each-upkeep trigger fires on seat {}'s turn",
                active.0
            );
            assert_eq!(triggers[0].controller, PlayerId(0));
        }
    }

    #[test]
    fn issue_607_a_step_trigger_ignores_a_crossing_into_another_step() {
        // A condition names one step. Every other crossing is not its boundary, and a
        // transition that crossed no boundary at all fires nothing either.
        let db = step_db();
        for step in [
            crate::phase::Step::Draw,
            crate::phase::Step::BeginCombat,
            crate::phase::Step::End,
        ] {
            let (before, after) =
                crossing(&db, "test_your_upkeep", PlayerId(0), 1, PlayerId(0), step);
            assert!(
                collect_triggers(&before, &after, &db).is_empty(),
                "an upkeep trigger must not fire on a crossing into {step:?}"
            );
        }
    }

    #[test]
    fn issue_607_a_transition_that_crosses_no_boundary_fires_nothing() {
        // The once-per-crossing property from the other side, and the reason the
        // condition reads recorded events rather than comparing `before.step` with
        // `after.step`: here the state *is* in the upkeep step both before and after,
        // and no crossing happened, so nothing triggers. A snapshot comparison that
        // looked only at `after.step` would fire on every such transition.
        let db = step_db();
        let (before, _) = crossing(
            &db,
            "test_your_upkeep",
            PlayerId(0),
            1,
            PlayerId(0),
            crate::phase::Step::Upkeep,
        );
        let mut before = before;
        before.step = crate::phase::Step::Upkeep;
        let mut after = before.clone();
        after.players[0].life -= 1; // some unrelated thing happened during upkeep
        assert!(collect_triggers(&before, &after, &db).is_empty());
    }

    #[test]
    fn issue_607_one_transition_crossing_several_steps_fires_each_of_them() {
        // One pass of priority can walk through several steps at once (end → cleanup
        // → untap → upkeep), and each crossing is recorded. A watcher of the end step
        // and a watcher of the upkeep both fire from that single transition — which a
        // comparison of `before.step` with `after.step` could never see, because only
        // the last step survives into `after`.
        let db = step_db();
        let mut before = GameState::new_two_player();
        before.step = crate::phase::Step::End;
        for (index, id) in ["test_each_end_step", "test_each_upkeep"]
            .into_iter()
            .enumerate()
        {
            before.battlefield.push(Permanent {
                id: PermanentId(index as u64 + 1),
                instance: CardInstanceId(index as u64 + 1),
                card: id_in(&db, id),
                controller: PlayerId(0),
                tapped: false,
                entered_turn: 0,
                attacking: None,
                blocking: None,
                damage: 0,
                counters: Default::default(),
                attached_to: None,
            });
        }
        let mut after = before.clone();
        for (turn, active, step) in [
            (1, PlayerId(0), crate::phase::Step::End),
            (1, PlayerId(0), crate::phase::Step::Cleanup),
            (2, PlayerId(1), crate::phase::Step::Untap),
            (2, PlayerId(1), crate::phase::Step::Upkeep),
        ] {
            after.record_event(GameEvent::StepChanged {
                turn,
                active_player: active,
                step,
            });
        }
        after.turn = 2;
        after.active_player = PlayerId(1);
        after.step = crate::phase::Step::Upkeep;

        let triggers = collect_triggers(&before, &after, &db);
        assert_eq!(triggers.len(), 2, "both crossings were observed");
        let amounts: Vec<Effect> = triggers.iter().flat_map(|t| t.effects.clone()).collect();
        assert!(amounts.contains(&Effect::GainLife {
            player_ref: crate::ability::PlayerRef::Controller,
            amount: 4
        }));
        assert!(amounts.contains(&Effect::GainLife {
            player_ref: crate::ability::PlayerRef::Controller,
            amount: 2
        }));
    }

    #[test]
    fn issue_607_a_source_that_is_gone_does_not_trigger() {
        // An ability that is no longer on the battlefield is not there to trigger,
        // the same rule the other watching conditions follow.
        let db = step_db();
        let (before, after) = crossing(
            &db,
            "test_your_combat",
            PlayerId(0),
            2,
            PlayerId(0),
            crate::phase::Step::BeginCombat,
        );
        assert_eq!(collect_triggers(&before, &after, &db).len(), 1);

        let mut gone = after;
        gone.battlefield.clear();
        assert!(collect_triggers(&before, &gone, &db).is_empty());
    }
}
