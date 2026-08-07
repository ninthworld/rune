//! Announcing an action and paying for it: playing a land, activating an ability,
//! casting a spell, and the additional cost that rides along with a cast.

use super::*;

#[cfg(test)]
mod tests;

/// What `payment` records about itself, read **before** any of it is charged
/// (CR 601.2h).
///
/// The one moment this number exists. A cost is paid as the object goes on the stack, so
/// by the time it resolves the permanents the payment named are in a graveyard with no
/// [`PermanentId`] — or, for a token, nowhere at all (CR 111.7). Asking then would be
/// asking about objects that have gone; asking here is CR 608.2h's last-known
/// information, taken while it is still merely current information.
///
/// The power is the **computed** one (CR 613), so a creature sacrificed while pumped was
/// worth its pumped power, which is what a player watching the board would expect.
pub(crate) fn paid_cost(
    state: &GameState,
    db: &CardDatabase,
    payment: &[crate::CostPayment],
    source: Option<crate::PermanentId>,
) -> crate::PaidCost {
    crate::PaidCost {
        // The source's own power, before a cost that sacrifices it takes it away.
        source_power: source
            .and_then(|id| crate::characteristics::characteristics(state, id, db).power),
        // The first creature the payment named — see [`PaidCost::sacrificed_power`] for
        // why "the sacrificed creature" is a phrase only a one-creature cost prints.
        sacrificed_power: crate::actions::sacrifices_of(payment)
            .iter()
            .find_map(|id| crate::characteristics::characteristics(state, *id, db).power),
    }
}

/// Move the cards a cost exiled out of `payer`'s graveyard and into their exile zone
/// (CR 601.2b / 701.19).
///
/// The graveyard counterpart of [`crate::choice::discard_to_cost`], and deliberately not a
/// death: nothing leaves the battlefield, so no dies trigger sees it and no state-based
/// action is owed. Both zones are public, so the movement needs no log entry of its own —
/// the card is simply in the other pile.
///
/// [`action_is_legal`](crate::apply_action) has already established that each named card
/// is in that graveyard and of the class the cost accepts, so this moves rather than
/// re-deciding; a card that is somehow not there is skipped rather than fabricated.
pub(crate) fn exile_to_cost(
    state: &mut GameState,
    payer: PlayerId,
    cards: &[crate::id::CardInstanceId],
) {
    for id in cards {
        let Some(player) = state.players.get_mut(payer.0) else {
            return;
        };
        let Some(pos) = player.graveyard.iter().position(|card| card.id == *id) else {
            continue;
        };
        let card = player.graveyard.remove(pos);
        player.exile.push(card);
    }
}

