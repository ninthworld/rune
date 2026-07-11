//! The state-transition pipeline: [`apply_action`] and its per-action helpers.
//!
//! [`apply_action`] is the single entry point of the engine. It validates the
//! chosen action against [`crate::valid_actions`], clones, applies the action,
//! runs replacement effects, state-based actions, and trigger collection, and
//! returns the new state. Pure over an immutable [`crate::GameState`].

use crate::ability::{is_mana_ability, Ability, Cost, Effect, Target};
use crate::actions::{action_is_legal, Action};
use crate::card::abilities_of;
use crate::id::{CardInstance, CardInstanceId, PermanentId, PlayerId};
use crate::mana::parse_mana_cost;
use crate::mulligan::advance_after_keep;
use crate::phase::Step;
use crate::player::MAX_HAND_SIZE;
use crate::resolve::resolve_stack_object;
use crate::rng::SplitMix64;
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
    // 1. Validate against the actions actually on offer, including — for a
    //    targeted action — its chosen targets against freshly computed legal sets
    //    (ADR 0009 §Enumeration). An illegal action is a no-op: the input is
    //    returned unchanged rather than erroring.
    if !action_is_legal(state, action, db) {
        return state.clone();
    }

    // 2. Clone: every mutation below happens on this owned copy.
    let mut next = state.clone();

    // 3. Apply the chosen action.
    match action {
        Action::PassPriority => apply_pass_priority(&mut next, db),
        Action::PlayLand { card } => apply_play_land(&mut next, *card),
        Action::ActivateAbility {
            permanent,
            index,
            targets,
        } => {
            apply_activate_ability(&mut next, *permanent, *index, targets, db);
        }
        Action::CastSpell { card } => apply_cast_spell(&mut next, *card, db),
        Action::Discard { card } => apply_discard(&mut next, *card),
        Action::Mulligan => apply_mulligan(&mut next),
        Action::Keep { bottom } => apply_keep(&mut next, bottom),
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
            // Target choosing on announcement is issue #71; triggers carry none.
            targets: Vec::new(),
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
            advance_through_turn_based_steps(state);
        }
        state.consecutive_passes = 0;
        state.priority = state.active_player;
    } else {
        state.priority = PlayerId((state.priority.0 + 1) % seats);
    }
}

/// Advance the turn structure past every step that neither grants priority nor
/// requires a player choice, performing each entered step's turn-based actions
/// (CR 500.2) along the way, and stop on the first step that does.
///
/// This wraps the pure FSM [`GameState::advance`] with the turn-based-action
/// dimension the FSM deliberately omits. The untap step grants no priority
/// (CR 502.5) and the cleanup step grants none either (CR 514.3) unless the
/// active player still owes a discard (CR 514.1), so both are skipped straight
/// through when nothing pauses on them — a player never has to pass in a step
/// where the rules give no priority. Priority assignment itself stays with the
/// caller. Terminates because every turn passes through a priority step
/// (e.g. upkeep) at most a couple of advances away.
fn advance_through_turn_based_steps(state: &mut GameState) {
    loop {
        *state = state.advance();
        perform_turn_based_actions(state);
        if step_pauses_for_players(state) {
            break;
        }
    }
}

/// Whether the current step stops the turn-structure walk to hand priority to a
/// player (CR 117) or to collect a required player choice.
///
/// Untap never pauses — it grants no priority (CR 502.5). Cleanup pauses only
/// while the active player is over the maximum hand size and thus owes a discard
/// (CR 514.1); otherwise it grants no priority (CR 514.3) and is walked through.
/// Every other step pauses to grant priority.
fn step_pauses_for_players(state: &GameState) -> bool {
    match state.step {
        Step::Untap => false,
        Step::Cleanup => active_player_over_hand_size(state),
        _ => true,
    }
}

/// Whether the active player currently holds more than [`MAX_HAND_SIZE`] cards
/// and so owes a cleanup-step discard (CR 514.1). `false` on a seatless state.
fn active_player_over_hand_size(state: &GameState) -> bool {
    state
        .players
        .get(state.active_player.0)
        .is_some_and(|p| p.hand.len() > MAX_HAND_SIZE)
}

