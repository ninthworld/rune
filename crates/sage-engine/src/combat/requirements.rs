//! Block **requirements** (CR 509.1c): the part of a declaration a player is not free
//! to leave out.
//!
//! Everything else in this module tree narrows a declaration. A restriction says the
//! declaration in hand is illegal because of a pair it contains, or a count it reached;
//! each of those can be answered by looking at what was submitted. CR 509.1c asks the
//! opposite question — *the declaration must obey the maximum possible number of
//! requirements without violating any restriction* — and that cannot be answered from
//! the submission alone, because "the maximum possible" is a fact about the declarations
//! that were **not** submitted. Validating one is therefore a search
//! ([`max_block_requirements_met`]), and this module is where that search lives so no
//! other gate is tempted to approximate it per pair.
//!
//! Two rules the search encodes, and both are the reason a per-pair approximation is
//! wrong rather than merely imprecise:
//!
//! - **Restrictions win.** A requirement is met only by a declaration that is legal to
//!   begin with, so a creature required to block an attacker it may not legally block is
//!   simply not required to. A per-pair check would refuse the declaration a player is
//!   forced into.
//! - **A requirement that cannot be met is not met.** Two attackers that each demand the
//!   defender's only creature demand it once between them: the maximum is one, and either
//!   answer is legal. A per-pair check would demand both and leave no legal declaration
//!   at all.

use std::collections::BTreeMap;

use crate::card::CombatRestriction;
use crate::id::{PermanentId, PlayerId};
use crate::state::GameState;
use crate::CardDatabase;

use super::eligibility::{attacking_defender_of, blocker_candidates_for, declared_attackers};
use super::helpers::{
    blocked_by_at_most_one, blocker_can_block_attacker, blocks_allowed, permanent_has_menace,
    permanent_has_restriction,
};

/// Whether **every creature able to block** the permanent `id` must do so (CR 509.1c) —
/// the one requirement in the [`CombatRestriction`] vocabulary, looked up by id so the
/// legality gate and the server's blocker prompt can ask about an attacker they hold only
/// an id for.
///
/// Read through the computed restrictions (CR 613.1f), so a granted requirement binds
/// exactly as a printed one would and stops binding the instant the grant ends — which
/// matters more here than anywhere else in the vocabulary, since every requirement in the
/// catalog is granted by a spell for one turn. `false` for an id no longer on the
/// battlefield.
#[must_use]
pub fn must_be_blocked_by_all_able(state: &GameState, id: PermanentId, db: &CardDatabase) -> bool {
    permanent_has_restriction(state, id, CombatRestriction::MustBeBlockedByAllAble, db)
}

/// The `(blocker, attacker)` pairs that `declarer` is currently **required** to declare
/// (CR 509.1c), one per creature able to block each attacker under a requirement.
///
/// "All creatures able to block this creature do so" is not one requirement — it is one
/// requirement **per able creature**, which is what makes the maximisation a count rather
/// than a yes-or-no. The set is derived from the board as it stands: the declarer's own
/// blocker candidates ([`blocker_candidates_for`], which is where tapped and "can't
/// block" drop out), each paired with an attacker attacking *them* that the pairwise gate
/// ([`blocker_can_block_attacker`]) says they may block.
///
/// Scoped to one declarer for the same reason the legality gate is (issue #344): a
/// requirement binds the player who owes the declaration, and says nothing to anyone
/// else's sub-combat. Empty — the overwhelmingly common case — when nothing attacking
/// this declarer carries a requirement.
#[must_use]
pub fn block_requirements(
    state: &GameState,
    declarer: PlayerId,
    db: &CardDatabase,
) -> Vec<(PermanentId, PermanentId)> {
    let required = required_attackers(state, declarer, db);
    if required.is_empty() {
        return Vec::new();
    }
    let blockers = blocker_candidates_for(state, declarer, db);
    required
        .into_iter()
        .flat_map(|attacker| {
            blockers
                .iter()
                .copied()
                .filter(move |&blocker| blocker_can_block_attacker(state, attacker, blocker, db))
                .map(move |blocker| (blocker, attacker))
        })
        .collect()
}