/// Play a land from the active player's hand — or, under a permission
/// ([`PlayerModification::PlayLandsFromGraveyard`](crate::ability::PlayerModification)),
/// from their graveyard — onto the battlefield. Not via the stack (CR 116.2a); a fresh
/// [`PermanentId`] is minted on entry while the card's [`crate::CardInstanceId`] carries
/// over unchanged.
///
/// The zone is *discovered* here rather than carried on the action, exactly as a cast
/// discovers whether its card is in a hand, a graveyard, or the command zone: one
/// instance is in one zone, so naming it is naming where it is. Whether it was allowed to
/// come from there was settled by [`crate::valid_actions`], which is the only thing that
/// offers this action and the gate `action_is_legal` re-runs.
pub(crate) fn apply_play_land(state: &mut GameState, card: CardInstance, db: &CardDatabase) {
    let controller = state.priority;
    {
        let Some(player) = state.players.get_mut(controller.0) else {
            return;
        };
        // The three zones a land can be played from, and the offer decides which are
        // reachable: the hand always, a graveyard under a continuous permission (CR 305.9
        // — Crucible of Worlds), and exile under a per-turn one naming that very card
        // ([`Effect::ExileTopForPlay`], issue #723). A card in none of them is a play the
        // offer never made, and doing nothing is the honest answer to it.
        if let Some(pos) = player.hand.iter().position(|&c| c.id == card.id) {
            player.hand.remove(pos);
        } else if let Some(pos) = player.graveyard.iter().position(|&c| c.id == card.id) {
            player.graveyard.remove(pos);
        } else if let Some(pos) = player.exile.iter().position(|&c| c.id == card.id) {
            player.exile.remove(pos);
        } else if let Some(pos) = player.library.iter().position(|&c| c.id == card.id) {
            // And a land offered off the top of a library (CR 608.2f, issue #787).
            player.library.remove(pos);
        } else {
            return;
        }
    }
    // CR 305.1: playing a land puts it onto the battlefield, which is a battlefield
    // entry like any other — so it goes through the one entry seam and its CR 614
    // replacement layer (a tapland's "enters tapped" among them), rather than building a
    // permanent here where a replacement could never see it.
    state.put_card_onto_battlefield(card, controller, false, None, db);
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
    payment: &[crate::CostPayment],
    db: &CardDatabase,
) {
    let Some(perm) = state.battlefield.iter().find(|p| p.id == permanent) else {
        return;
    };
    let controller = crate::characteristics::controller_of(state, perm);
    let Some(ability) = abilities_of_permanent(state, db, perm).get(index).cloned() else {
        return;
    };
    let Ability::Activated { cost, effects, .. } = &ability else {
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
            Cost::Tap
            | Cost::Loyalty { .. }
            | Cost::SacrificeThis
            | Cost::RemoveCounters { .. }
            | Cost::Sacrifice { .. }
            | Cost::ExileFromGraveyard { .. }
            | Cost::Discard { .. } => None,
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
            // CR 118.3: spend the counters the cost names. Only ever reached for a cost
            // `cost_payable` has already found the permanent holds, so the removal
            // cannot underflow — it saturates anyway, since a cost that could is a bug.
            Cost::RemoveCounters { counter, count } => {
                if let Some(p) = state.battlefield.iter_mut().find(|p| p.id == permanent) {
                    let held = p.counters.entry(*counter).or_insert(0);
                    *held = held.saturating_sub(*count);
                }
            }
            // Already settled against the pool above.
            Cost::Mana { .. } => {}
            // Applied after this loop: sacrificing the source first would leave a `{T}`
            // beside it with nothing to tap.
            Cost::SacrificeThis => {}
            // Paid from what the action named, below — for the same ordering reason, and
            // because nothing here re-decides what may pay a cost: `action_is_legal` has
            // already established that the payment is exactly what this demands.
            Cost::Sacrifice { .. } | Cost::ExileFromGraveyard { .. } | Cost::Discard { .. } => {}
        }
    }

    // CR 601.2h: what the payment recorded, read **before** anything it names leaves.
    // Nothing downstream could recover it — the permanents are about to stop being
    // permanents — so this is the one moment the numbers exist.
    let paid = crate::apply::paid_cost(state, db, payment, Some(permanent));

    // CR 601.2b: the components of the cost the *player* chose, charged from what the
    // action carried. A discard is the same move a discard made any other way performs
    // (CR 701.8), a sacrifice the same move any other sacrifice makes (CR 701.17), and an
    // exile the same graveyard→exile move — down one path each, so what they trigger is
    // collected by `apply_action` diffing the whole action and needs no trigger pass of
    // its own.
    crate::choice::discard_to_cost(state, controller, &crate::actions::discards_of(payment));
    crate::apply::exile_to_cost(state, controller, &crate::actions::exiles_of(payment));
    for sacrificed in crate::actions::sacrifices_of(payment) {
        state.move_permanent_to_graveyard(sacrificed);
    }

    // CR 701.17: the source's own sacrifice, last, so every other component of the cost was
    // charged against a permanent that was still on the battlefield. This is a real death —
    // it goes through the one leaves-battlefield seam, so a dies trigger (including the
    // source's own) observes it in the diff `apply_action` takes of the whole action.
    if cost.contains(&Cost::SacrificeThis) {
        state.move_permanent_to_graveyard(permanent);
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
            match crate::choice::choices_for_effect(
                state,
                effect,
                controller,
                Some(crate::stack::AbilitySource::Permanent(permanent)),
                &[],
                crate::Resolution::default(),
                db,
            ) {
                Some(choices) => {
                    crate::choice::pose_choices(state, choices, db);
                }
                // A mana ability has no resolution to be a window over — it never uses
                // the stack (CR 605.1a) — and every effect it may carry is a mana verb,
                // none of which reads an amount off the game. "From here on" is the
                // honest window, and nothing consults it.
                None => apply_effect(
                    state,
                    effect,
                    controller,
                    Some(crate::stack::AbilitySource::Permanent(permanent)),
                    crate::resolve::Resolution::at(state.next_log_sequence),
                    db,
                ),
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
            paid,
        });
        state.consecutive_passes = 0;
    }
}