/// Perform the turn-based actions of the step `state` has just entered
/// (CR 500.2). Each is a pure, automatic mutation of the active player's part of
/// the board; player-choice actions (the cleanup discard) are offered through
/// [`crate::valid_actions`] instead. Steps with no modeled turn-based action are
/// a no-op.
fn perform_turn_based_actions(state: &mut GameState) {
    match state.step {
        Step::Untap => untap_active_players_permanents(state),
        Step::Draw => draw_for_turn(state),
        Step::Cleanup => remove_all_marked_damage(state),
        _ => {}
    }
}

/// Untap step turn-based action: untap every permanent the active player controls
/// (CR 502.4). Permanents controlled by other players are unaffected.
fn untap_active_players_permanents(state: &mut GameState) {
    let active = state.active_player;
    for perm in &mut state.battlefield {
        if perm.controller == active {
            perm.tapped = false;
        }
    }
}

/// Draw step turn-based action: the active player draws a card (CR 504.1).
///
/// CR 103.8b: in a two-player game the player who takes the first turn skips the
/// draw step of that turn. Turn 1 is, by construction, always the starting
/// player's first turn, so that first draw is the one skipped. A draw from an
/// empty library is a no-op here; the loss it should cause (CR 104.3c) is a
/// state-based action not yet modeled.
fn draw_for_turn(state: &mut GameState) {
    if state.players.len() == 2 && state.turn == 1 {
        return;
    }
    let active = state.active_player;
    if let Some(player) = state.players.get_mut(active.0) {
        if let Some(card) = player.library.pop() {
            player.hand.push(card);
        }
    }
}

/// Cleanup step turn-based action: remove all damage marked on permanents
/// (CR 514.2). Runs on entry to the step; the discard (CR 514.1) is a separate
/// player choice routed through [`apply_discard`].
fn remove_all_marked_damage(state: &mut GameState) {
    for perm in &mut state.battlefield {
        perm.damage = 0;
    }
}

/// Discard one card from the active player's hand to its owner's graveyard,
/// satisfying part of the cleanup maximum-hand-size turn-based action (CR 514.1).
///
/// Only ever reached during [`Step::Cleanup`] (the action is offered nowhere
/// else — see [`crate::valid_actions`]). When the discard brings the player to
/// the maximum hand size the cleanup step is finished, so the turn structure
/// walks on to the next step that pauses for a player; priority is re-seated by
/// [`apply_action`]'s caller path via the pass handler's assignment, so it is set
/// here too. While the player is still over the limit the step stays put and more
/// discards are offered.
fn apply_discard(state: &mut GameState, card: CardInstance) {
    let active = state.active_player;
    {
        let Some(player) = state.players.get_mut(active.0) else {
            return;
        };
        let Some(pos) = player.hand.iter().position(|&c| c.id == card.id) else {
            return;
        };
        let discarded = player.hand.remove(pos);
        player.graveyard.push(discarded);
    }
    if state.step == Step::Cleanup && !active_player_over_hand_size(state) {
        advance_through_turn_based_steps(state);
        state.consecutive_passes = 0;
        state.priority = state.active_player;
    }
}

/// Take a mulligan during the pre-game London mulligan phase (CR 103.5): shuffle
/// the deciding seat's hand back into its library, redraw a fresh opening hand,
/// and record the mulligan.
///
/// The deciding seat is the priority holder (see [`crate::valid_actions`]).
/// Priority stays with that seat — after redrawing it decides again (keep or
/// mulligan). The reshuffle draws from
/// [`GameState::rng_seed`](crate::GameState::rng_seed) and stores the advanced
/// generator state back, so the whole game still replays from its seed.
fn apply_mulligan(state: &mut GameState) {
    let seat = state.priority;
    let Some(hand_size) = state.mulligan.as_ref().map(|m| m.hand_size) else {
        return;
    };
    // Read the seed, reshuffle-and-redraw for the deciding seat, then store the
    // advanced generator state back into the slot.
    let mut rng = SplitMix64::new(state.rng_seed);
    if let Some(player) = state.players.get_mut(seat.0) {
        player.library.append(&mut player.hand);
        rng.shuffle(&mut player.library);
        let draw = hand_size.min(player.library.len());
        for _ in 0..draw {
            if let Some(card) = player.library.pop() {
                player.hand.push(card);
            }
        }
    }
    state.rng_seed = rng.state();
    if let Some(decision) = state
        .mulligan
        .as_mut()
        .and_then(|m| m.decisions.get_mut(seat.0))
    {
        decision.taken += 1;
    }
}

