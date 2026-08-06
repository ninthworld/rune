//! Priority-automation support (issue #264, ADR 0010): the engine-side judgment of
//! whether the current priority holder has any *meaningful* action beyond passing.
//!
//! This is the one rules question the client is forbidden to answer
//! (`docs/design/ui-requirements.md`, "Stack, priority, and timers": "The client
//! cannot decide that a player has no meaningful response"). Deciding that a lone
//! `pass_priority` is safe depends on the seat's [`valid_actions`], the stack, and
//! timing — engine knowledge. The engine stays pure: it exposes the *predicate*
//! only; the *loop* that keeps auto-passing and the per-seat stop preferences that
//! gate it live in the room layer (ADR 0001 keeps loops, policy, and I/O out of the
//! engine).

use crate::ability::is_mana_ability;
use crate::actions::{potential_mana_pool, valid_actions, Action};
use crate::card::abilities_of_permanent;
use crate::combat::{
    attacker_candidates, attacking_defender_of, blocker_can_block_attacker, blocker_candidates_for,
    declared_attackers, defender_candidates, pending_blocker_declarer,
};
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
/// only fire where passing is a move the seat is already entitled to take (ADR 0010).
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

/// The forced combat declaration the priority holder owes whose **only legal
/// answer is the empty one**, returned as that empty declaration — or `None` when
/// the seat owes no such declaration.
///
/// The companion to [`priority_has_no_meaningful_action`] for the one window that
/// predicate deliberately refuses to judge. A combat declaration is advertised
/// *without* a `PassPriority` alongside, so the idle predicate short-circuits to
/// `false` there and any settle loop stops — correct when the seat owes a real
/// choice, but the active player is asked to declare attackers even on a turn-one
/// board with no creature at all (issue #453). This predicate answers the narrower
/// rules question: *is there any non-empty declaration this seat could legally
/// make?* When there is not, the empty declaration is not a choice, and the caller
/// may apply it as the ordinary engine action it is.
///
/// - **Attackers** (CR 508.1a): no legal attack exists when [`attacker_candidates`]
///   is empty, or when there is no opponent left to attack
///   ([`defender_candidates`]).
/// - **Blockers** (CR 509.1a): no legal block exists when no candidate blocker of
///   the player who owes the declaration ([`blocker_candidates_for`] the
///   [`pending_blocker_declarer`]) may be assigned to any attacker attacking *them*
///   — the same per-pair test the declaration's legality check applies, so evasion
///   (CR 702.9c/702.17b) counts. This covers the common shape the attacker fix
///   creates: an empty attacker declaration walks the game into the declare-blockers
///   step (CR 508.8 step-skipping is not modeled) with nothing to block.
///
/// Conservative by construction: it fires only when *every* non-empty declaration
/// is illegal, so no player is ever declared past a decision they could have made.
/// `None` when no one holds priority or the game is over. Pure over `state` and
/// `db`; the returned action is an ordinary [`Action`], so applying it is
/// indistinguishable from the player having clicked it and determinism is preserved.
///
/// Like [`priority_has_no_meaningful_action`], this is the *predicate* only — the
/// loop that acts on it, and the policy that gates it, live in the room layer
/// (ADR 0001, ADR 0010).
#[must_use]
pub fn forced_declaration_without_choice(state: &GameState, db: &CardDatabase) -> Option<Action> {
    if state.priority_holder().is_none() || state.is_over() {
        return None;
    }
    let actions = valid_actions(state, db);
    if actions
        .iter()
        .any(|a| matches!(a, Action::DeclareAttackers { .. }))
        && (attacker_candidates(state, db).is_empty() || defender_candidates(state, db).is_empty())
    {
        return Some(Action::DeclareAttackers {
            attackers: Vec::new(),
        });
    }
    if actions
        .iter()
        .any(|a| matches!(a, Action::DeclareBlockers { .. }))
        && !a_legal_block_exists(state, db)
    {
        return Some(Action::DeclareBlockers { blocks: Vec::new() });
    }
    None
}

/// Whether the player who owes the pending blocker declaration could legally block
/// *anything* (CR 509.1a): some candidate blocker of theirs may be assigned to some
/// attacker that is attacking them. `false` when no declaration is owed.
///
/// Deliberately the single-assignment legality test rather than a bare
/// candidate-set emptiness check: a multi-creature block is legal only if each of
/// its assignments is, so "no single assignment is legal" is exactly "no non-empty
/// declaration is legal".
fn a_legal_block_exists(state: &GameState, db: &CardDatabase) -> bool {
    let Some(declarer) = pending_blocker_declarer(state) else {
        return false;
    };
    let attackers: Vec<PermanentId> = declared_attackers(state)
        .into_iter()
        .filter(|&attacker| attacking_defender_of(state, attacker) == Some(declarer))
        .collect();
    blocker_candidates_for(state, declarer, db)
        .into_iter()
        .any(|blocker| {
            attackers
                .iter()
                .any(|&attacker| blocker_can_block_attacker(state, attacker, blocker, db))
        })
}

