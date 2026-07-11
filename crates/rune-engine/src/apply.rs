//! The state-transition pipeline: [`apply_action`] and its per-action helpers.
//!
//! [`apply_action`] is the single entry point of the engine. It validates the
//! chosen action against [`crate::valid_actions`], clones, applies the action,
//! runs replacement effects, state-based actions, and trigger collection, and
//! returns the new state. Pure over an immutable [`crate::GameState`].

use crate::ability::{is_mana_ability, Ability, Cost, Effect};
use crate::actions::{valid_actions, Action};
use crate::card::abilities_of;
use crate::id::{CardId, PermanentId, PlayerId};
use crate::mana::parse_mana_cost;
use crate::resolve::resolve_stack_object;
use crate::sba::run_state_based_actions;
use crate::stack::{StackId, StackObject, StackObjectKind};
use crate::state::{GameState, Permanent};
use crate::triggers::collect_triggers;
use crate::CardDatabase;

/// The single entry point of the engine: a pure state transition.
///
/// Pipeline: validate `action` against [`crate::valid_actions`] → clone → apply →
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

/// Apply a single [`Effect`] to `state` on behalf of `controller`.
pub(crate) fn apply_effect(state: &mut GameState, effect: &Effect, controller: PlayerId) {
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

/// Apply replacement effects. Scaffold: no replacement effects exist yet, so
/// this is intentionally a no-op. It marks where the pipeline stage lives.
fn apply_replacements(_state: &mut GameState) {}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::mana::Color;
    use crate::phase::Step;

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
}
