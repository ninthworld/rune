//! Zone mutation methods for permanents, life changes, and damage.

use crate::card_type::CardType;
use crate::id::{CardInstance, CardInstanceId, PermanentId, PlayerId};
use crate::player::Player;
use crate::token::{Printed, TokenData};
use crate::CardDatabase;

use super::{CommanderDamage, DamageTarget, GameEvent, GameState, LoggedPermanent, Permanent};

/// The physical card a permanent leaving the battlefield puts into the zone it is
/// headed for — or `None` for a **token**, which puts nothing anywhere because it
/// ceases to exist the moment it would leave (CR 111.7).
///
/// The single expression of CR 111.7 in the engine. All three leave-the-battlefield
/// seams below route through it, so "a token that dies reaches no graveyard", "a
/// bounced token never arrives in a hand", and "an exiled token is not in exile" are
/// one fact rather than three that could be implemented two-thirds of the way. CR
/// 704.5d states the rule as a state-based action over a token in the wrong zone; the
/// token never gets there, which is the same observable game and needs no phantom
/// object to clean up.
fn card_leaving(perm: &Permanent) -> Option<CardInstance> {
    perm.printed.card().map(|card| CardInstance {
        id: perm.instance,
        card,
    })
}

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
        if let (Some(card), Some(owner)) =
            (card_leaving(&perm), self.players.get_mut(perm.controller.0))
        {
            owner.graveyard.push(card);
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
        if let (Some(card), Some(owner)) =
            (card_leaving(&perm), self.players.get_mut(perm.controller.0))
        {
            owner.exile.push(card);
            flag_commander_return(owner, perm.instance);
        }
        Some(perm)
    }

    /// Move the permanent `id` from the battlefield to its owner's **hand** — the
    /// single leaves-battlefield → hand seam the bounce verb ([`crate::Effect::ReturnToHand`])
    /// routes through, mirroring [`Self::move_permanent_to_graveyard`] and
    /// [`Self::move_permanent_to_exile`] (CR 400.7).
    ///
    /// Identity semantics are exactly those seams': the physical
    /// [`CardInstance`](crate::id::CardInstance) carries over unchanged while the battlefield
    /// [`PermanentId`] is dropped, so recasting the card produces a brand-new object with a
    /// freshly minted id. Ownership apart from control is not tracked yet, so the controller
    /// stands in as the owner (the same shim the other two seams use).
    ///
    /// A return to hand is **not** a death (CR 700.4): the card does not reach a graveyard,
    /// so the diff-based collector sees no "dies" event for it and no commander return is
    /// flagged (CR 903.9a covers a graveyard or exile, not the hand). Returns the permanent
    /// that moved, or `None` when no permanent with that id was on the battlefield. A bare
    /// zone move that records no log event of its own.
    pub(crate) fn return_permanent_to_hand(&mut self, id: PermanentId) -> Option<Permanent> {
        let pos = self.battlefield.iter().position(|p| p.id == id)?;
        let perm = self.battlefield.remove(pos);
        if let (Some(card), Some(owner)) =
            (card_leaving(&perm), self.players.get_mut(perm.controller.0))
        {
            owner.hand.push(card);
        }
        Some(perm)
    }

    /// Put the physical card `card` onto the battlefield under `controller` — the
    /// single card → battlefield seam.
    ///
    /// A permanent spell resolving (CR 608.3) and a card found by a library search or a
    /// look-at-the-top both route through here, so a permanent that arrives by either
    /// road is indistinguishable afterwards: it mints a fresh [`PermanentId`] (the
    /// physical [`CardInstance`](crate::id::CardInstance) carries over unchanged), applies
    /// its own CR 614.1c/614.12 enters-the-battlefield self-replacements *as* it enters
    /// — before any state-based action or ETB trigger observes it — and is picked up by
    /// the diff-based trigger collector like anything else.
    ///
    /// `tapped` is the entry state the *effect* dictates ("onto the battlefield
    /// tapped"); a card's own "enters tapped" replacement is applied on top and can only
    /// add to it. `attached_to` is the host an entering Aura was cast at (CR 303.4d),
    /// `None` for everything else. Returns the new permanent's id.
    pub(crate) fn put_card_onto_battlefield(
        &mut self,
        card: crate::id::CardInstance,
        controller: PlayerId,
        tapped: bool,
        attached_to: Option<PermanentId>,
        db: &CardDatabase,
    ) -> PermanentId {
        let id = PermanentId(self.mint_id());
        let entered_turn = self.turn;
        let mut permanent = Permanent {
            id,
            instance: card.id,
            printed: Printed::Card(card.card),
            controller,
            tapped,
            entered_turn,
            attacking: None,
            blocking: None,
            damage: 0,
            counters: Default::default(),
            attached_to,
        };
        crate::card::apply_enters_replacements(db, &mut permanent);
        self.battlefield.push(permanent);
        id
    }

    /// Create one token with the characteristics `token` under `controller` — the
    /// single effect → battlefield seam for an object that is not a card (CR 111.1).
    ///
    /// The counterpart of [`Self::put_card_onto_battlefield`], and deliberately its
    /// twin: the token mints a fresh [`PermanentId`], takes the current turn as its
    /// [`entered_turn`](Permanent::entered_turn) so it is summoning-sick like anything
    /// else that just arrived (CR 302.6), applies its own enters-the-battlefield
    /// self-replacements, and is then simply pushed onto the battlefield — where the
    /// diff-based trigger collector picks it up as an entry indistinguishable from a
    /// creature spell resolving.
    ///
    /// It differs in exactly one place: there is no [`CardInstance`] to carry, because
    /// there is no card. A per-object [`CardInstanceId`] is still minted so the
    /// permanent's identity fields stay uniform, and — a token never leaving the
    /// battlefield as a card (CR 111.7) — that id can never appear in another zone.
    ///
    /// `tapped` is the entry state the creating effect dictates ("create a tapped 2/2
    /// Zombie token"). Returns the new permanent's id.
    pub(crate) fn create_token(
        &mut self,
        token: TokenData,
        controller: PlayerId,
        tapped: bool,
        db: &CardDatabase,
    ) -> PermanentId {
        let id = PermanentId(self.mint_id());
        let instance = CardInstanceId(self.mint_id());
        let entered_turn = self.turn;
        let mut permanent = Permanent {
            id,
            instance,
            printed: Printed::Token(Box::new(token)),
            controller,
            tapped,
            entered_turn,
            attacking: None,
            blocking: None,
            damage: 0,
            counters: Default::default(),
            attached_to: None,
        };
        crate::card::apply_enters_replacements(db, &mut permanent);
        self.battlefield.push(permanent);
        id
    }

    /// Put the top `count` cards of `player`'s library into their graveyard
    /// (CR 701.13, "mill"), and record a [`GameEvent::CardsMilled`] for however many
    /// actually moved.
    ///
    /// Milling is **not** drawing: a player asked to mill more cards than their library
    /// holds simply mills what is there, and never sets the attempted-draw-from-empty flag
    /// the CR 704.5c decking loss reads. Routing every mill through this one seam is what
    /// keeps that distinction from being re-derived (and got wrong) per effect. Returns the
    /// number of cards moved.
    pub(crate) fn mill(&mut self, player: PlayerId, count: u32) -> u32 {
        let mut milled = 0;
        for _ in 0..count {
            let Some(p) = self.players.get_mut(player.0) else {
                break;
            };
            let Some(card) = p.library.pop() else {
                break;
            };
            p.graveyard.push(card);
            milled += 1;
        }
        if milled > 0 {
            self.record_event(GameEvent::CardsMilled {
                player,
                count: milled,
            });
        }
        milled
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
        if perm
            .printed
            .face(db)
            .is_some_and(|face| face.has_type(CardType::Creature))
        {
            self.record_event(GameEvent::PermanentDied {
                permanent: LoggedPermanent::of(&perm),
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
        let logged = LoggedPermanent::of(perm);
        if amount > 0 {
            self.record_event(GameEvent::DamageDealt {
                target: DamageTarget::Permanent(logged),
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
