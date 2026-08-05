//! Applying one [`Effect`] to the [`Target`] its caster chose, after the resolve path
//! re-checked that the target is still legal (CR 608.2b).

use super::*;

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
pub(crate) fn apply_targeted_effect(
    state: &mut GameState,
    effect: &Effect,
    target: Target,
    controller: PlayerId,
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
            match target {
                Target::Permanent(id) => {
                    state.deal_damage_to_permanent(id, amount, db);
                }
                Target::Player(seat) => {
                    state.deal_damage_to_player(seat, amount);
                }
                Target::Card(_) | Target::Spell(_) => {}
            }
        }
        Effect::DealDamage { amount, .. } => match target {
            Target::Permanent(id) => {
                state.deal_damage_to_permanent(id, *amount, db);
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
                        modification: Modification::GrantRestriction(*restriction),
                        duration: Duration::UntilEndOfTurn,
                    });
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
        // than an assumption about the ability's controller.
        Effect::CreateToken {
            token,
            count,
            tapped,
            ..
        } => {
            if let Target::Player(seat) = target {
                for _ in 0..*count {
                    state.create_token(token.clone(), seat, *tapped, db);
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
            if let Target::Permanent(id) = target {
                if state.battlefield.iter().any(|p| p.id == id) {
                    let count = i32::try_from(crate::condition::count_permanents(
                        state, count_of, controller, db,
                    ))
                    .unwrap_or(i32::MAX);
                    let source = state.mint_id();
                    state.static_effects.push(StaticEffect {
                        source,
                        affects: EffectAffects::SpecificPermanent(id),
                        modification: Modification::PowerToughness {
                            power: power_per.saturating_mul(count),
                            toughness: toughness_per.saturating_mul(count),
                        },
                        duration: Duration::UntilEndOfTurn,
                    });
                }
            }
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
        | Effect::Conditional { .. }
        | Effect::PumpAll { .. }
        | Effect::GrantKeywordAll { .. }
        | Effect::RestrictAll { .. }
        | Effect::PumpSelf { .. }
        | Effect::RestrictSelf { .. }
        | Effect::GainLifeByCount { .. }
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
fn take_from_a_graveyard_with_owner(
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
