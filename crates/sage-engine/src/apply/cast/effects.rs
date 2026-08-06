//! Applying one [`Effect`] whose subject is named by class rather than chosen as a
//! target — mana, draws, life, and the battlefield-wide forms (CR 611.2c, CR 115.1).

use super::*;

#[cfg(test)]
mod tests;

/// Apply a single [`Effect`] to `state` on behalf of `controller`.
pub(crate) fn apply_effect(
    state: &mut GameState,
    effect: &Effect,
    controller: PlayerId,
    source: Option<PermanentId>,
    db: &CardDatabase,
) {
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
            for seat in non_targeting_subjects(state, *player_ref, controller) {
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
            for seat in non_targeting_subjects(state, *player_ref, controller) {
                state.graveyard_casting.push(crate::state::GraveyardCasting {
                    player: seat,
                    filter: filter.clone(),
                    turn,
                });
            }
        }
        // The same permission shape at the targeting gate instead of the casting one:
        // recorded per seat with the turn it was granted on, and idempotent for the same
        // reason — two identical permissions permit the same aims.
        Effect::IgnoreHexproof { player_ref } => {
            let turn = state.turn;
            for seat in non_targeting_subjects(state, *player_ref, controller) {
                state.ignoring_hexproof.push(crate::state::IgnoringHexproof {
                    player: seat,
                    turn,
                });
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
        // CR 119.3: the referenced player gains life. A non-targeting reference names
        // its seats outright ([`non_targeting_subjects`]); a targeting one is routed through
        // [`apply_targeted_effect`] instead and is a no-op here.
        Effect::GainLife { player_ref, amount } => {
            let delta = i32::try_from(*amount).unwrap_or(i32::MAX);
            for seat in non_targeting_subjects(state, *player_ref, controller) {
                state.change_life(seat, delta);
            }
        }
        // CR 119.3: the referenced player loses life; a drop to 0 or less feeds
        // the zero-life state-based action (CR 704.5a) in the SBA loop.
        Effect::LoseLife { player_ref, amount } => {
            let delta = i32::try_from(*amount).unwrap_or(i32::MAX);
            for seat in non_targeting_subjects(state, *player_ref, controller) {
                state.change_life(seat, -delta);
            }
        }
        // CR 701.13: the referenced player puts the top `count` cards of their
        // library into their graveyard. Not a draw — an empty library simply moves
        // fewer cards and never trips the CR 704.5c decking loss.
        Effect::Mill { player_ref, count } => {
            for seat in non_targeting_subjects(state, *player_ref, controller) {
                state.mill(seat, u32::from(*count));
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
            for seat in non_targeting_subjects(state, *player_ref, controller) {
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
            for seat in non_targeting_subjects(state, *player_ref, controller) {
                // Every token this seat creates joins the same attack, answered once:
                // the tokens are created simultaneously and there is one declaration
                // for them to join.
                let joins = attack_a_created_token_joins(state, *attacking, source, seat);
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
            apply_class_damage(state, subject, *amount, controller, db);
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
                Modification::GrantRestriction(restriction.clone()),
                db,
            );
        }
        // Self-referential effects: the subject is the ability's own source, which is
        // not a target (CR 115.1) and so was never chosen. A source that has left the
        // battlefield is not there to modify, and the effect simply does nothing —
        // the same no-op a fizzled target produces, without the fizzle.
        Effect::PumpSelf { power, toughness } => {
            if let Some(id) = source {
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
            if let Some(id) = source {
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
            if let Some(id) = source {
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
        Effect::PutCountersOnSelf { counter, count } => {
            if let Some(id) = source {
                if let Some(perm) = state.battlefield.iter_mut().find(|p| p.id == id) {
                    *perm.counters.entry(*counter).or_insert(0) += *count;
                }
            }
        }
        // An effect that poses a mid-resolution player choice never reaches either
        // apply function: the resolve loop intercepts it, queues the choice, and
        // suspends ([`crate::choice::choices_for_effect`]). Reaching here would mean
        // the interception was missed, so both arms are deliberately empty rather
        // than silently doing half the effect.
        Effect::Discard { .. }
        | Effect::Scry { .. }
        | Effect::LookAtTop { .. }
        | Effect::SearchLibrary { .. }
        | Effect::May { .. }
        // A conditional is likewise intercepted by the resolve loop, which evaluates it
        // and splices the chosen branch into what remains; reaching here would mean the
        // branch was never taken.
        | Effect::Conditional { .. } => {}
        // A targeting effect: its subject is a chosen target, not the controller,
        // so it is applied via [`apply_targeted_effect`] and is a no-op here.
        Effect::Tap { .. }
        | Effect::CounterSpell { .. }
        | Effect::Destroy { .. }
        | Effect::Exile { .. }
        | Effect::ReturnToHand { .. }
        | Effect::PutCounters { .. }
        | Effect::Pump { .. }
        | Effect::PumpByCount { .. }
        | Effect::GrantKeyword { .. }
        | Effect::ReturnCardToBattlefield { .. }
        | Effect::ReturnCardToHand { .. }
        | Effect::PutOnTopOfLibrary { .. }
        | Effect::GainControl { .. }
        // An equip names a host to attach to, so it too arrives with a chosen target.
        | Effect::Attach { .. }
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
            for seat in non_targeting_subjects(state, *player_ref, controller) {
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
            apply_class_damage(state, subject, amount, controller, db);
        }
        // Every card of the named graveyard, at once. An empty graveyard is a legal
        // subject and a resolution that does nothing.
        Effect::ExileGraveyard { player_ref } => {
            for seat in non_targeting_subjects(state, *player_ref, controller) {
                if let Some(player) = state.players.get_mut(seat.0) {
                    let cards: Vec<_> = player.graveyard.drain(..).collect();
                    player.exile.extend(cards);
                }
            }
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
    modification: Modification,
    db: &CardDatabase,
) {
    for id in permanents_in(state, affects, controller, db) {
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
    db: &CardDatabase,
) -> Vec<PermanentId> {
    let is_creature = |perm: &Permanent| {
        perm.printed
            .face(db)
            .is_some_and(|face| face.has_type(crate::card_type::CardType::Creature))
    };
    state
        .battlefield
        .iter()
        .filter(|p| {
            is_creature(p)
                && match affects {
                    // A subtype narrows the class to a lord's tribe ("Dragons you
                    // control"), read off the printed face — the same place every other
                    // subtype question is answered.
                    MassAffects::CreaturesYouControl { subtype } => {
                        crate::characteristics::controller_of(state, p) == controller
                            && subtype.as_deref().is_none_or(|wanted| {
                                p.printed
                                    .face(db)
                                    .is_some_and(|face| face.has_subtype(wanted))
                            })
                    }
                    MassAffects::EachCreature => true,
                    // Exactly the set declare-attackers produced (CR 508.1a); empty
                    // outside combat, which is what a combat pump cast in a main phase
                    // means.
                    MassAffects::AttackingCreatures => p.attacking.is_some(),
                    // A seat that has lost is no longer an opponent (CR 102.1); its
                    // permanents are on their way off the battlefield in the same SBA
                    // loop, and this is the same exclusion `non_targeting_subjects` makes.
                    MassAffects::CreaturesYourOpponentsControl => {
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
) -> Vec<PlayerId> {
    match player_ref {
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
    db: &CardDatabase,
) {
    match subject {
        DamageSubject::Target(_) => {}
        DamageSubject::Players(player_ref) => {
            for seat in non_targeting_subjects(state, *player_ref, controller) {
                state.deal_damage_to_player(seat, amount);
            }
        }
        DamageSubject::Permanents(affects) => {
            for id in permanents_in(state, affects, controller, db) {
                state.deal_damage_to_permanent(id, amount, db);
            }
        }
    }
}
