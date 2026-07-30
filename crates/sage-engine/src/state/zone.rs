//! Zone mutation methods for permanents, life changes, and damage.

use crate::card_type::CardType;
use crate::id::{CardInstanceId, PermanentId, PlayerId};
use crate::player::Player;
use crate::CardDatabase;

use super::{CommanderDamage, DamageTarget, GameEvent, GameState, LoggedPermanent, Permanent};

/// Flag the CR 903.9a return-to-command-zone decision on `owner` when the object that
/// just left the battlefield is their commander.
///
/// A commander that would be put into a graveyard **or** exile may instead be moved to
/// the command zone by its owner (CR 903.9a). This is not a replacement effect (the
/// compatibility report's replacement-effects exclusion must stay true): the card
/// really moves to the zone it was headed for, and the choice is offered at the next
/// state-based check. Both zone seams ([`GameState::move_permanent_to_graveyard`] and
/// [`GameState::move_permanent_to_exile`]) call this so the pending decision is raised
/// identically no matter which zone the commander went to, and
/// [`crate::valid_actions`] surfaces it.
fn flag_commander_return(owner: &mut Player, instance: CardInstanceId) {
    if let Some(commander) = owner.commander.as_mut() {
        if commander.instance == instance {
            commander.return_pending = true;
        }
    }
}

impl GameState {
    /// Move the permanent `id` from the battlefield to its owner's graveyard —
    /// the single leaves-battlefield → graveyard seam every death routes through
    /// (CR 700.4: a creature put into a graveyard from the battlefield; CR 603.6c:
    /// the resulting "dies" event). Both the lethal-damage / deathtouch
    /// state-based action (CR 704.5g/h, in [`crate::sba`]) and a `Destroy` effect
    /// (CR 701.7, in [`crate::apply`]) call this, so a death looks identical no
    /// matter its cause and is observed uniformly by the diff-based trigger
    /// collector ([`crate::triggers`]). Returns `true` when a permanent with that
    /// id was on the battlefield and moved.
    ///
    /// Ownership apart from control is not tracked yet, so the controller stands
    /// in as the owner (mirrors the engine→protocol `owner` shim); the physical
    /// [`CardInstance`](crate::id::CardInstance) carries over unchanged while the battlefield
    /// [`PermanentId`] is dropped, preserving zone-change identity.
    ///
    /// Returns the permanent that moved (so a caller can inspect its identity, e.g.
    /// to log a creature death), or `None` when no permanent with that id was on the
    /// battlefield. This is a bare zone move and records no log event; a creature
    /// death is logged by [`Self::destroy_permanent`], which routes through here.
    pub(crate) fn move_permanent_to_graveyard(&mut self, id: PermanentId) -> Option<Permanent> {
        let pos = self.battlefield.iter().position(|p| p.id == id)?;
        let perm = self.battlefield.remove(pos);
        if let Some(owner) = self.players.get_mut(perm.controller.0) {
            owner.graveyard.push(crate::id::CardInstance {
                id: perm.instance,
                card: perm.card,
            });
            flag_commander_return(owner, perm.instance);
        }
        Some(perm)
    }

    /// Move the permanent `id` from the battlefield to its owner's exile zone — the
    /// single leaves-battlefield → exile seam that effect resolution (an exile-removal
    /// spell or ability, [`crate::apply`]) and any future state-based path route
    /// through, mirroring [`Self::move_permanent_to_graveyard`] (CR 406.2 / CR 700.4).
    /// Keeping exile behind one seam is what lets a commander's owner ever be offered
    /// the CR 903.9a return, and makes every exile observed uniformly by the diff-based
    /// trigger collector ([`crate::triggers`]).
    ///
    /// Identity semantics are exactly the graveyard seam's: the physical
    /// [`CardInstance`](crate::id::CardInstance) carries over unchanged while the battlefield [`PermanentId`]
    /// is dropped, so a later return to any zone is a brand-new object (a fresh
    /// [`PermanentId`] is minted only on battlefield re-entry). Ownership apart from
    /// control is not tracked yet, so the controller stands in as the owner (the same
    /// shim the graveyard seam uses). Returns the permanent that moved (so a caller can
    /// inspect its identity), or `None` when no permanent with that id was on the
    /// battlefield. A bare zone move that records no log event of its own.
    pub(crate) fn move_permanent_to_exile(&mut self, id: PermanentId) -> Option<Permanent> {
        let pos = self.battlefield.iter().position(|p| p.id == id)?;
        let perm = self.battlefield.remove(pos);
        if let Some(owner) = self.players.get_mut(perm.controller.0) {
            owner.exile.push(crate::id::CardInstance {
                id: perm.instance,
                card: perm.card,
            });
            flag_commander_return(owner, perm.instance);
        }
        Some(perm)
    }

