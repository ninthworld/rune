//! Announcing an action and paying for it: playing a land, activating an ability,
//! casting a spell, and the additional cost that rides along with a cast.

use super::*;

#[cfg(test)]
mod tests;

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
        printed: card.card.into(),
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
    let Some(ability) = abilities_of_permanent(db, perm).get(index).cloned() else {
        return;
    };
    let Ability::Activated { cost, effects } = &ability else {
        return;
    };

    // Costs are paid **all or nothing** (CR 601.2h): the mana portion is settled
    // against the pool first, and only once it has succeeded does anything else get
    // charged. Tapping before discovering the mana could not be paid would leave the
    // source tapped for an activation that never happened.
    let mana_due: Vec<_> = cost
        .iter()
        .filter_map(|c| match c {
            Cost::Mana { mana } => Some(parse_mana_cost(mana)),
            Cost::Tap | Cost::Loyalty { .. } => None,
        })
        .collect();
    if !mana_due.is_empty() {
        let Some(player) = state.players.get_mut(controller.0) else {
            return;
        };
        let mut pool = player.mana_pool.clone();
        for due in &mana_due {
            let Some(paid) = pool.pay(due) else {
                return;
            };
            pool = paid;
        }
        player.mana_pool = pool;
    }
    for c in cost {
        match c {
            Cost::Tap => {
                if let Some(p) = state.battlefield.iter_mut().find(|p| p.id == permanent) {
                    p.tapped = true;
                }
            }
            // CR 606.1: paying a loyalty cost puts that many loyalty counters on the
            // source, or removes them for a negative amount. Only ever reached for a
            // cost `action_is_legal` has already found payable (CR 606.3), so the
            // removal can never take the permanent below zero — but it saturates
            // anyway, since a cost that could underflow is a bug, not a death.
            Cost::Loyalty { amount } => {
                if let Some(p) = state.battlefield.iter_mut().find(|p| p.id == permanent) {
                    let counter = p
                        .counters
                        .entry(crate::state::CounterKind::Loyalty)
                        .or_insert(0);
                    *counter = match u32::try_from(*amount) {
                        Ok(gained) => counter.saturating_add(gained),
                        Err(_) => counter.saturating_sub(amount.unsigned_abs()),
                    };
                }
                // CR 606.3: this permanent has now used its one loyalty activation for
                // the turn. Recorded whatever the ability does next, so an ability that
                // fizzles on resolution still spent the allowance.
                if !state.loyalty_activations.contains(&permanent) {
                    state.loyalty_activations.push(permanent);
                }
            }
            // Already settled against the pool above.
            Cost::Mana { .. } => {}
        }
    }

    if is_mana_ability(&ability) {
        // Mana ability: resolve now, no stack object, priority unchanged.
        //
        // "Add one mana of any color" is a mana ability that still asks a question
        // (CR 605.3b permits exactly that), so its colors are posed here rather than
        // applied. No remainder is carried: every effect of a mana ability is a mana
        // verb, and mana verbs commute — the pool ends up the same whether the fixed
        // points land before or after the chosen ones.
        for effect in effects {
            match crate::choice::choices_for_effect(state, effect, controller, None, None) {
                Some(choices) => {
                    crate::choice::pose_choices(state, choices, db);
                }
                None => apply_effect(state, effect, controller, Some(permanent), db),
            }
        }
    } else {
        let id = state.mint_id();
        state.stack.push(StackObject {
            id: StackId(id),
            controller,
            kind: StackObjectKind::Ability {
                source: crate::stack::AbilitySource::Permanent(permanent),
                // This is the *activation* push site (CR 602.2): a player chose
                // this ability and paid for it above. Recording that here is the
                // only place the fact exists — the object it produces is
                // otherwise identical to a trigger's (issue #579).
                origin: AbilityOrigin::Activated,
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

/// Record the targets its controller chose for a triggered ability already on the
/// stack (CR 603.3d).
///
/// The choice is the *only* thing this does: the ability stays exactly where it is
/// and resolves later on the ordinary path, where its targets are re-checked like any
/// other object's (CR 608.2b). Legality — that the chosen targets fill every slot and
/// each is legal now — has already been established by
/// [`crate::apply_action`]'s gate, so this writes rather than re-deciding. A stale
/// stack id names nothing and is a no-op.
pub(crate) fn apply_choose_trigger_targets(
    state: &mut GameState,
    ability: StackId,
    targets: &[Target],
) {
    if let Some(object) = state.stack.iter_mut().find(|o| o.id == ability) {
        object.targets = targets.to_vec();
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
    payment: &[crate::CostPayment],
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
        // CR 106.6: restricted mana may be spent here only if this spell is what its
        // restriction allows, which is why the payment is told what it is paying for.
        let Some(new_pool) = player.mana_pool.pay_for(
            &cost,
            crate::mana::SpendPurpose::CastingSpell {
                subtypes: &data.subtypes,
            },
        ) else {
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
        } else if let Some(pos) = player.hand.iter().position(|&c| c.id == card.id) {
            player.hand.remove(pos);
        } else if let Some(pos) = player.graveyard.iter().position(|&c| c.id == card.id) {
            // A card cast from the graveyard under a permission granted this turn
            // ([`Effect::AllowCastingFromGraveyard`]). It leaves the graveyard for the
            // stack exactly as a hand cast does; if it is countered or fizzles it comes
            // back to the graveyard down the ordinary path, and if it resolves as a
            // permanent it enters the battlefield.
            player.graveyard.remove(pos);
        } else {
            return;
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
    pay_additional_cost(state, controller, data, payment, db);
}

/// Pay `data`'s additional cast cost (CR 601.2b) from the cards the payment named.
///
/// Paid **after** the card is on the stack, which is what stops a spell being discarded
/// to pay for itself, and in the same `apply_action` as everything else about the cast.
/// That last part is the change worth noting: this used to pose a
/// [`PendingChoice`](crate::PendingChoice) and ask *once the spell was already on the
/// stack*, which made the cost the one part of a cast a player could not take back —
/// by the time they were asked, there was nothing left to abandon. Now the choice
/// arrives with the action, so a player assembling it has sent nothing and may put any
/// of it back.
///
/// [`action_is_legal`](crate::apply_action) has already established that the named cards
/// are exactly what the cost demands and that each is in hand, so this discards rather
/// than re-deciding.
fn pay_additional_cost(
    state: &mut GameState,
    controller: PlayerId,
    data: &crate::CardData,
    payment: &[crate::CostPayment],
    db: &CardDatabase,
) {
    if data.additional_cost.is_none() {
        return;
    }
    // The same move a discard made any other way performs (CR 701.8), so a card
    // discarded to a cost lands in the graveyard, logs as a count rather than a name,
    // and fires what a discard fires — down one path, not two.
    // Whatever a discard triggers is collected by `apply_action` diffing the whole
    // action, so paying the cost here needs no trigger pass of its own.
    let _ = db;
    crate::choice::discard_to_cost(state, controller, &crate::actions::discards_of(payment));
}
