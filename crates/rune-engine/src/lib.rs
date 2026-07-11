//! RUNE rules engine — layer 3.
//!
//! Invariants (see AGENTS.md in this crate):
//! - `GameState` is an immutable value type; `apply_action` returns a new state.
//! - No I/O, no async, no globals, no time. Pure functions only.
//! - Everything derivable is computed on demand (pull-based), never cached on objects.

mod ability;
mod card;
mod card_type;
mod id;
mod mana;
mod phase;
mod player;
mod scripted;
mod stack;
mod state;
mod zone;

pub use ability::{is_mana_ability, Ability, Cost, Effect, TriggerCondition};
pub use card::{abilities_of, CardData, CardDatabase};
pub use card_type::{CardType, Supertype};
pub use id::{CardId, PermanentId, PlayerId};
pub use mana::{parse_mana_cost, Color, ManaCost, ManaPool};
pub use phase::Step;
pub use player::{Player, STARTING_LIFE};
pub use stack::{StackId, StackObject, StackObjectKind};
pub use state::{GameState, Permanent};
pub use zone::Zone;

/// An action a player may take. The engine generates the legal set with
/// [`valid_actions`] and validates a chosen action against it in [`apply_action`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Yield priority without taking any other action.
    PassPriority,
    /// Play a land from hand (a special action; lands do not use the stack).
    PlayLand {
        /// The land card in the active player's hand to play.
        card: CardId,
    },
    /// Activate an ability of a permanent the priority holder controls.
    ActivateAbility {
        /// The permanent whose ability is activated.
        permanent: PermanentId,
        /// Index into the permanent's abilities (see [`abilities_of`]).
        index: usize,
    },
    /// Cast a spell from hand, paying its mana cost from the caster's pool.
    CastSpell {
        /// The card in the caster's hand to cast.
        card: CardId,
    },
}

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

/// Enumerate the actions legal for the player who currently holds priority.
///
/// Pull-based and pure: computed fresh from `state`, never cached on it. The
/// priority holder may always pass; may play a land, cast a creature, or (for
/// permanents they control) activate abilities when the relevant timing and cost
/// conditions hold. A state with no valid priority holder offers nothing.
#[must_use]
pub fn valid_actions(state: &GameState, db: &CardDatabase) -> Vec<Action> {
    if state.priority_holder().is_none() {
        return Vec::new();
    }
    let priority = state.priority;
    let mut actions = vec![Action::PassPriority];

    // Sorcery-speed: the active player, in a main phase, with an empty stack.
    let sorcery_speed = priority == state.active_player
        && matches!(state.step, Step::PrecombatMain | Step::PostcombatMain)
        && state.stack.is_empty();

    if let Some(player) = state.players.get(priority.0) {
        // Play a land: at sorcery speed, one per turn.
        if sorcery_speed && !state.land_played {
            for &card in &player.hand {
                if is_land(db, card) {
                    actions.push(Action::PlayLand { card });
                }
            }
        }

        // Cast a creature spell payable from the current pool (sorcery speed).
        if sorcery_speed {
            for &card in &player.hand {
                if let Some(data) = db.card(card) {
                    if is_creature(db, card)
                        && player.mana_pool.can_pay(&parse_mana_cost(&data.mana_cost))
                    {
                        actions.push(Action::CastSpell { card });
                    }
                }
            }
        }
    }

    // Activate abilities of permanents the priority holder controls.
    for perm in &state.battlefield {
        if perm.controller != priority {
            continue;
        }
        for (index, ability) in abilities_of(db, perm.card).iter().enumerate() {
            if let Ability::Activated { cost, .. } = ability {
                if cost_payable(cost, perm) {
                    actions.push(Action::ActivateAbility {
                        permanent: perm.id,
                        index,
                    });
                }
            }
        }
    }

    actions
}

