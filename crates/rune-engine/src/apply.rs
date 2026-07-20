//! The state-transition pipeline: [`apply_action`] and its per-action helpers.
//!
//! [`apply_action`] is the single entry point of the engine. It validates the
//! chosen action against [`crate::valid_actions`], clones, applies the action,
//! runs state-based actions, and collects triggers, and returns the new state.
//! Enters-the-battlefield self-replacements (CR 614.1c/614.12) are not a stage of
//! this pipeline — they modify the entry event itself and so run at the
//! battlefield-entry seam ([`crate::card::apply_enters_replacements`]). Pure over
//! an immutable [`crate::GameState`].

use crate::ability::{is_mana_ability, Ability, Cost, Effect, PlayerRef, Target};
use crate::actions::{action_is_legal, Action, Attack, Block, DamageOrder};
use crate::card::{abilities_of, apply_enters_replacements, Keyword};
use crate::combat::{
    blocked_attackers, combat_damage, combat_has_first_strike, defending_player, has_keyword,
    pending_blocker_declarer, priority_after_step_change, CombatDamage, DamageStep,
};
use crate::commander::commander_tax_cost;
use crate::id::{CardId, CardInstance, CardInstanceId, PermanentId, PlayerId};
use crate::mana::parse_mana_cost;
use crate::mulligan::advance_after_keep;
use crate::phase::Step;
use crate::player::{LossReason, MAX_HAND_SIZE};
use crate::resolve::resolve_stack_object;
use crate::rng::SplitMix64;
use crate::sba::run_state_based_actions;
use crate::stack::{StackId, StackObject, StackObjectKind};
use crate::state::{
    Duration, EffectAffects, GameEvent, GameState, LoggedPermanent, Modification, Permanent,
    StaticEffect,
};
use crate::triggers::collect_triggers;
use crate::CardDatabase;

