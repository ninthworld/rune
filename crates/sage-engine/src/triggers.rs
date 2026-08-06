//! Diff-based trigger collection.
//!
//! Triggers are discovered by comparing the state before and after an action —
//! never via listeners or observers (crate `AGENTS.md`). [`crate::apply_action`]
//! calls [`collect_triggers`] and puts each resulting [`Trigger`] on the stack.

use crate::ability::{
    Ability, ActivatorScope, ObservedActivation, ObservedPermanent, TriggerCondition, TurnScope,
};
use crate::card::abilities_of_permanent;
use crate::card_type::CardType;
use crate::id::{CardInstanceId, PermanentId, PlayerId};
use crate::stack::{AbilityOrigin, AbilitySource, StackId, StackObject, StackObjectKind};
use crate::state::{GameEvent, GameState, Permanent};
use crate::{CardDatabase, Effect};

/// A triggered ability that a state transition has caused to trigger.
///
/// Triggers are collected by diffing the state before and after an action (see
/// [`collect_triggers`]) — never via listeners or observers (crate `AGENTS.md`).
/// A collected trigger carries everything needed to put the ability on the stack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trigger {
    /// The object whose ability triggered — a permanent, an emblem (CR 114), or a card
    /// in a graveyard whose ability functions from there (CR 113.6).
    pub source: AbilitySource,
    /// The player who controls the triggered ability (its source's controller).
    pub controller: PlayerId,
    /// The effects the ability produces when it resolves.
    pub effects: Vec<Effect>,
    /// The slots the **trigger event itself** filled, in slot order — empty for every
    /// printed triggered ability, which arrives unaimed and is aimed by its controller
    /// (CR 603.3d).
    ///
    /// A *delayed* ability is the exception, and the reason this field exists: `when you
    /// next cast an instant or sorcery spell this turn, copy **that spell**` names an
    /// object the event fixed, not one anybody chooses (CR 603.7c). Filling the slot here
    /// is what keeps that reference out of the controller's hands while still getting the
    /// CR 608.2b re-check every stack object runs — so a spell that has been countered
    /// before the ability resolves is simply not copied.
    pub targets: Vec<crate::ability::Target>,
}

/// The object a trigger condition is being evaluated for, reduced to what the
/// conditions actually read: the source permanent when there is one, and the
/// controller whose "you" and whose turn the scoped conditions mean.
///
/// An [`Emblem`](crate::Emblem) and a card in a **graveyard** are the cases that make
/// this a type rather than a bare `&Permanent`. Every self-condition
/// (`self_enters_battlefield`, `self_dies`, `self_attacks`) is about a battlefield object
/// and can never be satisfied by one that is not on the battlefield; every watching
/// condition additionally requires its source to *still be there*, which each origin
/// answers its own way. Both facts follow from [`Self::permanent`] being `None`, so
/// neither is a special case written out per condition.
#[derive(Clone, Copy)]
struct Watcher<'a> {
    /// What the ability is on, and therefore where it is watching from.
    origin: Origin<'a>,
    /// The source's controller.
    controller: PlayerId,
}

/// Where a watching ability lives — the source lists [`collect_triggers`] walks.
#[derive(Clone, Copy)]
enum Origin<'a> {
    /// A permanent on the battlefield.
    Permanent(&'a Permanent),
    /// A permanent that **left** the battlefield across this transition, read from the
    /// snapshot that still has it (CR 603.10a, last-known information).
    ///
    /// The same object as [`Self::Permanent`] for every question about what it was and
    /// what it could see; a separate variant for the one question it answers
    /// differently, which is what the ability's source *is* now that the permanent is
    /// gone — see [`Watcher::source`].
    DeadPermanent(&'a Permanent),
    /// An emblem (CR 114), by its object id.
    Emblem(u64),
    /// A card in its owner's graveyard whose ability functions from there (CR 113.6).
    GraveyardCard(crate::id::CardInstance),
}

impl<'a> Watcher<'a> {
    /// A watcher on a battlefield permanent.
    fn on(permanent: &'a Permanent, controller: PlayerId) -> Self {
        Self {
            origin: Origin::Permanent(permanent),
            controller,
        }
    }

    /// A watcher on a permanent that has left the battlefield across this transition.
    fn departed(permanent: &'a Permanent, controller: PlayerId) -> Self {
        Self {
            origin: Origin::DeadPermanent(permanent),
            controller,
        }
    }

    /// The source permanent, or `None` for the two origins that are not one. A permanent
    /// that has just left is still one — that is exactly what a dies trigger is about.
    fn permanent(self) -> Option<&'a Permanent> {
        match self.origin {
            Origin::Permanent(perm) | Origin::DeadPermanent(perm) => Some(perm),
            Origin::Emblem(_) | Origin::GraveyardCard(_) => None,
        }
    }

    /// Whether the source is still there to trigger after the transition.
    ///
    /// A permanent must still be on the battlefield — an ability that has left is not
    /// watching the board it is no longer on. A graveyard card must still be in the
    /// graveyard it was watching from, for exactly the same reason and by exactly the
    /// same test, one zone over. An emblem always is: nothing removes one (CR 114.5), so
    /// the question has one answer and it is `true`.
    fn still_present(self, after: &GameState) -> bool {
        match self.origin {
            Origin::Permanent(perm) => after.battlefield.iter().any(|p| p.id == perm.id),
            // By construction it is not: the departure pass is the only thing that
            // builds one.
            Origin::DeadPermanent(_) => false,
            Origin::Emblem(_) => true,
            Origin::GraveyardCard(card) => after
                .players
                .get(self.controller.0)
                .is_some_and(|player| player.graveyard.iter().any(|c| c.id == card.id)),
        }
    }

    /// This watcher as the [`AbilitySource`] a collected trigger records.
    ///
    /// A permanent that has left records **both** halves of what it is now
    /// ([`AbilitySource::DeadPermanent`]): the id it had, for every effect that asks
    /// about the permanent, and the card it became, for the one that acts on the card in
    /// the graveyard. A **token** records only the id, because CR 111.7 leaves no card
    /// behind for the second half to name.
    fn source(self) -> AbilitySource {
        match self.origin {
            Origin::Permanent(perm) => AbilitySource::Permanent(perm.id),
            Origin::DeadPermanent(perm) => match perm.printed.card() {
                Some(card) => AbilitySource::DeadPermanent {
                    permanent: perm.id,
                    card: crate::id::CardInstance {
                        id: perm.instance,
                        card,
                    },
                },
                None => AbilitySource::Permanent(perm.id),
            },
            Origin::Emblem(id) => AbilitySource::Emblem(id),
            Origin::GraveyardCard(card) => AbilitySource::GraveyardCard(card),
        }
    }

    /// Whether this watcher is a card in a graveyard — the origin an ability that
    /// functions from there ([`crate::is_graveyard_ability`]) belongs to, and the one
    /// every other ability does not.
    fn is_graveyard_card(self) -> bool {
        matches!(self.origin, Origin::GraveyardCard(_))
    }
}

