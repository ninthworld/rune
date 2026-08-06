//! The stack: spells and non-mana abilities waiting to resolve.
//!
//! Plain data with no closures, so [`crate::GameState`] keeps its `Clone`/`Eq`
//! value semantics. Objects resolve top-first (last element is the top) when all
//! players pass priority in succession (see `crate::apply_action`).

use crate::ability::{Effect, Target};
use crate::id::{CardInstance, PermanentId, PlayerId};

/// Identity of an object on the stack, minted fresh when the object is put there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct StackId(pub u64);

/// What paying an object's cost recorded, for the effects that read a number off the
/// payment rather than off the game (CR 601.2h).
///
/// **Captured as the cost is paid, never re-derived.** That is the whole reason this
/// exists rather than a scan at resolution: a cost is paid as the spell goes on the stack,
/// so by the time the spell resolves the permanents it ate are in a graveyard, have lost
/// their [`PermanentId`], and — for a token — do not exist at all. `Thud deals damage
/// equal to the sacrificed creature's power` is a question with no answer left on the
/// board, and CR 608.2h says the answer is the one that *was* true: last-known information
/// about an object that has left.
///
/// Default is the empty payment, which is what every object whose cost took nothing the
/// player picked carries — almost all of them.
///
/// **How many a cost sacrificed is deliberately not here.** Every count a cost may name is
/// exact ([`SacrificeCount`](crate::SacrificeCount)), so the number is on the card and
/// needs no recording; the one card that read a sacrifice back — Scapeshift — sacrifices on
/// resolution instead, where the count is a live answer rather than last-known information.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PaidCost {
    /// The power the sacrificed creature had as it left (CR 608.2h), or `None` when the
    /// cost sacrificed no creature.
    ///
    /// The **first** creature the payment named, because the phrase that reads it — *the
    /// sacrificed creature's power* — is printed only on a card whose cost sacrifices
    /// exactly one. A cost that took several would make the phrase ambiguous on the card
    /// before it made it ambiguous here.
    pub sacrificed_power: Option<i32>,
}

/// One object on the stack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StackObject {
    /// This object's stack identity.
    pub id: StackId,
    /// The player who controls the object (chooses how it resolves).
    pub controller: PlayerId,
    /// What the object is.
    pub kind: StackObjectKind,
    /// The targets chosen for this object when it was put on the stack (CR
    /// 601.2c — targets are locked in on announcement, not on resolution), in
    /// the order of the targeting [`Effect`]s that consume them.
    ///
    /// Empty for an object that targets nothing. Recording the choice here keeps
    /// the stack a complete, inspectable record ("Lightning Bolt targeting that
    /// creature") and lets resolution re-check each target's legality against
    /// current state without any side lookup. Enumerating and choosing these
    /// values from `valid_actions` is issue #71; this field only stores them.
    pub targets: Vec<Target>,
    /// What paying for this object recorded (CR 601.2h) — the sibling of
    /// [`targets`](Self::targets), and here for the same reason.
    ///
    /// A target is chosen at announcement and stored because resolution has to know what
    /// was aimed at; a cost is *paid* at announcement and its payment stored because
    /// resolution has to know what was spent. Both questions have an answer only at the
    /// moment the object went on the stack, and neither could be reconstructed from the
    /// board afterwards.
    ///
    /// Empty for the overwhelming majority of objects — see [`PaidCost`].
    pub paid: PaidCost,
}