/// The single entry point of the engine: a pure state transition.
///
/// Pipeline: validate `action` against [`crate::valid_actions`] → clone → apply
/// (a battlefield entry applies the entering card's own CR 614 self-replacements at
/// its seam) → state-based-actions loop → collect triggers and put them on the stack
/// → return. An action that is not currently legal is rejected as a no-op: the input
/// is returned unchanged (never mutated either way). `db` supplies the immutable
/// oracle data the pipeline reads.
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
        Action::PlayLand { card } => apply_play_land(&mut next, *card, db),
        Action::ActivateAbility {
            permanent,
            index,
            targets,
        } => {
            apply_activate_ability(&mut next, *permanent, *index, targets, db);
        }
        Action::CastSpell { card, targets } => apply_cast_spell(&mut next, *card, targets, db),
        Action::Discard { card } => apply_discard(&mut next, *card, db),
        Action::Mulligan => apply_mulligan(&mut next),
        Action::Keep { bottom } => apply_keep(&mut next, bottom),
        Action::DeclareAttackers { attackers } => {
            apply_declare_attackers(&mut next, attackers, db);
        }
        Action::DeclareBlockers { blocks } => apply_declare_blockers(&mut next, blocks),
        Action::OrderCombatDamage { orders } => apply_order_combat_damage(&mut next, orders),
        Action::ReturnCommanderToCommandZone { card } => {
            apply_return_commander(&mut next, *card);
        }
        Action::DeclineCommanderReturn { card } => apply_decline_commander_return(&mut next, *card),
        Action::Concede => apply_concede(&mut next),
    }

    // Enters-the-battlefield self-replacements (CR 614.1c/614.12 — "enters tapped",
    // "enters with counters") are NOT a stage here: a replacement modifies the entry
    // event itself, so it is applied at the battlefield-entry seam inside step 3
    // (`apply_enters_replacements`), before the state-based-action loop and before any
    // ETB trigger below. That ordering is load-bearing — a 0/0 entering with two +1/+1
    // counters must already be a 2/2 when the SBA loop runs (CR 704.5f).

    // 4. State-based actions, run to a fixed point.
    run_state_based_actions(&mut next, db);

    // 5. Collect triggers by diffing before/after and put each on the stack. They
    //    observe the post-replacement state (the entered permanent already carries
    //    its "as enters" tapped state / counters, CR 614.12).
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

    // The terminal-result event closes the sequence. Every fact that could end the
    // game — a death, damage, a decking draw — has already been recorded at its own
    // seam above, so a `GameOver` recorded here lands last, after its causes. It is
    // derived (never stored, CR 104.2a) and emitted once, the transition it becomes
    // true.
    if state.result().is_none() {
        if let Some(result) = next.result() {
            next.record_event(GameEvent::GameOver { result });
        }
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
    // A full round is one pass from each player still in the game: an eliminated
    // seat neither receives nor passes priority (CR 800.4a), so it is not counted.
    if state.consecutive_passes >= state.living_player_count() {
        if let Some(top) = state.stack.pop() {
            resolve_stack_object(state, top, db);
        } else {
            advance_through_turn_based_steps(state, db);
        }
        state.consecutive_passes = 0;
        // Priority goes to the active player, except that a step whose turn-based
        // action is a pending combat declaration hands the choice to the declaring
        // player first (the defender declares blockers, CR 509.1).
        state.priority = priority_after_step_change(state);
    } else {
        // Skip every eliminated seat when passing priority (CR 800.4a).
        state.priority = state
            .next_living_seat(state.priority)
            .unwrap_or(state.priority);
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
fn advance_through_turn_based_steps(state: &mut GameState, db: &CardDatabase) {
    loop {
        *state = state.advance();
        // The step is entered *before* its turn-based actions run, so the log reads
        // in causal order — `step_changed: draw` precedes the `cards_drawn` the draw
        // step performs, and entering combat damage precedes the damage and deaths
        // it causes. Each iteration records its own transition, so a walk that skips
        // straight through several no-priority steps still logs each one.
        state.record_event(GameEvent::StepChanged {
            turn: state.turn,
            active_player: state.active_player,
            step: state.step,
        });
        perform_turn_based_actions(state, db);
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
fn perform_turn_based_actions(state: &mut GameState, db: &CardDatabase) {
    match state.step {
        Step::Untap => untap_active_players_permanents(state),
        Step::Draw => draw_for_turn(state),
        Step::CombatDamage => deal_combat_damage(state, db),
        Step::EndCombat => remove_creatures_from_combat(state),
        Step::Cleanup => cleanup_turn_based_actions(state),
        _ => {}
    }
}

/// Combat-damage step turn-based action: deal all combat damage (CR 510).
///
/// If any creature in combat has first strike there are **two** damage steps
/// (CR 510.5): first-strikers deal in the first, everyone else in the second, and
/// the state-based-actions loop runs *between* them so a creature killed by first
/// strike is gone before it would deal its regular-step damage. Otherwise a single
/// ordinary step is dealt. Each step's assignments are computed
/// ([`combat_damage`]) then applied in one pass, so the batch lands simultaneously
/// (CR 510.2): damage to a player is life loss (feeding CR 704.5a), damage to a
/// creature is marked (CR 120.3) for CR 704.5g, deathtouch damage additionally
/// flags its recipient for CR 704.5h, and lifelink gains life in the same batch
/// (CR 702.15e). The set of blocked attackers is captured up front so a blocked
/// creature whose blockers died to first strike is not re-read as unblocked
/// (CR 509.1h). The pipeline's state-based-actions loop runs again after this.
fn deal_combat_damage(state: &mut GameState, db: &CardDatabase) {
    // CR 509.1h: which attackers are blocked is fixed before any damage is dealt.
    let blocked = blocked_attackers(state);
    if combat_has_first_strike(state, db) {
        apply_combat_batch(
            state,
            combat_damage(state, db, DamageStep::FirstStrike, &blocked),
        );
        // CR 510.5: SBAs are checked between the two combat-damage steps.
        run_state_based_actions(state, db);
        apply_combat_batch(
            state,
            combat_damage(state, db, DamageStep::Regular, &blocked),
        );
    } else {
        apply_combat_batch(state, combat_damage(state, db, DamageStep::Only, &blocked));
    }
}

/// Apply one combat-damage step's computed batch to `state` (CR 510.2). Life
/// changes and marked damage land together; a deathtouch mark records the
/// recipient for the CR 704.5h state-based action, and lifelink life gain rides
/// the same batch as the damage (CR 702.15e).
fn apply_combat_batch(state: &mut GameState, batch: Vec<CombatDamage>) {
    for assignment in batch {
        match assignment {
            CombatDamage::ToPlayer { player, amount } => {
                // Damage to a player is life loss recorded as a `DamageDealt` event
                // (not a bare life change), so a client can report the hit.
                state.deal_damage_to_player(player, amount);
            }
            CombatDamage::ToPermanent {
                permanent,
                amount,
                deathtouch,
            } => {
                // Marks the damage and records the `DamageDealt` event in one seam.
                let marked = state.mark_damage_on_permanent(permanent, amount);
                // CR 702.2b / 704.5h: any nonzero damage from a deathtouch source
                // makes the recipient a candidate for destruction.
                if marked
                    && deathtouch
                    && amount > 0
                    && !state.deathtouch_struck.contains(&permanent)
                {
                    state.deathtouch_struck.push(permanent);
                }
            }
            CombatDamage::GainLife { player, amount } => {
                // Lifelink life gain is a non-damage life change (CR 702.15e).
                state.change_life(player, i32::try_from(amount).unwrap_or(i32::MAX));
            }
        }
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
/// player's first turn, so that first draw is the one skipped. Drawing from an
/// empty library flags the attempted draw so the state-based-actions loop makes
/// the player lose (CR 704.5c); the flagging lives in [`crate::Player::draw`].
fn draw_for_turn(state: &mut GameState) {
    if state.players.len() == 2 && state.turn == 1 {
        return;
    }
    let active = state.active_player;
    let drew = state
        .players
        .get_mut(active.0)
        .is_some_and(|player| player.draw());
    // Only an actual card moved is logged; a draw from an empty library flags the
    // decking loss (handled in `Player::draw`) but adds no card to report.
    if drew {
        state.record_event(GameEvent::CardsDrawn {
            player: active,
            count: 1,
        });
    }
}

/// CR 104.3a: the priority holder concedes — they leave the game and lose
/// immediately, at any time they could act. Modeled by marking the conceding seat
/// as having lost with [`LossReason::Concede`]; the state-based-actions loop then
/// settles and [`GameState::result`] derives the winner (CR 104.2a).
fn apply_concede(state: &mut GameState) {
    let seat = state.priority;
    if let Some(player) = state.players.get_mut(seat.0) {
        player.has_lost = true;
        player.loss_reason.get_or_insert(LossReason::Concede);
    }
}

/// CR 903.9a (accept): move the priority holder's commander from the graveyard or
/// exile it went to into their command zone, as a fresh object, and log the move.
///
/// The card is found by its stable instance id in whichever of the two zones holds
/// it, removed, and pushed to the command zone; the pending return decision is
/// cleared. Its [`crate::CardInstanceId`] carries over unchanged (as it does for
/// any move between non-battlefield zones), so the commander designation keeps
/// tracking it; a later recast mints a fresh [`crate::PermanentId`] on battlefield
/// entry, which is where "fresh object" is observable. Only ever reached for a
/// commander whose owner holds priority with a pending return (see
/// [`crate::valid_actions`]); a no-op if the card cannot be found.
fn apply_return_commander(state: &mut GameState, card: CardInstance) {
    let owner = state.priority;
    let Some(player) = state.players.get_mut(owner.0) else {
        return;
    };
    // Take the commander out of the graveyard or exile it currently sits in.
    let removed = remove_instance(&mut player.graveyard, card.id)
        .or_else(|| remove_instance(&mut player.exile, card.id));
    let Some(instance) = removed else {
        return;
    };
    player.command.push(instance);
    if let Some(commander) = player.commander.as_mut() {
        commander.return_pending = false;
    }
    state.record_event(GameEvent::CommanderReturnedToCommandZone {
        player: owner,
        card: instance,
    });
}

/// CR 903.9a (decline): leave the commander where it went and clear the pending
/// return decision, so [`crate::valid_actions`] stops offering the choice and
/// normal play resumes. Records nothing — the card did not move.
fn apply_decline_commander_return(state: &mut GameState, _card: CardInstance) {
    let owner = state.priority;
    if let Some(commander) = state
        .players
        .get_mut(owner.0)
        .and_then(|p| p.commander.as_mut())
    {
        commander.return_pending = false;
    }
}

/// Remove and return the first [`CardInstance`] in `pile` with instance id `id`,
/// or `None` if absent. Preserves the order of the remaining cards.
fn remove_instance(pile: &mut Vec<CardInstance>, id: CardInstanceId) -> Option<CardInstance> {
    let pos = pile.iter().position(|c| c.id == id)?;
    Some(pile.remove(pos))
}

/// Cleanup step turn-based action (CR 514.2): **simultaneously** remove all
/// damage marked on permanents and end every "until end of turn" continuous
/// effect. Runs on entry to the step; the discard (CR 514.1) is a separate player
/// choice routed through [`apply_discard`].
///
/// CR 514.2 sequences the damage wipe and the ending of "until end of turn"
/// effects as one simultaneous turn-based action, and — crucially — **no**
/// state-based actions or priority interrupt it (CR 514.3); the pipeline's SBA
/// loop runs only *after* this whole action completes. That simultaneity is the
/// classic pump interaction: a 1/1 pumped to 4/4 that took 3 damage this turn has
/// its pump wear off and its 3 marked damage removed at the same instant, so
/// there is never a moment where it is a 1/1 with 3 damage marked — the CR 704.5g
/// lethal-damage check that follows sees a 1/1 with 0 damage, and the creature
/// **survives** (it does not die). We therefore clear both here, together, before
/// returning to the SBA loop.
///
/// Also clears any lingering deathtouch marks (CR 702.2b lasts "this turn"): the
/// state-based-actions loop normally drains them the moment they are recorded, so
/// this is a belt-and-suspenders reset at the turn boundary.
fn cleanup_turn_based_actions(state: &mut GameState) {
    for perm in &mut state.battlefield {
        perm.damage = 0;
    }
    // CR 514.2: every "until end of turn" effect ends now, simultaneously with the
    // damage wipe above. Permanent-lifetime effects (anthems) are untouched.
    state
        .static_effects
        .retain(|effect| effect.duration != Duration::UntilEndOfTurn);
    state.deathtouch_struck.clear();
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
fn apply_discard(state: &mut GameState, card: CardInstance, db: &CardDatabase) {
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
        advance_through_turn_based_steps(state, db);
        state.consecutive_passes = 0;
        state.priority = priority_after_step_change(state);
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
    state.record_event(GameEvent::Mulligan { player: seat });
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
    state.record_event(GameEvent::HandKept { player: seat });
    advance_after_keep(state);
}

/// End-of-combat turn-based action: remove every creature from combat (CR 511.3)
/// by clearing the attacking flag and blocking assignment on every permanent. The
/// per-turn declaration flags are reset when the next turn begins
/// ([`GameState::begin_next_turn`]), so a fresh combat starts clean.
fn remove_creatures_from_combat(state: &mut GameState) {
    for perm in &mut state.battlefield {
        perm.attacking = None;
        perm.blocking = None;
    }
}

/// Declare the active player's attackers (CR 508.1): mark each as attacking and
/// tap it (attacking taps, CR 508.1f) unless it has vigilance (CR 702.20b), then
/// record that the declaration is done and open the step's priority round with the
/// active player. An empty selection is a legal "no attackers" declaration
/// (CR 508.1a).
///
/// Only ever reached during the declare-attackers step for the active player (the
/// action is offered nowhere else — see [`crate::valid_actions`]) and only for a
/// selection already validated in [`action_is_legal`].
fn apply_declare_attackers(state: &mut GameState, attackers: &[Attack], db: &CardDatabase) {
    for attack in attackers {
        // CR 508.1f / CR 702.20b: whether this attacker has vigilance (printed or
        // granted at layer 6) is read through the computed characteristics, which
        // borrows `state` immutably — so it is resolved before the mutable lookup
        // below rather than while `state.battlefield` is borrowed mutably.
        let has_vigilance = state
            .battlefield
            .iter()
            .find(|p| p.id == attack.attacker)
            .is_some_and(|perm| has_keyword(state, perm, Keyword::Vigilance, db));
        if let Some(perm) = state
            .battlefield
            .iter_mut()
            .find(|p| p.id == attack.attacker)
        {
            // CR 508.1a: record whom this attacker is attacking, so blocker
            // eligibility and combat damage follow the assignment (issue #341).
            perm.attacking = Some(attack.defender);
            // CR 508.1f / CR 702.20b: attacking taps the creature, unless it has
            // vigilance, in which case it attacks without tapping.
            if !has_vigilance {
                perm.tapped = true;
            }
        }
    }
    state.attackers_declared = true;
    // Record the declaration with each attacker's card identity, so the log can name
    // it even after it has left combat or the battlefield (CR 508.1).
    let declared: Vec<LoggedPermanent> = attackers
        .iter()
        .map(|attack| logged_permanent(state, attack.attacker))
        .collect();
    state.record_event(GameEvent::AttackersDeclared {
        player: state.active_player,
        attackers: declared,
    });
    // The declaration made, the declare-attackers step proceeds to its normal
    // priority round beginning with the active player (CR 508.2).
    state.priority = state.active_player;
    state.consecutive_passes = 0;
}

/// Pair a battlefield permanent's id with its current card identity for a log
/// event, so the name is projectable later even once the permanent has left the
/// battlefield. A missing permanent falls back to a default [`CardId`] — the
/// callers pass ids validated to be on the battlefield, so this is defensive only.
fn logged_permanent(state: &GameState, id: PermanentId) -> LoggedPermanent {
    let card = state
        .battlefield
        .iter()
        .find(|p| p.id == id)
        .map_or_else(CardId::default, |p| p.card);
    LoggedPermanent {
        permanent: id,
        card,
    }
}

/// Declare one attacked player's blockers (CR 509.1): record each blocker's
/// assignment to its attacker and either hand the next attacked player their own
/// declaration (multi-defender combat, APNAP order — issue #344) or, once every
/// attacked player has declared, open the step's priority round with the active
/// player (CR 509.4). An empty selection is a legal "no blockers" declaration.
///
/// Only ever reached during the declare-blockers step for the player who currently
/// owes the declaration ([`pending_blocker_declarer`]), and only for a selection
/// already validated in [`action_is_legal`]. Combat damage is computed later, at
/// the combat-damage step, so it is computed once — after every attacked player has
/// declared.
fn apply_declare_blockers(state: &mut GameState, blocks: &[Block]) {
    // The player who owes this declaration, captured before recording changes who
    // owes the next one.
    let declarer = pending_blocker_declarer(state).unwrap_or(state.active_player);
    for block in blocks {
        if let Some(perm) = state.battlefield.iter_mut().find(|p| p.id == block.blocker) {
            perm.blocking = Some(block.attacker);
        }
    }
    // Record the assignments with both creatures' card identity for stable naming.
    let declared: Vec<(LoggedPermanent, LoggedPermanent)> = blocks
        .iter()
        .map(|block| {
            (
                logged_permanent(state, block.blocker),
                logged_permanent(state, block.attacker),
            )
        })
        .collect();
    state.record_event(GameEvent::BlockersDeclared {
        player: declarer,
        blocks: declared,
    });
    // Mark this defender done and decide whether any attacked player still owes a
    // declaration. Two-player combat has a single declarer, so the one declaration
    // completes the step; multi-defender combat tracks each declarer and finishes
    // only once none remain.
    if defending_player(state).is_some() {
        state.blockers_declared = true;
    } else {
        state.blockers_declared_by.push(declarer);
        if pending_blocker_declarer(state).is_none() {
            state.blockers_declared = true;
        }
    }
    state.priority = if state.blockers_declared {
        // Every declaration is in: the step's normal priority round opens with the
        // active player (CR 509.4).
        state.active_player
    } else {
        // The next attacked player (APNAP order) declares before priority is passed.
        pending_blocker_declarer(state).unwrap_or(state.active_player)
    };
    state.consecutive_passes = 0;
}

/// Record the attacking player's combat-damage assignment orders (CR 510.1, issue
/// #346) and open the declare-blockers priority round. Each order is stored on
/// [`GameState::damage_orders`], where [`crate::combat::combat_damage`] reads it to
/// assign lethal-before-next along the chosen sequence; an attacker without a stored
/// order keeps stable battlefield order. Only ever reached for the attacking player
/// once every owed order is supplied (validated in [`action_is_legal`]).
fn apply_order_combat_damage(state: &mut GameState, orders: &[DamageOrder]) {
    for order in orders {
        state
            .damage_orders
            .push((order.attacker, order.blockers.clone()));
    }
    // Every owed order is in; the step's normal priority round opens with the active
    // player before combat damage is dealt (CR 510.1 precedes the damage step).
    state.priority = state.active_player;
    state.consecutive_passes = 0;
}

/// Play a land from the active player's hand onto the battlefield. Not via the
/// stack (CR 116.2a); a fresh [`PermanentId`] is minted on entry while the
/// card's [`crate::CardInstanceId`] carries over unchanged.
fn apply_play_land(state: &mut GameState, card: CardInstance, db: &CardDatabase) {
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
    let entered_turn = state.turn;
    let mut permanent = Permanent {
        id: PermanentId(id),
        instance: card.id,
        card: card.card,
        controller,
        tapped: false,
        entered_turn,
        attacking: None,
        blocking: None,
        damage: 0,
        counters: Default::default(),
        // A land is played directly, never attached to anything (CR 305).
        attached_to: None,
    };
    // CR 614.1c/614.12: apply the land's own enters-the-battlefield replacements
    // (e.g. a tapland's "enters tapped") as it enters, so it is tapped the instant
    // it is on the battlefield — no untapped window to tap for mana this turn.
    apply_enters_replacements(db, &mut permanent);
    state.battlefield.push(permanent);
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

/// Cast a spell of any castable type: pay its mana cost from the caster's pool,
/// move the card from hand onto the stack, and reset the pass count (the caster
/// keeps priority). Type-agnostic — the card's types decide only how it *resolves*
/// (a permanent enters the battlefield, an instant/sorcery goes to the graveyard,
/// CR 608.3), routed in [`resolve_stack_object`]; timing legality (instant vs.
/// sorcery speed, CR 117.1a) is enforced upstream in [`crate::valid_actions`].
fn apply_cast_spell(
    state: &mut GameState,
    card: CardInstance,
    targets: &[Target],
    db: &CardDatabase,
) {
    let controller = state.priority;
    let Some(data) = db.card(card.card) else {
        return;
    };
    let base = parse_mana_cost(&data.mana_cost);
    // A commander may be cast from the command zone (CR 903.8); anything else is
    // cast from hand. Detect which zone this instance is in so the cost carries the
    // commander tax and the card is removed from the right pile.
    let from_command = state
        .players
        .get(controller.0)
        .is_some_and(|p| p.command.iter().any(|c| c.id == card.id));
    let cost = if from_command {
        let casts = state
            .players
            .get(controller.0)
            .and_then(|p| p.commander.as_ref())
            .map_or(0, |c| c.casts);
        commander_tax_cost(&base, casts)
    } else {
        base
    };
    {
        let Some(player) = state.players.get_mut(controller.0) else {
            return;
        };
        let Some(new_pool) = player.mana_pool.pay(&cost) else {
            return;
        };
        if from_command {
            let Some(pos) = player.command.iter().position(|&c| c.id == card.id) else {
                return;
            };
            player.command.remove(pos);
            // CR 903.8: each cast from the command zone raises the tax for the next.
            if let Some(commander) = player.commander.as_mut() {
                commander.casts += 1;
            }
        } else {
            let Some(pos) = player.hand.iter().position(|&c| c.id == card.id) else {
                return;
            };
            player.hand.remove(pos);
        }
        player.mana_pool = new_pool;
    }
    let id = state.mint_id();
    state.stack.push(StackObject {
        id: StackId(id),
        controller,
        kind: StackObjectKind::Spell { card },
        // The targets chosen as part of casting this spell (CR 601.2c), already
        // validated against freshly computed legal sets in `action_is_legal` and
        // re-checked once more on resolution (CR 608.2b). Empty for a spell that
        // targets nothing.
        targets: targets.to_vec(),
    });
    state.record_event(GameEvent::SpellCast {
        player: controller,
        card,
    });
    state.consecutive_passes = 0;
}

/// Apply a single [`Effect`] to `state` on behalf of `controller`.
pub(crate) fn apply_effect(state: &mut GameState, effect: &Effect, controller: PlayerId) {
    if state.players.get(controller.0).is_none() {
        return;
    }
    match effect {
        Effect::AddMana { color, amount } => {
            if let Some(player) = state.players.get_mut(controller.0) {
                player.mana_pool.add(*color, *amount);
            }
        }
        Effect::AddColorlessMana { amount } => {
            if let Some(player) = state.players.get_mut(controller.0) {
                player.mana_pool.add_colorless(*amount);
            }
        }
        Effect::DrawCard { count } => {
            // Routes each draw through `Player::draw`, so a card-draw effect that
            // empties the library also flags the decking loss (CR 704.5c). Only the
            // cards that actually moved are logged (an empty-library draw adds none).
            let mut drawn = 0u32;
            for _ in 0..*count {
                let moved = state
                    .players
                    .get_mut(controller.0)
                    .is_some_and(|player| player.draw());
                if moved {
                    drawn += 1;
                }
            }
            if drawn > 0 {
                state.record_event(GameEvent::CardsDrawn {
                    player: controller,
                    count: drawn,
                });
            }
        }
        // CR 119.3: the referenced player gains life. `Controller` is "you", the
        // one player fetched above; other refs are added as effects need them.
        Effect::GainLife {
            player_ref: PlayerRef::Controller,
            amount,
        } => {
            state.change_life(controller, i32::try_from(*amount).unwrap_or(i32::MAX));
        }
        // CR 119.3: the referenced player loses life; a drop to 0 or less feeds
        // the zero-life state-based action (CR 704.5a) in the SBA loop.
        Effect::LoseLife {
            player_ref: PlayerRef::Controller,
            amount,
        } => {
            state.change_life(controller, -i32::try_from(*amount).unwrap_or(i32::MAX));
        }
        // A targeting effect: its subject is a chosen target, not the controller,
        // so it is applied via [`apply_targeted_effect`] and is a no-op here.
        Effect::Tap { .. }
        | Effect::CounterSpell { .. }
        | Effect::DealDamage { .. }
        | Effect::Destroy { .. }
        | Effect::PutCounters { .. }
        | Effect::Pump { .. }
        | Effect::GrantKeyword { .. } => {}
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
    db: &CardDatabase,
) {
    match effect {
        Effect::Tap { .. } => {
            if let Target::Permanent(id) = target {
                if let Some(perm) = state.battlefield.iter_mut().find(|p| p.id == id) {
                    perm.tapped = true;
                }
            }
        }
        // Counter the targeted spell (CR 701.5a): remove it from the stack without
        // resolving and put its card into its owner's graveyard. The caller has
        // already re-checked that the target is still a spell on the stack (CR
        // 608.2b); ownership apart from control is not tracked yet, so the countered
        // spell's controller stands in as its owner.
        Effect::CounterSpell { .. } => {
            if let Target::Spell(id) = target {
                if let Some(pos) = state.stack.iter().position(|o| o.id == id) {
                    let countered = state.stack.remove(pos);
                    if let StackObjectKind::Spell { card } = countered.kind {
                        let owner = countered.controller;
                        if let Some(player) = state.players.get_mut(owner.0) {
                            player.graveyard.push(card);
                        }
                        state.record_event(GameEvent::SpellCountered {
                            player: owner,
                            card,
                        });
                    }
                }
            }
        }
        // Deal damage to the chosen target (CR 120.3): to a creature it is marked
        // (CR 120.3d) for the lethal-damage SBA (CR 704.5g); to a player it is
        // life loss (CR 120.3a) feeding the zero-life SBA (CR 704.5a). Both seams
        // record the damage (including nonlethal) as a `DamageDealt` event.
        Effect::DealDamage { amount, .. } => match target {
            Target::Permanent(id) => {
                state.mark_damage_on_permanent(id, *amount);
            }
            Target::Player(seat) => {
                state.deal_damage_to_player(seat, *amount);
            }
            Target::Card(_) | Target::Spell(_) => {}
        },
        // Destroy the targeted permanent (CR 701.7): move it to its owner's
        // graveyard through the one creature-death seam
        // ([`GameState::destroy_permanent`], CR 700.4) — the same path lethal damage
        // uses in the SBA loop, so this death fires the dies trigger (CR 603.6c) and
        // logs a `permanent_died` identically. Regeneration is out of scope.
        Effect::Destroy { .. } => {
            if let Target::Permanent(id) = target {
                state.destroy_permanent(id, db);
            }
        }
        // Put counters on the targeted permanent (CR 122). Current power/toughness
        // folds `+1/+1` / `-1/-1` counters in on demand (CR 613.7c), so a `-1/-1`
        // counter can turn lethal by lowering toughness to at or below marked
        // damage; the SBA loop then destroys it (CR 704.5g).
        Effect::PutCounters { counter, count, .. } => {
            if let Target::Permanent(id) = target {
                if let Some(perm) = state.battlefield.iter_mut().find(|p| p.id == id) {
                    *perm.counters.entry(*counter).or_insert(0) += *count;
                }
            }
        }
        // Pump the targeted creature until end of turn (CR 514.2): add a
        // timestamped CR 613 layer-7c power/toughness modifier keyed to that one
        // permanent, with an `UntilEndOfTurn` duration the cleanup step removes.
        // The timestamp is a freshly minted, strictly increasing object id
        // (CR 613.7), so a second pump this turn stacks after the first. The
        // modifier folds into computed P/T on demand — nothing is written onto the
        // permanent — so removing it at cleanup reverts the value with nothing to
        // invalidate (ADR 0010). The caller has re-checked the target is still a
        // creature (CR 608.2b); a permanent that has since left is skipped.
        Effect::Pump {
            power, toughness, ..
        } => {
            if let Target::Permanent(id) = target {
                if state.battlefield.iter().any(|p| p.id == id) {
                    let source = state.mint_id();
                    state.static_effects.push(StaticEffect {
                        source,
                        affects: EffectAffects::SpecificPermanent(id),
                        modification: Modification::PowerToughness {
                            power: *power,
                            toughness: *toughness,
                        },
                        duration: Duration::UntilEndOfTurn,
                    });
                }
            }
        }
        // Grant the targeted creature a keyword until end of turn (CR 514.2): add a
        // CR 613 layer-6 keyword grant keyed to that one permanent, with an
        // `UntilEndOfTurn` duration the cleanup step removes (CR 613.1f). The grant
        // folds into the target's computed keyword set on demand — nothing is written
        // onto the permanent — so removing it at cleanup reverts the value with
        // nothing to invalidate (ADR 0010). A duplicate grant is redundant, not
        // additive. The caller has re-checked the target is still a creature
        // (CR 608.2b); a permanent that has since left is skipped.
        Effect::GrantKeyword { keyword, .. } => {
            if let Target::Permanent(id) = target {
                if state.battlefield.iter().any(|p| p.id == id) {
                    let source = state.mint_id();
                    state.static_effects.push(StaticEffect {
                        source,
                        affects: EffectAffects::SpecificPermanent(id),
                        modification: Modification::GrantKeyword(*keyword),
                        duration: Duration::UntilEndOfTurn,
                    });
                }
            }
        }
        // Implicit-subject effects do not target; they never reach here.
        Effect::AddMana { .. }
        | Effect::AddColorlessMana { .. }
        | Effect::DrawCard { .. }
        | Effect::GainLife { .. }
        | Effect::LoseLife { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::actions::valid_actions;
    use crate::fixtures::{fixture, id_in};
    use crate::id::CardId;
    use crate::mana::Color;
    use crate::phase::Step;

    /// The bundled card database, for tests that need oracle data.
    fn db() -> CardDatabase {
        CardDatabase::bundled().unwrap()
    }

    /// An inline combat catalog whose bodies carry the exact P/T and keywords the
    /// old invented combat fixtures had, so the damage/life arithmetic the combat
    /// tests assert stays unchanged. First strike, deathtouch, trample, lifelink and
    /// a bare "when this dies, draw" have no clean M19 representative, so the combat
    /// and dies-trigger tests build their own definitions (ADR 0025).
    fn combat_db() -> CardDatabase {
        let json = r#"[
            {"schema_version":1,"functional_id":"test_boar","name":"Test Boar",
             "types":["creature"],"subtypes":["Boar"],"mana_cost":"{2}{G}","colors":["green"],
             "power":3,"toughness":2},
            {"schema_version":1,"functional_id":"test_basilisk","name":"Test Basilisk",
             "types":["creature"],"subtypes":["Basilisk"],"mana_cost":"{4}{G}","colors":["green"],
             "power":4,"toughness":5},
            {"schema_version":1,"functional_id":"test_otter","name":"Test Otter",
             "types":["creature"],"subtypes":["Otter"],"mana_cost":"{1}{U}","colors":["blue"],
             "power":1,"toughness":3},
            {"schema_version":1,"functional_id":"test_duelist","name":"Test Duelist",
             "types":["creature"],"subtypes":["Human","Knight"],"mana_cost":"{1}{W}","colors":["white"],
             "power":2,"toughness":2,"keywords":["first_strike"]},
            {"schema_version":1,"functional_id":"test_adder","name":"Test Adder",
             "types":["creature"],"subtypes":["Snake"],"mana_cost":"{G}","colors":["green"],
             "power":1,"toughness":1,"keywords":["deathtouch"]},
            {"schema_version":1,"functional_id":"test_trampler","name":"Test Trampler",
             "types":["creature"],"subtypes":["Beast"],"mana_cost":"{4}{G}","colors":["green"],
             "power":5,"toughness":4,"keywords":["trample"]},
            {"schema_version":1,"functional_id":"test_baneclaw","name":"Test Baneclaw",
             "types":["creature"],"subtypes":["Beast"],"mana_cost":"{2}{G}{G}","colors":["green"],
             "power":4,"toughness":4,"keywords":["trample","deathtouch"]},
            {"schema_version":1,"functional_id":"test_lifelinker","name":"Test Lifelinker",
             "types":["creature"],"subtypes":["Cleric"],"mana_cost":"{2}{W}","colors":["white"],
             "power":2,"toughness":3,"keywords":["lifelink"]},
            {"schema_version":1,"functional_id":"test_lurker","name":"Test Lurker",
             "types":["creature"],"subtypes":["Horror"],"mana_cost":"{1}{B}","colors":["black"],
             "power":2,"toughness":2,
             "abilities":[{"type":"triggered","event":"self_dies","effects":[{"kind":"draw_card","count":1}]}]},
            {"schema_version":1,"functional_id":"test_twinstrike","name":"Test Twinstrike",
             "types":["creature"],"subtypes":["Cat"],"mana_cost":"{2}{W}","colors":["white"],
             "power":2,"toughness":2,"keywords":["double_strike"]},
            {"schema_version":1,"functional_id":"test_twintrample","name":"Test Twintrample",
             "types":["creature"],"subtypes":["Beast"],"mana_cost":"{3}{G}","colors":["green"],
             "power":3,"toughness":3,"keywords":["double_strike","trample"]},
            {"schema_version":1,"functional_id":"test_ward","name":"Test Ward",
             "types":["creature"],"subtypes":["Soldier"],"mana_cost":"{2}{W}","colors":["white"],
             "power":3,"toughness":3,"keywords":["first_strike"]}
        ]"#;
        CardDatabase::from_json(json).unwrap()
    }

    /// A two-player game in the precombat main phase with player 0 holding a
    /// Forest and Llanowar Elves, and one card to draw in the library. Each card
    /// is a freshly minted [`CardInstance`] so copies stay distinguishable.
    fn slice_state() -> GameState {
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let forest = state.new_instance(fixture("forest"));
        let scout = state.new_instance(fixture("llanowar_elves"));
        let draw = state.new_instance(fixture("onakke_ogre"));
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
        let forest = hand_instance(&before, 0, fixture("forest"));
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
        let inst = state.new_instance(fixture("forest"));
        let id = state.mint_id();
        state.battlefield.push(Permanent {
            id: PermanentId(id),
            instance: inst.id,
            card: fixture("forest"),
            controller: PlayerId(0),
            tapped: false,
            entered_turn: 0,
            attacking: None,
            blocking: None,
            damage: 0,
            counters: Default::default(),
            attached_to: None,
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
        let inst = state.new_instance(fixture("forest"));
        let id = state.mint_id();
        state.battlefield.push(Permanent {
            id: PermanentId(id),
            instance: inst.id,
            card: fixture("forest"),
            controller: PlayerId(0),
            tapped: false,
            entered_turn: 0,
            attacking: None,
            blocking: None,
            damage: 0,
            counters: Default::default(),
            attached_to: None,
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
        let scout = hand_instance(&state, 0, fixture("llanowar_elves"));
        let after = apply_action(
            &state,
            &Action::CastSpell {
                card: scout,
                targets: Vec::new(),
            },
            &db,
        );
        assert_eq!(after.stack.len(), 1);
        assert_eq!(after.players[0].mana_pool.green, 0);
        assert!(!after.players[0].hand.iter().any(|c| c.id == scout.id));
    }

    #[test]
    fn issue_card_effects_etb_draw_end_to_end() {
        // Full vertical slice: three Forests already in play tap for {G}{G}{G}, cast
        // Skyscanner ({3}, an ETB "draw a card"), resolve it (ETB triggers), then
        // resolve the trigger (controller draws).
        let db = db();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let forests: Vec<PermanentId> = (0..3)
            .map(|_| place_permanent(&mut state, fixture("forest"), PlayerId(0), false, 0))
            .collect();
        let scanner = state.new_instance(fixture("skyscanner"));
        let draw_card = state.new_instance(fixture("onakke_ogre"));
        state.players[0].hand = vec![scanner];
        state.players[0].library = vec![draw_card];

        // Tap the three Forests for {G} each (mana abilities resolve immediately).
        for forest in forests {
            state = apply_action(
                &state,
                &Action::ActivateAbility {
                    permanent: forest,
                    index: 0,
                    targets: Vec::new(),
                },
                &db,
            );
        }
        assert_eq!(state.players[0].mana_pool.green, 3);
        assert!(state.stack.is_empty());

        // Cast Skyscanner ({3} paid from the three green).
        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: scanner,
                targets: Vec::new(),
            },
            &db,
        );
        assert_eq!(state.stack.len(), 1);
        assert_eq!(state.players[0].mana_pool.green, 0);

        // Pass twice: the creature resolves and its ETB trigger goes on the stack.
        let state = apply_action(&state, &Action::PassPriority, &db);
        let state = apply_action(&state, &Action::PassPriority, &db);
        assert!(state
            .battlefield
            .iter()
            .any(|p| p.card == fixture("skyscanner")));
        assert_eq!(state.stack.len(), 1);

        // Pass twice more: the ETB ability resolves and player 0 draws.
        let state = apply_action(&state, &Action::PassPriority, &db);
        let state = apply_action(&state, &Action::PassPriority, &db);
        assert!(state.stack.is_empty());
        assert!(state.players[0].hand.iter().any(|c| c.id == draw_card.id));
        assert!(state.players[0].library.is_empty());
    }

    #[test]
    fn issue_155_tapland_enters_tapped_with_no_untapped_window_cr_614_1c() {
        // CR 614.1c/614.12: a land with an "enters tapped" self-replacement is tapped
        // the instant it is on the battlefield. Tranquil Expanse is played as a land
        // (CR 116.2a): the resulting permanent is already tapped, and because a {T}
        // mana ability is unpayable while tapped, no action to tap it for mana is
        // offered this same priority window — there is no observable untapped state.
        let db = db();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let land = state.new_instance(fixture("tranquil_expanse"));
        state.players[0].hand = vec![land];

        let after = apply_action(&state, &Action::PlayLand { card: land }, &db);

        assert_eq!(after.battlefield.len(), 1);
        let perm = &after.battlefield[0];
        assert!(
            perm.tapped,
            "the tapland is tapped the moment it enters (CR 614.1c/614.12)"
        );
        // No ActivateAbility for the tapland is on offer: its {T} abilities can't be
        // paid while it is tapped, so it cannot be tapped for mana this turn.
        assert!(
            !valid_actions(&after, &db).iter().any(
                |a| matches!(a, Action::ActivateAbility { permanent, .. } if *permanent == perm.id)
            ),
            "a tapland offers no mana ability the turn it enters — no untapped window"
        );
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
        let forest_a = state.new_instance(fixture("forest"));
        let forest_b = state.new_instance(fixture("forest"));
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
            entered_turn: 0,
            attacking: None,
            blocking: None,
            damage,
            counters: Default::default(),
            attached_to: None,
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
        let p0_perm = place_permanent(&mut state, fixture("forest"), PlayerId(0), true, 0);
        let p1_perm = place_permanent(&mut state, fixture("forest"), PlayerId(1), true, 0);

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
        let card = state.new_instance(fixture("onakke_ogre"));
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
        let card = state.new_instance(fixture("onakke_ogre"));
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
        let hand: Vec<CardInstance> = (0..9)
            .map(|_| state.new_instance(fixture("onakke_ogre")))
            .collect();
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
        let discards = choices
            .iter()
            .filter(|a| matches!(a, Action::Discard { .. }))
            .count();
        assert_eq!(discards, 9, "one discard choice per card in hand");
        // Concede is still offered during cleanup (CR 104.3a); nothing else is.
        assert!(choices.contains(&Action::Concede));
        assert_eq!(choices.len(), 10, "the nine discards plus concede");

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
            .map(|_| state.new_instance(fixture("onakke_ogre")))
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
        let perm = place_permanent(&mut state, fixture("forest"), PlayerId(0), false, 3);

        let after = pass_full_round(&state, &db);

        assert_eq!(
            find_perm(&after, perm).damage,
            0,
            "marked damage is wiped at cleanup (CR 514.2)"
        );
    }

    // ----- Until-end-of-turn pump: cleanup expiry (issue #150) -----

    /// Push an "until end of turn" pump of +`power`/+`toughness` onto `target`,
    /// timestamped by a freshly minted object id, and return that id.
    fn pump(state: &mut GameState, target: PermanentId, power: i32, toughness: i32) -> u64 {
        let source = state.mint_id();
        state.static_effects.push(StaticEffect {
            source,
            affects: EffectAffects::SpecificPermanent(target),
            modification: Modification::PowerToughness { power, toughness },
            duration: Duration::UntilEndOfTurn,
        });
        source
    }

    #[test]
    fn issue_150_pump_spell_boosts_its_target_until_end_of_turn_end_to_end() {
        // Cast Titanic Growth (+4/+4 until end of turn) on a 1/1 Llanowar Elves: on
        // resolution the creature computes as a 5/5 and one until-end-of-turn layer-7c
        // modifier is in force.
        use crate::characteristics::characteristics;
        let db = db();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let creature =
            place_permanent(&mut state, fixture("llanowar_elves"), PlayerId(0), false, 0);
        let surge = state.new_instance(fixture("titanic_growth"));
        state.players[0].hand = vec![surge];
        state.players[0].mana_pool.add(Color::Green, 1);
        state.players[0].mana_pool.colorless = 2;

        // The Elves is a printed 1/1 before the pump.
        assert_eq!(characteristics(&state, creature, &db).power, Some(1));

        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: surge,
                targets: vec![Target::Permanent(creature)],
            },
            &db,
        );
        // Pass twice: the spell resolves and applies its pump.
        let state = pass_full_round(&state, &db);

        assert!(state.stack.is_empty());
        let ch = characteristics(&state, creature, &db);
        assert_eq!(ch.power, Some(5), "printed 1 + 4 until end of turn");
        assert_eq!(ch.toughness, Some(5));
        assert_eq!(state.static_effects.len(), 1);
        assert_eq!(
            state.static_effects[0].duration,
            Duration::UntilEndOfTurn,
            "the pump is an until-end-of-turn effect"
        );
        // The instant itself went to the graveyard (CR 608.2m).
        assert!(state.players[0].graveyard.iter().any(|c| c.id == surge.id));
    }

    #[test]
    fn issue_150_pumped_creature_survives_lethal_to_base_damage_then_expires_at_cleanup_cr_514_2() {
        // CR 514.2: a 1/1 pumped to 4/4 that has taken 3 marked damage (lethal to
        // its *base* toughness of 1, but not to 4) survives the turn, and at
        // cleanup its pump wears off and its damage is removed **simultaneously** —
        // so the CR 704.5g check that follows never sees a 1/1 with 3 damage and
        // the creature survives cleanup as a printed 1/1.
        let db = db();
        let mut state = GameState::new_two_player();
        state.step = Step::End; // player 0, turn 1; empty hand so no discard.
        let creature =
            place_permanent(&mut state, fixture("llanowar_elves"), PlayerId(0), false, 3);
        pump(&mut state, creature, 3, 3);

        // Mid-turn: 4/4 with 3 marked damage is not lethal, so state-based actions
        // leave it on the battlefield.
        let mut mid = state.clone();
        run_state_based_actions(&mut mid, &db);
        assert!(
            mid.battlefield.iter().any(|p| p.id == creature),
            "3 damage is not lethal to a pumped 4/4"
        );

        // Walk through the cleanup step into the next turn.
        let after = pass_full_round(&state, &db);
        assert!(
            after.battlefield.iter().any(|p| p.id == creature),
            "the creature survives cleanup: damage and pump end simultaneously (CR 514.2)"
        );
        assert!(
            after.static_effects.is_empty(),
            "the until-end-of-turn pump wore off at cleanup"
        );
        assert_eq!(
            find_perm(&after, creature).damage,
            0,
            "marked damage was wiped at cleanup"
        );
    }

    // ----- Until-end-of-turn keyword grant: resolution and cleanup expiry (issue #374) -----

    /// Push an "until end of turn" grant of `keyword` onto `target`, timestamped by
    /// a freshly minted object id, and return that id.
    fn grant_keyword(state: &mut GameState, target: PermanentId, keyword: Keyword) -> u64 {
        let source = state.mint_id();
        state.static_effects.push(StaticEffect {
            source,
            affects: EffectAffects::SpecificPermanent(target),
            modification: Modification::GrantKeyword(keyword),
            duration: Duration::UntilEndOfTurn,
        });
        source
    }

    #[test]
    fn issue_374_grant_keyword_spell_grants_the_keyword_until_end_of_turn_end_to_end() {
        // Cast Jump (target creature gains flying until end of turn) on a ground
        // Llanowar Elves: on resolution the creature computes with flying and one
        // until-end-of-turn layer-6 grant is in force.
        use crate::characteristics::characteristics;
        let db = db();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let creature =
            place_permanent(&mut state, fixture("llanowar_elves"), PlayerId(0), false, 0);
        let jump = state.new_instance(fixture("jump"));
        state.players[0].hand = vec![jump];
        state.players[0].mana_pool.add(Color::Blue, 1);

        // The Elves has no flying before the spell.
        assert!(!characteristics(&state, creature, &db)
            .keywords
            .contains(&Keyword::Flying));

        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: jump,
                targets: vec![Target::Permanent(creature)],
            },
            &db,
        );
        // Pass twice: the spell resolves and applies its grant.
        let state = pass_full_round(&state, &db);

        assert!(state.stack.is_empty());
        assert!(
            characteristics(&state, creature, &db)
                .keywords
                .contains(&Keyword::Flying),
            "the resolved spell granted flying (CR 613.1f)"
        );
        assert_eq!(state.static_effects.len(), 1);
        assert_eq!(
            state.static_effects[0].duration,
            Duration::UntilEndOfTurn,
            "the grant is an until-end-of-turn effect"
        );
        assert!(state.players[0].graveyard.iter().any(|c| c.id == jump.id));
    }

    #[test]
    fn issue_374_until_end_of_turn_grant_expires_at_cleanup_cr_514_2() {
        // CR 514.2: an until-end-of-turn keyword grant ends in the cleanup step. The
        // grant is present the turn it is made and gone once the turn passes — verified
        // across the turn boundary.
        use crate::characteristics::characteristics;
        let db = db();
        let mut state = GameState::new_two_player();
        state.step = Step::End; // player 0, turn 1; empty hand so no discard.
        let creature =
            place_permanent(&mut state, fixture("walking_corpse"), PlayerId(0), false, 0);
        grant_keyword(&mut state, creature, Keyword::Flying);

        // Before cleanup the creature has the granted keyword.
        assert!(
            characteristics(&state, creature, &db)
                .keywords
                .contains(&Keyword::Flying),
            "the grant is in force during the turn it was made"
        );

        // Walk through the cleanup step into the next turn.
        let after = pass_full_round(&state, &db);
        assert!(
            after.static_effects.is_empty(),
            "the until-end-of-turn grant wore off at cleanup (CR 514.2)"
        );
        assert!(
            !characteristics(&after, creature, &db)
                .keywords
                .contains(&Keyword::Flying),
            "the granted keyword is gone across the turn boundary"
        );
    }

    #[test]
    fn issue_150_two_pumps_in_one_turn_stack_and_both_expire_at_cleanup() {
        // CR 613.7 / 514.2: two pumps on one creature this turn both apply (they
        // stack in timestamp order) and both wear off at cleanup.
        use crate::characteristics::characteristics;
        let db = db();
        let mut state = GameState::new_two_player();
        state.step = Step::End; // player 0, turn 1; empty hand.
        let creature =
            place_permanent(&mut state, fixture("llanowar_elves"), PlayerId(0), false, 0);
        let first = pump(&mut state, creature, 2, 2);
        let second = pump(&mut state, creature, 1, 1);
        assert!(second > first, "the later pump has the later timestamp");

        // Printed 1/1 + (+2/+2) + (+1/+1) = 4/4 while both are in force.
        let ch = characteristics(&state, creature, &db);
        assert_eq!(ch.power, Some(4));
        assert_eq!(ch.toughness, Some(4));

        let after = pass_full_round(&state, &db);
        assert!(
            after.static_effects.is_empty(),
            "both until-end-of-turn pumps expired at cleanup (CR 514.2)"
        );
        let reverted = characteristics(&after, creature, &db);
        assert_eq!(reverted.power, Some(1), "back to the printed 1/1");
        assert_eq!(reverted.toughness, Some(1));
    }

    #[test]
    fn issue_150_pump_never_outlives_its_permanent() {
        // A pumped creature that dies mid-turn (here to lethal-to-its-4/4 damage)
        // leaves no dangling modifier: the state-based-actions loop destroys it and
        // prunes its now-orphaned pump in the same pass.
        let db = db();
        let mut state = GameState::new_two_player();
        let creature =
            place_permanent(&mut state, fixture("llanowar_elves"), PlayerId(0), false, 5);
        pump(&mut state, creature, 3, 3); // 1/1 -> 4/4, but 5 damage is lethal

        run_state_based_actions(&mut state, &db);

        assert!(
            !state.battlefield.iter().any(|p| p.id == creature),
            "5 damage is lethal to the pumped 4/4 (CR 704.5g)"
        );
        assert!(
            state.static_effects.is_empty(),
            "the pump was pruned when its permanent left — no dangling modifier"
        );
    }

    #[test]
    fn issue_150_while_on_battlefield_effect_is_not_ended_by_cleanup() {
        // CR 514.2 ends only "until end of turn" effects; a permanent-lifetime
        // anthem (WhileOnBattlefield) is untouched by the cleanup step.
        let db = db();
        let mut state = GameState::new_two_player();
        state.step = Step::End; // player 0, turn 1; empty hand.
        let _creature =
            place_permanent(&mut state, fixture("walking_corpse"), PlayerId(0), false, 0);
        let source = state.mint_id();
        state.static_effects.push(StaticEffect {
            source,
            affects: EffectAffects::CreaturesControlledBy(PlayerId(0)),
            modification: Modification::PowerToughness {
                power: 1,
                toughness: 1,
            },
            duration: Duration::WhileOnBattlefield,
        });

        let after = pass_full_round(&state, &db);
        assert_eq!(
            after.static_effects.len(),
            1,
            "a while-on-battlefield anthem persists through cleanup (CR 514.2)"
        );
    }

    // ----- Auras end to end (issue #152) -----

    #[test]
    fn issue_152_minus_x_aura_cast_kills_its_host_and_follows_it_cr_704_5f() {
        // Full slice through the real cast path: cast a -2/-2 Aura on a 3/2 host. On
        // resolution the Aura enters attached, its -2/-2 drops the host's current
        // toughness to 0, and the pipeline's state-based-actions loop puts the host
        // into the graveyard (CR 704.5f) and its now-orphaned Aura with it (CR
        // 704.5m) — both gone in the same fixed point, the modifier vanishing with the
        // Aura. P/T Auras have no clean M19 card, so this is inline (ADR 0025).
        use crate::ability::Target;
        use crate::characteristics::characteristics;
        let json = r#"[
            {"schema_version":1,"functional_id":"test_curse","name":"Test Curse",
             "types":["enchantment"],"subtypes":["Aura"],"mana_cost":"{B}","colors":["black"],
             "aura":{"enchant":"any_creature","power":-2,"toughness":-2}},
            {"schema_version":1,"functional_id":"test_boar","name":"Test Boar",
             "types":["creature"],"subtypes":["Boar"],"mana_cost":"{2}{G}","colors":["green"],
             "power":3,"toughness":2}
        ]"#;
        let db = CardDatabase::from_json(json).unwrap();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let host = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(0), false, 0); // 3/2
        let curse = state.new_instance(id_in(&db, "test_curse")); // -2/-2 Aura, {B}
        state.players[0].hand = vec![curse];
        state.players[0].mana_pool.add(Color::Black, 1);

        // The host is a healthy 3/2 before the Aura is cast.
        assert_eq!(characteristics(&state, host, &db).toughness, Some(2));

        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: curse,
                targets: vec![Target::Permanent(host)],
            },
            &db,
        );
        // Both players pass: the Aura resolves, attaches, and the SBA loop settles.
        let state = pass_full_round(&state, &db);

        assert!(
            !state.battlefield.iter().any(|p| p.id == host),
            "the host at 0 toughness is put into the graveyard (CR 704.5f)"
        );
        assert!(
            !state
                .battlefield
                .iter()
                .any(|p| p.card == id_in(&db, "test_curse")),
            "the Aura follows its dead host to the graveyard (CR 704.5m)"
        );
        assert!(
            state.static_effects.is_empty(),
            "the Aura's derived modifier leaves no dangling static effect"
        );
        // The Boar and the Curse are both in the graveyard.
        assert_eq!(state.players[0].graveyard.len(), 2);
    }

    // ----- Combat I: declare attackers and blockers (issue #117) -----

    use crate::actions::{valid_actions as valid, Attack, Block, DamageOrder};
    use crate::combat::{attacker_candidates, blocker_candidates, pending_blocker_declarer};

    /// A two-player game paused at the declare-attackers step, turn 2 so that
    /// permanents which entered on turn 0/1 are free of summoning sickness. Player
    /// 0 is the active/attacking player, player 1 the defender.
    fn at_declare_attackers() -> GameState {
        let mut state = GameState::new_two_player();
        state.turn = 2;
        state.step = Step::DeclareAttackers;
        state.active_player = PlayerId(0);
        state.priority = PlayerId(0);
        state
    }

    /// Mark the permanent `id` as having entered on turn `turn` (its summoning-
    /// sickness clock).
    fn set_entered_turn(state: &mut GameState, id: PermanentId, turn: u32) {
        if let Some(perm) = state.battlefield.iter_mut().find(|p| p.id == id) {
            perm.entered_turn = turn;
        }
    }

    #[test]
    fn issue_117_declare_attackers_taps_and_marks_attackers_cr_508_1() {
        // CR 508.1a: the active player declares as attackers untapped creatures
        // they have controlled since the turn began. CR 508.1f: attacking taps them
        // (no vigilance modeled yet).
        let db = db();
        let mut state = at_declare_attackers();
        let attacker =
            place_permanent(&mut state, fixture("walking_corpse"), PlayerId(0), false, 0);

        // Before declaring, the only action offered to the active player is the
        // declaration itself — no pass, no other action (a turn-based choice).
        let offered = valid(&state, &db);
        // The declaration plus the always-available concede (CR 104.3a).
        assert_eq!(offered.len(), 2);
        assert!(matches!(offered[0], Action::DeclareAttackers { .. }));
        assert!(offered.contains(&Action::Concede));
        assert_eq!(attacker_candidates(&state, &db), vec![attacker]);

        let after = apply_action(
            &state,
            &Action::DeclareAttackers {
                attackers: atk1(&[attacker]),
            },
            &db,
        );

        let perm = find_perm(&after, attacker);
        assert!(
            perm.attacking.is_some(),
            "declared creature is attacking (CR 508.1a)"
        );
        assert!(perm.tapped, "attacking taps the creature (CR 508.1f)");
        assert!(after.attackers_declared);
        // The declaration made, the step opens its priority round with the active
        // player, who may now pass.
        assert_eq!(after.priority, PlayerId(0));
        assert!(valid(&after, &db).contains(&Action::PassPriority));
    }

    #[test]
    fn issue_117_empty_attacker_declaration_is_legal_cr_508_1a() {
        // CR 508.1a: declaring no attackers is a legal declaration; it advances the
        // step past its turn-based action without tapping anything.
        let db = db();
        let mut state = at_declare_attackers();
        let creature =
            place_permanent(&mut state, fixture("walking_corpse"), PlayerId(0), false, 0);

        let after = apply_action(
            &state,
            &Action::DeclareAttackers {
                attackers: Vec::new(),
            },
            &db,
        );

        assert!(after.attackers_declared);
        assert!(find_perm(&after, creature).attacking.is_none());
        assert!(!find_perm(&after, creature).tapped);
        assert!(valid(&after, &db).contains(&Action::PassPriority));
    }

    #[test]
    fn issue_117_summoning_sick_creature_cannot_attack_cr_302_6() {
        // CR 302.6: a creature that has not been controlled continuously since the
        // turn began can't attack. One that entered this very turn is not a
        // candidate, and naming it is an illegal declaration (a no-op).
        let db = db();
        let mut state = at_declare_attackers();
        let sick = place_permanent(&mut state, fixture("walking_corpse"), PlayerId(0), false, 0);
        let this_turn = state.turn;
        set_entered_turn(&mut state, sick, this_turn);

        assert!(attacker_candidates(&state, &db).is_empty());
        let after = apply_action(
            &state,
            &Action::DeclareAttackers {
                attackers: atk1(&[sick]),
            },
            &db,
        );
        assert_eq!(after, state, "declaring a sick attacker is a no-op");
    }

    #[test]
    fn issue_117_tapped_creature_cannot_attack_cr_508_1a() {
        // CR 508.1a: only untapped creatures can be declared as attackers.
        let db = db();
        let mut state = at_declare_attackers();
        let tapped = place_permanent(&mut state, fixture("walking_corpse"), PlayerId(0), true, 0);

        assert!(attacker_candidates(&state, &db).is_empty());
        let after = apply_action(
            &state,
            &Action::DeclareAttackers {
                attackers: atk1(&[tapped]),
            },
            &db,
        );
        assert_eq!(after, state, "declaring a tapped attacker is a no-op");
    }

    #[test]
    fn issue_117_defender_declares_blockers_multiple_per_attacker_cr_509_1a() {
        // CR 509.1a: the defending player assigns each blocker to one attacking
        // creature; several blockers may be assigned to the same attacker.
        let db = db();
        let mut state = at_declare_attackers();
        let attacker =
            place_permanent(&mut state, fixture("walking_corpse"), PlayerId(0), false, 0);
        let blocker_a =
            place_permanent(&mut state, fixture("walking_corpse"), PlayerId(1), false, 0);
        let blocker_b =
            place_permanent(&mut state, fixture("walking_corpse"), PlayerId(1), false, 0);

        // Declare the attacker, then pass to the declare-blockers step.
        let state = apply_action(
            &state,
            &Action::DeclareAttackers {
                attackers: atk1(&[attacker]),
            },
            &db,
        );
        let state = pass_full_round(&state, &db);
        assert_eq!(state.step, Step::DeclareBlockers);
        // The defender (player 1) is the one who must declare, and is offered the
        // declaration; both eligible blockers are candidates.
        assert_eq!(state.priority, PlayerId(1));
        let offered = valid(&state, &db);
        // The declaration plus the always-available concede (CR 104.3a).
        assert_eq!(offered.len(), 2);
        assert!(matches!(offered[0], Action::DeclareBlockers { .. }));
        assert!(offered.contains(&Action::Concede));
        let candidates = blocker_candidates(&state, &db);
        assert!(candidates.contains(&blocker_a) && candidates.contains(&blocker_b));

        // Assign both blockers to the single attacker.
        let after = apply_action(
            &state,
            &Action::DeclareBlockers {
                blocks: vec![
                    Block {
                        blocker: blocker_a,
                        attacker,
                    },
                    Block {
                        blocker: blocker_b,
                        attacker,
                    },
                ],
            },
            &db,
        );
        assert_eq!(find_perm(&after, blocker_a).blocking, Some(attacker));
        assert_eq!(find_perm(&after, blocker_b).blocking, Some(attacker));
        assert!(after.blockers_declared);
        // After blockers are declared the active player receives priority (CR 509.4).
        assert_eq!(after.priority, PlayerId(0));
    }

    #[test]
    fn issue_117_tapped_creature_cannot_block_cr_509_1a() {
        // CR 509.1a: a tapped creature can't be declared as a blocker.
        let db = db();
        let mut state = at_declare_attackers();
        let attacker =
            place_permanent(&mut state, fixture("walking_corpse"), PlayerId(0), false, 0);
        let tapped_blocker =
            place_permanent(&mut state, fixture("walking_corpse"), PlayerId(1), true, 0);
        let state = apply_action(
            &state,
            &Action::DeclareAttackers {
                attackers: atk1(&[attacker]),
            },
            &db,
        );
        let state = pass_full_round(&state, &db);

        assert!(!blocker_candidates(&state, &db).contains(&tapped_blocker));
        let after = apply_action(
            &state,
            &Action::DeclareBlockers {
                blocks: vec![Block {
                    blocker: tapped_blocker,
                    attacker,
                }],
            },
            &db,
        );
        assert_eq!(after, state, "declaring a tapped blocker is a no-op");
    }

    #[test]
    fn issue_117_blocker_must_be_assigned_to_an_attacking_creature_cr_509_1a() {
        // CR 509.1a: a blocker is assigned to an *attacking* creature. Assigning it
        // to a creature that is not attacking is an illegal declaration (a no-op).
        let db = db();
        let mut state = at_declare_attackers();
        let attacker =
            place_permanent(&mut state, fixture("walking_corpse"), PlayerId(0), false, 0);
        let non_attacker =
            place_permanent(&mut state, fixture("walking_corpse"), PlayerId(0), false, 0);
        let blocker = place_permanent(&mut state, fixture("walking_corpse"), PlayerId(1), false, 0);
        let state = apply_action(
            &state,
            &Action::DeclareAttackers {
                attackers: atk1(&[attacker]),
            },
            &db,
        );
        let state = pass_full_round(&state, &db);

        let after = apply_action(
            &state,
            &Action::DeclareBlockers {
                blocks: vec![Block {
                    blocker,
                    attacker: non_attacker,
                }],
            },
            &db,
        );
        assert_eq!(after, state, "blocking a non-attacker is a no-op");
    }

    #[test]
    fn issue_117_a_creature_cannot_be_declared_as_two_blocks_cr_509_1a() {
        // CR 509.1a: each blocker is assigned to *one* attacking creature, so the
        // same creature cannot appear as a blocker twice in one declaration.
        let db = db();
        let mut state = at_declare_attackers();
        let atk_a = place_permanent(&mut state, fixture("walking_corpse"), PlayerId(0), false, 0);
        let atk_b = place_permanent(&mut state, fixture("walking_corpse"), PlayerId(0), false, 0);
        let blocker = place_permanent(&mut state, fixture("walking_corpse"), PlayerId(1), false, 0);
        let state = apply_action(
            &state,
            &Action::DeclareAttackers {
                attackers: atk1(&[atk_a, atk_b]),
            },
            &db,
        );
        let state = pass_full_round(&state, &db);

        let after = apply_action(
            &state,
            &Action::DeclareBlockers {
                blocks: vec![
                    Block {
                        blocker,
                        attacker: atk_a,
                    },
                    Block {
                        blocker,
                        attacker: atk_b,
                    },
                ],
            },
            &db,
        );
        assert_eq!(after, state, "one creature blocking twice is a no-op");
    }

    #[test]
    fn issue_117_priority_is_withheld_until_attackers_are_declared_cr_508_1() {
        // CR 508.1: declaring attackers is a turn-based action performed before any
        // player receives priority in the step. The defender is offered nothing
        // until the active player has declared.
        let db = db();
        let mut state = at_declare_attackers();
        let _attacker =
            place_permanent(&mut state, fixture("walking_corpse"), PlayerId(0), false, 0);

        // The non-active player has no actions during the pre-declaration window.
        let mut defender_view = state.clone();
        defender_view.priority = PlayerId(1);
        assert!(valid(&defender_view, &db).is_empty());
    }

    #[test]
    fn issue_117_end_of_combat_removes_creatures_from_combat_cr_511_3() {
        // CR 511.3: at end of combat, all creatures are removed from combat — the
        // attacking flag and blocking assignments are cleared. Uses Giant Spiders
        // (2/4) so both survive the combat-damage step (issue #118) and are still on
        // the battlefield to check at end of combat.
        let db = db();
        let mut state = at_declare_attackers();
        let attacker = place_permanent(&mut state, fixture("giant_spider"), PlayerId(0), false, 0);
        let blocker = place_permanent(&mut state, fixture("giant_spider"), PlayerId(1), false, 0);

        // Declare attackers, pass to declare blockers, declare a block, then pass
        // through combat-damage into end-of-combat.
        let state = apply_action(
            &state,
            &Action::DeclareAttackers {
                attackers: atk1(&[attacker]),
            },
            &db,
        );
        let state = pass_full_round(&state, &db);
        let state = apply_action(
            &state,
            &Action::DeclareBlockers {
                blocks: vec![Block { blocker, attacker }],
            },
            &db,
        );
        // Passes: declare-blockers round → combat damage → end of combat.
        let state = pass_full_round(&state, &db); // → CombatDamage
        assert_eq!(state.step, Step::CombatDamage);
        let state = pass_full_round(&state, &db); // → EndCombat (turn-based action runs)
        assert_eq!(state.step, Step::EndCombat);

        assert!(find_perm(&state, attacker).attacking.is_none());
        assert_eq!(find_perm(&state, blocker).blocking, None);
    }

    // ----- Combat keywords I: flying/reach/vigilance/haste (issue #153) -----

    #[test]
    fn issue_153_vigilant_attacker_stays_untapped_and_can_block_next_turn_cr_702_20b() {
        // CR 702.20b: a creature with vigilance doesn't tap when it attacks, so it
        // stays untapped through combat and is available to block on the opponent's
        // next turn (an untapped creature can block, CR 509.1a). Serra Angel has
        // vigilance; Walking Corpse is a plain control.
        let db = db();
        let mut state = at_declare_attackers();
        let vigilant = place_permanent(&mut state, fixture("serra_angel"), PlayerId(0), false, 0);
        let plain = place_permanent(&mut state, fixture("walking_corpse"), PlayerId(0), false, 0);

        let after = apply_action(
            &state,
            &Action::DeclareAttackers {
                attackers: atk1(&[vigilant, plain]),
            },
            &db,
        );
        assert!(find_perm(&after, vigilant).attacking.is_some());
        assert!(
            !find_perm(&after, vigilant).tapped,
            "vigilance skips the attack tap (CR 702.20b)"
        );
        assert!(
            find_perm(&after, plain).tapped,
            "a non-vigilant attacker still taps (CR 508.1f)"
        );

        // Because it stayed untapped, on the opponent's turn (player 1 active, so
        // player 0 defends) it is an eligible blocker.
        let mut defense = after;
        defense.active_player = PlayerId(1);
        defense.step = Step::DeclareBlockers;
        let opp_attacker = place_permanent(
            &mut defense,
            fixture("walking_corpse"),
            PlayerId(1),
            false,
            0,
        );
        if let Some(p) = defense
            .battlefield
            .iter_mut()
            .find(|p| p.id == opp_attacker)
        {
            p.attacking = Some(PlayerId(1));
        }
        assert!(
            blocker_candidates(&defense, &db).contains(&vigilant),
            "the still-untapped vigilant creature can block next turn (CR 509.1a)"
        );
    }

    #[test]
    fn issue_153_hasty_creature_attacks_the_turn_it_enters_cr_702_10b() {
        // CR 702.10b: a creature with haste ignores the summoning-sickness attack
        // restriction, so Volcanic Dragon may attack even though it entered
        // this very turn — where a non-hasty creature could not (CR 302.6).
        let db = db();
        let mut state = at_declare_attackers();
        let hasty = place_permanent(
            &mut state,
            fixture("volcanic_dragon"),
            PlayerId(0),
            false,
            0,
        );
        let this_turn = state.turn;
        set_entered_turn(&mut state, hasty, this_turn);

        assert!(attacker_candidates(&state, &db).contains(&hasty));
        let after = apply_action(
            &state,
            &Action::DeclareAttackers {
                attackers: atk1(&[hasty]),
            },
            &db,
        );
        assert!(
            find_perm(&after, hasty).attacking.is_some(),
            "a hasty creature attacks the turn it enters (CR 702.10b)"
        );
        assert!(
            find_perm(&after, hasty).tapped,
            "attacking still taps it — it has no vigilance"
        );
    }

    #[test]
    fn issue_153_ground_creature_cannot_block_a_flyer_cr_702_9c() {
        // CR 702.9c / 702.17b: a ground creature assigned to block a flyer is an
        // illegal declaration (a no-op); a reach creature may block it. Snapping
        // Drake flies, Giant Spider has reach, Walking Corpse is a ground creature.
        let db = db();
        let mut state = at_declare_attackers();
        let flyer = place_permanent(&mut state, fixture("snapping_drake"), PlayerId(0), false, 0);
        let ground = place_permanent(&mut state, fixture("walking_corpse"), PlayerId(1), false, 0);
        let reacher = place_permanent(&mut state, fixture("giant_spider"), PlayerId(1), false, 0);

        let state = apply_action(
            &state,
            &Action::DeclareAttackers {
                attackers: atk1(&[flyer]),
            },
            &db,
        );
        let state = pass_full_round(&state, &db);
        assert_eq!(state.step, Step::DeclareBlockers);

        // A ground creature cannot be assigned to block the flyer: a no-op.
        let blocked_by_ground = apply_action(
            &state,
            &Action::DeclareBlockers {
                blocks: vec![Block {
                    blocker: ground,
                    attacker: flyer,
                }],
            },
            &db,
        );
        assert_eq!(
            blocked_by_ground, state,
            "a ground creature cannot block a flyer (CR 702.9c)"
        );

        // A reach creature can: the block is recorded.
        let blocked_by_reach = apply_action(
            &state,
            &Action::DeclareBlockers {
                blocks: vec![Block {
                    blocker: reacher,
                    attacker: flyer,
                }],
            },
            &db,
        );
        assert_eq!(
            find_perm(&blocked_by_reach, reacher).blocking,
            Some(flyer),
            "a reach creature can block a flyer (CR 702.17b)"
        );
        assert!(blocked_by_reach.blockers_declared);
    }

    // ----- Combat II: combat damage and lethal-damage SBA (issue #118) -----

    /// Whether a permanent id is still on the battlefield.
    fn alive(state: &GameState, id: PermanentId) -> bool {
        state.battlefield.iter().any(|p| p.id == id)
    }

    /// Wrap plain attacker ids as attacks on the sole opponent, seat 1 — the
    /// two-player default these combat tests exercise (issue #341).
    fn atk1(ids: &[PermanentId]) -> Vec<Attack> {
        ids.iter()
            .map(|&attacker| Attack {
                attacker,
                defender: PlayerId(1),
            })
            .collect()
    }

    /// Drive combat from the declare-attackers step through the combat-damage
    /// step: declare `attackers`, pass to declare-blockers, declare `blocks`, then
    /// pass into combat damage (where the turn-based damage assignment runs and the
    /// state-based-actions loop resolves). Returns the state paused at
    /// [`Step::CombatDamage`].
    fn run_combat(
        state: &GameState,
        attackers: Vec<PermanentId>,
        blocks: Vec<Block>,
        db: &CardDatabase,
    ) -> GameState {
        let state = apply_action(
            state,
            &Action::DeclareAttackers {
                attackers: atk1(&attackers),
            },
            db,
        );
        let state = pass_full_round(&state, db);
        assert_eq!(state.step, Step::DeclareBlockers);
        let state = apply_action(&state, &Action::DeclareBlockers { blocks }, db);
        // Issue #346: a multi-blocked attacker owes a combat-damage assignment order
        // before the priority round; submit the battlefield-order default so these
        // tests keep exercising the pre-#346 assignment order.
        let state = match default_damage_order(&state) {
            Some(order) => apply_action(&state, &order, db),
            None => state,
        };
        let state = pass_full_round(&state, db);
        assert_eq!(state.step, Step::CombatDamage);
        state
    }

    /// The battlefield-order `OrderCombatDamage` for every attacker that owes an
    /// order (issue #346), or `None` when none does. The deterministic default the
    /// server also falls back to on a timeout.
    fn default_damage_order(state: &GameState) -> Option<Action> {
        let owed = crate::combat::attackers_needing_damage_order(state);
        if owed.is_empty() {
            return None;
        }
        let orders = owed
            .into_iter()
            .map(|attacker| DamageOrder {
                attacker,
                blockers: state
                    .battlefield
                    .iter()
                    .filter(|p| p.blocking == Some(attacker))
                    .map(|p| p.id)
                    .collect(),
            })
            .collect();
        Some(Action::OrderCombatDamage { orders })
    }

    #[test]
    fn issue_346_attacker_orders_its_blockers_and_that_chooses_which_dies() {
        // CR 510.1: the attacking player orders a multi-blocked attacker's blockers,
        // and lethal-before-next assignment follows that order. A 3-power attacker
        // blocked by two 2-toughness creatures kills whichever it orders FIRST (it
        // takes the lethal 2; the second takes the leftover 1 and survives), so the
        // chosen order — not battlefield order — decides the casualty.
        let db = combat_db();
        let mut state = at_declare_attackers();
        let attacker = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(0), false, 0);
        let blk_a = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(1), false, 0);
        let blk_b = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(1), false, 0);

        let state = apply_action(
            &state,
            &Action::DeclareAttackers {
                attackers: atk1(&[attacker]),
            },
            &db,
        );
        let state = pass_full_round(&state, &db);
        let state = apply_action(
            &state,
            &Action::DeclareBlockers {
                blocks: vec![
                    Block {
                        blocker: blk_a,
                        attacker,
                    },
                    Block {
                        blocker: blk_b,
                        attacker,
                    },
                ],
            },
            &db,
        );

        // The declaration owes an ordering decision to the attacking player, and only
        // that action (plus concede) is offered.
        assert_eq!(
            crate::combat::pending_damage_order(&state),
            Some(PlayerId(0))
        );
        let offered = valid(&state, &db);
        assert!(offered
            .iter()
            .any(|a| matches!(a, Action::OrderCombatDamage { .. })));
        assert!(!offered.iter().any(|a| matches!(a, Action::PassPriority)));

        // Order blk_b first, the reverse of battlefield order.
        let state = apply_action(
            &state,
            &Action::OrderCombatDamage {
                orders: vec![DamageOrder {
                    attacker,
                    blockers: vec![blk_b, blk_a],
                }],
            },
            &db,
        );
        let state = pass_full_round(&state, &db);
        assert_eq!(state.step, Step::CombatDamage);

        let present = |id| state.battlefield.iter().any(|p| p.id == id);
        assert!(
            !present(blk_b),
            "the first-ordered blocker took the lethal damage"
        );
        assert!(
            present(blk_a),
            "the second-ordered blocker survived on the leftover 1"
        );
    }

    #[test]
    fn issue_346_a_single_blocker_needs_no_damage_order_decision() {
        // CR 510.1: an attacker blocked by one creature has no assignment choice, so
        // no ordering decision is offered — the declare-blockers priority round opens
        // straight away.
        let db = combat_db();
        let mut state = at_declare_attackers();
        let attacker = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(0), false, 0);
        let blocker = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(1), false, 0);
        let state = apply_action(
            &state,
            &Action::DeclareAttackers {
                attackers: atk1(&[attacker]),
            },
            &db,
        );
        let state = pass_full_round(&state, &db);
        let state = apply_action(
            &state,
            &Action::DeclareBlockers {
                blocks: vec![Block { blocker, attacker }],
            },
            &db,
        );
        assert_eq!(crate::combat::pending_damage_order(&state), None);
        assert!(valid(&state, &db)
            .iter()
            .any(|a| matches!(a, Action::PassPriority)));
    }

    #[test]
    fn issue_118_unblocked_attacker_damages_the_defending_player_cr_510_1c() {
        // CR 510.1c: an unblocked attacker assigns its combat damage to the player
        // it is attacking. A 3/2 test Boar hits the defender for 3.
        let db = combat_db();
        let mut state = at_declare_attackers();
        let attacker = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(0), false, 0);
        let start_life = state.players[1].life;

        let after = run_combat(&state, vec![attacker], Vec::new(), &db);

        assert_eq!(
            after.players[1].life,
            start_life - 3,
            "unblocked 3/2 deals 3 to the defending player (CR 510.1c)"
        );
        // The unblocked attacker took no damage and survives.
        assert!(alive(&after, attacker));
        assert_eq!(find_perm(&after, attacker).damage, 0);
    }

    #[test]
    fn issue_118_blocked_attacker_and_blocker_deal_lethal_and_both_die_cr_510_704_5g() {
        // CR 510.1c: a blocked attacker and its blocker deal combat damage to each
        // other. CR 704.5g: each takes lethal damage and is destroyed. Two 3/2
        // test Boars trade — both go to their owners' graveyards, and the defending
        // player takes no damage.
        let db = combat_db();
        let mut state = at_declare_attackers();
        let attacker = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(0), false, 0);
        let blocker = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(1), false, 0);
        let start_life = state.players[1].life;

        let after = run_combat(
            &state,
            vec![attacker],
            vec![Block { blocker, attacker }],
            &db,
        );

        assert!(!alive(&after, attacker), "attacker took lethal (CR 704.5g)");
        assert!(!alive(&after, blocker), "blocker took lethal (CR 704.5g)");
        assert_eq!(after.players[0].graveyard.len(), 1);
        assert_eq!(after.players[1].graveyard.len(), 1);
        assert_eq!(
            after.players[1].life, start_life,
            "a blocked attacker deals no damage to the defending player"
        );
    }

    #[test]
    fn issue_118_multi_block_mutual_destruction_cr_510_1c() {
        // CR 510.1c multi-block: a 4/5 Basilisk double-blocked by two 3/2 Boars
        // assigns its 4 power across the blockers in battlefield order,
        // lethal-per-blocker (2 each) — killing both — while the blockers deal a
        // combined 6 back, lethal to the 5-toughness attacker (CR 704.5g). All
        // three creatures are destroyed.
        let db = combat_db();
        let mut state = at_declare_attackers();
        let attacker = place_permanent(
            &mut state,
            id_in(&db, "test_basilisk"),
            PlayerId(0),
            false,
            0,
        );
        let blocker_a = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(1), false, 0);
        let blocker_b = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(1), false, 0);

        let after = run_combat(
            &state,
            vec![attacker],
            vec![
                Block {
                    blocker: blocker_a,
                    attacker,
                },
                Block {
                    blocker: blocker_b,
                    attacker,
                },
            ],
            &db,
        );

        assert!(!alive(&after, attacker), "4/5 dies to 3+3 combat damage");
        assert!(!alive(&after, blocker_a), "first blocker took lethal 2");
        assert!(!alive(&after, blocker_b), "second blocker took lethal 2");
        assert_eq!(after.players[0].graveyard.len(), 1);
        assert_eq!(after.players[1].graveyard.len(), 2);
    }

    #[test]
    fn issue_118_multi_block_assigns_lethal_in_battlefield_order_cr_510_1c() {
        // CR 510.1c: with no player-chosen order (deferred), the default splits the
        // attacker's power across blockers in battlefield order, assigning each
        // just-lethal before the next. A 4/5 Basilisk double-blocked by two 1/3
        // Otters assigns 3 (lethal) to the first Otter and the remaining 1 to the
        // second, so only the first dies; the leftover cannot spill further (no
        // trample). The Basilisk survives the 1+1 it takes, with that damage marked.
        let db = combat_db();
        let mut state = at_declare_attackers();
        let attacker = place_permanent(
            &mut state,
            id_in(&db, "test_basilisk"),
            PlayerId(0),
            false,
            0,
        );
        let first = place_permanent(&mut state, id_in(&db, "test_otter"), PlayerId(1), false, 0);
        let second = place_permanent(&mut state, id_in(&db, "test_otter"), PlayerId(1), false, 0);

        let after = run_combat(
            &state,
            vec![attacker],
            vec![
                Block {
                    blocker: first,
                    attacker,
                },
                Block {
                    blocker: second,
                    attacker,
                },
            ],
            &db,
        );

        assert!(
            !alive(&after, first),
            "first blocker took lethal 3 (1/3 Otter)"
        );
        assert!(
            alive(&after, second),
            "second blocker took only the leftover 1 and survives"
        );
        assert_eq!(
            find_perm(&after, second).damage,
            1,
            "the remaining 1 damage is marked on the second blocker"
        );
        assert!(
            alive(&after, attacker),
            "the 4/5 survives 1+1 combat damage"
        );
        assert_eq!(
            find_perm(&after, attacker).damage,
            2,
            "both blockers' 1 power is marked on the attacker"
        );
    }

    #[test]
    fn issue_118_combat_life_loss_flows_into_the_life_sba_cr_704_5a() {
        // CR 704.5a: a player at 0 or less life loses. Unblocked combat damage
        // (CR 510) reduces life, and the same SBA loop that runs after the action
        // registers the loss. Defender at 3 life takes 4 from a Basilisk and loses.
        let db = combat_db();
        let mut state = at_declare_attackers();
        state.players[1].life = 3;
        let attacker = place_permanent(
            &mut state,
            id_in(&db, "test_basilisk"),
            PlayerId(0),
            false,
            0,
        );

        let after = run_combat(&state, vec![attacker], Vec::new(), &db);

        assert_eq!(after.players[1].life, -1);
        assert!(
            after.players[1].has_lost,
            "combat life loss flows into the life ≤ 0 SBA (CR 704.5a)"
        );
        assert!(!after.players[0].has_lost);
    }

    #[test]
    fn issue_118_combat_marked_damage_is_cleared_at_cleanup_cr_514_2() {
        // CR 514.2: marked damage is removed at cleanup. A 4/5 Basilisk that
        // survives combat carries marked damage through the rest of the turn; by
        // the time the turn passes to the opponent, its combat cleanup has wiped it.
        let db = combat_db();
        let mut state = at_declare_attackers();
        let attacker = place_permanent(
            &mut state,
            id_in(&db, "test_basilisk"),
            PlayerId(0),
            false,
            0,
        );
        let blocker = place_permanent(&mut state, id_in(&db, "test_otter"), PlayerId(1), false, 0);

        let mut state = run_combat(
            &state,
            vec![attacker],
            vec![Block { blocker, attacker }],
            &db,
        );
        assert!(alive(&state, attacker));
        assert_eq!(
            find_perm(&state, attacker).damage,
            1,
            "1 damage marked in combat"
        );

        // Pass rounds until the turn advances to the opponent; the active player's
        // cleanup (CR 514.2) runs on the way and clears the marked damage.
        let mut guard = 0;
        while state.turn == 2 {
            state = pass_full_round(&state, &db);
            guard += 1;
            assert!(guard < 40, "combat should reach the next turn");
        }
        assert_eq!(
            find_perm(&state, attacker).damage,
            0,
            "marked damage is cleared at cleanup (CR 514.2)"
        );
    }

    // ----- Game over: decking, concede, win detection (issue #119) -----

    #[test]
    fn issue_119_decking_at_the_draw_step_loses_cr_704_5c() {
        // CR 704.5c: a player who attempts to draw from an empty library loses. On
        // turn 2 the active player (seat 1) reaches its draw step with an empty
        // library; the attempted draw makes it lose, so seat 0 wins (CR 104.2a).
        let db = db();
        let mut state = GameState::new_two_player();
        state.turn = 2;
        state.active_player = PlayerId(1);
        state.priority = PlayerId(1);
        state.step = Step::Upkeep; // both libraries empty by construction.

        let after = pass_full_round(&state, &db);

        assert_eq!(after.step, Step::Draw, "the walk stops at the draw step");
        assert!(
            after.players[1].has_lost,
            "an attempted draw from an empty library loses (CR 704.5c)"
        );
        assert_eq!(
            after.players[1].loss_reason,
            Some(LossReason::DrewFromEmptyLibrary)
        );
        let result = after.result().unwrap();
        assert_eq!(result.winner, Some(PlayerId(0)), "the other player wins");
        assert_eq!(result.losers, vec![PlayerId(1)]);
        assert_eq!(result.reason, LossReason::DrewFromEmptyLibrary);
    }

    #[test]
    fn issue_119_a_non_empty_draw_does_not_deck_cr_704_5c() {
        // CR 704.5c only fires on an *empty* library: a normal draw leaves the
        // player in the game with no loss recorded.
        let db = db();
        let mut state = GameState::new_two_player();
        state.turn = 2;
        state.active_player = PlayerId(1);
        state.priority = PlayerId(1);
        state.step = Step::Upkeep;
        let card = state.new_instance(fixture("onakke_ogre"));
        state.players[1].library = vec![card];

        let after = pass_full_round(&state, &db);

        assert!(after.players[1].hand.contains(&card), "the card was drawn");
        assert!(!after.players[1].has_lost, "a non-empty draw is no loss");
        assert!(after.result().is_none(), "the game continues");
    }

    #[test]
    fn issue_119_concede_ends_the_game_with_the_opponent_as_winner_cr_104_3a() {
        // CR 104.3a: conceding makes the conceding player lose; CR 104.2a: the
        // remaining player wins.
        let db = db();
        let state = GameState::new_two_player(); // seat 0 holds priority.
        assert!(valid_actions(&state, &db).contains(&Action::Concede));

        let after = apply_action(&state, &Action::Concede, &db);
        assert!(after.players[0].has_lost);
        assert_eq!(after.players[0].loss_reason, Some(LossReason::Concede));
        let result = after.result().unwrap();
        assert_eq!(result.winner, Some(PlayerId(1)));
        assert_eq!(result.losers, vec![PlayerId(0)]);
        assert_eq!(result.reason, LossReason::Concede);
    }

    #[test]
    fn issue_119_terminal_state_rejects_further_actions_purely_cr_104_2a() {
        // CR 104.2a: in a terminal state no action is legal; every submission is a
        // pure no-op that returns the terminal state unchanged.
        let db = db();
        let state = apply_action(&GameState::new_two_player(), &Action::Concede, &db);
        assert!(state.is_over());
        assert_eq!(apply_action(&state, &Action::PassPriority, &db), state);
        assert_eq!(apply_action(&state, &Action::Concede, &db), state);
    }

    #[test]
    fn issue_119_zero_life_loss_records_its_reason_cr_704_5a() {
        // CR 704.5a: the life ≤ 0 loss now carries its reason and consumes into a
        // terminal result naming the winner (CR 104.2a).
        let db = db();
        let mut state = GameState::new_two_player();
        state.players[1].life = 0;
        let after = apply_action(&state, &Action::PassPriority, &db);
        assert_eq!(after.players[1].loss_reason, Some(LossReason::ZeroLife));
        let result = after.result().unwrap();
        assert_eq!(result.winner, Some(PlayerId(0)));
        assert_eq!(result.reason, LossReason::ZeroLife);
    }

    // ----- Spell targets at cast + the first counterspell (issue #148) -----

    #[test]
    fn issue_148_counterspell_counters_a_creature_spell_end_to_end_cr_701_5() {
        // A creature spell (player 1) waits on the stack; player 0, holding
        // priority, casts Cancel ({1}{U}{U} instant) targeting it. The
        // counterspell records its target at cast (CR 601.2c) and, resolving first
        // (LIFO), removes the creature spell to its owner's graveyard without
        // resolving (CR 701.5a) — the creature never enters the battlefield.
        let db = db();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;

        // Player 1's Onakke Ogre (vanilla creature) on the stack.
        let boar = state.new_instance(fixture("onakke_ogre"));
        let boar_sid = StackId(state.mint_id());
        state.stack.push(StackObject {
            id: boar_sid,
            controller: PlayerId(1),
            kind: StackObjectKind::Spell { card: boar },
            targets: Vec::new(),
        });

        // Player 0 holds priority with the counterspell and {1}{U}{U}.
        let negation = state.new_instance(fixture("cancel"));
        state.players[0].hand = vec![negation];
        state.players[0].mana_pool.add(Color::Blue, 2);
        state.players[0].mana_pool.colorless = 1;
        state.priority = PlayerId(0);

        // Cast the counterspell targeting the creature spell (CR 601.2c).
        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: negation,
                targets: vec![Target::Spell(boar_sid)],
            },
            &db,
        );
        assert_eq!(
            state.stack.len(),
            2,
            "counterspell stacked over the creature"
        );
        assert_eq!(
            state.stack[1].targets,
            vec![Target::Spell(boar_sid)],
            "the chosen target is recorded on the stack at cast (CR 601.2c)"
        );
        assert_eq!(
            state.players[0].mana_pool.blue, 0,
            "the {{1}}{{U}}{{U}} was paid"
        );

        // Both pass: the counterspell resolves first and counters the creature.
        let state = apply_action(&state, &Action::PassPriority, &db);
        let state = apply_action(&state, &Action::PassPriority, &db);

        assert!(state.stack.is_empty(), "both spells have left the stack");
        assert!(
            state
                .battlefield
                .iter()
                .all(|p| p.card != fixture("onakke_ogre")),
            "the countered creature never entered the battlefield (CR 701.5a)"
        );
        assert!(
            state.players[1].graveyard.contains(&boar),
            "the countered spell went to its owner's graveyard (CR 701.5a)"
        );
        assert!(
            state.players[0]
                .graveyard
                .iter()
                .any(|c| c.id == negation.id),
            "the resolved counterspell went to its owner's graveyard (CR 608.2m)"
        );
    }

    #[test]
    fn issue_148_counterspell_fizzles_when_its_target_resolves_first_cr_608_2b() {
        // If the targeted spell resolves before the counterspell (the counterspell
        // sits *beneath* it), the counterspell's only target is gone at resolution,
        // so it fizzles (CR 608.2b): no spell is countered, and the counterspell
        // still goes to its owner's graveyard.
        let db = db();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;

        // Bottom of the stack: player 0's counterspell aimed at the creature above.
        let negation = state.new_instance(fixture("cancel"));
        let neg_sid = StackId(state.mint_id());
        let boar = state.new_instance(fixture("onakke_ogre"));
        let boar_sid = StackId(state.mint_id());
        state.stack.push(StackObject {
            id: neg_sid,
            controller: PlayerId(0),
            kind: StackObjectKind::Spell { card: negation },
            targets: vec![Target::Spell(boar_sid)],
        });
        // Top of the stack: player 1's vanilla creature spell, resolves first.
        state.stack.push(StackObject {
            id: boar_sid,
            controller: PlayerId(1),
            kind: StackObjectKind::Spell { card: boar },
            targets: Vec::new(),
        });

        // Resolve the top (the creature): it enters the battlefield.
        let state = apply_action(&state, &Action::PassPriority, &db);
        let state = apply_action(&state, &Action::PassPriority, &db);
        assert!(
            state
                .battlefield
                .iter()
                .any(|p| p.card == fixture("onakke_ogre")),
            "the creature spell resolved onto the battlefield"
        );

        // Resolve the counterspell: its target is gone, so it fizzles (CR 608.2b).
        let state = apply_action(&state, &Action::PassPriority, &db);
        let state = apply_action(&state, &Action::PassPriority, &db);
        assert!(state.stack.is_empty());
        assert!(
            state
                .battlefield
                .iter()
                .any(|p| p.card == fixture("onakke_ogre")),
            "the creature survives — nothing was countered"
        );
        assert!(
            state.players[0]
                .graveyard
                .iter()
                .any(|c| c.id == negation.id),
            "a fizzled spell still goes to its owner's graveyard (CR 608.2b)"
        );
    }

    // ----- Effect IR wave: damage, destroy, life, counters (issue #149) -----

    /// A precombat-main two-player game with player 0 the active player holding
    /// priority — so player 0 may cast at both instant and sorcery speed, an empty
    /// stack in front of it. Player 0 is the caster in the tests below.
    fn main_phase_p0() -> GameState {
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        state
    }

    #[test]
    fn issue_149_burn_spell_kills_a_creature_via_lethal_damage_sba_cr_704_5g() {
        // A burn spell that deals damage equal to a creature's toughness marks
        // lethal damage; the CR 704.5g state-based action then destroys it.
        let db = db();
        let mut state = main_phase_p0();
        // Onakke Ogre is a 4/2; Shock deals exactly 2 → lethal to its toughness.
        let boar = place_permanent(&mut state, fixture("onakke_ogre"), PlayerId(1), false, 0);
        let shock = state.new_instance(fixture("shock"));
        state.players[0].hand = vec![shock];
        state.players[0].mana_pool.add(Color::Red, 1);

        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: shock,
                targets: vec![Target::Permanent(boar)],
            },
            &db,
        );
        assert_eq!(state.stack.len(), 1, "the burn spell is on the stack");
        let state = pass_full_round(&state, &db);

        assert!(
            !state.battlefield.iter().any(|p| p.id == boar),
            "the burned creature is destroyed (CR 704.5g)"
        );
        assert_eq!(
            state.players[1].graveyard.len(),
            1,
            "it went to its owner's graveyard"
        );
    }

    #[test]
    fn issue_149_burn_spell_to_a_player_drops_life_and_loses_at_zero_cr_704_5a() {
        // The same burn verb aimed at a player is life loss (CR 120.3a); dropping a
        // player to 0 feeds the zero-life loss (CR 704.5a).
        let db = db();
        let mut state = main_phase_p0();
        state.players[1].life = 2;
        let shock = state.new_instance(fixture("shock"));
        state.players[0].hand = vec![shock];
        state.players[0].mana_pool.add(Color::Red, 1);

        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: shock,
                targets: vec![Target::Player(PlayerId(1))],
            },
            &db,
        );
        let state = pass_full_round(&state, &db);

        assert_eq!(state.players[1].life, 0);
        assert!(state.players[1].has_lost);
        assert_eq!(state.players[1].loss_reason, Some(LossReason::ZeroLife));
    }

    // ----- Wiring the four functionless cards (issue #256) -----

    #[test]
    fn issue_256_lightning_strike_deals_three_to_any_target() {
        // Lightning Strike is a {1}{R} bolt — 3 damage to any target, distinct from
        // Shock's 2. Aimed at a player on 3 life, it drops them to 0 (CR 704.5a).
        let db = db();
        let mut state = main_phase_p0();
        state.players[1].life = 3;
        let bolt = state.new_instance(fixture("lightning_strike"));
        state.players[0].hand = vec![bolt];
        state.players[0].mana_pool.add(Color::Red, 1);
        state.players[0].mana_pool.colorless = 1;

        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: bolt,
                targets: vec![Target::Player(PlayerId(1))],
            },
            &db,
        );
        let state = pass_full_round(&state, &db);

        assert_eq!(state.players[1].life, 0);
        assert!(state.players[1].has_lost);
    }

    #[test]
    fn issue_256_divination_draws_two_cards() {
        // Divination is a {2}{U} sorcery that draws two — DrawCard flowing through the
        // spell-resolution path (until now it was only ever a triggered-ability effect,
        // so this proves the cast → resolve routing).
        let db = db();
        let mut state = main_phase_p0();
        let study = state.new_instance(fixture("divination"));
        state.players[0].hand = vec![study];
        let first = state.new_instance(fixture("forest"));
        let second = state.new_instance(fixture("forest"));
        state.players[0].library = vec![first, second];
        state.players[0].mana_pool.add(Color::Blue, 1);
        state.players[0].mana_pool.colorless = 2;

        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: study,
                targets: Vec::new(),
            },
            &db,
        );
        let state = pass_full_round(&state, &db);

        assert!(state.players[0].hand.contains(&first));
        assert!(state.players[0].hand.contains(&second));
        assert!(state.players[0].library.is_empty());
    }

    #[test]
    fn issue_256_enchantment_etb_gains_life_when_it_enters() {
        // A {G} enchantment whose enters-the-battlefield trigger gains its controller
        // 4 life — an ETB trigger on a *non-creature* permanent, and GainLife as an
        // ability effect rather than a spell effect. No M19 card carries this, so it
        // is exercised inline (ADR 0025).
        let json = r#"[{"schema_version":1,"functional_id":"test_blessing","name":"Test Blessing",
            "types":["enchantment"],"subtypes":[],"mana_cost":"{G}","colors":["green"],
            "abilities":[{"type":"triggered","event":"self_enters_battlefield",
              "effects":[{"kind":"gain_life","player_ref":"controller","amount":4}]}]}]"#;
        let db = CardDatabase::from_json(json).unwrap();
        let mut state = main_phase_p0();
        let life_before = state.players[0].life;
        let blessing = state.new_instance(id_in(&db, "test_blessing"));
        state.players[0].hand = vec![blessing];
        state.players[0].mana_pool.add(Color::Green, 1);

        // Cast it; pass twice so it resolves and its ETB trigger goes on the stack.
        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: blessing,
                targets: Vec::new(),
            },
            &db,
        );
        let state = pass_full_round(&state, &db);
        assert!(state
            .battlefield
            .iter()
            .any(|p| p.card == id_in(&db, "test_blessing")));
        assert_eq!(state.stack.len(), 1, "its ETB trigger is on the stack");

        // Pass twice more: the trigger resolves and the controller gains 4 life.
        let state = pass_full_round(&state, &db);
        assert!(state.stack.is_empty());
        assert_eq!(state.players[0].life, life_before + 4);
    }

    #[test]
    fn issue_256_mana_rock_taps_for_colorless_mana() {
        // A {1} mana rock — {T}: Add {C}. Its ability is a mana ability, so it
        // resolves immediately without using the stack (CR 605.3). The colorless-mana
        // verb has no M19 representative, so it is exercised inline (ADR 0025).
        let json = r#"[{"schema_version":1,"functional_id":"test_lodestone","name":"Test Lodestone",
            "types":["artifact"],"mana_cost":"{1}","colors":[],
            "abilities":[{"type":"activated","cost":[{"kind":"tap"}],
              "effects":[{"kind":"add_colorless_mana","amount":1}]}]}]"#;
        let db = CardDatabase::from_json(json).unwrap();
        let mut state = main_phase_p0();
        let lodestone = place_permanent(
            &mut state,
            id_in(&db, "test_lodestone"),
            PlayerId(0),
            false,
            0,
        );

        let after = apply_action(
            &state,
            &Action::ActivateAbility {
                permanent: lodestone,
                index: 0,
                targets: Vec::new(),
            },
            &db,
        );
        assert_eq!(after.players[0].mana_pool.colorless, 1);
        assert!(find_perm(&after, lodestone).tapped);
        assert!(after.stack.is_empty());
    }

    #[test]
    fn issue_149_destroy_puts_a_creature_in_its_owners_graveyard_cr_701_7() {
        let db = db();
        let mut state = main_phase_p0();
        let boar = place_permanent(&mut state, fixture("onakke_ogre"), PlayerId(1), false, 0);
        // Murder is a {1}{B}{B} instant: two black pips and one generic.
        let ray = state.new_instance(fixture("murder"));
        state.players[0].hand = vec![ray];
        state.players[0].mana_pool.add(Color::Black, 2);
        state.players[0].mana_pool.colorless = 1;

        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: ray,
                targets: vec![Target::Permanent(boar)],
            },
            &db,
        );
        let state = pass_full_round(&state, &db);

        assert!(
            !state.battlefield.iter().any(|p| p.id == boar),
            "the targeted creature is destroyed (CR 701.7)"
        );
        assert!(state.players[1]
            .graveyard
            .iter()
            .any(|c| c.card == fixture("onakke_ogre")));
    }

    #[test]
    fn issue_149_destroy_fizzles_if_its_target_left_first_cr_608_2b() {
        let db = db();
        let mut state = main_phase_p0();
        let boar = place_permanent(&mut state, fixture("onakke_ogre"), PlayerId(1), false, 0);
        let ray = state.new_instance(fixture("murder"));
        state.players[0].hand = vec![ray];
        state.players[0].mana_pool.add(Color::Black, 2);
        state.players[0].mana_pool.colorless = 1;

        let mut state = apply_action(
            &state,
            &Action::CastSpell {
                card: ray,
                targets: vec![Target::Permanent(boar)],
            },
            &db,
        );
        // The target leaves the battlefield before the sorcery resolves.
        state.battlefield.retain(|p| p.id != boar);

        let state = pass_full_round(&state, &db);
        assert!(state.stack.is_empty());
        assert!(
            state.players[0].graveyard.iter().any(|c| c.id == ray.id),
            "a fizzled spell still goes to its owner's graveyard (CR 608.2b)"
        );
    }

    #[test]
    fn issue_149_minus_one_counter_lowers_toughness_to_lethal_cr_704_5g() {
        // A -1/-1 counter folds into computed toughness (CR 613.7c). A 3/2 with 1
        // marked damage is not lethal (1 < 2); after a -1/-1 counter it is a 2/1
        // and 1 damage is lethal (1 ≥ 1), so the SBA destroys it. The -1/-1 counter
        // spell has no M19 representative, so both cards are inline (ADR 0025).
        let json = r#"[
            {"schema_version":1,"functional_id":"test_boar","name":"Test Boar",
             "types":["creature"],"subtypes":["Boar"],"mana_cost":"{2}{G}","colors":["green"],
             "power":3,"toughness":2},
            {"schema_version":1,"functional_id":"test_wither","name":"Test Wither",
             "types":["sorcery"],"mana_cost":"{B}","colors":["black"],
             "spell_effects":[{"kind":"put_counters","target":"any_creature","counter":"minus_one_minus_one","count":1}]}
        ]"#;
        let db = CardDatabase::from_json(json).unwrap();
        let mut state = main_phase_p0();
        let boar = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(1), false, 1);
        let touch = state.new_instance(id_in(&db, "test_wither")); // {B} sorcery, -1/-1
        state.players[0].hand = vec![touch];
        state.players[0].mana_pool.add(Color::Black, 1);

        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: touch,
                targets: vec![Target::Permanent(boar)],
            },
            &db,
        );
        let state = pass_full_round(&state, &db);

        assert!(
            !state.battlefield.iter().any(|p| p.id == boar),
            "a -1/-1 counter made toughness ≤ marked damage → destroyed (CR 704.5g)"
        );
        assert_eq!(state.players[1].graveyard.len(), 1);
    }

    #[test]
    fn issue_149_life_gain_adds_to_a_low_life_total_cr_119() {
        let db = db();
        let mut state = main_phase_p0();
        state.players[0].life = 1;
        let balm = state.new_instance(fixture("revitalize")); // Revitalize {W}: gain 3, draw 1
        state.players[0].hand = vec![balm];
        // Revitalize also draws, so seed a card to avoid decking out (CR 704.5c).
        state.players[0].library = vec![state.new_instance(fixture("forest"))];
        state.players[0].mana_pool.add(Color::White, 1);

        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: balm,
                targets: Vec::new(),
            },
            &db,
        );
        let state = pass_full_round(&state, &db);

        assert_eq!(state.players[0].life, 4);
        assert!(!state.players[0].has_lost);
    }

    #[test]
    fn issue_149_life_loss_to_exactly_zero_triggers_the_loss_cr_704_5a() {
        // The lose-life verb has no M19 representative, so it is exercised inline.
        let json = r#"[{"schema_version":1,"functional_id":"test_drain","name":"Test Drain",
            "types":["instant"],"mana_cost":"{B}","colors":["black"],
            "spell_effects":[{"kind":"lose_life","player_ref":"controller","amount":2}]}]"#;
        let db = CardDatabase::from_json(json).unwrap();
        let mut state = main_phase_p0();
        state.players[0].life = 2;
        let ordeal = state.new_instance(id_in(&db, "test_drain")); // {B} instant, lose 2
        state.players[0].hand = vec![ordeal];
        state.players[0].mana_pool.add(Color::Black, 1);

        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: ordeal,
                targets: Vec::new(),
            },
            &db,
        );
        let state = pass_full_round(&state, &db);

        assert_eq!(state.players[0].life, 0);
        assert!(state.players[0].has_lost);
        assert_eq!(state.players[0].loss_reason, Some(LossReason::ZeroLife));
    }

    // ----- Combat II: first strike / trample / deathtouch / lifelink (issue #154) -----
    // These keywords have no clean M19 representative, so the bodies come from the
    // inline `combat_db()`: test_duelist first strike (2/2), test_trampler trample
    // (5/4), test_adder deathtouch (1/1), test_lifelinker lifelink (2/3),
    // test_baneclaw trample+deathtouch (4/4); test_boar (3/2), test_basilisk (4/5).

    #[test]
    fn issue_154_first_striker_kills_its_blocker_before_it_strikes_back_cr_510_5() {
        // CR 510.5: a 2/2 first striker deals in the first-strike step, killing a
        // 3/2 Boar (2 ≥ 2) before the regular step — so the Boar deals no damage
        // back and the first striker survives untouched, though a 3/2 would
        // otherwise have killed it.
        let db = combat_db();
        let mut state = at_declare_attackers();
        let striker = place_permanent(
            &mut state,
            id_in(&db, "test_duelist"),
            PlayerId(0),
            false,
            0,
        );
        let boar = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(1), false, 0);

        let after = run_combat(
            &state,
            vec![striker],
            vec![Block {
                blocker: boar,
                attacker: striker,
            }],
            &db,
        );

        assert!(!alive(&after, boar), "the blocker died to first strike");
        assert!(
            alive(&after, striker),
            "the first striker survives — its blocker never dealt damage"
        );
        assert_eq!(
            find_perm(&after, striker).damage,
            0,
            "no damage was dealt back to the first striker (CR 510.5)"
        );
    }

    #[test]
    fn issue_154_two_first_strikers_still_trade_cr_510_5() {
        // CR 510.5: two 2/2 first strikers both deal in the first-strike step, so
        // they trade normally — each deals lethal to the other simultaneously.
        let db = combat_db();
        let mut state = at_declare_attackers();
        let attacker = place_permanent(
            &mut state,
            id_in(&db, "test_duelist"),
            PlayerId(0),
            false,
            0,
        );
        let blocker = place_permanent(
            &mut state,
            id_in(&db, "test_duelist"),
            PlayerId(1),
            false,
            0,
        );

        let after = run_combat(
            &state,
            vec![attacker],
            vec![Block { blocker, attacker }],
            &db,
        );

        assert!(
            !alive(&after, attacker),
            "first striker took lethal first strike"
        );
        assert!(
            !alive(&after, blocker),
            "first striker took lethal first strike"
        );
    }

    #[test]
    fn issue_154_deathtouch_one_damage_destroys_a_big_creature_cr_704_5h() {
        // CR 702.2b / 704.5h: a 1/1 deathtouch blocker deals 1 to a 4/5 attacker,
        // which is not lethal by toughness (1 < 5) but is lethal by deathtouch — the
        // Basilisk is destroyed. The 1/1 dies to the Basilisk's 4 (CR 704.5g).
        let db = combat_db();
        let mut state = at_declare_attackers();
        let basilisk = place_permanent(
            &mut state,
            id_in(&db, "test_basilisk"),
            PlayerId(0),
            false,
            0,
        );
        let adder = place_permanent(&mut state, id_in(&db, "test_adder"), PlayerId(1), false, 0);

        let after = run_combat(
            &state,
            vec![basilisk],
            vec![Block {
                blocker: adder,
                attacker: basilisk,
            }],
            &db,
        );

        assert!(
            !alive(&after, basilisk),
            "1 deathtouch damage destroys the 4/5 (CR 704.5h)"
        );
        assert!(
            !alive(&after, adder),
            "the 1/1 took the Basilisk's 4 (CR 704.5g)"
        );
        assert!(
            after.deathtouch_struck.is_empty(),
            "the deathtouch flag is consumed by the SBA loop"
        );
    }

    #[test]
    fn issue_154_deathtouch_attacker_kills_the_five_five_it_strikes() {
        // Acceptance: a deathtouch 1/1 kills a large creature in combat. The 1/1
        // attacker assigns 1 (deathtouch-lethal) to a 4/5 blocker; the blocker is
        // destroyed by CR 704.5h.
        let db = combat_db();
        let mut state = at_declare_attackers();
        let adder = place_permanent(&mut state, id_in(&db, "test_adder"), PlayerId(0), false, 0);
        let basilisk = place_permanent(
            &mut state,
            id_in(&db, "test_basilisk"),
            PlayerId(1),
            false,
            0,
        );

        let after = run_combat(
            &state,
            vec![adder],
            vec![Block {
                blocker: basilisk,
                attacker: adder,
            }],
            &db,
        );

        assert!(
            !alive(&after, basilisk),
            "deathtouch kills the 4/5 (CR 704.5h)"
        );
    }

    #[test]
    fn issue_154_trample_over_a_chump_block_hits_the_player_cr_702_19e() {
        // CR 702.19e: a blocked 5/4 trampler assigns 2 (lethal) to a 3/2 Boar and
        // tramples the remaining 3 to the defending player.
        let db = combat_db();
        let mut state = at_declare_attackers();
        let start_life = state.players[1].life;
        let trampler = place_permanent(
            &mut state,
            id_in(&db, "test_trampler"),
            PlayerId(0),
            false,
            0,
        );
        let chump = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(1), false, 0);

        let after = run_combat(
            &state,
            vec![trampler],
            vec![Block {
                blocker: chump,
                attacker: trampler,
            }],
            &db,
        );

        assert!(!alive(&after, chump), "the chump blocker died");
        assert_eq!(
            after.players[1].life,
            start_life - 3,
            "the excess 3 tramples over to the player (CR 702.19e)"
        );
    }

    #[test]
    fn issue_154_full_block_absorbs_all_trample_damage_cr_702_19e() {
        // CR 702.19e: only the excess over lethal tramples. A 5/4 trampler fully
        // blocked by a 4/5 Basilisk assigns all 5 to the Basilisk (still 5 short of
        // absorbing? no — 5 ≥ 5 toughness) with none left over, so the player takes
        // nothing.
        let db = combat_db();
        let mut state = at_declare_attackers();
        let start_life = state.players[1].life;
        let trampler = place_permanent(
            &mut state,
            id_in(&db, "test_trampler"),
            PlayerId(0),
            false,
            0,
        ); // 5/4
        let wall = place_permanent(
            &mut state,
            id_in(&db, "test_basilisk"),
            PlayerId(1),
            false,
            0,
        ); // 4/5

        let after = run_combat(
            &state,
            vec![trampler],
            vec![Block {
                blocker: wall,
                attacker: trampler,
            }],
            &db,
        );

        assert_eq!(
            after.players[1].life, start_life,
            "a fully-absorbing blocker leaves no trample excess (CR 702.19e)"
        );
        assert!(!alive(&after, wall), "the 4/5 took lethal 5");
    }

    #[test]
    fn issue_154_deathtouch_trampler_assigns_one_per_blocker_rest_to_player() {
        // CR 510.1e + 702.19e: a 4/4 trample+deathtouch attacker needs assign only 1
        // to a 4/5 blocker (deathtouch makes 1 lethal), tramping the other 3 over to
        // the player; the blocker is destroyed by CR 704.5h.
        let db = combat_db();
        let mut state = at_declare_attackers();
        let start_life = state.players[1].life;
        let baneclaw = place_permanent(
            &mut state,
            id_in(&db, "test_baneclaw"),
            PlayerId(0),
            false,
            0,
        ); // 4/4
        let blocker = place_permanent(
            &mut state,
            id_in(&db, "test_basilisk"),
            PlayerId(1),
            false,
            0,
        ); // 4/5

        let after = run_combat(
            &state,
            vec![baneclaw],
            vec![Block {
                blocker,
                attacker: baneclaw,
            }],
            &db,
        );

        assert!(
            !alive(&after, blocker),
            "1 deathtouch damage destroys the blocker (CR 704.5h)"
        );
        assert_eq!(
            after.players[1].life,
            start_life - 3,
            "assigns 1 to the blocker, tramples 3 to the player (CR 510.1e/702.19e)"
        );
    }

    #[test]
    fn issue_154_lifelink_gains_life_in_the_same_event_as_the_damage_cr_702_15e() {
        // CR 702.15e: a lifelink source gains its controller life equal to the
        // damage, simultaneously. An unblocked 2/3 lifelinker hits player 1 for 2
        // and its controller (player 0) gains 2.
        let db = combat_db();
        let mut state = at_declare_attackers();
        let atk_life = state.players[0].life;
        let def_life = state.players[1].life;
        let cleric = place_permanent(
            &mut state,
            id_in(&db, "test_lifelinker"),
            PlayerId(0),
            false,
            0,
        );

        let after = run_combat(&state, vec![cleric], Vec::new(), &db);

        assert_eq!(
            after.players[0].life,
            atk_life + 2,
            "lifelink gains its controller 2 (CR 702.15e)"
        );
        assert_eq!(after.players[1].life, def_life - 2, "the defender took 2");
    }

    #[test]
    fn issue_154_lifelink_on_blocking_damage_gains_life_cr_702_15e() {
        // CR 702.15e: lifelink applies to any damage the source deals, including a
        // blocker's damage to the attacker. A 2/3 lifelink blocker deals 2 to a 3/2
        // Boar and its controller gains 2, even as the blocker dies to the Boar.
        let db = combat_db();
        let mut state = at_declare_attackers();
        let boar = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(0), false, 0);
        let cleric = place_permanent(
            &mut state,
            id_in(&db, "test_lifelinker"),
            PlayerId(1),
            false,
            0,
        );
        let def_life = state.players[1].life;

        let after = run_combat(
            &state,
            vec![boar],
            vec![Block {
                blocker: cleric,
                attacker: boar,
            }],
            &db,
        );

        assert_eq!(
            after.players[1].life,
            def_life + 2,
            "the lifelink blocker's controller gains 2 from its combat damage"
        );
    }

    // ----- Double strike (issue #373, CR 702.4) -----
    // Double strike has no clean M19 representative, so its bodies come from the
    // inline `combat_db()`: test_twinstrike (2/2 double strike), test_twintrample
    // (3/3 double strike + trample), test_ward (3/3 first strike, survives a 2/2's
    // first hit and kills it back).

    #[test]
    fn issue_373_unblocked_double_striker_deals_its_power_twice_cr_702_4b() {
        // CR 702.4b: an unblocked double striker deals combat damage in both the
        // first-strike and the regular step — a 2/2 double striker hits the defending
        // player for 2 twice, so it loses 4.
        let db = combat_db();
        let mut state = at_declare_attackers();
        let start_life = state.players[1].life;
        let striker = place_permanent(
            &mut state,
            id_in(&db, "test_twinstrike"),
            PlayerId(0),
            false,
            0,
        );

        let after = run_combat(&state, vec![striker], Vec::new(), &db);

        assert_eq!(
            after.players[1].life,
            start_life - 4,
            "a 2/2 double striker deals its power twice (CR 702.4b)"
        );
        assert!(alive(&after, striker), "the unblocked striker is untouched");
    }

    #[test]
    fn issue_373_blocked_double_striker_deals_no_regular_damage_without_trample_cr_702_4b() {
        // CR 702.4b: a 2/2 double striker kills its 3/2 blocker in the first-strike
        // step (2 ≥ 2). With its blocker dead and no trample, its regular-step strike
        // has nowhere to go — it deals no damage to anything, and takes none back (the
        // blocker died before it could strike).
        let db = combat_db();
        let mut state = at_declare_attackers();
        let start_life = state.players[1].life;
        let striker = place_permanent(
            &mut state,
            id_in(&db, "test_twinstrike"),
            PlayerId(0),
            false,
            0,
        );
        let boar = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(1), false, 0);

        let after = run_combat(
            &state,
            vec![striker],
            vec![Block {
                blocker: boar,
                attacker: striker,
            }],
            &db,
        );

        assert!(!alive(&after, boar), "the blocker died to first strike");
        assert!(
            alive(&after, striker),
            "the striker survives its dead blocker"
        );
        assert_eq!(
            find_perm(&after, striker).damage,
            0,
            "the blocker never struck back (CR 510.5)"
        );
        assert_eq!(
            after.players[1].life, start_life,
            "a blocked non-trampler's regular strike hits nothing (CR 509.1h)"
        );
    }

    #[test]
    fn issue_373_double_strike_trample_carries_the_regular_strike_over_a_dead_blocker_cr_702_4b() {
        // CR 702.4b + 702.19e: a 3/3 double-strike trampler blocked by a 3/2 Boar
        // assigns 2 (lethal) to the Boar and tramples 1 in the first-strike step; the
        // Boar dies before the regular step, so the whole 3 of the regular strike
        // tramples to the player. The defender loses 1 + 3 = 4.
        let db = combat_db();
        let mut state = at_declare_attackers();
        let start_life = state.players[1].life;
        let trampler = place_permanent(
            &mut state,
            id_in(&db, "test_twintrample"),
            PlayerId(0),
            false,
            0,
        );
        let boar = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(1), false, 0);

        let after = run_combat(
            &state,
            vec![trampler],
            vec![Block {
                blocker: boar,
                attacker: trampler,
            }],
            &db,
        );

        assert!(!alive(&after, boar), "the blocker died to first strike");
        assert!(alive(&after, trampler), "the trampler survives");
        assert_eq!(
            after.players[1].life,
            start_life - 4,
            "1 trample excess in the first step, the full 3 over the dead blocker in \
             the regular step (CR 702.4b/702.19e)"
        );
    }

    #[test]
    fn issue_373_double_striker_slain_in_the_first_step_deals_no_regular_damage_cr_702_4b() {
        // CR 702.4b: a double striker that dies during/after the first-strike step
        // deals no regular-step damage. A 2/2 double striker attacks a 3/3 first-strike
        // blocker: in the first-strike step both deal — the striker marks 2 on the ward
        // (which survives), the ward's 3 kills the striker. The dead striker never
        // deals its second hit, so the ward keeps exactly 2 marked (a second hit would
        // make it 4 and destroy it).
        let db = combat_db();
        let mut state = at_declare_attackers();
        let striker = place_permanent(
            &mut state,
            id_in(&db, "test_twinstrike"),
            PlayerId(0),
            false,
            0,
        );
        let ward = place_permanent(&mut state, id_in(&db, "test_ward"), PlayerId(1), false, 0);

        let after = run_combat(
            &state,
            vec![striker],
            vec![Block {
                blocker: ward,
                attacker: striker,
            }],
            &db,
        );

        assert!(
            !alive(&after, striker),
            "the double striker took lethal first strike"
        );
        assert!(
            alive(&after, ward),
            "the 3/3 ward survived the one 2-damage hit"
        );
        assert_eq!(
            find_perm(&after, ward).damage,
            2,
            "the slain double striker dealt no regular-step damage (CR 702.4b)"
        );
    }

    // ----- Dies / leaves-battlefield triggers (issue #151, CR 603.6c / 700.4) -----

    /// Push a player-0-controlled ability with `effects` (aimed at `targets`) onto
    /// the stack, one full priority round from resolving. Mirrors how a real cast
    /// or activation seats an ability, so the whole death-then-trigger pipeline is
    /// driven through the public `apply_action` path.
    fn push_ability(
        state: &mut GameState,
        source: PermanentId,
        effects: Vec<Effect>,
        targets: Vec<Target>,
    ) {
        let id = state.mint_id();
        state.stack.push(StackObject {
            id: StackId(id),
            controller: PlayerId(0),
            kind: StackObjectKind::Ability { source, effects },
            targets,
        });
    }

    /// A precombat-main two-player game with the dies fixture (`test_lurker`, a
    /// 2/2 with a self-dies draw) on the battlefield under player 0 with `damage`
    /// marked, and a single card in player 0's library to draw. Cards are resolved
    /// from `db` (a [`combat_db`]). Returns the state and the lurker's id.
    fn state_with_lurker(db: &CardDatabase, damage: u32) -> (GameState, PermanentId, CardInstance) {
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let lurker = place_permanent(
            &mut state,
            id_in(db, "test_lurker"),
            PlayerId(0),
            false,
            damage,
        );
        let draw = state.new_instance(id_in(db, "test_boar"));
        state.players[0].library = vec![draw];
        (state, lurker, draw)
    }

    #[test]
    fn issue_151_dies_trigger_fires_from_lethal_combat_damage_cr_700_4() {
        // CR 700.4 / 603.6c: a creature put into a graveyard by lethal combat
        // damage (CR 704.5g) dies, firing its dies trigger. The 2/2 Lurker attacks
        // into a 4/5 Basilisk blocker, takes 4, and dies; its controller then draws.
        let db = combat_db();
        let mut state = at_declare_attackers();
        let lurker = place_permanent(&mut state, id_in(&db, "test_lurker"), PlayerId(0), false, 0);
        let blocker = place_permanent(
            &mut state,
            id_in(&db, "test_basilisk"),
            PlayerId(1),
            false,
            0,
        );
        let draw = state.new_instance(id_in(&db, "test_boar"));
        state.players[0].library = vec![draw];

        let after = run_combat(
            &state,
            vec![lurker],
            vec![Block {
                blocker,
                attacker: lurker,
            }],
            &db,
        );

        // The lurker died through the leaves-battlefield seam; its dies trigger is a
        // synthetic stack entry that has not resolved yet (CR 603.3b).
        assert!(
            !alive(&after, lurker),
            "the 2/2 took 4 combat damage and died"
        );
        assert_eq!(after.stack.len(), 1, "the dies trigger is on the stack");
        assert!(after.players[0].hand.is_empty(), "it has not resolved yet");

        // A full priority round resolves it: player 0 draws.
        let after = pass_full_round(&after, &db);
        assert!(after.stack.is_empty());
        assert!(
            after.players[0].hand.contains(&draw),
            "the dies trigger drew its controller a card (CR 700.4)"
        );
    }

    #[test]
    fn issue_151_dies_trigger_fires_from_a_destroy_effect_cr_701_7() {
        // CR 701.7 → 700.4: a `Destroy` effect routes the creature to its graveyard
        // through the same seam, so the dies trigger fires exactly as it does for a
        // combat death.
        use crate::ability::TargetSpec;
        let db = combat_db();
        let (mut state, lurker, draw) = state_with_lurker(&db, 0);
        push_ability(
            &mut state,
            lurker,
            vec![Effect::Destroy {
                target: TargetSpec::AnyCreature,
            }],
            vec![Target::Permanent(lurker)],
        );

        // Resolve the destroy: the lurker dies and its dies trigger replaces it.
        let state = apply_action(&state, &Action::PassPriority, &db);
        let state = apply_action(&state, &Action::PassPriority, &db);
        assert!(!alive(&state, lurker), "the Destroy killed the lurker");
        assert_eq!(state.stack.len(), 1, "the dies trigger is on the stack");
        assert!(state.players[0].hand.is_empty(), "it has not resolved yet");

        // Resolve the dies trigger: player 0 draws.
        let state = pass_full_round(&state, &db);
        assert!(state.stack.is_empty());
        assert!(state.players[0].hand.contains(&draw));
    }

    #[test]
    fn issue_151_dies_trigger_fires_from_a_minus_one_counter_toughness_drop() {
        // CR 704.5g → 700.4: a `-1/-1` counter drops the 2/2 Lurker to a 2/1, making
        // its 1 marked damage lethal; the SBA loop destroys it through the seam and
        // the dies trigger fires.
        use crate::ability::TargetSpec;
        use crate::state::CounterKind;
        let db = combat_db();
        let (mut state, lurker, draw) = state_with_lurker(&db, 1);
        push_ability(
            &mut state,
            lurker,
            vec![Effect::PutCounters {
                target: TargetSpec::AnyCreature,
                counter: CounterKind::MinusOneMinusOne,
                count: 1,
            }],
            vec![Target::Permanent(lurker)],
        );

        // Resolve the counter: toughness 2→1, 1 marked damage is now lethal, the
        // lurker dies, and its dies trigger lands on the stack.
        let state = apply_action(&state, &Action::PassPriority, &db);
        let state = apply_action(&state, &Action::PassPriority, &db);
        assert!(
            !alive(&state, lurker),
            "the -1/-1 toughness drop made the marked damage lethal (CR 704.5g)"
        );
        assert_eq!(state.stack.len(), 1, "the dies trigger is on the stack");

        // Resolve the dies trigger: player 0 draws.
        let state = pass_full_round(&state, &db);
        assert!(state.stack.is_empty());
        assert!(state.players[0].hand.contains(&draw));
    }

    #[test]
    fn issue_151_dies_trigger_is_a_synthetic_stack_entry_resolving_after_priority_cr_603_3b() {
        // CR 603.3b: an ability that triggers during the state-based-action check is
        // put on the stack the next time a player would receive priority, not
        // resolved immediately. After the death-causing action, the trigger sits on
        // the stack with a player holding priority and the draw has not happened; it
        // resolves only once priority passes around.
        use crate::ability::TargetSpec;
        let db = combat_db();
        let (mut state, lurker, draw) = state_with_lurker(&db, 0);
        push_ability(
            &mut state,
            lurker,
            vec![Effect::Destroy {
                target: TargetSpec::AnyCreature,
            }],
            vec![Target::Permanent(lurker)],
        );

        // The action that kills the lurker leaves its dies trigger on the stack —
        // one synthetic ability entry, unresolved, with priority handed to a player.
        let paused = apply_action(&state, &Action::PassPriority, &db);
        let paused = apply_action(&paused, &Action::PassPriority, &db);
        assert_eq!(paused.stack.len(), 1);
        assert!(matches!(
            paused.stack[0].kind,
            StackObjectKind::Ability { source, .. } if source == lurker
        ));
        assert_eq!(
            paused.consecutive_passes, 0,
            "priority was handed out fresh with the trigger on the stack (CR 603.3b)"
        );
        assert!(
            paused.players[0].library.contains(&draw),
            "the trigger has not resolved, so nothing is drawn yet"
        );

        // Only a full priority round resolves the synthetic entry.
        let resolved = pass_full_round(&paused, &db);
        assert!(resolved.stack.is_empty());
        assert!(resolved.players[0].hand.contains(&draw));
    }

    #[test]
    fn issue_259_step_transition_is_recorded_in_authoritative_log() {
        let database = db();
        let state = GameState::new_two_player();
        let state = apply_action(&state, &Action::PassPriority, &database);
        let state = apply_action(&state, &Action::PassPriority, &database);

        assert_eq!(state.log.len(), 1);
        assert_eq!(state.log[0].sequence, 1);
        assert!(matches!(
            state.log[0].event,
            GameEvent::StepChanged {
                turn: 1,
                active_player: PlayerId(0),
                step: Step::Upkeep,
            }
        ));
    }

    // ----- Multi-defender declare-blockers flow (issue #344) -----

    /// A 3-seat state parked at declare-blockers: seat 0 attacks seat 1 (with
    /// attacker A) and seat 2 (with attacker B), each defender has one untapped
    /// blocker, and priority sits with the first attacked player to declare.
    fn split_combat_at_declare_blockers() -> (
        GameState,
        PermanentId,
        PermanentId,
        PermanentId,
        PermanentId,
    ) {
        let mut state = GameState::new_multiplayer(3);
        state.turn = 2;
        state.step = Step::DeclareBlockers;
        state.active_player = PlayerId(0);
        state.attackers_declared = true;
        let atk_a = place_permanent(&mut state, fixture("onakke_ogre"), PlayerId(0), true, 0);
        let atk_b = place_permanent(&mut state, fixture("onakke_ogre"), PlayerId(0), true, 0);
        for (id, defender) in [(atk_a, PlayerId(1)), (atk_b, PlayerId(2))] {
            state
                .battlefield
                .iter_mut()
                .find(|p| p.id == id)
                .unwrap()
                .attacking = Some(defender);
        }
        let blk1 = place_permanent(&mut state, fixture("onakke_ogre"), PlayerId(1), false, 0);
        let blk2 = place_permanent(&mut state, fixture("onakke_ogre"), PlayerId(2), false, 0);
        state.priority = pending_blocker_declarer(&state).unwrap();
        (state, atk_a, atk_b, blk1, blk2)
    }

    #[test]
    fn issue_344_split_attacks_each_defender_declares_in_apnap_order() {
        // CR 509.1 + 101.4: each attacked player declares their own blockers, seat 1
        // then seat 2; combat is not "done" until both have declared.
        let db = db();
        let (state, atk_a, atk_b, blk1, blk2) = split_combat_at_declare_blockers();
        assert_eq!(state.priority, PlayerId(1), "seat 1 declares first (APNAP)");

        let after1 = apply_action(
            &state,
            &Action::DeclareBlockers {
                blocks: vec![Block {
                    blocker: blk1,
                    attacker: atk_a,
                }],
            },
            &db,
        );
        assert_eq!(find_perm(&after1, blk1).blocking, Some(atk_a));
        assert!(
            !after1.blockers_declared,
            "seat 2 still owes a declaration, so combat is not done"
        );
        assert_eq!(after1.priority, PlayerId(2), "seat 2 declares next (APNAP)");

        let after2 = apply_action(
            &after1,
            &Action::DeclareBlockers {
                blocks: vec![Block {
                    blocker: blk2,
                    attacker: atk_b,
                }],
            },
            &db,
        );
        assert_eq!(find_perm(&after2, blk2).blocking, Some(atk_b));
        assert!(
            after2.blockers_declared,
            "both attacked players declared — the step is done"
        );
        assert_eq!(
            after2.priority,
            PlayerId(0),
            "the priority round opens with the active player (CR 509.4)"
        );
    }

    #[test]
    fn issue_344_a_defender_cannot_block_an_attacker_attacking_someone_else() {
        // CR 509.1a: seat 1 may block only the attacker attacking seat 1. Assigning
        // its blocker to the attacker attacking seat 2 is illegal — a no-op.
        let db = db();
        let (state, _atk_a, atk_b, blk1, _blk2) = split_combat_at_declare_blockers();

        let rejected = apply_action(
            &state,
            &Action::DeclareBlockers {
                blocks: vec![Block {
                    blocker: blk1,
                    attacker: atk_b, // attacking seat 2, not seat 1
                }],
            },
            &db,
        );
        assert_eq!(
            rejected, state,
            "blocking an attacker attacking another player is rejected"
        );
    }

    #[test]
    fn issue_344_damage_is_computed_once_after_all_declarations_route_per_defender() {
        // After both defenders declare, passing the priority round advances to the
        // combat-damage step, where damage is computed once and routes per #341:
        // each attacker's block resolves against its own defender's blocker.
        let db = db();
        let (state, atk_a, _atk_b, blk1, _blk2) = split_combat_at_declare_blockers();

        // Seat 1 blocks attacker A; seat 2 declares no blockers (attacker B is
        // unblocked and will hit seat 2).
        let state = apply_action(
            &state,
            &Action::DeclareBlockers {
                blocks: vec![Block {
                    blocker: blk1,
                    attacker: atk_a,
                }],
            },
            &db,
        );
        let state = apply_action(&state, &Action::DeclareBlockers { blocks: Vec::new() }, &db);
        assert!(state.blockers_declared);

        // A full 3-seat priority round advances into combat damage.
        let mut state = state;
        for _ in 0..3 {
            state = apply_action(&state, &Action::PassPriority, &db);
        }
        // Attacker A (4/2) and its blocker (4/2) traded; seat 2 took 4 from the
        // unblocked attacker B.
        assert_eq!(state.players[2].life, 16, "unblocked attacker B hit seat 2");
        assert_eq!(
            state.players[1].life, 20,
            "seat 1 blocked its attacker, so took no damage"
        );
    }
}