/// The single entry point of the engine: a pure state transition.
///
/// Pipeline: validate `action` against [`valid_actions`] → clone → apply →
/// replacement effects (scaffold) → state-based-actions loop → collect triggers
/// and put them on the stack → return. An action that is not currently legal is
/// rejected as a no-op: the input is returned unchanged (never mutated either
/// way). `db` supplies the immutable oracle data the pipeline reads.
#[must_use]
pub fn apply_action(state: &GameState, action: &Action, db: &CardDatabase) -> GameState {
    // 1. Validate against the actions actually on offer. An illegal action is a
    //    no-op — return the input unchanged rather than erroring.
    if !valid_actions(state, db).contains(action) {
        return state.clone();
    }

    // 2. Clone: every mutation below happens on this owned copy.
    let mut next = state.clone();

    // 3. Apply the chosen action.
    match action {
        Action::PassPriority => apply_pass_priority(&mut next, db),
        Action::PlayLand { card } => apply_play_land(&mut next, *card),
        Action::ActivateAbility { permanent, index } => {
            apply_activate_ability(&mut next, *permanent, *index, db);
        }
        Action::CastSpell { card } => apply_cast_spell(&mut next, *card, db),
    }

    // 4. Replacement effects. Scaffold: no replacement effects are modeled yet,
    //    so this is a documented no-op, wired in for later.
    apply_replacements(&mut next);

    // 5. State-based actions, run to a fixed point.
    run_state_based_actions(&mut next);

    // 6. Collect triggers by diffing before/after and put each on the stack.
    for trigger in collect_triggers(state, &next, db) {
        let id = next.mint_id();
        next.stack.push(StackObject {
            id: StackId(id),
            controller: trigger.controller,
            kind: StackObjectKind::Ability {
                source: trigger.source,
                effects: trigger.effects,
            },
        });
    }

    next
}

/// Resolve a pass of priority. Priority moves to the next seat; once every
/// player has passed in unbroken succession, the top of the stack resolves (if
/// any), otherwise the turn structure advances ([`GameState::advance`]); either
/// way the new active player receives priority.
fn apply_pass_priority(state: &mut GameState, db: &CardDatabase) {
    let seats = state.players.len();
    if seats == 0 {
        return;
    }
    state.consecutive_passes += 1;
    if state.consecutive_passes >= seats {
        if let Some(top) = state.stack.pop() {
            resolve_stack_object(state, top, db);
        } else {
            *state = state.advance();
        }
        state.consecutive_passes = 0;
        state.priority = state.active_player;
    } else {
        state.priority = PlayerId((state.priority.0 + 1) % seats);
    }
}

/// Play a land from the active player's hand onto the battlefield. Not via the
/// stack (CR 116.2a); a fresh [`PermanentId`] is minted on entry.
fn apply_play_land(state: &mut GameState, card: CardId) {
    let controller = state.priority;
    {
        let Some(player) = state.players.get_mut(controller.0) else {
            return;
        };
        let Some(pos) = player.hand.iter().position(|&c| c == card) else {
            return;
        };
        player.hand.remove(pos);
    }
    let id = state.mint_id();
    state.battlefield.push(Permanent {
        id: PermanentId(id),
        card,
        controller,
        tapped: false,
    });
    state.land_played = true;
}

/// Activate ability `index` of `permanent`, paying its costs. A mana ability
/// resolves immediately without using the stack or changing priority (CR 605.3);
/// any other ability goes on the stack and the caster retains priority.
fn apply_activate_ability(
    state: &mut GameState,
    permanent: PermanentId,
    index: usize,
    db: &CardDatabase,
) {
    let Some(perm) = state.battlefield.iter().find(|p| p.id == permanent) else {
        return;
    };
    let controller = perm.controller;
    let card = perm.card;
    let Some(ability) = abilities_of(db, card).get(index).cloned() else {
        return;
    };
    let Ability::Activated { cost, effects } = &ability else {
        return;
    };

    // Pay the costs.
    for c in cost {
        match c {
            Cost::Tap => {
                if let Some(p) = state.battlefield.iter_mut().find(|p| p.id == permanent) {
                    p.tapped = true;
                }
            }
        }
    }

    if is_mana_ability(&ability) {
        // Mana ability: resolve now, no stack object, priority unchanged.
        for effect in effects {
            apply_effect(state, effect, controller);
        }
    } else {
        let id = state.mint_id();
        state.stack.push(StackObject {
            id: StackId(id),
            controller,
            kind: StackObjectKind::Ability {
                source: permanent,
                effects: effects.clone(),
            },
        });
        state.consecutive_passes = 0;
    }
}