/// The two things that can be on the stack: a spell or an ability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StackObjectKind {
    /// A permanent spell cast from a hand; resolving it puts the permanent onto
    /// the battlefield.
    Spell {
        /// The physical card being cast. Carried as a [`CardInstance`] so the
        /// card's identity is preserved from hand, across the stack, onto the
        /// battlefield or into the graveyard.
        card: CardInstance,
        /// The **mode** chosen as this spell was announced (CR 700.2), as an index into
        /// its card's printed list. `None` for every spell that is not modal.
        ///
        /// Recorded here for the reason [`StackObject::targets`] is: the choice was made
        /// on announcement and the object on the stack is the complete record of it.
        /// Resolution reads its effects through this and can reach no other mode.
        mode: Option<u8>,
        /// The value announced for **X** (CR 601.2b). `None` for every spell whose cost
        /// prints none.
        ///
        /// **This is where X is locked.** It was fixed before the cost was paid, and
        /// from here it is the only X anything reads: the resolving effect
        /// ([`DerivedAmount::AnnouncedX`](crate::DerivedAmount)), the threshold a
        /// [`SpellTrait`](crate::SpellTrait) is measured against, and the text a caller
        /// generates for the object on the stack. Nothing re-derives it from the cost,
        /// because by the time the spell is here the cost is already paid and gone.
        x: Option<u32>,
    },
    /// A **copy of a spell** (CR 707.10): a spell on the stack that was never cast and
    /// has no card behind it.
    ///
    /// Its own variant rather than a flag on [`Self::Spell`], for the reason
    /// [`Printed::Token`](crate::Printed) is its own variant rather than a flag on a
    /// permanent: the difference is an *absence*, and every place that difference matters
    /// is a place that asks for the card. A copy has none — so it is never logged as
    /// cast, nothing watching a cast notices it (CR 707.10: a copy of a spell is not
    /// cast), and when it finishes resolving there is no card to put in a graveyard, so
    /// it simply ceases to exist (CR 707.10a).
    ///
    /// It carries the copied spell's [`CardId`] because that is where its
    /// characteristics and its spell effects are read from (CR 707.2 — a copy acquires
    /// the copiable values of the original), and only that: the *decisions* made for the
    /// original ride on [`StackObject::targets`] exactly as the original's do.
    SpellCopy {
        /// The card whose copiable values this copy has (CR 707.2). Not a
        /// [`CardInstance`]: there is no physical copy, which is the whole point.
        card: crate::id::CardId,
        /// Whether this copy's controller was offered **new targets** for it
        /// (CR 707.10c), and therefore whether the slots on
        /// [`StackObject::targets`] are theirs to fill.
        ///
        /// Raw stored state, not a derivation (ADR 0005 §1): the permission comes from
        /// the effect that made the copy and nothing about the copy itself records it. It
        /// is not a flag anyone clears — the copy is owed an answer exactly while it has
        /// no targets, so answering ends the question by filling them
        /// ([`pending_trigger_target_choice`](crate::pending_trigger_target_choice)).
        ///
        /// `false` for a copy that simply inherited the original's decisions
        /// (CR 707.10), which is every copy of a spell that targets nothing and every one
        /// whose slots have no legal candidate left to offer.
        new_targets: bool,
        /// The **mode** the original announced (CR 601.2b), carried because CR 707.10
        /// gives a copy the original's characteristics *and* the choices made for it — a
        /// copy of a modal spell resolves the mode that was chosen, and nobody chooses
        /// again.
        mode: Option<u8>,
        /// The **X** the original announced, carried for the same reason. Nothing is
        /// charged for a copy, so this is read and never paid.
        x: Option<u32>,
    },
    /// A triggered or activated (non-mana) ability; resolving it applies its
    /// effects.
    Ability {
        /// The object whose ability this is — a permanent, or an emblem (CR 114).
        source: AbilitySource,
        /// How this ability got onto the stack (CR 113.3).
        origin: AbilityOrigin,
        /// The effects to apply on resolution.
        effects: Vec<Effect>,
    },
}