/// The **maximum number of block requirements** any legal declaration by `declarer` could
/// meet right now (CR 509.1c) — the number a submitted declaration is measured against.
///
/// This is the search the rest of combat is careful not to be. It is exact rather than
/// heuristic, and it is exact by exploring only declarations that are *already* legal, so
/// "obey the maximum possible number of requirements **without violating any
/// restrictions**" needs no second pass: a declaration a restriction forbids is never a
/// candidate, and the requirements it would have met are therefore not possible.
///
/// **What it searches.** Only assignments to attackers under a requirement, because no
/// other assignment can meet one: blocking an attacker that demands nothing spends a
/// blocker's capacity and adds nothing to the count, so dropping every such assignment
/// from a candidate declaration leaves it legal (an attacker's blocker count falls to
/// zero, which no floor or ceiling forbids) and no worse. Every declaration this walks is
/// therefore realisable, which is what guarantees a player always has a legal answer.
///
/// **How it searches.** One pass over the declarer's blockers, carrying the set of
/// reachable *outcomes*: how many creatures have been assigned to each required attacker,
/// and how many requirements that met. The count per attacker is clamped at two, because
/// two is as far as any restriction can see — a ceiling forbids a second blocker
/// ([`blocked_by_at_most_one`]) and menace's floor forbids stopping at exactly one — so
/// the frontier is bounded by `3^k` in the number `k` of attackers under a requirement,
/// and a blocker's choices by the subsets of those it may block up to its own allowance
/// ([`blocks_allowed`], one for every creature without a permission). `k` is the count of
/// attackers a requirement has been *granted* to this combat, which is one per resolution
/// of the one spell in the catalog that grants any.
///
/// `0` when nothing is required, which is the answer for every ordinary combat and is
/// reached without touching the battlefield twice.
#[must_use]
pub fn max_block_requirements_met(
    state: &GameState,
    declarer: PlayerId,
    db: &CardDatabase,
) -> usize {
    let required = required_attackers(state, declarer, db);
    if required.is_empty() {
        return 0;
    }
    // Per required attacker, the two bounds a count can run into: a ceiling that forbids
    // a second blocker outright, and menace's floor that forbids stopping at one.
    let ceilings: Vec<bool> = required
        .iter()
        .map(|&attacker| blocked_by_at_most_one(state, attacker, db))
        .collect();
    let floors: Vec<bool> = required
        .iter()
        .map(|&attacker| permanent_has_menace(state, attacker, db))
        .collect();

    // The reachable outcomes: clamped per-attacker blocker counts, each mapped to the
    // most requirements any declaration reaching that outcome has met. Nothing assigned
    // is always reachable, which is why a legal declaration always exists.
    let mut outcomes: BTreeMap<Vec<u8>, usize> = BTreeMap::new();
    outcomes.insert(vec![0; required.len()], 0);

    for blocker in blocker_candidates_for(state, declarer, db) {
        let able: Vec<usize> = required
            .iter()
            .enumerate()
            .filter(|&(_, &attacker)| blocker_can_block_attacker(state, attacker, blocker, db))
            .map(|(index, _)| index)
            .collect();
        if able.is_empty() {
            continue;
        }
        let allowance = usize::try_from(blocks_allowed(state, blocker, db)).unwrap_or(usize::MAX);
        let assignments = assignments_up_to(&able, allowance);
        // Starting from the outcomes as they stand is this blocker declining to block:
        // a requirement never forces a creature into a declaration a restriction would
        // then make illegal, so "assign nothing" is always one of its choices.
        let mut extended = outcomes.clone();
        for (counts, met) in &outcomes {
            for assignment in &assignments {
                let Some(next) = extend(counts, assignment, &ceilings) else {
                    continue;
                };
                let met = met + assignment.len();
                let best = extended.entry(next).or_insert(met);
                *best = (*best).max(met);
            }
        }
        outcomes = extended;
    }

    outcomes
        .into_iter()
        .filter(|(counts, _)| meets_floors(counts, &floors))
        .map(|(_, met)| met)
        .max()
        .unwrap_or(0)
}

