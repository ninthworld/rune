//! Priority-automation support (issue #264, ADR 0020): the engine-side judgment of
//! whether the current priority holder has any *meaningful* action beyond passing.
//!
//! This is the one rules question the client is forbidden to answer
//! (`docs/design/ui-requirements.md`, "Stack, priority, and timers": "The client
//! cannot decide that a player has no meaningful response"). Deciding that a lone
//! `pass_priority` is safe depends on the seat's [`valid_actions`], the stack, and
//! timing — engine knowledge. The engine stays pure: it exposes the *predicate*
//! only; the *loop* that keeps auto-passing and the per-seat stop preferences that
//! gate it live in the room layer (ADR 0002 keeps loops, policy, and I/O out of the
//! engine).

use crate::ability::{is_mana_ability, Ability, Effect};
use crate::actions::{valid_actions, Action};
use crate::card::abilities_of;
use crate::id::PermanentId;
use crate::state::GameState;
use crate::CardDatabase;

/// Whether the current priority holder has **no meaningful action** available —
/// nothing to do but pass, concede, or float mana that would go unspent.
///
/// The one subtlety is *potential* mana. The engine offers a `CastSpell` only once
/// its cost is payable from mana **already** in the pool (CR 117.1a is checked
/// against the current pool), but a player taps lands on demand: a seat sitting on
/// untapped lands with an empty pool and a castable spell in hand still has a real
/// play. So this predicate first floats every point of mana the seat's untapped
/// sources could produce, then asks whether *any* non-idle action would then be on
/// offer — a spell it could now cast, a land it can play, or a non-mana ability. If
/// only passes, concedes, and mana abilities remain, passing truly is its only move.
/// Over-estimating the mana (adding every source's output, ignoring that a source
/// taps for one thing) only ever errs toward *not* auto-passing, so the predicate is
/// conservative: it never reports "no meaningful action" for a seat that could act.
///
/// Every forced turn-based choice (a combat declaration, the cleanup discard, a
/// mulligan decision) is advertised *without* a `PassPriority` alongside, so the
/// predicate short-circuits to `false` there — a seat is never auto-passed out of a
/// choice it owes. Returns `false` when no one holds priority or the game is over.
/// Because a `true` result requires `PassPriority` to be on offer, the predicate can
/// only fire where passing is a move the seat is already entitled to take (ADR 0020).
/// Pure over `state` and `db` (it works on a clone), so it is deterministic.
#[must_use]
pub fn priority_has_no_meaningful_action(state: &GameState, db: &CardDatabase) -> bool {
    if state.priority_holder().is_none() || state.is_over() {
        return false;
    }
    // Judge against a copy in which the seat has floated all the mana its untapped
    // sources could make, so a "castable once I tap" spell counts as a real action.
    let mut hypothetical = state.clone();
    float_potential_mana(&mut hypothetical, db);
    let actions = valid_actions(&hypothetical, db);
    // A window that offers no pass at all is a forced choice (combat declaration,
    // cleanup discard, mulligan decision): never idle, never auto-passed.
    if !actions.iter().any(|a| matches!(a, Action::PassPriority)) {
        return false;
    }
    actions
        .iter()
        .all(|action| is_idle_action(&hypothetical, db, action))
}

/// Add to the priority seat's mana pool every unit of mana its untapped permanents
/// could produce via their mana abilities (CR 605.1). A deliberate *over*-estimate —
/// it sums the output of every mana ability of every untapped source, ignoring that
/// a source can only be tapped for one of them — because over-estimating mana can
/// only make more spells look castable, which makes the seat look *less* idle: the
/// safe direction (never auto-pass a seat that might have had a play).
fn float_potential_mana(state: &mut GameState, db: &CardDatabase) {
    let seat = state.priority;
    let mut produced: Vec<Effect> = Vec::new();
    for perm in &state.battlefield {
        if perm.controller != seat || perm.tapped {
            continue;
        }
        for ability in abilities_of(db, perm.card) {
            if is_mana_ability(&ability) {
                if let Ability::Activated { effects, .. } = ability {
                    produced.extend(effects);
                }
            }
        }
    }
    let Some(player) = state.players.get_mut(seat.0) else {
        return;
    };
    for effect in &produced {
        match effect {
            Effect::AddMana { color, amount } => player.mana_pool.add(*color, *amount),
            Effect::AddColorlessMana { amount } => player.mana_pool.add_colorless(*amount),
            _ => {}
        }
    }
}

/// Whether a single offered action is "idle" — a pass, a concede, or a mana ability
/// (see [`priority_has_no_meaningful_action`]). Every other action is meaningful.
fn is_idle_action(state: &GameState, db: &CardDatabase, action: &Action) -> bool {
    match action {
        Action::PassPriority | Action::Concede => true,
        Action::ActivateAbility {
            permanent, index, ..
        } => is_mana_ability_action(state, db, *permanent, *index),
        _ => false,
    }
}