/// The object an ability on the stack came from (CR 113.3).
///
/// Until emblems existed this was always a [`PermanentId`], and the ability model could
/// assume its source was on the battlefield: a self-referential effect modified it, a
/// trigger condition observed it, and a source that had left simply did nothing. An
/// **emblem** breaks that assumption in a way no future variant will un-break — it is
/// not a permanent, it has no `PermanentId`, and it is in no zone — so the distinction
/// is stated here rather than smuggled through a sentinel id.
///
/// [`Self::permanent`] is the accessor every existing caller wants: it answers `None`
/// for an emblem, which is the same answer a permanent that has left the battlefield
/// effectively gave, so self-referential effects need no new case.
///
/// A **card in a graveyard** whose ability functions from there (CR 113.6) is the third
/// answer, and it is a third answer rather than a `PermanentId` for the emblem's reason:
/// there is no permanent, and there never was one for this activation. It carries the
/// physical [`CardInstance`] because that — not a [`crate::CardId`] — is what identifies
/// *which* copy in the graveyard the ability belongs to, and a self-referential effect
/// that moves the source out of the graveyard has to name exactly that copy.
///
/// The fourth is the one object that is **both**: a permanent whose dies trigger is on
/// the stack (CR 603.6c). Its ability functioned from the battlefield — that is where it
/// triggered — but by the time anyone can respond the permanent is gone and its card is
/// in a graveyard, and an ability that acts on "it" means that card. CR 603.10a calls
/// this last-known information; here it is simply both halves of the identity recorded
/// at the moment the trigger was collected, so neither accessor below has to guess.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AbilitySource {
    /// A permanent on the battlefield.
    Permanent(PermanentId),
    /// An emblem (CR 114), by its object id
    /// ([`Emblem::id`](crate::Emblem::id)).
    Emblem(u64),
    /// A card in a graveyard whose ability functions from that zone (CR 113.6) — the
    /// source of an activation that never involved the battlefield.
    GraveyardCard(CardInstance),
    /// A permanent that has **left the battlefield for a graveyard**, and the card it
    /// became — the source of a dies trigger (CR 603.6c).
    ///
    /// Only the leave-the-battlefield pass of the trigger diff creates one, and only for
    /// a permanent that was a card: a token reaches no graveyard (CR 111.7), so its dies
    /// trigger records [`Self::Permanent`] and its `graveyard_card` is rightly `None`.
    DeadPermanent {
        /// What the permanent was, by the battlefield identity it had. Never reused, so
        /// it still names that object and nothing else.
        permanent: PermanentId,
        /// The physical card it is now, in its owner's graveyard.
        card: CardInstance,
    },
    /// A **delayed triggered ability** (CR 603.7), which belongs to no object anyone can
    /// point at.
    ///
    /// The one answer that names nothing at all. CR 603.7d/e give a delayed ability the
    /// source of whatever created it — a spell that has since resolved into a graveyard,
    /// or an ability whose permanent may be long gone — and CR 603.7e is explicit that the
    /// ability fires regardless. Nothing in the engine reads a delayed ability's source
    /// back, so recording a handle to an object that is not there would be a fact kept
    /// only to be wrong: this variant says the honest thing instead, and both accessors
    /// below answer `None`.
    DelayedAbility,
}

impl AbilitySource {
    /// The permanent this ability came from, or `None` for an emblem, a card in a
    /// graveyard, or a delayed ability — none of which is one.
    ///
    /// A **dead** permanent answers with the id it had. That is the same answer it gave
    /// before the id had a name for the state it is in, and it stays right for the same
    /// reason: every caller looks the id up on the battlefield, finds nothing, and does
    /// nothing — which is what a self-referential effect on a permanent that has left
    /// should do.
    #[must_use]
    pub fn permanent(self) -> Option<PermanentId> {
        match self {
            Self::Permanent(id) | Self::DeadPermanent { permanent: id, .. } => Some(id),
            Self::Emblem(_) | Self::GraveyardCard(_) | Self::DelayedAbility => None,
        }
    }

    /// The graveyard card this ability came from, or `None` for every other source.
    ///
    /// The counterpart of [`Self::permanent`], and the accessor a self-referential
    /// effect that moves its own card out of a graveyard reads. A permanent still on the
    /// battlefield answers `None` here for the same reason a graveyard card's answers
    /// `None` there: the object is simply not that kind of thing. A **dead** permanent
    /// answers both, because it is both, and a delayed ability answers `None` to both —
    /// which is what makes every self-referential effect a no-op on one.
    #[must_use]
    pub fn graveyard_card(self) -> Option<CardInstance> {
        match self {
            Self::GraveyardCard(card) | Self::DeadPermanent { card, .. } => Some(card),
            Self::Permanent(_) | Self::Emblem(_) | Self::DelayedAbility => None,
        }
    }
}

