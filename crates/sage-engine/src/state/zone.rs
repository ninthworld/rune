//! Zone mutation methods for permanents, life changes, and damage.

use crate::card_type::CardType;
use crate::combat::AttackTarget;
use crate::id::{CardInstance, CardInstanceId, PermanentId, PlayerId};
use crate::player::Player;
use crate::replacement::{DamageRecipient, EnteringObject, PendingDamage, PendingEntry};
use crate::rng::SplitMix64;
use crate::token::TokenData;
use crate::{CardData, CardDatabase};

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
    /// It goes to the graveyard of [`Permanent::controller`] — the permanent's *base*
    /// controller, which is the engine's owner shim. That is CR 400.7 working: a control
    /// change is a CR 613 layer-2 computation and never touches the stored field, so a
    /// creature that dies while someone else controls it goes home to the seat it
    /// started under rather than staying with the thief. Ownership apart from that
    /// starting seat is still untracked. The physical
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
    /// [`PermanentId`] is minted only on battlefield re-entry). It goes to the exile of
    /// the permanent's **base** controller — the owner shim the graveyard seam uses, and
    /// the reason a stolen permanent is exiled to its own seat (CR 400.7). Returns the
    /// permanent that moved (so a caller can
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
    /// freshly minted id. It goes to the hand of the permanent's **base** controller —
    /// the same owner shim the other two seams use, so bouncing a creature you have
    /// stolen hands it back to its own player (CR 400.7).
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

    /// Put the permanent `id` on **top of its owner's library** — the third
    /// battlefield-departure seam beside the graveyard and the hand.
    ///
    /// A token put anywhere but the battlefield ceases to exist (CR 111.7), and
    /// `card_leaving` is where that rule lives, so a bounced token simply never arrives.
    /// The top of a library is its last element, matching every other read of it.
    pub(crate) fn put_permanent_on_top_of_library(&mut self, id: PermanentId) -> Option<Permanent> {
        let pos = self.battlefield.iter().position(|p| p.id == id)?;
        let perm = self.battlefield.remove(pos);
        if let (Some(card), Some(owner)) =
            (card_leaving(&perm), self.players.get_mut(perm.controller.0))
        {
            owner.library.push(card);
        }
        Some(perm)
    }

    /// **Shuffle** the permanent `id` into its owner's library (CR 701.19) — the fourth
    /// battlefield-departure seam, and the only one whose destination is unordered.
    ///
    /// It is [`Self::put_permanent_on_top_of_library`] plus the shuffle, and the shuffle
    /// is the whole difference: a card put on top is a known next draw, while one shuffled
    /// in is nowhere in particular. The library is randomized whether or not a card
    /// arrived, because the instruction is to shuffle — a **token** shuffled into a library
    /// ceases to exist on the way (CR 111.7, [`card_leaving`]) and the shuffle still
    /// happened.
    ///
    /// Owner, not controller: the card goes to the library of the permanent's **base**
    /// controller, the owner shim every other departure seam uses (CR 400.7), so a stolen
    /// creature shuffled away goes into its own player's deck.
    pub(crate) fn shuffle_permanent_into_library(&mut self, id: PermanentId) -> Option<Permanent> {
        let pos = self.battlefield.iter().position(|p| p.id == id)?;
        let perm = self.battlefield.remove(pos);
        let owner = perm.controller;
        if let (Some(card), Some(player)) = (card_leaving(&perm), self.players.get_mut(owner.0)) {
            player.library.push(card);
        }
        self.shuffle_library(owner);
        Some(perm)
    }

    /// Randomize the order of `player`'s library (CR 103.3) from the seeded stream, storing
    /// the generator's state back into [`GameState::rng_seed`](crate::GameState::rng_seed)
    /// so the next draw of randomness continues it.
    ///
    /// The engine's one in-game shuffle, shared by the search that ends with one
    /// (CR 701.19c) and the effect that shuffles a permanent back in, so a game replays
    /// identically from its seed however the library came to be shuffled (ADR 0006).
    pub(crate) fn shuffle_library(&mut self, player: PlayerId) {
        let Some(mut library) = self.players.get(player.0).map(|p| p.library.clone()) else {
            return;
        };
        let mut rng = SplitMix64::new(self.rng_seed);
        rng.shuffle(&mut library);
        self.rng_seed = rng.state();
        if let Some(p) = self.players.get_mut(player.0) {
            p.library = library;
        }
    }

    /// Put the physical card `card` onto the battlefield under `controller` — the
    /// single card → battlefield seam.
    ///
    /// A card found by a library search, one returned from a graveyard, and one put
    /// there by any other effect all route through here, so a permanent that arrives by
    /// any road is indistinguishable afterwards: it runs through the CR 614 replacement
    /// layer ([`Self::begin_battlefield_entry`]), mints a fresh [`PermanentId`] (the
    /// physical [`CardInstance`](crate::id::CardInstance) carries over unchanged), and is
    /// picked up by the diff-based trigger collector like anything else. A card entering
    /// because a **spell resolved** uses
    /// [`Self::resolve_permanent_spell_onto_battlefield`] instead — the same road, with
    /// the one fact a replacement may ask about that this one cannot answer.
    ///
    /// `tapped` is the entry state the *effect* dictates ("onto the battlefield
    /// tapped"); a card's own "enters tapped" replacement is applied on top and can only
    /// add to it. `attached_to` is the host an entering Aura was cast at (CR 303.4d),
    /// `None` for everything else.
    ///
    /// Returns the new permanent's id, or `None` when nothing entered — see
    /// [`Self::begin_battlefield_entry`].
    pub(crate) fn put_card_onto_battlefield(
        &mut self,
        card: crate::id::CardInstance,
        controller: PlayerId,
        tapped: bool,
        attached_to: Option<PermanentId>,
        db: &CardDatabase,
    ) -> Option<PermanentId> {
        self.put_card_onto_battlefield_with_face(
            card,
            controller,
            tapped,
            attached_to,
            crate::card::Face::Front,
            db,
        )
    }

    /// [`Self::put_card_onto_battlefield`], for a card arriving on a face other than its
    /// front (CR 712.4a) — `return it to the battlefield transformed`.
    ///
    /// The same single seam with the one fact that road adds. It is deliberately a
    /// second entry point rather than a sixth parameter on the common one: every other
    /// caller puts a card onto the battlefield front-face up, and a `Face::Front`
    /// argument repeated at each of them would be a rule stated five times and
    /// forgettable at the sixth.
    pub(crate) fn put_card_onto_battlefield_with_face(
        &mut self,
        card: crate::id::CardInstance,
        controller: PlayerId,
        tapped: bool,
        attached_to: Option<PermanentId>,
        face: crate::card::Face,
        db: &CardDatabase,
    ) -> Option<PermanentId> {
        self.begin_battlefield_entry(
            PendingEntry {
                object: EnteringObject::Card(card),
                face,
                controller,
                tapped,
                attacking: None,
                attached_to,
                counters: Vec::new(),
                announced_x: None,
                cast: false,
                applied: Vec::new(),
                chosen_color: None,
                named_card: None,
                copied: None,
            },
            db,
        )
    }

    /// Put the card of a **resolving permanent spell** onto the battlefield (CR 608.3).
    ///
    /// Identical to [`Self::put_card_onto_battlefield`] in every respect but one: the
    /// permanent is entering *because a spell was cast*, and a printed replacement that
    /// says `without being cast` needs to know. That fact cannot be recovered from the
    /// object — the same creature card reanimated and cast produces the same permanent —
    /// so it is recorded here, at the one seam a cast spell becomes a permanent, and
    /// nowhere else.
    pub(crate) fn resolve_permanent_spell_onto_battlefield(
        &mut self,
        card: crate::id::CardInstance,
        controller: PlayerId,
        attached_to: Option<PermanentId>,
        announced_x: Option<u32>,
        db: &CardDatabase,
    ) -> Option<PermanentId> {
        self.begin_battlefield_entry(
            PendingEntry {
                object: EnteringObject::Card(card),
                // A spell is cast as its front face and resolves as one (CR 712.4a);
                // nothing about casting can put a back face onto the battlefield.
                face: crate::card::Face::Front,
                controller,
                tapped: false,
                attacking: None,
                attached_to,
                counters: Vec::new(),
                announced_x,
                cast: true,
                applied: Vec::new(),
                chosen_color: None,
                named_card: None,
                copied: None,
            },
            db,
        )
    }

    /// Exile the permanent `id` and return it to the battlefield **transformed**, under
    /// its owner's control — the compound zone change
    /// [`Effect::ExileSelfAndReturnTransformed`](crate::Effect) performs.
    ///
    /// Two real zone changes rather than a turn-over, and that is the whole difference
    /// from [`Printed::transform`](crate::Printed): the permanent that comes back is a
    /// **new object** (CR 400.7), with a fresh [`PermanentId`], no counters but the ones
    /// it enters with, no damage, nothing attached, and summoning sickness. What makes
    /// the printed card work is exactly that — a planeswalker back face arrives with its
    /// own starting loyalty (CR 306.5b), applied where every entering permanent's is.
    ///
    /// Both halves use the shared seams: the exile is
    /// [`Self::move_permanent_to_exile`], so a commander's CR 903.9a decision is raised
    /// as it would be for any other exile, and the return is
    /// [`Self::put_card_onto_battlefield_with_face`], so the arrival runs the CR 614
    /// replacement layer and is seen by the trigger diff like any other. A token has no
    /// card to return (CR 111.7) and simply stays exiled — which is to say, ceases to
    /// exist.
    ///
    /// Returns the returned permanent's id, or `None` when there was no permanent, no
    /// card behind it, or the entry did not complete.
    pub(crate) fn exile_and_return_transformed(
        &mut self,
        id: PermanentId,
        db: &CardDatabase,
    ) -> Option<PermanentId> {
        let perm = self.battlefield.iter().find(|p| p.id == id)?;
        // CR 701.28d: a permanent with only one face is not turned over, so it comes back
        // exactly as it left. Asked here rather than trusted, because a face the card has
        // not got would be a permanent nothing could read a characteristic off.
        let has_other_face = perm
            .printed
            .card()
            .and_then(|card| db.card(card))
            .is_some_and(CardData::has_back_face);
        let face = if has_other_face {
            perm.printed.face_up().other()
        } else {
            perm.printed.face_up()
        };
        let departed = self.move_permanent_to_exile(id)?;
        let card = card_leaving(&departed)?;
        let owner = departed.controller;
        if let Some(player) = self.players.get_mut(owner.0) {
            if let Some(position) = player.exile.iter().position(|c| c.id == card.id) {
                player.exile.remove(position);
            }
        }
        self.put_card_onto_battlefield_with_face(card, owner, false, None, face, db)
    }

    /// Run `entry` through the **replacement layer** (CR 614) and, if anything is left
    /// of it, put the permanent it describes on the battlefield.
    ///
    /// The single battlefield-entry seam, and the whole of the layer's control flow:
    ///
    /// 1. Ask what applies to the event and has not applied to it already
    ///    ([`applicable_to_entry`](crate::replacement)).
    /// 2. **Exactly one** — apply it. Either the entry is modified and the loop asks
    ///    again, or the event was replaced outright and nothing enters.
    /// 3. **More than one** — the affected object's controller chooses which applies
    ///    first (CR 616.1). The whole entry goes on the choice queue and the object
    ///    waits in no zone at all until the answer comes back, which is what makes "the
    ///    permanent is never briefly on the battlefield mid-decision" true by
    ///    construction.
    /// 4. **None** — the CR 614.12 questions, if the card asks any, defer the entry the
    ///    same way, one at a time; otherwise the permanent arrives.
    ///
    /// Step 4 is a loop over *unfilled answer slots on the event*, not a branch per
    /// question: an answer writes itself onto the [`PendingEntry`] and re-enters this
    /// function, which asks whatever is still owed and then finishes. That is why a card
    /// asking two questions needs no code saying which comes first, and why the whole
    /// thing terminates — a filled slot is never emptied, exactly as an applied
    /// replacement is never unapplied (CR 614.5).
    ///
    /// Returns the new permanent's id, or **`None` when nothing entered** — because the
    /// event was replaced, or because the entry is deferred on a question. It is
    /// deliberately not a value a caller has to handle: every one of them puts something
    /// somewhere and moves on, so "there is no permanent" is simply the absence of an id.
    pub(crate) fn begin_battlefield_entry(
        &mut self,
        mut entry: PendingEntry,
        db: &CardDatabase,
    ) -> Option<PermanentId> {
        loop {
            let options = crate::replacement::applicable_to_entry(self, db, &entry);
            match options.len() {
                0 => break,
                1 => {
                    if crate::replacement::apply_to_entry(self, &mut entry, options[0], db)
                        == crate::replacement::EntryOutcome::Replaced
                    {
                        return None;
                    }
                }
                // CR 616.1: the affected object's controller — not the effects'
                // controller — picks one to apply. Posed through the one choice queue
                // every mid-resolution decision uses (ADR 0013), so the freeze, the
                // routing, and the priority hand-off need nothing new.
                _ => {
                    self.pending_choices.push(crate::choice::PendingChoice {
                        chooser: entry.controller,
                        question: crate::choice::ChoiceQuestion::Replacement(
                            crate::choice::ReplacementRequest { entry },
                        ),
                        resume: None,
                    });
                    return None;
                }
            }
        }
        // CR 614.12: each choice is made *as* the permanent enters, so the card waits
        // here — in no zone, exactly as a spell's card waits while its resolution is
        // suspended — rather than entering and being amended afterwards. A question
        // whose slot on the event is already filled is not asked again, which is what
        // lets an answer route straight back into this function.
        if let EnteringObject::Card(card) = entry.object {
            if entry.chosen_color.is_none() && crate::card::chooses_color_on_entry(db, card.card) {
                self.pending_choices.push(crate::choice::PendingChoice {
                    chooser: entry.controller,
                    question: crate::choice::ChoiceQuestion::Color(crate::choice::ColorRequest {
                        outcome: crate::choice::ColorOutcome::RecordOnEntry(entry),
                    }),
                    resume: None,
                });
                return None;
            }
            if entry.named_card.is_none() {
                if let Some(class) = crate::card::names_a_card_on_entry(db, card.card) {
                    self.pending_choices.push(crate::choice::PendingChoice {
                        chooser: entry.controller,
                        question: crate::choice::ChoiceQuestion::CardName(
                            crate::choice::CardNameRequest { class, entry },
                        ),
                        resume: None,
                    });
                    return None;
                }
            }
            // The third CR 614.12 question, and the same road: naming a permanent whose
            // copiable values this arrival takes (CR 707.5). It is asked only when the
            // board holds something to name — an `enters as a copy` with no legal choice
            // copies nothing and enters as itself, rather than posing a question with no
            // answer.
            if entry.copied.is_none() {
                if let Some(copying) = crate::card::copies_on_entry(db, card.card, entry.face) {
                    let candidates =
                        crate::copy::copy_choice_candidates(self, copying.of, entry.controller, db);
                    if !candidates.is_empty() {
                        self.pending_choices.push(crate::choice::PendingChoice {
                            chooser: entry.controller,
                            question: crate::choice::ChoiceQuestion::Permanent(
                                crate::choice::CopyChoiceRequest {
                                    of: copying.of,
                                    optional: copying.optional,
                                    outcome: crate::choice::CopyChoiceOutcome::RecordOnEntry {
                                        entry,
                                        subject: copying.subject,
                                    },
                                },
                            ),
                            resume: None,
                        });
                        return None;
                    }
                }
            }
        }
        Some(self.complete_battlefield_entry(&entry, db))
    }

    /// Build the permanent `entry` describes — answers and all — and put it on the
    /// battlefield: the half of [`Self::begin_battlefield_entry`] that actually arrives.
    ///
    /// Split out because it is the one place that mints a permanent, and because it is
    /// reached only once everything the event was waiting on is settled — immediately for
    /// the objects that ask nothing, and one action later for a card whose colour, whose
    /// named card, or whose copiable values (CR 707.5) had to be settled first. The
    /// answers a card's controller gave ride on the event itself rather than on this
    /// signature, so adding a question adds a field there and nothing here — and so a
    /// permanent built from an entry can never disagree with the entry it was built from.
    ///
    /// The one thing applied *here* rather than by the replacement layer is a
    /// planeswalker's starting loyalty (CR 306.5b). It is not a replacement effect: every
    /// planeswalker does it, from its printed number alone, and there is nothing for the
    /// affected player to order it against. It still has to happen before the permanent
    /// is on the battlefield, or a planeswalker would arrive at zero loyalty and be put
    /// into its owner's graveyard by CR 704.5i before anyone could act on it.
    pub(crate) fn complete_battlefield_entry(
        &mut self,
        entry: &PendingEntry,
        db: &CardDatabase,
    ) -> PermanentId {
        let id = PermanentId(self.mint_id());
        // A card carries the physical copy it came from; a token has no card, so a
        // per-object id is minted from the same counter to keep the identity fields
        // uniform (ADR 0015).
        let instance = match &entry.object {
            EnteringObject::Card(card) => card.id,
            EnteringObject::Token(_) => CardInstanceId(self.mint_id()),
        };
        let entered_turn = self.turn;
        let mut counters: std::collections::BTreeMap<super::CounterKind, u32> = Default::default();
        for (counter, count) in &entry.counters {
            *counters.entry(*counter).or_insert(0) += count;
        }
        let printed = entry.object.printed(entry.face);
        // CR 613 layer 1 before CR 306.5b: a permanent entering as a copy of a
        // planeswalker enters with the *copied* starting loyalty, because that is the
        // loyalty its characteristics have by the time the rule is applied.
        let seed = match entry.copied.as_ref().and_then(Option::as_ref) {
            Some(copied) if copied.subject == crate::copy::CopySubject::This => &copied.printed,
            _ => &printed,
        };
        if let Some(loyalty) = seed.face(db).and_then(|face| face.loyalty()) {
            *counters.entry(super::CounterKind::Loyalty).or_insert(0) += loyalty;
        }
        let permanent = Permanent {
            id,
            instance,
            printed,
            controller: entry.controller,
            tapped: entry.tapped,
            entered_turn,
            attacking: entry.attacking,
            blocking: Vec::new(),
            skips_untap: false,
            dealt_damage: false,
            damage: 0,
            counters,
            attached_to: entry.attached_to,
            chosen_color: entry.chosen_color,
            named_card: entry.named_card,
            copied: entry.copied.clone().flatten(),
        };
        self.battlefield.push(permanent);
        id
    }

    /// Create one token with the characteristics `token` under `controller` — the
    /// single effect → battlefield seam for an object that is not a card (CR 111.1).
    ///
    /// The counterpart of [`Self::put_card_onto_battlefield`], and deliberately not a
    /// twin but the *same* road: both build a [`PendingEntry`] and hand it to
    /// [`Self::begin_battlefield_entry`], so a token runs through the replacement layer
    /// (CR 614) exactly as a card does. That is what makes the `nontoken` wording of a
    /// printed replacement a filter rather than an omission — a replacement that does
    /// not say it is asked about tokens too.
    ///
    /// It differs in exactly one place: there is no [`CardInstance`] to carry, because
    /// there is no card. A per-object [`CardInstanceId`] is minted at arrival so the
    /// permanent's identity fields stay uniform, and — a token never leaving the
    /// battlefield as a card (CR 111.7) — that id can never appear in another zone.
    ///
    /// `tapped` and `attacking` are the entry state the creating effect dictates
    /// ("create a tapped 2/2 Zombie token", "create two Cat tokens that are tapped and
    /// attacking"). `attacking` carries *what* is attacked rather than a bare yes,
    /// because a token joining a declaration attacks the same player or planeswalker
    /// the effect's source does — the caller answers that question, and `None` is both
    /// "the effect did not say attacking" and "there was no attack to join". Nothing
    /// here taps a token for attacking: it was never declared as an attacker
    /// (CR 506.3c), so the only thing that taps it is `tapped`. Returns the new
    /// permanent's id, or `None` when the entry was replaced — a token that never
    /// arrives simply ceases to exist (CR 111.7).
    pub(crate) fn create_token(
        &mut self,
        token: TokenData,
        controller: PlayerId,
        tapped: bool,
        attacking: Option<AttackTarget>,
        db: &CardDatabase,
    ) -> Option<PermanentId> {
        self.begin_battlefield_entry(
            PendingEntry {
                object: EnteringObject::Token(Box::new(token)),
                // A token has exactly one face: the effect that created it (CR 111.3).
                face: crate::card::Face::Front,
                controller,
                tapped,
                attacking,
                attached_to: None,
                counters: Vec::new(),
                announced_x: None,
                cast: false,
                applied: Vec::new(),
                chosen_color: None,
                named_card: None,
                copied: None,
            },
            db,
        )
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
        let mut milled = Vec::new();
        for _ in 0..count {
            let Some(p) = self.players.get_mut(player.0) else {
                break;
            };
            let Some(card) = p.library.pop() else {
                break;
            };
            p.graveyard.push(card);
            milled.push(card);
        }
        let moved = u32::try_from(milled.len()).unwrap_or(u32::MAX);
        if moved > 0 {
            self.record_event(GameEvent::CardsMilled {
                player,
                count: moved,
                // Which cards moved, so an `if at least one Zombie card was milled this
                // way` condition has something to read. A graveyard is public, so this
                // reveals nothing the board does not already show.
                cards: milled,
            });
        }
        moved
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
    /// order. Damage to a player uses [`Self::deal_damage`] instead, which is subject to
    /// prevention and records a [`GameEvent::DamageDealt`] rather than a life change.
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

    /// Deal `damage` (CR 120.3) — **the** damage seam, and the one place a prevention
    /// shield is consulted (CR 615.1). Returns how much damage was actually dealt.
    ///
    /// Everything that deals damage builds a [`PendingDamage`] and comes here: the
    /// combat-damage step, a burn spell, a class-wide sweeper, an ability, a fight. The
    /// event is described before it happens, the shields in force are applied to it
    /// ([`after_prevention`](crate::replacement)), and only what survives is dealt —
    /// which is what makes "prevented damage is never marked, never kills, never becomes
    /// life loss, and gains a lifelink source nothing" one fact instead of four.
    ///
    /// A fully prevented event does **nothing at all**: no life moves, nothing is marked,
    /// and no [`GameEvent::DamageDealt`] is recorded, because no damage was dealt to
    /// record. The prevention itself records nothing either — `sage_protocol` has no wire
    /// event for it, and a fact the projection would silently drop is worse than one that
    /// is not recorded (the same call [`crate::replacement`] makes for a replaced entry).
    ///
    /// The return is the amount dealt rather than a yes/no, because two callers need the
    /// number and not the verdict: lifelink gains life equal to the damage *dealt*
    /// (CR 702.15e), and the CR 903.10a commander tally counts the damage that landed.
    /// Zero comes back for damage that was prevented **and** for damage aimed at a
    /// permanent that is no longer there — neither dealt any.
    pub(crate) fn deal_damage(&mut self, damage: PendingDamage, db: &CardDatabase) -> u32 {
        let amount = crate::replacement::after_prevention(self, &damage);
        if amount == 0 {
            return 0;
        }
        match damage.recipient {
            DamageRecipient::Player(player) => {
                self.deal_damage_to_player(player, amount, damage.source, damage.combat)
            }
            DamageRecipient::Permanent(id) => {
                self.deal_damage_to_permanent(id, amount, damage.source, damage.combat, db)
            }
        }
    }

    /// Deal `amount` damage to a player: reduce their life (CR 120.3a) and record a
    /// [`GameEvent::DamageDealt`] when `amount` is nonzero. Zero-life is settled by
    /// the state-based-actions loop (CR 704.5a). Returns the damage dealt — zero for a
    /// seat that does not exist.
    ///
    /// The player half of [`Self::deal_damage`], and private for that reason: damage
    /// that skipped the prevention shield would be damage the rules do not allow, so
    /// there is no way to ask for it.
    fn deal_damage_to_player(
        &mut self,
        player: PlayerId,
        amount: u32,
        source: Option<PermanentId>,
        combat: bool,
    ) -> u32 {
        let Some(p) = self.players.get_mut(player.0) else {
            return 0;
        };
        p.life -= i32::try_from(amount).unwrap_or(i32::MAX);
        if amount > 0 {
            self.record_event(GameEvent::DamageDealt {
                target: DamageTarget::Player(player),
                amount,
                source,
                combat,
            });
        }
        amount
    }

    /// Deal `amount` damage to the permanent `id` (CR 120.3) — the permanent half of
    /// [`Self::deal_damage`], which decides *what damage does to this object* before
    /// anything is recorded.
    ///
    /// Two outcomes, and the permanent's type picks between them:
    ///
    /// - a **planeswalker** has that much loyalty removed (CR 120.3c) — the damage is
    ///   not marked, and the CR 704.5i state-based action reads the counters that are
    ///   left rather than any toughness;
    /// - everything else has the damage **marked** on it (CR 120.3d) for the
    ///   lethal-damage state-based action to compare against toughness (CR 704.5g).
    ///
    /// Every source of damage to a permanent routes through [`Self::deal_damage`] and
    /// arrives here — combat, a targeted burn effect, a class-wide one, a fight — so
    /// "damage to a planeswalker takes loyalty" is one fact rather than four that could
    /// be implemented two-thirds of the way. Both branches record the same
    /// [`GameEvent::DamageDealt`], so a client reports a hit on a planeswalker exactly
    /// as it reports one on a creature. Returns the damage dealt — zero when no
    /// permanent with that id is there to take it, which is how a combat caller knows
    /// not to apply a deathtouch flag or gain lifelink life.
    ///
    /// The **redirection rule is gone** from current rules: damage aimed at a player is
    /// dealt to that player, never moved to a planeswalker they control, so nothing
    /// here looks at where damage was pointed.
    fn deal_damage_to_permanent(
        &mut self,
        id: PermanentId,
        amount: u32,
        source: Option<PermanentId>,
        combat: bool,
        db: &CardDatabase,
    ) -> u32 {
        let is_planeswalker = self
            .battlefield
            .iter()
            .find(|p| p.id == id)
            .and_then(|p| p.printed.face(db))
            .is_some_and(|face| face.has_type(CardType::Planeswalker));
        if !is_planeswalker {
            return self.mark_damage_on_permanent(id, amount, source, combat);
        }
        let Some(perm) = self.battlefield.iter_mut().find(|p| p.id == id) else {
            return 0;
        };
        // CR 120.3c: that much loyalty is removed. Saturating, so overkill damage
        // leaves it at zero rather than wrapping — zero is what CR 704.5i reads.
        let counter = perm
            .counters
            .entry(super::CounterKind::Loyalty)
            .or_insert(0);
        *counter = counter.saturating_sub(amount);
        let logged = LoggedPermanent::of(perm);
        if amount > 0 {
            self.record_event(GameEvent::DamageDealt {
                target: DamageTarget::Permanent(logged),
                amount,
                source,
                combat,
            });
        }
        amount
    }

    /// Mark `amount` damage on the permanent `id` (CR 120.3d) and record a
    /// [`GameEvent::DamageDealt`] when `amount` is nonzero. Returns the damage dealt,
    /// zero when no permanent with that id was present. Marked damage feeds the
    /// lethal-damage SBA (CR 704.5g).
    ///
    /// The marking half of [`Self::deal_damage_to_permanent`]: damage decides between
    /// marking and removing loyalty, and only that seam makes the decision.
    fn mark_damage_on_permanent(
        &mut self,
        id: PermanentId,
        amount: u32,
        source: Option<PermanentId>,
        combat: bool,
    ) -> u32 {
        let Some(perm) = self.battlefield.iter_mut().find(|p| p.id == id) else {
            return 0;
        };
        perm.damage = perm.damage.saturating_add(amount);
        let logged = LoggedPermanent::of(perm);
        if amount > 0 {
            self.record_event(GameEvent::DamageDealt {
                target: DamageTarget::Permanent(logged),
                amount,
                source,
                combat,
            });
        }
        amount
    }

    /// Record that the permanent `source` has **dealt** damage
    /// ([`Permanent::dealt_damage`]) — the counterpart of the two seams above, which
    /// record damage *taken*.
    ///
    /// Called wherever a permanent is the source of damage that was actually dealt: the
    /// combat-damage batch, a fight, and the damage verb of an ability whose source is a
    /// permanent (CR 609.7). Zero damage is not damage (CR 120.3) and never reaches here.
    /// Idempotent and one-way — nothing ever unsets it, because "hasn't dealt damage yet"
    /// has no end.
    pub(crate) fn note_damage_dealt_by(&mut self, source: PermanentId) {
        if let Some(perm) = self.battlefield.iter_mut().find(|p| p.id == source) {
            perm.dealt_damage = true;
        }
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
    /// Scoped to the currently modeled slice. "Objects the player owns" is read off the
    /// **base** controller ([`Permanent::controller`], the owner shim), so a permanent
    /// they had merely gained control of stays on the battlefield and the effect giving
    /// them that control ends — which is what CR 800.4a says. Their own battlefield
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
        // CR 800.4a: "any effects that give that player control of any objects end". A
        // control change is not sourced from a permanent — its timestamp is a bare minted
        // id — so the prune above cannot see it, and a permanent someone else owns would
        // otherwise be left controlled by a seat that is no longer in the game.
        let before = self.static_effects.len();
        self.static_effects
            .retain(|effect| effect.modification != super::Modification::GainControl(seat));
        if self.static_effects.len() != before {
            changed = true;
        }
        // Take the departed player out of combat: any surviving attacker declared
        // against them — or against a planeswalker of theirs, which has just left the
        // game with them — is removed from combat (CR 508 no longer has a defender), so
        // it deals no combat damage anywhere.
        for perm in &mut self.battlefield {
            let gone = match perm.attacking {
                Some(crate::combat::AttackTarget::Player(player)) => player == seat,
                Some(crate::combat::AttackTarget::Planeswalker(id)) => departing.contains(&id),
                None => false,
            };
            if gone {
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