    /// Move `id` to its owner's graveyard and, if it was a **creature**, record a
    /// [`GameEvent::PermanentDied`] (CR 700.4 — only a creature "dies").
    ///
    /// This is the single creature-death seam: both the lethal-damage / zero-
    /// toughness / deathtouch state-based actions (CR 704.5f/g/h) and a `Destroy`
    /// effect (CR 701.7) route deaths through here, so a death is logged once, in
    /// order, no matter its cause — and an Aura or other noncreature moving to a
    /// graveyard (e.g. the CR 704.5m orphaned-Aura action) is *not* mislabeled as a
    /// death (it should call [`Self::move_permanent_to_graveyard`] directly).
    /// Creature-ness is read from printed types, consistent with the rest of the
    /// engine (type-changing effects are unmodeled). Returns whether a permanent
    /// moved.
    pub(crate) fn destroy_permanent(&mut self, id: PermanentId, db: &CardDatabase) -> bool {
        let Some(perm) = self.move_permanent_to_graveyard(id) else {
            return false;
        };
        if db
            .card(perm.card)
            .is_some_and(|c| c.has_type(CardType::Creature))
        {
            self.record_event(GameEvent::PermanentDied {
                permanent: LoggedPermanent {
                    permanent: perm.id,
                    card: perm.card,
                },
            });
        }
        true
    }

    /// Adjust a player's life by `delta` and record a [`GameEvent::LifeChanged`]
    /// when the change is nonzero. The seam every **non-damage** life movement
    /// (life gain, life paid or lost) routes through, so the log observes it in
    /// order. Damage to a player uses [`Self::deal_damage_to_player`] instead, which
    /// records a [`GameEvent::DamageDealt`] rather than a life change.
    pub(crate) fn change_life(&mut self, player: PlayerId, delta: i32) {
        let Some(p) = self.players.get_mut(player.0) else {
            return;
        };
        p.life += delta;
        if delta != 0 {
            self.record_event(GameEvent::LifeChanged {
                player,
                amount: delta,
            });
        }
    }

    /// Deal `amount` damage to a player: reduce their life (CR 120.3a) and record a
    /// [`GameEvent::DamageDealt`] when `amount` is nonzero. Zero-life is settled by
    /// the state-based-actions loop (CR 704.5a).
    pub(crate) fn deal_damage_to_player(&mut self, player: PlayerId, amount: u32) {
        let Some(p) = self.players.get_mut(player.0) else {
            return;
        };
        p.life -= i32::try_from(amount).unwrap_or(i32::MAX);
        if amount > 0 {
            self.record_event(GameEvent::DamageDealt {
                target: DamageTarget::Player(player),
                amount,
            });
        }
    }

    /// Mark `amount` damage on the permanent `id` (CR 120.3d) and record a
    /// [`GameEvent::DamageDealt`] when `amount` is nonzero. Returns whether a
    /// permanent with that id was present (so a combat caller can then apply a
    /// deathtouch flag). Marked damage feeds the lethal-damage SBA (CR 704.5g).
    pub(crate) fn mark_damage_on_permanent(&mut self, id: PermanentId, amount: u32) -> bool {
        let Some(perm) = self.battlefield.iter_mut().find(|p| p.id == id) else {
            return false;
        };
        perm.damage = perm.damage.saturating_add(amount);
        let card = perm.card;
        if amount > 0 {
            self.record_event(GameEvent::DamageDealt {
                target: DamageTarget::Permanent(LoggedPermanent {
                    permanent: id,
                    card,
                }),
                amount,
            });
        }
        true
    }