impl From<PermanentId> for AbilitySource {
    fn from(id: PermanentId) -> Self {
        Self::Permanent(id)
    }
}

/// How an ability came to be on the stack — its rules provenance (CR 113.3).
///
/// The two ways differ in rule, not only in flavour: an activated ability is put
/// on the stack by a player who paid its cost (CR 602.2), a triggered one by the
/// game itself the next time a player would receive priority (CR 603.3). Nothing
/// else on a [`StackObject`] separates them — the effects, the source, and the
/// composed description can all be identical — so the distinction is recorded at
/// the moment of the push or it is gone (issue #579).
///
/// Storing it here is what lets the server *state* the finer kind on the wire
/// instead of guessing. A client may never reconstruct this from `description`
/// prose or from when the object appeared: that is rules interpretation, which
/// ADR 0001 puts on the server.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AbilityOrigin {
    /// A player activated it, paying its costs (CR 602.2).
    Activated,
    /// A trigger condition was met and the game put it on the stack (CR 603.3).
    Triggered,
}

impl StackObject {
    /// Whether `trait_kind` is in force for this object right now — a
    /// [`SpellTrait`](crate::SpellTrait) its card declares, measured against the X this
    /// object announced (CR 601.2b).
    ///
    /// `false` for an ability: a trait is printed on a card and an ability on the stack
    /// is not one. `false` for a spell whose card carries no such trait, and for one
    /// whose threshold the announced value does not reach — which is the whole of
    /// "if X is 5 or more", asked in one place so the counter check and the damage seam
    /// can never disagree about the same spell.
    ///
    /// A **copy** answers exactly as the original does (CR 707.10): the trait is rules
    /// text, which a copy has (CR 707.2a), and the threshold is measured against the X
    /// the original announced, which the copy carries. Reading it off the copied card is
    /// the same read the copy's effects go through — a copy that answered `false` here
    /// would be a Banefire that stops being uncounterable for having been copied.
    #[must_use]
    pub fn has_trait(&self, db: &crate::CardDatabase, trait_kind: SpellTraitKind) -> bool {
        let (card, x) = match self.kind {
            StackObjectKind::Spell { card, x, .. } => (card.card, x),
            StackObjectKind::SpellCopy { card, x, .. } => (card, x),
            StackObjectKind::Ability { .. } => return false,
        };
        db.card(card).is_some_and(|data| {
            data.spell_traits
                .iter()
                .filter(|declared| trait_kind.names(**declared))
                .any(|declared| declared.applies(x))
        })
    }
}

/// Which [`SpellTrait`](crate::SpellTrait) a caller is asking
/// [`StackObject::has_trait`] about, without naming the threshold that trait carries.
///
/// The two are separate types because the question and the declaration are separate
/// things: a card declares `can\'t be countered **if X is 5 or more**`, and a
/// counterspell asks only `can this be countered`. Folding the threshold into the
/// question would make every asker restate a number it has no business knowing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SpellTraitKind {
    /// CR 701.5a — the spell cannot be removed from the stack by a counter.
    CantBeCountered,
    /// CR 615.1 — no prevention shield applies to damage this spell deals.
    DamageCantBePrevented,
}

impl SpellTraitKind {
    /// Whether `declared` is this kind of trait, whatever threshold it carries.
    fn names(self, declared: crate::card::SpellTrait) -> bool {
        matches!(
            (self, declared),
            (
                Self::CantBeCountered,
                crate::card::SpellTrait::CantBeCountered { .. }
            ) | (
                Self::DamageCantBePrevented,
                crate::card::SpellTrait::DamageCantBePrevented { .. }
            )
        )
    }
}
