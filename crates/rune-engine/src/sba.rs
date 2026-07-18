//! State-based actions: the checks the engine applies to a fixed point after
//! every action (CR 704). [`crate::apply_action`] calls
//! [`run_state_based_actions`] as a pipeline stage.

use crate::ability::Target;
use crate::characteristics::characteristics;
use crate::id::{PermanentId, PlayerId};
use crate::player::LossReason;
use crate::resolve::target_is_legal;
use crate::state::{EffectAffects, GameEvent, GameState, Permanent};
use crate::CardDatabase;

/// Run state-based actions to a fixed point: keep applying them until a full
/// pass changes nothing (CR 704.3). Pure over the owned state. Takes `db` for
/// the current-toughness read the lethal-damage check needs (CR 704.5g).
///
/// Modeled today:
/// - **CR 704.5a** — a player at 0 or less life loses the game (combat life loss
///   flows in here).
/// - **CR 704.5c** — a player who attempted to draw from an empty library since
///   the last check loses the game (decking); the attempt is flagged on the
///   player by [`crate::Player::draw`] and consumed here.
/// - **CR 704.5f** — a creature with toughness 0 or less is put into its owner's
///   graveyard. Unlike CR 704.5g this is not "destruction" (regeneration can't
///   save it), but it routes through the same leaves-battlefield seam. A `-X/-X`
///   Aura or `-1/-1` counters that drop current toughness to 0 trigger it.
/// - **CR 704.5g** — a creature with lethal marked damage (damage ≥ its
///   toughness, toughness > 0) is destroyed and put into its owner's graveyard.
/// - **CR 704.5m** — an Aura attached to an illegal object, or whose host has left
///   the battlefield, is put into its owner's graveyard.
/// - **CR 704.5n** — an Aura attached to nothing is put into its owner's graveyard.
/// - **CR 704.5h** — a creature dealt any nonzero damage this turn by a source
///   with deathtouch is destroyed, regardless of whether that damage is lethal by
///   toughness. The struck creatures are recorded in
///   [`GameState::deathtouch_struck`](crate::GameState::deathtouch_struck) when
///   combat damage is applied and consumed (drained) here.
///
/// These run in the same loop so a chain settles in one call: e.g. a creature
/// dying does not itself change a life total today, but keeping the checks in one
/// fixed-point pass is what CR 704.3 requires as more actions land. Consuming a
/// loss into the terminal [`GameResult`](crate::GameResult) — deciding the winner
/// once one player remains (CR 104.2a) — is a pure derivation done on read, not
/// stored here.
pub(crate) fn run_state_based_actions(state: &mut GameState, db: &CardDatabase) {
    loop {
        let mut changed = false;
        // Losing conditions, unified (CR 704.5). Each marks the player as having
        // lost and records why, exactly once.
        for player in &mut state.players {
            // CR 704.5a: a player at 0 or less life loses.
            if player.life <= 0 && !player.has_lost {
                player.has_lost = true;
                player.loss_reason = Some(LossReason::ZeroLife);
                changed = true;
            }
            // CR 704.5c: a player who attempted to draw from an empty library
            // loses. Consume the flag so the pass reaches a fixed point.
            if player.attempted_draw_from_empty {
                player.attempted_draw_from_empty = false;
                if !player.has_lost {
                    player.has_lost = true;
                    player.loss_reason = Some(LossReason::DrewFromEmptyLibrary);
                }
                changed = true;
            }
        }
        // CR 800.4a: a player who lost while two or more players remain *leaves the
        // game* — the game continues without them. Their objects are removed and the
        // departure is logged, exactly once (guarded by `left_game`). In a
        // two-player game a loss instead ends the game (CR 104.2a) before this
        // applies, so `left_game` stays false and nothing is cleaned up here —
        // preserving the two-player behavior unchanged. Done before the death checks
        // below so those judge the battlefield the eliminated player has already
        // left, and so cleanup itself cannot re-trigger from the departing objects.
        if state.living_player_count() >= 2 {
            let leaving: Vec<(PlayerId, LossReason)> = state
                .players
                .iter()
                .enumerate()
                .filter(|(_, p)| p.has_lost && !p.left_game)
                .map(|(seat, p)| {
                    (
                        PlayerId(seat),
                        p.loss_reason.unwrap_or(LossReason::ZeroLife),
                    )
                })
                .collect();
            for (seat, reason) in leaving {
                state.record_event(GameEvent::PlayerEliminated {
                    player: seat,
                    reason,
                });
                state.remove_player_from_game(seat);
                if let Some(player) = state.players.get_mut(seat.0) {
                    player.left_game = true;
                }
                changed = true;
            }
        }
        // CR 704.5h: a creature dealt damage by a deathtouch source is destroyed.
        // The set is recorded when combat damage is applied (see
        // `apply.rs :: deal_combat_damage`); draining it here consumes the flag so
        // the pass reaches a fixed point, mirroring the empty-library-draw flag.
        let struck = std::mem::take(&mut state.deathtouch_struck);
        if !struck.is_empty() {
            changed = true;
        }
        // CR 704.5f/704.5g/704.5h: put into the graveyard every creature with 0-or-
        // less toughness (CR 704.5f), with lethal marked damage (CR 704.5g), or
        // flagged as struck by deathtouch (CR 704.5h). Collected before mutating so
        // the whole set is judged against one snapshot (the checks are simultaneous,
        // CR 704.3), then each is moved to its owner's graveyard.
        let doomed: Vec<PermanentId> = state
            .battlefield
            .iter()
            .filter(|perm| {
                has_zero_toughness(perm, state, db)
                    || has_lethal_damage(perm, state, db)
                    || struck.contains(&perm.id)
            })
            .map(|perm| perm.id)
            .collect();
        for id in doomed {
            // Route through the one creature-death seam (CR 700.4) so a lethal-damage
            // / deathtouch / zero-toughness death and a `Destroy` death are the same
            // observable zone change for the dies trigger (CR 603.6c) and log a single
            // `permanent_died` — every id in `doomed` is a creature (the checks read
            // toughness or a combat strike), so each is a genuine death.
            if state.destroy_permanent(id, db) {
                changed = true;
            }
        }
        // CR 704.5m/704.5n: put into the graveyard every Aura that is illegally
        // attached — attached to nothing (CR 704.5n) or to an object it can no
        // longer legally enchant, including a host that has just left the
        // battlefield above (CR 704.5m). Judged after the creature deaths so a host
        // dying this same check orphans its Aura; the outer loop re-runs to a fixed
        // point regardless. The Aura's derived P/T contribution disappears with it —
        // nothing keyed in `static_effects` to prune (see `characteristics.rs`).
        let doomed_auras: Vec<PermanentId> = state
            .battlefield
            .iter()
            .filter(|perm| aura_is_illegally_attached(perm, state, db))
            .map(|perm| perm.id)
            .collect();
        for id in doomed_auras {
            // An Aura leaving for the graveyard is a zone change, not a death (CR
            // 700.4 — only creatures "die"), so it uses the bare zone move and logs
            // no `permanent_died`.
            if state.move_permanent_to_graveyard(id).is_some() {
                changed = true;
            }
        }
        // Prune any continuous effect keyed to a specific permanent that has now
        // left the battlefield (destroyed above, or removed by another effect this
        // action). A permanent-specific modifier — a pump — has nothing to apply to
        // once its permanent is gone, and a `PermanentId` is never reused, so the
        // effect can never match again; removing it keeps a modifier from
        // outliving its permanent (no dangling static effect). Anthem-style
        // selectors are left alone — they track a live set, not one object.
        let before = state.static_effects.len();
        state.static_effects.retain(|effect| match effect.affects {
            EffectAffects::SpecificPermanent(id) => {
                state.battlefield.iter().any(|perm| perm.id == id)
            }
            EffectAffects::CreaturesControlledBy(_) => true,
        });
        if state.static_effects.len() != before {
            changed = true;
        }
        if !changed {
            break;
        }
    }
}

