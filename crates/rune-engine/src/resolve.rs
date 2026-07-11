//! Stack resolution: turning the top object of the stack into its effect.
//!
//! When all players pass priority in succession, [`crate::apply_action`] pops
//! the top of the stack and hands it to [`resolve_stack_object`], which first
//! re-checks the object's chosen targets against current state (CR 608.2b), then
//! routes a spell by its card types and applies an ability's effects.

use crate::ability::{Effect, Target, TargetSpec};
use crate::apply::{apply_effect, apply_targeted_effect};
use crate::card::CardData;
use crate::card_type::CardType;
use crate::id::PermanentId;
use crate::stack::{StackObject, StackObjectKind};
use crate::state::{GameState, Permanent};
use crate::CardDatabase;

/// Whether `target` is a legal choice for `spec` against the *current* `state`
/// (CR 115). A pure predicate: it derives legality on demand and never mutates,
/// consistent with the engine's pull-based, no-observer rule.
///
/// This is the check the resolve path re-runs on each stored target (CR 608.2b);
/// enumerating the full legal set from a spec is issue #71's job and can build on
/// this same predicate.
#[must_use]
pub(crate) fn target_is_legal(
    spec: TargetSpec,
    target: Target,
    state: &GameState,
    db: &CardDatabase,
) -> bool {
    match (spec, target) {
        // A player is a legal target while they are still in the game.
        (TargetSpec::AnyPlayer, Target::Player(player)) => {
            state.players.get(player.0).is_some_and(|p| !p.has_lost)
        }
        // A permanent target is legal while that exact battlefield object exists.
        (TargetSpec::AnyPermanent, Target::Permanent(id)) => {
            state.battlefield.iter().any(|p| p.id == id)
        }
        // A creature target additionally requires the permanent's printed types
        // to include Creature (the layer system's type-changing effects are
        // future work, so printed types are authoritative here).
        (TargetSpec::AnyCreature, Target::Permanent(id)) => state.battlefield.iter().any(|p| {
            p.id == id
                && db
                    .card(p.card)
                    .is_some_and(|c| c.has_type(CardType::Creature))
        }),
        // Any other spec/value pairing names the wrong kind of object and is
        // never legal.
        _ => false,
    }
}

