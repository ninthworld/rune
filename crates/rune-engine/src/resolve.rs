//! Stack resolution: turning the top object of the stack into its effect.
//!
//! When all players pass priority in succession, [`crate::apply_action`] pops
//! the top of the stack and hands it to [`resolve_stack_object`], which first
//! re-checks the object's chosen targets against current state (CR 608.2b), then
//! routes a spell by its card types and applies an ability's effects.

use crate::ability::{Effect, Target, TargetSpec};
use crate::apply::{apply_effect, apply_targeted_effect};
use crate::card::{apply_enters_replacements, spell_effects_of, CardData};
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
        // "Any target" (CR 115.4): legal against a player still in the game or a
        // creature still on the battlefield — the union of the AnyPlayer and
        // AnyCreature checks above (printed types are authoritative here too).
        (TargetSpec::AnyTarget, Target::Player(player)) => {
            state.players.get(player.0).is_some_and(|p| !p.has_lost)
        }
        (TargetSpec::AnyTarget, Target::Permanent(id)) => state.battlefield.iter().any(|p| {
            p.id == id
                && db
                    .card(p.card)
                    .is_some_and(|c| c.has_type(CardType::Creature))
        }),
        // A spell target is legal while that exact spell is still on the stack
        // (CR 701.5): once it has resolved (or been countered) it is gone, so a
        // counterspell aimed at it fizzles (CR 608.2b). An ability on the stack is
        // not a spell and is never a legal "counter target spell" target.
        (TargetSpec::SpellOnStack, Target::Spell(id)) => state
            .stack
            .iter()
            .any(|o| o.id == id && matches!(o.kind, StackObjectKind::Spell { .. })),
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
    // The effects this object resolves, and the specs the stored targets were
    // chosen for (same order the targeting effects consume them). An ability
    // carries its effects on the stack object; a spell's effects are read from
    // its card's spell IR ([`spell_effects_of`], CR 601.2c/608.2c).
    let effects: Vec<Effect> = match &object.kind {
        StackObjectKind::Ability { effects, .. } => effects.clone(),
        StackObjectKind::Spell { card } => spell_effects_of(db, card.card),
    };
    // The specs the stored targets were chosen for (CR 601.2c), in slot order.
    // An ability's specs come from its effects; a spell's include any spell-effect
    // targets **and** an Aura's enchant restriction (CR 303.4a), which is chosen as
    // a target at cast though it produces no `Effect` — so a fizzled Aura target is
    // re-checked on the same path as any other (CR 608.2b).
    let specs: Vec<TargetSpec> = match &object.kind {
        StackObjectKind::Ability { .. } => effects.iter().filter_map(Effect::target_spec).collect(),
        StackObjectKind::Spell { card } => db
            .card(card.card)
            .map(CardData::cast_target_specs)
            .unwrap_or_default(),
    };

    // CR 608.2b: if the object chose targets and *every* one is now illegal, it
    // is removed from the stack without resolving — none of its effects occur. A
    // fizzled *spell* still leaves the stack for its owner's graveyard (it is a
    // card that failed to resolve); a fizzled ability simply ceases to exist.
    if !specs.is_empty()
        && specs
            .iter()
            .zip(&object.targets)
            .all(|(&spec, &target)| !target_is_legal(spec, target, state, db))
    {
        if let StackObjectKind::Spell { card } = object.kind {
            if let Some(player) = state.players.get_mut(object.controller.0) {
                player.graveyard.push(card);
            }
        }
        return;
    }

    // Apply the object's effects, pairing each targeting effect with the next
    // stored target and applying it only while that target is still legal;
    // individually-illegal targets are skipped (CR 608.2c) while legal ones
    // resolve. Effects with an implicit subject apply unconditionally.
    apply_effects_with_targets(state, &effects, &object.targets, object.controller, db);

    // A spell additionally leaves the stack for its final zone (CR 608.3). A
    // permanent spell enters the battlefield with a fresh id (its instance id
    // carries over); an instant/sorcery creates no Permanent and instead goes to
    // its owner's graveyard as the same instance (CR 608.2m). Ownership apart from
    // control is not tracked yet, so the controller's graveyard stands in on the
    // owner == controller assumption. An ability has no card to move.
    if let StackObjectKind::Spell { card } = object.kind {
        if db.card(card.card).is_some_and(CardData::is_permanent) {
            let id = state.mint_id();
            let entered_turn = state.turn;
            // An Aura enters attached to the object its enchant ability chose
            // (CR 303.4d). That target was picked at cast (CR 601.2c) and re-checked
            // just above (CR 608.2b), so an already-illegal one has fizzled and only
            // a legal host remains; a non-Aura permanent enters attached to nothing.
            let attached_to = if db.card(card.card).is_some_and(|c| c.aura.is_some()) {
                object.targets.iter().find_map(|t| match t {
                    crate::ability::Target::Permanent(host) => Some(*host),
                    _ => None,
                })
            } else {
                None
            };
            let mut permanent = Permanent {
                id: PermanentId(id),
                instance: card.id,
                card: card.card,
                controller: object.controller,
                tapped: false,
                entered_turn,
                attacking: false,
                blocking: None,
                damage: 0,
                counters: Default::default(),
                attached_to,
            };
            // CR 614.1c/614.12: apply the permanent's own enters-the-battlefield
            // replacements (e.g. "enters tapped", "enters with N +1/+1 counters") as
            // it enters — before the SBA loop and before ETB triggers are collected —
            // so a 0/0 that enters with two +1/+1 counters is a 2/2 and lives.
            apply_enters_replacements(db, &mut permanent);
            state.battlefield.push(permanent);
        } else if let Some(player) = state.players.get_mut(object.controller.0) {
            player.graveyard.push(card);
        }
    }
}

