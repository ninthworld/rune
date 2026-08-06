//! Replacement and prevention effects (CR 614, CR 615): modifying an event *before* it
//! happens.
//!
//! Everything else in the engine reacts to an event after the fact — a trigger is
//! collected by diffing the state a change produced, a state-based action tidies up a
//! board that has already moved. A replacement effect is the opposite: it watches for
//! an event that *would* happen and substitutes a different one, so the original event
//! never occurs at all and nothing downstream ever sees it (CR 614.1).
//!
//! The layer is built on three facts, and they are the whole of it:
//!
//! - **The event is a value.** [`PendingEntry`] is a permanent's arrival on the
//!   battlefield described in full *before* the battlefield has heard about it — the
//!   object, its controller, the tapped state and counters it would arrive with, and
//!   whether it got there by being cast. A replacement rewrites that value or cancels
//!   it; only when nothing is left to apply does the permanent actually enter
//!   ([`GameState::begin_battlefield_entry`](crate::GameState)).
//! - **The affected player orders them (CR 616.1).** When more than one replacement
//!   applies to the same event, the *affected object's controller* — not the effects'
//!   controller — chooses which applies first. That is a player decision in the middle
//!   of a resolution, so it rides the queue every other one rides
//!   ([`ChoiceQuestion::Replacement`](crate::ChoiceQuestion), ADR 0013): the entry waits
//!   off the battlefield while the question is owed, exactly as a card naming a colour
//!   as it enters does, and there is no second mechanism.
//! - **Nothing applies twice to one event (CR 614.5).** [`PendingEntry::applied`]
//!   records every replacement already applied to *this* event, and the collector below
//!   skips them. Without it a permanent that enters tapped would enter tapped forever:
//!   the modified event still matches the effect that modified it.
//!
//! ## Where replacements come from
//!
//! Two source lists, walked into one ordered set — the same shape the ability
//! collectors use for emblems (ADR 0017):
//!
//! - the entering object's **own** abilities, its self-replacements (CR 614.1c) —
//!   [`Ability::EntersTapped`] and [`Ability::EntersWithCounters`]; and
//! - [`GameState::replacements`](crate::GameState), the one-shot replacement effects an
//!   ability created for the turn ([`Effect::CreateReplacement`](crate::Effect)) — the
//!   `the next time … this turn, … instead` a card prints.
//!
//! A **static** replacement ability on a permanent (`Permanents entering the battlefield
//! under your opponents' control enter tapped`) is a third source list and is not
//! modeled; nothing outside these two produces a replacement effect, and the
//! compatibility report's exclusion says so.
//!
//! ## The second event: damage (CR 615)
//!
//! [`PendingDamage`] is damage described before it lands — who or what would take it,
//! how much, and whether it is **combat** damage — and it is consulted by the same kind
//! of layer at the one seam damage is dealt
//! ([`GameState::deal_damage`](crate::GameState)). Prevention is a replacement effect
//! (CR 615.1): the damage that is prevented is never dealt at all, so it is never marked
//! on a permanent (CR 120.3d), never feeds the lethal-damage state-based action
//! (CR 704.5g), never becomes life loss (CR 120.3a), and gains a lifelink source nothing
//! (CR 702.15e).
//!
//! Two things make it a smaller layer than the entry event rather than a copy of it, and
//! both are facts about the shields a card can print today rather than shortcuts:
//!
//! - **A shield is not one-shot.** `Prevent all combat damage that would be dealt this
//!   turn` covers every damage event for the rest of the turn, so it lives in
//!   [`GameState::prevention`](crate::GameState) — a duration, expiring in the cleanup
//!   step beside the pump it is authored like (CR 514.2) — rather than in the one-shot
//!   [`GameState::replacements`](crate::GameState) list, which a single application
//!   spends.
//! - **No one is asked to order them.** Every shield modeled prevents *all* of the
//!   damage it applies to, so two applicable shields produce the same event in either
//!   order and the CR 616.1 question has no answer that could differ. A shield that
//!   prevents only part of the damage is what would make the order observable, and it
//!   would ask through the same queue an entry's ordering does.
//!
//! ## What is not here
//!
//! Two events are replaceable: a permanent **entering** the battlefield, and **damage**
//! being dealt. A permanent *leaving* the battlefield, a card being drawn, and life
//! being gained are all events a printed card can replace, and none of them routes
//! through this layer yet. Adding one is a variant of the event value plus its seam —
//! the collector, the ordering choice, and the never-twice rule are already shared.
//!
//! Damage that **can't be prevented** is not here either: it is a fact about the damage
//! event that the shield would have to read, and the one M19 card that prints it needs
//! `X` before it needs prevention.
//!
//! [`Ability::EntersChoosingColor`] is deliberately *not* one of the replacements
//! collected here even though CR 614.12 calls it one. It is not a modification anybody
//! could order against another: it is a question, and the entry is already deferred onto
//! the choice queue until it is answered (ADR 0013 §8). Folding it in would mean posing
//! a choice about a choice.