/// Collect the triggers that should now exist by diffing `before` against
/// `after`. Each triggered ability whose condition ([`condition_met`]) holds
/// across the diff yields one [`Trigger`]. Pure, with no listeners (crate
/// `AGENTS.md`).
///
/// Two directions of zone change are observed. Enter-the-battlefield conditions
/// are checked against the permanents present in `after`; leave-the-battlefield
/// ("dies") conditions are checked against the permanents that were in `before`
/// but are gone from `after` — a dead permanent is no longer on the battlefield,
/// so its ability must be read from the *before* snapshot (last-known information,
/// CR 603.10a in spirit). A permanent that persists across the transition is visited
/// only in the first pass, where it can still match a condition about something that
/// happened *to it in place* — being declared as an attacker (CR 603.6d).
///
/// **Three source lists, not one.** Besides the battlefield, an ability may be watching
/// from an emblem (CR 114.1, in no zone) or from a **card in a graveyard** whose ability
/// functions there (CR 113.6). All three are walked here and nowhere else, and which one
/// reads a given ability is decided by the ability itself — see [`collect_from`].
///
/// **Ordering (simultaneous triggers).** Triggers are appended in the order their
/// sources are iterated: `after.battlefield` order for enters, then
/// `before.battlefield` order for deaths, then emblems, then graveyards in seat order.
/// That battlefield-position order is the
/// engine's deterministic default when several abilities trigger at once — the
/// full APNAP each-player-orders-their-own prompt (CR 603.3b / CR 101.4) is later
/// work. [`crate::apply_action`] puts the collected triggers on the stack after
/// the state-based-action loop, i.e. the next time a player would receive
/// priority (CR 603.3b), matching how ETB triggers already reach the stack.
#[must_use]
pub fn collect_triggers(before: &GameState, after: &GameState, db: &CardDatabase) -> Vec<Trigger> {
    let mut triggers = Vec::new();
    // Enter-the-battlefield direction: observe permanents present in `after`.
    for perm in &after.battlefield {
        collect_from(
            // CR 613 layer 2: an ability triggers for whoever controls its source
            // *now*, so a stolen permanent's triggers belong to the thief.
            Watcher::on(perm, crate::characteristics::controller_of(after, perm)),
            // CR 613 layer 6, read against the same snapshot: a permanent that has
            // lost all its abilities has no triggered ones either, so nothing here
            // fires for it.
            abilities_of_permanent(after, db, perm),
            before,
            after,
            db,
            &mut triggers,
        );
    }
    // Leave-the-battlefield ("dies") direction: observe permanents that were in
    // `before` but are gone from `after`. Iterating `before.battlefield` keeps the
    // stack order of simultaneous deaths deterministic (see the ordering note).
    for perm in &before.battlefield {
        if after.battlefield.iter().any(|p| p.id == perm.id) {
            continue;
        }
        collect_from(
            // Read against `before`, the snapshot the permanent still exists in — and
            // therefore the one whose control-changing effects still apply to it. A
            // creature that dies while stolen dies under the thief's control.
            Watcher::departed(perm, crate::characteristics::controller_of(before, perm)),
            // Read against `before` for the same reason the controller is: it is the
            // snapshot the permanent still exists in, and therefore the one whose
            // layer-6 effects still silenced it as it died.
            abilities_of_permanent(before, db, perm),
            before,
            after,
            db,
            &mut triggers,
        );
    }
    // The second source list (CR 114.1): an emblem's triggered abilities. Read from
    // `before`, not `after`, which is the whole of "an ability only triggers for events
    // that happen after its source exists" (CR 603.6) — an emblem created by this very
    // transition must not fire on an end step the same transition already crossed.
    // Ordered after the battlefield so simultaneous triggers keep a deterministic stack
    // order, the same guarantee the two passes above give.
    for emblem in &before.emblems {
        collect_from(
            Watcher {
                origin: Origin::Emblem(emblem.id),
                controller: emblem.controller,
            },
            emblem.abilities.clone(),
            before,
            after,
            db,
            &mut triggers,
        );
    }
    // The third source list (CR 113.6): a card in a graveyard whose triggered ability
    // functions from there — Spit Flame watching for a Dragon while it waits in the pile.
    // Read from `before` for the emblem's reason, and controlled by the seat whose
    // graveyard it is: a card in a zone has no controller of its own (CR 108.4), so the
    // "you" of `a Dragon you control` is its owner.
    //
    // Ordered last, after both battlefield passes and the emblems, so simultaneous
    // triggers keep a deterministic stack order.
    for (seat, player) in before.players.iter().enumerate() {
        for &card in &player.graveyard {
            collect_from(
                Watcher {
                    origin: Origin::GraveyardCard(card),
                    controller: PlayerId(seat),
                },
                crate::card::abilities_of(db, card.card),
                before,
                after,
                db,
                &mut triggers,
            );
        }
    }
    triggers
}

