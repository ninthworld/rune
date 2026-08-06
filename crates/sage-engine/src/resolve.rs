//! Stack resolution: turning the top object of the stack into its effect.
//!
//! When all players pass priority in succession, [`crate::apply_action`] pops
//! the top of the stack and hands it to [`resolve_stack_object`], which first
//! re-checks the object's chosen targets against current state (CR 608.2b), then
//! routes a spell by its card types and applies an ability's effects.

use crate::ability::{Effect, GraveyardScope, Target, TargetGroup, TargetSpec};
use crate::apply::{apply_effect, apply_multi_target_effect, apply_targeted_effect};
use crate::card::{spell_effects_of, CardData, Keyword};
use crate::card_type::CardType;
use crate::characteristics::permanent_has_keyword;
use crate::choice::{Resume, SuspendedSpell};
use crate::id::{PermanentId, PlayerId};
#[cfg(test)]
use crate::stack::AbilityOrigin;
use crate::stack::{AbilitySource, StackObject, StackObjectKind};
use crate::state::{GameEvent, GameState, Permanent};
use crate::CardDatabase;

/// What a resolution knows **about itself** — the three facts every effect it applies
/// may need and none of them can look up.
///
/// One value rather than three parameters because they travel together everywhere,
/// including through a suspension: an effect that stops to ask a question resumes with
/// the same window, the same announced X, and the same declaration about its damage as
/// it started with. Adding a fourth fact is then a field, not another argument on nine
/// functions.
///
/// All three come from the object being resolved, and every one of them is fixed before
/// the first effect runs — which is the point. A resolution does not re-read the cost to
/// find its X or re-read the card to find out whether its damage is preventable; those
/// were settled at announcement (CR 601.2b) and are carried, not derived.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Resolution {
    /// The log sequence this resolution began at — the window a "…this way" question
    /// reads over ([`Condition::MilledThisWay`](crate::Condition)).
    pub start: u64,
    /// The X its object announced (CR 601.2b), or `None` for the overwhelming majority
    /// of objects, which announce none. Read by
    /// [`DerivedAmount::AnnouncedX`](crate::DerivedAmount).
    pub announced_x: Option<u32>,
    /// Whether damage this resolution deals may be prevented (CR 615.1). `true` only
    /// while the object's card declares
    /// [`SpellTrait::DamageCantBePrevented`](crate::SpellTrait) *and* the announced X
    /// meets whatever threshold that declaration names.
    pub damage_unpreventable: bool,
}

impl Resolution {
    /// A resolution beginning at log sequence `start` with nothing announced — the shape
    /// every ability's resolution has, and every spell's that prints neither an X nor a
    /// clause about its damage.
    #[must_use]
    pub fn at(start: u64) -> Self {
        Self {
            start,
            ..Self::default()
        }
    }

    /// `damage` as this resolution deals it: the same event, marked unpreventable when
    /// this resolution's object says its damage cannot be prevented.
    ///
    /// The one place that declaration is applied, so every damage verb inherits it by
    /// going through here rather than by each remembering to ask.
    #[must_use]
    pub(crate) fn damage(
        self,
        damage: crate::replacement::PendingDamage,
    ) -> crate::replacement::PendingDamage {
        if self.damage_unpreventable {
            damage.unpreventable()
        } else {
            damage
        }
    }
}

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

