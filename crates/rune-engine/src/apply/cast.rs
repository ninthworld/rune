use super::*;
use crate::ability::{is_mana_ability, Ability, Cost, Effect, PlayerRef, Target};
use crate::card::{abilities_of, apply_enters_replacements};
use crate::commander::commander_tax_cost;
use crate::id::{CardInstance, PermanentId, PlayerId};
use crate::mana::parse_mana_cost;
use crate::state::{Duration, EffectAffects, Modification, Permanent, StaticEffect};

/// Play a land from the active player's hand onto the battlefield. Not via the
/// stack (CR 116.2a); a fresh [`PermanentId`] is minted on entry while the
/// card's [`crate::CardInstanceId`] carries over unchanged.
pub(crate) fn apply_play_land(state: &mut GameState, card: CardInstance, db: &CardDatabase) {
    let controller = state.priority;
    {
        let Some(player) = state.players.get_mut(controller.0) else {
            return;
        };
        let Some(pos) = player.hand.iter().position(|&c| c.id == card.id) else {
            return;
        };
        player.hand.remove(pos);
    }
    let id = state.mint_id();
    let entered_turn = state.turn;
    let mut permanent = Permanent {
        id: PermanentId(id),
        instance: card.id,
        card: card.card,
        controller,
        tapped: false,
        entered_turn,
        attacking: None,
        blocking: None,
        damage: 0,
        counters: Default::default(),
        // A land is played directly, never attached to anything (CR 305).
        attached_to: None,
    };
    // CR 614.1c/614.12: apply the land's own enters-the-battlefield replacements
    // (e.g. a tapland's "enters tapped") as it enters, so it is tapped the instant
    // it is on the battlefield — no untapped window to tap for mana this turn.
    apply_enters_replacements(db, &mut permanent);
    state.battlefield.push(permanent);
    state.land_played = true;
}

/// Activate ability `index` of `permanent`, paying its costs. A mana ability
/// resolves immediately without using the stack or changing priority (CR 605.3);
/// any other ability goes on the stack and the caster retains priority.
pub(crate) fn apply_activate_ability(
    state: &mut GameState,
    permanent: PermanentId,
    index: usize,
    targets: &[Target],
    db: &CardDatabase,
) {
    let Some(perm) = state.battlefield.iter().find(|p| p.id == permanent) else {
        return;
    };
    let controller = perm.controller;
    let card = perm.card;
    let Some(ability) = abilities_of(db, card).get(index).cloned() else {
        return;
    };
    let Ability::Activated { cost, effects } = &ability else {
        return;
    };

    // Pay the costs.
    for c in cost {
        match c {
            Cost::Tap => {
                if let Some(p) = state.battlefield.iter_mut().find(|p| p.id == permanent) {
                    p.tapped = true;
                }
            }
        }
    }

    if is_mana_ability(&ability) {
        // Mana ability: resolve now, no stack object, priority unchanged.
        for effect in effects {
            apply_effect(state, effect, controller);
        }
    } else {
        let id = state.mint_id();
        state.stack.push(StackObject {
            id: StackId(id),
            controller,
            kind: StackObjectKind::Ability {
                source: permanent,
                effects: effects.clone(),
            },
            // The targets chosen for this activation (CR 601.2c), already
            // validated against freshly computed legal sets in `action_is_legal`
            // and re-checked once more on resolution (CR 608.2b, the resolve
            // path). Empty for a non-targeting ability.
            targets: targets.to_vec(),
        });
        state.consecutive_passes = 0;
    }
}

/// Cast a spell of any castable type: pay its mana cost from the caster's pool,
/// move the card from hand onto the stack, and reset the pass count (the caster
/// keeps priority). Type-agnostic — the card's types decide only how it *resolves*
/// (a permanent enters the battlefield, an instant/sorcery goes to the graveyard,
/// CR 608.3), routed in [`resolve_stack_object`]; timing legality (instant vs.
/// sorcery speed, CR 117.1a) is enforced upstream in [`crate::valid_actions`].
pub(crate) fn apply_cast_spell(
    state: &mut GameState,
    card: CardInstance,
    targets: &[Target],
    db: &CardDatabase,
) {
    let controller = state.priority;
    let Some(data) = db.card(card.card) else {
        return;
    };
    let base = parse_mana_cost(&data.mana_cost);
    // A commander may be cast from the command zone (CR 903.8); anything else is
    // cast from hand. Detect which zone this instance is in so the cost carries the
    // commander tax and the card is removed from the right pile.
    let from_command = state
        .players
        .get(controller.0)
        .is_some_and(|p| p.command.iter().any(|c| c.id == card.id));
    let cost = if from_command {
        let casts = state
            .players
            .get(controller.0)
            .and_then(|p| p.commander.as_ref())
            .map_or(0, |c| c.casts);
        commander_tax_cost(&base, casts)
    } else {
        base
    };
    {
        let Some(player) = state.players.get_mut(controller.0) else {
            return;
        };
        let Some(new_pool) = player.mana_pool.pay(&cost) else {
            return;
        };
        if from_command {
            let Some(pos) = player.command.iter().position(|&c| c.id == card.id) else {
                return;
            };
            player.command.remove(pos);
            // CR 903.8: each cast from the command zone raises the tax for the next.
            if let Some(commander) = player.commander.as_mut() {
                commander.casts += 1;
            }
        } else {
            let Some(pos) = player.hand.iter().position(|&c| c.id == card.id) else {
                return;
            };
            player.hand.remove(pos);
        }
        player.mana_pool = new_pool;
    }
    let id = state.mint_id();
    state.stack.push(StackObject {
        id: StackId(id),
        controller,
        kind: StackObjectKind::Spell { card },
        // The targets chosen as part of casting this spell (CR 601.2c), already
        // validated against freshly computed legal sets in `action_is_legal` and
        // re-checked once more on resolution (CR 608.2b). Empty for a spell that
        // targets nothing.
        targets: targets.to_vec(),
    });
    state.record_event(GameEvent::SpellCast {
        player: controller,
        card,
    });
    state.consecutive_passes = 0;
}

/// Apply a single [`Effect`] to `state` on behalf of `controller`.
pub(crate) fn apply_effect(state: &mut GameState, effect: &Effect, controller: PlayerId) {
    if state.players.get(controller.0).is_none() {
        return;
    }
    match effect {
        Effect::AddMana { color, amount } => {
            if let Some(player) = state.players.get_mut(controller.0) {
                player.mana_pool.add(*color, *amount);
            }
        }
        Effect::AddColorlessMana { amount } => {
            if let Some(player) = state.players.get_mut(controller.0) {
                player.mana_pool.add_colorless(*amount);
            }
        }
        Effect::DrawCard { count } => {
            // Routes each draw through `Player::draw`, so a card-draw effect that
            // empties the library also flags the decking loss (CR 704.5c). Only the
            // cards that actually moved are logged (an empty-library draw adds none).
            let mut drawn = 0u32;
            for _ in 0..*count {
                let moved = state
                    .players
                    .get_mut(controller.0)
                    .is_some_and(|player| player.draw());
                if moved {
                    drawn += 1;
                }
            }
            if drawn > 0 {
                state.record_event(GameEvent::CardsDrawn {
                    player: controller,
                    count: drawn,
                });
            }
        }
        // CR 119.3: the referenced player gains life. `Controller` is "you", the
        // one player fetched above; other refs are added as effects need them.
        Effect::GainLife {
            player_ref: PlayerRef::Controller,
            amount,
        } => {
            state.change_life(controller, i32::try_from(*amount).unwrap_or(i32::MAX));
        }
        // CR 119.3: the referenced player loses life; a drop to 0 or less feeds
        // the zero-life state-based action (CR 704.5a) in the SBA loop.
        Effect::LoseLife {
            player_ref: PlayerRef::Controller,
            amount,
        } => {
            state.change_life(controller, -i32::try_from(*amount).unwrap_or(i32::MAX));
        }
        // A targeting effect: its subject is a chosen target, not the controller,
        // so it is applied via [`apply_targeted_effect`] and is a no-op here.
        Effect::Tap { .. }
        | Effect::CounterSpell { .. }
        | Effect::DealDamage { .. }
        | Effect::Destroy { .. }
        | Effect::PutCounters { .. }
        | Effect::Pump { .. }
        | Effect::GrantKeyword { .. } => {}
    }
}

