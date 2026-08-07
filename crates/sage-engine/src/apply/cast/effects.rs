//! Applying one [`Effect`] whose subject is named by class rather than chosen as a
//! target — mana, draws, life, and the battlefield-wide forms (CR 611.2c, CR 115.1).

use super::*;

#[cfg(test)]
mod tests;

/// Apply a single [`Effect`] to `state` on behalf of `controller`, resolving in a window
/// described by `resolution` (its log window, its announced X, whether its damage can be
/// prevented, and what its cost payment recorded).
///
/// The frame carries the window an intervening condition is judged over, and it is here
/// for the same reason: an amount that says "this way"
/// ([`DerivedAmount::MilledThisWay`]) is a question about what *this* resolution has
/// already done, which no snapshot of the game can answer. It also carries what paying for
/// the object recorded (CR 601.2h), which no snapshot can answer either — by now the
/// permanents that paid are gone.
pub(crate) fn apply_effect(
    state: &mut GameState,
    effect: &Effect,
    controller: PlayerId,
    source: Option<crate::stack::AbilitySource>,
    resolution: crate::resolve::Resolution,
    db: &CardDatabase,
) {
    if state.players.get(controller.0).is_none() {
        return;
    }
    // What the source is, asked of the source itself (CR 113.3). A permanent-shaped
    // self-referential effect wants this one; a graveyard-shaped one asks
    // `graveyard_card()` in its own arm below. Each is `None` when the source is not
    // that kind of object, which is exactly the no-op an absent source already produced.
    let permanent_source = source.and_then(crate::stack::AbilitySource::permanent);
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
        // "Add N mana in any combination of colors": nothing is added *here*. Each
        // point's color is a question for the controller, posed by the resolution path
        // ([`crate::choice::choices_for_effect`]) and applied one answer at a time, so
        // reaching this arm at all means the effect was applied without its questions.
        Effect::AddManaAnyColor { .. } => {}
        // CR 106.6: mana that may be spent only on certain things. It joins the same
        // pool as ordinary mana and empties with it at the end of the step (CR 500.4);
        // the restriction rides on the mana, so nothing about the pool has to remember
        // which ability made it.
        Effect::AddRestrictedMana {
            color,
            amount,
            restriction,
        } => {
            if let Some(player) = state.players.get_mut(controller.0) {
                player
                    .mana_pool
                    .add_restricted(*color, *amount, restriction.clone());
            }
        }
        // CR 114.3: the named player gets an emblem. It is minted from the same
        // monotonic counter every other object uses — which is both its identity and its
        // CR 613.7 timestamp — and then it simply exists, for the rest of the game.
        // Nothing here has to arrange for it to be cleaned up, because nothing ever
        // cleans one up.
        Effect::CreateEmblem {
            abilities,
            player_ref,
        } => {
            for seat in non_targeting_subjects(state, *player_ref, controller, resolution.chosen_player) {
                let id = state.mint_id();
                state.emblems.push(crate::state::Emblem {
                    id,
                    controller: seat,
                    abilities: abilities.clone(),
                });
            }
        }
        // A permission for the rest of the turn, recorded with the turn it was granted
        // on. Re-granting it is idempotent in effect — two identical permissions offer
        // the same cards — so no de-duplication is needed for correctness.
        Effect::AllowCastingFromGraveyard { player_ref, filter } => {
            let turn = state.turn;
            for seat in non_targeting_subjects(state, *player_ref, controller, resolution.chosen_player) {
                state.graveyard_casting.push(crate::state::GraveyardCasting {
                    player: seat,
                    filter: filter.clone(),
                    turn,
                });
            }
        }
        // Exile from the top, and a permission naming exactly what was exiled. The two
        // are one effect because only this resolution knows which cards *"that card"*
        // means — see [`Effect::ExileTopForPlay`].
        Effect::ExileTopForPlay { count, cast_only } => {
            let turn = state.turn;
            let Some(player) = state.players.get_mut(controller.0) else {
                return;
            };
            // The top of a library is its last element, so taking from the top is popping.
            // A library with fewer cards exiles what it has (CR 701.3d) — this is not a
            // draw, and running out is not a loss.
            let mut exiled = Vec::new();
            for _ in 0..*count {
                let Some(card) = player.library.pop() else {
                    break;
                };
                exiled.push(card.id);
                player.exile.push(card);
            }
            if !exiled.is_empty() {
                state.exile_playing.push(crate::state::ExilePlaying {
                    player: controller,
                    cards: exiled,
                    turn,
                    cast_only: *cast_only,
                });
            }
        }
        // The same permission shape at the targeting gate instead of the casting one:
        // recorded per seat with the turn it was granted on, and idempotent for the same
        // reason — two identical permissions permit the same aims.
        Effect::IgnoreHexproof { player_ref } => {
            let turn = state.turn;
            for seat in non_targeting_subjects(state, *player_ref, controller, resolution.chosen_player) {
                state.ignoring_hexproof.push(crate::state::IgnoringHexproof {
                    player: seat,
                    turn,
                });
            }
        }
        // CR 614.1b: a replacement effect that exists for the rest of the turn and is
        // spent the first time the event it watches would happen. Minted from the same
        // monotonic counter every other object uses — which is both its identity and the
        // handle an ordering answer names — and then it simply waits.
        Effect::CreateReplacement { replacement } => {
            let id = state.mint_id();
            let turn = state.turn;
            state
                .replacements
                .push(crate::replacement::PendingReplacement {
                    id,
                    controller,
                    effect: replacement.clone(),
                    turn,
                });
        }
        // CR 615.1: a prevention shield that covers every damage event it names for the
        // rest of the turn. It is not spent by applying — the cleanup step's turn-based
        // action is what ends it (CR 514.2) — and it belongs to nobody: no target, no
        // player, and no controller anything reads back.
        Effect::PreventDamage { damage } => state.prevention.push(damage.clone()),
        // CR 603.7a: a delayed triggered ability is created during a resolution and waits
        // for its event. Recorded exactly as a created replacement is, on a per-turn list
        // carrying the turn it was made on, and controlled by whoever controlled the
        // spell or ability as it resolved (CR 603.7d/e) — which is `controller`.
        Effect::CreateDelayedTrigger { trigger } => {
            let id = state.mint_id();
            let turn = state.turn;
            state
                .delayed_triggers
                .push(crate::delayed::PendingDelayedTrigger {
                    id,
                    controller,
                    trigger: trigger.clone(),
                    turn,
                });
        }
        // CR 603.11: an ability created by this resolution about something this
        // resolution just did. Whether it fires is decided here and now — the resolution
        // is the only thing that knows what it did — and the ability then goes on the
        // stack through the ordinary trigger seam, unaimed, for its controller to aim.
        //
        // Its source is the permanent the sentence is about ("**it** deals damage equal to
        // **its** power"), and that permanent's power rides along as last known
        // information: killing the creature in response is the obvious answer to this
        // trigger, and CR 608.2h says it does not stop the damage.
        Effect::CreateReflexiveTrigger { trigger } => {
            let crate::reflexive::ReflexiveCondition::CreaturePutOntoBattlefieldThisWay =
                trigger.event;
            let Some(entered) = resolution.entered else {
                return;
            };
            let is_creature = state
                .battlefield
                .iter()
                .find(|perm| perm.id == entered)
                .and_then(|perm| perm.printed.face(db))
                .is_some_and(|face| face.has_type(crate::CardType::Creature));
            if !is_creature {
                return;
            }
            let source_power = crate::characteristics::characteristics(state, entered, db).power;
            state
                .reflexive_triggers
                .push(crate::reflexive::PendingReflexive {
                    controller,
                    source: entered,
                    source_power,
                    effects: trigger.effects.clone(),
                });
        }
        // Aimed at a chosen permanent, so it is applied through [`apply_targeted_effect`]
        // and this arm is never the one that runs it.
        Effect::SelfDealsDamage { .. } | Effect::Animate { .. } => {}
        // Both are aimed at chosen permanents, so they are applied through the targeted
        // path; an exchange goes through the multi-slot one beside it.
        Effect::ExchangeControl { .. } | Effect::ExileUntilSourceLeaves { .. } => {}
        // CR 613 layers 4, 5 and 7b applied to the source itself — the self-referential
        // animation. Until end of turn always: every printed card that says "becomes" of
        // itself says it for the turn, and a permanent-lifetime version would be a
        // different sentence.
        Effect::AnimateSelf {
            types,
            subtypes,
            replace_subtypes,
            colors,
            power,
            toughness,
        } => {
            let Some(id) = permanent_source else {
                return;
            };
            if !types.is_empty()
                || !subtypes.is_empty()
                || !colors.is_empty()
                || *replace_subtypes
            {
                let timestamp = state.mint_id();
                state.static_effects.push(crate::state::StaticEffect {
                    source: timestamp,
                    affects: crate::state::EffectAffects::SpecificPermanent(id),
                    modification: crate::state::Modification::AddTypes {
                        types: types.clone(),
                        subtypes: subtypes.clone(),
                        colors: colors.clone(),
                        replace_subtypes: *replace_subtypes,
                    },
                    duration: crate::state::Duration::UntilEndOfTurn,
                });
            }
            if let (Some(power), Some(toughness)) = (power, toughness) {
                let timestamp = state.mint_id();
                state.static_effects.push(crate::state::StaticEffect {
                    source: timestamp,
                    affects: crate::state::EffectAffects::SpecificPermanent(id),
                    modification: crate::state::Modification::SetBasePowerToughness {
                        power: *power,
                        toughness: *toughness,
                    },
                    duration: crate::state::Duration::UntilEndOfTurn,
                });
            }
        }
        // CR 701.17: the source itself, through the one battlefield→graveyard seam a
        // death takes — so a sacrifice fires the dies triggers and logs the death exactly
        // as any other does. A source that has already left sacrifices nothing.
        Effect::SacrificeSelf => {
            if let Some(id) = permanent_source {
                state.move_permanent_to_graveyard(id, db);
            }
        }
        // CR 303.4: the permanent this Aura is on, which it chose when it was cast. A
        // source that is attached to nothing — or that has left — taps nothing.
        Effect::TapAttached => {
            let host = permanent_source
                .and_then(|id| state.battlefield.iter().find(|perm| perm.id == id))
                .and_then(|perm| perm.attached_to);
            if let Some(host) = host {
                if let Some(perm) = state.battlefield.iter_mut().find(|perm| perm.id == host) {
                    perm.tapped = true;
                }
            }
        }
        // The same host, destroyed rather than tapped — and through the one destruction
        // seam, so it is a real death: an indestructible host stays, and a creature that
        // goes fires its dies triggers like any other (CR 701.7, CR 700.4).
        Effect::DestroyAttached => {
            let host = permanent_source
                .and_then(|id| state.battlefield.iter().find(|perm| perm.id == id))
                .and_then(|perm| perm.attached_to);
            if let Some(host) = host {
                state.destroy_permanent(host, db);
            }
        }
        Effect::DrawCard { count } => draw_cards(state, controller, u32::from(*count)),
        // The same draw, with the number taken off the game instead of off the card
        // (CR 608.2) — once, here, so a mill this same resolution performed is what the
        // "this way" sources read.
        Effect::DrawCardsByAmount { amount } => {
            let count = crate::condition::derived_amount(
                state,
                amount,
                controller,
                controller,
                resolution,
                db,
            );
            draw_cards(state, controller, count);
        }
        // CR 119.3: the referenced player gains life. A non-targeting reference names
        // its seats outright ([`non_targeting_subjects`]); a targeting one is routed through
        // [`apply_targeted_effect`] instead and is a no-op here.
        Effect::GainLife { player_ref, amount } => {
            let delta = i32::try_from(*amount).unwrap_or(i32::MAX);
            for seat in non_targeting_subjects(state, *player_ref, controller, resolution.chosen_player) {
                state.change_life(seat, delta);
            }
        }
        // CR 119.3: the referenced player loses life; a drop to 0 or less feeds
        // the zero-life state-based action (CR 704.5a) in the SBA loop.
        Effect::LoseLife { player_ref, amount } => {
            let delta = i32::try_from(*amount).unwrap_or(i32::MAX);
            for seat in non_targeting_subjects(state, *player_ref, controller, resolution.chosen_player) {
                state.change_life(seat, -delta);
            }
        }
        // The same loss with X off the game rather than off the card (CR 608.2), read
        // once **per named seat**: `each player loses half their life` is each of them
        // reading their own total, which is why the amount is asked about `seat` and
        // not about the controller.
        Effect::LoseLifeByAmount { player_ref, amount } => {
            for seat in non_targeting_subjects(state, *player_ref, controller, resolution.chosen_player) {
                let lost = crate::condition::derived_amount(
                    state, amount, controller, seat, resolution, db,
                );
                state.change_life(seat, -i32::try_from(lost).unwrap_or(i32::MAX));
            }
        }
        // CR 720.1: the named player takes an extra turn after this one. Queued rather
        // than taken — the game finishes the turn it is in, and the rotation hands the
        // next one over — and last in first out, which is what the queue already does.
        Effect::TakeExtraTurn { player_ref } => {
            for seat in non_targeting_subjects(state, *player_ref, controller, resolution.chosen_player) {
                state.extra_turns.push(seat);
            }
        }
        // CR 104.2b: the referenced player wins the game, which the engine says by
        // making every other player in it lose (CR 104.2a — the survivor is the
        // winner). A subject who has already lost cannot win and is skipped, and a
        // seat that has already lost keeps the reason it lost for.
        Effect::WinTheGame { player_ref } => {
            for seat in non_targeting_subjects(state, *player_ref, controller, resolution.chosen_player) {
                win_the_game(state, seat);
            }
        }
        // CR 701.13: the referenced player puts the top `count` cards of their
        // library into their graveyard. Not a draw — an empty library simply moves
        // fewer cards and never trips the CR 704.5c decking loss.
        Effect::Mill { player_ref, count } => {
            for seat in non_targeting_subjects(state, *player_ref, controller, resolution.chosen_player) {
                state.mill(seat, u32::from(*count), db);
            }
        }
        // CR 502.4 / 611.2c: every creature the referenced player controls **right now**
        // is tapped, and optionally flagged to skip that player's next untap step. A
        // targeting reference names its seat instead and is applied through
        // [`apply_targeted_effect`], so it is a no-op here.
        Effect::TapAll {
            player_ref,
            skip_next_untap,
        } => {
            for seat in non_targeting_subjects(state, *player_ref, controller, resolution.chosen_player) {
                tap_creatures_of(state, seat, *skip_next_untap, db);
            }
        }
        // CR 111.1: the referenced player creates `count` tokens, each with the
        // characteristics the effect describes. Tokens enter one at a time through the
        // one effect→battlefield seam, so each is a separate object with its own
        // `PermanentId` — two tokens created together are two permanents, and the diff
        // collector sees two entries. A targeting reference names its creator instead
        // and is applied through [`apply_targeted_effect`], so it is a no-op here.
        Effect::CreateToken {
            token,
            count,
            count_of,
            player_ref,
            tapped,
            attacking,
        } => {
            // X, if the effect has one, is taken once here — before the first token
            // arrives, so a token this effect creates never counts towards its own
            // number.
            let made = tokens_created(state, *count, count_of.as_ref(), controller, db);
            for seat in non_targeting_subjects(state, *player_ref, controller, resolution.chosen_player) {
                // Every token this seat creates joins the same attack, answered once:
                // the tokens are created simultaneously and there is one declaration
                // for them to join.
                let joins =
                    attack_a_created_token_joins(state, *attacking, permanent_source, seat);
                for _ in 0..made {
                    state.create_token(token.clone(), seat, *tapped, joins, db);
                }
            }
        }
        // CR 120.3 damage dealt to a **class** rather than to a target: no slot was
        // filled, so there is nothing to re-check and nothing to fizzle. The class is
        // enumerated **here, on resolution** (CR 611.2c, the rule `apply_mass_modification`
        // already follows) — a creature that arrived after the spell was cast is
        // included and one that has died is not. What is dealt is damage either way:
        // marked on each creature for the lethal-damage SBA (CR 704.5g), lost life for
        // each player (CR 120.3a). A targeting subject named a target instead and is
        // applied through [`apply_targeted_effect`], so it is a no-op here.
        Effect::DealDamage { subject, amount } => {
            apply_class_damage(
                state,
                subject,
                *amount,
                controller,
                resolution,
                permanent_source,
                db,
            );
        }
        // The same damage with the amount taken off the announcement instead of off the
        // card (CR 608.2). A class-subject form chose no target and is applied here; a
        // targeted one goes through [`apply_targeted_effect`] and is a no-op in this arm.
        Effect::DealDamageByAmount { subject, amount } => {
            let value = crate::condition::derived_amount(
                state, amount, controller, controller, resolution, db,
            );
            apply_class_damage(
                state,
                subject,
                value,
                controller,
                resolution,
                permanent_source,
                db,
            );
        }
        // CR 701.7, over a class instead of a target. The set is enumerated **here**, on
        // resolution (CR 611.2c), and each member leaves through the one destruction seam
        // a single `Destroy` uses — so a token ceases to exist (CR 111.7) and a death
        // trigger sees every one of them. Collected before any of it happens, because
        // destroying the first member moves the battlefield out from under the scan.
        Effect::DestroyAll { affects } => {
            for id in permanents_to_destroy(state, *affects, db) {
                state.destroy_permanent(id, db);
            }
        }
        // A mass, non-targeting until-end-of-turn modification (CR 611.2c): the
        // affected class is enumerated **once, here**, and one modifier is keyed to
        // each permanent found. Freezing the set is the point — an anthem is
        // re-evaluated every read, a one-shot pump is not, so a creature that arrives
        // later this turn must not pick the bonus up.
        Effect::PumpAll {
            affects,
            power,
            toughness,
        } => {
            apply_mass_modification(
                state,
                affects,
                controller,
                resolution.paid.source_power,
                resolution.chosen_player,
                Modification::PowerToughness {
                    power: *power,
                    toughness: *toughness,
                },
                db,
            );
        }
        Effect::GrantKeywordAll { affects, keyword } => {
            apply_mass_modification(
                state,
                affects,
                controller,
                resolution.paid.source_power,
                resolution.chosen_player,
                Modification::GrantKeyword(*keyword),
                db,
            );
        }
        Effect::RestrictAll {
            affects,
            restriction,
        } => {
            apply_mass_modification(
                state,
                affects,
                controller,
                resolution.paid.source_power,
                resolution.chosen_player,
                Modification::GrantRestriction(restriction.clone()),
                db,
            );
        }
        // Self-referential effects: the subject is the ability's own source, which is
        // not a target (CR 115.1) and so was never chosen. A source that has left the
        // battlefield is not there to modify, and the effect simply does nothing —
        // the same no-op a fizzled target produces, without the fizzle.
        Effect::PumpSelf { power, toughness } => {
            if let Some(id) = permanent_source {
                if state.battlefield.iter().any(|p| p.id == id) {
                    let stamp = state.mint_id();
                    state.static_effects.push(StaticEffect {
                        source: stamp,
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
        // The same implicit subject, at CR 613 layer 6 instead of 7c: the source
        // restricts *itself* until end of turn ("this creature can't be blocked this
        // turn"), with nothing targeted and nothing to fizzle.
        Effect::RestrictSelf { restriction } => {
            if let Some(id) = permanent_source {
                if state.battlefield.iter().any(|p| p.id == id) {
                    let stamp = state.mint_id();
                    state.static_effects.push(StaticEffect {
                        source: stamp,
                        affects: EffectAffects::SpecificPermanent(id),
                        modification: Modification::GrantRestriction(restriction.clone()),
                        duration: Duration::UntilEndOfTurn,
                    });
                }
            }
        }
        // Layer 6 again, and the half of it that subtracts. One minted timestamp for the
        // whole clause, because it is one continuous effect (CR 613.7): the pieces are
        // pushed subtraction-first and the layer-6 sort is stable, so within the clause
        // the losses settle before the gains while *between* clauses the timestamp still
        // decides. A clause that neither loses nor gains anything pushes nothing.
        Effect::AlterAbilitiesSelf {
            lose_all,
            lose,
            gain,
        } => {
            // A layer-6 clause is about a permanent, so it asks the source for the one
            // it is: a spell, an emblem, or a card activated from a graveyard answers
            // `None` and the clause applies to nothing.
            if let Some(id) = permanent_source {
                if state.battlefield.iter().any(|p| p.id == id) {
                    let stamp = state.mint_id();
                    let mut push = |modification| {
                        state.static_effects.push(StaticEffect {
                            source: stamp,
                            affects: EffectAffects::SpecificPermanent(id),
                            modification,
                            duration: Duration::UntilEndOfTurn,
                        });
                    };
                    if *lose_all {
                        push(Modification::LoseAllAbilities);
                    }
                    for keyword in lose {
                        push(Modification::LoseKeyword(*keyword));
                    }
                    for keyword in gain {
                        push(Modification::GrantKeyword(*keyword));
                    }
                }
            }
        }
        // The source puts *itself* back in the deck (CR 701.19). The same implicit
        // subject every self-referential effect has, and the same no-op when the source
        // is gone — a creature already destroyed in response is not there to shuffle, and
        // nothing is shuffled in its place.
        Effect::ShuffleSelfIntoLibrary => {
            if let Some(id) = permanent_source {
                state.shuffle_permanent_into_library(id);
            }
        }
        Effect::PutCountersOnSelf { counter, count, .. } => {
            if let Some(id) = permanent_source {
                if let Some(perm) = state.battlefield.iter_mut().find(|p| p.id == id) {
                    *perm.counters.entry(*counter).or_insert(0) += *count;
                }
            }
        }
        // The self-referential effect whose source is a **card in a graveyard** rather
        // than a permanent (CR 113.6): the source moves itself out. Every other
        // self-referential effect asks `permanent()`; this one asks `graveyard_card()`,
        // and both are `None` for the source that is not that kind of object.
        //
        // The card is looked up **now**, on resolution, not when the ability was
        // activated: an opponent who exiled the card in response leaves nothing to
        // return, and the ability resolves and does nothing (CR 608.2 — it never had a
        // target to fizzle on). The moves themselves go through the same seams a targeted
        // return uses, so a card that comes back to the battlefield mints a fresh
        // `PermanentId` and fires its entry replacements and triggers exactly as any
        // other arrival does.
        Effect::ReturnSelfFromGraveyard { destination } => {
            let Some(card) = source.and_then(crate::stack::AbilitySource::graveyard_card) else {
                return;
            };
            let Some((owner, card)) =
                super::targeted::take_from_a_graveyard_with_owner(state, card.id)
            else {
                return;
            };
            match destination {
                crate::ability::FoundDestination::Hand => {
                    if let Some(player) = state.players.get_mut(owner.0) {
                        player.hand.push(card);
                    }
                }
                crate::ability::FoundDestination::Battlefield => {
                    state.put_card_onto_battlefield(card, controller, false, None, db);
                }
                crate::ability::FoundDestination::BattlefieldTapped => {
                    state.put_card_onto_battlefield(card, controller, true, None, db);
                }
            }
        }
        // An effect that poses a mid-resolution player choice never reaches either
        // apply function: the resolve loop intercepts it, queues the choice, and
        // suspends ([`crate::choice::choices_for_effect`]). Reaching here would mean
        // the interception was missed, so both arms are deliberately empty rather
        // than silently doing half the effect.
        Effect::Discard { .. }
        // Both derived-number verbs that ask a player something suspend the same way,
        // and for the same reason: the number is fixed when the choice is posed, not
        // when the answer comes back.
        | Effect::DiscardByAmount { .. }
        | Effect::Sacrifice { .. }
        | Effect::Scry { .. }
        | Effect::PutHandOntoBattlefieldFaceDown { .. }
        | Effect::LookAtTop { .. }
        | Effect::RevealTopAndMayPlay { .. }
        | Effect::MayCastExiledThisWay { .. }
        | Effect::SearchLibrary { .. }
        | Effect::May { .. }
        // Both are questions, posed by the resolution path before either apply path is
        // reached.
        | Effect::MayPayForTrigger { .. }
        // A conditional is likewise intercepted by the resolve loop, which evaluates it
        // and splices the chosen branch into what remains; reaching here would mean the
        // branch was never taken.
        | Effect::Conditional { .. } => {}
        // A targeting effect: its subject is a chosen target, not the controller,
        // so it is applied via [`apply_targeted_effect`] and is a no-op here.
        Effect::Tap { .. }
        | Effect::CounterSpell { .. }
        // Copying a spell names the spell it copies in a slot, so it arrives with a
        // chosen value and is applied there.
        | Effect::CopySpell { .. }
        | Effect::Destroy { .. }
        | Effect::Exile { .. }
        | Effect::ReturnToHand { .. }
        | Effect::PutCounters { .. }
        | Effect::Pump { .. }
        | Effect::PumpByCount { .. }
        | Effect::PumpByAmount { .. }
        | Effect::GrantKeyword { .. }
        | Effect::ReturnCardToBattlefield { .. }
        | Effect::ReturnCardToHand { .. }
        | Effect::PutOnTopOfLibrary { .. }
        | Effect::GainControl { .. }
        // An equip names a host to attach to, so it too arrives with a chosen target.
        | Effect::Attach { .. }
        // A fight arrives with *two* chosen targets, so it is applied via
        // [`apply_multi_target_effect`] and is doubly a no-op here.
        | Effect::Fight { .. }
        | Effect::Restrict { .. } => {}
        // X is taken **once, on resolution** (CR 608.2), from the board as it stands
        // then — a creature that dies afterwards does not take the life back. The count
        // is relative to the effect's controller even when the life goes elsewhere,
        // because "each creature you control" says "you".
        Effect::GainLifeByCount {
            player_ref,
            amount_per,
            count_of,
        } => {
            let count = crate::condition::count_permanents(state, count_of, controller, db);
            let delta = i32::try_from(amount_per.saturating_mul(count)).unwrap_or(i32::MAX);
            for seat in non_targeting_subjects(state, *player_ref, controller, resolution.chosen_player) {
                state.change_life(seat, delta);
            }
        }
        // The same count, feeding damage instead. A targeting subject never reaches
        // here — it is applied by [`apply_targeted_effect`] — so this arm handles only
        // the class forms.
        Effect::DealDamageByCount {
            subject,
            amount_per,
            count_of,
        } => {
            let count = crate::condition::count_permanents(state, count_of, controller, db);
            let amount = amount_per.saturating_mul(count);
            apply_class_damage(
                state,
                subject,
                amount,
                controller,
                resolution,
                permanent_source,
                db,
            );
        }
        // The non-targeting form of the dig. Every printed card of this shape targets, so
        // this is reached only by a definition that named a non-targeting reference; it
        // does the same thing to each named seat, through the same helper, so the two
        // forms cannot drift.
        Effect::ExileFromLibraryUntil { player_ref, class } => {
            for seat in non_targeting_subjects(state, *player_ref, controller, resolution.chosen_player) {
                super::targeted::dig_until(state, seat, *class, db);
            }
        }
        // Every card of the named graveyard, at once. An empty graveyard is a legal
        // subject and a resolution that does nothing.
        Effect::ExileGraveyard { player_ref } => {
            for seat in non_targeting_subjects(state, *player_ref, controller, resolution.chosen_player) {
                if let Some(player) = state.players.get_mut(seat.0) {
                    let cards: Vec<_> = player.graveyard.drain(..).collect();
                    player.exile.extend(cards);
                }
            }
        }
        // The library counterpart, and the one place a hidden zone is emptied wholesale.
        // The bottom card is the first element (the top is the last, as everywhere else),
        // so what is exiled is everything after it — in library order, so the exile pile
        // reads bottom-upward exactly as the library did.
        Effect::ExileLibraryExceptBottom { target } => {
            for seat in non_targeting_subjects(state, *target, controller, resolution.chosen_player) {
                if let Some(player) = state.players.get_mut(seat.0) {
                    if player.library.len() > 1 {
                        let cards: Vec<_> = player.library.drain(1..).collect();
                        player.exile.extend(cards);
                    }
                }
            }
        }
        // CR 701.28a: the permanent turns over. Nothing else about it changes, which is
        // CR 712.a in one line — the object keeps its id, its counters, its damage, its
        // attachments, and its combat state, because none of them is where the face is.
        Effect::TransformSelf => {
            if let Some(id) = permanent_source {
                if let Some(perm) = state.battlefield.iter_mut().find(|p| p.id == id) {
                    perm.printed.transform(db);
                }
            }
        }
        // Exile the source and bring it back on its other face. Two zone changes, so the
        // permanent that arrives is a new object (CR 400.7) with a fresh id and its back
        // face's starting loyalty — the exile is what makes it *return* rather than turn
        // over, and both halves use the seams every other exile and arrival use.
        //
        // The source is looked up now, on resolution: a permanent that has already left
        // leaves nothing to exile, and the ability resolves and does nothing (CR 608.2).
        Effect::ExileSelfAndReturnTransformed => {
            if let Some(id) = permanent_source {
                state.exile_and_return_transformed(id, db);
            }
        }
    }
}

/// Draw `count` cards for `controller`, one at a time (CR 121.1).
///
/// Routes each draw through [`Player::draw`](crate::Player::draw), so a card-draw effect
/// that empties the library also flags the decking loss (CR 704.5c). Only the cards that
/// actually moved are logged — an empty-library draw adds none — and a count of zero
/// records nothing at all, which is what a derived amount of zero should look like in a
/// log: an effect that drew no cards, not a draw of no cards.
///
/// One function so the printed and the derived spellings of a draw cannot differ about
/// what drawing is.
fn draw_cards(state: &mut GameState, controller: PlayerId, count: u32) {
    let mut drawn = 0u32;
    for _ in 0..count {
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

/// `winner` wins the game (CR 104.2b): every other player still in it loses, recorded as
/// [`LossReason::OpponentWon`](crate::player::LossReason::OpponentWon).
///
/// The engine stores no winner — [`GameState::result`](crate::GameState::result) derives
/// one from who has lost, because the survivor of CR 104.2a *is* the winner — so this is
/// what winning is written as, and it is also what the rules say happens at a table of
/// any size. A seat that has already lost keeps the reason it lost for, and a `winner`
/// who has already lost wins nothing (CR 104.3a: they are no longer in the game).
///
/// One function so the targeting and non-targeting spellings of [`Effect::WinTheGame`]
/// end the game identically; the only difference between them is who the seat is.
pub(super) fn win_the_game(state: &mut GameState, winner: PlayerId) {
    if state.players.get(winner.0).is_none_or(|p| p.has_lost) {
        return;
    }
    for (seat, player) in state.players.iter_mut().enumerate() {
        if seat != winner.0 && !player.has_lost {
            player.has_lost = true;
            player.loss_reason = Some(crate::player::LossReason::OpponentWon);
        }
    }
}

/// Add `modification` to every permanent in `affects` until end of turn, on behalf of
/// `controller` (CR 611.2c).
///
/// One [`StaticEffect`] is pushed **per affected permanent**, each keyed to that one
/// [`crate::PermanentId`], rather than one class-scoped effect: a class selector is
/// re-evaluated on every read, which is right for an anthem and wrong for a one-shot.
/// Keying to the permanents present now is what locks the set in, and it reuses the
/// exact pruning and cleanup the single-target pump already has, so nothing about
/// duration or timestamp ordering is special-cased for the mass case.
fn apply_mass_modification(
    state: &mut GameState,
    affects: &MassAffects,
    controller: PlayerId,
    source_power: Option<i32>,
    chosen_player: Option<PlayerId>,
    modification: Modification,
    db: &CardDatabase,
) {
    for id in permanents_in(state, affects, controller, source_power, chosen_player, db) {
        let source = state.mint_id();
        state.static_effects.push(StaticEffect {
            source,
            affects: EffectAffects::SpecificPermanent(id),
            modification: modification.clone(),
            duration: Duration::UntilEndOfTurn,
        });
    }
}

/// Tap every creature `seat` controls, flagging each to skip that seat's next untap step
/// when `skip_next_untap` (CR 502.4).
///
/// One function so the targeting and non-targeting spellings of [`Effect::TapAll`] tap
/// exactly the same set: the difference between them is who the seat is, and nothing
/// else. The set is enumerated **here, on resolution** (CR 611.2c) — a creature that
/// arrives later is untouched, and one already tapped is flagged anyway, which is what
/// stops a card from being a blank against a board that already attacked.
pub(super) fn tap_creatures_of(
    state: &mut GameState,
    seat: PlayerId,
    skip_next_untap: bool,
    db: &CardDatabase,
) {
    // CR 613 layer 2, taken before the mutable walk (the control answer is a read of the
    // whole state, which the loop below holds mutably): a stolen creature is tapped as
    // one of its *new* controller's creatures.
    let theirs: Vec<PermanentId> = state
        .battlefield
        .iter()
        .filter(|perm| crate::characteristics::controller_of(state, perm) == seat)
        .map(|perm| perm.id)
        .collect();
    for perm in &mut state.battlefield {
        if !theirs.contains(&perm.id) {
            continue;
        }
        if !perm
            .printed
            .face(db)
            .is_some_and(|face| face.has_type(crate::card_type::CardType::Creature))
        {
            continue;
        }
        perm.tapped = true;
        if skip_next_untap {
            perm.skips_untap = true;
        }
    }
}

/// What a token created **attacking** attacks (CR 506.3c), or `None` when it is not
/// created attacking at all.
///
/// A card that creates a token "that's attacking" never says *what* it attacks, because
/// there is only one sensible answer: the token joins the attack its own source is
/// already making, against that same player or planeswalker. So the question is asked of
/// the effect's source permanent, and every way that can fail to name an attack is the
/// same `None` — the effect is resolving outside combat, its source was removed from
/// combat before it resolved, or the source is not a permanent at all (a spell, an
/// emblem). Nothing here invents a defender: an attacking token with nothing to join is
/// simply an ordinary token.
///
/// `creator` guards the one case the source cannot answer for: a token created under
/// *another* seat's control is not part of that source's attack, and a creature never
/// attacks the player who controls it.
///
/// One function so the targeting and non-targeting spellings of
/// [`Effect::CreateToken`] cannot disagree about which attack is joined.
pub(super) fn attack_a_created_token_joins(
    state: &GameState,
    attacking: bool,
    source: Option<PermanentId>,
    creator: PlayerId,
) -> Option<crate::combat::AttackTarget> {
    if !attacking {
        return None;
    }
    let source = state.battlefield.iter().find(|p| Some(p.id) == source)?;
    if source.controller != creator {
        return None;
    }
    source.attacking
}

/// How many tokens one [`Effect::CreateToken`] creates for one creator: `count`, or
/// `count` **per permanent** matching `count_of` when the effect derives its number from
/// a count (CR 608.2).
///
/// Taken here, at the moment the effect is applied, which is what makes X the board as it
/// stands on resolution rather than at announcement. The multiplication saturates rather
/// than wrapping; no realistic game reaches a count that needs it.
///
/// The count is relative to the effect's *controller* even when a targeting `player_ref`
/// hands the tokens to someone else, for the reason [`Effect::GainLifeByCount`]'s is:
/// "each creature you control" says "you", and naming a different creator does not change
/// who that is. One function so the targeting and non-targeting spellings cannot disagree
/// about how many tokens the effect makes.
pub(super) fn tokens_created(
    state: &GameState,
    count: u8,
    count_of: Option<&crate::ability::PermanentCount>,
    controller: PlayerId,
    db: &CardDatabase,
) -> u32 {
    let count = u32::from(count);
    match count_of {
        None => count,
        Some(wanted) => count.saturating_mul(crate::condition::count_permanents(
            state, wanted, controller, db,
        )),
    }
}

/// The permanents a [`MassAffects`] class names, in battlefield order, for an object
/// controlled by `controller`.
///
/// The permanent-side counterpart of [`non_targeting_subjects`], and the one place a class is
/// turned into a concrete set: a mass modification and a mass damage effect must agree
/// on what "each creature" means, and the set is enumerated **at the moment of
/// resolution** (CR 611.2c) so a permanent that arrived after announcement is included
/// and one that has left is not. Every class is read relative to `controller`, which is
/// what lets one authored card mean "you" from either seat.
fn permanents_in(
    state: &GameState,
    affects: &MassAffects,
    controller: PlayerId,
    source_power: Option<i32>,
    chosen_player: Option<PlayerId>,
    db: &CardDatabase,
) -> Vec<PermanentId> {
    // CR 613 layer 4 has already run by the time a resolution asks: an artifact animated
    // into a creature is in every class of creatures, which is what makes it die to a
    // sweeper. Safe to read the computed types here, outside the layer system.
    let is_creature = |perm: &Permanent| {
        crate::characteristics::characteristics(state, perm.id, db)
            .types
            .contains(&crate::card_type::CardType::Creature)
    };
    // Every class but one is a class of creatures, so the type test is applied once here
    // rather than restated in each arm. The exception names planeswalkers outright and
    // says so by answering the question itself.
    let type_ok = |perm: &Permanent| match affects {
        MassAffects::CreaturesAndPlaneswalkersYourOpponentsControl => {
            is_creature(perm)
                || perm
                    .printed
                    .face(db)
                    .is_some_and(|face| face.has_type(crate::card_type::CardType::Planeswalker))
        }
        _ => is_creature(perm),
    };
    state
        .battlefield
        .iter()
        .filter(|p| {
            type_ok(p)
                && match affects {
                    // A subtype narrows the class to a lord's tribe ("Dragons you
                    // control"), read off the printed face — the same place every other
                    // subtype question is answered.
                    MassAffects::CreaturesYouControl {
                        subtype,
                        min_power,
                        below_source_power,
                    } => {
                        crate::characteristics::controller_of(state, p) == controller
                            && subtype.as_deref().is_none_or(|wanted| {
                                p.printed
                                    .face(db)
                                    .is_some_and(|face| face.has_subtype(wanted))
                            })
                            // A power bound is the one field here read through the
                            // **computed** characteristics (CR 613.1f): "each creature
                            // you control with power 4 or greater" means the power the
                            // creature has now, so one pumped up to 4 is in the class
                            // and one shrunk below it is out. Safe from inside a
                            // resolution, which is outside the layer system.
                            && min_power.is_none_or(|min| {
                                crate::characteristics::characteristics(state, p.id, db)
                                    .power
                                    .is_some_and(|power| power >= min)
                            })
                            // And the same reading against the *source's* power rather
                            // than a printed number. A source that is gone has no power to
                            // compare against, and the honest answer to "less than its
                            // power" is then nobody — never everybody.
                            && (!below_source_power
                                || source_power.is_some_and(|source| {
                                    crate::characteristics::characteristics(state, p.id, db)
                                        .power
                                        .is_some_and(|power| power < source)
                                }))
                    }
                    MassAffects::EachCreature => true,
                    // Exactly the set declare-attackers produced (CR 508.1a); empty
                    // outside combat, which is what a combat pump cast in a main phase
                    // means.
                    MassAffects::AttackingCreatures => p.attacking.is_some(),
                    // A seat that has lost is no longer an opponent (CR 102.1); its
                    // permanents are on their way off the battlefield in the same SBA
                    // loop, and this is the same exclusion `non_targeting_subjects` makes.
                    // The class counterpart of `PlayerRef::ThatPlayer`, reading the same
                    // fact: whose creatures were named by the sentence before this one.
                    MassAffects::CreaturesThatPlayerControls => chosen_player.is_some_and(|seat| {
                        crate::characteristics::controller_of(state, p) == seat
                    }),
                    MassAffects::CreaturesYourOpponentsControl
                    | MassAffects::CreaturesAndPlaneswalkersYourOpponentsControl => {
                        let seat = crate::characteristics::controller_of(state, p);
                        seat != controller
                            && state
                                .players
                                .get(seat.0)
                                .is_some_and(|player| !player.has_lost)
                    }
                    // Flying is read through the computed keywords (CR 613.1f), so a
                    // creature that was *granted* flying is outside the class exactly
                    // as a printed flyer is.
                    MassAffects::CreaturesWithoutFlying => {
                        !crate::characteristics::permanent_has_keyword(
                            state,
                            p.id,
                            crate::card::Keyword::Flying,
                            db,
                        )
                    }
                }
        })
        .map(|p| p.id)
        .collect()
}

/// The seats a **non-targeting** [`PlayerRef`] names, in seat order, for an object
/// controlled by `controller` (CR 115.1 — no target is chosen, so this list is
/// derived fresh at resolution and never fizzles).
///
/// Empty for a targeting reference: those carry a chosen [`Target`] instead and are
/// applied through [`apply_targeted_effect`], so returning nothing here is what keeps
/// a targeted drain from *also* silently hitting everyone.
pub(crate) fn non_targeting_subjects(
    state: &GameState,
    player_ref: PlayerRef,
    controller: PlayerId,
    chosen: Option<PlayerId>,
) -> Vec<PlayerId> {
    match player_ref {
        // The player a sentence before this one named (CR 608.2h). Nobody, in a resolution
        // that has aimed at nothing — a phrase that only exists after a choice.
        PlayerRef::ThatPlayer => chosen.into_iter().collect(),
        PlayerRef::Controller => vec![controller],
        // Every opponent still in the game (CR 102.1) — in a game of three or more
        // this really is all of them, which is the whole reason it is not spelled
        // "the opponent".
        PlayerRef::EachOpponent => state
            .players
            .iter()
            .enumerate()
            .filter(|(seat, player)| PlayerId(*seat) != controller && !player.has_lost)
            .map(|(seat, _)| PlayerId(seat))
            .collect(),
        // Every seat still in the game, the controller included — the symmetric class,
        // and the reason it is not `EachOpponent` plus the caster is that a spell which
        // names it hits the caster whether they like it or not.
        PlayerRef::EachPlayer => state
            .players
            .iter()
            .enumerate()
            .filter(|(_, player)| !player.has_lost)
            .map(|(seat, _)| PlayerId(seat))
            .collect(),
        PlayerRef::TargetPlayer | PlayerRef::TargetOpponent => Vec::new(),
    }
}

/// Deal `amount` damage to whatever `subject` names **as a class**, on behalf of
/// `controller` (CR 120.3).
///
/// The shared body of the fixed and the count-derived damage verbs: by the time either
/// reaches here the amount is a number, so nothing below knows which one it came from.
/// A [`DamageSubject::Target`] chose a target and is applied by
/// [`apply_targeted_effect`] instead, so it is a no-op here.
fn apply_class_damage(
    state: &mut GameState,
    subject: &DamageSubject,
    amount: u32,
    controller: PlayerId,
    resolution: crate::resolve::Resolution,
    source: Option<PermanentId>,
    db: &CardDatabase,
) {
    let mut dealt = false;
    match subject {
        DamageSubject::Target(_) => {}
        DamageSubject::Players(player_ref) => {
            for seat in
                non_targeting_subjects(state, *player_ref, controller, resolution.chosen_player)
            {
                // Prevented damage was never dealt (CR 615.1), so a shield is also what
                // keeps `hasn't dealt damage yet` true — the flag follows the amount that
                // actually landed rather than the amount that was aimed.
                dealt |= state.deal_damage(
                    resolution.damage(PendingDamage::to_player(seat, amount).from(source)),
                    db,
                ) > 0;
            }
        }
        DamageSubject::Permanents(affects) => {
            for id in permanents_in(
                state,
                affects,
                controller,
                resolution.paid.source_power,
                resolution.chosen_player,
                db,
            ) {
                dealt |= state.deal_damage(
                    resolution.damage(PendingDamage::to_permanent(id, amount).from(source)),
                    db,
                ) > 0;
            }
        }
    }
    // CR 609.7: an ability's damage comes from the permanent the ability is on, so a
    // class-wide hit is still that permanent dealing damage.
    if dealt {
        if let Some(source) = source {
            state.note_damage_dealt_by(source);
        }
    }
}

/// The permanents a [`DestroyAffects`] class names, in battlefield order.
///
/// The mass-destruction counterpart of [`permanents_in`], and separate from it for the
/// reason [`DestroyAffects`] is separate from [`MassAffects`]: nothing here is
/// controller-relative and nothing here is limited to creatures, so sharing the scan
/// would mean a filter with two halves that never both apply.
fn permanents_to_destroy(
    state: &GameState,
    affects: crate::ability::DestroyAffects,
    db: &CardDatabase,
) -> Vec<PermanentId> {
    use crate::ability::DestroyAffects;
    use crate::card_type::CardType;
    state
        .battlefield
        .iter()
        .filter(|p| {
            p.printed.face(db).is_some_and(|face| match affects {
                DestroyAffects::EachCreature => face.has_type(CardType::Creature),
                DestroyAffects::EachArtifactOrEnchantment => {
                    face.has_type(CardType::Artifact) || face.has_type(CardType::Enchantment)
                }
            })
        })
        .map(|p| p.id)
        .collect()
}
