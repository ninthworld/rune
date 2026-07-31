//! Action generation — enumeration of legal actions from game state.

use crate::ability::Ability;
use crate::card_type::CardType;
use crate::commander::commander_tax_cost;
use crate::mana::parse_mana_cost;
use crate::phase::Step;
use crate::player::MAX_HAND_SIZE;
use crate::state::GameState;
use crate::CardDatabase;

use super::definition::Action;
use super::targeting::legal_targets_for_spec;
use super::utilities::{cost_payable, is_castable_spell, is_land, tap_cost_is_summoning_sick};

/// Enumerate the actions legal for the player who currently holds priority.
///
/// Pull-based and pure: computed fresh from `state`, never cached on it. The
/// priority holder may always pass; may play a land, cast a spell at its legal
/// timing (instants any time they hold priority, everything else at sorcery
/// speed — CR 117.1a), or (for permanents they control) activate abilities when
/// the relevant timing and cost conditions hold. A state with no valid priority
/// holder offers nothing.
///
/// A targeted ability is advertised **once**, in its requirement form (empty
/// [`Action::ActivateAbility::targets`]); its per-slot legal candidate sets are
/// obtained separately via [`crate::target_requirements`]. The generator therefore
/// never pre-expands one action per target combination (ADR 0004 §Enumeration).
#[must_use]
pub fn valid_actions(state: &GameState, db: &CardDatabase) -> Vec<Action> {
    if state.priority_holder().is_none() {
        return Vec::new();
    }
    // CR 104.2a: once the game is over nothing is legal — the terminal state offers
    // no actions and [`crate::apply_action`] rejects any that are submitted.
    if state.is_over() {
        return Vec::new();
    }
    let priority = state.priority;

    // Pre-game London mulligan (CR 103.5): while the mulligan phase is in progress
    // the only choices are the deciding seat's keep/mulligan, and turn 1 has not
    // begun — no lands, spells, abilities, or priority passes are offered until
    // every player has kept (see [`crate::mulligan`]). Concede (CR 104.3a) is still
    // offered — a player may leave even during the mulligan.
    if let Some(mut actions) = crate::mulligan::mulligan_actions(state) {
        offer_concede(&mut actions);
        return actions;
    }

    // Commander return decision (CR 903.9a): when the priority holder's commander
    // is sitting in a graveyard or exile awaiting the choice, that decision is the
    // only thing they may take — offered like the cleanup discard and combat
    // declarations, a discrete choice rather than something taken with priority.
    // Both accept and decline are always available, so any priority automation can
    // pick decline and move on (it never stalls). Not a replacement effect: the
    // commander already moved; this offers to move it again to the command zone.
    if let Some(commander) = state
        .players
        .get(priority.0)
        .and_then(|p| p.commander.as_ref())
    {
        if commander.return_pending {
            let card = crate::id::CardInstance {
                id: commander.instance,
                card: commander.card,
            };
            let mut actions = vec![
                Action::ReturnCommanderToCommandZone { card },
                Action::DeclineCommanderReturn { card },
            ];
            offer_concede(&mut actions);
            return actions;
        }
    }

    // Cleanup step: no player receives priority (CR 514.3). The only choice is
    // the active player discarding down to the maximum hand size (CR 514.1),
    // offered as a select-from-zone choice — one [`Action::Discard`] per card in
    // hand — and only while they are over the limit. Everything else (passing,
    // lands, spells, abilities) is unavailable here — except conceding (CR 104.3a).
    if state.step == Step::Cleanup {
        let mut actions = Vec::new();
        if priority == state.active_player {
            if let Some(player) = state.players.get(priority.0) {
                if player.hand.len() > MAX_HAND_SIZE {
                    for &card in &player.hand {
                        actions.push(Action::Discard { card });
                    }
                    offer_concede(&mut actions);
                }
            }
        }
        return actions;
    }

    // Combat declarations are turn-based player choices, offered like the cleanup
    // discard rather than taken with priority: while a declaration is owed, the
    // declaring player's only action is the declaration itself (no pass, no
    // spells), and no other player acts. The declaration is advertised once in its
    // empty requirement form; its multi-select candidates come from
    // [`crate::attacker_candidates`] / [`crate::blocker_candidates`] (see [`crate::target_requirements`]
    // for how the requirement is surfaced) and a filled selection is checked in
    // [`crate::apply_action`].
    if state.step == Step::DeclareAttackers && !state.attackers_declared {
        // CR 508.1: the active player declares attackers.
        return if priority == state.active_player {
            let mut actions = vec![Action::DeclareAttackers {
                attackers: Vec::new(),
            }];
            offer_concede(&mut actions);
            actions
        } else {
            Vec::new()
        };
    }
    if state.step == Step::DeclareBlockers
        && crate::combat::pending_blocker_declarer(state).is_some()
    {
        // CR 509.1: each attacked player declares blockers for the attackers
        // attacking them, in APNAP order (issue #344). Only the player who owes the
        // next declaration is offered it.
        return if Some(priority) == crate::combat::pending_blocker_declarer(state) {
            let mut actions = vec![Action::DeclareBlockers { blocks: Vec::new() }];
            offer_concede(&mut actions);
            actions
        } else {
            Vec::new()
        };
    }
    if state.step == Step::DeclareBlockers && crate::combat::pending_damage_order(state).is_some() {
        // CR 510.1 (issue #346): once every blocker declaration is in, the attacking
        // player orders each multi-blocked attacker's blockers before combat damage.
        return if Some(priority) == crate::combat::pending_damage_order(state) {
            let mut actions = vec![Action::OrderCombatDamage { orders: Vec::new() }];
            offer_concede(&mut actions);
            actions
        } else {
            Vec::new()
        };
    }

    let mut actions = vec![Action::PassPriority];

    // Sorcery-speed: the active player, in a main phase, with an empty stack.
    let sorcery_speed = priority == state.active_player
        && matches!(state.step, Step::PrecombatMain | Step::PostcombatMain)
        && state.stack.is_empty();

    if let Some(player) = state.players.get(priority.0) {
        // Play a land: at sorcery speed, one per turn.
        if sorcery_speed && !state.land_played {
            for &card in &player.hand {
                if is_land(db, card.card) {
                    actions.push(Action::PlayLand { card });
                }
            }
        }

        // Cast a spell from hand payable from the current pool, at the correct
        // timing. A land is played, not cast (CR 116.2a); every other card type
        // is cast as a spell. An instant may be cast whenever its controller has
        // priority (CR 117.1a); every other spell — sorcery (CR 304.1), artifact,
        // enchantment (CR 307.1), creature — is bound by the sorcery-speed gate
        // above (the active player, a main phase, an empty stack). Only a cost
        // payable from the current pool ([`crate::ManaPool::can_pay`]) is offered.
        for &card in &player.hand {
            let Some(data) = db.card(card.card) else {
                continue;
            };
            if !is_castable_spell(data) {
                continue;
            }
            // CR 117.1a: an instant ignores the sorcery-speed gate; every other
            // spell is bound by it.
            let timing_ok = data.has_type(CardType::Instant) || sorcery_speed;
            if timing_ok && player.mana_pool.can_pay(&parse_mana_cost(&data.mana_cost)) {
                // A targeted spell is offered only when *every* target slot has at
                // least one legal candidate (CR 601.2c — a spell that can't choose
                // legal targets can't be cast; for an Aura this is the CR 303.4c
                // "no legal object to enchant" rule). A slot's candidates come from
                // the same per-slot enumeration abilities use, so this stays O(N)
                // per slot and never forms the cartesian product.
                let castable = data
                    .cast_target_specs()
                    .into_iter()
                    .all(|spec| !legal_targets_for_spec(spec, state, priority, db).is_empty());
                if castable {
                    actions.push(Action::CastSpell {
                        card,
                        targets: Vec::new(),
                    });
                }
            }
        }

        // Cast the commander from the command zone (CR 903.8). It is offered as a
        // normal [`Action::CastSpell`] naming the command-zone copy — the same
        // stack object and resolution path as a hand cast, never a parallel casting
        // pipeline — subject to the same timing (instant vs. sorcery speed) and to
        // its cost *plus the commander tax*: {2} generic for each previous cast from
        // the command zone this game. Payability is checked against that taxed cost,
        // so the offer and the charge (in `apply_cast_spell`) always agree.
        if let Some(commander) = &player.commander {
            for &card in &player.command {
                let Some(data) = db.card(card.card) else {
                    continue;
                };
                if !is_castable_spell(data) {
                    continue;
                }
                let timing_ok = data.has_type(CardType::Instant) || sorcery_speed;
                let cost = commander_tax_cost(&parse_mana_cost(&data.mana_cost), commander.casts);
                if timing_ok && player.mana_pool.can_pay(&cost) {
                    let castable = data
                        .cast_target_specs()
                        .into_iter()
                        .all(|spec| !legal_targets_for_spec(spec, state, priority, db).is_empty());
                    if castable {
                        actions.push(Action::CastSpell {
                            card,
                            targets: Vec::new(),
                        });
                    }
                }
            }
        }
    }

    // Activate abilities of permanents the priority holder controls. A targeting
    // ability is offered once with no targets filled in — the requirement form —
    // never once per legal target (see [`crate::target_requirements`] for the O(N)-per-
    // slot candidate enumeration and the combinatorial guard).
    //
    // CR 302.6: a creature that has not been under its controller's control since
    // their most recent turn began can't have an ability with `{T}` in its cost
    // activated. CR 605.3a makes no exception for mana abilities, so a freshly cast
    // Llanowar Elves offers nothing until its controller's next turn; haste
    // (CR 702.10b) lifts the restriction. Non-creature permanents are never sick, so
    // a land played this turn still taps for mana.
    for perm in &state.battlefield {
        if perm.controller != priority {
            continue;
        }
        for (index, ability) in crate::card::abilities_of(db, perm.card).iter().enumerate() {
            if let Ability::Activated { cost, .. } = ability {
                if tap_cost_is_summoning_sick(state, perm, cost, db) {
                    continue;
                }
                if cost_payable(state, cost, perm) {
                    actions.push(Action::ActivateAbility {
                        permanent: perm.id,
                        index,
                        targets: Vec::new(),
                    });
                }
            }
        }
    }

    offer_concede(&mut actions);
    actions
}