/// Activate ability `index` of the card `card` in the priority holder's **graveyard**,
/// paying its cost (CR 113.6, issue #723).
///
/// The graveyard counterpart of [`apply_activate_ability`], and separate from it for the
/// reason the action is: there is no permanent here. Nothing is tapped, nothing is
/// sacrificed, and no counter is spent — the cost is mana and only mana
/// ([`graveyard_cost_payable`](crate::actions) has already established that, and the
/// catalog validator that the card could not author anything else) — and the card does
/// **not** leave the graveyard on activation. It stays exactly where it is until the
/// ability resolves, which is what makes an opponent's response to it meaningful: exiling
/// the card in reply leaves the ability on the stack with nothing to return.
///
/// A graveyard ability is never a mana ability ([`is_mana_ability`] requires every effect
/// to be a mana verb), so this always uses the stack: the object goes on with an
/// [`AbilitySource::GraveyardCard`](crate::AbilitySource) naming the physical copy that
/// was paid for, and the activator retains priority. Everything after that — the response
/// window, the resolution, the CR 608.2b target re-check — is the ordinary path.
///
/// A stale card or index is a no-op; [`crate::apply_action`]'s gate has already re-derived
/// both, so reaching either `return` would mean the gate was bypassed.
pub(crate) fn apply_activate_ability_from_graveyard(
    state: &mut GameState,
    card: CardInstance,
    index: usize,
    targets: &[Target],
    payment: &[crate::CostPayment],
    db: &CardDatabase,
) {
    let controller = state.priority;
    let Some(Ability::Activated { cost, effects, .. }) =
        crate::actions::graveyard_ability(state, db, controller, card, index)
    else {
        return;
    };

    // CR 601.2h / 602.2b: the cost is paid all at once, and a payment that cannot be made
    // leaves the state untouched. The mana half is one settlement against the pool.
    for due in cost.iter().filter_map(|c| match c {
        Cost::Mana { mana } => Some(parse_mana_cost(mana)),
        Cost::Tap
        | Cost::Loyalty { .. }
        | Cost::SacrificeThis
        | Cost::RemoveCounters { .. }
        | Cost::Sacrifice { .. }
        | Cost::ExileFromGraveyard { .. }
        | Cost::Discard { .. } => None,
    }) {
        let Some(player) = state.players.get_mut(controller.0) else {
            return;
        };
        let Some(paid) = player.mana_pool.pay(&due) else {
            return;
        };
        player.mana_pool = paid;
    }

    // And the components the *player* chose (CR 601.2b) — which cards leave this graveyard,
    // which leave the hand — charged from what the action carried, down the same movers a
    // cast and a battlefield activation use. `action_is_legal` has already established that
    // the payment is exactly what this cost demands, including that the source is not among
    // the cards being exiled to pay for returning it (issue #723), so this moves rather than
    // re-deciding.
    //
    // **After the mana and before the ability reaches the stack.** A card exiled here is
    // gone from the graveyard the ability is about to leave, which is what stops the source
    // paying for itself even if some future path forgets the *other* rule.
    crate::apply::exile_to_cost(state, controller, &crate::actions::exiles_of(payment));
    crate::choice::discard_to_cost(state, controller, &crate::actions::discards_of(payment));

    let id = state.mint_id();
    state.stack.push(StackObject {
        id: StackId(id),
        controller,
        kind: StackObjectKind::Ability {
            source: crate::stack::AbilitySource::GraveyardCard(card),
            origin: AbilityOrigin::Activated,
            effects: effects.clone(),
        },
        targets: targets.to_vec(),
        // A graveyard activation's cost is mana and only mana, so its payment records
        // nothing (`graveyard_cost_payable`).
        paid: crate::PaidCost::default(),
    });
    state.consecutive_passes = 0;
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
#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_cast_spell(
    state: &mut GameState,
    card: CardInstance,
    mode: Option<u8>,
    x: Option<u32>,
    targets: &[Target],
    payment: &[crate::CostPayment],
    db: &CardDatabase,
) {
    let controller = state.priority;
    let Some(data) = db.card(card.card) else {
        return;
    };
    // The one answer the offer was gated on and the payment search assembled against —
    // the announced X folded in, plus the commander tax where it applies (CR 903.8),
    // after every cost modification in force (CR 601.2f). Charging anything else here is
    // how a seat gets advertised a discount and then refused, and with an X in the cost
    // it is a spell paid for at the wrong price.
    let Some((cost, _)) = crate::actions::cast_cost(state, db, card, x) else {
        return;
    };
    // Which pile the card leaves. One instance is in one zone, so naming it is naming
    // where it is; the command zone is called out because a cast from there also raises
    // the tax for the next one (CR 903.8).
    let from_command = state
        .players
        .get(controller.0)
        .is_some_and(|p| p.command.iter().any(|c| c.id == card.id));
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
        } else if let Some(pos) = player.exile.iter().position(|&c| c.id == card.id) {
            // The same, one zone over: a card cast from exile under a per-turn permission
            // naming that very card ([`Effect::ExileTopForPlay`], issue #723). It leaves
            // exile for the stack exactly as a hand cast does, and everything downstream —
            // countering, fizzling, resolving as a permanent — is the ordinary path.
            player.exile.remove(pos);
        } else if let Some(pos) = player.library.iter().position(|&c| c.id == card.id) {
            // The top of a library, while a resolution is offering that very card
            // (CR 608.2f, issue #787). The offer gate has already established that this is
            // the card the pending question named — nothing else makes a library card
            // castable — so this moves it and lets the ordinary path take over.
            player.library.remove(pos);
        } else {
            return;
        }
        player.mana_pool = new_pool;
    }
    // CR 601.2h: what the payment records, read while the permanents it names are still
    // permanents. The card is on the stack a line below and the sacrifices happen a few
    // lines after that; by then the creature whose power `Thud` reads has left, which is
    // exactly why the number is taken here and stored rather than asked for later.
    let paid = crate::apply::paid_cost(state, db, payment, None);
    let id = state.mint_id();
    state.stack.push(StackObject {
        id: StackId(id),
        controller,
        // The announcement travels with the object (CR 601.2b): the mode decides which
        // effects will resolve, and the X is now locked — the same number that was just
        // charged is the one the resolution and every reader of this object will see.
        kind: StackObjectKind::Spell { card, mode, x },
        // The targets chosen as part of casting this spell (CR 601.2c), already
        // validated against freshly computed legal sets in `action_is_legal` and
        // re-checked once more on resolution (CR 608.2b). Empty for a spell that
        // targets nothing.
        targets: targets.to_vec(),
        paid,
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
    // A cast's additional cost never exiles — that shape exists only on an activation —
    // and `payment_covers_cast` refuses a payment that names one, so there is nothing to
    // charge here beyond the two below.
    // The same move a discard made any other way performs (CR 701.8), so a card
    // discarded to a cost lands in the graveyard, logs as a count rather than a name,
    // and fires what a discard fires — down one path, not two.
    // Whatever a discard triggers is collected by `apply_action` diffing the whole
    // action, so paying the cost here needs no trigger pass of its own.
    let _ = db;
    crate::choice::discard_to_cost(state, controller, &crate::actions::discards_of(payment));
    // CR 701.17: the same move any other sacrifice makes, down the one
    // leaves-battlefield seam — so a permanent sacrificed to a cost is a real death that
    // its own dies trigger, and every other watcher, sees in the diff `apply_action`
    // takes of the whole action. Nothing here re-decides what may be sacrificed;
    // `action_is_legal` has already established that.
    for permanent in crate::actions::sacrifices_of(payment) {
        state.move_permanent_to_graveyard(permanent);
    }
}