use serde::Deserialize;

use crate::ability::Ability;
use crate::card_type::CardType;
use crate::combat::AttackTarget;
use crate::id::{CardInstance, PermanentId, PlayerId};
use crate::state::{CounterKind, GameState};
use crate::token::{Printed, PrintedFace, TokenData};
use crate::CardDatabase;

/// The object a battlefield entry is about: a card, or a token (CR 111).
///
/// The entry-time counterpart of [`Printed`], and it carries one thing `Printed` cannot:
/// *which physical copy*. A card entering the battlefield is a specific
/// [`CardInstance`] that has to survive the entry — and, when the entry is replaced, has
/// to be put somewhere else instead — while a token has no card at all and simply
/// ceases to exist if it never arrives (CR 111.7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EnteringObject {
    /// A card (CR 110.1): the physical copy that will become the permanent.
    Card(CardInstance),
    /// A token (CR 111): no card, characteristics carried on the object itself. Boxed
    /// for the reason [`Printed::Token`] is — a [`PendingEntry`] is cloned with the
    /// state on every action.
    Token(Box<TokenData>),
}

impl EnteringObject {
    /// The physical card entering, or `None` for a token.
    ///
    /// The seam CR 111.7 sits at on this side of the entry, exactly as
    /// [`Printed::card`] is on the other: a replacement that puts the object somewhere
    /// other than the battlefield gets a card to put there only when there is one.
    #[must_use]
    pub fn card(&self) -> Option<CardInstance> {
        match self {
            Self::Card(card) => Some(*card),
            Self::Token(_) => None,
        }
    }

    /// Whether this is a token (CR 111) — the `nontoken` wording of a printed
    /// replacement, asked as a predicate rather than inferred from an absence.
    #[must_use]
    pub fn is_token(&self) -> bool {
        matches!(self, Self::Token(_))
    }

    /// The printed characteristics of whatever is entering, borrowed from whichever
    /// source has them. `None` only for a card handle the database does not know.
    #[must_use]
    pub fn face<'a>(&'a self, db: &'a CardDatabase) -> Option<PrintedFace<'a>> {
        match self {
            Self::Card(card) => db.card(card.card).map(PrintedFace::Card),
            Self::Token(token) => Some(PrintedFace::Token(token)),
        }
    }

    /// What the permanent's [`Printed`] will be once it arrives.
    #[must_use]
    pub fn printed(&self) -> Printed {
        match self {
            Self::Card(card) => Printed::Card(card.card),
            Self::Token(token) => Printed::Token(token.clone()),
        }
    }
}