/// Keep the current hand during the pre-game London mulligan phase (CR 103.5).
///
/// Puts the chosen `bottom` cards (already validated to be exactly one distinct
/// hand card per mulligan taken — see [`action_is_legal`]) on the bottom of the
/// deciding seat's library in the given order, marks the seat as having kept, and
/// hands the decision to the next still-deciding seat. Once every seat has kept
/// the phase ends and turn 1 begins ([`advance_after_keep`]).
fn apply_keep(state: &mut GameState, bottom: &[Target]) {
    let seat = state.priority;
    if let Some(player) = state.players.get_mut(seat.0) {
        // Remove the chosen cards from hand, preserving the chosen order.
        let chosen: Vec<CardInstanceId> = bottom
            .iter()
            .filter_map(|t| match t {
                Target::Card(id) => Some(*id),
                _ => None,
            })
            .collect();
        let mut bottomed = Vec::with_capacity(chosen.len());
        for id in &chosen {
            if let Some(pos) = player.hand.iter().position(|inst| inst.id == *id) {
                bottomed.push(player.hand.remove(pos));
            }
        }
        // Place them on the bottom of the library. The top of the library is the
        // last element, so the bottom is the front: insert the chosen cards there
        // in order (first chosen ends up deepest).
        for (offset, card) in bottomed.into_iter().enumerate() {
            player.library.insert(offset, card);
        }
    }
    if let Some(decision) = state
        .mulligan
        .as_mut()
        .and_then(|m| m.decisions.get_mut(seat.0))
    {
        decision.kept = true;
    }
    advance_after_keep(state);
}

/// Play a land from the active player's hand onto the battlefield. Not via the
/// stack (CR 116.2a); a fresh [`PermanentId`] is minted on entry while the
/// card's [`crate::CardInstanceId`] carries over unchanged.
fn apply_play_land(state: &mut GameState, card: CardInstance) {
    let controller = state.priority;
    {
        let Some(player) = state.players.get_mut(controller.0) else {
            return;
        };
        let Some(pos) = player.hand.iter().position(|&c| c.id == card.id) else {
            return;
        };
        player.hand.remove(pos);
    }
    let id = state.mint_id();
    state.battlefield.push(Permanent {
        id: PermanentId(id),
        instance: card.id,
        card: card.card,
        controller,
        tapped: false,
        damage: 0,
        counters: Default::default(),
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
    targets: &[Target],
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
            // The targets chosen for this activation (CR 601.2c), already
            // validated against freshly computed legal sets in `action_is_legal`
            // and re-checked once more on resolution (CR 608.2b, the resolve
            // path). Empty for a non-targeting ability.
            targets: targets.to_vec(),
        });
        state.consecutive_passes = 0;
    }
}

