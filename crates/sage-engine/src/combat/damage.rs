use crate::card::Keyword;
use crate::id::{PermanentId, PlayerId};
use crate::state::GameState;
use crate::CardDatabase;

use super::helpers::{
    combat_power, deals_in_step, has_keyword, lethal_needed, push_permanent_damage,
    push_player_damage,
};

/// A single combat-damage assignment computed for a combat-damage step
/// (CR 510.1c). Kept as data to apply *after* every assignment is computed, so
/// all combat damage in the step is dealt at once (simultaneously, CR 510.2) — no
/// creature leaves combat partway through the batch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CombatDamage {
    /// Combat damage a creature deals to a player: an unblocked attacker, or the
    /// trample excess of a blocked one, striking the defending player (CR 510.1c,
    /// CR 702.19e).
    ToPlayer {
        /// The player the damage is dealt to.
        player: PlayerId,
        /// How much damage.
        amount: u32,
        /// The commander designation of the source, if the striking creature is a
        /// commander — its owning [`PlayerId`], the stable tally key (CR 903.10a).
        /// `None` for an ordinary source. When set, the batch application adds this
        /// hit to the CR 903.10a commander-damage tally
        /// ([`GameState::add_commander_damage`]); a bare life change alone would
        /// lose the "which commander dealt it" fact the 21-damage loss needs.
        source_commander: Option<PlayerId>,
    },
    /// Combat damage a creature deals to another creature: an attacker to its
    /// blockers, or a blocker to the attacker it blocks (CR 510.1c). Marked on
    /// the permanent (CR 120.3).
    ToPermanent {
        /// The permanent the damage is marked on.
        permanent: PermanentId,
        /// How much damage.
        amount: u32,
        /// Whether the source has deathtouch (CR 702.2b): any nonzero such damage
        /// is lethal, so the recipient is flagged for the CR 704.5h state-based
        /// action when the batch is applied.
        deathtouch: bool,
    },
    /// Life a lifelink source's controller gains, dealt in the *same* batch as the
    /// damage that caused it so the gain is simultaneous with the damage event
    /// (CR 702.15e).
    GainLife {
        /// The player who gains the life (the damage source's controller).
        player: PlayerId,
        /// How much life is gained (equal to the damage dealt).
        amount: u32,
    },
}

/// Which combat-damage step is being computed (CR 510.5).
///
/// Most creatures deal in exactly one step: first-strikers in the first step,
/// everyone else in the second. A creature with double strike (CR 702.4b) deals in
/// *both* the first-strike and the regular step — the one creature [`deals_in_step`]
/// admits to both.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DamageStep {
    /// The single combat-damage step of an ordinary combat (no first strike is
    /// present): every creature in combat deals (CR 510.1).
    Only,
    /// The first-strike combat-damage step: only creatures with first strike deal
    /// (CR 510.5).
    FirstStrike,
    /// The regular combat-damage step that follows a first-strike step: creatures
    /// without first strike deal (CR 510.5).
    Regular,
}

/// Whether any creature currently in combat (attacking or blocking) has first
/// strike **or double strike**, so combat needs the two-step damage sequence
/// (CR 510.5). A double striker deals in the first-strike step too (CR 702.4b), so
/// its mere presence splits combat in two even when no creature has plain first
/// strike. When none qualify, a single [`DamageStep::Only`] step suffices.
#[must_use]
pub(crate) fn combat_has_first_strike(state: &GameState, db: &CardDatabase) -> bool {
    state.battlefield.iter().any(|p| {
        (p.attacking.is_some() || p.blocking.is_some())
            && (has_keyword(state, p, Keyword::FirstStrike, db)
                || has_keyword(state, p, Keyword::DoubleStrike, db))
    })
}

/// The attackers that are *blocked* this combat — each has at least one creature
/// assigned to block it (CR 509.1h) — captured before any combat damage is dealt.
///
/// A creature stays blocked for the rest of combat even if its blockers later
/// leave (e.g. a first-struck blocker dies before the regular step): a blocked
/// attacker deals no damage to the defending player unless it has trample. This
/// snapshot is what the regular damage step consults so a blocked-but-now-
/// blockerless attacker is not mistaken for an unblocked one.
#[must_use]
pub(crate) fn blocked_attackers(state: &GameState) -> Vec<PermanentId> {
    state
        .battlefield
        .iter()
        .filter(|p| {
            p.attacking.is_some() && state.battlefield.iter().any(|b| b.blocking == Some(p.id))
        })
        .map(|p| p.id)
        .collect()
}