/// The attackers attacking `declarer` that currently carry a block requirement
/// (CR 509.1c), in stable battlefield order — the frame of reference every count in this
/// module is indexed by.
fn required_attackers(
    state: &GameState,
    declarer: PlayerId,
    db: &CardDatabase,
) -> Vec<PermanentId> {
    declared_attackers(state)
        .into_iter()
        .filter(|&attacker| attacking_defender_of(state, attacker) == Some(declarer))
        .filter(|&attacker| must_be_blocked_by_all_able(state, attacker, db))
        .collect()
}

/// Every set of required attackers one blocker could be assigned to: the non-empty
/// subsets of `able` no larger than `allowance` (CR 509.1a — one attacker unless a
/// permission says otherwise).
///
/// Non-empty because declining is carried separately, and by *size* rather than as every
/// bitmask so the ordinary blocker — allowance one — produces one assignment per attacker
/// it can reach rather than a power set of them.
fn assignments_up_to(able: &[usize], allowance: usize) -> Vec<Vec<usize>> {
    let mut out = Vec::new();
    let mut current = Vec::new();
    fn walk(
        able: &[usize],
        start: usize,
        allowance: usize,
        current: &mut Vec<usize>,
        out: &mut Vec<Vec<usize>>,
    ) {
        if current.len() == allowance {
            return;
        }
        for index in start..able.len() {
            current.push(able[index]);
            out.push(current.clone());
            walk(able, index + 1, allowance, current, out);
            current.pop();
        }
    }
    walk(able, 0, allowance, &mut current, &mut out);
    out
}

/// The outcome reached by adding one blocker's `assignment` to `counts`, or `None` when a
/// ceiling forbids it — the one bound that can be judged as the count rises rather than
/// once the declaration is whole.
///
/// Counts saturate at two: no restriction in the vocabulary distinguishes two blockers
/// from ten, so collapsing them is what keeps the frontier bounded without losing an
/// answer.
fn extend(counts: &[u8], assignment: &[usize], ceilings: &[bool]) -> Option<Vec<u8>> {
    let mut next = counts.to_vec();
    for &index in assignment {
        let count = next.get_mut(index)?;
        if ceilings.get(index).copied().unwrap_or(false) && *count >= 1 {
            return None;
        }
        *count = (*count + 1).min(2);
    }
    Some(next)
}

