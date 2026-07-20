use super::*;
use crate::actions::{Attack, Block, DamageOrder};
use crate::card::Keyword;
use crate::combat::{
    blocked_attackers, combat_damage, combat_has_first_strike, defending_player, has_keyword,
    pending_blocker_declarer, CombatDamage, DamageStep,
};
use crate::id::{CardId, PermanentId};
use crate::state::LoggedPermanent;

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
pub(crate) fn deal_combat_damage(state: &mut GameState, db: &CardDatabase) {
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
pub(crate) fn apply_combat_batch(state: &mut GameState, batch: Vec<CombatDamage>) {
    for assignment in batch {
        match assignment {
            CombatDamage::ToPlayer {
                player,
                amount,
                source_commander,
            } => {
                // Damage to a player is life loss recorded as a `DamageDealt` event
                // (not a bare life change), so a client can report the hit.
                state.deal_damage_to_player(player, amount);
                // CR 903.10a: combat damage from a commander also accrues to the
                // per-designation tally that the 21-damage loss reads. Keyed to the
                // commander's owning player, so it survives the commander's zone
                // changes; non-combat damage never reaches this seam, so it never
                // counts.
                if let Some(commander) = source_commander {
                    state.add_commander_damage(commander, player, amount);
                }
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

/// End-of-combat turn-based action: remove every creature from combat (CR 511.3)
/// by clearing the attacking flag and blocking assignment on every permanent. The
/// per-turn declaration flags are reset when the next turn begins
/// ([`GameState::begin_next_turn`]), so a fresh combat starts clean.
pub(crate) fn remove_creatures_from_combat(state: &mut GameState) {
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
pub(crate) fn apply_declare_attackers(
    state: &mut GameState,
    attackers: &[Attack],
    db: &CardDatabase,
) {
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
pub(crate) fn apply_declare_blockers(state: &mut GameState, blocks: &[Block]) {
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
pub(crate) fn apply_order_combat_damage(state: &mut GameState, orders: &[DamageOrder]) {
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::apply::test_support::*;

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

    #[test]
    fn issue_399_double_strike_first_step_assigns_lethal_then_next_in_chosen_order_cr_510_1c() {
        // ACCEPTANCE 1 / CR 510.1c–d: a double striker blocked by TWO creatures under a
        // player-chosen order assigns lethal-then-next in the FIRST-STRIKE step. A 3/4
        // double striker ordered [boar_b, boar_a] over two 3/2 Boars puts the lethal 2
        // on the first-ordered Boar, then the leftover 1 on the next — battlefield order
        // (boar_a, boar_b) is overridden by the chosen order.
        let db = combat_db();
        let mut state = at_declare_attackers();
        let start_life = state.players[1].life;
        let striker = place_permanent(
            &mut state,
            id_in(&db, "test_twinsoldier"),
            PlayerId(0),
            false,
            0,
        ); // 3/4 double strike
        let boar_a = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(1), false, 0);
        let boar_b = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(1), false, 0);

        let blocks = vec![
            Block {
                blocker: boar_a,
                attacker: striker,
            },
            Block {
                blocker: boar_b,
                attacker: striker,
            },
        ];
        let orders = vec![DamageOrder {
            attacker: striker,
            blockers: vec![boar_b, boar_a], // reverse of battlefield order
        }];
        let ordered =
            ordered_before_damage(&state, vec![striker], blocks.clone(), orders.clone(), &db);

        // First-strike step: lethal (2) to the first-ordered Boar, remainder (1) to the
        // next — in the chosen order, not battlefield order (CR 510.1c–d).
        let blocked = blocked_attackers(&ordered);
        let first = combat_damage(&ordered, &db, DamageStep::FirstStrike, &blocked);
        assert_eq!(
            first,
            vec![
                CombatDamage::ToPermanent {
                    permanent: boar_b,
                    amount: 2,
                    deathtouch: false,
                },
                CombatDamage::ToPermanent {
                    permanent: boar_a,
                    amount: 1,
                    deathtouch: false,
                },
            ],
            "first-strike step honors the chosen order: lethal to boar_b, remainder to boar_a"
        );

        // End-to-end through the real pipeline: boar_b dies to first strike; the regular
        // step finishes boar_a (its 1 marked + a fresh 1) while the survivor's 3 marks
        // the 3/4 striker, which lives. No trample, so the regular excess hits nothing.
        let after = run_combat_ordered(&state, vec![striker], blocks, orders, &db);
        assert!(
            !alive(&after, boar_b),
            "first-ordered Boar died to first strike"
        );
        assert!(
            !alive(&after, boar_a),
            "second-ordered Boar died in the regular step"
        );
        assert!(alive(&after, striker), "the 3/4 double striker survives");
        assert_eq!(
            find_perm(&after, striker).damage,
            3,
            "only the surviving Boar struck back (CR 510.5)"
        );
        assert_eq!(
            after.players[1].life, start_life,
            "no trample: the regular-step excess is dealt nowhere (CR 702.19e)"
        );
    }

    #[test]
    fn issue_399_double_strike_regular_step_honors_order_over_survivors_only_cr_510_1c() {
        // ACCEPTANCE 2 / CR 510.1c–d across CR 702.4b steps: the regular step honors the
        // SAME chosen order against the SURVIVING blockers — a blocker killed in the
        // first-strike step receives NOTHING in the second step, and (no trample) excess
        // carries over nowhere. A 3/4 double striker ordered [o2, o3, o1] over three 1/3
        // Otters kills o2 in the first-strike step; the regular step then assigns to
        // o3 (next in the surviving order), leaving o1 untouched.
        let db = combat_db();
        let mut state = at_declare_attackers();
        let start_life = state.players[1].life;
        let striker = place_permanent(
            &mut state,
            id_in(&db, "test_twinsoldier"),
            PlayerId(0),
            false,
            0,
        ); // 3/4 double strike, power 3
        let o1 = place_permanent(&mut state, id_in(&db, "test_otter"), PlayerId(1), false, 0); // 1/3
        let o2 = place_permanent(&mut state, id_in(&db, "test_otter"), PlayerId(1), false, 0);
        let o3 = place_permanent(&mut state, id_in(&db, "test_otter"), PlayerId(1), false, 0);

        let blocks = vec![
            Block {
                blocker: o1,
                attacker: striker,
            },
            Block {
                blocker: o2,
                attacker: striker,
            },
            Block {
                blocker: o3,
                attacker: striker,
            },
        ];
        let orders = vec![DamageOrder {
            attacker: striker,
            blockers: vec![o2, o3, o1],
        }];
        let mut walk = ordered_before_damage(&state, vec![striker], blocks, orders, &db);
        let blocked = blocked_attackers(&walk);

        // First-strike step: all 3 power goes to the first-ordered Otter, which is
        // lethal (toughness 3) — o2 alone is struck.
        let first = combat_damage(&walk, &db, DamageStep::FirstStrike, &blocked);
        assert_eq!(
            first,
            vec![CombatDamage::ToPermanent {
                permanent: o2,
                amount: 3,
                deathtouch: false,
            }],
            "the first-strike step spends all lethal on the first-ordered survivor"
        );
        apply_combat_batch(&mut walk, first);
        run_state_based_actions(&mut walk, &db); // CR 510.5: SBAs between the two steps
        assert!(!alive(&walk, o2), "o2 died in the first-strike step");

        // Regular step: the SAME order is honored over the SURVIVORS [o3, o1] — o3 (next)
        // takes the lethal 3, o1 gets nothing, and the dead o2 receives nothing.
        let regular = combat_damage(&walk, &db, DamageStep::Regular, &blocked);
        assert!(
            !regular.iter().any(|d| matches!(
                d,
                CombatDamage::ToPermanent { permanent, .. } if *permanent == o2
            )),
            "a blocker killed in the first-strike step receives nothing in the regular step"
        );
        assert!(
            regular.contains(&CombatDamage::ToPermanent {
                permanent: o3,
                amount: 3,
                deathtouch: false,
            }),
            "the regular step assigns lethal to o3, the next survivor in the chosen order"
        );
        assert!(
            !regular.iter().any(|d| matches!(
                d,
                CombatDamage::ToPermanent { permanent, .. } if *permanent == o1
            )),
            "o1, last in the order, receives no attacker damage"
        );
        apply_combat_batch(&mut walk, regular);
        run_state_based_actions(&mut walk, &db);

        assert!(
            !alive(&walk, o3),
            "o3 took its lethal 3 in the regular step"
        );
        assert!(alive(&walk, o1), "o1 survived — it was never assigned to");
        assert_eq!(
            find_perm(&walk, o1).damage,
            0,
            "the untouched survivor has no marked damage"
        );
        assert!(
            alive(&walk, striker),
            "the 3/4 striker survives o3+o1's 2 back"
        );
        assert_eq!(
            walk.players[1].life, start_life,
            "no trample: nothing carries over to the defending player"
        );

        // The real pipeline reaches the same outcome end-to-end.
        let after = run_combat_ordered(
            &state,
            vec![striker],
            vec![
                Block {
                    blocker: o1,
                    attacker: striker,
                },
                Block {
                    blocker: o2,
                    attacker: striker,
                },
                Block {
                    blocker: o3,
                    attacker: striker,
                },
            ],
            vec![DamageOrder {
                attacker: striker,
                blockers: vec![o2, o3, o1],
            }],
            &db,
        );
        assert!(!alive(&after, o2) && !alive(&after, o3));
        assert!(alive(&after, o1) && alive(&after, striker));
        assert_eq!(after.players[1].life, start_life);
    }

    #[test]
    fn issue_399_double_strike_trample_carries_over_in_both_steps_cr_702_19e() {
        // ACCEPTANCE 3 / CR 702.4b + 702.19e: a TRAMPLING double striker blocked by two
        // creatures carries excess to the defending player in BOTH steps. A 5/5 double
        // strike + trample ordered [boar_b, boar_a] over two 3/2 Boars assigns lethal 2
        // to each (in the chosen order) and tramples 1 over in the first-strike step;
        // both Boars are dead by the regular step, so its full 5 tramples over. The
        // defender loses 1 + 5 = 6.
        let db = combat_db();
        let mut state = at_declare_attackers();
        let start_life = state.players[1].life;
        let striker = place_permanent(
            &mut state,
            id_in(&db, "test_twinjugg"),
            PlayerId(0),
            false,
            0,
        ); // 5/5 double strike + trample
        let boar_a = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(1), false, 0);
        let boar_b = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(1), false, 0);

        let blocks = vec![
            Block {
                blocker: boar_a,
                attacker: striker,
            },
            Block {
                blocker: boar_b,
                attacker: striker,
            },
        ];
        let orders = vec![DamageOrder {
            attacker: striker,
            blockers: vec![boar_b, boar_a],
        }];
        let mut walk =
            ordered_before_damage(&state, vec![striker], blocks.clone(), orders.clone(), &db);
        let blocked = blocked_attackers(&walk);

        // First-strike step: lethal 2 to each Boar in the chosen order, then trample 1
        // to the defending player (CR 702.19e).
        let first = combat_damage(&walk, &db, DamageStep::FirstStrike, &blocked);
        assert_eq!(
            first,
            vec![
                CombatDamage::ToPermanent {
                    permanent: boar_b,
                    amount: 2,
                    deathtouch: false,
                },
                CombatDamage::ToPermanent {
                    permanent: boar_a,
                    amount: 2,
                    deathtouch: false,
                },
                CombatDamage::ToPlayer {
                    player: PlayerId(1),
                    amount: 1,
                    source_commander: None,
                },
            ],
            "first-strike step: lethal to each blocker in order, then 1 tramples over"
        );
        apply_combat_batch(&mut walk, first);
        run_state_based_actions(&mut walk, &db);
        assert!(
            !alive(&walk, boar_a) && !alive(&walk, boar_b),
            "both Boars took lethal first strike"
        );

        // Regular step: no blockers survive, so the whole 5 tramples over (CR 702.19e).
        let regular = combat_damage(&walk, &db, DamageStep::Regular, &blocked);
        assert_eq!(
            regular,
            vec![CombatDamage::ToPlayer {
                player: PlayerId(1),
                amount: 5,
                source_commander: None,
            }],
            "regular step: with every blocker dead, the full power tramples over"
        );

        // End-to-end through the real pipeline: the defender loses 1 + 5 = 6.
        let after = run_combat_ordered(&state, vec![striker], blocks, orders, &db);
        assert!(alive(&after, striker), "the 5/5 trampler is untouched");
        assert_eq!(
            after.players[1].life,
            start_life - 6,
            "trample carries over in BOTH steps: 1 then 5 (CR 702.4b/702.19e)"
        );
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
