//! Stack resolution: turning the top object of the stack into its effect.
//!
//! When all players pass priority in succession, [`crate::apply_action`] pops
//! the top of the stack and hands it to [`resolve_stack_object`], which first
//! re-checks the object's chosen targets against current state (CR 608.2b), then
//! routes a spell by its card types and applies an ability's effects.

use crate::ability::{Effect, Target, TargetSpec};
use crate::apply::{apply_effect, apply_targeted_effect};
use crate::card::{spell_effects_of, CardData, Keyword};
use crate::card_type::CardType;
use crate::characteristics::permanent_has_keyword;
use crate::choice::{Resume, SuspendedSpell};
use crate::id::{PermanentId, PlayerId};
#[cfg(test)]
use crate::stack::AbilityOrigin;
use crate::stack::{StackObject, StackObjectKind};
use crate::state::{GameEvent, GameState, Permanent};
use crate::CardDatabase;

/// Whether the player in seat `player` is still in the game — the shared condition
/// under every player-shaped [`TargetSpec`] (CR 115.1: a player who has left is not
/// a legal target).
fn player_in_game(state: &GameState, player: PlayerId) -> bool {
    state.players.get(player.0).is_some_and(|p| !p.has_lost)
}

/// Whether the permanent `id` is on the battlefield and satisfies `predicate` —
/// the one battlefield lookup every permanent-shaped [`TargetSpec`] arm below
/// shares, so "the object still exists" is checked in exactly one place and a
/// stale id can never survive by way of a spec that forgot to look.
fn permanent_matches(
    state: &GameState,
    id: PermanentId,
    predicate: impl Fn(&Permanent) -> bool,
) -> bool {
    state.battlefield.iter().any(|p| p.id == id && predicate(p))
}

/// Whether the permanent `perm` has printed card type `card_type`. Type-changing
/// continuous effects are unmodeled, so printed types are authoritative here — the
/// same assumption `crate::combat` makes.
fn has_type(perm: &Permanent, card_type: CardType, db: &CardDatabase) -> bool {
    perm.printed
        .face(db)
        .is_some_and(|face| face.has_type(card_type))
}