/// A permanent's arrival on the battlefield, described **before it happens** — the
/// event the replacement layer matches against and rewrites (CR 614.1a).
///
/// Every argument the entry seams were called with, kept together so a deferred entry is
/// the *same* entry rather than a reconstruction of one: an effect's own "onto the
/// battlefield tapped", an Aura's chosen host, and a token's attacking declaration are
/// all decided before any replacement is consulted and must survive the consultation.
///
/// While one of these is on the choice queue the object is in **no zone at all** — the
/// same place a spell's card waits while its resolution is suspended. That is what makes
/// "the permanent is never briefly on the battlefield in a state a replacement was going
/// to change" true by construction rather than by ordering.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingEntry {
    /// What is entering.
    pub object: EnteringObject,
    /// The player it enters under — and, being the affected object's controller, the
    /// one who orders the applicable replacements (CR 616.1).
    pub controller: PlayerId,
    /// Whether it would arrive tapped. Seeded by whatever the *effect* putting it there
    /// said ("create a tapped 2/2"), then modified by an "enters tapped" replacement.
    pub tapped: bool,
    /// What it would arrive attacking (CR 506.3c — a token created attacking), `None`
    /// for every ordinary entry.
    pub attacking: Option<AttackTarget>,
    /// The host an entering Aura was cast at (CR 303.4d), `None` for everything else.
    pub attached_to: Option<PermanentId>,
    /// The counters it would arrive with, accumulated by the "enters with counters"
    /// replacements that have applied so far. A planeswalker's loyalty is **not** here:
    /// CR 306.5b is a rule about every planeswalker rather than a replacement effect
    /// anyone could order, and it is applied where the permanent is built.
    pub counters: Vec<(CounterKind, u32)>,
    /// Whether the object is entering because a **spell was cast** and resolved
    /// (CR 608.3), rather than being put there by an effect or created as a token.
    ///
    /// The one fact about an entry that cannot be recovered from the object: a creature
    /// card reanimated out of a graveyard and the same card cast from hand produce
    /// identical permanents, and a printed replacement that says `without being cast`
    /// distinguishes exactly those two. Set at the one seam that resolves a permanent
    /// spell and nowhere else.
    pub cast: bool,
    /// The replacements already applied to this event (CR 614.5), so none applies
    /// twice. This is what terminates the loop: a modification that leaves the entry
    /// still matching the effect that made it would otherwise be applied forever.
    pub applied: Vec<ReplacementOption>,
}

/// One replacement effect that could apply to an event, named by where it comes from.
///
/// A handle rather than the effect itself, for the reason a choice never snapshots its
/// candidates (ADR 0013): the applicable set is recomputed from the state and the event
/// on every read, and this is what an answer names within that freshly computed list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplacementOption {
    /// A **self-replacement** printed on the entering object itself (CR 614.1c), named
    /// by its index among that object's abilities.
    SelfReplacement(usize),
    /// A one-shot replacement an ability **created** for the turn, named by its
    /// [`PendingReplacement::id`] — which is minted from the same monotonic counter
    /// every other object uses, so it is stable across the suspension.
    Created(u64),
}

/// A one-shot replacement effect an ability created, waiting for the event it modifies
/// (CR 614.1b) — the `the next time a … would … this turn, … instead` of a printed card.
///
/// The shape of [`GraveyardCasting`](crate::GraveyardCasting) and for the same reasons:
/// raw stored state nothing else in [`GameState`] could recover (ADR 0005 §1), kept as a
/// list because two can be in force at once, and carrying the [`turn`](Self::turn) it
/// was created on rather than a duration to tick down.
///
/// It is **one-shot**: applying it removes it, because that is what "the next time"
/// means. Nothing here can outlive its turn either — the turn boundary clears the list
/// — so the two halves of `the next time … this turn` are one fact each rather than a
/// duration vocabulary nothing yet needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingReplacement {
    /// Identity, minted from [`GameState::mint_id`](crate::GameState) — stable across a
    /// suspension, which is what lets an ordering answer name it.
    pub id: u64,
    /// The player who controls it (the creating ability's controller). Deliberately
    /// **not** the seat that orders it: CR 616.1 gives that decision to the affected
    /// object's controller, which is frequently an opponent.
    pub controller: PlayerId,
    /// What it watches, and what happens instead.
    pub effect: ReplacementEffect,
    /// The turn it was created on; it lapses when that turn ends.
    pub turn: u32,
}