/// The topmost triggered ability on the stack that is still owed targets
/// (CR 603.3d), or `None` when nothing is owed.
///
/// **Derived, never stored.** A triggered ability reaches the stack with no targets
/// (its controller has not chosen yet), so "owes targets" is exactly *declares more
/// target slots than it carries* — a fact about the object, readable at any time,
/// with no flag to set or clear. Spells and activated abilities are never in this
/// state: both choose their targets as part of the action that put them there
/// (CR 601.2c), so they always arrive full.
///
/// Topmost first, because triggers are chosen for in the order they will be answered
/// and the last one put on the stack is the first a player sees.
#[must_use]
pub fn pending_trigger_target_choice(state: &GameState) -> Option<StackId> {
    state
        .stack
        .iter()
        .rev()
        .find(|object| match &object.kind {
            // "Owes targets" is *carries fewer than its groups demand*. Reading the
            // groups' minimums rather than counting slots is what keeps an "up to N"
            // group — which is satisfied by nothing at all — from leaving a trigger
            // permanently unaimed and the game frozen on a question with no answer.
            StackObjectKind::Ability { effects, .. } => {
                crate::ability::minimum_targets(effects) > object.targets.len()
            }
            // A **copy of a spell** whose controller may choose new targets (CR 707.10c)
            // is in exactly the same position as an unaimed trigger: it is on the stack,
            // it declares slots, and nobody has filled them. So it is answered by the same
            // action rather than by a second aiming mechanism.
            //
            // "Owes targets" is still derived rather than flagged: the copy carries
            // *whether re-aiming was offered* — a permission nothing else in the state
            // records — and is owed an answer exactly while it has none. The seam that
            // creates it only offers re-aiming for a spell with at least one **required**
            // slot every one of which has a legal candidate, so an answer always exists
            // and always fills this.
            StackObjectKind::SpellCopy { new_targets, .. } => {
                *new_targets && object.targets.is_empty()
            }
            StackObjectKind::Spell { .. } => false,
        })
        .map(|object| object.id)
}

/// The controller of the stack object `id` — the player who chooses its targets.
#[must_use]
pub(crate) fn controller_of_stack_object(state: &GameState, id: StackId) -> Option<PlayerId> {
    state
        .stack
        .iter()
        .find(|o| o.id == id)
        .map(|o| o.controller)
}

/// Push a [`Trigger`] for every triggered ability in `abilities` whose condition holds
/// across the diff, for the object `watcher` describes.
///
/// The source object is read from whichever snapshot still has it (the `after`
/// battlefield for enters, the `before` battlefield for deaths, and `before` for an
/// emblem, which is in neither, and for a card in a graveyard).
///
/// **Where an ability functions decides which pass reads it** (CR 113.6): an ability that
/// returns its own card from a graveyard ([`crate::is_graveyard_ability`]) fires only from
/// the graveyard pass, and every other ability fires only from the others. One comparison
/// rather than a rule written twice, so a permanent card whose trigger works in a
/// graveyard can never fire from both.
fn collect_from(
    watcher: Watcher<'_>,
    abilities: Vec<Ability>,
    before: &GameState,
    after: &GameState,
    db: &CardDatabase,
    out: &mut Vec<Trigger>,
) {
    for ability in abilities {
        if crate::ability::is_graveyard_ability(&ability) != watcher.is_graveyard_card() {
            continue;
        }
        if let Ability::Triggered { event, effects } = ability {
            // A condition reports *how many times* it was met, not whether: an ability
            // watching the rest of the board sees one event per qualifying object, and
            // two creatures dying at once must trigger it twice (CR 603.2). The
            // self-conditions can only ever answer 0 or 1.
            for _ in 0..fire_count(&event, watcher, before, after, db) {
                out.push(Trigger {
                    source: watcher.source(),
                    controller: watcher.controller,
                    effects: effects.clone(),
                    // A printed trigger names nothing yet: its controller aims it once it
                    // is on the stack (CR 603.3d).
                    targets: Vec::new(),
                });
            }
        }
    }
}

/// The permanents that entered the battlefield across the diff: present in `after`,
/// absent from `before`.
fn entered<'a>(before: &GameState, after: &'a GameState) -> impl Iterator<Item = &'a Permanent> {
    let ids: Vec<PermanentId> = before.battlefield.iter().map(|p| p.id).collect();
    after
        .battlefield
        .iter()
        .filter(move |p| !ids.contains(&p.id))
}

/// The permanents that **died** across the diff — the [`TriggerCondition::SelfDies`]
/// test applied to the whole board, so "died" means exactly the same thing whether an
/// ability is watching itself or its neighbours.
fn died<'a>(before: &'a GameState, after: &GameState) -> Vec<&'a Permanent> {
    before
        .battlefield
        .iter()
        .filter(|p| !after.battlefield.iter().any(|q| q.id == p.id) && death_of(p, before, after))
        .collect()
}

/// The permanents **declared as attackers** across the diff: attacking in `after` and
/// not attacking in `before`.
///
/// The [`TriggerCondition::SelfAttacks`] test applied to the whole board, reading the
/// one field a declaration writes, so "attacks" means exactly the same thing whether an
/// ability watches itself or its neighbours: a creature that was already attacking when
/// the transition began has not attacked again, and one that merely became tapped has
/// not attacked at all.
fn attacked<'a>(before: &GameState, after: &'a GameState) -> impl Iterator<Item = &'a Permanent> {
    let already: Vec<PermanentId> = before
        .battlefield
        .iter()
        .filter(|p| p.attacking.is_some())
        .map(|p| p.id)
        .collect();
    after
        .battlefield
        .iter()
        .filter(move |p| p.attacking.is_some() && !already.contains(&p.id))
}

