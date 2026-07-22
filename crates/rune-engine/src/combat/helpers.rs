use crate::card::Keyword;
use crate::card_type::CardType;
use crate::characteristics::{characteristics, permanent_has_keyword};
use crate::id::PermanentId;
use crate::state::{GameState, Permanent};
use crate::CardDatabase;

/// Whether `perm` has summoning sickness for its controller (CR 302.6): it has
/// **not** been under that player's control continuously since their most recent
/// turn began.
///
/// Derived from [`Permanent::entered_turn`]: a permanent that entered on an
/// earlier turn than the current one was already in play when this turn began, so
/// it is not sick; one that entered this turn is. Written for the active player,
/// whose most recent turn is the current [`GameState::turn`] — the only player
/// who declares attackers in this slice.
#[must_use]
pub(crate) fn has_summoning_sickness(perm: &Permanent, state: &GameState) -> bool {
    perm.entered_turn >= state.turn
}

/// Whether `perm` is a creature by its printed card types. Type-changing
/// continuous effects are future work, so the printed types are authoritative
/// here (as they are in [`crate::resolve::target_is_legal`]).
#[must_use]
pub(super) fn is_creature(perm: &Permanent, db: &CardDatabase) -> bool {
    db.card(perm.card)
        .is_some_and(|c| c.has_type(CardType::Creature))
}

/// Whether `perm` currently has keyword `keyword` (CR 702): its printed keywords
/// unioned with any granted at CR 613 layer 6 (CR 613.1f). Reads through the
/// computed [`characteristics`], so a keyword granted by an Aura, an anthem, or an
/// until-end-of-turn pump is enforced in combat exactly like a printed one.
#[must_use]
pub(crate) fn has_keyword(
    state: &GameState,
    perm: &Permanent,
    keyword: Keyword,
    db: &CardDatabase,
) -> bool {
    permanent_has_keyword(state, perm.id, keyword, db)
}

/// Whether the creature `blocker` may legally be assigned to block `attacker`
/// given evasion keywords (CR 509.1b): a creature with flying can be blocked only
/// by creatures with flying or reach (CR 702.9c, CR 702.17b). Any creature can
/// block a non-flying attacker.
///
/// Both ids are looked up on the battlefield; a missing permanent (a stale id)
/// yields `false`, so the caller rejects the assignment. This is a per-pair
/// predicate the block-legality check applies on top of the candidate-set
/// membership tests, so partial blocks of mixed flying/ground attackers stay
/// expressible — evasion is enforced in legality, not by hiding candidates.
#[must_use]
pub(crate) fn blocker_can_block_attacker(
    state: &GameState,
    attacker: PermanentId,
    blocker: PermanentId,
    db: &CardDatabase,
) -> bool {
    let Some(atk) = state.battlefield.iter().find(|p| p.id == attacker) else {
        return false;
    };
    // A non-flying attacker imposes no evasion constraint.
    if !has_keyword(state, atk, Keyword::Flying, db) {
        return true;
    }
    let Some(blk) = state.battlefield.iter().find(|p| p.id == blocker) else {
        return false;
    };
    // CR 702.9c / 702.17b: only flying or reach may block a flyer.
    has_keyword(state, blk, Keyword::Flying, db) || has_keyword(state, blk, Keyword::Reach, db)
}

/// Whether `perm` deals its combat damage in `step` (CR 510.5). In an ordinary
/// combat ([`crate::combat::DamageStep::Only`]) every creature deals; when a
/// first-strike step is present, a first-striker deals only in
/// [`crate::combat::DamageStep::FirstStrike`] and every other creature only in
/// [`crate::combat::DamageStep::Regular`].
///
/// Double strike (CR 702.4b) is the exception that deals in *both* steps: it
/// participates in the first-strike step alongside first strike, and — unlike plain
/// first strike — deals again in the regular step. A creature with both first strike
/// and double strike deals exactly twice, not three times (CR 702.4c): the two
/// keywords collapse to the same two participations rather than adding a third.
#[must_use]
pub(crate) fn deals_in_step(
    state: &GameState,
    perm: &Permanent,
    step: crate::combat::DamageStep,
    db: &CardDatabase,
) -> bool {
    let double_strike = has_keyword(state, perm, Keyword::DoubleStrike, db);
    match step {
        crate::combat::DamageStep::Only => true,
        // CR 702.4b / 702.7b: first strike *and* double strike deal in the
        // first-strike step.
        crate::combat::DamageStep::FirstStrike => {
            has_keyword(state, perm, Keyword::FirstStrike, db) || double_strike
        }
        // CR 510.5: the regular step is for creatures without first strike — plus
        // double strikers, which strike a second time here (CR 702.4b).
        crate::combat::DamageStep::Regular => {
            double_strike || !has_keyword(state, perm, Keyword::FirstStrike, db)
        }
    }
}

/// The current power of `id` as a non-negative amount of combat damage: a
/// creature assigns combat damage equal to its power (CR 510.1a), and a creature
/// with `0` or negative power (or none at all) assigns none. Reads current
/// power through [`characteristics`], so counters and anthems are folded in.
pub(crate) fn combat_power(state: &GameState, id: PermanentId, db: &CardDatabase) -> u32 {
    let power = characteristics(state, id, db).power.unwrap_or(0);
    u32::try_from(power.max(0)).unwrap_or(0)
}