/// Whether activating ability `index` of `permanent` is a mana ability (CR 605) —
/// the one activated-ability shape [`is_idle_action`] treats as idle. `false` for a
/// permanent that has since left the battlefield or an out-of-range index.
fn is_mana_ability_action(
    state: &GameState,
    db: &CardDatabase,
    permanent: PermanentId,
    index: usize,
) -> bool {
    let Some(perm) = state.battlefield.iter().find(|p| p.id == permanent) else {
        return false;
    };
    abilities_of(db, perm.card)
        .get(index)
        .is_some_and(is_mana_ability)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::fixtures::fixture;
    use crate::id::{CardId, PlayerId};
    use crate::mana::Color;
    use crate::phase::Step;
    use crate::state::Permanent;

    fn db() -> CardDatabase {
        CardDatabase::bundled().unwrap()
    }

    /// Put a permanent of `card` on the battlefield under `controller` (untapped).
    fn place(state: &mut GameState, card: CardId, controller: PlayerId) -> PermanentId {
        let inst = state.new_instance(card);
        let id = state.mint_id();
        state.battlefield.push(Permanent {
            id: PermanentId(id),
            instance: inst.id,
            card,
            controller,
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

    #[test]
    fn a_bare_priority_holder_with_only_pass_is_idle() {
        // Nothing to do but pass (and concede): idle.
        let state = GameState::new_two_player();
        assert!(priority_has_no_meaningful_action(&state, &db()));
    }

    #[test]
    fn a_seat_holding_only_untapped_lands_is_idle() {
        // Untapped lands offer a mana ability, which is not meaningful on its own —
        // so the seat is still idle and safe to auto-pass.
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        place(&mut state, fixture("forest"), PlayerId(0));
        place(&mut state, fixture("forest"), PlayerId(0));
        assert!(priority_has_no_meaningful_action(&state, &db()));
    }

    #[test]
    fn a_seat_with_a_castable_spell_is_not_idle() {
        // A playable land or castable creature in hand is a meaningful action.
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let land = state.new_instance(fixture("forest"));
        state.players[0].hand = vec![land];
        assert!(
            !priority_has_no_meaningful_action(&state, &db()),
            "a playable land is a meaningful action"
        );

        // With mana available, a castable creature is likewise meaningful.
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let scout = state.new_instance(fixture("verdant_scout"));
        state.players[0].hand = vec![scout];
        state.players[0].mana_pool.add(Color::Green, 1);
        assert!(!priority_has_no_meaningful_action(&state, &db()));
    }

    #[test]
    fn a_castable_after_tapping_spell_keeps_a_seat_non_idle() {
        // The potential-mana case: an untapped Forest and a creature in hand the seat
        // cannot yet afford (empty pool). The engine offers no cast until mana floats,
        // but tapping the Forest would pay for it — so the seat is NOT idle and must
        // never be auto-passed past its own play (ADR 0020, the acceptance criterion).
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        place(&mut state, fixture("forest"), PlayerId(0));
        let scout = state.new_instance(fixture("verdant_scout")); // a {G} creature
        state.players[0].hand = vec![scout];
        assert!(
            state.players[0].mana_pool.green == 0,
            "the pool starts empty"
        );
        assert!(
            !priority_has_no_meaningful_action(&state, &db()),
            "a spell castable once the seat taps its land is a meaningful action"
        );
    }

    #[test]
    fn a_seat_holding_an_uncastable_instant_off_turn_is_not_idle() {
        // Off-turn, a seat with an untapped land and an affordable-after-tapping
        // instant keeps priority — the "instant-speed option" acceptance criterion.
        let mut state = GameState::new_two_player();
        state.active_player = PlayerId(0);
        state.priority = PlayerId(1);
        state.step = Step::Upkeep;
        // A blue source and Runic Negation ({U} instant), plus a spell on the stack
        // for the counter to legally target.
        place(&mut state, fixture("island"), PlayerId(1));
        let negation = state.new_instance(fixture("runic_negation"));
        state.players[1].hand = vec![negation];
        let boar = state.new_instance(fixture("thornback_boar"));
        let sid = crate::stack::StackId(state.mint_id());
        state.stack.push(crate::stack::StackObject {
            id: sid,
            controller: PlayerId(0),
            kind: crate::stack::StackObjectKind::Spell { card: boar },
            targets: Vec::new(),
        });
        assert!(
            !priority_has_no_meaningful_action(&state, &db()),
            "an instant castable once the seat taps its land keeps it non-idle"
        );
    }

    #[test]
    fn a_forced_combat_declaration_is_not_idle() {
        // The declare-attackers window offers no pass, only the declaration: never
        // auto-passable (the seat owes a real choice).
        let mut state = GameState::new_two_player();
        state.turn = 2;
        state.step = Step::DeclareAttackers;
        assert!(!priority_has_no_meaningful_action(&state, &db()));
    }

    #[test]
    fn no_priority_holder_is_not_idle() {
        // A seatless/priority-less state has nothing to automate.
        assert!(!priority_has_no_meaningful_action(
            &GameState::default(),
            &db()
        ));
    }

    #[test]
    fn a_terminal_state_is_not_idle() {
        let mut state = GameState::new_two_player();
        state.players[1].has_lost = true;
        assert!(state.is_over());
        assert!(!priority_has_no_meaningful_action(&state, &db()));
    }

    #[test]
    fn a_non_mana_activated_ability_is_meaningful() {
        // A permanent whose only activated ability taps a creature (not a mana
        // ability) keeps its controller non-idle: they have a real play available.
        let json = r#"[
            {"schema_version":1,"functional_id":"tapper","name":"Tapper","types":["artifact"],"mana_cost":"",
             "abilities":[{"type":"activated","cost":[{"kind":"tap"}],
                          "effects":[{"kind":"tap","target":"any_creature"}]}]},
            {"schema_version":1,"functional_id":"bear","name":"Bear","types":["creature"],"mana_cost":"",
             "power":2,"toughness":2}
        ]"#;
        let db = CardDatabase::from_json(json).unwrap();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        place(
            &mut state,
            crate::fixtures::id_in(&db, "tapper"),
            PlayerId(0),
        );
        place(&mut state, crate::fixtures::id_in(&db, "bear"), PlayerId(0));
        assert!(
            !priority_has_no_meaningful_action(&state, &db),
            "a non-mana activated ability is a meaningful action"
        );
    }
}