/// Whether a permission granted **this turn** lets `player` aim spells and abilities as
/// though hexproof were not there ([`Effect::IgnoreHexproof`](crate::Effect)).
///
/// The turn comparison is belt-and-braces for the same reason the graveyard-casting one
/// is: the turn boundary clears the list, so an entry from an earlier turn should never
/// be here — checking anyway means the permission cannot outlive its turn even if some
/// future path forgets to clear it.
///
/// Read from inside [`target_is_legal`], which is the *only* place hexproof is enforced,
/// so announcement and the CR 608.2b re-check consult it by construction rather than by
/// each remembering to.
fn ignores_hexproof(state: &GameState, player: PlayerId) -> bool {
    state
        .ignoring_hexproof
        .iter()
        .any(|permission| permission.player == player && permission.turn == state.turn)
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
    // spec inherits it — and skipped outright while `controller` holds a permission to
    // aim as though hexproof were not there, which is the one thing that turns it off.
    if let Target::Permanent(id) = target {
        if !ignores_hexproof(state, controller)
            && permanent_has_keyword(state, id, Keyword::Hexproof, db)
            && state.battlefield.iter().any(|p| {
                p.id == id && crate::characteristics::controller_of(state, p) != controller
            })
        {
            return false;
        }
    }
    match (spec, target) {
        // A player is a legal target while they are still in the game.
        (TargetSpec::AnyPlayer, Target::Player(player)) => player_in_game(state, player),
        // "Target player or planeswalker": the same two arms as "any target" minus the
        // creature one. A player is legal while in the game; a permanent is legal only
        // while it is a planeswalker on the battlefield, so a creature is never a
        // candidate however the spell is aimed.
        (TargetSpec::AnyPlayerOrPlaneswalker, Target::Player(player)) => {
            player_in_game(state, player)
        }
        (TargetSpec::AnyPlayerOrPlaneswalker, Target::Permanent(id)) => {
            permanent_matches(state, id, |p| has_type(p, CardType::Planeswalker, db))
        }
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
        // The one-sided form: the same class, minus the controller's own board. An
        // eliminated seat controls nothing that can be targeted, so its permanents drop
        // out for the same reason a targeted opponent does.
        (TargetSpec::AnyNonlandPermanentAnOpponentControls, Target::Permanent(id)) => {
            permanent_matches(state, id, |p| {
                let owner = crate::characteristics::controller_of(state, p);
                owner != controller
                    && player_in_game(state, owner)
                    && !has_type(p, CardType::Land, db)
            })
        }
        // A mana value read off the permanent's printed face (CR 202.3), through the
        // same accessor a card and a token both answer — so a token is a candidate at
        // mana value 0 (CR 111.4) rather than being quietly absent from the universe.
        // Nothing in the engine modifies a permanent's mana value, so the printed face
        // is the whole answer; when something does, this is the one place that changes.
        (TargetSpec::AnyPermanentWithManaValue { mana_value }, Target::Permanent(id)) => {
            permanent_matches(state, id, |p| {
                p.printed
                    .face(db)
                    .is_some_and(|face| face.mana_value() == mana_value)
            })
        }
        // A creature target additionally requires the permanent's printed types
        // to include Creature (the layer system's type-changing effects are
        // future work, so printed types are authoritative here).
        (TargetSpec::AnyCreature, Target::Permanent(id)) => {
            permanent_matches(state, id, |p| has_type(p, CardType::Creature, db))
        }
        (TargetSpec::AnyCreatureYouControl, Target::Permanent(id)) => {
            permanent_matches(state, id, |p| {
                crate::characteristics::controller_of(state, p) == controller
                    && has_type(p, CardType::Creature, db)
            })
        }
        (TargetSpec::AnyCreatureAnOpponentControls, Target::Permanent(id)) => {
            permanent_matches(state, id, |p| {
                let owner = crate::characteristics::controller_of(state, p);
                owner != controller
                    && player_in_game(state, owner)
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
        // Both types together, both printed: an "artifact creature you control" is one
        // permanent that is both, not two slots.
        (TargetSpec::AnyArtifactCreatureYouControl, Target::Permanent(id)) => {
            permanent_matches(state, id, |p| {
                crate::characteristics::controller_of(state, p) == controller
                    && has_type(p, CardType::Artifact, db)
                    && has_type(p, CardType::Creature, db)
            })
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
        // One slot accepting any of three classes (CR 601.2c), not three slots: Vivien
        // Reid's `-3` names a single target that may be an artifact, an enchantment, or
        // a creature with flying. Flying is read through the computed keywords
        // (CR 613.1f), so a granted flyer is as legal a target as a printed one.
        (TargetSpec::AnyArtifactEnchantmentOrCreatureWithFlying, Target::Permanent(id)) => {
            permanent_matches(state, id, |p| {
                has_type(p, CardType::Artifact, db) || has_type(p, CardType::Enchantment, db)
            }) || (permanent_matches(state, id, |p| has_type(p, CardType::Creature, db))
                && permanent_has_keyword(state, id, Keyword::Flying, db))
        }
        // A card in the object controller's own graveyard (CR 400.7). The only spec that
        // names a card rather than a battlefield object, so it is the only one a
        // `Target::Card` satisfies — and it is scoped to *your* graveyard, so an
        // opponent's identically-named creature card is never a candidate.
        (
            TargetSpec::CardInGraveyard {
                scope,
                class,
                max_mana_value,
            },
            Target::Card(instance),
        ) => {
            let seats: Box<dyn Iterator<Item = &crate::player::Player>> = match scope {
                GraveyardScope::Yours => Box::new(state.players.get(controller.0).into_iter()),
                GraveyardScope::Any => Box::new(state.players.iter()),
            };
            seats
                .flat_map(|player| player.graveyard.iter())
                .find(|card| card.id == instance)
                .and_then(|card| db.card(card.card))
                .is_some_and(|data| {
                    class.matches(data) && max_mana_value.is_none_or(|cap| data.mana_value() <= cap)
                })
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
                    StackObjectKind::Spell { card, .. } => db
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
    // The mode and the X this object announced (CR 601.2b), read once here and used by
    // everything below: which effects there are at all, which target slots they declare,
    // what X the resolution reads, and which spell traits are in force.
    let (announced_mode, announced_x) = match &object.kind {
        StackObjectKind::Spell { mode, x, .. } => (*mode, *x),
        StackObjectKind::Ability { .. } => (None, None),
    };
    let effects: Vec<Effect> = match &object.kind {
        StackObjectKind::Ability { effects, .. } => effects.clone(),
        StackObjectKind::Spell { card, .. } => spell_effects_of(db, card.card, announced_mode),
    };
    // The groups the stored targets were chosen for (CR 601.2c), in slot order.
    // An ability's groups come from its effects; a spell's include any spell-effect
    // targets **and** an Aura's enchant restriction (CR 303.4a), which is chosen as
    // a target at cast though it produces no `Effect` — so a fizzled Aura target is
    // re-checked on the same path as any other (CR 608.2b).
    let groups: Vec<TargetGroup> = match &object.kind {
        StackObjectKind::Ability { .. } => effects.iter().flat_map(Effect::target_groups).collect(),
        StackObjectKind::Spell { card, .. } => db
            .card(card.card)
            .map(|data| data.cast_target_groups(announced_mode))
            .unwrap_or_default(),
    };
    // The spec each stored target was chosen for, flattened out of the groups in the
    // order the announcement filled them — the pairing the fizzle check needs, and the
    // only place an "up to two" group's second target has to be told apart from a second
    // group's first.
    let chosen_specs: Vec<TargetSpec> = groups
        .iter()
        .zip(crate::ability::group_target_counts(
            &groups,
            object.targets.len(),
        ))
        .flat_map(|(group, count)| std::iter::repeat_n(group.spec, count))
        .collect();

    // CR 608.2b: if the object chose targets and *every* one is now illegal, it
    // is removed from the stack without resolving — none of its effects occur. A
    // fizzled *spell* still leaves the stack for its owner's graveyard (it is a
    // card that failed to resolve); a fizzled ability simply ceases to exist.
    //
    // An object that chose **no** targets never fizzles, and that stays true of one
    // whose only group was an "up to N" the player filled with nothing: it did not lose
    // its targets, it never had any (CR 608.2b speaks of the targets an object *has*).
    if !object.targets.is_empty()
        && chosen_specs
            .iter()
            .zip(&object.targets)
            .all(|(&spec, &target)| !target_is_legal(spec, target, state, object.controller, db))
    {
        if let StackObjectKind::Spell { card, .. } = object.kind {
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
    if let StackObjectKind::Spell { card, .. } = object.kind {
        state.record_event(GameEvent::SpellResolved {
            player: object.controller,
            card,
        });
    }

    // Apply the object's effects, pairing each targeting effect with the next
    // stored target and applying it only while that target is still legal;
    // individually-illegal targets are skipped (CR 608.2c) while legal ones
    // resolve. Effects with an implicit subject apply unconditionally.
    //
    // An ability carries **what it came from** (CR 113.3) rather than a permanent id,
    // because not every source is a permanent: an emblem is in no zone at all, and a
    // graveyard ability's source is a card in one. Each self-referential effect asks the
    // source the question it can answer — `permanent()` for a pump, `graveyard_card()`
    // for a return — and gets `None` from the sources that are not that thing, which is
    // the same answer a spell gives and needs no special case anywhere.
    let source = match &object.kind {
        StackObjectKind::Ability { source, .. } => Some(*source),
        StackObjectKind::Spell { .. } => None,
    };
    // A spell still owes its card a final zone (CR 608.3) after its effects. Carried
    // into the effect loop so that, if an effect suspends on a player choice, the
    // remainder — including this last step — travels with the suspension instead of
    // being skipped.
    let spell = match &object.kind {
        StackObjectKind::Spell { card, .. } => Some(SuspendedSpell {
            card: *card,
            targets: object.targets.clone(),
        }),
        StackObjectKind::Ability { .. } => None,
    };
    // What this resolution knows about itself, settled once, before any effect runs, and
    // carried through a suspension — so a mill that stops to ask a question still answers
    // the question about itself correctly when it resumes, and a spell that stops to ask
    // one resumes with the X it was cast for.
    let resolution = Resolution {
        start: state.next_log_sequence,
        announced_x,
        // CR 615.1: whether this object's damage can be prevented is settled here, once,
        // off the card and the value it announced — never asked again at each damage
        // verb, and never re-derived from a cost that has already been paid.
        damage_unpreventable: object
            .has_trait(db, crate::stack::SpellTraitKind::DamageCantBePrevented),
    };
    let suspended = apply_effects_with_targets(
        state,
        &effects,
        &object.targets,
        object.controller,
        source,
        spell.clone(),
        resolution,
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
        // a legal host remains; every other permanent — an **Equipment** included,
        // which chose nothing at cast and waits for its equip ability (CR 301.5c) —
        // enters attached to nothing.
        let attached_to = if db.card(card.card).is_some_and(|c| {
            c.attachment
                .as_ref()
                .is_some_and(|a| a.kind == crate::card::AttachmentKind::Aura)
        }) {
            spell.targets.iter().find_map(|t| match t {
                crate::ability::Target::Permanent(host) => Some(*host),
                _ => None,
            })
        } else {
            None
        };
        // CR 614: the battlefield-entry seam runs the arrival through the replacement
        // layer — the permanent's own "enters tapped" / "enters with counters", plus any
        // replacement an ability created — before the SBA loop and before ETB triggers
        // are collected, so a 0/0 that enters with two +1/+1 counters is a 2/2 and lives.
        // This is the one seam that records the entry as a **cast** one, which is what a
        // replacement saying `without being cast` reads.
        state.resolve_permanent_spell_onto_battlefield(card, controller, attached_to, db);
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
#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_effects_with_targets(
    state: &mut GameState,
    effects: &[Effect],
    stored: &[Target],
    controller: crate::id::PlayerId,
    source: Option<AbilitySource>,
    spell: Option<SuspendedSpell>,
    resolution: Resolution,
    db: &CardDatabase,
) -> bool {
    // The printed card a `same_name_as_source` filter compares against, resolved now
    // because the source permanent may be gone by the time the choice is answered.
    // A token has no card to compare against, and no card in a library or hand can
    // share an identity it has not got (CR 111), so it simply matches nothing.
    let source_card = source.and_then(AbilitySource::permanent).and_then(|id| {
        state
            .battlefield
            .iter()
            .find(|perm| perm.id == id)
            .and_then(|perm| perm.printed.card())
    });
    // A work queue rather than a walk over a fixed list, because a conditional's chosen
    // branch is *spliced in front of what is left*: the branch then travels through the
    // same targeting, choice-posing, and suspension machinery every other effect does,
    // instead of a second, parallel application path that would have to reimplement all
    // three.
    let mut queue: std::collections::VecDeque<Effect> = effects.iter().cloned().collect();
    let mut targets: std::collections::VecDeque<Target> = stored.iter().copied().collect();
    while let Some(effect) = queue.pop_front() {
        // CR 608.2: an intervening condition is judged as the effect is reached, so it
        // sees everything the effects before it did — including a mill or a discard this
        // same resolution performed.
        if let Effect::Conditional {
            condition,
            then,
            otherwise,
        } = &effect
        {
            let branch = if crate::condition::condition_holds(
                state,
                condition,
                controller,
                resolution.start,
                db,
            ) {
                then
            } else {
                otherwise
            };
            for nested in branch.iter().rev() {
                queue.push_front(nested.clone());
            }
            continue;
        }

        // An effect's groups take as many stored targets as the announcement gave them
        // (CR 601.2c) — no group for a class-subject effect, one for nearly every
        // targeting one, and two for an effect whose slots do not share a spec.
        let groups = effect.target_groups();
        let taken: Vec<Target> = {
            // The remaining queue's groups still owe their minimums; whatever is left
            // over belongs to this effect, up to its groups' summed maximum.
            let later: usize = queue
                .iter()
                .flat_map(Effect::target_groups)
                .map(|g| usize::from(g.min))
                .sum();
            let available = targets.len().saturating_sub(later);
            let capacity: usize = groups.iter().map(|g| usize::from(g.max)).sum();
            let take = available.min(capacity);
            (0..take).filter_map(|_| targets.pop_front()).collect()
        };

        // A choice-posing effect (CR 701.8 discard, 701.17 scry, 701.19 search) stops
        // here. Choices whose clamped maximum is zero are applied outright inside
        // `pose_choices`, so an empty hand or an empty library never suspends anything.
        if let Some(choices) =
            crate::choice::choices_for_effect(state, &effect, controller, source_card, &taken)
        {
            if crate::choice::pose_choices(state, choices, db) {
                crate::choice::attach_resume(
                    state,
                    crate::choice::Resume {
                        controller,
                        source,
                        effects: queue.into_iter().collect(),
                        targets: targets.into_iter().collect(),
                        spell,
                        resolution,
                    },
                );
                return true;
            }
            continue;
        }

        match groups.as_slice() {
            [] => apply_effect(state, &effect, controller, source, resolution, db),
            // CR 608.2c: each chosen target is re-checked on its own, and an
            // individually-illegal one is skipped while its legal siblings resolve. For
            // an "up to two" effect that is the difference between one dead target
            // wasting the whole ability and it doing half its work.
            [group] => {
                for target in taken {
                    if target_is_legal(group.spec, target, state, controller, db) {
                        apply_targeted_effect(
                            state, &effect, target, controller, source, resolution, db,
                        );
                    }
                }
            }
            // An effect whose slots have **different** specs acts on all of them or on
            // none: its slots are not interchangeable, so half of a fight is not a
            // smaller fight but a different effect the card never printed. CR 701.12c
            // says so outright — if either creature is an illegal target, neither deals
            // nor is dealt damage — and it is the conservative reading of CR 608.2c for
            // whatever multi-slot effect comes next.
            slots => {
                let filled = taken.len() == slots.len()
                    && slots.iter().zip(&taken).all(|(group, &target)| {
                        target_is_legal(group.spec, target, state, controller, db)
                    });
                if filled {
                    apply_multi_target_effect(state, &effect, &taken, db);
                }
            }
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
        resume.resolution,
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