/// The blockers assigned to `attacker`, in the order combat damage is assigned
/// across them (see [`combat_damage`]): the attacking player's chosen
/// damage-assignment order (CR 510.1, issue #346) when one has been recorded for
/// this attacker, otherwise stable battlefield order. A chosen order is filtered to
/// the attacker's *current* blockers, so a blocker that has since left combat is
/// simply skipped and the rest keep their chosen sequence.
fn blockers_of(state: &GameState, attacker: PermanentId) -> Vec<PermanentId> {
    let battlefield_order = || -> Vec<PermanentId> {
        state
            .battlefield
            .iter()
            .filter(|p| p.blocking == Some(attacker))
            .map(|p| p.id)
            .collect()
    };
    match state.damage_orders.iter().find(|(atk, _)| *atk == attacker) {
        Some((_, order)) => order
            .iter()
            .copied()
            .filter(|blocker| {
                state
                    .battlefield
                    .iter()
                    .any(|p| p.id == *blocker && p.blocking == Some(attacker))
            })
            .collect(),
        None => battlefield_order(),
    }
}

/// Compute all combat damage for the combat-damage step `step` (CR 510.1): every
/// attacking and blocking creature that deals in this step assigns its power as
/// combat damage, gathered here so [`crate::apply_action`] can apply the whole
/// batch at once (simultaneously, CR 510.2).
///
/// `blocked` is the set of attackers blocked this combat ([`blocked_attackers`]),
/// captured before any damage so a blocked attacker whose blockers have since died
/// is still treated as blocked (CR 509.1h). `step` gates which creatures deal
/// (first strike splits combat in two, CR 510.5 — see [`deals_in_step`]).
///
/// - An **unblocked** attacker assigns its combat damage to the player it is
///   attacking — its own chosen defender (CR 510.1c / 508.1a), not a single global
///   defender, so split attacks route to the right seats. Lifelink gains its
///   controller that much life (CR 702.15e).
/// - A **blocked** attacker assigns its combat damage among its *surviving*
///   blockers in battlefield order, each just-lethal before the next
///   (deathtouch-aware, CR 510.1e); with **trample** any remainder is assigned to
///   the player it is attacking (CR 702.19e), otherwise it is left undealt.
///   Player-chosen damage-assignment order is still deferred.
/// - Each surviving blocker assigns its combat damage to the attacker it blocks
///   (CR 510.1c), carrying its own deathtouch/lifelink.
///
/// Deathtouch is recorded on each [`CombatDamage::ToPermanent`] so the CR 704.5h
/// state-based action can destroy a creature dealt any nonzero deathtouch damage.
/// Pure over the immutable state.
pub(crate) fn combat_damage(
    state: &GameState,
    db: &CardDatabase,
    step: DamageStep,
    blocked: &[PermanentId],
) -> Vec<CombatDamage> {
    let mut out = Vec::new();
    for attacker in state.battlefield.iter().filter(|p| p.attacking.is_some()) {
        // The player this attacker is attacking (CR 508.1a): its damage and any
        // trample overflow route here, not to a single global defender.
        let defender = attacker.attacking;
        let blockers = blockers_of(state, attacker.id);
        // The attacker's own strike, if it deals in this step.
        if deals_in_step(state, attacker, step, db) {
            let power = combat_power(state, attacker.id, db);
            let deathtouch = has_keyword(state, attacker, Keyword::Deathtouch, db);
            let lifelink = has_keyword(state, attacker, Keyword::Lifelink, db);
            let controller = attacker.controller;
            // CR 903.10a: whether this attacker is a commander (identified by its
            // stable instance → designation), so its damage to a player counts
            // toward the 21-combat-damage loss. `None` for an ordinary creature.
            let source_commander = state.commander_owner_of(attacker.instance);
            if !blocked.contains(&attacker.id) {
                // Unblocked: the attacker's damage goes to the player it attacks.
                if power > 0 {
                    if let Some(player) = defender {
                        push_player_damage(
                            &mut out,
                            player,
                            power,
                            controller,
                            lifelink,
                            source_commander,
                        );
                    }
                }
            } else {
                // Blocked: spread across surviving blockers, lethal-per-blocker
                // (deathtouch-aware); trample sends the remainder to the player.
                let mut remaining = power;
                for blocker in &blockers {
                    if remaining == 0 {
                        break;
                    }
                    let assign = remaining.min(lethal_needed(state, *blocker, db, deathtouch));
                    if assign > 0 {
                        push_permanent_damage(
                            &mut out, *blocker, assign, deathtouch, controller, lifelink,
                        );
                        remaining -= assign;
                    }
                }
                // CR 702.19e: a trampler assigns its leftover to the defending
                // player; without trample a blocked creature deals it nowhere. A
                // trampling commander's overflow still counts toward CR 903.10a.
                if remaining > 0 && has_keyword(state, attacker, Keyword::Trample, db) {
                    if let Some(player) = defender {
                        push_player_damage(
                            &mut out,
                            player,
                            remaining,
                            controller,
                            lifelink,
                            source_commander,
                        );
                    }
                }
            }
        }
        // Each surviving blocker deals its power back to the attacker, if it deals
        // in this step (CR 510.1c).
        for blocker in &blockers {
            let Some(bperm) = state.battlefield.iter().find(|p| p.id == *blocker) else {
                continue;
            };
            if !deals_in_step(state, bperm, step, db) {
                continue;
            }
            let bp = combat_power(state, *blocker, db);
            if bp > 0 {
                push_permanent_damage(
                    &mut out,
                    attacker.id,
                    bp,
                    has_keyword(state, bperm, Keyword::Deathtouch, db),
                    bperm.controller,
                    has_keyword(state, bperm, Keyword::Lifelink, db),
                );
            }
        }
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::fixtures::{fixture, id_in};
    use crate::id::CardId;
    use crate::state::Permanent;

    /// A first-strike attacker and a plain blocker/attacker, as an inline catalog —
    /// first strike and deathtouch have no clean M19 representative, so the combat
    /// tests that need those keywords build their own definitions (ADR 0026).
    pub(crate) fn keyword_db() -> CardDatabase {
        let json = r#"[
            {"schema_version":1,"functional_id":"test_duelist","name":"Test Duelist",
             "types":["creature"],"subtypes":["Human","Knight"],"mana_cost":"{1}{W}","colors":["white"],
             "power":2,"toughness":2,"keywords":["first_strike"]},
            {"schema_version":1,"functional_id":"test_adder","name":"Test Adder",
             "types":["creature"],"subtypes":["Snake"],"mana_cost":"{G}","colors":["green"],
             "power":1,"toughness":1,"keywords":["deathtouch"]},
            {"schema_version":1,"functional_id":"test_basilisk","name":"Test Basilisk",
             "types":["creature"],"subtypes":["Basilisk"],"mana_cost":"{4}{G}","colors":["green"],
             "power":4,"toughness":5},
            {"schema_version":1,"functional_id":"test_boar","name":"Test Boar",
             "types":["creature"],"subtypes":["Boar"],"mana_cost":"{2}{G}","colors":["green"],
             "power":3,"toughness":2},
            {"schema_version":1,"functional_id":"test_twinstrike","name":"Test Twinstrike",
             "types":["creature"],"subtypes":["Cat"],"mana_cost":"{2}{W}","colors":["white"],
             "power":2,"toughness":2,"keywords":["double_strike"]},
            {"schema_version":1,"functional_id":"test_paragon","name":"Test Paragon",
             "types":["creature"],"subtypes":["Human","Knight"],"mana_cost":"{2}{W}{W}","colors":["white"],
             "power":2,"toughness":2,"keywords":["first_strike","double_strike"]}
        ]"#;
        CardDatabase::from_json(json).unwrap()
    }

    /// The bundled card database, for tests that need oracle data.
    fn db() -> CardDatabase {
        CardDatabase::bundled().unwrap()
    }

    /// Put a creature of printed card `card` on the battlefield under `controller`,
    /// untapped, entered on turn `entered_turn`; returns its fresh id. Used to
    /// place the keyword-bearing real cards (flying, reach, vigilance, haste).
    pub(crate) fn creature_card(
        state: &mut GameState,
        card: CardId,
        controller: crate::id::PlayerId,
        entered_turn: u32,
    ) -> PermanentId {
        let inst = state.new_instance(card);
        let id = PermanentId(state.mint_id());
        state.battlefield.push(Permanent {
            id,
            instance: inst.id,
            card,
            controller,
            tapped: false,
            entered_turn,
            attacking: None,
            blocking: None,
            damage: 0,
            counters: Default::default(),
            attached_to: None,
        });
        id
    }

    /// Place an attacking creature of `card` under `controller` attacking the sole
    /// opponent (the two-player default); returns its id.
    fn attacker(
        state: &mut GameState,
        card: CardId,
        controller: crate::id::PlayerId,
    ) -> PermanentId {
        let defender = crate::combat::defending_player(state).unwrap_or(crate::id::PlayerId(1));
        attacker_of(state, card, controller, defender)
    }

    /// Place an attacking creature of `card` under `controller` attacking
    /// `defender`; returns its id. Used by the multi-defender combat tests.
    fn attacker_of(
        state: &mut GameState,
        card: CardId,
        controller: crate::id::PlayerId,
        defender: crate::id::PlayerId,
    ) -> PermanentId {
        let id = creature_card(state, card, controller, 0);
        if let Some(perm) = state.battlefield.iter_mut().find(|p| p.id == id) {
            perm.attacking = Some(defender);
        }
        id
    }

    /// Place a creature of `card` under `controller` blocking `blocks`; returns its id.
    fn blocker(
        state: &mut GameState,
        card: CardId,
        controller: crate::id::PlayerId,
        blocks: PermanentId,
    ) -> PermanentId {
        let id = creature_card(state, card, controller, 0);
        if let Some(perm) = state.battlefield.iter_mut().find(|p| p.id == id) {
            perm.blocking = Some(blocks);
        }
        id
    }

    #[test]
    fn issue_153_flying_can_be_blocked_only_by_flying_or_reach_cr_702_9c() {
        // CR 702.9c / 702.17b: a flyer can be blocked only by flying or reach.
        // Tested both directions: a ground creature cannot, flying and reach can.
        let db = db();
        let mut state = GameState::new_two_player();
        let flyer = creature_card(
            &mut state,
            fixture("snapping_drake"),
            crate::id::PlayerId(0),
            0,
        ); // flying
        let ground = creature_card(
            &mut state,
            fixture("walking_corpse"),
            crate::id::PlayerId(1),
            0,
        ); // no keyword
        let other_flyer = creature_card(
            &mut state,
            fixture("snapping_drake"),
            crate::id::PlayerId(1),
            0,
        );
        let reacher = creature_card(
            &mut state,
            fixture("giant_spider"),
            crate::id::PlayerId(1),
            0,
        ); // reach

        assert!(
            !crate::combat::blocker_can_block_attacker(&state, flyer, ground, &db),
            "a ground creature cannot block a flyer (CR 702.9c)"
        );
        assert!(
            crate::combat::blocker_can_block_attacker(&state, flyer, other_flyer, &db),
            "a flyer can block a flyer (CR 702.9c)"
        );
        assert!(
            crate::combat::blocker_can_block_attacker(&state, flyer, reacher, &db),
            "a reach creature can block a flyer (CR 702.17b)"
        );

        // A non-flying attacker imposes no evasion constraint: the ground creature
        // can block a ground attacker.
        let ground_attacker = creature_card(
            &mut state,
            fixture("walking_corpse"),
            crate::id::PlayerId(0),
            0,
        );
        assert!(crate::combat::blocker_can_block_attacker(
            &state,
            ground_attacker,
            ground,
            &db
        ));
    }

    #[test]
    fn issue_153_haste_creature_is_an_attacker_candidate_cr_702_10b() {
        // CR 702.10b: haste exempts a creature from the summoning-sickness attack
        // restriction, so one that entered this very turn may still attack. A
        // vanilla creature that entered this turn stays ineligible (CR 302.6).
        let db = db();
        let mut state = GameState::new_two_player();
        state.turn = 2;
        let hasty = creature_card(
            &mut state,
            fixture("volcanic_dragon"),
            crate::id::PlayerId(0),
            2,
        ); // haste, entered this turn
        let sick = creature_card(
            &mut state,
            fixture("walking_corpse"),
            crate::id::PlayerId(0),
            2,
        ); // entered this turn

        let candidates = crate::combat::attacker_candidates(&state, &db);
        assert!(
            candidates.contains(&hasty),
            "a hasty creature attacks the turn it enters (CR 702.10b)"
        );
        assert!(
            !candidates.contains(&sick),
            "a non-hasty creature that entered this turn cannot attack (CR 302.6)"
        );
    }

    #[test]
    fn issue_154_first_strike_present_needs_two_damage_steps_cr_510_5() {
        // CR 510.5: any first striker in combat means two damage steps; without one
        // a single step suffices.
        let db = keyword_db();
        let mut state = GameState::new_two_player();
        let atk = attacker(
            &mut state,
            id_in(&db, "test_duelist"),
            crate::id::PlayerId(0),
        ); // first strike
        let _blk = blocker(
            &mut state,
            id_in(&db, "test_boar"),
            crate::id::PlayerId(1),
            atk,
        );
        assert!(combat_has_first_strike(&state, &db));

        let mut plain = GameState::new_two_player();
        let a = attacker(&mut plain, id_in(&db, "test_boar"), crate::id::PlayerId(0));
        let _b = blocker(
            &mut plain,
            id_in(&db, "test_boar"),
            crate::id::PlayerId(1),
            a,
        );
        assert!(!combat_has_first_strike(&plain, &db));
    }

    #[test]
    fn issue_154_first_striker_deals_only_in_the_first_step_cr_510_5() {
        // CR 510.5: a first-strike attacker deals in the first-strike step; its
        // vanilla blocker deals in the regular step. `deals_in_step` gates each.
        let db = keyword_db();
        let mut state = GameState::new_two_player();
        let atk = attacker(
            &mut state,
            id_in(&db, "test_duelist"),
            crate::id::PlayerId(0),
        ); // first strike 2/2
        let blk = blocker(
            &mut state,
            id_in(&db, "test_boar"),
            crate::id::PlayerId(1),
            atk,
        ); // vanilla 3/2
        let blocked = blocked_attackers(&state);

        // First-strike step: only the attacker deals (2 to the blocker).
        let first = combat_damage(&state, &db, DamageStep::FirstStrike, &blocked);
        assert_eq!(
            first,
            vec![CombatDamage::ToPermanent {
                permanent: blk,
                amount: 2,
                deathtouch: false,
            }]
        );
        // Regular step: only the (still-present, in this pure call) blocker deals.
        let regular = combat_damage(&state, &db, DamageStep::Regular, &blocked);
        assert_eq!(
            regular,
            vec![CombatDamage::ToPermanent {
                permanent: atk,
                amount: 3,
                deathtouch: false,
            }]
        );
    }

    #[test]
    fn issue_373_double_striker_alone_needs_two_damage_steps_cr_510_5() {
        // CR 702.4b: a double striker deals in the first-strike step, so its mere
        // presence splits combat into two steps even when no creature has plain first
        // strike.
        let db = keyword_db();
        let mut state = GameState::new_two_player();
        let _atk = attacker(
            &mut state,
            id_in(&db, "test_twinstrike"),
            crate::id::PlayerId(0),
        ); // double strike
        assert!(
            combat_has_first_strike(&state, &db),
            "a lone double striker still needs the two-step sequence (CR 510.5)"
        );
    }

    #[test]
    fn issue_373_unblocked_double_striker_deals_in_both_steps_cr_702_4b() {
        // CR 702.4b: an unblocked double striker assigns its power in the first-strike
        // step AND again in the regular step — its power to the defending player twice.
        let db = keyword_db();
        let mut state = GameState::new_two_player();
        let _atk = attacker(
            &mut state,
            id_in(&db, "test_twinstrike"),
            crate::id::PlayerId(0),
        ); // 2/2 double strike
        let blocked = blocked_attackers(&state);

        let first = combat_damage(&state, &db, DamageStep::FirstStrike, &blocked);
        assert_eq!(
            first,
            vec![CombatDamage::ToPlayer {
                player: crate::id::PlayerId(1),
                amount: 2,
                source_commander: None,
            }],
            "the double striker deals in the first-strike step (CR 702.4b)"
        );
        let regular = combat_damage(&state, &db, DamageStep::Regular, &blocked);
        assert_eq!(
            regular,
            vec![CombatDamage::ToPlayer {
                player: crate::id::PlayerId(1),
                amount: 2,
                source_commander: None,
            }],
            "and deals its power again in the regular step (CR 702.4b)"
        );
    }

    #[test]
    fn cr_702_4c_double_strike_with_first_strike_deals_exactly_twice() {
        // CR 702.4c: a creature with both first strike and double strike deals combat
        // damage exactly twice — once per step, never a third time. Combat has only
        // the two steps, and the creature deals its power in each, not more.
        let db = keyword_db();
        let mut state = GameState::new_two_player();
        let _atk = attacker(
            &mut state,
            id_in(&db, "test_paragon"),
            crate::id::PlayerId(0),
        ); // first strike + double strike
        let blocked = blocked_attackers(&state);

        let first = combat_damage(&state, &db, DamageStep::FirstStrike, &blocked);
        let regular = combat_damage(&state, &db, DamageStep::Regular, &blocked);
        // One hit in each step, and there is no third step: exactly twice.
        assert_eq!(
            first,
            vec![CombatDamage::ToPlayer {
                player: crate::id::PlayerId(1),
                amount: 2,
                source_commander: None,
            }],
        );
        assert_eq!(
            regular,
            vec![CombatDamage::ToPlayer {
                player: crate::id::PlayerId(1),
                amount: 2,
                source_commander: None,
            }],
        );
    }

    #[test]
    fn issue_154_deathtouch_makes_one_damage_lethal_for_assignment_cr_510_1e() {
        // CR 510.1e / 702.2b: a deathtouch source needs assign only 1 to a blocker
        // to count as lethal. A 1/1 deathtouch attacker assigns 1 to a 4/5 blocker,
        // flagged deathtouch; the assignment records the deathtouch flag.
        let db = keyword_db();
        let mut state = GameState::new_two_player();
        let atk = attacker(&mut state, id_in(&db, "test_adder"), crate::id::PlayerId(0)); // deathtouch 1/1
        let blk = blocker(
            &mut state,
            id_in(&db, "test_basilisk"),
            crate::id::PlayerId(1),
            atk,
        ); // 4/5
        let blocked = blocked_attackers(&state);

        let batch = combat_damage(&state, &db, DamageStep::Only, &blocked);
        assert!(batch.contains(&CombatDamage::ToPermanent {
            permanent: blk,
            amount: 1,
            deathtouch: true,
        }));
        // The blocker deals its 4 back to the 1/1 attacker.
        assert!(batch.contains(&CombatDamage::ToPermanent {
            permanent: atk,
            amount: 4,
            deathtouch: false,
        }));
    }

    #[test]
    fn issue_374_aura_granted_flying_makes_host_unblockable_and_reverts_cr_702_9c() {
        // CR 613.1f + 702.9c: an Aura granting flying makes its host a flier, so a
        // ground creature cannot block it — exactly as a printed flier. The grant
        // disappears when the Aura leaves.
        let db = db(); // bundled catalog, which includes the `flight` Aura
        let mut state = GameState::new_two_player();
        let host = creature_card(
            &mut state,
            fixture("walking_corpse"),
            crate::id::PlayerId(0),
            0,
        ); // ground
        let ground = creature_card(
            &mut state,
            fixture("walking_corpse"),
            crate::id::PlayerId(1),
            0,
        );
        // Baseline: a ground creature can block a ground attacker.
        assert!(crate::combat::blocker_can_block_attacker(
            &state, host, ground, &db
        ));

        // Attach Flight (Aura granting flying) to the host.
        let aura = creature_card(&mut state, fixture("flight"), crate::id::PlayerId(0), 0);
        state
            .battlefield
            .iter_mut()
            .find(|p| p.id == aura)
            .unwrap()
            .attached_to = Some(host);
        assert!(
            !crate::combat::blocker_can_block_attacker(&state, host, ground, &db),
            "the enchanted creature is a flier; a ground creature cannot block it (CR 702.9c)"
        );

        // The Aura leaves: the grant reverts and the ground creature can block again.
        state.battlefield.retain(|p| p.id != aura);
        assert!(
            crate::combat::blocker_can_block_attacker(&state, host, ground, &db),
            "removing the Aura reverts the granted flying"
        );
    }

    #[test]
    fn issue_374_granted_deathtouch_is_lethal_in_combat_cr_510_1e() {
        // CR 613.1f + 510.1e: a granted deathtouch behaves in combat exactly like a
        // printed one — an attacker with deathtouch granted until end of turn needs
        // assign only 1 to its blocker to be lethal, flagged deathtouch.
        use crate::state::{Duration, EffectAffects, Modification, StaticEffect};
        let db = db();
        let mut state = GameState::new_two_player();
        let atk = attacker(&mut state, fixture("onakke_ogre"), crate::id::PlayerId(0)); // 4/2, no keyword
        let blk = blocker(
            &mut state,
            fixture("colossal_dreadmaw"),
            crate::id::PlayerId(1),
            atk,
        ); // 6/6
        let source = state.mint_id();
        state.static_effects.push(StaticEffect {
            source,
            affects: EffectAffects::SpecificPermanent(atk),
            modification: Modification::GrantKeyword(Keyword::Deathtouch),
            duration: Duration::UntilEndOfTurn,
        });
        let blocked = blocked_attackers(&state);

        let batch = combat_damage(&state, &db, DamageStep::Only, &blocked);
        assert!(
            batch.contains(&CombatDamage::ToPermanent {
                permanent: blk,
                amount: 1,
                deathtouch: true,
            }),
            "granted deathtouch makes 1 damage lethal to the 6/6 blocker (CR 510.1e)"
        );
    }

    #[test]
    fn issue_154_trample_assigns_lethal_then_excess_to_the_player_cr_702_19e() {
        // CR 702.19e: a blocked trampler assigns just-lethal to its blocker, the
        // rest to the defending player. A 6/6 trampler over a 4/2 Ogre assigns 2
        // (lethal) to the Ogre and 4 to player 1.
        let db = db();
        let mut state = GameState::new_two_player();
        let atk = attacker(
            &mut state,
            fixture("colossal_dreadmaw"),
            crate::id::PlayerId(0),
        ); // trample 6/6
        let blk = blocker(
            &mut state,
            fixture("onakke_ogre"),
            crate::id::PlayerId(1),
            atk,
        ); // 4/2
        let blocked = blocked_attackers(&state);

        let batch = combat_damage(&state, &db, DamageStep::Only, &blocked);
        assert!(batch.contains(&CombatDamage::ToPermanent {
            permanent: blk,
            amount: 2,
            deathtouch: false,
        }));
        assert!(batch.contains(&CombatDamage::ToPlayer {
            player: crate::id::PlayerId(1),
            amount: 4,
            source_commander: None,
        }));
    }

    #[test]
    fn issue_154_deathtouch_trample_assigns_one_per_blocker_rest_to_player() {
        // CR 510.1e + 702.19e together: a deathtouch trampler needs assign only 1
        // per blocker before the rest tramples over. The assignment math is verified
        // by exercising `lethal_needed` directly against a blocker — deathtouch makes
        // 1 lethal, otherwise its full toughness is.
        let db = db();
        let mut state = GameState::new_two_player();
        let blk = creature_card(
            &mut state,
            fixture("giant_spider"),
            crate::id::PlayerId(1),
            0,
        ); // 2/4
        assert_eq!(
            super::super::helpers::lethal_needed(&state, blk, &db, true),
            1,
            "deathtouch: 1 is lethal"
        );
        assert_eq!(
            super::super::helpers::lethal_needed(&state, blk, &db, false),
            4,
            "without deathtouch: full toughness is lethal"
        );
    }

    #[test]
    fn issue_154_lifelink_gains_its_controller_life_in_the_same_batch_cr_702_15e() {
        // CR 702.15e: a lifelink source's controller gains life equal to the damage,
        // recorded in the same batch (so it is simultaneous when applied). An
        // unblocked 2/1 lifelinker attacking player 1 hits for 2 and gains 2.
        let db = db();
        let mut state = GameState::new_two_player();
        let _atk = attacker(
            &mut state,
            fixture("child_of_night"),
            crate::id::PlayerId(0),
        ); // lifelink 2/1
        let blocked = blocked_attackers(&state);

        let batch = combat_damage(&state, &db, DamageStep::Only, &blocked);
        assert!(batch.contains(&CombatDamage::ToPlayer {
            player: crate::id::PlayerId(1),
            amount: 2,
            source_commander: None,
        }));
        assert!(batch.contains(&CombatDamage::GainLife {
            player: crate::id::PlayerId(0),
            amount: 2,
        }));
    }

    #[test]
    fn issue_154_blocked_attacker_stays_blocked_when_its_blockers_leave() {
        // CR 509.1h: an attacker recorded as blocked deals no player damage even
        // once its blockers are gone (no trample). Removing the blocker after
        // capturing the blocked set leaves the attacker dealing nothing.
        let db = db();
        let mut state = GameState::new_two_player();
        let atk = attacker(&mut state, fixture("onakke_ogre"), crate::id::PlayerId(0)); // vanilla 3/2, no trample
        let blk = blocker(
            &mut state,
            fixture("onakke_ogre"),
            crate::id::PlayerId(1),
            atk,
        );
        let blocked = blocked_attackers(&state);
        assert_eq!(blocked, vec![atk]);

        // The blocker dies before damage: remove it, keep the blocked snapshot.
        state.battlefield.retain(|p| p.id != blk);
        let batch = combat_damage(&state, &db, DamageStep::Only, &blocked);
        assert!(
            batch.is_empty(),
            "a blocked non-trampler with no surviving blockers deals nothing"
        );
    }

    #[test]
    fn issue_341_split_attacks_route_damage_to_each_chosen_defender() {
        // CR 510.1c: with attackers split across two defenders, each unblocked
        // attacker's damage goes to *its own* chosen defender, not one global one.
        let db = db();
        let mut state = GameState::new_multiplayer(3);
        // Seat 0 attacks: a 4/2 at seat 1 and a 4/2 at seat 2, both unblocked.
        let _at1 = super::super::declaration::tests::attacker_of(
            &mut state,
            fixture("onakke_ogre"),
            crate::id::PlayerId(0),
            crate::id::PlayerId(1),
        );
        let _at2 = super::super::declaration::tests::attacker_of(
            &mut state,
            fixture("onakke_ogre"),
            crate::id::PlayerId(0),
            crate::id::PlayerId(2),
        );
        let blocked = blocked_attackers(&state);
        assert!(blocked.is_empty());

        let batch = combat_damage(&state, &db, DamageStep::Only, &blocked);
        assert!(
            batch.contains(&CombatDamage::ToPlayer {
                player: crate::id::PlayerId(1),
                amount: 4,
                source_commander: None,
            }),
            "the attacker assigned to seat 1 hits seat 1"
        );
        assert!(
            batch.contains(&CombatDamage::ToPlayer {
                player: crate::id::PlayerId(2),
                amount: 4,
                source_commander: None,
            }),
            "the attacker assigned to seat 2 hits seat 2"
        );
    }

    #[test]
    fn issue_341_trample_overflow_routes_to_the_attackers_own_defender() {
        // CR 702.19e: a blocked trampler's overflow goes to the player it is
        // attacking. A 6/6 trampler at seat 2, blocked by seat 2's 4/2, assigns 2
        // (lethal) to the blocker and tramples 4 to seat 2 — never seat 1.
        let db = db();
        let mut state = GameState::new_multiplayer(3);
        let atk = super::super::declaration::tests::attacker_of(
            &mut state,
            fixture("colossal_dreadmaw"),
            crate::id::PlayerId(0),
            crate::id::PlayerId(2),
        ); // trample 6/6
        let blk = blocker(
            &mut state,
            fixture("onakke_ogre"),
            crate::id::PlayerId(2),
            atk,
        ); // 4/2
        let blocked = blocked_attackers(&state);

        let batch = combat_damage(&state, &db, DamageStep::Only, &blocked);
        assert!(batch.contains(&CombatDamage::ToPermanent {
            permanent: blk,
            amount: 2,
            deathtouch: false,
        }));
        assert!(
            batch.contains(&CombatDamage::ToPlayer {
                player: crate::id::PlayerId(2),
                amount: 4,
                source_commander: None,
            }),
            "trample overflow hits the attacker's own defender (seat 2)"
        );
        assert!(
            !batch.contains(&CombatDamage::ToPlayer {
                player: crate::id::PlayerId(1),
                amount: 4,
                source_commander: None,
            }),
            "no damage leaks to the other opponent (seat 1)"
        );
    }
}