/// Apply `effects` in order on behalf of `controller`, pairing each targeting
/// effect with the next entry of `stored` targets. A targeting effect applies
/// only while its chosen target is still legal against current state (CR 608.2c —
/// individually-illegal targets are skipped); an implicit-subject effect always
/// applies. Shared by spell and ability resolution so both walk targets the same
/// way.
fn apply_effects_with_targets(
    state: &mut GameState,
    effects: &[Effect],
    stored: &[Target],
    controller: crate::id::PlayerId,
    db: &CardDatabase,
) {
    let mut targets = stored.iter();
    for effect in effects {
        match effect.target_spec() {
            Some(spec) => {
                if let Some(&target) = targets.next() {
                    if target_is_legal(spec, target, state, db) {
                        apply_targeted_effect(state, effect, target, controller);
                    }
                }
            }
            None => apply_effect(state, effect, controller),
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
        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: scout,
                targets: Vec::new(),
            },
            &db,
        );
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
            entered_turn: 0,
            attacking: false,
            blocking: None,
            damage: 0,
            counters: Default::default(),
            attached_to: None,
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

    #[test]
    fn issue_148_spell_on_stack_target_is_legal_only_while_the_spell_is_on_the_stack() {
        // CR 701.5: a "counter target spell" target is legal while that exact spell
        // is on the stack and illegal once it has left (resolved/countered). An
        // ability on the stack is not a spell and is never a legal target.
        let db = db();
        let mut state = GameState::new_two_player();
        let spell = state.new_instance(CardId(1));
        let sid = StackId(state.mint_id());
        state.stack.push(StackObject {
            id: sid,
            controller: PlayerId(0),
            kind: StackObjectKind::Spell { card: spell },
            targets: Vec::new(),
        });
        // An ability sharing the stack is not a spell target.
        let aid = StackId(state.mint_id());
        state.stack.push(StackObject {
            id: aid,
            controller: PlayerId(0),
            kind: StackObjectKind::Ability {
                source: crate::id::PermanentId(1),
                effects: vec![Effect::DrawCard { count: 1 }],
            },
            targets: Vec::new(),
        });

        assert!(target_is_legal(
            TargetSpec::SpellOnStack,
            Target::Spell(sid),
            &state,
            &db
        ));
        assert!(
            !target_is_legal(TargetSpec::SpellOnStack, Target::Spell(aid), &state, &db),
            "an ability on the stack is not a spell"
        );

        // Once the spell leaves the stack it is no longer a legal target.
        state.stack.retain(|o| o.id != sid);
        assert!(!target_is_legal(
            TargetSpec::SpellOnStack,
            Target::Spell(sid),
            &state,
            &db
        ));
    }

    #[test]
    fn issue_149_any_target_is_legal_for_creatures_and_in_game_players() {
        // CR 115.4: an "any target" is a creature or an in-game player. A player
        // who has left the game and a non-creature permanent are both illegal.
        let db = db();
        let mut state = GameState::new_two_player();
        let creature = creature_on_battlefield(&mut state);
        assert!(target_is_legal(
            TargetSpec::AnyTarget,
            Target::Permanent(creature),
            &state,
            &db
        ));
        assert!(target_is_legal(
            TargetSpec::AnyTarget,
            Target::Player(PlayerId(0)),
            &state,
            &db
        ));

        // A non-creature permanent (a Forest) is not an "any target".
        let inst = state.new_instance(CardId(5));
        let forest = PermanentId(state.mint_id());
        state.battlefield.push(Permanent {
            id: forest,
            instance: inst.id,
            card: CardId(5),
            controller: PlayerId(0),
            tapped: false,
            entered_turn: 0,
            attacking: false,
            blocking: None,
            damage: 0,
            counters: Default::default(),
            attached_to: None,
        });
        assert!(!target_is_legal(
            TargetSpec::AnyTarget,
            Target::Permanent(forest),
            &state,
            &db
        ));

        // A player who has lost is no longer a legal target.
        state.players[1].has_lost = true;
        assert!(!target_is_legal(
            TargetSpec::AnyTarget,
            Target::Player(PlayerId(1)),
            &state,
            &db
        ));
    }

    #[test]
    fn issue_149_put_counters_ability_lands_on_its_target_cr_122() {
        // The PutCounters verb runs through the *ability* resolution path exactly
        // as it does through a spell: a "+1/+1 counter on target creature" ability
        // adds one counter to the chosen creature.
        use crate::state::CounterKind;
        let db = db();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let creature = creature_on_battlefield(&mut state);
        let sid = state.mint_id();
        state.stack.push(StackObject {
            id: StackId(sid),
            controller: PlayerId(0),
            kind: StackObjectKind::Ability {
                source: creature,
                effects: vec![Effect::PutCounters {
                    target: TargetSpec::AnyCreature,
                    counter: CounterKind::PlusOnePlusOne,
                    count: 1,
                }],
            },
            targets: vec![Target::Permanent(creature)],
        });

        let state = apply_action(&state, &Action::PassPriority, &db);
        let state = apply_action(&state, &Action::PassPriority, &db);

        let perm = state.battlefield.iter().find(|p| p.id == creature).unwrap();
        assert_eq!(perm.counter_count(CounterKind::PlusOnePlusOne), 1);
    }

    // ----- Auras: enchant, attachment, and fizzle (issue #152) -----
    //
    // Fixture oracle ids: 29 Ironbark Aegis ({1}{G} Aura, "+2/+2, enchant
    // creature"), 6 Verdant Scout (1/1 creature).
    const AURA_BUFF: CardId = CardId(29);

    #[test]
    fn issue_152_aura_resolves_attached_to_its_target_and_boosts_it_cr_303_4d() {
        // CR 303.4d: a resolving Aura enters the battlefield attached to the object
        // its enchant ability chose, and its +2/+2 grant folds into the host's
        // current P/T (CR 613.7c). Verdant Scout is a printed 1/1 -> 3/3 enchanted.
        use crate::characteristics::characteristics;
        let db = db();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let host = creature_on_battlefield(&mut state); // Verdant Scout, 1/1
        let aura = state.new_instance(AURA_BUFF);
        state.players[0].hand = vec![aura];
        state.players[0].mana_pool.add(Color::Green, 1);
        state.players[0].mana_pool.colorless = 1;

        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: aura,
                targets: vec![Target::Permanent(host)],
            },
            &db,
        );
        // Both players pass: the Aura resolves onto the battlefield attached.
        let state = apply_action(&state, &Action::PassPriority, &db);
        let state = apply_action(&state, &Action::PassPriority, &db);

        let aura_perm = state
            .battlefield
            .iter()
            .find(|p| p.card == AURA_BUFF)
            .unwrap();
        assert_eq!(
            aura_perm.attached_to,
            Some(host),
            "the Aura entered attached to its cast-time target (CR 303.4d)"
        );
        let ch = characteristics(&state, host, &db);
        assert_eq!(ch.power, Some(3), "printed 1 + Aura's +2");
        assert_eq!(ch.toughness, Some(3));
    }

    #[test]
    fn issue_152_aura_fizzles_when_its_target_left_before_resolution_cr_608_2b() {
        // CR 608.2b: with its only target (the enchant object) now illegal, the Aura
        // spell is removed from the stack without resolving — it never enters the
        // battlefield, and (a card that failed to resolve) goes to the graveyard.
        let db = db();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let host = creature_on_battlefield(&mut state);
        let aura = state.new_instance(AURA_BUFF);
        state.players[0].hand = vec![aura];
        state.players[0].mana_pool.add(Color::Green, 1);
        state.players[0].mana_pool.colorless = 1;

        let mut state = apply_action(
            &state,
            &Action::CastSpell {
                card: aura,
                targets: vec![Target::Permanent(host)],
            },
            &db,
        );
        // The chosen host is killed in response, before the Aura resolves.
        state.battlefield.retain(|p| p.id != host);

        let state = apply_action(&state, &Action::PassPriority, &db);
        let state = apply_action(&state, &Action::PassPriority, &db);

        assert!(state.stack.is_empty());
        assert!(
            !state.battlefield.iter().any(|p| p.card == AURA_BUFF),
            "a fizzled Aura never enters the battlefield (CR 608.2b)"
        );
        assert!(
            state.players[0].graveyard.iter().any(|c| c.id == aura.id),
            "the fizzled Aura spell goes to its owner's graveyard"
        );
    }

    // ----- ETB replacements: enters with counters (issue #155) -----

    #[test]
    fn issue_155_zero_zero_entering_with_two_counters_lives_cr_614_12() {
        // CR 614.12 / 704.5f: a 0/0 that enters with two +1/+1 counters has the
        // counters as part of *entering* — so it is a 2/2 by the time the SBA loop
        // runs and is never put into the graveyard for 0 toughness. Bramble Hatchling
        // (id 32) is a printed 0/0 that enters with two +1/+1 counters.
        use crate::characteristics::characteristics;
        use crate::state::CounterKind;
        let db = db();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let hatchling = state.new_instance(CardId(32));
        state.players[0].hand = vec![hatchling];
        // Pay {1}{G}.
        state.players[0].mana_pool.add(Color::Green, 1);
        state.players[0].mana_pool.colorless = 1;

        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: hatchling,
                targets: Vec::new(),
            },
            &db,
        );
        // Both players pass: the creature resolves onto the battlefield. The SBA loop
        // runs in this same transition; the counters must already be present.
        let state = apply_action(&state, &Action::PassPriority, &db);
        let state = apply_action(&state, &Action::PassPriority, &db);

        let perm = state
            .battlefield
            .iter()
            .find(|p| p.card == CardId(32))
            .unwrap(); // the 0/0 survives entry with two +1/+1 counters (CR 614.12/704.5f)
        assert_eq!(perm.counter_count(CounterKind::PlusOnePlusOne), 2);
        let ch = characteristics(&state, perm.id, &db);
        assert_eq!(ch.power, Some(2), "0 base + two +1/+1 counters");
        assert_eq!(ch.toughness, Some(2));
        assert!(
            state.players[0].graveyard.is_empty(),
            "no 0-toughness state-based death — it entered as a 2/2"
        );
    }

    #[test]
    fn issue_155_etb_trigger_observes_the_replaced_counters_state_cr_614_12() {
        // CR 614.12: a co-entering enters-the-battlefield trigger observes the replaced
        // state. A synthetic creature that both enters with two +1/+1 counters and has
        // an ETB "draw a card" trigger: on resolution it is already a 2/2 with the two
        // counters AND its ETB trigger is on the stack — both from the one entry event.
        use crate::state::CounterKind;
        let json = r#"[{"id":200,"name":"Test Broodling","types":["creature"],"mana_cost":"","oracle_text":"","power":0,"toughness":0,"abilities":[{"type":"enters_with_counters","counter":"plus_one_plus_one","count":2},{"type":"triggered","event":"self_enters_battlefield","effects":[{"kind":"draw_card","count":1}]}]}]"#;
        let db = CardDatabase::from_json(json).unwrap();

        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let broodling = state.new_instance(CardId(200));
        // A card to draw so the ETB trigger's effect has something to fetch.
        let draw = state.new_instance(CardId(200));
        state.players[0].library = vec![draw];
        let sid = state.mint_id();
        state.stack.push(StackObject {
            id: StackId(sid),
            controller: PlayerId(0),
            kind: StackObjectKind::Spell { card: broodling },
            targets: Vec::new(),
        });

        // Both players pass: the spell resolves — the permanent enters with its
        // counters (the replacement) and its ETB trigger is collected onto the stack.
        let state = apply_action(&state, &Action::PassPriority, &db);
        let state = apply_action(&state, &Action::PassPriority, &db);

        let perm = state
            .battlefield
            .iter()
            .find(|p| p.card == CardId(200))
            .unwrap(); // the creature entered the battlefield
        assert_eq!(
            perm.counter_count(CounterKind::PlusOnePlusOne),
            2,
            "the replaced 'enters with counters' state is present"
        );
        assert_eq!(
            state.stack.len(),
            1,
            "the co-entering ETB trigger was collected against the replaced state"
        );
        assert!(matches!(
            state.stack[0].kind,
            StackObjectKind::Ability { .. }
        ));
    }
}
