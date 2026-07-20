use super::*;
use crate::combat::priority_after_step_change;
use crate::phase::Step;
use crate::player::MAX_HAND_SIZE;
use crate::resolve::resolve_stack_object;
use crate::state::Duration;

/// Resolve a pass of priority. Priority moves to the next seat; once every
/// player has passed in unbroken succession, the top of the stack resolves (if
/// any), otherwise the turn structure advances ([`GameState::advance`]); either
/// way the new active player receives priority.
pub(crate) fn apply_pass_priority(state: &mut GameState, db: &CardDatabase) {
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
pub(crate) fn advance_through_turn_based_steps(state: &mut GameState, db: &CardDatabase) {
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
pub(crate) fn active_player_over_hand_size(state: &GameState) -> bool {
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::apply::test_support::*;

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
}