/// Whether `perm` has lethal marked damage (CR 704.5g): it is a creature with
/// toughness greater than 0 whose marked damage is at least that toughness.
/// Current toughness is read through [`characteristics`], so counters and
/// anthems are folded in. A non-creature (no toughness) is never lethal here.
fn has_lethal_damage(perm: &Permanent, state: &GameState, db: &CardDatabase) -> bool {
    match characteristics(state, perm.id, db).toughness {
        Some(toughness) if toughness > 0 => {
            perm.damage >= u32::try_from(toughness).unwrap_or(u32::MAX)
        }
        _ => false,
    }
}

/// Whether `perm` is a creature whose *current* toughness is 0 or less (CR 704.5f).
/// Current toughness is read through [`characteristics`], so `-1/-1` counters and a
/// `-X/-X` Aura are folded in — a Boar reduced to 0 toughness by a `-2/-2` Aura is
/// put into its graveyard even with no marked damage. A non-creature (no toughness)
/// never qualifies.
fn has_zero_toughness(perm: &Permanent, state: &GameState, db: &CardDatabase) -> bool {
    matches!(characteristics(state, perm.id, db).toughness, Some(t) if t <= 0)
}

/// Whether `perm` is an Aura that is now illegally attached and so must go to its
/// owner's graveyard (CR 704.5m/n). `false` for a non-Aura permanent.
///
/// An Aura is illegal when it is attached to nothing (CR 704.5n) or when its host is
/// no longer a legal object for its enchant restriction (CR 704.5m) — which
/// [`target_is_legal`] reports `false` for once the host has left the battlefield or
/// stopped matching (e.g. a creature Aura on something no longer a creature).
fn aura_is_illegally_attached(perm: &Permanent, state: &GameState, db: &CardDatabase) -> bool {
    let Some(grant) = db.card(perm.card).and_then(|card| card.aura) else {
        return false;
    };
    match perm.attached_to {
        None => true,
        Some(host) => !target_is_legal(grant.enchant, Target::Permanent(host), state, db),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::actions::Action;
    use crate::apply_action;
    use crate::fixtures::fixture;
    use crate::id::{CardId, PlayerId};
    use crate::state::{CounterKind, Permanent};
    use crate::CardDatabase;

    /// The bundled card database, for tests that need oracle data.
    fn db() -> CardDatabase {
        CardDatabase::bundled().unwrap()
    }

    /// Place a permanent of `card` under `controller` with `damage` marked; return
    /// its fresh id.
    fn place(
        state: &mut GameState,
        card: CardId,
        controller: PlayerId,
        damage: u32,
    ) -> PermanentId {
        let inst = state.new_instance(card);
        let id = PermanentId(state.mint_id());
        state.battlefield.push(Permanent {
            id,
            instance: inst.id,
            card,
            controller,
            tapped: false,
            entered_turn: 0,
            attacking: None,
            blocking: None,
            damage,
            counters: Default::default(),
            attached_to: None,
        });
        id
    }

    #[test]
    fn cr_704_5g_creature_with_lethal_marked_damage_is_destroyed() {
        // CR 704.5g: a creature with damage marked greater than or equal to its
        // toughness is destroyed and put into its owner's graveyard. Thornback
        // Boar is a 3/2; two marked damage is lethal.
        let db = db();
        let mut state = GameState::new_two_player();
        let boar = place(&mut state, fixture("thornback_boar"), PlayerId(0), 2);

        run_state_based_actions(&mut state, &db);

        assert!(
            !state.battlefield.iter().any(|p| p.id == boar),
            "a creature with lethal marked damage leaves the battlefield (CR 704.5g)"
        );
        assert_eq!(
            state.players[0].graveyard.len(),
            1,
            "the destroyed creature is in its owner's graveyard"
        );
    }

    #[test]
    fn cr_704_5g_creature_below_lethal_survives() {
        // CR 704.5g: damage below toughness is not lethal. A 3/2 Boar with one
        // marked damage stays on the battlefield.
        let db = db();
        let mut state = GameState::new_two_player();
        let boar = place(&mut state, fixture("thornback_boar"), PlayerId(0), 1);

        run_state_based_actions(&mut state, &db);

        assert!(state.battlefield.iter().any(|p| p.id == boar));
        assert!(state.players[0].graveyard.is_empty());
    }

    #[test]
    fn cr_704_5g_lethality_reads_current_toughness_with_counters() {
        // CR 704.5g reads *current* toughness (CR 613 layer 7c). A +1/+1 counter
        // makes the 3/2 Boar a 3/3, so two damage is no longer lethal — but three
        // is. This proves the SBA folds counters in, not the printed toughness.
        let db = db();
        let mut state = GameState::new_two_player();
        let boar = place(&mut state, fixture("thornback_boar"), PlayerId(0), 2);
        if let Some(perm) = state.battlefield.iter_mut().find(|p| p.id == boar) {
            perm.counters.insert(CounterKind::PlusOnePlusOne, 1);
        }

        run_state_based_actions(&mut state, &db);
        assert!(
            state.battlefield.iter().any(|p| p.id == boar),
            "2 damage is not lethal to a 3/3 (printed 3/2 + counter)"
        );

        if let Some(perm) = state.battlefield.iter_mut().find(|p| p.id == boar) {
            perm.damage = 3;
        }
        run_state_based_actions(&mut state, &db);
        assert!(
            !state.battlefield.iter().any(|p| p.id == boar),
            "3 damage is lethal to a 3/3 (CR 704.5g)"
        );
    }

    #[test]
    fn cr_704_5h_deathtouch_struck_creature_is_destroyed_below_lethal_damage() {
        // CR 704.5h: a creature flagged as struck by a deathtouch source is
        // destroyed even though its 1 marked damage is far below its toughness. The
        // Basilisk (4/5) survives 1 marked damage by CR 704.5g but not the flag.
        let db = db();
        let mut state = GameState::new_two_player();
        let basilisk = place(&mut state, fixture("stonehide_basilisk"), PlayerId(0), 1); // 1 marked damage
        state.deathtouch_struck.push(basilisk);

        run_state_based_actions(&mut state, &db);

        assert!(
            !state.battlefield.iter().any(|p| p.id == basilisk),
            "a deathtouch-struck creature is destroyed (CR 704.5h)"
        );
        assert_eq!(state.players[0].graveyard.len(), 1);
        assert!(
            state.deathtouch_struck.is_empty(),
            "the deathtouch flag is consumed so the loop settles"
        );
    }

    #[test]
    fn cr_704_5h_stale_deathtouch_flag_settles_without_error() {
        // A struck id whose creature is already gone is drained harmlessly and the
        // loop still reaches a fixed point (no infinite loop, no panic).
        let db = db();
        let mut state = GameState::new_two_player();
        state.deathtouch_struck.push(PermanentId(999));

        run_state_based_actions(&mut state, &db);

        assert!(state.deathtouch_struck.is_empty());
        assert!(state.battlefield.is_empty());
    }

    #[test]
    fn state_based_actions_mark_a_player_at_zero_life_as_lost() {
        let mut state = GameState::new_two_player();
        state.players[1].life = 0;
        let after = apply_action(&state, &Action::PassPriority, &db());
        assert!(after.players[1].has_lost);
        // CR 704.5a: the loss records its reason so the terminal result can name it.
        assert_eq!(after.players[1].loss_reason, Some(LossReason::ZeroLife));
        assert!(!after.players[0].has_lost);
    }

    #[test]
    fn cr_704_5c_attempted_draw_from_empty_library_loses() {
        // CR 704.5c: an attempted draw from an empty library, flagged on the
        // player, is consumed by the SBA loop into a loss and the flag is cleared
        // so the pass reaches a fixed point.
        let db = db();
        let mut state = GameState::new_two_player();
        state.players[0].attempted_draw_from_empty = true;

        run_state_based_actions(&mut state, &db);

        assert!(
            state.players[0].has_lost,
            "decking loses the game (CR 704.5c)"
        );
        assert_eq!(
            state.players[0].loss_reason,
            Some(LossReason::DrewFromEmptyLibrary)
        );
        assert!(
            !state.players[0].attempted_draw_from_empty,
            "the attempt flag is consumed so the loop settles"
        );
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

    // ----- Aura state-based actions (issue #152) -----
    //
    // Fixtures: 29 Ironbark Aegis (+2/+2 Aura), 30 Witherbrand Curse (-2/-2 Aura),
    // 1 Thornback Boar (3/2), 6 Verdant Scout (1/1).

    /// Place an Aura of `card` attached to `host` under player 0's control, and
    /// return its fresh id.
    fn place_aura(state: &mut GameState, card: CardId, host: PermanentId) -> PermanentId {
        let id = place(state, card, PlayerId(0), 0);
        if let Some(aura) = state.battlefield.iter_mut().find(|p| p.id == id) {
            aura.attached_to = Some(host);
        }
        id
    }

    #[test]
    fn cr_704_5m_aura_follows_its_host_to_the_graveyard_in_one_fixed_point() {
        // CR 704.5m: when an Aura's host leaves the battlefield the Aura is put into
        // its owner's graveyard. Here the host (a 3/2 Boar) dies to lethal marked
        // damage; the same state-based-actions fixed point moves the host and then
        // its now-orphaned Aura, and the Aura's +2/+2 modifier is gone.
        use crate::characteristics::characteristics;
        let db = db();
        let mut state = GameState::new_two_player();
        let host = place(&mut state, fixture("thornback_boar"), PlayerId(0), 4); // 3/2 with 4 damage
        let aura = place_aura(&mut state, fixture("ironbark_aegis"), host); // +2/+2

        // Before SBAs the Aura buffs the host to a 5/4; 4 marked damage is lethal.
        assert_eq!(characteristics(&state, host, &db).toughness, Some(4));

        run_state_based_actions(&mut state, &db);

        assert!(
            !state.battlefield.iter().any(|p| p.id == host),
            "the host died to lethal damage (CR 704.5g)"
        );
        assert!(
            !state.battlefield.iter().any(|p| p.id == aura),
            "the Aura followed its host to the graveyard (CR 704.5m)"
        );
        // Both the host and the Aura are now in the graveyard.
        assert_eq!(state.players[0].graveyard.len(), 2);
    }

    #[test]
    fn cr_704_5n_aura_attached_to_nothing_is_put_into_the_graveyard() {
        // CR 704.5n: an Aura that is not attached to anything (its `attached_to` is
        // `None`) is put into its owner's graveyard.
        let db = db();
        let mut state = GameState::new_two_player();
        let aura = place(&mut state, fixture("ironbark_aegis"), PlayerId(0), 0); // unattached

        run_state_based_actions(&mut state, &db);

        assert!(
            !state.battlefield.iter().any(|p| p.id == aura),
            "an unattached Aura goes to the graveyard (CR 704.5n)"
        );
        assert_eq!(state.players[0].graveyard.len(), 1);
    }

    #[test]
    fn cr_704_5f_minus_x_aura_reduces_toughness_to_zero_and_kills_the_host() {
        // CR 704.5f (with CR 613.7c and CR 704.5m): a -2/-2 Aura on a 3/2 Boar drops
        // its current toughness to 0, so the creature is put into the graveyard as a
        // state-based action (CR 704.5f — no marked damage, no "destruction"), and
        // its now-orphaned Aura follows (CR 704.5m) in the same fixed point.
        use crate::characteristics::characteristics;
        let db = db();
        let mut state = GameState::new_two_player();
        let host = place(&mut state, fixture("thornback_boar"), PlayerId(0), 0); // 3/2, no damage
        let aura = place_aura(&mut state, fixture("witherbrand_curse"), host); // -2/-2

        // Current toughness is 2 + (-2) = 0 before the SBA runs.
        assert_eq!(characteristics(&state, host, &db).toughness, Some(0));

        run_state_based_actions(&mut state, &db);

        assert!(
            !state.battlefield.iter().any(|p| p.id == host),
            "a creature at 0 toughness is put into the graveyard (CR 704.5f)"
        );
        assert!(
            !state.battlefield.iter().any(|p| p.id == aura),
            "the Aura on the dead host follows it (CR 704.5m)"
        );
        assert_eq!(state.players[0].graveyard.len(), 2);
    }

    #[test]
    fn issue_152_aura_on_a_live_host_is_not_a_state_based_action() {
        // A legally-attached Aura on a healthy creature is left alone: neither the
        // host nor the Aura is a state-based action, and the buff persists.
        use crate::characteristics::characteristics;
        let db = db();
        let mut state = GameState::new_two_player();
        let host = place(&mut state, fixture("verdant_scout"), PlayerId(0), 0); // 1/1 Scout
        let aura = place_aura(&mut state, fixture("ironbark_aegis"), host); // +2/+2

        run_state_based_actions(&mut state, &db);

        assert!(state.battlefield.iter().any(|p| p.id == host));
        assert!(state.battlefield.iter().any(|p| p.id == aura));
        assert_eq!(characteristics(&state, host, &db).power, Some(3));
    }

    // ----- Elimination: leaving the game cleanly (issue #342) -----

    fn eliminated(event: &GameEvent) -> Option<PlayerId> {
        match event {
            GameEvent::PlayerEliminated { player, .. } => Some(*player),
            _ => None,
        }
    }

    #[test]
    fn issue_342_lost_player_leaves_the_game_while_others_remain_cr_800_4a() {
        // CR 800.4a: in a game of three, a player at 0 life leaves — their objects
        // are removed and the game continues with no terminal result.
        let db = db();
        let mut state = GameState::new_multiplayer(3);
        let perm = place(&mut state, fixture("thornback_boar"), PlayerId(1), 0);
        state.players[1].hand = vec![state.new_instance(fixture("verdant_scout"))];
        state.players[1].library = vec![state.new_instance(fixture("forest"))];
        state.players[1].life = 0;

        run_state_based_actions(&mut state, &db);

        assert!(state.players[1].has_lost && state.players[1].left_game);
        assert!(
            !state.battlefield.iter().any(|p| p.id == perm),
            "the eliminated player's permanents leave the game (CR 800.4a)"
        );
        assert!(
            state.players[1].hand.is_empty() && state.players[1].library.is_empty(),
            "their hidden zones are no longer part of the game"
        );
        assert!(
            state.result().is_none(),
            "the game continues — two players remain"
        );
        assert_eq!(
            state
                .log
                .iter()
                .filter_map(|e| eliminated(&e.event))
                .count(),
            1,
            "the elimination is logged exactly once"
        );
        assert_eq!(
            state.log.iter().find_map(|e| eliminated(&e.event)),
            Some(PlayerId(1))
        );

        // Idempotent: a second pass neither re-logs nor changes anything.
        let sequence_before = state.next_log_sequence;
        run_state_based_actions(&mut state, &db);
        assert_eq!(state.next_log_sequence, sequence_before);
    }

    #[test]
    fn issue_342_eliminated_players_aura_on_a_survivor_is_cleaned_up() {
        // CR 800.4a + 704.5m: an eliminated player's Aura attached to a survivor's
        // creature leaves with its owner; a survivor's Aura orphaned by the departed
        // player's creature leaving goes to its owner's graveyard.
        let db = db();
        let mut state = GameState::new_multiplayer(3);
        // Survivor (seat 0) creature enchanted by the doomed player's (seat 1) Aura.
        let survivor_creature = place(&mut state, fixture("verdant_scout"), PlayerId(0), 0);
        let doomed_aura = place(&mut state, fixture("ironbark_aegis"), PlayerId(1), 0);
        if let Some(a) = state.battlefield.iter_mut().find(|p| p.id == doomed_aura) {
            a.attached_to = Some(survivor_creature);
        }
        state.players[1].life = 0;

        run_state_based_actions(&mut state, &db);

        assert!(
            !state.battlefield.iter().any(|p| p.id == doomed_aura),
            "the eliminated player's Aura leaves the game"
        );
        assert!(
            state.battlefield.iter().any(|p| p.id == survivor_creature),
            "the survivor's creature stays"
        );
    }

    #[test]
    fn issue_342_two_player_loss_ends_the_game_with_no_cleanup() {
        // Two-player behavior is unchanged: a loss ends the game immediately
        // (CR 104.2a); no CR 800.4a leave-the-game cleanup runs and `left_game`
        // stays false.
        let db = db();
        let mut state = GameState::new_two_player();
        let perm = place(&mut state, fixture("thornback_boar"), PlayerId(1), 0);
        state.players[1].life = 0;

        run_state_based_actions(&mut state, &db);

        assert!(state.players[1].has_lost);
        assert!(
            !state.players[1].left_game,
            "no one leaves a two-player game — it just ends"
        );
        assert!(
            state.battlefield.iter().any(|p| p.id == perm),
            "the loser's permanents are untouched (the game is simply over)"
        );
        assert!(state.result().is_some(), "the game is over");
        assert!(
            !state.log.iter().any(|e| eliminated(&e.event).is_some()),
            "no elimination event in a two-player game"
        );
    }

    #[test]
    fn issue_342_last_player_standing_wins_with_all_losers_recorded() {
        // Two of three players lost: the survivor wins and both losers appear in
        // GameResult.losers (existing shape, no contract change).
        let db = db();
        let mut state = GameState::new_multiplayer(3);
        state.players[1].life = 0;
        run_state_based_actions(&mut state, &db); // seat 1 leaves; game continues
        assert!(state.result().is_none());
        state.players[2].life = -1;
        run_state_based_actions(&mut state, &db); // seat 2 lost; now over

        let result = state.result().unwrap();
        assert_eq!(result.winner, Some(PlayerId(0)));
        assert!(result.losers.contains(&PlayerId(1)) && result.losers.contains(&PlayerId(2)));
    }

    #[test]
    fn issue_342_eliminated_defender_removes_its_attackers_from_combat() {
        // A player eliminated mid-combat is removed from combat: an attacker
        // declared against them is no longer attacking, so it deals no player damage.
        let db = db();
        let mut state = GameState::new_multiplayer(3);
        // Seat 0's attacker is attacking seat 1, who is about to be eliminated.
        let attacker = place(&mut state, fixture("thornback_boar"), PlayerId(0), 0);
        if let Some(a) = state.battlefield.iter_mut().find(|p| p.id == attacker) {
            a.attacking = Some(PlayerId(1));
        }
        state.players[1].life = 0;

        run_state_based_actions(&mut state, &db);

        let attacker_state = state.battlefield.iter().find(|p| p.id == attacker).unwrap();
        assert_eq!(
            attacker_state.attacking, None,
            "an attacker on the departed player is removed from combat (CR 800.4a)"
        );
    }
}