/// Append the always-available concede action (CR 104.3a) to `actions`. Called at
/// every point [`valid_actions`] returns a non-empty offer to the acting seat, so
/// a player may leave the game regardless of phase, step, or which special choice
/// is currently owed.
pub(crate) fn offer_concede(actions: &mut Vec<Action>) {
    actions.push(Action::Concede);
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::card::Keyword;
    use crate::fixtures::{fixture, id_in};
    use crate::id::{CardId, PermanentId, PlayerId};
    use crate::state::{Duration, EffectAffects, Modification, Permanent, StaticEffect};

    /// The bundled card database, for tests that need oracle data.
    fn db() -> CardDatabase {
        CardDatabase::bundled().unwrap()
    }

    /// Put a permanent of `card` on the battlefield under player 0 (untapped),
    /// recorded as having entered on turn `entered_turn`.
    fn place(state: &mut GameState, card: CardId, entered_turn: u32) -> PermanentId {
        place_for(state, card, PlayerId(0), entered_turn)
    }

    /// [`place`], but under an explicit controller.
    fn place_for(
        state: &mut GameState,
        card: CardId,
        controller: PlayerId,
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

    /// Put `state` at its active player's precombat main, holding priority.
    fn at_main_phase(mut state: GameState) -> GameState {
        state.step = Step::PrecombatMain;
        state.priority = state.active_player;
        state
    }

    /// A two-player game at player 0's precombat main on turn `turn`, reached by
    /// walking real turns — so `turn` must be one player 0 actually takes (odd, in
    /// an unmodified two-player rotation).
    fn main_phase(turn: u32) -> GameState {
        let mut state = GameState::new_two_player();
        while state.turn < turn {
            state = state.advance_to_next_turn();
        }
        assert_eq!(
            (state.turn, state.active_player),
            (turn, PlayerId(0)),
            "fixture turn must belong to player 0"
        );
        at_main_phase(state)
    }

    /// Whether `actions` offers an activation of `permanent`'s ability `index`.
    fn offers_activation(actions: &[Action], permanent: PermanentId, index: usize) -> bool {
        actions.iter().any(|a| {
            matches!(a, Action::ActivateAbility { permanent: p, index: i, .. }
                if *p == permanent && *i == index)
        })
    }

    #[test]
    fn cr_302_6_a_creature_that_entered_this_turn_offers_no_tap_ability() {
        // CR 302.6 / 605.3a (issue #454): Llanowar Elves cast this turn may not
        // activate its `{T}: Add {G}` — being a *mana* ability exempts it from
        // nothing. On its controller's next turn the same permanent offers it.
        //
        // The turns are walked, not assigned: the restriction lasts until the
        // *controller's* next turn begins, which in a two-player rotation is two
        // turn boundaries away, and a test that only bumped `state.turn` would call
        // the opponent's turn the controller's own.
        let db = db();
        let mut state = main_phase(3);
        let elves = place(&mut state, fixture("llanowar_elves"), 3);
        assert!(
            !offers_activation(&valid_actions(&state, &db), elves, 0),
            "a creature that entered this turn offers no {{T}} mana ability"
        );

        state = at_main_phase(state.advance_to_next_turn());
        assert_eq!((state.turn, state.active_player), (4, PlayerId(1)));
        // Player 0 may hold priority at instant speed during their opponent's turn,
        // so their offer set is the one under test here.
        state.priority = PlayerId(0);
        assert!(
            !offers_activation(&valid_actions(&state, &db), elves, 0),
            "the restriction lasts through the opponent's whole turn"
        );

        state = at_main_phase(state.advance_to_next_turn());
        assert_eq!((state.turn, state.active_player), (5, PlayerId(0)));
        assert!(
            offers_activation(&valid_actions(&state, &db), elves, 0),
            "on its controller's next turn the same creature taps freely"
        );
    }

    #[test]
    fn cr_302_6_holds_across_multiplayer_rotation_for_a_non_active_controller() {
        // Three seats. Seat 1's Llanowar Elves entered on seat 1's own turn 2; it
        // stays restricted through seat 2's turn 3 and seat 0's turn 4 — with seat 1
        // holding priority at instant speed each time — and is offered only once
        // seat 1's turn 5 begins.
        let db = db();
        let mut state = GameState::new_multiplayer(3).advance_to_next_turn();
        assert_eq!((state.turn, state.active_player), (2, PlayerId(1)));
        let elves = place_for(&mut state, fixture("llanowar_elves"), PlayerId(1), 2);
        let mut state = at_main_phase(state);
        assert!(!offers_activation(&valid_actions(&state, &db), elves, 0));

        for expected in [(3, PlayerId(2)), (4, PlayerId(0))] {
            state = at_main_phase(state.advance_to_next_turn());
            assert_eq!((state.turn, state.active_player), expected);
            state.priority = PlayerId(1);
            assert!(
                !offers_activation(&valid_actions(&state, &db), elves, 0),
                "still restricted on turn {}",
                state.turn
            );
        }

        state = at_main_phase(state.advance_to_next_turn());
        assert_eq!((state.turn, state.active_player), (5, PlayerId(1)));
        assert!(
            offers_activation(&valid_actions(&state, &db), elves, 0),
            "offered once its own controller's next turn begins"
        );
    }

    #[test]
    fn cr_702_10b_haste_exempts_a_creature_from_the_tap_ability_restriction() {
        // CR 702.10b: haste lifts the CR 302.6 restriction, so a hasty Llanowar
        // Elves taps for mana the turn it entered. The keyword is *granted* here, to
        // prove the gate reads computed characteristics (CR 613.1f) rather than only
        // the printed keyword list.
        let db = db();
        let mut state = main_phase(3);
        let elves = place(&mut state, fixture("llanowar_elves"), 3);
        assert!(!offers_activation(&valid_actions(&state, &db), elves, 0));

        state.static_effects.push(StaticEffect {
            source: 100,
            affects: EffectAffects::SpecificPermanent(elves),
            modification: Modification::GrantKeyword(Keyword::Haste),
            duration: Duration::UntilEndOfTurn,
        });
        assert!(
            offers_activation(&valid_actions(&state, &db), elves, 0),
            "haste exempts the creature from the summoning-sickness restriction"
        );
    }

    #[test]
    fn cr_302_6_a_land_played_this_turn_still_taps_for_mana() {
        // Summoning sickness is a *creature* restriction: a Forest played this turn
        // taps for {G} exactly as one that has been in play for ages.
        let db = db();
        let mut state = main_phase(1);
        let forest = place(&mut state, fixture("forest"), 1);
        assert!(
            offers_activation(&valid_actions(&state, &db), forest, 0),
            "a non-creature permanent is never summoning sick"
        );
    }

    #[test]
    fn cr_302_6_restricts_only_costs_containing_the_tap_symbol() {
        // The gate is scoped to `{T}` (CR 302.6 names `{T}` and `{Q}`, no more): a
        // summoning-sick creature's *cost-free* activated ability is still offered.
        let json = r#"[
            {"schema_version":1,"functional_id":"test_chanter","name":"Test Chanter",
             "types":["creature"],"mana_cost":"{G}","power":1,"toughness":1,
             "abilities":[{"type":"activated","cost":[{"kind":"tap"}],
                           "effects":[{"kind":"add_mana","color":"green","amount":1}]},
                          {"type":"activated","cost":[],
                           "effects":[{"kind":"add_mana","color":"green","amount":1}]}]}
        ]"#;
        let db = CardDatabase::from_json(json).unwrap();
        let mut state = main_phase(3);
        let chanter = place(&mut state, id_in(&db, "test_chanter"), 3);

        let actions = valid_actions(&state, &db);
        assert!(
            !offers_activation(&actions, chanter, 0),
            "the {{T}}-cost ability is withheld"
        );
        assert!(
            offers_activation(&actions, chanter, 1),
            "the cost-free ability is unaffected by summoning sickness"
        );
    }
}
