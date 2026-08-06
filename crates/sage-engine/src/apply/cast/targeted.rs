//! Applying one [`Effect`] to the [`Target`] its caster chose, after the resolve path
//! re-checked that the target is still legal (CR 608.2b).

use super::*;
use crate::card::Keyword;

#[cfg(test)]
mod fight_tests;
#[cfg(test)]
mod modification_tests;
#[cfg(test)]
mod removal_tests;

/// Apply a targeting [`Effect`] to its already-legality-checked chosen
/// [`Target`], on behalf of `controller`.
///
/// The caller (the resolve path) is responsible for re-checking the target's
/// legality first (CR 608.2b) and only invoking this for a target that is still
/// legal; a mismatched target-value kind is a no-op here. Effects with an
/// implicit subject never reach this function — they route through
/// [`apply_effect`].
///
/// `source` is what the resolving object came from (CR 113.3) — the same reference
/// [`apply_effect`] takes — and `None` for a spell, which has no source object of its own.
/// Two effects here read it, both because they name an object no player chose: an effect
/// that says "attacking" is asking about the attack its own source is making, and
/// [`Effect::Attach`] names *two* objects, the host it targets and the source it moves.
/// Both want a permanent, so both go through [`AbilitySource::permanent`](crate::AbilitySource),
/// which answers `None` for an emblem and for a card in a graveyard — neither is one.
///
/// `resolution` is what the resolving object knows about itself, carried for the same
/// reason [`apply_effect`] carries it: an amount that says "this way" is a question about
/// what this resolution has already done, an amount that says "X" is a question about
/// what it announced, and whether its damage can be prevented is a fact about the object
/// rather than about the recipient.
pub(crate) fn apply_targeted_effect(
    state: &mut GameState,
    effect: &Effect,
    target: Target,
    controller: PlayerId,
    source: Option<crate::stack::AbilitySource>,
    resolution: crate::resolve::Resolution,
    db: &CardDatabase,
) {
    let source = source.and_then(crate::stack::AbilitySource::permanent);
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
                // CR 701.5a: a spell that can't be countered stays on the stack. It was
                // a perfectly legal target — "can't be countered" is not hexproof and
                // does not touch targeting — so the counterspell resolved, chose it, and
                // simply failed to remove it. Asked of the object rather than of its
                // card, because the answer depends on the X *this* copy announced.
                let protected = state
                    .stack
                    .iter()
                    .find(|o| o.id == id)
                    .is_some_and(|o| o.has_trait(db, crate::stack::SpellTraitKind::CantBeCountered));
                if let Some(pos) = (!protected)
                    .then(|| state.stack.iter().position(|o| o.id == id))
                    .flatten()
                {
                    let countered = state.stack.remove(pos);
                    if let StackObjectKind::Spell { card, .. } = countered.kind {
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
        // record the damage (including nonlethal) as a `DamageDealt` event. A
        // class-subject damage effect chose no target and never reaches here — it is
        // applied by [`apply_effect`] over the class it names.
        // The count-derived damage verb: X is taken here, on resolution (CR 608.2),
        // from the board as it stands — the same moment the fixed verb's amount is read
        // off the card, and the same seams take it from there.
        Effect::DealDamageByCount {
            amount_per,
            count_of,
            ..
        } => {
            let count = crate::condition::count_permanents(state, count_of, controller, db);
            let amount = amount_per.saturating_mul(count);
            deal_damage_to_target(state, target, amount, resolution, db);
        }
        Effect::DealDamage { amount, .. } => {
            deal_damage_to_target(state, target, *amount, resolution, db);
        }
        // The announced-amount damage verb: X was fixed at announcement (CR 601.2b), so
        // reading it here is a lookup rather than a computation, and it is the same
        // number the cast was charged for.
        Effect::DealDamageByAmount { amount, .. } => {
            let value = crate::condition::derived_amount(state, amount, controller, resolution, db);
            deal_damage_to_target(state, target, value, resolution, db);
        }
        // Destroy the targeted permanent (CR 701.7): move it to its owner's
        // graveyard through the one creature-death seam
        // ([`GameState::destroy_permanent`], CR 700.4) — the same path lethal damage
        // uses in the SBA loop, so this death fires the dies trigger (CR 603.6c) and
        // logs a `permanent_died` identically. Regeneration is out of scope.
        Effect::Destroy { .. } => {
            if let Target::Permanent(id) = target {
                // CR 702.12: an indestructible permanent is not destroyed. The effect
                // still resolves and still had a legal target — it simply does nothing
                // to it, which is what "can't be destroyed" means.
                if !crate::characteristics::permanent_has_keyword(
                    state,
                    id,
                    crate::card::Keyword::Indestructible,
                    db,
                ) {
                    state.destroy_permanent(id, db);
                }
            }
        }
        // Exile the targeted permanent (CR 406.2 / CR 701.19): move it from the
        // battlefield to its owner's exile zone through the one battlefield→exile seam
        // ([`GameState::move_permanent_to_exile`]) — the exile counterpart of the
        // graveyard path `Destroy` uses. A commander exiled here is flagged for the
        // CR 903.9a return-to-command-zone decision by that seam.
        Effect::Exile { .. } => {
            if let Target::Permanent(id) = target {
                state.move_permanent_to_exile(id);
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
        // invalidate (ADR 0005). The caller has re-checked the target is still a
        // creature (CR 608.2b); a permanent that has since left is skipped.
        // Any keywords the same effect grants ride along on the same target and the
        // same duration, each its own layer-6 modification — the two halves of
        // "gets +2/+2 and gains flying" applied to one creature because one effect
        // chose one target.
        Effect::Pump {
            power,
            toughness,
            keywords,
            restrictions,
            ..
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
                    for keyword in keywords {
                        let source = state.mint_id();
                        state.static_effects.push(StaticEffect {
                            source,
                            affects: EffectAffects::SpecificPermanent(id),
                            modification: Modification::GrantKeyword(*keyword),
                            duration: Duration::UntilEndOfTurn,
                        });
                    }
                    // The same creature, in the same breath — a CR 613 layer-6
                    // imposition the cleanup step removes (CR 514.2), exactly as the
                    // standalone restrict verb adds one.
                    for restriction in restrictions {
                        let source = state.mint_id();
                        state.static_effects.push(StaticEffect {
                            source,
                            affects: EffectAffects::SpecificPermanent(id),
                            modification: Modification::GrantRestriction(restriction.clone()),
                            duration: Duration::UntilEndOfTurn,
                        });
                    }
                }
            }
        }
        // Grant the targeted creature a keyword until end of turn (CR 514.2): add a
        // CR 613 layer-6 keyword grant keyed to that one permanent, with an
        // `UntilEndOfTurn` duration the cleanup step removes (CR 613.1f). The grant
        // folds into the target's computed keyword set on demand — nothing is written
        // onto the permanent — so removing it at cleanup reverts the value with
        // nothing to invalidate (ADR 0005). A duplicate grant is redundant, not
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
        // Impose a combat restriction on the targeted creature until end of turn
        // (CR 514.2): a CR 613 layer-6 imposition keyed to that one permanent, the
        // exact counterpart of the keyword grant above. It folds into the target's
        // computed restrictions on demand — nothing is written onto the permanent —
        // so cleanup reverts it with nothing to invalidate (ADR 0005), and a
        // duplicate imposition is redundant rather than additive.
        Effect::Restrict { restriction, .. } => {
            if let Target::Permanent(id) = target {
                if state.battlefield.iter().any(|p| p.id == id) {
                    let source = state.mint_id();
                    state.static_effects.push(StaticEffect {
                        source,
                        affects: EffectAffects::SpecificPermanent(id),
                        modification: Modification::GrantRestriction(restriction.clone()),
                        duration: Duration::UntilEndOfTurn,
                    });
                }
            }
        }
        // Gain control of the targeted creature until end of turn (CR 613 layer 2):
        // a timestamped control change keyed to that one permanent, with an
        // `UntilEndOfTurn` duration the cleanup step removes. Nothing is written onto
        // the permanent's stored controller, so cleanup reverts control with nothing to
        // invalidate (ADR 0005) — and the stored field goes on standing in for the
        // owner, which is what sends the creature to *its own* graveyard if it dies
        // while stolen (CR 400.7).
        //
        // Three things happen to one creature because one effect chose one target: the
        // control change, the untap, and any keywords (haste, in practice). The untap
        // and the keywords are applied *after* the control change so a reader of the
        // resulting state sees a coherent object, and `entered_turn` is restamped
        // because a creature that has just changed hands has not been controlled by its
        // new controller since their turn began (CR 302.6).
        Effect::GainControl {
            untap, keywords, ..
        } => {
            if let Target::Permanent(id) = target {
                if state.battlefield.iter().any(|p| p.id == id) {
                    let source = state.mint_id();
                    state.static_effects.push(StaticEffect {
                        source,
                        affects: EffectAffects::SpecificPermanent(id),
                        modification: Modification::GainControl(controller),
                        duration: Duration::UntilEndOfTurn,
                    });
                    for keyword in keywords {
                        let source = state.mint_id();
                        state.static_effects.push(StaticEffect {
                            source,
                            affects: EffectAffects::SpecificPermanent(id),
                            modification: Modification::GrantKeyword(*keyword),
                            duration: Duration::UntilEndOfTurn,
                        });
                    }
                    let turn = state.turn;
                    if let Some(perm) = state.battlefield.iter_mut().find(|p| p.id == id) {
                        // CR 302.6: summoning sickness is measured from when the
                        // permanent came under its controller's control, and that is
                        // now.
                        perm.entered_turn = turn;
                        if *untap {
                            perm.tapped = false;
                        }
                    }
                }
            }
        }
        // Return the targeted permanent to its owner's hand (CR 400.7): the bounce
        // verb, moving it through the one battlefield→hand seam
        // ([`GameState::return_permanent_to_hand`]) — the hand counterpart of the
        // graveyard path `Destroy` uses and the exile path `Exile` uses. It is not a
        // death, so no dies trigger fires and nothing is logged as one.
        Effect::ReturnToHand { .. } => {
            if let Target::Permanent(id) = target {
                state.return_permanent_to_hand(id);
            }
        }
        // A **targeted** player reference (CR 115.1): the chosen player is the
        // subject, in place of the seats `non_targeting_subjects` would have named. The
        // non-targeting refs never reach here — they are applied in `apply_effect`.
        Effect::GainLife { amount, .. } => {
            if let Target::Player(seat) = target {
                state.change_life(seat, i32::try_from(*amount).unwrap_or(i32::MAX));
            }
        }
        Effect::LoseLife { amount, .. } => {
            if let Target::Player(seat) = target {
                state.change_life(seat, -i32::try_from(*amount).unwrap_or(i32::MAX));
            }
        }
        Effect::Mill { count, .. } => {
            if let Target::Player(seat) = target {
                state.mill(seat, u32::from(*count));
            }
        }
        // "Tap all creatures target player controls" (CR 502.4 / 611.2c): the chosen
        // seat's creatures, enumerated now. Shares the one tapping function with the
        // non-targeting spelling, so the two cannot disagree about what they tap.
        Effect::TapAll {
            skip_next_untap, ..
        } => {
            if let Target::Player(seat) = target {
                super::effects::tap_creatures_of(state, seat, *skip_next_untap, db);
            }
        }
        // "Target player creates …" (CR 111.1): the token is created under the chosen
        // seat's control, which is the whole reason the creator is a reference rather
        // than an assumption about the ability's controller. Which attack an attacking
        // token joins is answered by the one function the non-targeting spelling uses,
        // so the chosen seat is judged against the source exactly as an implicit one is.
        Effect::CreateToken {
            token,
            count,
            count_of,
            tapped,
            attacking,
            ..
        } => {
            if let Target::Player(seat) = target {
                let joins =
                    super::effects::attack_a_created_token_joins(state, *attacking, source, seat);
                // Counted by the same function the non-targeting spelling uses, and
                // relative to the effect's controller rather than to the chosen creator.
                let made = super::effects::tokens_created(
                    state,
                    *count,
                    count_of.as_ref(),
                    controller,
                    db,
                );
                for _ in 0..made {
                    state.create_token(token.clone(), seat, *tapped, joins, db);
                }
            }
        }
        // A choice-posing effect is intercepted by the resolve loop before either
        // apply function sees it (see [`apply_effect`]).
        Effect::Discard { .. }
        | Effect::Scry { .. }
        | Effect::LookAtTop { .. }
        | Effect::SearchLibrary { .. }
        | Effect::May { .. } => {}
        // Shrink (or pump) the targeted creature by a count taken **now** (CR 608.2):
        // X is computed once, on resolution, and the fixed modifier that results is what
        // the layer system folds in — so a Zombie dying later in the turn does not give
        // the creature its toughness back.
        Effect::PumpByCount {
            power_per,
            toughness_per,
            count_of,
            ..
        } => {
            let count = i32::try_from(crate::condition::count_permanents(
                state, count_of, controller, db,
            ))
            .unwrap_or(i32::MAX);
            pump_by(state, target, *power_per, *toughness_per, count);
        }
        // The same freeze, with X off a source that is not a count of permanents: the
        // amount is read here, on resolution, and the modifier that results is fixed —
        // life gained later this turn does not shrink the creature any further.
        Effect::PumpByAmount {
            power_per,
            toughness_per,
            amount,
            ..
        } => {
            let count = i32::try_from(crate::condition::derived_amount(
                state,
                amount,
                controller,
                resolution,
                db,
            ))
            .unwrap_or(i32::MAX);
            pump_by(state, target, *power_per, *toughness_per, count);
        }
        // Return the targeted card from a graveyard to the battlefield. It goes through
        // the one card→battlefield seam, so it mints a fresh `PermanentId`, applies its
        // own enters-the-battlefield replacements, and is seen by the trigger diff
        // exactly as a resolving creature spell is. The caller has re-checked that the
        // card is still there and still matches (CR 608.2b).
        Effect::ReturnCardToBattlefield { tapped, .. } => {
            if let Target::Card(instance) = target {
                if let Some(card) = take_from_a_graveyard(state, instance) {
                    state.put_card_onto_battlefield(card, controller, *tapped, None, db);
                }
            }
        }
        // The graveyard→hand counterpart. The card goes to its **owner's** hand
        // (CR 400.7), which for a card in a graveyard is the seat whose graveyard it
        // was in — so a reanimation-to-hand of an opponent's card hands it back to
        // them, which is what every card printed this way says.
        Effect::ReturnCardToHand { .. } => {
            if let Target::Card(instance) = target {
                if let Some((owner, card)) = take_from_a_graveyard_with_owner(state, instance) {
                    if let Some(player) = state.players.get_mut(owner.0) {
                        player.hand.push(card);
                    }
                }
            }
        }
        // Implicit-subject and class-scoped effects do not target; they never reach
        // here, and are applied by [`apply_effect`].
        Effect::AddMana { .. }
        | Effect::AddColorlessMana { .. }
        | Effect::AddRestrictedMana { .. }
        | Effect::AddManaAnyColor { .. }
        | Effect::DrawCard { .. }
        | Effect::CreateEmblem { .. }
        | Effect::AllowCastingFromGraveyard { .. }
        | Effect::IgnoreHexproof { .. }
        | Effect::CreateReplacement { .. }
        | Effect::PreventDamage { .. }
        | Effect::Conditional { .. }
        | Effect::DestroyAll { .. }
        | Effect::PumpAll { .. }
        | Effect::GrantKeywordAll { .. }
        | Effect::RestrictAll { .. }
        | Effect::PumpSelf { .. }
        | Effect::RestrictSelf { .. }
        | Effect::AlterAbilitiesSelf { .. }
        | Effect::GainLifeByCount { .. }
        | Effect::DrawCardsByAmount { .. }
        // A card returning itself out of a graveyard names its own source, never a
        // chosen one (CR 115.1), so it too is applied by [`apply_effect`].
        | Effect::ReturnSelfFromGraveyard { .. }
        | Effect::PutCountersOnSelf { .. } => {}
        // "Target player's graveyard": the targeting form of the same verb, routed here
        // for the reason a targeted mill is — the reference chose a seat, and this is
        // where a chosen seat arrives.
        Effect::ExileGraveyard { .. } => {
            if let Target::Player(seat) = target {
                if let Some(player) = state.players.get_mut(seat.0) {
                    let cards: Vec<_> = player.graveyard.drain(..).collect();
                    player.exile.extend(cards);
                }
            }
        }
        // Attach the ability's own source to the targeted permanent — the equip action
        // (CR 702.6b), and the one effect whose subject is not the target.
        //
        // Writing the one field *is* the move (CR 701.3c): an Equipment already attached
        // to another creature becomes unattached from it and attached to this one in the
        // same step, and because the grant is derived from the attachment on every read
        // (ADR 0005), the old host loses it and the new host gains it with nothing to
        // migrate. A source that has left the battlefield since the ability was activated
        // is not there to attach, and the effect does nothing — the Equipment was
        // destroyed in response, and the creature simply keeps standing there.
        Effect::Attach { .. } => {
            if let (Some(equipment), Target::Permanent(host)) = (source, target) {
                if let Some(perm) = state.battlefield.iter_mut().find(|p| p.id == equipment) {
                    perm.attached_to = Some(host);
                }
            }
        }
        // A fight declares two target groups, so the resolve path routes it to
        // [`apply_multi_target_effect`] with both of its targets at once; one target on
        // its own says nothing about which slot it filled, so this arm stays empty.
        Effect::Fight { .. } => {}
        // Put the targeted permanent on top of its owner's library (CR 400.7). A token
        // put anywhere but the battlefield ceases to exist (CR 111.7), so it never
        // arrives — which the one leaves-battlefield seam below already knows.
        Effect::PutOnTopOfLibrary { .. } => {
            if let Target::Permanent(id) = target {
                state.put_permanent_on_top_of_library(id);
            }
        }
    }
}

/// Deal `amount` damage to the one object a targeting damage effect chose (CR 120.3).
///
/// Three damage verbs share it — the printed amount, the count-derived one, and the
/// announced-X one — so the seam they all funnel into is written once, and the
/// resolution's "can't be prevented" declaration (CR 615.1) is attached in one place
/// rather than three. A card or a spell is never a damage recipient and is simply
/// ignored: the target specs cannot produce one, and a silent skip is the right answer
/// for a pairing the type system permits and the rules do not.
fn deal_damage_to_target(
    state: &mut GameState,
    target: Target,
    amount: u32,
    resolution: crate::resolve::Resolution,
    db: &CardDatabase,
) {
    match target {
        Target::Permanent(id) => {
            state.deal_damage(
                resolution.damage(PendingDamage::to_permanent(id, amount)),
                db,
            );
        }
        Target::Player(seat) => {
            state.deal_damage(
                resolution.damage(PendingDamage::to_player(seat, amount)),
                db,
            );
        }
        Target::Card(_) | Target::Spell(_) => {}
    }
}

/// Give `target` a fixed `power_per`/`toughness_per` **times** `units` until end of turn
/// (CR 613 layer 7c).
///
/// The shared tail of the two X-derived pumps: by the time either reaches here X is a
/// number, so nothing below knows whether it came from a count of permanents or from a
/// [`DerivedAmount`](crate::DerivedAmount). Freezing it into one timestamped modifier is
/// the whole point — a selector re-evaluated on every read would give a shrunk creature
/// its toughness back the moment the board changed.
///
/// A target that is no longer on the battlefield gets nothing; the caller has already
/// re-checked legality (CR 608.2b), and this is the narrower question of whether the
/// permanent is still there to modify.
fn pump_by(state: &mut GameState, target: Target, power_per: i32, toughness_per: i32, units: i32) {
    let Target::Permanent(id) = target else {
        return;
    };
    if !state.battlefield.iter().any(|p| p.id == id) {
        return;
    }
    let source = state.mint_id();
    state.static_effects.push(StaticEffect {
        source,
        affects: EffectAffects::SpecificPermanent(id),
        modification: Modification::PowerToughness {
            power: power_per.saturating_mul(units),
            toughness: toughness_per.saturating_mul(units),
        },
        duration: Duration::UntilEndOfTurn,
    });
}

/// Apply a targeting [`Effect`] that declares **more than one** target slot, to the
/// targets its caster chose — in slot order, and only once the resolve path has found
/// every one of them still legal (CR 608.2b).
///
/// The multi-slot counterpart of [`apply_targeted_effect`], and a separate function for
/// the reason the slots are separate: an effect that names two classes acts on the pair,
/// so applying it once per target would be a different effect. The resolve path routes
/// here only for an effect whose [`Effect::target_groups`] answered with more than one
/// group; anything else is a no-op, which keeps the pairing between the two entry points
/// total rather than assumed.
pub(crate) fn apply_multi_target_effect(
    state: &mut GameState,
    effect: &Effect,
    targets: &[Target],
    db: &CardDatabase,
) {
    // CR 701.12: the first creature deals damage equal to its power to the second, and —
    // when the card printed the word *fights* — the second deals damage equal to its
    // power back. One arm today; a second multi-slot effect joins it here.
    let Effect::Fight { mutual, .. } = effect else {
        return;
    };
    let [Target::Permanent(dealer), Target::Permanent(dealt_to)] = targets else {
        return;
    };
    let (dealer, dealt_to) = (*dealer, *dealt_to);
    // CR 701.12a: the damage is dealt **simultaneously**, so both powers are read before
    // either is applied. A creature that dies to the damage it took still dealt its own
    // power, and a `-1/-1` counter the first damage would have put on the second (were
    // there such a card) could not shrink the answer.
    let forward = power_as_damage(state, dealer, db);
    let back = if *mutual {
        power_as_damage(state, dealt_to, db)
    } else {
        0
    };
    deal_damage_between_permanents(state, dealer, dealt_to, forward, db);
    if *mutual {
        deal_damage_between_permanents(state, dealt_to, dealer, back, db);
    }
}

/// The damage a creature's power deals outside combat (CR 701.12a), floored at zero: a
/// creature with negative power, zero power, or no power at all deals none.
///
/// Read through [`crate::characteristics::characteristics`] like every other power
/// question, so counters and anthems are folded in (CR 613).
fn power_as_damage(state: &GameState, permanent: PermanentId, db: &CardDatabase) -> u32 {
    let power = crate::characteristics::characteristics(state, permanent, db)
        .power
        .unwrap_or(0);
    u32::try_from(power.max(0)).unwrap_or(0)
}

/// Deal `amount` damage from one permanent to another (CR 120.3) — the seam for damage
/// whose **source is a permanent** rather than a spell.
///
/// That source is the whole reason this exists beside
/// [`GameState::deal_damage`](crate::GameState): damage from a creature carries that
/// creature's deathtouch (CR 702.2b) and lifelink (CR 702.15e), which damage from a burn
/// spell has no way to. Both ride the same fields combat damage uses — the CR 704.5h flag
/// list and a plain life change — so a creature killed by a fight dies exactly the way one
/// killed by a block does. The damage itself still goes through the one seam, so a
/// prevention shield (CR 615.1) stops a fight exactly as it stops a block.
///
/// Zero damage is not dealt at all (CR 120.3), so it triggers nothing and gains nobody
/// life.
fn deal_damage_between_permanents(
    state: &mut GameState,
    source: PermanentId,
    recipient: PermanentId,
    amount: u32,
    db: &CardDatabase,
) {
    if amount == 0 {
        return;
    }
    // Both keywords are read off the source *before* the damage lands, for CR 608.2h's
    // reason: the damage may destroy the source's own opposite number, and a keyword is
    // read from the object as it was when the effect began.
    let deathtouch =
        crate::characteristics::permanent_has_keyword(state, source, Keyword::Deathtouch, db);
    let lifelink =
        crate::characteristics::permanent_has_keyword(state, source, Keyword::Lifelink, db);
    let gains = crate::characteristics::controller_of_id(state, source);
    let dealt = state.deal_damage(PendingDamage::to_permanent(recipient, amount), db);
    // CR 702.2b / 704.5h: any nonzero damage from a deathtouch source makes the
    // recipient a candidate for destruction, whether or not it was lethal.
    if dealt > 0 && deathtouch && !state.deathtouch_struck.contains(&recipient) {
        state.deathtouch_struck.push(recipient);
    }
    // CR 702.15e: lifelink life gain is a non-damage life change to the source's
    // controller, and it rides *damage that was dealt* — so a recipient that is not there
    // to take any, and one whose damage was prevented (CR 615.1), gain nobody anything.
    if lifelink && dealt > 0 {
        if let Some(seat) = gains {
            state.change_life(seat, i32::try_from(dealt).unwrap_or(i32::MAX));
        }
    }
}

/// Remove the card `instance` from whichever graveyard holds it, or `None` when no
/// graveyard does.
///
/// The one lookup both graveyard-return effects share. It searches **every** seat
/// rather than the effect controller's own because a
/// [`TargetSpec::CardInGraveyard`](crate::TargetSpec) may name any of them
/// ([`GraveyardScope::Any`](crate::GraveyardScope)); which graveyards were *legal* was
/// settled at announcement and re-checked on resolution, so by the time the card is
/// being moved the only question left is where it is.
fn take_from_a_graveyard(
    state: &mut GameState,
    instance: crate::id::CardInstanceId,
) -> Option<crate::id::CardInstance> {
    take_from_a_graveyard_with_owner(state, instance).map(|(_, card)| card)
}

/// [`take_from_a_graveyard`], paired with the seat it came out of — the card's owner
/// (CR 400.7), and therefore whose hand it goes back to.
pub(super) fn take_from_a_graveyard_with_owner(
    state: &mut GameState,
    instance: crate::id::CardInstanceId,
) -> Option<(PlayerId, crate::id::CardInstance)> {
    for (seat, player) in state.players.iter_mut().enumerate() {
        if let Some(pos) = player.graveyard.iter().position(|card| card.id == instance) {
            return Some((PlayerId(seat), player.graveyard.remove(pos)));
        }
    }
    None
}