/// Give the priority seat the mana it would have if it tapped out
/// ([`potential_mana_pool`], CR 605.1), so a "castable once I tap" spell counts as a
/// real action.
///
/// The estimate is deliberately generous, and shared with the optional-cost gate so
/// there is exactly one answer in the engine to "what could this board pay for" — see
/// [`potential_mana_pool`] for why erring high is the safe direction for both.
fn float_potential_mana(state: &mut GameState, db: &CardDatabase) {
    let seat = state.priority;
    let pool = potential_mana_pool(state, seat, db);
    if let Some(player) = state.players.get_mut(seat.0) {
        player.mana_pool = pool;
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
    abilities_of_permanent(state, db, perm)
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
            printed: card.into(),
            controller,
            tapped: false,
            entered_turn: 0,
            attacking: None,
            blocking: Vec::new(),
            skips_untap: false,
            damage: 0,
            counters: Default::default(),
            attached_to: None,
            chosen_color: None,
            named_card: None,
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
    fn issue_454_a_seat_whose_only_source_is_a_summoning_sick_creature_is_idle() {
        // A Llanowar Elves cast this turn offers nothing (CR 302.6), so the seat's
        // whole offer is pass + concede: idle, and safe to auto-pass. The predicate
        // still *floats* the Elves' mana while judging (see `float_potential_mana`) —
        // a deliberate over-estimate that only ever errs toward not auto-passing —
        // but with nothing in hand there is no spell for it to make castable.
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let elves = place(&mut state, fixture("llanowar_elves"), PlayerId(0));
        let perm = state
            .battlefield
            .iter_mut()
            .find(|p| p.id == elves)
            .unwrap();
        perm.entered_turn = state.turn;

        assert!(
            !valid_actions(&state, &db()).iter().any(|a| matches!(
                a,
                Action::ActivateAbility { permanent, .. } if *permanent == elves
            )),
            "the sick creature's {{T}} mana ability is not offered"
        );
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
        let scout = state.new_instance(fixture("llanowar_elves"));
        state.players[0].hand = vec![scout];
        state.players[0].mana_pool.add(Color::Green, 1);
        assert!(!priority_has_no_meaningful_action(&state, &db()));
    }

    #[test]
    fn a_castable_after_tapping_spell_keeps_a_seat_non_idle() {
        // The potential-mana case: an untapped Forest and a creature in hand the seat
        // cannot yet afford (empty pool). The engine offers no cast until mana floats,
        // but tapping the Forest would pay for it — so the seat is NOT idle and must
        // never be auto-passed past its own play (ADR 0010, the acceptance criterion).
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        place(&mut state, fixture("forest"), PlayerId(0));
        let scout = state.new_instance(fixture("llanowar_elves")); // a {G} creature
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
    fn issue_537_a_seat_that_floated_mana_and_cast_nothing_is_idle_after_the_step_ends() {
        // The ADR 0010 stall behind issue #537. A seat taps its land, casts nothing,
        // and the step ends. Before CR 500.4 was implemented the floating mana
        // persisted forever, so `valid_actions` kept offering the mana-dependent
        // `CastSpell` and this predicate kept reporting the seat non-idle — the room
        // never auto-passed it and every seat that had ever tapped a land stalled.
        //
        // Revitalize is a {1}{W} instant with no targets, so its castability turns on
        // the pool alone and not on timing or a legal target.
        let db = db();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let plains = place(&mut state, fixture("plains"), PlayerId(0));
        let second = place(&mut state, fixture("plains"), PlayerId(0));
        let heal = state.new_instance(fixture("revitalize"));
        state.players[0].hand = vec![heal];

        // Untapped land plus an affordable-after-tapping spell: still non-idle. The
        // fix must not break the `float_potential_mana` hypothetical (ADR 0010).
        assert!(
            !priority_has_no_meaningful_action(&state, &db),
            "a spell castable once the seat taps its land is a meaningful action"
        );

        // The seat taps both lands for {W}{W} and then declines to cast.
        let state = crate::apply_action(
            &state,
            &Action::ActivateAbility {
                permanent: plains,
                index: 0,
                targets: Vec::new(),
                payment: Vec::new(),
            },
            &db,
        );
        let state = crate::apply_action(
            &state,
            &Action::ActivateAbility {
                permanent: second,
                index: 0,
                targets: Vec::new(),
                payment: Vec::new(),
            },
            &db,
        );
        assert_eq!(
            state.players[0].mana_pool.white, 2,
            "the pool is floating {{W}}{{W}}"
        );
        assert!(
            !priority_has_no_meaningful_action(&state, &db),
            "with the mana in hand the cast is a real, offered action"
        );

        // Both seats pass, ending the precombat main phase (CR 500.4).
        let after = crate::apply_action(&state, &Action::PassPriority, &db);
        let after = crate::apply_action(&after, &Action::PassPriority, &db);

        assert_eq!(after.step, Step::BeginCombat);
        assert_eq!(after.priority, PlayerId(0));
        assert_eq!(
            after.players[0].mana_pool.total(),
            0,
            "the pool emptied when the phase ended"
        );
        assert!(
            priority_has_no_meaningful_action(&after, &db),
            "with its land tapped and its pool emptied the seat has nothing but a \
             pass — it must auto-pass rather than stall (ADR 0010)"
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
        // Three blue sources and Cancel ({1}{U}{U} instant), plus a spell on the
        // stack for the counter to legally target.
        place(&mut state, fixture("island"), PlayerId(1));
        place(&mut state, fixture("island"), PlayerId(1));
        place(&mut state, fixture("island"), PlayerId(1));
        let negation = state.new_instance(fixture("cancel"));
        state.players[1].hand = vec![negation];
        let boar = state.new_instance(fixture("onakke_ogre"));
        let sid = crate::stack::StackId(state.mint_id());
        state.stack.push(crate::stack::StackObject {
            paid: Default::default(),
            id: sid,
            controller: PlayerId(0),
            kind: crate::stack::StackObjectKind::Spell {
                card: boar,
                mode: None,
                x: None,
            },
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

    /// A two-player game parked in `step` with `priority` holding, ready to owe a
    /// combat declaration. Turn 2, so a permanent placed by [`place`] (which records
    /// `entered_turn: 0`) is free of summoning sickness.
    fn combat_state(step: Step, priority: PlayerId) -> GameState {
        let mut state = GameState::new_two_player();
        state.turn = 2;
        state.step = step;
        state.priority = priority;
        state
    }

    #[test]
    fn issue_453_declare_attackers_with_no_candidates_is_the_empty_declaration() {
        // The observed bug: the active player is asked to declare attackers on a
        // board with no creature at all. There is no legal non-empty declaration, so
        // the predicate hands back the empty one for the room to apply.
        let state = combat_state(Step::DeclareAttackers, PlayerId(0));
        assert!(
            !priority_has_no_meaningful_action(&state, &db()),
            "the forced window still short-circuits the idle predicate"
        );
        assert_eq!(
            forced_declaration_without_choice(&state, &db()),
            Some(Action::DeclareAttackers {
                attackers: Vec::new()
            })
        );
    }

    #[test]
    fn issue_453_a_seat_with_a_legal_attacker_keeps_its_forced_choice() {
        // The safety property: one eligible attacker is a real decision, and must
        // never be declared away. A tapped creature is not eligible, so the same
        // board with its only creature tapped is auto-resolvable again.
        let db = db();
        let mut state = combat_state(Step::DeclareAttackers, PlayerId(0));
        let bear = place(&mut state, fixture("walking_corpse"), PlayerId(0));
        assert_eq!(forced_declaration_without_choice(&state, &db), None);

        state
            .battlefield
            .iter_mut()
            .find(|p| p.id == bear)
            .unwrap()
            .tapped = true;
        assert_eq!(
            forced_declaration_without_choice(&state, &db),
            Some(Action::DeclareAttackers {
                attackers: Vec::new()
            })
        );
    }

    #[test]
    fn issue_453_declare_blockers_with_nothing_to_block_is_the_empty_declaration() {
        // CR 508.8 step-skipping is not modeled, so an empty attacker declaration
        // still walks the game into the declare-blockers step. The defender has an
        // untapped creature but no attacker is attacking them: no legal block exists.
        let db = db();
        let mut state = combat_state(Step::DeclareBlockers, PlayerId(1));
        state.attackers_declared = true;
        place(&mut state, fixture("walking_corpse"), PlayerId(1));
        assert_eq!(
            forced_declaration_without_choice(&state, &db),
            Some(Action::DeclareBlockers { blocks: Vec::new() })
        );
    }

    #[test]
    fn issue_453_a_seat_with_a_legal_blocker_keeps_its_forced_choice() {
        // The safety property for blockers: with an attacker attacking them and an
        // untapped creature to block with, the defender owes a real choice.
        let db = db();
        let mut state = combat_state(Step::DeclareBlockers, PlayerId(1));
        state.attackers_declared = true;
        let attacker = place(&mut state, fixture("walking_corpse"), PlayerId(0));
        state
            .battlefield
            .iter_mut()
            .find(|p| p.id == attacker)
            .unwrap()
            .attacking = Some(crate::combat::AttackTarget::Player(PlayerId(1)));
        place(&mut state, fixture("walking_corpse"), PlayerId(1));
        assert_eq!(forced_declaration_without_choice(&state, &db), None);
    }

    #[test]
    fn issue_453_an_ordinary_priority_window_owes_no_declaration() {
        // Outside a declare step — and on a state with no priority holder or a
        // finished game — there is nothing to auto-resolve.
        let db = db();
        let state = combat_state(Step::PrecombatMain, PlayerId(0));
        assert_eq!(forced_declaration_without_choice(&state, &db), None);
        assert_eq!(
            forced_declaration_without_choice(&GameState::default(), &db),
            None
        );

        let mut over = combat_state(Step::DeclareAttackers, PlayerId(0));
        over.players[1].has_lost = true;
        assert!(over.is_over());
        assert_eq!(forced_declaration_without_choice(&over, &db), None);
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
