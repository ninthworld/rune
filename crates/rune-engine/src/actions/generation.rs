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
use super::utilities::{cost_payable, is_castable_spell, is_land};

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
/// never pre-expands one action per target combination (ADR 0009 §Enumeration).
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
                    .all(|spec| !legal_targets_for_spec(spec, state, db).is_empty());
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
                        .all(|spec| !legal_targets_for_spec(spec, state, db).is_empty());
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
    for perm in &state.battlefield {
        if perm.controller != priority {
            continue;
        }
        for (index, ability) in crate::card::abilities_of(db, perm.card).iter().enumerate() {
            if let Ability::Activated { cost, .. } = ability {
                if cost_payable(cost, perm) {
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