/// The objects **put on the stack** by this transition: present in `after`, absent from
/// `before`. A [`StackId`] is minted once and never reused, so its presence is the whole
/// test.
fn pushed_onto_stack<'a>(
    before: &GameState,
    after: &'a GameState,
) -> impl Iterator<Item = &'a StackObject> {
    let ids: Vec<StackId> = before.stack.iter().map(|object| object.id).collect();
    after
        .stack
        .iter()
        .filter(move |object| !ids.contains(&object.id))
}

/// Whether the stack object `object` is an **activation** that `observes` notices, for
/// an ability on `source`.
///
/// Three questions, in the order that makes the cheapest one first: was this object put
/// there by a player activating an ability of a permanent (CR 602.2 — an
/// [`AbilityOrigin::Activated`] push, which a triggered ability and a cast spell are
/// not), was that player one this selector watches, and is the permanent whose ability
/// it was one of the named types.
///
/// The activating permanent is read from `before`, the state it was activated in: an
/// activation is announced from the battlefield, and a cost paid by sacrificing the
/// source (CR 701.17) leaves nothing in `after` to ask.
fn observed_activation_matches(
    observes: &ObservedActivation,
    object: &StackObject,
    source: Watcher<'_>,
    before: &GameState,
    db: &CardDatabase,
) -> bool {
    let StackObjectKind::Ability {
        source: AbilitySource::Permanent(activated),
        origin: AbilityOrigin::Activated,
        ..
    } = &object.kind
    else {
        return false;
    };
    match observes.activator {
        ActivatorScope::Any => {}
        ActivatorScope::Opponents => {
            if object.controller == source.controller {
                return false;
            }
        }
    }
    if observes.source_types.is_empty() {
        return true;
    }
    let Some(perm) = before.battlefield.iter().find(|p| p.id == *activated) else {
        return false;
    };
    let Some(face) = perm.printed.face(db) else {
        return false;
    };
    // "A creature **or** land": any one of the named types satisfies it.
    observes
        .source_types
        .iter()
        .any(|&card_type| face.has_type(card_type))
}

/// Whether the permanent `perm`, which has left the battlefield across this
/// transition, left it by **dying** (CR 700.4) rather than by being bounced, exiled,
/// or removed with its controller.
///
/// A card answers this from the board alone: its physical instance is in a graveyard
/// now and was not before, which is what makes a leave to any other zone not a death.
/// A **token** cannot answer it that way, because a token that dies reaches no
/// graveyard to be found in (CR 111.7) — the death is real, the destination is not.
/// Its evidence is the [`GameEvent::PermanentDied`] the one death seam
/// ([`GameState::destroy_permanent`](crate::GameState::destroy_permanent)) records,
/// read from the same event diff the life-gain and cast conditions already use
/// (ADR 0007). So a dies trigger fires on a token exactly as it does on a card
/// (CR 603.6c: the ability triggers on the way out, before the object ceases to
/// exist), while a bounced or exiled token — which records no such event — fires
/// nothing.
fn death_of(perm: &Permanent, before: &GameState, after: &GameState) -> bool {
    if perm.printed.is_token() {
        return events_in(before, after).any(|event| {
            matches!(event, GameEvent::PermanentDied { permanent }
                if permanent.permanent == perm.id)
        });
    }
    in_graveyard(after, perm.instance) && !in_graveyard(before, perm.instance)
}

/// The events recorded **by this transition** — the entries `after` has that `before`
/// did not.
///
/// This is still a diff, over the one part of the state that records *what happened*
/// rather than what is (ADR 0007). Some conditions are about events, not about board
/// positions, and cannot be recovered from a snapshot comparison at all: gaining three
/// life and losing three leaves every total unchanged, but a card that triggers on
/// gaining life has triggered. Reading the recorded events is what makes those
/// conditions expressible without adding a listener.
///
/// The log is a bounded window, so a transition recording more events than the window
/// holds would lose its earliest ones. A single transition records a handful.
fn events_in<'a>(before: &GameState, after: &'a GameState) -> impl Iterator<Item = &'a GameEvent> {
    let from = before.next_log_sequence;
    after
        .log
        .iter()
        .filter(move |entry| entry.sequence >= from)
        .map(|entry| &entry.event)
}

/// Whether `candidate` is one of the permanents `observes` watches, for an ability on
/// `source`. Evaluated relative to the source, exactly as [`crate::StaticAffects`] is:
/// "you" is the source's controller and "another" excludes the source itself.
///
/// `seen_in` is the state the observed event happened in — the state **after** for a
/// permanent entering, and the state **before** for one dying, since a dead permanent
/// is on neither battlefield afterwards. It exists only so a power bound can be read
/// through the computed characteristics: "a creature with power 2 or less entered" is a
/// question about the creature as it entered, and a printed reading would answer it
/// wrongly for every creature that entered with a counter or under an anthem.
fn observed_matches(
    observes: &ObservedPermanent,
    candidate: &Permanent,
    source: Watcher<'_>,
    seen_in: &GameState,
    db: &CardDatabase,
) -> bool {
    // An emblem is not a permanent, so it is never the "this" an `except_this`
    // excludes — `map` answering `None` says so without an arm of its own.
    if observes.excludes_source() && source.permanent().map(|p| p.id) == Some(candidate.id) {
        return false;
    }
    let Some(face) = candidate.printed.face(db) else {
        return false;
    };
    if !face.has_type(CardType::Creature) {
        return false;
    }
    if let Some(subtype) = observes.subtype() {
        if !face.has_subtype(subtype) {
            return false;
        }
    }
    // "Nontoken": a token's printed face is the effect that made it, not a card
    // (ADR 0015), so `Printed::card()` answering `None` is the whole of the test.
    if observes.nontoken_only() && candidate.printed.card().is_none() {
        return false;
    }
    // Computed, and only when a bound is authored. A permanent with no power satisfies
    // no bound — though the creature test above has already excluded every such case.
    if let Some(max) = observes.max_power() {
        let power = crate::characteristics::characteristics(seen_in, candidate.id, db).power;
        if power.is_none_or(|power| power > max) {
            return false;
        }
    }
    // Computed too, and for the same reason: a keyword grant is a CR 613 layer-6 effect
    // the engine implements, so the printed face is the wrong reading — a creature given
    // flying is one a flying watcher notices, and stops being one when the grant ends.
    if let Some(keyword) = observes.keyword() {
        if !crate::characteristics::characteristics(seen_in, candidate.id, db)
            .keywords
            .contains(&keyword)
        {
            return false;
        }
    }
    match observes {
        // CR 613 layer 2, read in the same snapshot the event was seen in — the state
        // the observed permanent is still on the battlefield of.
        ObservedPermanent::CreaturesYouControl { .. } => {
            crate::characteristics::controller_of(seen_in, candidate) == source.controller
        }
        ObservedPermanent::AnyCreature { .. } => true,
    }
}

