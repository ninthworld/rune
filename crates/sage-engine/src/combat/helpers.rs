use crate::card::{CombatRestriction, Keyword};
use crate::card_type::CardType;
use crate::characteristics::{characteristics, permanent_has_keyword};
use crate::id::{PermanentId, PlayerId};
use crate::state::{GameState, Permanent};
use crate::CardDatabase;

/// Whether `perm` has summoning sickness for its controller (CR 302.6): it has
/// **not** been under that player's control continuously since their most recent
/// turn began.
///
/// Derived from [`Permanent::entered_turn`] and [`turn_began_for`]: the permanent
/// is sick exactly when it came under its controller's control on or after the
/// turn that controller most recently began.
///
/// The comparison is against **the controller's** most recent turn, not the
/// current one. Those differ for every seat that is not the active player, and
/// getting it wrong frees a creature too early: a creature cast by seat 0 on
/// turn 1 is still restricted throughout seat 1's turn 2 and only loses the
/// restriction when seat 0's turn 3 begins. Since a non-active player may hold
/// priority and activate abilities at instant speed, that difference is
/// observable, not academic.
#[must_use]
pub(crate) fn has_summoning_sickness(perm: &Permanent, state: &GameState) -> bool {
    perm.entered_turn >= turn_began_for(state, crate::characteristics::controller_of(state, perm))
}

/// The turn number on which `player`'s most recent turn began — the reference
/// point CR 302.6 measures continuous control from.
///
/// The active player's most recent turn is, by definition, the current one, so
/// their answer is read from [`GameState::turn`] rather than from stored state.
/// Everyone else is answered from [`crate::player::Player::turn_began`], which the turn boundary
/// records; `0` there means the seat has not taken a turn yet, so nothing that
/// entered during the game has been controlled since one began.
#[must_use]
pub(crate) fn turn_began_for(state: &GameState, player: PlayerId) -> u32 {
    if player == state.active_player {
        return state.turn;
    }
    state
        .players
        .get(player.0)
        .map_or(state.turn, |p| p.turn_began)
}

/// Whether `perm` is a creature by its printed card types. Type-changing
/// continuous effects are future work, so the printed types are authoritative
/// here (as they are in [`crate::resolve::target_is_legal`]).
#[must_use]
pub(super) fn is_creature(perm: &Permanent, db: &CardDatabase) -> bool {
    perm.printed
        .face(db)
        .is_some_and(|face| face.has_type(CardType::Creature))
}

/// Whether the summoning-sickness restriction of CR 302.6 currently applies to
/// `perm`: it is a creature, it has [`has_summoning_sickness`], and it does not
/// have haste (CR 702.10b, which exempts it).
///
/// CR 302.6 imposes **one** restriction that governs **two** things: such a
/// creature can't attack, *and* an ability of it whose cost contains `{T}` (or
/// `{Q}`) can't be activated. Both call sites — [`super::attacker_candidates`] and
/// the activated-ability arm of [`crate::valid_actions`] — read this single
/// predicate, so the haste exemption can never drift between them.
///
/// Only creatures are ever summoning sick: a land or mana rock that entered this
/// turn taps freely, so this is `false` for every non-creature permanent.
#[must_use]
pub fn summoning_sickness_restricts(
    state: &GameState,
    perm: &Permanent,
    db: &CardDatabase,
) -> bool {
    is_creature(perm, db)
        && has_summoning_sickness(perm, state)
        && !has_keyword(state, perm, Keyword::Haste, db)
}