/// The damage the assigning creature must put on blocker `id` to count as lethal
/// (CR 510.1c — an attacker assigns at least lethal damage to a blocker before the
/// next). Ordinarily this is the blocker's current toughness less any damage
/// already marked, floored at `0`; when the source has **deathtouch** it is just
/// `1` (any nonzero damage is lethal, CR 510.1e / 702.2b). `0` for a creature with
/// no toughness or already at/over lethal.
pub(crate) fn lethal_needed(
    state: &GameState,
    id: PermanentId,
    db: &CardDatabase,
    deathtouch: bool,
) -> u32 {
    let toughness = characteristics(state, id, db).toughness.unwrap_or(0);
    let marked = state
        .battlefield
        .iter()
        .find(|p| p.id == id)
        .map_or(0, |p| p.damage);
    let remaining =
        u32::try_from((toughness - i32::try_from(marked).unwrap_or(i32::MAX)).max(0)).unwrap_or(0);
    if deathtouch {
        // CR 510.1e: with deathtouch, 1 damage is lethal — but never assign to a
        // creature that already needs none.
        remaining.min(1)
    } else {
        remaining
    }
}

/// Record `amount` combat damage a `source_controller`'s creature deals to
/// `player`, plus the simultaneous lifelink life gain if the source has it
/// (CR 702.15e). `source_commander` carries the source's commander designation
/// (its owning player) when the striking creature is a commander, so the batch
/// application can feed the CR 903.10a commander-damage tally (`None` otherwise).
pub(crate) fn push_player_damage(
    out: &mut Vec<crate::combat::CombatDamage>,
    player: crate::id::PlayerId,
    amount: u32,
    source_controller: crate::id::PlayerId,
    lifelink: bool,
    source_commander: Option<crate::id::PlayerId>,
) {
    out.push(crate::combat::CombatDamage::ToPlayer {
        player,
        amount,
        source_commander,
    });
    if lifelink && amount > 0 {
        out.push(crate::combat::CombatDamage::GainLife {
            player: source_controller,
            amount,
        });
    }
}

/// Record `amount` combat damage a `source_controller`'s creature deals to
/// `permanent`, carrying the source's deathtouch flag (CR 702.2b) and adding the
/// simultaneous lifelink life gain if the source has it (CR 702.15e).
pub(crate) fn push_permanent_damage(
    out: &mut Vec<crate::combat::CombatDamage>,
    permanent: PermanentId,
    amount: u32,
    deathtouch: bool,
    source_controller: crate::id::PlayerId,
    lifelink: bool,
) {
    out.push(crate::combat::CombatDamage::ToPermanent {
        permanent,
        amount,
        deathtouch,
    });
    if lifelink && amount > 0 {
        out.push(crate::combat::CombatDamage::GainLife {
            player: source_controller,
            amount,
        });
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::fixtures::{fixture, id_in};
    use crate::state::Permanent;

    /// A first-strike attacker and a plain blocker/attacker, as an inline catalog —
    /// first strike and deathtouch have no clean M19 representative, so the combat
    /// tests that need those keywords build their own definitions (ADR 0026).
    fn keyword_db() -> CardDatabase {
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
    #[allow(dead_code)]
    fn db() -> CardDatabase {
        CardDatabase::bundled().unwrap()
    }

    /// Put a creature (Walking Corpse, a vanilla 2/2 with no combat keyword) on the
    /// battlefield under `controller` with the given tapped state, having entered on
    /// turn `entered_turn`.
    fn creature(
        state: &mut GameState,
        controller: crate::id::PlayerId,
        tapped: bool,
        entered_turn: u32,
    ) -> PermanentId {
        let inst = state.new_instance(fixture("walking_corpse"));
        let id = PermanentId(state.mint_id());
        state.battlefield.push(Permanent {
            id,
            instance: inst.id,
            card: fixture("walking_corpse"),
            controller,
            tapped,
            entered_turn,
            attacking: None,
            blocking: None,
            damage: 0,
            counters: Default::default(),
            attached_to: None,
        });
        id
    }

    #[test]
    fn summoning_sickness_is_by_entry_turn_cr_302_6() {
        // CR 302.6: a creature that entered this turn is sick; one that entered a
        // previous turn is not.
        let mut state = GameState::new_two_player();
        state.turn = 3;
        let fresh = creature(&mut state, crate::id::PlayerId(0), false, 3);
        let seasoned = creature(&mut state, crate::id::PlayerId(0), false, 1);
        let fresh = state.battlefield.iter().find(|p| p.id == fresh).unwrap();
        let seasoned = state.battlefield.iter().find(|p| p.id == seasoned).unwrap();
        assert!(has_summoning_sickness(fresh, &state));
        assert!(!has_summoning_sickness(seasoned, &state));
    }

    #[test]
    fn issue_154_deathtouch_makes_one_damage_lethal_for_assignment_cr_510_1e() {
        // CR 510.1e / 702.2b: a deathtouch source needs assign only 1 to a blocker
        // to count as lethal. A 1/1 deathtouch attacker assigns 1 to a 4/5 blocker,
        // flagged deathtouch; the assignment records the deathtouch flag.
        let db = keyword_db();
        let mut state = GameState::new_two_player();
        let blk = super::super::damage::tests::creature_card(
            &mut state,
            id_in(&db, "test_basilisk"),
            crate::id::PlayerId(1),
            0,
        );
        assert_eq!(
            lethal_needed(&state, blk, &db, true),
            1,
            "deathtouch: 1 is lethal"
        );
        assert_eq!(
            lethal_needed(&state, blk, &db, false),
            5,
            "without deathtouch: full toughness is lethal"
        );
    }
}