/// Resolve one object popped from the top of the stack.
///
/// Targets are re-checked first: an individually-illegal target is skipped, and
/// an object all of whose chosen targets are now illegal does not resolve at all
/// — it is removed from the stack with no effect (CR 608.2b, "fizzle").
pub(crate) fn resolve_stack_object(state: &mut GameState, object: StackObject, db: &CardDatabase) {
    // The specs the stored targets were chosen for, in the same order the
    // targeting effects consume them. A spell carries no engine-modeled effects
    // yet, so it exposes no specs (its recorded targets are inert until spell
    // effects exist).
    let specs: Vec<TargetSpec> = match &object.kind {
        StackObjectKind::Ability { effects, .. } => {
            effects.iter().filter_map(Effect::target_spec).collect()
        }
        StackObjectKind::Spell { .. } => Vec::new(),
    };

    // CR 608.2b: if the object chose targets and *every* one is now illegal, it
    // is removed from the stack without resolving — it simply ceases to exist,
    // producing no effect. Only abilities expose specs today (see above), so a
    // fizzle can only befall an ability; once spell effects carry specs, a
    // fizzled spell must additionally be put into its owner's graveyard here.
    if !specs.is_empty()
        && specs
            .iter()
            .zip(&object.targets)
            .all(|(&spec, &target)| !target_is_legal(spec, target, state, db))
    {
        return;
    }

    match object.kind {
        StackObjectKind::Spell { card } => {
            // Route by the resolving card's types (CR 608.3). A permanent spell
            // enters the battlefield with a fresh id (its instance id carries
            // over); an instant/sorcery creates no Permanent and instead goes to
            // its owner's graveyard as the same instance (CR 608.2m). The engine
            // does not yet track ownership apart from control, so we use the
            // controller's graveyard on the owner == controller assumption —
            // ownership tracking is future work.
            if db.card(card.card).is_some_and(CardData::is_permanent) {
                let id = state.mint_id();
                state.battlefield.push(Permanent {
                    id: PermanentId(id),
                    instance: card.id,
                    card: card.card,
                    controller: object.controller,
                    tapped: false,
                    damage: 0,
                    counters: Default::default(),
                });
            } else if let Some(player) = state.players.get_mut(object.controller.0) {
                player.graveyard.push(card);
            }
        }
        StackObjectKind::Ability { effects, .. } => {
            // Pair each targeting effect with the next stored target, applying
            // it only if that target is still legal; individually-illegal
            // targets are skipped (CR 608.2c) while legal ones resolve. Effects
            // with an implicit subject apply unconditionally.
            let mut targets = object.targets.iter();
            for effect in &effects {
                match effect.target_spec() {
                    Some(spec) => {
                        if let Some(&target) = targets.next() {
                            if target_is_legal(spec, target, state, db) {
                                apply_targeted_effect(state, effect, target, object.controller);
                            }
                        }
                    }
                    None => apply_effect(state, effect, object.controller),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::actions::Action;
    use crate::apply_action;
    use crate::id::{CardId, CardInstance, PlayerId};
    use crate::mana::Color;
    use crate::phase::Step;
    use crate::stack::StackId;

    /// The bundled card database, for tests that need oracle data.
    fn db() -> CardDatabase {
        CardDatabase::bundled().unwrap()
    }

    /// A two-player game in the precombat main phase with player 0 holding a
    /// Forest and Verdant Scout, and one card to draw in the library. Each card
    /// is a freshly minted [`CardInstance`] so copies stay distinguishable.
    fn slice_state() -> GameState {
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let forest = state.new_instance(CardId(5));
        let scout = state.new_instance(CardId(6));
        let draw = state.new_instance(CardId(1));
        state.players[0].hand = vec![forest, scout];
        state.players[0].library = vec![draw];
        state
    }

    /// The first hand instance in `seat`'s hand whose printed card is `card`.
    fn hand_instance(state: &GameState, seat: usize, card: CardId) -> CardInstance {
        *state.players[seat]
            .hand
            .iter()
            .find(|c| c.card == card)
            .unwrap()
    }

    #[test]
    fn resolving_a_creature_spell_puts_it_on_the_battlefield() {
        let db = db();
        let mut state = slice_state();
        state.players[0].mana_pool.add(Color::Green, 1);
        let scout = hand_instance(&state, 0, CardId(6));
        let state = apply_action(&state, &Action::CastSpell { card: scout }, &db);
        let state = apply_action(&state, &Action::PassPriority, &db);
        let state = apply_action(&state, &Action::PassPriority, &db);
        // The permanent that resolves carries the same instance the spell had.
        let perm = state
            .battlefield
            .iter()
            .find(|p| p.card == CardId(6))
            .unwrap();
        assert_eq!(perm.instance, scout.id);
    }

    #[test]
    fn issue_47_non_permanent_spell_resolves_to_graveyard_not_battlefield() {
        // A resolving instant must never create a Permanent; it goes to its
        // owner's graveyard (CR 608.3 / 608.2m). The casting gate still only
        // offers creature casts (out of scope for #47), so we seed a synthetic
        // instant directly on the stack and drive resolution through the public
        // apply_action path (both players pass → the top of the stack resolves).
        let json = r#"[{"id":100,"name":"Test Bolt","types":["instant"],"mana_cost":"{R}","oracle_text":""}]"#;
        let db = CardDatabase::from_json(json).unwrap();

        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let bolt = state.new_instance(CardId(100));
        let sid = state.mint_id();
        state.stack.push(StackObject {
            id: StackId(sid),
            controller: PlayerId(0),
            kind: StackObjectKind::Spell { card: bolt },
            targets: Vec::new(),
        });

        let state = apply_action(&state, &Action::PassPriority, &db);
        let state = apply_action(&state, &Action::PassPriority, &db);

        assert!(state.stack.is_empty());
        assert!(state.battlefield.is_empty());
        assert_eq!(state.players[0].graveyard, vec![bolt]);
    }

    /// Put a creature (Verdant Scout, [`CardId(6)`]) onto the battlefield under
    /// player 0's control and return its fresh [`PermanentId`].
    fn creature_on_battlefield(state: &mut GameState) -> PermanentId {
        let inst = state.new_instance(CardId(6));
        let id = state.mint_id();
        state.battlefield.push(Permanent {
            id: PermanentId(id),
            instance: inst.id,
            card: CardId(6),
            controller: PlayerId(0),
            tapped: false,
            damage: 0,
            counters: Default::default(),
        });
        PermanentId(id)
    }

    /// Push a "tap target creature" ability onto the stack aimed at `target`,
    /// with both players already having passed once so the next pass resolves it.
    fn tap_ability_targeting(state: &mut GameState, source: PermanentId, target: Target) {
        let sid = state.mint_id();
        state.stack.push(StackObject {
            id: StackId(sid),
            controller: PlayerId(0),
            kind: StackObjectKind::Ability {
                source,
                effects: vec![Effect::Tap {
                    target: TargetSpec::AnyCreature,
                }],
            },
            targets: vec![target],
        });
    }

    #[test]
    fn a_legal_target_resolves_onto_that_target() {
        // "Tap target creature" aimed at a creature still on the battlefield taps
        // exactly that creature.
        let db = db();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let creature = creature_on_battlefield(&mut state);
        tap_ability_targeting(&mut state, creature, Target::Permanent(creature));

        let state = apply_action(&state, &Action::PassPriority, &db);
        let state = apply_action(&state, &Action::PassPriority, &db);

        assert!(state.stack.is_empty());
        let perm = state.battlefield.iter().find(|p| p.id == creature).unwrap();
        assert!(perm.tapped);
    }

    #[test]
    fn an_object_whose_target_became_illegal_fizzles() {
        // The chosen creature leaves the battlefield before the ability resolves.
        // With its only target now illegal the ability is removed from the stack
        // without effect (CR 608.2b) — nothing is tapped.
        let db = db();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let creature = creature_on_battlefield(&mut state);
        // A second, untargeted creature to prove resolution touches nothing.
        let bystander = creature_on_battlefield(&mut state);
        tap_ability_targeting(&mut state, creature, Target::Permanent(creature));

        // The targeted creature is gone by the time the ability would resolve.
        state.battlefield.retain(|p| p.id != creature);

        let state = apply_action(&state, &Action::PassPriority, &db);
        let state = apply_action(&state, &Action::PassPriority, &db);

        assert!(state.stack.is_empty());
        // The bystander was never a target and stays untapped: no effect happened.
        let perm = state
            .battlefield
            .iter()
            .find(|p| p.id == bystander)
            .unwrap();
        assert!(!perm.tapped);
    }

    #[test]
    fn resolving_does_not_mutate_the_input_state() {
        // apply_action is pure: resolving a targeted ability leaves the input
        // state untouched (the tap lands only on the returned copy).
        let db = db();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let creature = creature_on_battlefield(&mut state);
        tap_ability_targeting(&mut state, creature, Target::Permanent(creature));
        let state = apply_action(&state, &Action::PassPriority, &db);

        // One pass remains before resolution, so the input still has the ability
        // on its stack and an untapped creature.
        let snapshot = state.clone();
        let _after = apply_action(&state, &Action::PassPriority, &db);
        assert_eq!(state, snapshot);
    }

    #[test]
    fn target_legality_tracks_current_state() {
        // The predicate re-derives legality from whatever state it is handed.
        let db = db();
        let mut state = GameState::new_two_player();
        let creature = creature_on_battlefield(&mut state);
        let target = Target::Permanent(creature);

        // Legal while the creature is present…
        assert!(target_is_legal(
            TargetSpec::AnyCreature,
            target,
            &state,
            &db
        ));
        assert!(target_is_legal(
            TargetSpec::AnyPermanent,
            target,
            &state,
            &db
        ));
        // …a player is a legal AnyPlayer target, but not an AnyCreature one.
        assert!(target_is_legal(
            TargetSpec::AnyPlayer,
            Target::Player(PlayerId(1)),
            &state,
            &db
        ));
        assert!(!target_is_legal(
            TargetSpec::AnyCreature,
            Target::Player(PlayerId(1)),
            &state,
            &db
        ));

        // …and illegal once it is gone.
        state.battlefield.clear();
        assert!(!target_is_legal(
            TargetSpec::AnyCreature,
            target,
            &state,
            &db
        ));
    }
}