/// Cast a creature spell: pay its mana cost from the caster's pool, move the card
/// from hand onto the stack, and reset the pass count (the caster keeps priority).
fn apply_cast_spell(state: &mut GameState, card: CardId, db: &CardDatabase) {
    let controller = state.priority;
    let Some(data) = db.card(card) else {
        return;
    };
    let cost = parse_mana_cost(&data.mana_cost);
    {
        let Some(player) = state.players.get_mut(controller.0) else {
            return;
        };
        let Some(new_pool) = player.mana_pool.pay(&cost) else {
            return;
        };
        let Some(pos) = player.hand.iter().position(|&c| c == card) else {
            return;
        };
        player.hand.remove(pos);
        player.mana_pool = new_pool;
    }
    let id = state.mint_id();
    state.stack.push(StackObject {
        id: StackId(id),
        controller,
        kind: StackObjectKind::Spell { card },
    });
    state.consecutive_passes = 0;
}

/// Resolve one object popped from the top of the stack.
fn resolve_stack_object(state: &mut GameState, object: StackObject, db: &CardDatabase) {
    match object.kind {
        StackObjectKind::Spell { card } => {
            // Route by the resolving card's types (CR 608.3). A permanent spell
            // enters the battlefield with a fresh id; an instant/sorcery creates
            // no Permanent and instead goes to its owner's graveyard (CR 608.2m).
            // The engine does not yet track ownership apart from control, so we
            // use the controller's graveyard on the owner == controller
            // assumption — ownership tracking is future work.
            if db.card(card).is_some_and(CardData::is_permanent) {
                let id = state.mint_id();
                state.battlefield.push(Permanent {
                    id: PermanentId(id),
                    card,
                    controller: object.controller,
                    tapped: false,
                });
            } else if let Some(player) = state.players.get_mut(object.controller.0) {
                player.graveyard.push(card);
            }
        }
        StackObjectKind::Ability { effects, .. } => {
            for effect in &effects {
                apply_effect(state, effect, object.controller);
            }
        }
    }
}

/// Apply a single [`Effect`] to `state` on behalf of `controller`.
fn apply_effect(state: &mut GameState, effect: &Effect, controller: PlayerId) {
    let Some(player) = state.players.get_mut(controller.0) else {
        return;
    };
    match effect {
        Effect::AddMana { color, amount } => player.mana_pool.add(*color, *amount),
        Effect::DrawCard { count } => {
            for _ in 0..*count {
                if let Some(card) = player.library.pop() {
                    player.hand.push(card);
                }
            }
        }
    }
}

/// Whether every cost in `cost` is payable given the source `permanent`'s state.
fn cost_payable(cost: &[Cost], permanent: &Permanent) -> bool {
    cost.iter().all(|c| match c {
        Cost::Tap => !permanent.tapped,
    })
}

/// Whether `card` is a land, by its structured printed types.
fn is_land(db: &CardDatabase, card: CardId) -> bool {
    db.card(card).is_some_and(|c| c.has_type(CardType::Land))
}

/// Whether `card` is a creature, by its structured printed types.
fn is_creature(db: &CardDatabase, card: CardId) -> bool {
    db.card(card)
        .is_some_and(|c| c.has_type(CardType::Creature))
}

/// Apply replacement effects. Scaffold: no replacement effects exist yet, so
/// this is intentionally a no-op. It marks where the pipeline stage lives.
fn apply_replacements(_state: &mut GameState) {}

