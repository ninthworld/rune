//! Stack resolution: turning the top object of the stack into its effect.
//!
//! When all players pass priority in succession, [`crate::apply_action`] pops
//! the top of the stack and hands it to [`resolve_stack_object`], which routes a
//! spell by its card types and applies an ability's effects.

use crate::apply::apply_effect;
use crate::card::CardData;
use crate::id::PermanentId;
use crate::stack::{StackObject, StackObjectKind};
use crate::state::{GameState, Permanent};
use crate::CardDatabase;

/// Resolve one object popped from the top of the stack.
pub(crate) fn resolve_stack_object(state: &mut GameState, object: StackObject, db: &CardDatabase) {
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::actions::Action;
    use crate::apply_action;
    use crate::id::{CardId, PlayerId};
    use crate::mana::Color;
    use crate::phase::Step;
    use crate::stack::StackId;

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
}