/// Whether `target` is a legal choice for `spec` against the *current* `state`
/// (CR 115), for an object controlled by `controller`.
///
/// A pure predicate: it derives legality on demand and never mutates, consistent
/// with the engine's pull-based, no-observer rule.
///
/// `controller` is what makes a possessive spec mean anything — "target creature
/// **you** control" is a different set from each seat, and an authored card carries
/// one spec for both. Every caller supplies the controller of the object choosing
/// the target: the resolve path from the [`StackObject`], the action-legality and
/// enumeration paths from the priority holder (who is, by definition, the player
/// taking the action).
///
/// This is the check the resolve path re-runs on each stored target (CR 608.2b),
/// and the one [`crate::target_requirements`] filters its candidate universe by, so
/// "offered" and "legal on resolution" are one predicate rather than two that can
/// drift.
///
/// **Hexproof** (CR 702.11) is checked here, once, ahead of the spec match: it is a
/// property of the *object being aimed at* rather than of any one spec, so gating it
/// per spec would mean re-stating it in a dozen arms and forgetting it in the next one
/// added. Because it is controller-relative — "can't be the target of spells or
/// abilities your **opponents** control" — the `controller` this already takes is
/// exactly the frame it needs.
#[must_use]
pub(crate) fn target_is_legal(
    spec: TargetSpec,
    target: Target,
    state: &GameState,
    controller: PlayerId,
    db: &CardDatabase,
) -> bool {
    // CR 702.11b: a hexproof permanent is off limits to its controller's opponents and
    // to nobody else. Checked before the spec so every present and future permanent
    // spec inherits it.
    if let Target::Permanent(id) = target {
        if permanent_has_keyword(state, id, Keyword::Hexproof, db)
            && state
                .battlefield
                .iter()
                .any(|p| p.id == id && p.controller != controller)
        {
            return false;
        }
    }
    match (spec, target) {
        // A player is a legal target while they are still in the game.
        (TargetSpec::AnyPlayer, Target::Player(player)) => player_in_game(state, player),
        // "Target opponent" excludes the controller's own seat (CR 102.1), which for
        // a life-loss effect is the difference between a drain and a way to lose.
        (TargetSpec::AnyOpponent, Target::Player(player)) => {
            player != controller && player_in_game(state, player)
        }
        // A permanent target is legal while that exact battlefield object exists.
        (TargetSpec::AnyPermanent, Target::Permanent(id)) => permanent_matches(state, id, |_| true),
        (TargetSpec::AnyNonlandPermanent, Target::Permanent(id)) => {
            permanent_matches(state, id, |p| !has_type(p, CardType::Land, db))
        }
        // A creature target additionally requires the permanent's printed types
        // to include Creature (the layer system's type-changing effects are
        // future work, so printed types are authoritative here).
        (TargetSpec::AnyCreature, Target::Permanent(id)) => {
            permanent_matches(state, id, |p| has_type(p, CardType::Creature, db))
        }
        (TargetSpec::AnyCreatureYouControl, Target::Permanent(id)) => {
            permanent_matches(state, id, |p| {
                p.controller == controller && has_type(p, CardType::Creature, db)
            })
        }
        (TargetSpec::AnyCreatureAnOpponentControls, Target::Permanent(id)) => {
            permanent_matches(state, id, |p| {
                p.controller != controller
                    && player_in_game(state, p.controller)
                    && has_type(p, CardType::Creature, db)
            })
        }
        // Flying is read through the computed characteristics (CR 613.1f), so a
        // creature that was *granted* flying is as legal a target as a printed flyer
        // — and loses legality the instant the grant does.
        (TargetSpec::AnyCreatureWithFlying, Target::Permanent(id)) => {
            permanent_matches(state, id, |p| has_type(p, CardType::Creature, db))
                && permanent_has_keyword(state, id, Keyword::Flying, db)
        }
        (TargetSpec::AnyTappedCreature, Target::Permanent(id)) => {
            permanent_matches(state, id, |p| {
                p.tapped && has_type(p, CardType::Creature, db)
            })
        }
        (TargetSpec::AnyArtifact, Target::Permanent(id)) => {
            permanent_matches(state, id, |p| has_type(p, CardType::Artifact, db))
        }
        (TargetSpec::AnyEnchantment, Target::Permanent(id)) => {
            permanent_matches(state, id, |p| has_type(p, CardType::Enchantment, db))
        }
        // One slot that accepts either type, not two slots (CR 601.2c): a
        // naturalize-style spell names a single target.
        (TargetSpec::AnyArtifactOrEnchantment, Target::Permanent(id)) => {
            permanent_matches(state, id, |p| {
                has_type(p, CardType::Artifact, db) || has_type(p, CardType::Enchantment, db)
            })
        }
        (TargetSpec::AnyLand, Target::Permanent(id)) => {
            permanent_matches(state, id, |p| has_type(p, CardType::Land, db))
        }
        // "Any target" (CR 115.4): legal against a player still in the game, a creature
        // still on the battlefield, or a **planeswalker** still on the battlefield —
        // the union of the AnyPlayer and AnyCreature checks above plus the planeswalker
        // arm issue #608 restored (printed types are authoritative here too). Battles
        // are still unmodeled and so still absent.
        (TargetSpec::AnyTarget, Target::Player(player)) => player_in_game(state, player),
        (TargetSpec::AnyTarget, Target::Permanent(id)) => permanent_matches(state, id, |p| {
            has_type(p, CardType::Creature, db) || has_type(p, CardType::Planeswalker, db)
        }),
        // A spell target is legal while that exact spell is still on the stack
        // (CR 701.5): once it has resolved (or been countered) it is gone, so a
        // counterspell aimed at it fizzles (CR 608.2b). An ability on the stack is
        // not a spell and is never a legal "counter target spell" target.
        (TargetSpec::SpellOnStack, Target::Spell(id)) => state
            .stack
            .iter()
            .any(|o| o.id == id && matches!(o.kind, StackObjectKind::Spell { .. })),
        // A *creature* spell is one whose card has the creature type while it is
        // still on the stack — read off the card, since no permanent exists yet.
        (TargetSpec::CreatureSpellOnStack, Target::Spell(id)) => state.stack.iter().any(|o| {
            o.id == id
                && match o.kind {
                    StackObjectKind::Spell { card } => db
                        .card(card.card)
                        .is_some_and(|c| c.has_type(CardType::Creature)),
                    StackObjectKind::Ability { .. } => false,
                }
        }),
        // Any other spec/value pairing names the wrong kind of object and is
        // never legal.
        _ => false,
    }
}