/// Run state-based actions to a fixed point: keep applying them until a full
/// pass changes nothing. Pure over the owned state. The only rule modeled today
/// is CR 704.5a — a player at 0 or less life loses the game.
fn run_state_based_actions(state: &mut GameState) {
    loop {
        let mut changed = false;
        for player in &mut state.players {
            if player.life <= 0 && !player.has_lost {
                player.has_lost = true;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
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

    /// The bundled card database, for tests that need oracle data.
    fn db() -> CardDatabase {
        CardDatabase::bundled().unwrap()
    }

    /// A two-player game in the precombat main phase with player 0 holding a
    /// Forest and Verdant Scout, and one card to draw in the library.
    fn slice_state() -> GameState {
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        state.players[0].hand = vec![CardId(5), CardId(6)];
        state.players[0].library = vec![CardId(1)];
        state
    }

    #[test]
    fn apply_action_does_not_mutate_input() {
        // PassPriority now changes the state, so the input and output differ —
        // what must hold is that the *input* is untouched (purity).
        let before = GameState::new_two_player();
        let snapshot = before.clone();
        let _after = apply_action(&before, &Action::PassPriority, &db());
        assert_eq!(before, snapshot);
    }

    #[test]
    fn new_actions_do_not_mutate_input() {
        let before = slice_state();
        let snapshot = before.clone();
        let _ = apply_action(&before, &Action::PlayLand { card: CardId(5) }, &db());
        assert_eq!(before, snapshot);
    }

    #[test]
    fn valid_actions_offers_pass_priority_to_the_priority_holder() {
        let state = GameState::new_two_player();
        assert_eq!(valid_actions(&state, &db()), vec![Action::PassPriority]);
    }

    #[test]
    fn valid_actions_on_seatless_state_is_empty() {
        // Default has no players, so no one holds priority and nothing is legal.
        assert!(valid_actions(&GameState::default(), &db()).is_empty());
    }

    #[test]
    fn illegal_action_is_a_no_op() {
        // On a seatless state PassPriority is not on offer; applying it must
        // leave the state unchanged.
        let state = GameState::default();
        let after = apply_action(&state, &Action::PassPriority, &db());
        assert_eq!(after, state);
    }

    #[test]
    fn passing_priority_rotates_before_the_step_ends() {
        // First pass hands priority to the other seat without ending the step.
        let state = GameState::new_two_player();
        let after = apply_action(&state, &Action::PassPriority, &db());
        assert_eq!(after.priority, PlayerId(1));
        assert_eq!(after.consecutive_passes, 1);
        assert_eq!(after.step, Step::Untap);
        assert_eq!(after.active_player, PlayerId(0));
    }

    #[test]
    fn a_full_round_of_passes_advances_the_step() {
        // Both players pass in succession: the step advances and priority
        // returns to the active player with the pass count reset.
        let db = db();
        let state = GameState::new_two_player();
        let state = apply_action(&state, &Action::PassPriority, &db);
        let state = apply_action(&state, &Action::PassPriority, &db);
        assert_eq!(state.step, Step::Upkeep);
        assert_eq!(state.turn, 1);
        assert_eq!(state.active_player, PlayerId(0));
        assert_eq!(state.priority, PlayerId(0));
        assert_eq!(state.consecutive_passes, 0);
    }

    #[test]
    fn state_based_actions_mark_a_player_at_zero_life_as_lost() {
        let mut state = GameState::new_two_player();
        state.players[1].life = 0;
        let after = apply_action(&state, &Action::PassPriority, &db());
        assert!(after.players[1].has_lost);
        assert!(!after.players[0].has_lost);
    }

    #[test]
    fn state_based_actions_reach_a_fixed_point() {
        // Running SBAs on an already-settled state changes nothing (a second
        // application is idempotent), i.e. the loop terminates at a fixed point.
        let db = db();
        let mut state = GameState::new_two_player();
        state.players[0].life = -3;
        let once = apply_action(&state, &Action::PassPriority, &db);
        let twice = apply_action(&once, &Action::PassPriority, &db);
        assert!(once.players[0].has_lost);
        assert_eq!(once.players[0].has_lost, twice.players[0].has_lost);
    }

    #[test]
    fn trigger_diff_yields_nothing_for_a_plain_transition() {
        let before = GameState::new_two_player();
        let after = before.advance();
        assert!(collect_triggers(&before, &after, &db()).is_empty());
    }

    #[test]
    fn forest_mana_ability_adds_green_without_using_the_stack() {
        let db = db();
        let mut state = slice_state();
        let id = state.mint_id();
        state.battlefield.push(Permanent {
            id: PermanentId(id),
            card: CardId(5),
            controller: PlayerId(0),
            tapped: false,
        });
        let after = apply_action(
            &state,
            &Action::ActivateAbility {
                permanent: PermanentId(id),
                index: 0,
            },
            &db,
        );
        assert_eq!(after.players[0].mana_pool.green, 1);
        assert!(after.battlefield[0].tapped);
        assert!(after.stack.is_empty());
    }

    #[test]
    fn mana_ability_does_not_pass_priority() {
        let db = db();
        let mut state = slice_state();
        let id = state.mint_id();
        state.battlefield.push(Permanent {
            id: PermanentId(id),
            card: CardId(5),
            controller: PlayerId(0),
            tapped: false,
        });
        let after = apply_action(
            &state,
            &Action::ActivateAbility {
                permanent: PermanentId(id),
                index: 0,
            },
            &db,
        );
        assert_eq!(after.priority, PlayerId(0));
        assert_eq!(after.consecutive_passes, 0);
    }

    #[test]
    fn casting_a_creature_moves_it_to_the_stack_and_pays_mana() {
        let db = db();
        let mut state = slice_state();
        state.players[0].mana_pool.add(Color::Green, 1);
        let after = apply_action(&state, &Action::CastSpell { card: CardId(6) }, &db);
        assert_eq!(after.stack.len(), 1);
        assert_eq!(after.players[0].mana_pool.green, 0);
        assert!(!after.players[0].hand.contains(&CardId(6)));
    }

    #[test]
    fn resolving_a_creature_spell_puts_it_on_the_battlefield() {
        let db = db();
        let mut state = slice_state();
        state.players[0].mana_pool.add(Color::Green, 1);
        let state = apply_action(&state, &Action::CastSpell { card: CardId(6) }, &db);
        let state = apply_action(&state, &Action::PassPriority, &db);
        let state = apply_action(&state, &Action::PassPriority, &db);
        assert!(state.battlefield.iter().any(|p| p.card == CardId(6)));
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
        let sid = state.mint_id();
        state.stack.push(StackObject {
            id: StackId(sid),
            controller: PlayerId(0),
            kind: StackObjectKind::Spell { card: CardId(100) },
        });

        let state = apply_action(&state, &Action::PassPriority, &db);
        let state = apply_action(&state, &Action::PassPriority, &db);

        assert!(state.stack.is_empty());
        assert!(state.battlefield.is_empty());
        assert_eq!(state.players[0].graveyard, vec![CardId(100)]);
    }

    #[test]
    fn collect_triggers_detects_etb_by_permanent_id_diff() {
        let db = db();
        let before = GameState::new_two_player();
        let mut after = before.clone();
        after.battlefield.push(Permanent {
            id: PermanentId(1),
            card: CardId(6),
            controller: PlayerId(0),
            tapped: false,
        });
        let triggers = collect_triggers(&before, &after, &db);
        assert_eq!(triggers.len(), 1);
        assert_eq!(triggers[0].source, PermanentId(1));
        assert_eq!(triggers[0].effects, vec![Effect::DrawCard { count: 1 }]);
    }

    #[test]
    fn issue_card_effects_etb_draw_end_to_end() {
        // Full vertical slice: play Forest, tap for {G}, cast Verdant Scout,
        // resolve it (ETB triggers), then resolve the trigger (controller draws).
        let db = db();
        let state = slice_state();

        // Play Forest.
        let state = apply_action(&state, &Action::PlayLand { card: CardId(5) }, &db);
        assert_eq!(state.battlefield.len(), 1);
        assert!(state.land_played);
        let forest = state.battlefield[0].id;

        // Tap Forest for {G} (mana ability resolves immediately).
        let state = apply_action(
            &state,
            &Action::ActivateAbility {
                permanent: forest,
                index: 0,
            },
            &db,
        );
        assert!(state.battlefield[0].tapped);
        assert_eq!(state.players[0].mana_pool.green, 1);
        assert!(state.stack.is_empty());
        assert_eq!(state.priority, PlayerId(0));

        // Cast Verdant Scout.
        let state = apply_action(&state, &Action::CastSpell { card: CardId(6) }, &db);
        assert_eq!(state.stack.len(), 1);
        assert_eq!(state.players[0].mana_pool.green, 0);

        // Pass twice: the creature resolves and its ETB trigger goes on the stack.
        let state = apply_action(&state, &Action::PassPriority, &db);
        let state = apply_action(&state, &Action::PassPriority, &db);
        assert!(state.battlefield.iter().any(|p| p.card == CardId(6)));
        assert_eq!(state.stack.len(), 1);

        // Pass twice more: the ETB ability resolves and player 0 draws.
        let state = apply_action(&state, &Action::PassPriority, &db);
        let state = apply_action(&state, &Action::PassPriority, &db);
        assert!(state.stack.is_empty());
        assert!(state.players[0].hand.contains(&CardId(1)));
        assert!(state.players[0].library.is_empty());
    }

    #[test]
    fn new_two_player_initial_invariants() {
        let state = GameState::new_two_player();
        assert_eq!(state.turn, 1);
        assert_eq!(state.active_player, PlayerId(0));
        assert_eq!(state.step, Step::Untap);
        assert_eq!(state.players.len(), 2);
        assert!(state.battlefield.is_empty());
        assert!(state.stack.is_empty());
        assert!(!state.land_played);

        for player in &state.players {
            assert_eq!(player.life, STARTING_LIFE);
            assert!(player.library.is_empty());
            assert!(player.hand.is_empty());
            assert!(player.graveyard.is_empty());
            assert!(player.exile.is_empty());
        }

        // The active player resolves to an actual seat.
        let active = state.active_player().unwrap();
        assert_eq!(active.life, STARTING_LIFE);
    }

    #[test]
    fn default_state_is_empty() {
        let state = GameState::default();
        assert_eq!(state.turn, 0);
        assert_eq!(state.step, Step::Untap);
        assert!(state.players.is_empty());
        // No seats, so there is no active player to borrow.
        assert!(state.active_player().is_none());
    }

    #[test]
    fn step_next_cycles_through_the_turn() {
        // Twelve steps, wrapping back to Untap.
        let mut step = Step::Untap;
        let sequence = [
            Step::Upkeep,
            Step::Draw,
            Step::PrecombatMain,
            Step::BeginCombat,
            Step::DeclareAttackers,
            Step::DeclareBlockers,
            Step::CombatDamage,
            Step::EndCombat,
            Step::PostcombatMain,
            Step::End,
            Step::Cleanup,
            Step::Untap,
        ];
        for expected in sequence {
            step = step.next();
            assert_eq!(step, expected);
        }
    }

    #[test]
    fn advance_walks_one_full_turn_without_rotating() {
        // From Untap, eleven advances reach Cleanup, all within turn 1 for the
        // same active player — no rotation happens mid-turn.
        let mut state = GameState::new_two_player();
        let sequence = [
            Step::Upkeep,
            Step::Draw,
            Step::PrecombatMain,
            Step::BeginCombat,
            Step::DeclareAttackers,
            Step::DeclareBlockers,
            Step::CombatDamage,
            Step::EndCombat,
            Step::PostcombatMain,
            Step::End,
            Step::Cleanup,
        ];
        for expected in sequence {
            state = state.advance();
            assert_eq!(state.step, expected);
            assert_eq!(state.turn, 1);
            assert_eq!(state.active_player, PlayerId(0));
        }
    }

    #[test]
    fn advance_past_cleanup_starts_next_players_turn() {
        let mut state = GameState::new_two_player();
        state.step = Step::Cleanup;

        let next = state.advance();
        assert_eq!(next.turn, 2);
        assert_eq!(next.active_player, PlayerId(1));
        assert_eq!(next.step, Step::Untap);
    }

    #[test]
    fn two_turns_cycle_back_to_the_first_player() {
        // Player 0 (turn 1) -> player 1 (turn 2) -> player 0 (turn 3).
        let mut state = GameState::new_two_player();
        state.step = Step::Cleanup;
        let state = state.advance();
        assert_eq!(state.active_player, PlayerId(1));

        let mut state = state;
        state.step = Step::Cleanup;
        let state = state.advance();
        assert_eq!(state.turn, 3);
        assert_eq!(state.active_player, PlayerId(0));
    }

    #[test]
    fn extra_turn_is_taken_before_normal_rotation() {
        // Active player 0 has an extra turn queued; ending the turn hands the
        // turn back to player 0 rather than rotating to player 1.
        let mut state = GameState::new_two_player().with_extra_turn(PlayerId(0));
        state.step = Step::Cleanup;

        let next = state.advance();
        assert_eq!(next.turn, 2);
        assert_eq!(next.active_player, PlayerId(0));
        assert_eq!(next.step, Step::Untap);
        assert!(next.extra_turns.is_empty());
    }

    #[test]
    fn extra_turns_are_taken_last_in_first_out() {
        // Grant player 1's extra turn, then player 0's: player 0 goes first.
        let mut state = GameState::new_two_player()
            .with_extra_turn(PlayerId(1))
            .with_extra_turn(PlayerId(0));

        state.step = Step::Cleanup;
        let state = state.advance();
        assert_eq!(state.active_player, PlayerId(0));

        let mut state = state;
        state.step = Step::Cleanup;
        let state = state.advance();
        assert_eq!(state.active_player, PlayerId(1));

        // With the queue drained, rotation resumes normally.
        let mut state = state;
        state.step = Step::Cleanup;
        let state = state.advance();
        assert_eq!(state.active_player, PlayerId(0));
    }

    #[test]
    fn extra_step_is_visited_before_the_natural_sequence() {
        // An additional precombat main phase inserted after the postcombat main.
        let mut state = GameState::new_two_player();
        state.step = Step::PostcombatMain;
        let state = state.with_extra_step(Step::PrecombatMain);

        let next = state.advance();
        assert_eq!(next.step, Step::PrecombatMain);
        assert_eq!(next.turn, 1);
        assert_eq!(next.active_player, PlayerId(0));
        assert!(next.extra_steps.is_empty());

        // Once the extra step is consumed, the sequence resumes from it.
        assert_eq!(next.advance().step, Step::BeginCombat);
    }

    #[test]
    fn advance_does_not_mutate_input() {
        let before = GameState::new_two_player();
        let _ = before.advance();
        assert_eq!(before.step, Step::Untap);
        assert_eq!(before.turn, 1);
    }

    #[test]
    fn advance_on_seatless_state_does_not_panic() {
        // Default state has no players; ending its turn must not divide by zero.
        let state = GameState {
            step: Step::Cleanup,
            ..GameState::default()
        };
        let next = state.advance();
        assert_eq!(next.turn, 0);
        assert_eq!(next.step, Step::Cleanup);
    }

    #[test]
    fn player_zone_accessor_matches_fields() {
        let mut player = Player::new();
        player.hand.push(CardId(7));
        player.graveyard.push(CardId(9));
        assert_eq!(player.zone(Zone::Hand), &vec![CardId(7)]);
        assert_eq!(player.zone(Zone::Graveyard), &vec![CardId(9)]);
        assert!(player.zone(Zone::Library).is_empty());
        assert!(player.zone(Zone::Exile).is_empty());
    }
}