/// What a replacement effect watches for, and what happens instead (CR 614.1a).
///
/// A closed, plain-data vocabulary authored as card data, exactly as
/// [`Effect`](crate::Effect) is. It grows by adding variants: each one names an event it
/// can modify and the modification, and the collector and the ordering choice below
/// apply to all of them without change.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReplacementEffect {
    /// A permanent that **would enter the battlefield is exiled instead**.
    ///
    /// The event is cancelled outright rather than modified: nothing enters, so no
    /// enters-the-battlefield trigger is collected, no state-based action sees the
    /// object, and every replacement still applicable to that entry stops applying —
    /// there is no longer an entry for it to modify. A card goes to its owner's exile;
    /// a token simply ceases to exist (CR 111.7), having never been anywhere.
    ///
    /// Authored as
    /// `{"kind":"exile_entering","entering":{"card_type":"creature","nontoken":true,"not_cast":true}}`.
    ExileEntering {
        /// Which entries it applies to. An absent filter applies to every one.
        #[serde(default)]
        entering: EnteringFilter,
    },
}

/// Which battlefield entries a replacement effect applies to.
///
/// A small product of independent filters rather than a closed list of named classes,
/// for the reason [`PermanentCount`](crate::ability::PermanentCount) is one: the classes
/// printed replacements name are an open-ended combination of a type, token-ness, and
/// how the object got there, and enumerating each combination as its own variant would
/// grow the vocabulary once per card. Every field defaults to "no restriction", so an
/// omitted filter matches everything.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct EnteringFilter {
    /// Restrict to entering permanents with this printed card type. Absent matches
    /// every type.
    #[serde(default)]
    pub card_type: Option<CardType>,
    /// Restrict to entering permanents that are **not tokens** (CR 111) — the
    /// `nontoken creature` of a printed replacement. `false` matches both.
    #[serde(default)]
    pub nontoken: bool,
    /// Restrict to entering permanents that got there **without being cast** — a
    /// reanimation, a token, a search that put its find onto the battlefield. `false`
    /// matches both.
    #[serde(default)]
    pub not_cast: bool,
}

impl EnteringFilter {
    /// Whether `entry` is the kind of arrival this filter names.
    #[must_use]
    fn matches(&self, entry: &PendingEntry, db: &CardDatabase) -> bool {
        if self.nontoken && entry.object.is_token() {
            return false;
        }
        if self.not_cast && entry.cast {
            return false;
        }
        match self.card_type {
            // Printed types, consistent with the rest of the engine: the layer that
            // would change an object's type is not implemented, and an object that is
            // not on the battlefield has no computed characteristics to read anyway.
            Some(wanted) => entry
                .object
                .face(db)
                .is_some_and(|face| face.has_type(wanted)),
            None => true,
        }
    }
}

/// What applying one replacement did to the event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EntryOutcome {
    /// The entry is modified and still happening; the layer looks for what else applies.
    Continues,
    /// The entry was replaced by something else entirely and no permanent will arrive.
    /// Every other applicable replacement stops applying with it — there is no event
    /// left for them to modify.
    Replaced,
}

/// Whether `ability` is a self-replacement the layer applies to an entry (CR 614.1c).
///
/// [`Ability::EntersChoosingColor`] is not one of these: see the module docs — it is a
/// question the entry is already deferred on, not a modification to be ordered.
fn is_entry_self_replacement(ability: &Ability) -> bool {
    matches!(
        ability,
        Ability::EntersTapped | Ability::EntersWithCounters { .. }
    )
}