/// Resolve one object popped from the top of the stack.
///
/// Targets are re-checked first: an individually-illegal target is skipped, and
/// an object all of whose chosen targets are now illegal does not resolve at all
/// — it is removed from the stack with no effect (CR 608.2b, "fizzle").
pub(crate) fn resolve_stack_object(state: &mut GameState, object: StackObject, db: &CardDatabase) {
    // The effects this object resolves, and the specs the stored targets were
    // chosen for (same order the targeting effects consume them). An ability
    // carries its effects on the stack object; a spell's effects are read from
    // its card's spell IR ([`spell_effects_of`], CR 601.2c/608.2c).
    let effects: Vec<Effect> = match &object.kind {
        StackObjectKind::Ability { effects, .. } => effects.clone(),
        StackObjectKind::Spell { card } => spell_effects_of(db, card.card),
    };
    // The specs the stored targets were chosen for (CR 601.2c), in slot order.
    // An ability's specs come from its effects; a spell's include any spell-effect
    // targets **and** an Aura's enchant restriction (CR 303.4a), which is chosen as
    // a target at cast though it produces no `Effect` — so a fizzled Aura target is
    // re-checked on the same path as any other (CR 608.2b).
    let specs: Vec<TargetSpec> = match &object.kind {
        StackObjectKind::Ability { .. } => effects.iter().filter_map(Effect::target_spec).collect(),
        StackObjectKind::Spell { card } => db
            .card(card.card)
            .map(CardData::cast_target_specs)
            .unwrap_or_default(),
    };

    // CR 608.2b: if the object chose targets and *every* one is now illegal, it
    // is removed from the stack without resolving — none of its effects occur. A
    // fizzled *spell* still leaves the stack for its owner's graveyard (it is a
    // card that failed to resolve); a fizzled ability simply ceases to exist.
    if !specs.is_empty()
        && specs
            .iter()
            .zip(&object.targets)
            .all(|(&spec, &target)| !target_is_legal(spec, target, state, object.controller, db))
    {
        if let StackObjectKind::Spell { card } = object.kind {
            if let Some(player) = state.players.get_mut(object.controller.0) {
                player.graveyard.push(card);
            }
            // CR 608.2b: a spell removed for all-targets-illegal fizzled; log it so a
            // client can distinguish it from a spell that resolved or was countered.
            state.record_event(GameEvent::SpellFizzled {
                player: object.controller,
                card,
            });
        }
        return;
    }

    // A spell begins resolving (CR 608.2): log it before its effects so the history
    // reads header-then-consequences (`spell_resolved` precedes any damage/draw/death
    // the spell causes). An ability on the stack is not a "spell" and is not logged.
    if let StackObjectKind::Spell { card } = object.kind {
        state.record_event(GameEvent::SpellResolved {
            player: object.controller,
            card,
        });
    }

    // Apply the object's effects, pairing each targeting effect with the next
    // stored target and applying it only while that target is still legal;
    // individually-illegal targets are skipped (CR 608.2c) while legal ones
    // resolve. Effects with an implicit subject apply unconditionally.
    // An ability carries the permanent it is on; a spell has no source permanent, so a
    // self-referential effect on one would modify nothing.
    let source = match &object.kind {
        StackObjectKind::Ability { source, .. } => Some(*source),
        StackObjectKind::Spell { .. } => None,
    };
    // A spell still owes its card a final zone (CR 608.3) after its effects. Carried
    // into the effect loop so that, if an effect suspends on a player choice, the
    // remainder — including this last step — travels with the suspension instead of
    // being skipped.
    let spell = match &object.kind {
        StackObjectKind::Spell { card } => Some(SuspendedSpell {
            card: *card,
            targets: object.targets.clone(),
        }),
        StackObjectKind::Ability { .. } => None,
    };
    let suspended = apply_effects_with_targets(
        state,
        &effects,
        &object.targets,
        object.controller,
        source,
        spell.clone(),
        db,
    );
    if suspended {
        // A player choice is owed. Everything left of this resolution — the remaining
        // effects and the spell's final zone move — is queued on that choice and
        // happens when it is answered (see `resume_after_choice`).
        return;
    }

    if let Some(spell) = spell {
        put_resolved_spell_in_its_final_zone(state, &spell, object.controller, db);
    }
}