/// Apply a targeting [`Effect`] to its already-legality-checked chosen
/// [`Target`], on behalf of `controller`.
///
/// The caller (the resolve path) is responsible for re-checking the target's
/// legality first (CR 608.2b) and only invoking this for a target that is still
/// legal; a mismatched target-value kind is a no-op here. Effects with an
/// implicit subject never reach this function — they route through
/// [`apply_effect`].
pub(crate) fn apply_targeted_effect(
    state: &mut GameState,
    effect: &Effect,
    target: Target,
    _controller: PlayerId,
    db: &CardDatabase,
) {
    match effect {
        Effect::Tap { .. } => {
            if let Target::Permanent(id) = target {
                if let Some(perm) = state.battlefield.iter_mut().find(|p| p.id == id) {
                    perm.tapped = true;
                }
            }
        }
        // Counter the targeted spell (CR 701.5a): remove it from the stack without
        // resolving and put its card into its owner's graveyard. The caller has
        // already re-checked that the target is still a spell on the stack (CR
        // 608.2b); ownership apart from control is not tracked yet, so the countered
        // spell's controller stands in as its owner.
        Effect::CounterSpell { .. } => {
            if let Target::Spell(id) = target {
                if let Some(pos) = state.stack.iter().position(|o| o.id == id) {
                    let countered = state.stack.remove(pos);
                    if let StackObjectKind::Spell { card } = countered.kind {
                        let owner = countered.controller;
                        if let Some(player) = state.players.get_mut(owner.0) {
                            player.graveyard.push(card);
                        }
                        state.record_event(GameEvent::SpellCountered {
                            player: owner,
                            card,
                        });
                    }
                }
            }
        }
        // Deal damage to the chosen target (CR 120.3): to a creature it is marked
        // (CR 120.3d) for the lethal-damage SBA (CR 704.5g); to a player it is
        // life loss (CR 120.3a) feeding the zero-life SBA (CR 704.5a). Both seams
        // record the damage (including nonlethal) as a `DamageDealt` event.
        Effect::DealDamage { amount, .. } => match target {
            Target::Permanent(id) => {
                state.mark_damage_on_permanent(id, *amount);
            }
            Target::Player(seat) => {
                state.deal_damage_to_player(seat, *amount);
            }
            Target::Card(_) | Target::Spell(_) => {}
        },
        // Destroy the targeted permanent (CR 701.7): move it to its owner's
        // graveyard through the one creature-death seam
        // ([`GameState::destroy_permanent`], CR 700.4) — the same path lethal damage
        // uses in the SBA loop, so this death fires the dies trigger (CR 603.6c) and
        // logs a `permanent_died` identically. Regeneration is out of scope.
        Effect::Destroy { .. } => {
            if let Target::Permanent(id) = target {
                state.destroy_permanent(id, db);
            }
        }
        // Put counters on the targeted permanent (CR 122). Current power/toughness
        // folds `+1/+1` / `-1/-1` counters in on demand (CR 613.7c), so a `-1/-1`
        // counter can turn lethal by lowering toughness to at or below marked
        // damage; the SBA loop then destroys it (CR 704.5g).
        Effect::PutCounters { counter, count, .. } => {
            if let Target::Permanent(id) = target {
                if let Some(perm) = state.battlefield.iter_mut().find(|p| p.id == id) {
                    *perm.counters.entry(*counter).or_insert(0) += *count;
                }
            }
        }
        // Pump the targeted creature until end of turn (CR 514.2): add a
        // timestamped CR 613 layer-7c power/toughness modifier keyed to that one
        // permanent, with an `UntilEndOfTurn` duration the cleanup step removes.
        // The timestamp is a freshly minted, strictly increasing object id
        // (CR 613.7), so a second pump this turn stacks after the first. The
        // modifier folds into computed P/T on demand — nothing is written onto the
        // permanent — so removing it at cleanup reverts the value with nothing to
        // invalidate (ADR 0010). The caller has re-checked the target is still a
        // creature (CR 608.2b); a permanent that has since left is skipped.
        Effect::Pump {
            power, toughness, ..
        } => {
            if let Target::Permanent(id) = target {
                if state.battlefield.iter().any(|p| p.id == id) {
                    let source = state.mint_id();
                    state.static_effects.push(StaticEffect {
                        source,
                        affects: EffectAffects::SpecificPermanent(id),
                        modification: Modification::PowerToughness {
                            power: *power,
                            toughness: *toughness,
                        },
                        duration: Duration::UntilEndOfTurn,
                    });
                }
            }
        }
        // Grant the targeted creature a keyword until end of turn (CR 514.2): add a
        // CR 613 layer-6 keyword grant keyed to that one permanent, with an
        // `UntilEndOfTurn` duration the cleanup step removes (CR 613.1f). The grant
        // folds into the target's computed keyword set on demand — nothing is written
        // onto the permanent — so removing it at cleanup reverts the value with
        // nothing to invalidate (ADR 0010). A duplicate grant is redundant, not
        // additive. The caller has re-checked the target is still a creature
        // (CR 608.2b); a permanent that has since left is skipped.
        Effect::GrantKeyword { keyword, .. } => {
            if let Target::Permanent(id) = target {
                if state.battlefield.iter().any(|p| p.id == id) {
                    let source = state.mint_id();
                    state.static_effects.push(StaticEffect {
                        source,
                        affects: EffectAffects::SpecificPermanent(id),
                        modification: Modification::GrantKeyword(*keyword),
                        duration: Duration::UntilEndOfTurn,
                    });
                }
            }
        }
        // Implicit-subject effects do not target; they never reach here.
        Effect::AddMana { .. }
        | Effect::AddColorlessMana { .. }
        | Effect::DrawCard { .. }
        | Effect::GainLife { .. }
        | Effect::LoseLife { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::apply::test_support::*;

    #[test]
    fn forest_mana_ability_adds_green_without_using_the_stack() {
        let db = db();
        let mut state = slice_state();
        let inst = state.new_instance(fixture("forest"));
        let id = state.mint_id();
        state.battlefield.push(Permanent {
            id: PermanentId(id),
            instance: inst.id,
            card: fixture("forest"),
            controller: PlayerId(0),
            tapped: false,
            entered_turn: 0,
            attacking: None,
            blocking: None,
            damage: 0,
            counters: Default::default(),
            attached_to: None,
        });
        let after = apply_action(
            &state,
            &Action::ActivateAbility {
                permanent: PermanentId(id),
                index: 0,
                targets: Vec::new(),
            },
            &db,
        );
        assert_eq!(after.players[0].mana_pool.green, 1);
        assert!(after.battlefield[0].tapped);
        assert!(after.stack.is_empty());
    }

    #[test]
    fn mana_ability_does_not_pass_priority() {
        let db = db();
        let mut state = slice_state();
        let inst = state.new_instance(fixture("forest"));
        let id = state.mint_id();
        state.battlefield.push(Permanent {
            id: PermanentId(id),
            instance: inst.id,
            card: fixture("forest"),
            controller: PlayerId(0),
            tapped: false,
            entered_turn: 0,
            attacking: None,
            blocking: None,
            damage: 0,
            counters: Default::default(),
            attached_to: None,
        });
        let after = apply_action(
            &state,
            &Action::ActivateAbility {
                permanent: PermanentId(id),
                index: 0,
                targets: Vec::new(),
            },
            &db,
        );
        assert_eq!(after.priority, PlayerId(0));
        assert_eq!(after.consecutive_passes, 0);
    }

    #[test]
    fn casting_a_creature_moves_it_to_the_stack_and_pays_mana() {
        let db = db();
        let mut state = slice_state();
        state.players[0].mana_pool.add(Color::Green, 1);
        let scout = hand_instance(&state, 0, fixture("llanowar_elves"));
        let after = apply_action(
            &state,
            &Action::CastSpell {
                card: scout,
                targets: Vec::new(),
            },
            &db,
        );
        assert_eq!(after.stack.len(), 1);
        assert_eq!(after.players[0].mana_pool.green, 0);
        assert!(!after.players[0].hand.iter().any(|c| c.id == scout.id));
    }

    #[test]
    fn issue_card_effects_etb_draw_end_to_end() {
        // Full vertical slice: three Forests already in play tap for {G}{G}{G}, cast
        // Skyscanner ({3}, an ETB "draw a card"), resolve it (ETB triggers), then
        // resolve the trigger (controller draws).
        let db = db();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let forests: Vec<PermanentId> = (0..3)
            .map(|_| place_permanent(&mut state, fixture("forest"), PlayerId(0), false, 0))
            .collect();
        let scanner = state.new_instance(fixture("skyscanner"));
        let draw_card = state.new_instance(fixture("onakke_ogre"));
        state.players[0].hand = vec![scanner];
        state.players[0].library = vec![draw_card];

        // Tap the three Forests for {G} each (mana abilities resolve immediately).
        for forest in forests {
            state = apply_action(
                &state,
                &Action::ActivateAbility {
                    permanent: forest,
                    index: 0,
                    targets: Vec::new(),
                },
                &db,
            );
        }
        assert_eq!(state.players[0].mana_pool.green, 3);
        assert!(state.stack.is_empty());

        // Cast Skyscanner ({3} paid from the three green).
        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: scanner,
                targets: Vec::new(),
            },
            &db,
        );
        assert_eq!(state.stack.len(), 1);
        assert_eq!(state.players[0].mana_pool.green, 0);

        // Pass twice: the creature resolves and its ETB trigger goes on the stack.
        let state = apply_action(&state, &Action::PassPriority, &db);
        let state = apply_action(&state, &Action::PassPriority, &db);
        assert!(state
            .battlefield
            .iter()
            .any(|p| p.card == fixture("skyscanner")));
        assert_eq!(state.stack.len(), 1);

        // Pass twice more: the ETB ability resolves and player 0 draws.
        let state = apply_action(&state, &Action::PassPriority, &db);
        let state = apply_action(&state, &Action::PassPriority, &db);
        assert!(state.stack.is_empty());
        assert!(state.players[0].hand.iter().any(|c| c.id == draw_card.id));
        assert!(state.players[0].library.is_empty());
    }

    #[test]
    fn issue_155_tapland_enters_tapped_with_no_untapped_window_cr_614_1c() {
        // CR 614.1c/614.12: a land with an "enters tapped" self-replacement is tapped
        // the instant it is on the battlefield. Tranquil Expanse is played as a land
        // (CR 116.2a): the resulting permanent is already tapped, and because a {T}
        // mana ability is unpayable while tapped, no action to tap it for mana is
        // offered this same priority window — there is no observable untapped state.
        let db = db();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let land = state.new_instance(fixture("tranquil_expanse"));
        state.players[0].hand = vec![land];

        let after = apply_action(&state, &Action::PlayLand { card: land }, &db);

        assert_eq!(after.battlefield.len(), 1);
        let perm = &after.battlefield[0];
        assert!(
            perm.tapped,
            "the tapland is tapped the moment it enters (CR 614.1c/614.12)"
        );
        // No ActivateAbility for the tapland is on offer: its {T} abilities can't be
        // paid while it is tapped, so it cannot be tapped for mana this turn.
        assert!(
            !valid_actions(&after, &db).iter().any(
                |a| matches!(a, Action::ActivateAbility { permanent, .. } if *permanent == perm.id)
            ),
            "a tapland offers no mana ability the turn it enters — no untapped window"
        );
    }

    #[test]
    fn issue_51_duplicate_cards_have_distinct_instances_and_routable_actions() {
        // Two copies of the same printed card (two Forests) in one hand must be
        // individually addressable: distinct instance ids, one PlayLand action
        // per copy, and applying one action plays that exact copy — not "the
        // first matching copy".
        let db = db();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let forest_a = state.new_instance(fixture("forest"));
        let forest_b = state.new_instance(fixture("forest"));
        state.players[0].hand = vec![forest_a, forest_b];

        // Same printed card, but two distinct physical instances.
        assert_eq!(forest_a.card, forest_b.card);
        assert_ne!(forest_a.id, forest_b.id);

        // The engine offers one land action per copy, each naming its own copy.
        let plays: Vec<CardInstance> = valid_actions(&state, &db)
            .into_iter()
            .filter_map(|action| match action {
                Action::PlayLand { card } => Some(card),
                _ => None,
            })
            .collect();
        assert_eq!(plays.len(), 2);
        assert!(plays.contains(&forest_a));
        assert!(plays.contains(&forest_b));

        // Routing the action for the second copy removes exactly that copy,
        // leaving the first untouched in hand.
        let after = apply_action(&state, &Action::PlayLand { card: forest_b }, &db);
        assert_eq!(after.players[0].hand, vec![forest_a]);
        assert_eq!(after.battlefield.len(), 1);
        assert_eq!(after.battlefield[0].instance, forest_b.id);
    }

    #[test]
    fn issue_150_pump_spell_boosts_its_target_until_end_of_turn_end_to_end() {
        // Cast Titanic Growth (+4/+4 until end of turn) on a 1/1 Llanowar Elves: on
        // resolution the creature computes as a 5/5 and one until-end-of-turn layer-7c
        // modifier is in force.
        use crate::characteristics::characteristics;
        let db = db();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let creature =
            place_permanent(&mut state, fixture("llanowar_elves"), PlayerId(0), false, 0);
        let surge = state.new_instance(fixture("titanic_growth"));
        state.players[0].hand = vec![surge];
        state.players[0].mana_pool.add(Color::Green, 1);
        state.players[0].mana_pool.colorless = 2;

        // The Elves is a printed 1/1 before the pump.
        assert_eq!(characteristics(&state, creature, &db).power, Some(1));

        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: surge,
                targets: vec![Target::Permanent(creature)],
            },
            &db,
        );
        // Pass twice: the spell resolves and applies its pump.
        let state = pass_full_round(&state, &db);

        assert!(state.stack.is_empty());
        let ch = characteristics(&state, creature, &db);
        assert_eq!(ch.power, Some(5), "printed 1 + 4 until end of turn");
        assert_eq!(ch.toughness, Some(5));
        assert_eq!(state.static_effects.len(), 1);
        assert_eq!(
            state.static_effects[0].duration,
            Duration::UntilEndOfTurn,
            "the pump is an until-end-of-turn effect"
        );
        // The instant itself went to the graveyard (CR 608.2m).
        assert!(state.players[0].graveyard.iter().any(|c| c.id == surge.id));
    }

    #[test]
    fn issue_150_pumped_creature_survives_lethal_to_base_damage_then_expires_at_cleanup_cr_514_2() {
        // CR 514.2: a 1/1 pumped to 4/4 that has taken 3 marked damage (lethal to
        // its *base* toughness of 1, but not to 4) survives the turn, and at
        // cleanup its pump wears off and its damage is removed **simultaneously** —
        // so the CR 704.5g check that follows never sees a 1/1 with 3 damage and
        // the creature survives cleanup as a printed 1/1.
        let db = db();
        let mut state = GameState::new_two_player();
        state.step = Step::End; // player 0, turn 1; empty hand so no discard.
        let creature =
            place_permanent(&mut state, fixture("llanowar_elves"), PlayerId(0), false, 3);
        pump(&mut state, creature, 3, 3);

        // Mid-turn: 4/4 with 3 marked damage is not lethal, so state-based actions
        // leave it on the battlefield.
        let mut mid = state.clone();
        run_state_based_actions(&mut mid, &db);
        assert!(
            mid.battlefield.iter().any(|p| p.id == creature),
            "3 damage is not lethal to a pumped 4/4"
        );

        // Walk through the cleanup step into the next turn.
        let after = pass_full_round(&state, &db);
        assert!(
            after.battlefield.iter().any(|p| p.id == creature),
            "the creature survives cleanup: damage and pump end simultaneously (CR 514.2)"
        );
        assert!(
            after.static_effects.is_empty(),
            "the until-end-of-turn pump wore off at cleanup"
        );
        assert_eq!(
            find_perm(&after, creature).damage,
            0,
            "marked damage was wiped at cleanup"
        );
    }

    #[test]
    fn issue_374_grant_keyword_spell_grants_the_keyword_until_end_of_turn_end_to_end() {
        // Cast Jump (target creature gains flying until end of turn) on a ground
        // Llanowar Elves: on resolution the creature computes with flying and one
        // until-end-of-turn layer-6 grant is in force.
        use crate::characteristics::characteristics;
        let db = db();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let creature =
            place_permanent(&mut state, fixture("llanowar_elves"), PlayerId(0), false, 0);
        let jump = state.new_instance(fixture("jump"));
        state.players[0].hand = vec![jump];
        state.players[0].mana_pool.add(Color::Blue, 1);

        // The Elves has no flying before the spell.
        assert!(!characteristics(&state, creature, &db)
            .keywords
            .contains(&Keyword::Flying));

        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: jump,
                targets: vec![Target::Permanent(creature)],
            },
            &db,
        );
        // Pass twice: the spell resolves and applies its grant.
        let state = pass_full_round(&state, &db);

        assert!(state.stack.is_empty());
        assert!(
            characteristics(&state, creature, &db)
                .keywords
                .contains(&Keyword::Flying),
            "the resolved spell granted flying (CR 613.1f)"
        );
        assert_eq!(state.static_effects.len(), 1);
        assert_eq!(
            state.static_effects[0].duration,
            Duration::UntilEndOfTurn,
            "the grant is an until-end-of-turn effect"
        );
        assert!(state.players[0].graveyard.iter().any(|c| c.id == jump.id));
    }

    #[test]
    fn issue_374_until_end_of_turn_grant_expires_at_cleanup_cr_514_2() {
        // CR 514.2: an until-end-of-turn keyword grant ends in the cleanup step. The
        // grant is present the turn it is made and gone once the turn passes — verified
        // across the turn boundary.
        use crate::characteristics::characteristics;
        let db = db();
        let mut state = GameState::new_two_player();
        state.step = Step::End; // player 0, turn 1; empty hand so no discard.
        let creature =
            place_permanent(&mut state, fixture("walking_corpse"), PlayerId(0), false, 0);
        grant_keyword(&mut state, creature, Keyword::Flying);

        // Before cleanup the creature has the granted keyword.
        assert!(
            characteristics(&state, creature, &db)
                .keywords
                .contains(&Keyword::Flying),
            "the grant is in force during the turn it was made"
        );

        // Walk through the cleanup step into the next turn.
        let after = pass_full_round(&state, &db);
        assert!(
            after.static_effects.is_empty(),
            "the until-end-of-turn grant wore off at cleanup (CR 514.2)"
        );
        assert!(
            !characteristics(&after, creature, &db)
                .keywords
                .contains(&Keyword::Flying),
            "the granted keyword is gone across the turn boundary"
        );
    }

    #[test]
    fn issue_150_two_pumps_in_one_turn_stack_and_both_expire_at_cleanup() {
        // CR 613.7 / 514.2: two pumps on one creature this turn both apply (they
        // stack in timestamp order) and both wear off at cleanup.
        use crate::characteristics::characteristics;
        let db = db();
        let mut state = GameState::new_two_player();
        state.step = Step::End; // player 0, turn 1; empty hand.
        let creature =
            place_permanent(&mut state, fixture("llanowar_elves"), PlayerId(0), false, 0);
        let first = pump(&mut state, creature, 2, 2);
        let second = pump(&mut state, creature, 1, 1);
        assert!(second > first, "the later pump has the later timestamp");

        // Printed 1/1 + (+2/+2) + (+1/+1) = 4/4 while both are in force.
        let ch = characteristics(&state, creature, &db);
        assert_eq!(ch.power, Some(4));
        assert_eq!(ch.toughness, Some(4));

        let after = pass_full_round(&state, &db);
        assert!(
            after.static_effects.is_empty(),
            "both until-end-of-turn pumps expired at cleanup (CR 514.2)"
        );
        let reverted = characteristics(&after, creature, &db);
        assert_eq!(reverted.power, Some(1), "back to the printed 1/1");
        assert_eq!(reverted.toughness, Some(1));
    }

    #[test]
    fn issue_150_pump_never_outlives_its_permanent() {
        // A pumped creature that dies mid-turn (here to lethal-to-its-4/4 damage)
        // leaves no dangling modifier: the state-based-actions loop destroys it and
        // prunes its now-orphaned pump in the same pass.
        let db = db();
        let mut state = GameState::new_two_player();
        let creature =
            place_permanent(&mut state, fixture("llanowar_elves"), PlayerId(0), false, 5);
        pump(&mut state, creature, 3, 3); // 1/1 -> 4/4, but 5 damage is lethal

        run_state_based_actions(&mut state, &db);

        assert!(
            !state.battlefield.iter().any(|p| p.id == creature),
            "5 damage is lethal to the pumped 4/4 (CR 704.5g)"
        );
        assert!(
            state.static_effects.is_empty(),
            "the pump was pruned when its permanent left — no dangling modifier"
        );
    }

    #[test]
    fn issue_150_while_on_battlefield_effect_is_not_ended_by_cleanup() {
        // CR 514.2 ends only "until end of turn" effects; a permanent-lifetime
        // anthem (WhileOnBattlefield) is untouched by the cleanup step.
        let db = db();
        let mut state = GameState::new_two_player();
        state.step = Step::End; // player 0, turn 1; empty hand.
        let _creature =
            place_permanent(&mut state, fixture("walking_corpse"), PlayerId(0), false, 0);
        let source = state.mint_id();
        state.static_effects.push(StaticEffect {
            source,
            affects: EffectAffects::CreaturesControlledBy(PlayerId(0)),
            modification: Modification::PowerToughness {
                power: 1,
                toughness: 1,
            },
            duration: Duration::WhileOnBattlefield,
        });

        let after = pass_full_round(&state, &db);
        assert_eq!(
            after.static_effects.len(),
            1,
            "a while-on-battlefield anthem persists through cleanup (CR 514.2)"
        );
    }

    #[test]
    fn issue_152_minus_x_aura_cast_kills_its_host_and_follows_it_cr_704_5f() {
        // Full slice through the real cast path: cast a -2/-2 Aura on a 3/2 host. On
        // resolution the Aura enters attached, its -2/-2 drops the host's current
        // toughness to 0, and the pipeline's state-based-actions loop puts the host
        // into the graveyard (CR 704.5f) and its now-orphaned Aura with it (CR
        // 704.5m) — both gone in the same fixed point, the modifier vanishing with the
        // Aura. P/T Auras have no clean M19 card, so this is inline (ADR 0025).
        use crate::ability::Target;
        use crate::characteristics::characteristics;
        let json = r#"[
            {"schema_version":1,"functional_id":"test_curse","name":"Test Curse",
             "types":["enchantment"],"subtypes":["Aura"],"mana_cost":"{B}","colors":["black"],
             "aura":{"enchant":"any_creature","power":-2,"toughness":-2}},
            {"schema_version":1,"functional_id":"test_boar","name":"Test Boar",
             "types":["creature"],"subtypes":["Boar"],"mana_cost":"{2}{G}","colors":["green"],
             "power":3,"toughness":2}
        ]"#;
        let db = CardDatabase::from_json(json).unwrap();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let host = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(0), false, 0); // 3/2
        let curse = state.new_instance(id_in(&db, "test_curse")); // -2/-2 Aura, {B}
        state.players[0].hand = vec![curse];
        state.players[0].mana_pool.add(Color::Black, 1);

        // The host is a healthy 3/2 before the Aura is cast.
        assert_eq!(characteristics(&state, host, &db).toughness, Some(2));

        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: curse,
                targets: vec![Target::Permanent(host)],
            },
            &db,
        );
        // Both players pass: the Aura resolves, attaches, and the SBA loop settles.
        let state = pass_full_round(&state, &db);

        assert!(
            !state.battlefield.iter().any(|p| p.id == host),
            "the host at 0 toughness is put into the graveyard (CR 704.5f)"
        );
        assert!(
            !state
                .battlefield
                .iter()
                .any(|p| p.card == id_in(&db, "test_curse")),
            "the Aura follows its dead host to the graveyard (CR 704.5m)"
        );
        assert!(
            state.static_effects.is_empty(),
            "the Aura's derived modifier leaves no dangling static effect"
        );
        // The Boar and the Curse are both in the graveyard.
        assert_eq!(state.players[0].graveyard.len(), 2);
    }

    #[test]
    fn issue_119_zero_life_loss_records_its_reason_cr_704_5a() {
        // CR 704.5a: the life ≤ 0 loss now carries its reason and consumes into a
        // terminal result naming the winner (CR 104.2a).
        let db = db();
        let mut state = GameState::new_two_player();
        state.players[1].life = 0;
        let after = apply_action(&state, &Action::PassPriority, &db);
        assert_eq!(after.players[1].loss_reason, Some(LossReason::ZeroLife));
        let result = after.result().unwrap();
        assert_eq!(result.winner, Some(PlayerId(0)));
        assert_eq!(result.reason, LossReason::ZeroLife);
    }

    #[test]
    fn issue_148_counterspell_counters_a_creature_spell_end_to_end_cr_701_5() {
        // A creature spell (player 1) waits on the stack; player 0, holding
        // priority, casts Cancel ({1}{U}{U} instant) targeting it. The
        // counterspell records its target at cast (CR 601.2c) and, resolving first
        // (LIFO), removes the creature spell to its owner's graveyard without
        // resolving (CR 701.5a) — the creature never enters the battlefield.
        let db = db();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;

        // Player 1's Onakke Ogre (vanilla creature) on the stack.
        let boar = state.new_instance(fixture("onakke_ogre"));
        let boar_sid = StackId(state.mint_id());
        state.stack.push(StackObject {
            id: boar_sid,
            controller: PlayerId(1),
            kind: StackObjectKind::Spell { card: boar },
            targets: Vec::new(),
        });

        // Player 0 holds priority with the counterspell and {1}{U}{U}.
        let negation = state.new_instance(fixture("cancel"));
        state.players[0].hand = vec![negation];
        state.players[0].mana_pool.add(Color::Blue, 2);
        state.players[0].mana_pool.colorless = 1;
        state.priority = PlayerId(0);

        // Cast the counterspell targeting the creature spell (CR 601.2c).
        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: negation,
                targets: vec![Target::Spell(boar_sid)],
            },
            &db,
        );
        assert_eq!(
            state.stack.len(),
            2,
            "counterspell stacked over the creature"
        );
        assert_eq!(
            state.stack[1].targets,
            vec![Target::Spell(boar_sid)],
            "the chosen target is recorded on the stack at cast (CR 601.2c)"
        );
        assert_eq!(
            state.players[0].mana_pool.blue, 0,
            "the {{1}}{{U}}{{U}} was paid"
        );

        // Both pass: the counterspell resolves first and counters the creature.
        let state = apply_action(&state, &Action::PassPriority, &db);
        let state = apply_action(&state, &Action::PassPriority, &db);

        assert!(state.stack.is_empty(), "both spells have left the stack");
        assert!(
            state
                .battlefield
                .iter()
                .all(|p| p.card != fixture("onakke_ogre")),
            "the countered creature never entered the battlefield (CR 701.5a)"
        );
        assert!(
            state.players[1].graveyard.contains(&boar),
            "the countered spell went to its owner's graveyard (CR 701.5a)"
        );
        assert!(
            state.players[0]
                .graveyard
                .iter()
                .any(|c| c.id == negation.id),
            "the resolved counterspell went to its owner's graveyard (CR 608.2m)"
        );
    }

    #[test]
    fn issue_148_counterspell_fizzles_when_its_target_resolves_first_cr_608_2b() {
        // If the targeted spell resolves before the counterspell (the counterspell
        // sits *beneath* it), the counterspell's only target is gone at resolution,
        // so it fizzles (CR 608.2b): no spell is countered, and the counterspell
        // still goes to its owner's graveyard.
        let db = db();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;

        // Bottom of the stack: player 0's counterspell aimed at the creature above.
        let negation = state.new_instance(fixture("cancel"));
        let neg_sid = StackId(state.mint_id());
        let boar = state.new_instance(fixture("onakke_ogre"));
        let boar_sid = StackId(state.mint_id());
        state.stack.push(StackObject {
            id: neg_sid,
            controller: PlayerId(0),
            kind: StackObjectKind::Spell { card: negation },
            targets: vec![Target::Spell(boar_sid)],
        });
        // Top of the stack: player 1's vanilla creature spell, resolves first.
        state.stack.push(StackObject {
            id: boar_sid,
            controller: PlayerId(1),
            kind: StackObjectKind::Spell { card: boar },
            targets: Vec::new(),
        });

        // Resolve the top (the creature): it enters the battlefield.
        let state = apply_action(&state, &Action::PassPriority, &db);
        let state = apply_action(&state, &Action::PassPriority, &db);
        assert!(
            state
                .battlefield
                .iter()
                .any(|p| p.card == fixture("onakke_ogre")),
            "the creature spell resolved onto the battlefield"
        );

        // Resolve the counterspell: its target is gone, so it fizzles (CR 608.2b).
        let state = apply_action(&state, &Action::PassPriority, &db);
        let state = apply_action(&state, &Action::PassPriority, &db);
        assert!(state.stack.is_empty());
        assert!(
            state
                .battlefield
                .iter()
                .any(|p| p.card == fixture("onakke_ogre")),
            "the creature survives — nothing was countered"
        );
        assert!(
            state.players[0]
                .graveyard
                .iter()
                .any(|c| c.id == negation.id),
            "a fizzled spell still goes to its owner's graveyard (CR 608.2b)"
        );
    }

    #[test]
    fn issue_149_burn_spell_kills_a_creature_via_lethal_damage_sba_cr_704_5g() {
        // A burn spell that deals damage equal to a creature's toughness marks
        // lethal damage; the CR 704.5g state-based action then destroys it.
        let db = db();
        let mut state = main_phase_p0();
        // Onakke Ogre is a 4/2; Shock deals exactly 2 → lethal to its toughness.
        let boar = place_permanent(&mut state, fixture("onakke_ogre"), PlayerId(1), false, 0);
        let shock = state.new_instance(fixture("shock"));
        state.players[0].hand = vec![shock];
        state.players[0].mana_pool.add(Color::Red, 1);

        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: shock,
                targets: vec![Target::Permanent(boar)],
            },
            &db,
        );
        assert_eq!(state.stack.len(), 1, "the burn spell is on the stack");
        let state = pass_full_round(&state, &db);

        assert!(
            !state.battlefield.iter().any(|p| p.id == boar),
            "the burned creature is destroyed (CR 704.5g)"
        );
        assert_eq!(
            state.players[1].graveyard.len(),
            1,
            "it went to its owner's graveyard"
        );
    }

    #[test]
    fn issue_149_burn_spell_to_a_player_drops_life_and_loses_at_zero_cr_704_5a() {
        // The same burn verb aimed at a player is life loss (CR 120.3a); dropping a
        // player to 0 feeds the zero-life loss (CR 704.5a).
        let db = db();
        let mut state = main_phase_p0();
        state.players[1].life = 2;
        let shock = state.new_instance(fixture("shock"));
        state.players[0].hand = vec![shock];
        state.players[0].mana_pool.add(Color::Red, 1);

        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: shock,
                targets: vec![Target::Player(PlayerId(1))],
            },
            &db,
        );
        let state = pass_full_round(&state, &db);

        assert_eq!(state.players[1].life, 0);
        assert!(state.players[1].has_lost);
        assert_eq!(state.players[1].loss_reason, Some(LossReason::ZeroLife));
    }

    #[test]
    fn issue_256_lightning_strike_deals_three_to_any_target() {
        // Lightning Strike is a {1}{R} bolt — 3 damage to any target, distinct from
        // Shock's 2. Aimed at a player on 3 life, it drops them to 0 (CR 704.5a).
        let db = db();
        let mut state = main_phase_p0();
        state.players[1].life = 3;
        let bolt = state.new_instance(fixture("lightning_strike"));
        state.players[0].hand = vec![bolt];
        state.players[0].mana_pool.add(Color::Red, 1);
        state.players[0].mana_pool.colorless = 1;

        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: bolt,
                targets: vec![Target::Player(PlayerId(1))],
            },
            &db,
        );
        let state = pass_full_round(&state, &db);

        assert_eq!(state.players[1].life, 0);
        assert!(state.players[1].has_lost);
    }

    #[test]
    fn issue_256_divination_draws_two_cards() {
        // Divination is a {2}{U} sorcery that draws two — DrawCard flowing through the
        // spell-resolution path (until now it was only ever a triggered-ability effect,
        // so this proves the cast → resolve routing).
        let db = db();
        let mut state = main_phase_p0();
        let study = state.new_instance(fixture("divination"));
        state.players[0].hand = vec![study];
        let first = state.new_instance(fixture("forest"));
        let second = state.new_instance(fixture("forest"));
        state.players[0].library = vec![first, second];
        state.players[0].mana_pool.add(Color::Blue, 1);
        state.players[0].mana_pool.colorless = 2;

        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: study,
                targets: Vec::new(),
            },
            &db,
        );
        let state = pass_full_round(&state, &db);

        assert!(state.players[0].hand.contains(&first));
        assert!(state.players[0].hand.contains(&second));
        assert!(state.players[0].library.is_empty());
    }

    #[test]
    fn issue_256_enchantment_etb_gains_life_when_it_enters() {
        // A {G} enchantment whose enters-the-battlefield trigger gains its controller
        // 4 life — an ETB trigger on a *non-creature* permanent, and GainLife as an
        // ability effect rather than a spell effect. No M19 card carries this, so it
        // is exercised inline (ADR 0025).
        let json = r#"[{"schema_version":1,"functional_id":"test_blessing","name":"Test Blessing",
            "types":["enchantment"],"subtypes":[],"mana_cost":"{G}","colors":["green"],
            "abilities":[{"type":"triggered","event":"self_enters_battlefield",
              "effects":[{"kind":"gain_life","player_ref":"controller","amount":4}]}]}]"#;
        let db = CardDatabase::from_json(json).unwrap();
        let mut state = main_phase_p0();
        let life_before = state.players[0].life;
        let blessing = state.new_instance(id_in(&db, "test_blessing"));
        state.players[0].hand = vec![blessing];
        state.players[0].mana_pool.add(Color::Green, 1);

        // Cast it; pass twice so it resolves and its ETB trigger goes on the stack.
        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: blessing,
                targets: Vec::new(),
            },
            &db,
        );
        let state = pass_full_round(&state, &db);
        assert!(state
            .battlefield
            .iter()
            .any(|p| p.card == id_in(&db, "test_blessing")));
        assert_eq!(state.stack.len(), 1, "its ETB trigger is on the stack");

        // Pass twice more: the trigger resolves and the controller gains 4 life.
        let state = pass_full_round(&state, &db);
        assert!(state.stack.is_empty());
        assert_eq!(state.players[0].life, life_before + 4);
    }

    #[test]
    fn issue_256_mana_rock_taps_for_colorless_mana() {
        // A {1} mana rock — {T}: Add {C}. Its ability is a mana ability, so it
        // resolves immediately without using the stack (CR 605.3). The colorless-mana
        // verb has no M19 representative, so it is exercised inline (ADR 0025).
        let json = r#"[{"schema_version":1,"functional_id":"test_lodestone","name":"Test Lodestone",
            "types":["artifact"],"mana_cost":"{1}","colors":[],
            "abilities":[{"type":"activated","cost":[{"kind":"tap"}],
              "effects":[{"kind":"add_colorless_mana","amount":1}]}]}]"#;
        let db = CardDatabase::from_json(json).unwrap();
        let mut state = main_phase_p0();
        let lodestone = place_permanent(
            &mut state,
            id_in(&db, "test_lodestone"),
            PlayerId(0),
            false,
            0,
        );

        let after = apply_action(
            &state,
            &Action::ActivateAbility {
                permanent: lodestone,
                index: 0,
                targets: Vec::new(),
            },
            &db,
        );
        assert_eq!(after.players[0].mana_pool.colorless, 1);
        assert!(find_perm(&after, lodestone).tapped);
        assert!(after.stack.is_empty());
    }

    #[test]
    fn issue_149_destroy_puts_a_creature_in_its_owners_graveyard_cr_701_7() {
        let db = db();
        let mut state = main_phase_p0();
        let boar = place_permanent(&mut state, fixture("onakke_ogre"), PlayerId(1), false, 0);
        // Murder is a {1}{B}{B} instant: two black pips and one generic.
        let ray = state.new_instance(fixture("murder"));
        state.players[0].hand = vec![ray];
        state.players[0].mana_pool.add(Color::Black, 2);
        state.players[0].mana_pool.colorless = 1;

        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: ray,
                targets: vec![Target::Permanent(boar)],
            },
            &db,
        );
        let state = pass_full_round(&state, &db);

        assert!(
            !state.battlefield.iter().any(|p| p.id == boar),
            "the targeted creature is destroyed (CR 701.7)"
        );
        assert!(state.players[1]
            .graveyard
            .iter()
            .any(|c| c.card == fixture("onakke_ogre")));
    }

    #[test]
    fn issue_149_destroy_fizzles_if_its_target_left_first_cr_608_2b() {
        let db = db();
        let mut state = main_phase_p0();
        let boar = place_permanent(&mut state, fixture("onakke_ogre"), PlayerId(1), false, 0);
        let ray = state.new_instance(fixture("murder"));
        state.players[0].hand = vec![ray];
        state.players[0].mana_pool.add(Color::Black, 2);
        state.players[0].mana_pool.colorless = 1;

        let mut state = apply_action(
            &state,
            &Action::CastSpell {
                card: ray,
                targets: vec![Target::Permanent(boar)],
            },
            &db,
        );
        // The target leaves the battlefield before the sorcery resolves.
        state.battlefield.retain(|p| p.id != boar);

        let state = pass_full_round(&state, &db);
        assert!(state.stack.is_empty());
        assert!(
            state.players[0].graveyard.iter().any(|c| c.id == ray.id),
            "a fizzled spell still goes to its owner's graveyard (CR 608.2b)"
        );
    }

    #[test]
    fn issue_149_minus_one_counter_lowers_toughness_to_lethal_cr_704_5g() {
        // A -1/-1 counter folds into computed toughness (CR 613.7c). A 3/2 with 1
        // marked damage is not lethal (1 < 2); after a -1/-1 counter it is a 2/1
        // and 1 damage is lethal (1 ≥ 1), so the SBA destroys it. The -1/-1 counter
        // spell has no M19 representative, so both cards are inline (ADR 0025).
        let json = r#"[
            {"schema_version":1,"functional_id":"test_boar","name":"Test Boar",
             "types":["creature"],"subtypes":["Boar"],"mana_cost":"{2}{G}","colors":["green"],
             "power":3,"toughness":2},
            {"schema_version":1,"functional_id":"test_wither","name":"Test Wither",
             "types":["sorcery"],"mana_cost":"{B}","colors":["black"],
             "spell_effects":[{"kind":"put_counters","target":"any_creature","counter":"minus_one_minus_one","count":1}]}
        ]"#;
        let db = CardDatabase::from_json(json).unwrap();
        let mut state = main_phase_p0();
        let boar = place_permanent(&mut state, id_in(&db, "test_boar"), PlayerId(1), false, 1);
        let touch = state.new_instance(id_in(&db, "test_wither")); // {B} sorcery, -1/-1
        state.players[0].hand = vec![touch];
        state.players[0].mana_pool.add(Color::Black, 1);

        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: touch,
                targets: vec![Target::Permanent(boar)],
            },
            &db,
        );
        let state = pass_full_round(&state, &db);

        assert!(
            !state.battlefield.iter().any(|p| p.id == boar),
            "a -1/-1 counter made toughness ≤ marked damage → destroyed (CR 704.5g)"
        );
        assert_eq!(state.players[1].graveyard.len(), 1);
    }

    #[test]
    fn issue_149_life_gain_adds_to_a_low_life_total_cr_119() {
        let db = db();
        let mut state = main_phase_p0();
        state.players[0].life = 1;
        let balm = state.new_instance(fixture("revitalize")); // Revitalize {W}: gain 3, draw 1
        state.players[0].hand = vec![balm];
        // Revitalize also draws, so seed a card to avoid decking out (CR 704.5c).
        state.players[0].library = vec![state.new_instance(fixture("forest"))];
        state.players[0].mana_pool.add(Color::White, 1);

        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: balm,
                targets: Vec::new(),
            },
            &db,
        );
        let state = pass_full_round(&state, &db);

        assert_eq!(state.players[0].life, 4);
        assert!(!state.players[0].has_lost);
    }

    #[test]
    fn issue_149_life_loss_to_exactly_zero_triggers_the_loss_cr_704_5a() {
        // The lose-life verb has no M19 representative, so it is exercised inline.
        let json = r#"[{"schema_version":1,"functional_id":"test_drain","name":"Test Drain",
            "types":["instant"],"mana_cost":"{B}","colors":["black"],
            "spell_effects":[{"kind":"lose_life","player_ref":"controller","amount":2}]}]"#;
        let db = CardDatabase::from_json(json).unwrap();
        let mut state = main_phase_p0();
        state.players[0].life = 2;
        let ordeal = state.new_instance(id_in(&db, "test_drain")); // {B} instant, lose 2
        state.players[0].hand = vec![ordeal];
        state.players[0].mana_pool.add(Color::Black, 1);

        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: ordeal,
                targets: Vec::new(),
            },
            &db,
        );
        let state = pass_full_round(&state, &db);

        assert_eq!(state.players[0].life, 0);
        assert!(state.players[0].has_lost);
        assert_eq!(state.players[0].loss_reason, Some(LossReason::ZeroLife));
    }

    // === issue #401: behavior of the nontrivial M19 catalog additions ===
    // Vanilla and single-keyword bodies (Loxodon Line Breaker, Havoc Devils,
    // Daybreak Chaplain, …) reuse mechanics already covered by the generic
    // keyword/combat tests, so only the cards that *do* something get a boundary
    // test here. Skeleton Archer's ETB "deal 1 damage to any target" is omitted:
    // like Viashino Pyromancer it is a triggered ability, and triggers carry no
    // chosen targets until issue #71 — so its damage is exercised only by the
    // rules-text generator, not end-to-end.

    #[test]
    fn issue_401_aegis_of_the_heavens_pumps_plus_one_plus_seven() {
        // Aegis of the Heavens: a {1}{W} instant, +1/+7 until end of turn.
        use crate::characteristics::characteristics;
        let db = db();
        let mut state = main_phase_p0();
        let creature =
            place_permanent(&mut state, fixture("llanowar_elves"), PlayerId(0), false, 0);
        let aegis = state.new_instance(fixture("aegis_of_the_heavens"));
        state.players[0].hand = vec![aegis];
        state.players[0].mana_pool.add(Color::White, 1);
        state.players[0].mana_pool.colorless = 1;

        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: aegis,
                targets: vec![Target::Permanent(creature)],
            },
            &db,
        );
        let state = pass_full_round(&state, &db);

        let ch = characteristics(&state, creature, &db);
        assert_eq!(ch.power, Some(2), "printed 1 + 1");
        assert_eq!(ch.toughness, Some(8), "printed 1 + 7");
    }

    #[test]
    fn issue_401_mighty_leap_pumps_and_grants_flying_in_one_spell() {
        // Mighty Leap: +2/+2 *and* gains flying until end of turn — two spell
        // effects, so the cast supplies the same creature as the target of each
        // (the pump slot and the grant slot).
        use crate::characteristics::characteristics;
        let db = db();
        let mut state = main_phase_p0();
        let creature =
            place_permanent(&mut state, fixture("llanowar_elves"), PlayerId(0), false, 0);
        let leap = state.new_instance(fixture("mighty_leap"));
        state.players[0].hand = vec![leap];
        state.players[0].mana_pool.add(Color::White, 1);
        state.players[0].mana_pool.colorless = 1;

        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: leap,
                targets: vec![Target::Permanent(creature), Target::Permanent(creature)],
            },
            &db,
        );
        let state = pass_full_round(&state, &db);

        let ch = characteristics(&state, creature, &db);
        assert_eq!(ch.power, Some(3), "1 + 2");
        assert_eq!(ch.toughness, Some(3));
        assert!(
            ch.keywords.contains(&Keyword::Flying),
            "the same spell granted flying (CR 613.1f)"
        );
    }

    #[test]
    fn issue_401_sure_strike_pumps_power_and_grants_first_strike() {
        // Sure Strike: +3/+0 and gains first strike until end of turn.
        use crate::characteristics::characteristics;
        let db = db();
        let mut state = main_phase_p0();
        let creature =
            place_permanent(&mut state, fixture("llanowar_elves"), PlayerId(0), false, 0);
        let strike = state.new_instance(fixture("sure_strike"));
        state.players[0].hand = vec![strike];
        state.players[0].mana_pool.add(Color::Red, 1);
        state.players[0].mana_pool.colorless = 1;

        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: strike,
                targets: vec![Target::Permanent(creature), Target::Permanent(creature)],
            },
            &db,
        );
        let state = pass_full_round(&state, &db);

        let ch = characteristics(&state, creature, &db);
        assert_eq!(ch.power, Some(4), "1 + 3");
        assert_eq!(ch.toughness, Some(1), "toughness unchanged");
        assert!(ch.keywords.contains(&Keyword::FirstStrike));
    }

    #[test]
    fn issue_401_strangling_spores_shrinks_a_creature_to_death_cr_704_5f() {
        // Strangling Spores: target creature gets -3/-3 until end of turn — a
        // negative pump. A 4/2 Onakke Ogre drops to a 1/-1, and the CR 704.5f
        // zero-toughness state-based action puts it into the graveyard.
        let db = db();
        let mut state = main_phase_p0();
        let ogre = place_permanent(&mut state, fixture("onakke_ogre"), PlayerId(1), false, 0);
        let spores = state.new_instance(fixture("strangling_spores"));
        state.players[0].hand = vec![spores];
        state.players[0].mana_pool.add(Color::Black, 1);
        state.players[0].mana_pool.colorless = 3;

        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: spores,
                targets: vec![Target::Permanent(ogre)],
            },
            &db,
        );
        let state = pass_full_round(&state, &db);

        assert!(
            !state.battlefield.iter().any(|p| p.id == ogre),
            "-3/-3 dropped toughness to 0 or less (CR 704.5f)"
        );
        assert_eq!(state.players[1].graveyard.len(), 1);
    }

    #[test]
    fn issue_401_knights_pledge_aura_boosts_its_host_plus_two_plus_two() {
        // Knight's Pledge: a bundled +2/+2 Aura — the first shipped P/T Aura.
        use crate::characteristics::characteristics;
        let db = db();
        let mut state = main_phase_p0();
        let host = place_permanent(&mut state, fixture("llanowar_elves"), PlayerId(0), false, 0);
        let pledge = state.new_instance(fixture("knight_s_pledge"));
        state.players[0].hand = vec![pledge];
        state.players[0].mana_pool.add(Color::White, 1);
        state.players[0].mana_pool.colorless = 1;

        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: pledge,
                targets: vec![Target::Permanent(host)],
            },
            &db,
        );
        let state = pass_full_round(&state, &db);

        let ch = characteristics(&state, host, &db);
        assert_eq!(ch.power, Some(3), "printed 1 + 2 while enchanted");
        assert_eq!(ch.toughness, Some(3));
        assert!(
            state
                .battlefield
                .iter()
                .any(|p| p.card == fixture("knight_s_pledge") && p.attached_to == Some(host)),
            "the Aura entered attached to its host (CR 303.4d)"
        );
    }

    #[test]
    fn issue_401_oakenform_aura_boosts_its_host_plus_three_plus_three() {
        // Oakenform: a bundled +3/+3 Aura.
        use crate::characteristics::characteristics;
        let db = db();
        let mut state = main_phase_p0();
        let host = place_permanent(&mut state, fixture("llanowar_elves"), PlayerId(0), false, 0);
        let oak = state.new_instance(fixture("oakenform"));
        state.players[0].hand = vec![oak];
        state.players[0].mana_pool.add(Color::Green, 1);
        state.players[0].mana_pool.colorless = 2;

        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: oak,
                targets: vec![Target::Permanent(host)],
            },
            &db,
        );
        let state = pass_full_round(&state, &db);

        let ch = characteristics(&state, host, &db);
        assert_eq!(ch.power, Some(4), "1 + 3");
        assert_eq!(ch.toughness, Some(4));
    }

    #[test]
    fn issue_401_prodigious_growth_aura_grants_p_t_and_trample() {
        // Prodigious Growth: +7/+7 *and* trample — a P/T-and-keyword Aura in one.
        use crate::characteristics::characteristics;
        let db = db();
        let mut state = main_phase_p0();
        let host = place_permanent(&mut state, fixture("llanowar_elves"), PlayerId(0), false, 0);
        let growth = state.new_instance(fixture("prodigious_growth"));
        state.players[0].hand = vec![growth];
        state.players[0].mana_pool.add(Color::Green, 2);
        state.players[0].mana_pool.colorless = 4;

        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: growth,
                targets: vec![Target::Permanent(host)],
            },
            &db,
        );
        let state = pass_full_round(&state, &db);

        let ch = characteristics(&state, host, &db);
        assert_eq!(ch.power, Some(8), "1 + 7");
        assert_eq!(ch.toughness, Some(8));
        assert!(
            ch.keywords.contains(&Keyword::Trample),
            "the Aura grants trample (CR 613.1f) alongside its P/T"
        );
    }

    #[test]
    fn issue_401_lichs_caress_destroys_a_creature_and_gains_three_life() {
        // Lich's Caress: destroy target creature, then you gain 3 life.
        let db = db();
        let mut state = main_phase_p0();
        let life_before = state.players[0].life;
        let victim = place_permanent(&mut state, fixture("onakke_ogre"), PlayerId(1), false, 0);
        let caress = state.new_instance(fixture("lich_s_caress"));
        state.players[0].hand = vec![caress];
        state.players[0].mana_pool.add(Color::Black, 2);
        state.players[0].mana_pool.colorless = 3;

        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: caress,
                targets: vec![Target::Permanent(victim)],
            },
            &db,
        );
        let state = pass_full_round(&state, &db);

        assert!(
            !state.battlefield.iter().any(|p| p.id == victim),
            "the targeted creature is destroyed (CR 701.7)"
        );
        assert_eq!(
            state.players[0].life,
            life_before + 3,
            "and its controller gains 3 life"
        );
    }

    #[test]
    fn issue_401_lava_axe_deals_five_to_a_player() {
        // Lava Axe: 5 damage to target player (planeswalkers are unmodeled, so the
        // spec is `any_player`) — the first shipped burn aimed only at a player.
        let db = db();
        let mut state = main_phase_p0();
        state.players[1].life = 20;
        let axe = state.new_instance(fixture("lava_axe"));
        state.players[0].hand = vec![axe];
        state.players[0].mana_pool.add(Color::Red, 1);
        state.players[0].mana_pool.colorless = 4;

        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: axe,
                targets: vec![Target::Player(PlayerId(1))],
            },
            &db,
        );
        let state = pass_full_round(&state, &db);

        assert_eq!(state.players[1].life, 15, "20 - 5 (CR 120.3a)");
    }

    #[test]
    fn issue_401_highland_game_gains_two_life_when_it_dies() {
        // Highland Game: "When Highland Game dies, you gain 2 life." Killed by a
        // Destroy effect, its dies trigger resolves and its controller gains 2.
        use crate::ability::TargetSpec;
        let db = db();
        let mut state = main_phase_p0();
        let life_before = state.players[0].life;
        let elk = place_permanent(&mut state, fixture("highland_game"), PlayerId(0), false, 0);
        push_ability(
            &mut state,
            elk,
            vec![Effect::Destroy {
                target: TargetSpec::AnyCreature,
            }],
            vec![Target::Permanent(elk)],
        );

        // Resolve the destroy: the Elk dies and its dies trigger lands on the stack.
        let state = apply_action(&state, &Action::PassPriority, &db);
        let state = apply_action(&state, &Action::PassPriority, &db);
        assert!(!alive(&state, elk), "the Destroy killed the Elk");
        assert_eq!(state.stack.len(), 1, "the dies trigger is on the stack");

        // Resolve the dies trigger.
        let state = pass_full_round(&state, &db);
        assert!(state.stack.is_empty());
        assert_eq!(state.players[0].life, life_before + 2);
    }

    #[test]
    fn issue_401_rhox_oracle_draws_a_card_when_it_enters() {
        // Rhox Oracle: a {4}{G} 4/2 whose ETB draws a card.
        let db = db();
        let mut state = main_phase_p0();
        let oracle = state.new_instance(fixture("rhox_oracle"));
        let card = state.new_instance(fixture("forest"));
        state.players[0].hand = vec![oracle];
        state.players[0].library = vec![card];
        state.players[0].mana_pool.add(Color::Green, 1);
        state.players[0].mana_pool.colorless = 4;

        // Cast; pass twice so it resolves and its ETB trigger goes on the stack.
        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: oracle,
                targets: Vec::new(),
            },
            &db,
        );
        let state = pass_full_round(&state, &db);
        assert!(state
            .battlefield
            .iter()
            .any(|p| p.card == fixture("rhox_oracle")));
        assert_eq!(state.stack.len(), 1, "its ETB trigger is on the stack");

        // Pass twice more: the trigger resolves and player 0 draws.
        let state = pass_full_round(&state, &db);
        assert!(state.stack.is_empty());
        assert!(state.players[0].hand.contains(&card));
    }

    #[test]
    fn issue_401_pelakka_wurm_gains_seven_life_on_etb_and_draws_when_it_dies() {
        // Pelakka Wurm carries two triggers: ETB gain 7 life, and dies draw a card.
        use crate::ability::TargetSpec;
        let db = db();

        // ETB: cast it and resolve the enters trigger.
        let mut state = main_phase_p0();
        let life_before = state.players[0].life;
        let wurm = state.new_instance(fixture("pelakka_wurm"));
        state.players[0].hand = vec![wurm];
        state.players[0].mana_pool.add(Color::Green, 3);
        state.players[0].mana_pool.colorless = 4;
        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: wurm,
                targets: Vec::new(),
            },
            &db,
        );
        let state = pass_full_round(&state, &db); // resolves the creature; ETB on stack
        assert_eq!(
            state.stack.len(),
            1,
            "the ETB gain-life trigger is on the stack"
        );
        let state = pass_full_round(&state, &db); // resolves the ETB trigger
        assert_eq!(state.players[0].life, life_before + 7);

        // Dies: place one and destroy it, then resolve the dies-draw trigger.
        let mut state = main_phase_p0();
        let onbf = place_permanent(&mut state, fixture("pelakka_wurm"), PlayerId(0), false, 0);
        let card = state.new_instance(fixture("forest"));
        state.players[0].library = vec![card];
        push_ability(
            &mut state,
            onbf,
            vec![Effect::Destroy {
                target: TargetSpec::AnyCreature,
            }],
            vec![Target::Permanent(onbf)],
        );
        let state = apply_action(&state, &Action::PassPriority, &db);
        let state = apply_action(&state, &Action::PassPriority, &db);
        assert!(!alive(&state, onbf), "the Wurm died to the Destroy");
        assert_eq!(
            state.stack.len(),
            1,
            "the dies-draw trigger is on the stack"
        );
        let state = pass_full_round(&state, &db);
        assert!(state.players[0].hand.contains(&card));
    }
}