/// Cast a creature spell: pay its mana cost from the caster's pool, move the card
/// from hand onto the stack, and reset the pass count (the caster keeps priority).
fn apply_cast_spell(state: &mut GameState, card: CardInstance, db: &CardDatabase) {
    let controller = state.priority;
    let Some(data) = db.card(card.card) else {
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
        let Some(pos) = player.hand.iter().position(|&c| c.id == card.id) else {
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
        // Choosing targets when casting is issue #71; none for now.
        targets: Vec::new(),
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
        // A targeting effect: its subject is a chosen target, not the controller,
        // so it is applied via [`apply_targeted_effect`] and is a no-op here.
        Effect::Tap { .. } => {}
    }
}

/// Apply a targeting [`Effect`] to its already-legality-checked chosen
/// [`Target`], on behalf of `controller`.
///
/// The caller (the resolve path) is responsible for re-checking the target's
/// legality first (CR 608.2b) and only invoking this for a target that is still
/// legal; a mismatched target-value kind is a no-op here. Effects with an
/// implicit subject never reach this function — they route through
/// [`apply_effect`].
pub(crate) fn apply_targeted_effect(
    state: &mut GameState,
    effect: &Effect,
    target: Target,
    _controller: PlayerId,
) {
    match effect {
        Effect::Tap { .. } => {
            if let Target::Permanent(id) = target {
                if let Some(perm) = state.battlefield.iter_mut().find(|p| p.id == id) {
                    perm.tapped = true;
                }
            }
        }
        // Implicit-subject effects do not target; they never reach here.
        Effect::AddMana { .. } | Effect::DrawCard { .. } => {}
    }
}

/// Apply replacement effects. Scaffold: no replacement effects exist yet, so
/// this is intentionally a no-op. It marks where the pipeline stage lives.
fn apply_replacements(_state: &mut GameState) {}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::actions::valid_actions;
    use crate::id::CardId;
    use crate::mana::Color;
    use crate::phase::Step;

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
        let forest = hand_instance(&before, 0, CardId(5));
        let _ = apply_action(&before, &Action::PlayLand { card: forest }, &db());
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
        let inst = state.new_instance(CardId(5));
        let id = state.mint_id();
        state.battlefield.push(Permanent {
            id: PermanentId(id),
            instance: inst.id,
            card: CardId(5),
            controller: PlayerId(0),
            tapped: false,
            damage: 0,
            counters: Default::default(),
        });
        let after = apply_action(
            &state,
            &Action::ActivateAbility {
                permanent: PermanentId(id),
                index: 0,
                targets: Vec::new(),
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
        let inst = state.new_instance(CardId(5));
        let id = state.mint_id();
        state.battlefield.push(Permanent {
            id: PermanentId(id),
            instance: inst.id,
            card: CardId(5),
            controller: PlayerId(0),
            tapped: false,
            damage: 0,
            counters: Default::default(),
        });
        let after = apply_action(
            &state,
            &Action::ActivateAbility {
                permanent: PermanentId(id),
                index: 0,
                targets: Vec::new(),
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
        let scout = hand_instance(&state, 0, CardId(6));
        let after = apply_action(&state, &Action::CastSpell { card: scout }, &db);
        assert_eq!(after.stack.len(), 1);
        assert_eq!(after.players[0].mana_pool.green, 0);
        assert!(!after.players[0].hand.iter().any(|c| c.id == scout.id));
    }

    #[test]
    fn issue_card_effects_etb_draw_end_to_end() {
        // Full vertical slice: play Forest, tap for {G}, cast Verdant Scout,
        // resolve it (ETB triggers), then resolve the trigger (controller draws).
        let db = db();
        let state = slice_state();
        let forest_card = hand_instance(&state, 0, CardId(5));
        let scout_card = hand_instance(&state, 0, CardId(6));
        let draw_card = state.players[0].library[0];

        // Play Forest.
        let state = apply_action(&state, &Action::PlayLand { card: forest_card }, &db);
        assert_eq!(state.battlefield.len(), 1);
        assert!(state.land_played);
        // The land keeps its hand instance identity on the battlefield.
        assert_eq!(state.battlefield[0].instance, forest_card.id);
        let forest = state.battlefield[0].id;

        // Tap Forest for {G} (mana ability resolves immediately).
        let state = apply_action(
            &state,
            &Action::ActivateAbility {
                permanent: forest,
                index: 0,
                targets: Vec::new(),
            },
            &db,
        );
        assert!(state.battlefield[0].tapped);
        assert_eq!(state.players[0].mana_pool.green, 1);
        assert!(state.stack.is_empty());
        assert_eq!(state.priority, PlayerId(0));

        // Cast Verdant Scout.
        let state = apply_action(&state, &Action::CastSpell { card: scout_card }, &db);
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
        assert!(state.players[0].hand.contains(&draw_card));
        assert!(state.players[0].library.is_empty());
    }

    #[test]
    fn issue_51_duplicate_cards_have_distinct_instances_and_routable_actions() {
        // Two copies of the same printed card (two Forests) in one hand must be
        // individually addressable: distinct instance ids, one PlayLand action
        // per copy, and applying one action plays that exact copy — not "the
        // first matching copy".
        let db = db();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let forest_a = state.new_instance(CardId(5));
        let forest_b = state.new_instance(CardId(5));
        state.players[0].hand = vec![forest_a, forest_b];

        // Same printed card, but two distinct physical instances.
        assert_eq!(forest_a.card, forest_b.card);
        assert_ne!(forest_a.id, forest_b.id);

        // The engine offers one land action per copy, each naming its own copy.
        let plays: Vec<CardInstance> = valid_actions(&state, &db)
            .into_iter()
            .filter_map(|action| match action {
                Action::PlayLand { card } => Some(card),
                _ => None,
            })
            .collect();
        assert_eq!(plays.len(), 2);
        assert!(plays.contains(&forest_a));
        assert!(plays.contains(&forest_b));

        // Routing the action for the second copy removes exactly that copy,
        // leaving the first untouched in hand.
        let after = apply_action(&state, &Action::PlayLand { card: forest_b }, &db);
        assert_eq!(after.players[0].hand, vec![forest_a]);
        assert_eq!(after.battlefield.len(), 1);
        assert_eq!(after.battlefield[0].instance, forest_b.id);
    }

    // ----- Turn-based actions: untap, draw, cleanup (issue #116) -----

    /// Put a permanent of `card` on the battlefield under `controller`, with the
    /// given tapped and marked-damage state; returns its fresh id.
    fn place_permanent(
        state: &mut GameState,
        card: CardId,
        controller: PlayerId,
        tapped: bool,
        damage: u32,
    ) -> PermanentId {
        let inst = state.new_instance(card);
        let id = state.mint_id();
        state.battlefield.push(Permanent {
            id: PermanentId(id),
            instance: inst.id,
            card,
            controller,
            tapped,
            damage,
            counters: Default::default(),
        });
        PermanentId(id)
    }

    /// Borrow the permanent with id `id`; panics if it is gone.
    fn find_perm(state: &GameState, id: PermanentId) -> &Permanent {
        state.battlefield.iter().find(|p| p.id == id).unwrap()
    }

    /// Both seats pass priority in succession, ending the current step.
    fn pass_full_round(state: &GameState, db: &CardDatabase) -> GameState {
        let s = apply_action(state, &Action::PassPriority, db);
        apply_action(&s, &Action::PassPriority, db)
    }

    #[test]
    fn issue_116_untap_step_untaps_only_the_active_players_permanents() {
        // CR 502.4: the untap step untaps the permanents the active player
        // controls (and only those). CR 502.5: no player receives priority during
        // untap, so the walk never rests there — it proceeds straight to upkeep.
        let db = db();
        let mut state = GameState::new_two_player();
        state.step = Step::End; // player 0's first turn, about to end.
        let p0_perm = place_permanent(&mut state, CardId(5), PlayerId(0), true, 0);
        let p1_perm = place_permanent(&mut state, CardId(5), PlayerId(1), true, 0);

        let after = pass_full_round(&state, &db);

        // The turn passed to player 1; their permanent untapped, player 0's did not.
        assert_eq!(after.turn, 2);
        assert_eq!(after.active_player, PlayerId(1));
        assert!(
            !find_perm(&after, p1_perm).tapped,
            "active player's permanent untaps (CR 502.4)"
        );
        assert!(
            find_perm(&after, p0_perm).tapped,
            "a non-active player's permanent stays tapped (CR 502.4)"
        );
        // Untap granted no priority (CR 502.5): the walk stopped at upkeep.
        assert_eq!(after.step, Step::Upkeep);
        assert_eq!(after.priority, PlayerId(1));
    }

    #[test]
    fn issue_116_draw_step_active_player_draws() {
        // CR 504.1: the active player draws a card as the draw step's turn-based
        // action.
        let db = db();
        let mut state = GameState::new_two_player();
        state.turn = 2;
        state.active_player = PlayerId(1);
        state.priority = PlayerId(1);
        state.step = Step::Upkeep;
        let card = state.new_instance(CardId(1));
        state.players[1].library = vec![card];

        let after = pass_full_round(&state, &db);

        assert_eq!(after.step, Step::Draw);
        assert!(
            after.players[1].hand.contains(&card),
            "the active player drew the top card (CR 504.1)"
        );
        assert!(after.players[1].library.is_empty());
    }

    #[test]
    fn issue_116_starting_player_skips_first_turn_draw() {
        // CR 103.8b: in a two-player game the player who plays first skips the draw
        // step of their first turn. Turn 1 is that first turn, so the library is
        // untouched even though the draw step is entered.
        let db = db();
        let mut state = GameState::new_two_player();
        state.step = Step::Upkeep; // turn 1, player 0 (the starting player).
        let card = state.new_instance(CardId(1));
        state.players[0].library = vec![card];

        let after = pass_full_round(&state, &db);

        assert_eq!(after.step, Step::Draw);
        assert_eq!(
            after.players[0].library,
            vec![card],
            "the first-turn draw is skipped (CR 103.8)"
        );
        assert!(after.players[0].hand.is_empty());
    }

    #[test]
    fn issue_116_cleanup_discards_down_to_max_hand_size_via_a_choice() {
        // CR 514.1: with more than the maximum hand size, the active player
        // discards down to it during cleanup. CR 514.3: no priority is granted, so
        // the only thing offered is the discard — a select-from-zone choice, one
        // Discard per card in hand, never an automatic discard.
        let db = db();
        let mut state = GameState::new_two_player();
        state.step = Step::End; // player 0, turn 1.
        let hand: Vec<CardInstance> = (0..9).map(|_| state.new_instance(CardId(1))).collect();
        state.players[0].hand = hand.clone();

        // Ending the turn walks into cleanup and stops for the discard.
        let at_cleanup = pass_full_round(&state, &db);
        assert_eq!(at_cleanup.step, Step::Cleanup);
        assert_eq!(at_cleanup.active_player, PlayerId(0));
        assert_eq!(at_cleanup.priority, PlayerId(0));
        let choices = valid_actions(&at_cleanup, &db);
        assert!(
            !choices.contains(&Action::PassPriority),
            "cleanup grants no priority (CR 514.3)"
        );
        assert_eq!(choices.len(), 9, "one discard choice per card in hand");
        assert!(choices.iter().all(|a| matches!(a, Action::Discard { .. })));

        // Discard two specific cards; the second brings the hand to the maximum,
        // so cleanup completes and the turn advances to player 1.
        let s = apply_action(&at_cleanup, &Action::Discard { card: hand[0] }, &db);
        assert_eq!(
            s.step,
            Step::Cleanup,
            "still over the limit after one discard"
        );
        assert_eq!(s.players[0].hand.len(), 8);
        let s = apply_action(&s, &Action::Discard { card: hand[1] }, &db);

        assert_eq!(
            s.players[0].hand.len(),
            MAX_HAND_SIZE,
            "discarded to the max (CR 514.1)"
        );
        assert_eq!(s.players[0].graveyard.len(), 2);
        assert!(s.players[0].graveyard.contains(&hand[0]));
        assert!(s.players[0].graveyard.contains(&hand[1]));
        // Cleanup finished with no priority granted; the next turn has begun.
        assert_eq!(s.turn, 2);
        assert_eq!(s.active_player, PlayerId(1));
        assert_eq!(s.step, Step::Upkeep);
    }

    #[test]
    fn issue_116_cleanup_at_or_under_max_hand_size_needs_no_discard() {
        // CR 514.1 applies only when over the maximum: a hand at the limit walks
        // straight through cleanup with no discard offered.
        let db = db();
        let mut state = GameState::new_two_player();
        state.step = Step::End; // player 0, turn 1.
        let hand: Vec<CardInstance> = (0..MAX_HAND_SIZE)
            .map(|_| state.new_instance(CardId(1)))
            .collect();
        state.players[0].hand = hand;

        let after = pass_full_round(&state, &db);

        // No discard: the turn advanced with the hand intact.
        assert_eq!(after.players[0].hand.len(), MAX_HAND_SIZE);
        assert!(after.players[0].graveyard.is_empty());
        assert_eq!(after.turn, 2);
        assert_eq!(after.active_player, PlayerId(1));
        assert_eq!(after.step, Step::Upkeep);
    }

    #[test]
    fn issue_116_cleanup_removes_marked_damage() {
        // CR 514.2: all damage marked on permanents is removed during cleanup.
        let db = db();
        let mut state = GameState::new_two_player();
        state.step = Step::End; // player 0, turn 1; hand empty so no discard.
        let perm = place_permanent(&mut state, CardId(5), PlayerId(0), false, 3);

        let after = pass_full_round(&state, &db);

        assert_eq!(
            find_perm(&after, perm).damage,
            0,
            "marked damage is wiped at cleanup (CR 514.2)"
        );
    }
}