/// Finish a resolved spell by putting its card into its final zone (CR 608.3).
///
/// A permanent spell enters the battlefield with a fresh id (its instance id carries
/// over); an instant/sorcery creates no [`Permanent`] and instead goes to its owner's
/// graveyard as the same instance (CR 608.2m). Ownership apart from control is not
/// tracked yet, so the controller's graveyard stands in on the owner == controller
/// assumption. An ability has no card to move and never reaches here.
///
/// Split out of [`resolve_stack_object`] because a spell whose resolution suspended on
/// a player choice must still take this step afterwards — Tormenting Voice discards,
/// draws two, and only then reaches the graveyard.
pub(crate) fn put_resolved_spell_in_its_final_zone(
    state: &mut GameState,
    spell: &SuspendedSpell,
    controller: PlayerId,
    db: &CardDatabase,
) {
    let card = spell.card;
    if db.card(card.card).is_some_and(CardData::is_permanent) {
        // An Aura enters attached to the object its enchant ability chose
        // (CR 303.4d). That target was picked at cast (CR 601.2c) and re-checked
        // on resolution (CR 608.2b), so an already-illegal one has fizzled and only
        // a legal host remains; a non-Aura permanent enters attached to nothing.
        let attached_to = if db.card(card.card).is_some_and(|c| c.aura.is_some()) {
            spell.targets.iter().find_map(|t| match t {
                crate::ability::Target::Permanent(host) => Some(*host),
                _ => None,
            })
        } else {
            None
        };
        // CR 614.1c/614.12: the battlefield-entry seam applies the permanent's own
        // enters-the-battlefield replacements ("enters tapped", "enters with N +1/+1
        // counters") as it enters — before the SBA loop and before ETB triggers are
        // collected — so a 0/0 that enters with two +1/+1 counters is a 2/2 and lives.
        state.put_card_onto_battlefield(card, controller, false, attached_to, db);
    } else if let Some(player) = state.players.get_mut(controller.0) {
        player.graveyard.push(card);
    }
}

/// Apply `effects` in order on behalf of `controller`, pairing each targeting
/// effect with the next entry of `stored` targets. A targeting effect applies
/// only while its chosen target is still legal against current state (CR 608.2c —
/// individually-illegal targets are skipped); an implicit-subject effect always
/// applies. Shared by spell and ability resolution so both walk targets the same
/// way.
///
/// Returns whether resolution **suspended**: an effect that poses a player choice
/// queues it, hands the caller's remaining work to that choice as a
/// [`Resume`](crate::Resume), and stops. Everything after the suspending effect happens
/// when the choice is answered, not here — which is what lets one card discard, then
/// draw, and still reach its graveyard in the right order.
pub(crate) fn apply_effects_with_targets(
    state: &mut GameState,
    effects: &[Effect],
    stored: &[Target],
    controller: crate::id::PlayerId,
    source: Option<PermanentId>,
    spell: Option<SuspendedSpell>,
    db: &CardDatabase,
) -> bool {
    // The printed card a `same_name_as_source` filter compares against, resolved now
    // because the source permanent may be gone by the time the choice is answered.
    // A token has no card to compare against, and no card in a library or hand can
    // share an identity it has not got (CR 111), so it simply matches nothing.
    let source_card = source.and_then(|id| {
        state
            .battlefield
            .iter()
            .find(|perm| perm.id == id)
            .and_then(|perm| perm.printed.card())
    });
    let mut targets = stored.iter();
    for (index, effect) in effects.iter().enumerate() {
        let spec = effect.target_spec();
        let chosen = if spec.is_some() {
            targets.next().copied()
        } else {
            None
        };

        // A choice-posing effect (CR 701.8 discard, 701.17 scry, 701.19 search) stops
        // here. Choices whose clamped maximum is zero are applied outright inside
        // `pose_choices`, so an empty hand or an empty library never suspends anything.
        if let Some(choices) =
            crate::choice::choices_for_effect(state, effect, controller, source_card, chosen)
        {
            if crate::choice::pose_choices(state, choices, db) {
                crate::choice::attach_resume(
                    state,
                    crate::choice::Resume {
                        controller,
                        source,
                        effects: effects[index + 1..].to_vec(),
                        targets: targets.copied().collect(),
                        spell,
                    },
                );
                return true;
            }
            continue;
        }

        match spec {
            Some(spec) => {
                if let Some(target) = chosen {
                    if target_is_legal(spec, target, state, controller, db) {
                        apply_targeted_effect(state, effect, target, controller, db);
                    }
                }
            }
            None => apply_effect(state, effect, controller, source, db),
        }
    }
    false
}

/// Continue a resolution that suspended on a player choice, once that choice has been
/// answered: apply what was left of the object's effects and, for a spell, put its card
/// into its final zone (CR 608.3).
///
/// Symmetrical with [`resolve_stack_object`]'s own tail, and re-entrant: a remaining
/// effect may pose a further choice (a card that draws, then discards), in which case
/// this suspends again exactly as the first pass did.
pub(crate) fn resume_after_choice(state: &mut GameState, resume: Resume, db: &CardDatabase) {
    let suspended = apply_effects_with_targets(
        state,
        &resume.effects,
        &resume.targets,
        resume.controller,
        resume.source,
        resume.spell.clone(),
        db,
    );
    if suspended {
        return;
    }
    if let Some(spell) = resume.spell {
        put_resolved_spell_in_its_final_zone(state, &spell, resume.controller, db);
    }
}

#[cfg(test)]
mod tests;