/// Whether the permanent `perm` is present on `state`'s battlefield.
fn on_battlefield(state: &GameState, perm: &Permanent) -> bool {
    state.battlefield.iter().any(|p| p.id == perm.id)
}

/// How many times `condition` was met across the transition, for an ability on `perm`.
///
/// A pure function of the two snapshots — never an event listener. The self-conditions
/// answer 0 or 1 because they are about one object; a condition watching the board
/// answers once per qualifying event, which is what makes a board wipe trigger a
/// death-watcher once per creature rather than once.
fn fire_count(
    condition: &TriggerCondition,
    watcher: Watcher<'_>,
    before: &GameState,
    after: &GameState,
    db: &CardDatabase,
) -> usize {
    match condition {
        // The three *self* conditions are about a battlefield object, so an emblem —
        // which is in no zone — can never satisfy any of them. `watcher.permanent`
        // answering `None` is what says so, once, rather than three arms each carrying
        // the same caveat.
        TriggerCondition::SelfEntersBattlefield => usize::from(
            watcher
                .permanent()
                .is_some_and(|perm| on_battlefield(after, perm) && !on_battlefield(before, perm)),
        ),
        // CR 700.4 / 603.6c: the permanent died — it left the battlefield for a
        // graveyard. Observed purely by diff: its id is gone from the battlefield
        // and its physical instance is now in some graveyard where it was not
        // before. Requiring the *graveyard* destination is what stops a leave to a
        // non-graveyard zone (a future bounce or exile) from firing this.
        TriggerCondition::SelfDies => usize::from(watcher.permanent().is_some_and(|perm| {
            on_battlefield(before, perm)
                && !on_battlefield(after, perm)
                && death_of(perm, before, after)
        })),
        // CR 508.1 / 603.6d: the permanent was declared as an attacker this
        // transition. Observed by diff on the one field the declaration writes —
        // `attacking` is set after and was not before — so it fires once, from the
        // declare-attackers action, and never from a creature that was merely tapped
        // or that is still attacking from an earlier check.
        //
        // CR 506.3c: a permanent *put onto the battlefield attacking* was never
        // declared, so this does not fire for it. The diff alone cannot tell the two
        // apart — an object that was not there before has no previous `attacking`
        // either way — so the rule is stated as the extra condition it is: the
        // permanent has to have been on the battlefield to be declared from it.
        TriggerCondition::SelfAttacks => usize::from(watcher.permanent().is_some_and(|perm| {
            let attacking_in = |state: &GameState| {
                state
                    .battlefield
                    .iter()
                    .any(|p| p.id == perm.id && p.attacking.is_some())
            };
            on_battlefield(before, perm) && attacking_in(after) && !attacking_in(before)
        })),
        // The watching conditions count rather than answer. Each first asks whether its
        // source is still there to watch — a permanent must still be on the battlefield,
        // and an emblem always is (CR 114.5: nothing removes one), which
        // [`Watcher::still_present`] answers for both.
        TriggerCondition::PermanentEnters(observes) => {
            // An ability that is no longer on the battlefield is not watching it. A
            // death-watcher is the deliberate exception below: a creature that died
            // alongside another still observed that death.
            if !watcher.still_present(after) {
                return 0;
            }
            entered(before, after)
                .filter(|candidate| observed_matches(observes, candidate, watcher, after, db))
                .count()
        }
        TriggerCondition::PermanentDies(observes) => died(before, after)
            .into_iter()
            .filter(|candidate| observed_matches(observes, candidate, watcher, before, db))
            .count(),
        // CR 508.1: the watching form of `SelfAttacks`, counted over every attacker the
        // declaration produced. The selector is judged in `after` — the state the
        // declaration made — so a creature granted flying beforehand really is one a
        // flying watcher notices.
        TriggerCondition::PermanentAttacks(observes) => {
            if !watcher.still_present(after) {
                return 0;
            }
            attacked(before, after)
                .filter(|candidate| observed_matches(observes, candidate, watcher, after, db))
                .count()
        }
        TriggerCondition::YouGainLife => {
            if !watcher.still_present(after) {
                return 0;
            }
            // A *gain*, not a net change: the recorded delta is signed, and only a
            // positive one is life gained (CR 118.3). Damage to a player is recorded as
            // damage rather than as a life change, so it never reaches here.
            events_in(before, after)
                .filter(|event| {
                    matches!(event, GameEvent::LifeChanged { player, amount }
                        if *player == watcher.controller && *amount > 0)
                })
                .count()
        }
        // CR 603.6a: the ability triggers as the step begins. Counted over the
        // `StepChanged` entries this transition recorded rather than by comparing
        // `before.step` with `after.step` — one pass of priority can walk through
        // several steps at once, and a step recurs every turn, so the snapshot
        // comparison both misses crossings and fires on transitions that crossed
        // nothing. One event per crossing makes this exactly once per crossing.
        TriggerCondition::BeginningOfStep { step, whose_turn } => {
            if !watcher.still_present(after) {
                return 0;
            }
            let watched = step.step();
            events_in(before, after)
                .filter(|event| {
                    matches!(event, GameEvent::StepChanged { step, active_player, .. }
                    if *step == watched
                        && match whose_turn {
                            TurnScope::Yours => *active_player == watcher.controller,
                            TurnScope::Each => true,
                        })
                })
                .count()
        }
        // CR 121.1: the controller drew. Counted over the cards the recorded draw
        // events say actually moved, which is the only reading that survives a
        // draw-and-discard (the hand is the size it was) and an empty library (nothing
        // was drawn, and the game is being lost instead). Each card is its own event as
        // far as a watcher is concerned (CR 121.2), so the counts are summed rather
        // than the events counted.
        TriggerCondition::YouDrawCard => {
            if !watcher.still_present(after) {
                return 0;
            }
            events_in(before, after)
                .filter_map(|event| match event {
                    GameEvent::CardsDrawn { player, count } if *player == watcher.controller => {
                        Some(usize::try_from(*count).unwrap_or(usize::MAX))
                    }
                    _ => None,
                })
                .sum()
        }
        // CR 602.2: a player activated an ability. Observed on the stack rather than in
        // the event log, because the push is where the activation is recorded — and
        // because a mana ability never reaches the stack at all (CR 605.3a), which is
        // what makes tapping a land for mana silent here without any card saying so.
        TriggerCondition::AbilityActivated(observes) => {
            if !watcher.still_present(after) {
                return 0;
            }
            pushed_onto_stack(before, after)
                .filter(|object| observed_activation_matches(observes, object, watcher, before, db))
                .count()
        }
        TriggerCondition::YouCastSpell(spell) => {
            if !watcher.still_present(after) {
                return 0;
            }
            events_in(before, after)
                .filter(|event| {
                    matches!(event, GameEvent::SpellCast { player, card }
                    if *player == watcher.controller
                        && crate::card::spell_matches_class(
                            db,
                            card.card,
                            *spell,
                            watcher.permanent().and_then(|perm| perm.chosen_color),
                        ))
                })
                .count()
        }
    }
}