/// The abilities of the object that is entering.
///
/// The pre-battlefield counterpart of
/// [`abilities_of_permanent`](crate::abilities_of_permanent), and it deliberately does
/// **not** take the state: the object is not on the battlefield and carries no id yet,
/// so no continuous effect can be keyed to it and the CR 613 layer-6 gate that accessor
/// applies is inert here by construction.
fn entering_abilities(db: &CardDatabase, object: &EnteringObject) -> Vec<Ability> {
    match object {
        EnteringObject::Card(card) => crate::card::abilities_of(db, card.card),
        EnteringObject::Token(token) => token.abilities.clone(),
    }
}

/// Every replacement effect that applies to `entry` right now and has not already been
/// applied to it (CR 614.5), in a stable order.
///
/// Recomputed from the state and the event on every call — the counterpart of
/// [`choice_candidates`](crate::choice_candidates) for a replacement — so an ordering
/// answer is validated against the set that exists *now*. It can afford to be: while the
/// ordering question is owed nothing else in the game is legal, so neither list under it
/// can move.
///
/// The order is the object's own self-replacements first, in printed order, then the
/// created ones in the order they were created. Nothing about that order decides
/// anything — it is what the affected player is offered and indexes into, and CR 616.1
/// gives the decision to them.
#[must_use]
pub(crate) fn applicable_to_entry(
    state: &GameState,
    db: &CardDatabase,
    entry: &PendingEntry,
) -> Vec<ReplacementOption> {
    let mut options = Vec::new();
    for (index, ability) in entering_abilities(db, &entry.object).iter().enumerate() {
        let option = ReplacementOption::SelfReplacement(index);
        if is_entry_self_replacement(ability) && !entry.applied.contains(&option) {
            options.push(option);
        }
    }
    for replacement in &state.replacements {
        let option = ReplacementOption::Created(replacement.id);
        let applies = match &replacement.effect {
            ReplacementEffect::ExileEntering { entering } => entering.matches(entry, db),
        };
        if applies && !entry.applied.contains(&option) {
            options.push(option);
        }
    }
    options
}

/// The replacements the pending CR 616.1 ordering question offers, in the order an
/// answer indexes into — described well enough for a projection to name them.
///
/// The public read behind the prompt: a client is shown one option per applicable
/// replacement and answers with its position, so this and
/// [`applicable_to_entry`] must walk the same list, which they do by construction.
/// Empty when the game is not waiting on an ordering question.
#[must_use]
pub fn pending_replacement_options(
    state: &GameState,
    db: &CardDatabase,
) -> Vec<OfferedReplacement> {
    let Some(request) = crate::choice::pending_player_choice(state)
        .and_then(|pending| pending.question.replacement())
    else {
        return Vec::new();
    };
    let abilities = entering_abilities(db, &request.entry.object);
    applicable_to_entry(state, db, &request.entry)
        .into_iter()
        .filter_map(|option| match option {
            ReplacementOption::SelfReplacement(index) => abilities
                .get(index)
                .cloned()
                .map(OfferedReplacement::SelfReplacement),
            ReplacementOption::Created(id) => state
                .replacements
                .iter()
                .find(|replacement| replacement.id == id)
                .map(|replacement| OfferedReplacement::Created(replacement.effect.clone())),
        })
        .collect()
}

/// One option of the CR 616.1 ordering question, resolved to the thing it would do.
///
/// Two variants because the two source lists speak two vocabularies: a self-replacement
/// is one of the entering object's [`Ability`] values, and a created one is a
/// [`ReplacementEffect`]. A projection renders both; nothing in the engine reads this.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OfferedReplacement {
    /// A self-replacement printed on the entering object (CR 614.1c).
    SelfReplacement(Ability),
    /// A one-shot replacement an ability created ([`PendingReplacement`]).
    Created(ReplacementEffect),
}