/// Whether an outcome clears every menace floor (CR 702.110b): an attacker with menace
/// is blocked by two or more creatures, or by none. The bound that can only be judged
/// once the declaration is whole, which is why it is applied to the finished outcomes
/// rather than as they are built.
fn meets_floors(counts: &[u8], floors: &[bool]) -> bool {
    counts
        .iter()
        .zip(floors)
        .all(|(&count, &floor)| !floor || count != 1)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::fixtures::fixture;

    fn db() -> CardDatabase {
        CardDatabase::bundled().unwrap()
    }

    /// Impose `restriction` on `id` until end of turn, the way a spell does — so the
    /// search can be exercised over restrictions no printed card carries together.
    fn impose(state: &mut GameState, id: PermanentId, restriction: CombatRestriction) {
        let source = state.mint_id();
        state.static_effects.push(crate::state::StaticEffect {
            source,
            affects: crate::state::EffectAffects::SpecificPermanent(id),
            modification: crate::state::Modification::GrantRestriction(restriction),
            duration: crate::state::Duration::UntilEndOfTurn,
        });
    }

    /// An attacking creature of `slug` under seat 0, attacking seat 1.
    fn attacker(state: &mut GameState, slug: &str) -> PermanentId {
        super::super::declaration::tests::attacker_of(
            state,
            fixture(slug),
            PlayerId(0),
            PlayerId(1),
        )
    }

    /// An untapped creature of `slug` under seat 1, the declaring player.
    fn blocker(state: &mut GameState, slug: &str) -> PermanentId {
        super::super::damage::tests::creature_card(state, fixture(slug), PlayerId(1), 0)
    }

    #[test]
    fn issue_739_an_ordinary_combat_requires_nothing() {
        // The answer for every combat the catalog could play before this: no attacker
        // carries a requirement, so nothing is required and no search happens.
        let db = db();
        let mut state = GameState::new_two_player();
        attacker(&mut state, "centaur_courser");
        blocker(&mut state, "walking_corpse");

        assert!(block_requirements(&state, PlayerId(1), &db).is_empty());
        assert_eq!(max_block_requirements_met(&state, PlayerId(1), &db), 0);
    }

    #[test]
    fn issue_739_every_able_creature_is_one_requirement_of_its_own() {
        // "All creatures able to block it do so" binds each able creature separately,
        // which is what makes the maximum a count. Two blockers, one required attacker,
        // and both are required to block it.
        let db = db();
        let mut state = GameState::new_two_player();
        let required = attacker(&mut state, "centaur_courser");
        let first = blocker(&mut state, "walking_corpse");
        let second = blocker(&mut state, "sun_sentinel");
        impose(
            &mut state,
            required,
            CombatRestriction::MustBeBlockedByAllAble,
        );

        let mut pairs = block_requirements(&state, PlayerId(1), &db);
        pairs.sort_by_key(|&(blocker, _)| blocker.0);
        assert_eq!(pairs, vec![(first, required), (second, required)]);
        assert_eq!(max_block_requirements_met(&state, PlayerId(1), &db), 2);
    }

    #[test]
    fn issue_739_a_creature_that_cannot_legally_block_is_not_required_to() {
        // Restrictions win (CR 509.1c). The requirement is granted to a flyer, so the
        // ground creature is not able and is not required; the reach blocker beside it
        // still is. A per-pair check that read the requirement without the pairwise gate
        // would demand a block no rule permits.
        let db = db();
        let mut state = GameState::new_two_player();
        let flyer = attacker(&mut state, "air_elemental");
        let grounded = blocker(&mut state, "walking_corpse");
        let spider = blocker(&mut state, "giant_spider");
        impose(&mut state, flyer, CombatRestriction::MustBeBlockedByAllAble);

        assert_eq!(
            block_requirements(&state, PlayerId(1), &db),
            vec![(spider, flyer)],
            "only the creature with reach is able, so only it is required"
        );
        assert_eq!(max_block_requirements_met(&state, PlayerId(1), &db), 1);
        assert!(!blocker_can_block_attacker(&state, flyer, grounded, &db));
    }

    #[test]
    fn issue_739_one_blocker_owed_to_two_attackers_meets_one_requirement() {
        // The case that makes this a search: both attackers require the defender's only
        // creature, and it can block one of them. The maximum is one — not two, which no
        // declaration could reach, and not zero.
        let db = db();
        let mut state = GameState::new_two_player();
        let first = attacker(&mut state, "centaur_courser");
        let second = attacker(&mut state, "centaur_courser");
        let sole = blocker(&mut state, "walking_corpse");
        impose(&mut state, first, CombatRestriction::MustBeBlockedByAllAble);
        impose(
            &mut state,
            second,
            CombatRestriction::MustBeBlockedByAllAble,
        );

        assert_eq!(block_requirements(&state, PlayerId(1), &db).len(), 2);
        assert_eq!(max_block_requirements_met(&state, PlayerId(1), &db), 1);

        // A blocker that may block an additional creature meets both, because the
        // allowance is what the search spends (CR 509.1a).
        impose(&mut state, sole, CombatRestriction::CanBlockAdditional(1));
        assert_eq!(max_block_requirements_met(&state, PlayerId(1), &db), 2);
    }

    #[test]
    fn issue_739_menace_lowers_the_maximum_to_what_the_floor_allows() {
        // A restriction beats a requirement (CR 509.1c). With menace on the required
        // attacker a lone blocker is illegal, so with one creature the maximum is zero —
        // the requirement simply cannot be met — and with two it is two.
        let db = db();
        let mut state = GameState::new_two_player();
        let required = attacker(&mut state, "centaur_courser");
        blocker(&mut state, "walking_corpse");
        impose(
            &mut state,
            required,
            CombatRestriction::MustBeBlockedByAllAble,
        );
        impose(&mut state, required, CombatRestriction::CantBeBlocked);
        assert_eq!(
            max_block_requirements_met(&state, PlayerId(1), &db),
            0,
            "nothing is able to block it, so nothing is required to"
        );

        let mut state = GameState::new_two_player();
        let required = attacker(&mut state, "boggart_brute"); // menace
        blocker(&mut state, "walking_corpse");
        impose(
            &mut state,
            required,
            CombatRestriction::MustBeBlockedByAllAble,
        );
        assert_eq!(
            block_requirements(&state, PlayerId(1), &db).len(),
            1,
            "the pair is legal on its own; only the count is not"
        );
        assert_eq!(
            max_block_requirements_met(&state, PlayerId(1), &db),
            0,
            "a lone blocker violates menace's floor, so no declaration meets it"
        );

        blocker(&mut state, "sun_sentinel");
        assert_eq!(
            max_block_requirements_met(&state, PlayerId(1), &db),
            2,
            "two blockers clear the floor, and both are then required"
        );
    }

    #[test]
    fn issue_739_a_block_count_ceiling_caps_the_maximum_at_one() {
        // The mirror of the floor: two creatures are able, but no declaration may assign
        // both, so the maximum is one and either of them satisfies it.
        let db = db();
        let mut state = GameState::new_two_player();
        let required = attacker(&mut state, "bristling_boar"); // can't be blocked by more than one
        blocker(&mut state, "walking_corpse");
        blocker(&mut state, "sun_sentinel");
        impose(
            &mut state,
            required,
            CombatRestriction::MustBeBlockedByAllAble,
        );

        assert_eq!(block_requirements(&state, PlayerId(1), &db).len(), 2);
        assert_eq!(max_block_requirements_met(&state, PlayerId(1), &db), 1);
    }

    #[test]
    fn issue_739_a_requirement_binds_only_the_player_being_attacked() {
        // Scoped exactly as the declaration is (issue #344): the requirement is on an
        // attacker aimed at seat 1, and says nothing about seat 2's creatures.
        let db = db();
        let mut state = GameState::new_multiplayer(3);
        let required = super::super::declaration::tests::attacker_of(
            &mut state,
            fixture("centaur_courser"),
            PlayerId(0),
            PlayerId(1),
        );
        blocker(&mut state, "walking_corpse");
        super::super::damage::tests::creature_card(
            &mut state,
            fixture("walking_corpse"),
            PlayerId(2),
            0,
        );
        impose(
            &mut state,
            required,
            CombatRestriction::MustBeBlockedByAllAble,
        );

        assert_eq!(max_block_requirements_met(&state, PlayerId(1), &db), 1);
        assert_eq!(max_block_requirements_met(&state, PlayerId(2), &db), 0);
    }

    #[test]
    fn issue_739_assignments_are_bounded_by_the_allowance() {
        // The helper the search spends a blocker's allowance with: one attacker at a
        // time for an ordinary creature, and every pair as well once it may block an
        // additional one — never the whole power set.
        assert_eq!(
            assignments_up_to(&[0, 1, 2], 1),
            vec![vec![0], vec![1], vec![2]]
        );
        assert_eq!(
            assignments_up_to(&[0, 1, 2], 2),
            vec![
                vec![0],
                vec![0, 1],
                vec![0, 2],
                vec![1],
                vec![1, 2],
                vec![2],
            ]
        );
        assert!(assignments_up_to(&[0, 1], 0).is_empty());
    }
}