    /// Add `amount` to the cumulative combat damage the commander owned by
    /// `commander` has dealt `damaged` this game (CR 903.10a), keyed to the
    /// commander designation and the damaged player so the total survives the
    /// commander's zone changes and recasts. A zero amount records nothing. Only
    /// combat damage a commander deals a player routes here (see
    /// `apply.rs :: apply_combat_batch`).
    pub(crate) fn add_commander_damage(
        &mut self,
        commander: PlayerId,
        damaged: PlayerId,
        amount: u32,
    ) {
        if amount == 0 {
            return;
        }
        match self
            .commander_damage
            .iter_mut()
            .find(|entry| entry.commander == commander && entry.damaged == damaged)
        {
            Some(entry) => entry.amount = entry.amount.saturating_add(amount),
            None => self.commander_damage.push(CommanderDamage {
                commander,
                damaged,
                amount,
            }),
        }
    }

    /// Remove every object owned by the eliminated player `seat` from the game
    /// (CR 800.4a), and take that player out of combat. Idempotent: it removes only
    /// what is still present and reports whether anything changed, so the
    /// state-based-actions loop reaches a fixed point.
    ///
    /// Scoped to the currently modeled slice. Ownership is not tracked separately
    /// from control yet (a permanent's owner mirrors its controller), so "objects
    /// the player owns" is read as the objects they control: their battlefield
    /// permanents (including Auras they control attached to others' permanents) and
    /// their stack objects leave the game, and their private/graveyard/exile zones
    /// are emptied. A surviving player's Aura left dangling on a departed permanent
    /// is handled by the CR 704.5m state-based action in the same fixed point. The
    /// full CR 800.4a treatment of control-changing effects, foreign-owned objects,
    /// and delayed triggers is future work, gated on an ownership model.
    pub(crate) fn remove_player_from_game(&mut self, seat: PlayerId) -> bool {
        let mut changed = false;
        // The permanents leaving the battlefield — captured so continuous effects
        // sourced from them can be pruned (they can never match again; ids are not
        // reused). They leave the game entirely (CR 800.4a), not to a graveyard.
        let departing: Vec<PermanentId> = self
            .battlefield
            .iter()
            .filter(|perm| perm.controller == seat)
            .map(|perm| perm.id)
            .collect();
        if !departing.is_empty() {
            self.battlefield.retain(|perm| perm.controller != seat);
            self.static_effects
                .retain(|effect| !departing.iter().any(|id| id.0 == effect.source));
            changed = true;
        }
        // Take the departed player out of combat: any surviving attacker declared
        // against them is removed from combat (CR 508 no longer has a defender), so
        // it deals no combat damage to a player.
        for perm in &mut self.battlefield {
            if perm.attacking == Some(seat) {
                perm.attacking = None;
                changed = true;
            }
        }
        // Their spells and abilities on the stack cease to exist (CR 800.4a).
        let before = self.stack.len();
        self.stack.retain(|obj| obj.controller != seat);
        if self.stack.len() != before {
            changed = true;
        }
        // Their hand, library, graveyard, exile, and command zone are no longer
        // part of the game (CR 800.4a).
        if let Some(player) = self.players.get_mut(seat.0) {
            for zone in [
                &mut player.hand,
                &mut player.library,
                &mut player.graveyard,
                &mut player.exile,
                &mut player.command,
            ] {
                if !zone.is_empty() {
                    zone.clear();
                    changed = true;
                }
            }
            // The departed player's commander designation leaves the game with
            // them; drop any pending return so no stale choice lingers.
            if player.commander.take().is_some() {
                changed = true;
            }
        }
        changed
    }
}