/// Whether the physical card `instance` is in any player's graveyard in `state`.
fn in_graveyard(state: &GameState, instance: CardInstanceId) -> bool {
    state
        .players
        .iter()
        .any(|p| p.graveyard.iter().any(|c| c.id == instance))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::fixtures::{fixture, id_in};
    use crate::id::CardInstanceId;
    use crate::state::Permanent;

    /// The bundled card database, for tests that need oracle data.
    fn db() -> CardDatabase {
        CardDatabase::bundled().unwrap()
    }

    /// An inline catalog with a dies-draw creature. No clean M19 card carries a bare
    /// "when this dies, draw a card", so the dies-trigger tests build their own
    /// definition (ADR 0009).
    fn lurker_db() -> CardDatabase {
        let json = r#"[{"schema_version":1,"functional_id":"test_lurker","name":"Test Lurker",
            "types":["creature"],"subtypes":["Horror"],"mana_cost":"{1}{B}","colors":["black"],
            "power":2,"toughness":2,
            "abilities":[{"type":"triggered","event":"self_dies","effects":[{"kind":"draw_card","count":1}]}]}]"#;
        CardDatabase::from_json(json).unwrap()
    }

    #[test]
    fn trigger_diff_yields_nothing_for_a_plain_transition() {
        let before = GameState::new_two_player();
        let after = before.advance();
        assert!(collect_triggers(&before, &after, &db()).is_empty());
    }

    #[test]
    fn collect_triggers_detects_etb_by_permanent_id_diff() {
        let db = db();
        let before = GameState::new_two_player();
        let mut after = before.clone();
        after.battlefield.push(Permanent {
            id: PermanentId(1),
            instance: CardInstanceId(1),
            printed: fixture("skyscanner").into(),
            controller: PlayerId(0),
            tapped: false,
            entered_turn: 0,
            attacking: None,
            blocking: Vec::new(),
            skips_untap: false,
            dealt_damage: false,
            damage: 0,
            counters: Default::default(),
            attached_to: None,
            chosen_color: None,
            named_card: None,
            copied: None,
        });
        let triggers = collect_triggers(&before, &after, &db);
        assert_eq!(triggers.len(), 1);
        assert_eq!(triggers[0].source, AbilitySource::Permanent(PermanentId(1)));
        assert_eq!(triggers[0].effects, vec![Effect::DrawCard { count: 1 }]);
    }

    /// A battlefield holding the dies-draw creature (`test_lurker`) as a lone
    /// permanent under player 0. Returns the `before` state and the permanent's id
    /// and instance so a test can craft the matching `after`.
    fn before_with_lurker(db: &CardDatabase) -> (GameState, PermanentId, CardInstanceId) {
        let mut before = GameState::new_two_player();
        let instance = CardInstanceId(77);
        let id = PermanentId(1);
        before.battlefield.push(Permanent {
            id,
            instance,
            printed: id_in(db, "test_lurker").into(),
            controller: PlayerId(0),
            tapped: false,
            entered_turn: 0,
            attacking: None,
            blocking: Vec::new(),
            skips_untap: false,
            dealt_damage: false,
            damage: 0,
            counters: Default::default(),
            attached_to: None,
            chosen_color: None,
            named_card: None,
            copied: None,
        });
        (before, id, instance)
    }

    #[test]
    fn issue_151_collect_triggers_detects_a_death_by_battlefield_to_graveyard_diff() {
        // CR 700.4 / 603.6c: the permanent left the battlefield and its instance is
        // now in a graveyard — the diff observes the death and yields the dies
        // trigger, its source both halves of what the object now is (CR 603.10a): the
        // (now-gone) permanent id, and the card it became.
        let db = lurker_db();
        let (before, id, instance) = before_with_lurker(&db);
        let mut after = before.clone();
        after.battlefield.clear();
        after.players[0].graveyard.push(crate::id::CardInstance {
            id: instance,
            card: id_in(&db, "test_lurker"),
        });

        let triggers = collect_triggers(&before, &after, &db);
        assert_eq!(triggers.len(), 1);
        assert_eq!(
            triggers[0].source,
            AbilitySource::DeadPermanent {
                permanent: id,
                card: crate::id::CardInstance {
                    id: instance,
                    card: id_in(&db, "test_lurker"),
                },
            }
        );
        assert_eq!(triggers[0].source.permanent(), Some(id));
        assert_eq!(triggers[0].controller, PlayerId(0));
        assert_eq!(triggers[0].effects, vec![Effect::DrawCard { count: 1 }]);
    }

    #[test]
    fn issue_151_leaving_to_a_non_graveyard_zone_does_not_fire_dies() {
        // Future-proofing (CR 603.6c): a permanent that leaves the battlefield but
        // is *not* put into a graveyard (e.g. a bounce or a countered permanent)
        // does not satisfy `SelfDies`. The permanent is simply gone from `after`
        // with nothing in any graveyard.
        let db = lurker_db();
        let (before, _id, _instance) = before_with_lurker(&db);
        let mut after = before.clone();
        after.battlefield.clear();

        assert!(
            collect_triggers(&before, &after, &db).is_empty(),
            "a leave to a non-graveyard zone must not fire SelfDies"
        );
    }

    // ----- CR 603.6a: triggers at the beginning of a step (issue #607) -----

    /// An inline catalog of step-triggered permanents, one per scope/step pairing the
    /// vocabulary can express. No M19 card carries a bare step trigger with nothing
    /// attached to it, so these tests build their own definitions (ADR 0009).
    ///
    /// The life amounts are deliberately distinct (1/2/4/8) so a single life total
    /// says exactly which of them fired, and how often.
    fn step_db() -> CardDatabase {
        let json = r#"[
            {"schema_version":1,"functional_id":"test_your_upkeep","name":"Test Your Upkeep",
             "types":["enchantment"],"mana_cost":"{1}{W}","colors":["white"],
             "abilities":[{"type":"triggered",
                "event":{"beginning_of_step":{"step":"upkeep","whose_turn":"yours"}},
                "effects":[{"kind":"gain_life","player_ref":"controller","amount":1}]}]},
            {"schema_version":1,"functional_id":"test_each_upkeep","name":"Test Each Upkeep",
             "types":["artifact"],"mana_cost":"{1}",
             "abilities":[{"type":"triggered",
                "event":{"beginning_of_step":{"step":"upkeep","whose_turn":"each"}},
                "effects":[{"kind":"gain_life","player_ref":"controller","amount":2}]}]},
            {"schema_version":1,"functional_id":"test_each_end_step","name":"Test Each End Step",
             "types":["enchantment"],"mana_cost":"{2}{B}","colors":["black"],
             "abilities":[{"type":"triggered",
                "event":{"beginning_of_step":{"step":"end_step","whose_turn":"each"}},
                "effects":[{"kind":"gain_life","player_ref":"controller","amount":4}]}]},
            {"schema_version":1,"functional_id":"test_your_combat","name":"Test Your Combat",
             "types":["creature"],"subtypes":["Dwarf"],"mana_cost":"{3}{W}","colors":["white"],
             "power":3,"toughness":3,
             "abilities":[{"type":"triggered",
                "event":{"beginning_of_step":{"step":"begin_combat","whose_turn":"yours"}},
                "effects":[{"kind":"gain_life","player_ref":"controller","amount":8}]}]}
        ]"#;
        CardDatabase::from_json(json).unwrap()
    }

    /// A two-player state with `functional_id` on the battlefield under `controller`,
    /// plus the `after` snapshot in which the turn structure has entered `step` on
    /// `active_player`'s turn `turn` — the recorded crossing the condition reads.
    fn crossing(
        db: &CardDatabase,
        functional_id: &str,
        controller: PlayerId,
        turn: u32,
        active_player: PlayerId,
        step: crate::phase::Step,
    ) -> (GameState, GameState) {
        let mut before = GameState::new_two_player();
        before.battlefield.push(Permanent {
            id: PermanentId(1),
            instance: CardInstanceId(1),
            printed: id_in(db, functional_id).into(),
            controller,
            tapped: false,
            entered_turn: 0,
            attacking: None,
            blocking: Vec::new(),
            skips_untap: false,
            dealt_damage: false,
            damage: 0,
            counters: Default::default(),
            attached_to: None,
            chosen_color: None,
            named_card: None,
            copied: None,
        });
        let mut after = before.clone();
        after.turn = turn;
        after.active_player = active_player;
        after.step = step;
        after.record_event(GameEvent::StepChanged {
            turn,
            active_player,
            step,
        });
        (before, after)
    }

    #[test]
    fn issue_607_your_upkeep_fires_once_on_its_controllers_upkeep() {
        // CR 603.6a with a "your" scope: seat 0's own upkeep, and exactly one trigger
        // for the one crossing.
        let db = step_db();
        let (before, after) = crossing(
            &db,
            "test_your_upkeep",
            PlayerId(0),
            1,
            PlayerId(0),
            crate::phase::Step::Upkeep,
        );
        let triggers = collect_triggers(&before, &after, &db);
        assert_eq!(triggers.len(), 1);
        assert_eq!(triggers[0].controller, PlayerId(0));
        assert_eq!(
            triggers[0].effects,
            vec![Effect::GainLife {
                player_ref: crate::ability::PlayerRef::Controller,
                amount: 1
            }]
        );
    }

    #[test]
    fn issue_607_your_upkeep_does_not_fire_on_an_opponents_upkeep() {
        // The whole point of the scope: seat 0's ability is silent on seat 1's turn.
        let db = step_db();
        let (before, after) = crossing(
            &db,
            "test_your_upkeep",
            PlayerId(0),
            2,
            PlayerId(1),
            crate::phase::Step::Upkeep,
        );
        assert!(collect_triggers(&before, &after, &db).is_empty());
    }

    #[test]
    fn issue_607_each_upkeep_fires_on_every_players_upkeep() {
        // The "each" scope ignores whose turn it is: seat 0's artifact triggers on
        // seat 1's upkeep just as it does on its own.
        let db = step_db();
        for active in [PlayerId(0), PlayerId(1)] {
            let (before, after) = crossing(
                &db,
                "test_each_upkeep",
                PlayerId(0),
                2,
                active,
                crate::phase::Step::Upkeep,
            );
            let triggers = collect_triggers(&before, &after, &db);
            assert_eq!(
                triggers.len(),
                1,
                "an each-upkeep trigger fires on seat {}'s turn",
                active.0
            );
            assert_eq!(triggers[0].controller, PlayerId(0));
        }
    }

    #[test]
    fn issue_607_a_step_trigger_ignores_a_crossing_into_another_step() {
        // A condition names one step. Every other crossing is not its boundary, and a
        // transition that crossed no boundary at all fires nothing either.
        let db = step_db();
        for step in [
            crate::phase::Step::Draw,
            crate::phase::Step::BeginCombat,
            crate::phase::Step::End,
        ] {
            let (before, after) =
                crossing(&db, "test_your_upkeep", PlayerId(0), 1, PlayerId(0), step);
            assert!(
                collect_triggers(&before, &after, &db).is_empty(),
                "an upkeep trigger must not fire on a crossing into {step:?}"
            );
        }
    }

    #[test]
    fn issue_607_a_transition_that_crosses_no_boundary_fires_nothing() {
        // The once-per-crossing property from the other side, and the reason the
        // condition reads recorded events rather than comparing `before.step` with
        // `after.step`: here the state *is* in the upkeep step both before and after,
        // and no crossing happened, so nothing triggers. A snapshot comparison that
        // looked only at `after.step` would fire on every such transition.
        let db = step_db();
        let (before, _) = crossing(
            &db,
            "test_your_upkeep",
            PlayerId(0),
            1,
            PlayerId(0),
            crate::phase::Step::Upkeep,
        );
        let mut before = before;
        before.step = crate::phase::Step::Upkeep;
        let mut after = before.clone();
        after.players[0].life -= 1; // some unrelated thing happened during upkeep
        assert!(collect_triggers(&before, &after, &db).is_empty());
    }

    #[test]
    fn issue_607_one_transition_crossing_several_steps_fires_each_of_them() {
        // One pass of priority can walk through several steps at once (end → cleanup
        // → untap → upkeep), and each crossing is recorded. A watcher of the end step
        // and a watcher of the upkeep both fire from that single transition — which a
        // comparison of `before.step` with `after.step` could never see, because only
        // the last step survives into `after`.
        let db = step_db();
        let mut before = GameState::new_two_player();
        before.step = crate::phase::Step::End;
        for (index, id) in ["test_each_end_step", "test_each_upkeep"]
            .into_iter()
            .enumerate()
        {
            before.battlefield.push(Permanent {
                id: PermanentId(index as u64 + 1),
                instance: CardInstanceId(index as u64 + 1),
                printed: id_in(&db, id).into(),
                controller: PlayerId(0),
                tapped: false,
                entered_turn: 0,
                attacking: None,
                blocking: Vec::new(),
                skips_untap: false,
                dealt_damage: false,
                damage: 0,
                counters: Default::default(),
                attached_to: None,
                chosen_color: None,
                named_card: None,
                copied: None,
            });
        }
        let mut after = before.clone();
        for (turn, active, step) in [
            (1, PlayerId(0), crate::phase::Step::End),
            (1, PlayerId(0), crate::phase::Step::Cleanup),
            (2, PlayerId(1), crate::phase::Step::Untap),
            (2, PlayerId(1), crate::phase::Step::Upkeep),
        ] {
            after.record_event(GameEvent::StepChanged {
                turn,
                active_player: active,
                step,
            });
        }
        after.turn = 2;
        after.active_player = PlayerId(1);
        after.step = crate::phase::Step::Upkeep;

        let triggers = collect_triggers(&before, &after, &db);
        assert_eq!(triggers.len(), 2, "both crossings were observed");
        let amounts: Vec<Effect> = triggers.iter().flat_map(|t| t.effects.clone()).collect();
        assert!(amounts.contains(&Effect::GainLife {
            player_ref: crate::ability::PlayerRef::Controller,
            amount: 4
        }));
        assert!(amounts.contains(&Effect::GainLife {
            player_ref: crate::ability::PlayerRef::Controller,
            amount: 2
        }));
    }

    #[test]
    fn issue_607_a_source_that_is_gone_does_not_trigger() {
        // An ability that is no longer on the battlefield is not there to trigger,
        // the same rule the other watching conditions follow.
        let db = step_db();
        let (before, after) = crossing(
            &db,
            "test_your_combat",
            PlayerId(0),
            2,
            PlayerId(0),
            crate::phase::Step::BeginCombat,
        );
        assert_eq!(collect_triggers(&before, &after, &db).len(), 1);

        let mut gone = after;
        gone.battlefield.clear();
        assert!(collect_triggers(&before, &gone, &db).is_empty());
    }
}