/// Apply one replacement to `entry`, and say whether the entry survived it.
///
/// The caller has already established that `option` is applicable; this writes and
/// decides nothing. Every path records the option on [`PendingEntry::applied`] so
/// CR 614.5 holds even for a modification that leaves the event still matching.
pub(crate) fn apply_to_entry(
    state: &mut GameState,
    entry: &mut PendingEntry,
    option: ReplacementOption,
    db: &CardDatabase,
) -> EntryOutcome {
    entry.applied.push(option);
    match option {
        ReplacementOption::SelfReplacement(index) => {
            let ability = entering_abilities(db, &entry.object).into_iter().nth(index);
            match ability {
                Some(Ability::EntersTapped) => entry.tapped = true,
                Some(Ability::EntersWithCounters { counter, count }) => {
                    entry.counters.push((counter, count));
                }
                // Applicability was established from the same list; anything else here
                // would be a replacement that is not one, and changing nothing is the
                // right amount of damage for that.
                _ => {}
            }
            EntryOutcome::Continues
        }
        ReplacementOption::Created(id) => {
            // One-shot: "the next time" is spent by applying it, whatever it does.
            let Some(position) = state.replacements.iter().position(|r| r.id == id) else {
                return EntryOutcome::Continues;
            };
            let replacement = state.replacements.remove(position);
            match replacement.effect {
                ReplacementEffect::ExileEntering { .. } => {
                    exile_entering(state, entry);
                    EntryOutcome::Replaced
                }
            }
        }
    }
}

/// Put the object that would have entered into its owner's exile instead.
///
/// A **token** is put nowhere: it never existed anywhere but the battlefield it is not
/// going to reach, so CR 111.7 leaves nothing to move — the same `None` every other
/// battlefield-departure seam reads, arriving from the other side of the entry.
///
/// It records no log event, because the log has no way to say this yet: the public
/// wire vocabulary (`sage_protocol::GameLogEvent`) has no entry for a replaced event, and
/// a fact recorded in the engine that the projection silently drops is worse than one
/// that is not recorded at all. The exile zone itself is public, so the card is visible
/// where it landed.
fn exile_entering(state: &mut GameState, entry: &PendingEntry) {
    if let (Some(card), Some(owner)) = (
        entry.object.card(),
        state.players.get_mut(entry.controller.0),
    ) {
        owner.exile.push(card);
    }
}

// ----- the damage event (CR 615) ---------------------------------------------

/// Who or what damage would be dealt to (CR 120.3): a player, or a permanent.
///
/// The recipient half of [`PendingDamage`], and deliberately not
/// [`DamageTarget`](crate::DamageTarget): that one is the *log's* vocabulary and carries
/// a permanent's rendered identity, which is a fact about an object that has already
/// been dealt damage. This one names an object the damage has not reached yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DamageRecipient {
    /// A player, who would lose that much life (CR 120.3a).
    Player(PlayerId),
    /// A permanent, which would have that much damage marked on it (CR 120.3d) — or,
    /// for a planeswalker, that much loyalty removed (CR 120.3c).
    Permanent(PermanentId),
}

/// Damage described **before it is dealt** — the event a prevention shield is consulted
/// about (CR 615.1), and the counterpart of [`PendingEntry`] for the second replaceable
/// event.
///
/// It carries the one fact about damage that cannot be recovered from the recipient:
/// whether it is **combat** damage (CR 510.1). A blocked creature's 3 and a burn spell's
/// 3 are the same 3 by the time either is marked, and `Prevent all combat damage that
/// would be dealt this turn` distinguishes exactly those two. So the caller that knows
/// says so here, and every road funnels into the one seam that asks
/// ([`GameState::deal_damage`](crate::GameState)).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PendingDamage {
    /// What would take the damage.
    pub recipient: DamageRecipient,
    /// How much would be dealt, before any of it is prevented.
    pub amount: u32,
    /// Whether this is combat damage (CR 510.1) — dealt by a creature in the
    /// combat-damage step, which includes a trampler's excess and a blocker's swing
    /// back. Damage from a spell, an ability, or a fight (CR 701.12) is not.
    pub combat: bool,
    /// Whether **no prevention shield may apply** to this damage (CR 615.1) — the
    /// `the damage can't be prevented` a spell declares about its own damage.
    ///
    /// A fact about the damage rather than about the shield, and carried here for
    /// exactly the reason [`Self::combat`] is: by the time the amount is being marked,
    /// nothing about the recipient could tell you where it came from. The resolving
    /// object says so as it describes the event, and the one seam that consults shields
    /// reads it.
    ///
    /// `false` everywhere but the handful of spells that print the clause, so a game
    /// with none behaves byte-for-byte as it did before this field existed.
    pub unpreventable: bool,
}