/// Whether declaring `attacker` as an attacker would **tap** it: attacking taps
/// (CR 508.1f) unless the creature has vigilance (CR 702.20b).
///
/// The same question [`crate::apply_action`] answers while applying the declaration,
/// asked *before* anything is applied — which is the whole point of exposing it. A
/// declaration is assembled a creature at a time and a player wants to see what the
/// choice they are making does, so the server states the answer per candidate
/// (`docs/protocol.md`) and a client turns the card it is told to turn. The rule stays
/// here: vigilance is a keyword judgment, and a presentation that read one would be
/// deciding a rule (`AGENTS.md`).
///
/// Pure and unapplied, like every other predicate in this module. Keyword grants come
/// through the computed characteristics, so an anthem granting vigilance counts exactly
/// as a printed one; a permanent that is not on the battlefield taps nothing.
#[must_use]
pub fn attacking_taps(state: &GameState, attacker: PermanentId, db: &CardDatabase) -> bool {
    state
        .battlefield
        .iter()
        .find(|perm| perm.id == attacker)
        .is_some_and(|perm| !has_keyword(state, perm, Keyword::Vigilance, db))
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
/// given the **pairwise** evasion rules (CR 509.1b) — every restriction that can be
/// judged from one attacker/blocker pair alone:
///
/// - flying: only a creature with flying or reach may block it (CR 702.9c, 702.17b);
/// - [`CombatRestriction::CantBeBlocked`]: nothing may block it;
/// - [`CombatRestriction::CantBeBlockedBy`]: no creature of the named colour may;
/// - [`CombatRestriction::CantBeBlockedExceptBy`]: only a creature of the named subtype may;
/// - [`CombatRestriction::CantBlock`] on the *blocker*, which is a fact about the
///   blocker rather than the pair but is enforced here too, so a creature restricted
///   after blocker candidates were computed still cannot slip through.
///
/// Restrictions on *how many* creatures may block ([`permanent_has_menace`],
/// [`blocked_by_at_most_one`]) are deliberately **not** here: they cannot be judged
/// from a pair, only from the assembled selection, and live in the declare-blockers
/// legality gate.
///
/// Both ids are looked up on the battlefield; a missing permanent (a stale id)
/// yields `false`, so the caller rejects the assignment. Applying this per pair rather
/// than by trimming the candidate set is what keeps partial blocks of mixed attackers
/// expressible — a ground creature may block the ground attacker in the same
/// declaration a flyer blocks the flyer.
#[must_use]
pub fn blocker_can_block_attacker(
    state: &GameState,
    attacker: PermanentId,
    blocker: PermanentId,
    db: &CardDatabase,
) -> bool {
    if state.battlefield.iter().all(|p| p.id != attacker) {
        return false;
    }
    let Some(blk) = state.battlefield.iter().find(|p| p.id == blocker) else {
        return false;
    };
    // CR 506.3c: a creature that can't block blocks nothing, whatever it is aimed at.
    if permanent_has_restriction(state, blocker, CombatRestriction::CantBlock, db) {
        return false;
    }
    // CR 702.9c / 702.17b: only flying or reach may block a flyer.
    if permanent_has_keyword(state, attacker, Keyword::Flying, db)
        && !has_keyword(state, blk, Keyword::Flying, db)
        && !has_keyword(state, blk, Keyword::Reach, db)
    {
        return false;
    }
    // CR 509.1b: the attacker's own evasion restrictions, read through the computed
    // characteristics so a granted one restricts exactly as a printed one does.
    let blocker_colors = blk
        .printed
        .face(db)
        .map(|face| face.colors().to_vec())
        .unwrap_or_default();
    // The blocker's power and subtypes, unlike its colour, are read through the computed
    // characteristics: a pumped blocker really has escaped a power-based evasion, and
    // reading computed subtypes is already right for the day CR 613 layer 4 lands.
    let blocker_characteristics = crate::characteristics::characteristics(state, blocker, db);
    let blocker_power = blocker_characteristics.power;
    let blocker_subtypes = blocker_characteristics.subtypes;
    for restriction in permanent_restrictions(state, attacker, db) {
        match restriction {
            CombatRestriction::CantBeBlocked => return false,
            CombatRestriction::CantBeBlockedBy(color) if blocker_colors.contains(&color) => {
                return false
            }
            CombatRestriction::CantBeBlockedByPowerOrLess(max)
                if blocker_power.is_some_and(|power| power <= max) =>
            {
                return false
            }
            // The one evasion stated as a permission: a blocker without the named
            // subtype is forbidden, which is the exact inverse of the colour form's test.
            CombatRestriction::CantBeBlockedExceptBy(ref subtype)
                if !blocker_subtypes.contains(subtype) =>
            {
                return false
            }
            // Not pairwise facts, or not about being blocked at all.
            CombatRestriction::CantBeBlockedBy(_)
            | CombatRestriction::CantBeBlockedByPowerOrLess(_)
            | CombatRestriction::CantBeBlockedExceptBy(_)
            | CombatRestriction::CantAttack
            | CombatRestriction::CantBlock
            | CombatRestriction::CantBeBlockedByMoreThanOne => {}
        }
    }
    true
}

/// Whether the permanent `id` currently has the exact combat restriction
/// `restriction`, looked up by id so a legality gate holding only an id can ask.
///
/// Read through the computed restrictions (CR 613.1f), so an Aura's or a spell's
/// imposition binds exactly as a printed one does. `false` for an id no longer on the
/// battlefield.
#[must_use]
pub fn permanent_has_restriction(
    state: &GameState,
    id: PermanentId,
    restriction: CombatRestriction,
    db: &CardDatabase,
) -> bool {
    crate::characteristics::permanent_has_restriction(state, id, restriction, db)
}

/// The permanent `id`'s current combat restrictions (CR 506.3, CR 509.1b), printed and
/// granted together. The list form of [`permanent_has_restriction`], for the gates that
/// must inspect a restriction's payload rather than test for one exact value.
#[must_use]
pub fn permanent_restrictions(
    state: &GameState,
    id: PermanentId,
    db: &CardDatabase,
) -> Vec<CombatRestriction> {
    crate::characteristics::permanent_restrictions(state, id, db)
}

/// Whether the permanent `id` can't be blocked by more than one creature
/// (CR 509.1b) — the per-attacker half of the block-count *ceiling*, and the exact
/// mirror of [`permanent_has_menace`]'s floor.
///
/// Like menace this is a fact the declare-blockers gate can only judge over the
/// assembled selection, which is why it is exposed by id beside menace rather than
/// folded into [`blocker_can_block_attacker`].
#[must_use]
pub fn blocked_by_at_most_one(state: &GameState, id: PermanentId, db: &CardDatabase) -> bool {
    permanent_has_restriction(state, id, CombatRestriction::CantBeBlockedByMoreThanOne, db)
}

/// Whether the permanent `id` currently has **menace** (CR 702.110) — the
/// per-attacker half of the block-declaration restriction, looked up by id so the
/// legality gate can ask about an attacker it holds only an id for.
///
/// Read through the computed keywords (CR 613.1f), so a granted menace restricts
/// exactly as a printed one does. `false` for an id no longer on the battlefield.
#[must_use]
pub fn permanent_has_menace(state: &GameState, id: PermanentId, db: &CardDatabase) -> bool {
    crate::characteristics::permanent_has_keyword(state, id, Keyword::Menace, db)
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
    /// tests that need those keywords build their own definitions (ADR 0009).
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
            printed: fixture("walking_corpse").into(),
            controller,
            tapped,
            entered_turn,
            attacking: None,
            blocking: None,
            skips_untap: false,
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
    fn cr_302_6_measures_the_controllers_most_recent_turn_not_the_current_one() {
        // A creature that entered under seat 0 on turn 1 is restricted until seat
        // 0's *next* turn begins. Through the whole of seat 1's turn 2 it is still
        // sick, even though `state.turn` has moved on — that is the difference
        // between "the current turn" and "its controller's most recent turn", and
        // seat 0 can hold priority during turn 2 and try to tap it.
        let mut state = GameState::new_two_player();
        let sick = creature(&mut state, PlayerId(0), false, 1);
        let is_sick = |state: &GameState| {
            let perm = state.battlefield.iter().find(|p| p.id == sick).unwrap();
            has_summoning_sickness(perm, state)
        };

        assert!(is_sick(&state), "the turn it entered");

        let state = state.advance_to_next_turn();
        assert_eq!((state.turn, state.active_player), (2, PlayerId(1)));
        assert!(
            is_sick(&state),
            "still restricted throughout the opponent's turn"
        );

        let state = state.advance_to_next_turn();
        assert_eq!((state.turn, state.active_player), (3, PlayerId(0)));
        assert!(!is_sick(&state), "freed when its controller's turn begins");
    }

    #[test]
    fn cr_302_6_follows_multiplayer_rotation_around_the_table() {
        // Three seats: a creature that entered under seat 1 on seat 1's turn stays
        // restricted across both intervening seats' turns. A predicate written
        // against `state.turn` would free it one turn into the rotation.
        let state = GameState::new_multiplayer(3).advance_to_next_turn();
        assert_eq!((state.turn, state.active_player), (2, PlayerId(1)));
        let mut state = state;
        let sick = creature(&mut state, PlayerId(1), false, 2);
        let is_sick = |state: &GameState| {
            let perm = state.battlefield.iter().find(|p| p.id == sick).unwrap();
            has_summoning_sickness(perm, state)
        };
        assert!(is_sick(&state));

        for expected in [(3, PlayerId(2)), (4, PlayerId(0))] {
            state = state.advance_to_next_turn();
            assert_eq!((state.turn, state.active_player), expected);
            assert!(is_sick(&state), "still sick on turn {}", state.turn);
        }

        state = state.advance_to_next_turn();
        assert_eq!((state.turn, state.active_player), (5, PlayerId(1)));
        assert!(!is_sick(&state), "freed on its controller's next turn");
    }

    #[test]
    fn cr_302_6_is_not_lifted_by_an_opponents_extra_turn() {
        // CR 720.1: an extra turn taken by seat 0 is not seat 1's turn, so seat 1's
        // creature is still restricted during it. Turn *numbers* advance either
        // way, which is exactly why the reference point is stored per seat.
        let mut state = GameState::new_two_player().advance_to_next_turn();
        assert_eq!(state.active_player, PlayerId(1));
        let entered = state.turn;
        let sick = creature(&mut state, PlayerId(1), false, entered);
        let is_sick = |state: &GameState| {
            let perm = state.battlefield.iter().find(|p| p.id == sick).unwrap();
            has_summoning_sickness(perm, state)
        };

        // Seat 0 takes turn 3, then an extra turn 4 instead of passing to seat 1.
        let mut state = state.advance_to_next_turn();
        assert_eq!((state.turn, state.active_player), (3, PlayerId(0)));
        assert!(is_sick(&state));
        state = state.with_extra_turn(PlayerId(0)).advance_to_next_turn();
        assert_eq!(
            (state.turn, state.active_player),
            (4, PlayerId(0)),
            "the extra turn is seat 0's"
        );
        assert!(
            is_sick(&state),
            "an extra turn is not the controller's turn"
        );

        state = state.advance_to_next_turn();
        assert_eq!((state.turn, state.active_player), (5, PlayerId(1)));
        assert!(!is_sick(&state));
    }

    /// Grant `restriction` to `id` until end of turn, the way a spell or an activated
    /// ability does — so a test can assert a *granted* restriction behaves as a printed
    /// one without going through a card that happens to grant it.
    fn grant(state: &mut GameState, id: PermanentId, restriction: CombatRestriction) {
        let source = state.mint_id();
        state.static_effects.push(crate::state::StaticEffect {
            source,
            affects: crate::state::EffectAffects::SpecificPermanent(id),
            modification: crate::state::Modification::GrantRestriction(restriction),
            duration: crate::state::Duration::UntilEndOfTurn,
        });
    }

    #[test]
    fn issue_606_an_unblockable_attacker_refuses_every_blocker_printed_or_granted() {
        // CR 509.1b: "can't be blocked" is a pairwise fact, so it is enforced here and
        // it removes *every* blocker — while an ordinary attacker beside it is
        // untouched, which is what makes this a per-pair check rather than a filter on
        // the candidate set.
        let db = db();
        let mut state = GameState::new_two_player();
        let evasive = super::super::damage::tests::creature_card(
            &mut state,
            fixture("centaur_courser"),
            PlayerId(0),
            0,
        );
        let ordinary = super::super::damage::tests::creature_card(
            &mut state,
            fixture("centaur_courser"),
            PlayerId(0),
            0,
        );
        let blocker = super::super::damage::tests::creature_card(
            &mut state,
            fixture("sun_sentinel"),
            PlayerId(1),
            0,
        );

        assert!(blocker_can_block_attacker(&state, evasive, blocker, &db));
        grant(&mut state, evasive, CombatRestriction::CantBeBlocked);
        assert!(
            !blocker_can_block_attacker(&state, evasive, blocker, &db),
            "a granted restriction binds exactly as a printed one"
        );
        assert!(
            blocker_can_block_attacker(&state, ordinary, blocker, &db),
            "the unaffected neighbour is still blockable"
        );
    }

    #[test]
    fn issue_606_a_colour_restriction_removes_only_that_colour() {
        // Vine Mare's printed restriction, at the seam it lives in: the black creature
        // is refused and the green one beside it is not.
        let db = db();
        let mut state = GameState::new_two_player();
        let mare = super::super::damage::tests::creature_card(
            &mut state,
            fixture("vine_mare"),
            PlayerId(0),
            0,
        );
        let black = super::super::damage::tests::creature_card(
            &mut state,
            fixture("walking_corpse"),
            PlayerId(1),
            0,
        );
        let green = super::super::damage::tests::creature_card(
            &mut state,
            fixture("centaur_courser"),
            PlayerId(1),
            0,
        );

        assert!(!blocker_can_block_attacker(&state, mare, black, &db));
        assert!(blocker_can_block_attacker(&state, mare, green, &db));
    }

    #[test]
    fn issue_606_a_creature_that_cant_block_is_refused_against_every_attacker() {
        // CR 506.3c is a fact about the blocker, not the pair, so it is enforced here
        // as well as in the candidate set — the two gates cannot disagree.
        let db = db();
        let mut state = GameState::new_two_player();
        let attacker = super::super::damage::tests::creature_card(
            &mut state,
            fixture("centaur_courser"),
            PlayerId(0),
            0,
        );
        let silenced = super::super::damage::tests::creature_card(
            &mut state,
            fixture("sun_sentinel"),
            PlayerId(1),
            0,
        );
        let other = super::super::damage::tests::creature_card(
            &mut state,
            fixture("sun_sentinel"),
            PlayerId(1),
            0,
        );

        grant(&mut state, silenced, CombatRestriction::CantBlock);
        assert!(!blocker_can_block_attacker(&state, attacker, silenced, &db));
        assert!(blocker_can_block_attacker(&state, attacker, other, &db));
    }

    #[test]
    fn issue_606_the_block_count_ceiling_is_not_a_pairwise_fact() {
        // The ceiling is deliberately *absent* from this predicate: a single blocker
        // against a Bristling Boar is a perfectly legal pair, and only the assembled
        // selection can say whether a second one is too.
        let db = db();
        let mut state = GameState::new_two_player();
        let boar = super::super::damage::tests::creature_card(
            &mut state,
            fixture("bristling_boar"),
            PlayerId(0),
            0,
        );
        let blocker = super::super::damage::tests::creature_card(
            &mut state,
            fixture("sun_sentinel"),
            PlayerId(1),
            0,
        );

        assert!(blocked_by_at_most_one(&state, boar, &db));
        assert!(
            blocker_can_block_attacker(&state, boar, blocker, &db),
            "the count restriction is judged over the declaration, not the pair"
        );
    }

    #[test]
    fn attacking_taps_every_attacker_except_a_vigilant_one_cr_508_1f() {
        // The predicate a client is shown a declaration through: choosing this creature
        // as an attacker would turn it, and choosing that one would not. Sun Sentinel has
        // vigilance (CR 702.20b); Walking Corpse is a plain control.
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        let vigilant = super::super::damage::tests::creature_card(
            &mut state,
            crate::fixtures::fixture("sun_sentinel"),
            PlayerId(0),
            0,
        );
        let plain = super::super::damage::tests::creature_card(
            &mut state,
            crate::fixtures::fixture("walking_corpse"),
            PlayerId(0),
            0,
        );

        assert!(
            attacking_taps(&state, plain, &db),
            "attacking taps (CR 508.1f)"
        );
        assert!(
            !attacking_taps(&state, vigilant, &db),
            "vigilance attacks without tapping (CR 702.20b)"
        );
        // A permanent that is not on the battlefield taps nothing, rather than being
        // reported as an attacker that would turn.
        assert!(!attacking_taps(&state, PermanentId(9999), &db));
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