impl PendingDamage {
    /// Non-combat damage to a player (CR 120.3a) — a burn spell, an ability, a drain.
    #[must_use]
    pub fn to_player(player: PlayerId, amount: u32) -> Self {
        Self {
            recipient: DamageRecipient::Player(player),
            amount,
            combat: false,
            unpreventable: false,
        }
    }

    /// Non-combat damage to a permanent (CR 120.3c/d) — a burn spell, an ability, or
    /// the damage a fight deals (CR 701.12).
    #[must_use]
    pub fn to_permanent(permanent: PermanentId, amount: u32) -> Self {
        Self {
            recipient: DamageRecipient::Permanent(permanent),
            amount,
            combat: false,
            unpreventable: false,
        }
    }

    /// The same damage, dealt in the combat-damage step (CR 510.1). Written as a
    /// modifier on the recipient rather than as a second pair of constructors, because
    /// combat-ness is the *only* thing the two roads disagree about.
    #[must_use]
    pub fn in_combat(mut self) -> Self {
        self.combat = true;
        self
    }

    /// The same damage, declared unpreventable by the object dealing it (CR 615.1).
    ///
    /// Written as a modifier for [`Self::in_combat`]'s reason: it is one more thing the
    /// caller knows and the recipient cannot be asked, and neither constructor should
    /// grow a second pair for it.
    #[must_use]
    pub fn unpreventable(mut self) -> Self {
        self.unpreventable = true;
        self
    }
}

/// Which damage events a prevention shield applies to.
///
/// The [`EnteringFilter`] of the damage event, and the same shape for the same reason:
/// a product of independent restrictions, each defaulting to "no restriction", so an
/// omitted filter prevents every damage event there is.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct DamageFilter {
    /// Restrict to **combat** damage (CR 510.1) — Root Snare's `all combat damage`.
    /// `false` covers damage from every source, combat or not.
    #[serde(default)]
    pub combat_only: bool,
}

impl DamageFilter {
    /// Whether `damage` is the kind of damage event this shield prevents.
    #[must_use]
    pub(crate) fn matches(&self, damage: &PendingDamage) -> bool {
        !self.combat_only || damage.combat
    }
}

/// How much of `damage` is actually dealt once every prevention shield in force has been
/// applied to it (CR 615.1).
///
/// One applicable shield is the whole answer: every shield a card can print today
/// prevents *all* of the damage it applies to, so a second one has nothing left to
/// prevent and the CR 616.1 ordering question — which a partial shield would make
/// observable — has no answer that could differ. The moment a shield prevents an amount
/// rather than everything, this becomes a fold and that question joins the choice queue.
#[must_use]
pub(crate) fn after_prevention(state: &GameState, damage: &PendingDamage) -> u32 {
    // CR 615.1: damage that can't be prevented defeats every shield at once, however
    // many there are and whatever each of them says. Checked ahead of the shields rather
    // than inside `DamageFilter::matches`, because it is a property of the damage and not
    // a restriction on the shield — a filter that had to know about it would be the
    // wrong half of the sentence answering.
    if damage.unpreventable {
        return damage.amount;
    }
    if state.prevention.iter().any(|shield| shield.matches(damage)) {
        0
    } else {
        damage.amount
    }
}
