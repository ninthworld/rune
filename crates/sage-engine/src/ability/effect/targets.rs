//! What an [`Effect`] must be given **targets** for (CR 601.2c) — the one question every
//! announcement, legality, and resolution path asks of the effect vocabulary.
//!
//! Split out of [`super::effect`] for size (`docs/coding-standards.md`, File size). It is
//! the whole `impl Effect`: the exhaustive, wildcard-free match that names every variant
//! and says which target slots it declares, so a new effect cannot be added without
//! answering the question.

use super::*;

impl Effect {
    /// The [`TargetSpec`] this effect names, for the effects that name exactly **one**.
    ///
    /// The convenience over [`Self::target_groups`] for the great majority of callers,
    /// which care what may be targeted and not how many. `None` for an effect with an
    /// implicit subject ([`Effect::AddMana`], [`Effect::DrawCard`]) *and* for one that
    /// declares more than one group ([`Effect::Fight`]): a two-slot effect has no single
    /// spec, and answering with the first one would be a wrong answer rather than a
    /// partial one.
    #[must_use]
    pub fn target_spec(&self, seats: usize) -> Option<TargetSpec> {
        match self.target_groups(seats).as_slice() {
            [group] => Some(group.spec),
            _ => None,
        }
    }

    /// The [`TargetGroup`] this effect must be given chosen targets for, for an effect
    /// that declares exactly one; `None` for an effect with an implicit subject and for
    /// one that declares two ([`Effect::Fight`]).
    ///
    /// The single-group narrowing of [`Self::target_groups`], kept for the resolve and
    /// legality paths that have already established they are looking at one slot.
    #[must_use]
    pub fn target_group(&self, seats: usize) -> Option<TargetGroup> {
        match self.target_groups(seats).as_slice() {
            [group] => Some(*group),
            _ => None,
        }
    }

    /// Whether this effect's target slots stand or fall **together** — the question
    /// CR 608.2b leaves open for an effect that declares more than one.
    ///
    /// `true` is the conservative answer and the default. A fight is a fight or it is
    /// nothing (CR 701.12c), and an exchange of control is all or nothing (CR 701.10c):
    /// their slots are not interchangeable, so half of either is an effect the card never
    /// printed.
    ///
    /// `false` is for an effect that is one sentence about several **separate** things.
    /// `For each player, choose target permanent that player controls` says the same thing
    /// once per seat, and one seat's target having become illegal says nothing about
    /// another's — so CR 608.2b's ordinary rule applies and the effect does as much as it
    /// still can.
    ///
    /// Only ever asked of an effect with two or more groups, which is why the wildcard is
    /// safe: an effect with one slot has no togetherness to have, and one with none is
    /// never on this path at all.
    #[must_use]
    pub fn slots_are_indivisible(&self) -> bool {
        !matches!(self, Effect::SacrificeChosenPerPlayer { .. })
    }

    /// The [`TargetGroup`]s this effect must be given chosen targets for, in slot
    /// order — empty for an effect with an implicit subject ([`Effect::AddMana`],
    /// [`Effect::DrawCard`]).
    ///
    /// **An ordered list rather than one group**, because an effect's slots need not all
    /// share a spec: a fight names "target creature you control" and "target creature you
    /// don't control" in one sentence (CR 701.12), and the two are aimed together or not
    /// at all. Every other effect in the vocabulary returns zero or one group and behaves
    /// exactly as it did when this answered `Option`.
    ///
    /// The resolution path uses this to pair each of an object's stored [`Target`]s with
    /// the effect that consumes it and to re-check that target's legality (CR 608.2b).
    /// Kept exhaustive so a new targeting [`Effect`] variant must declare its specs here.
    ///
    /// # Why this is given the seat count
    ///
    /// Because one effect's slot count is not a property of the effect. `For each player,
    /// choose target permanent that player controls` declares **one group per seat**
    /// ([`TargetSpec::PermanentThatPlayerControls`]), and how many that is depends on how
    /// many people are playing. Every other effect ignores `seats` entirely and answers
    /// exactly what it always did.
    ///
    /// It is the seat *count* rather than the whole [`GameState`](crate::GameState) on
    /// purpose: this is asked on every announcement, every legality check, and every
    /// resolution, and the only thing about the game any effect needs in order to say how
    /// many slots it has is how many seats there are. Passing less means the answer
    /// cannot quietly start depending on the board — the slots an object declares must be
    /// the same at resolution as at announcement, or the CR 608.2b re-check would be
    /// re-checking a different shape than the one the player answered.
    #[must_use]
    pub fn target_groups(&self, seats: usize) -> Vec<TargetGroup> {
        match self {
            // The one effect whose slot count comes from the table rather than from the
            // card: one required slot per seat, each naming that seat's permanents
            // (CR 601.2c chooses them all at once, in seat order).
            Effect::SacrificeChosenPerPlayer { .. } => (0..seats)
                .map(|seat| {
                    TargetGroup::single(TargetSpec::PermanentThatPlayerControls { seat })
                })
                .collect(),
            // The variable-arity effects: `put a +1/+1 counter on each of up to two
            // target creatures` chooses between zero and two, and every other authoring
            // of the same effects leaves the count at its default of one. They differ
            // only in the verb — the arity is one field, read here in one arm, so a
            // fourth effect that needs it inherits the whole pipeline by joining this
            // pattern.
            Effect::PutCounters {
                target, targets, ..
            }
            | Effect::Restrict {
                target, targets, ..
            }
            | Effect::Destroy { target, targets }
            | Effect::ReturnCardToBattlefield {
                target, targets, ..
            }
            | Effect::ReturnCardToHand { target, targets } => {
                vec![TargetGroup::counted(*target, *targets)]
            }
            // An optional effect declares the group of the **one** effect it wraps, so
            // "you may destroy target artifact" names its target once, at announcement
            // (CR 601.2c), and the yes-or-no comes later. The wrapper is a forwarder
            // rather than a second slot: the same group is answered for here and by the
            // wrapped effect once it is spliced in, which is what keeps the flat stored
            // target list pairing back exactly. The catalog validator holds a `may` to
            // one such effect, so this is a lookup and never a choice between two.
            // The `unless` branch declares the slot when the accepted branch has none:
            // "deals 5 damage to target opponent unless that player sacrifices" aims from
            // the consequence, because accepting is the branch that does nothing. Still
            // one group either way — the validator holds a `may` to one targeting effect
            // across both branches.
            Effect::May {
                effects, otherwise, ..
            } => {
                let accepted: Vec<TargetGroup> = effects
                    .iter()
                    .flat_map(|effect| effect.target_groups(seats))
                    .collect();
                if accepted.is_empty() {
                    otherwise
                        .iter()
                        .flat_map(|effect| effect.target_groups(seats))
                        .collect()
                } else {
                    accepted
                }
            }
            // The one effect whose slots do **not** share a spec (CR 701.12): each of the
            // two creatures is chosen from its own class, in the order the printed
            // sentence names them, and both slots are required.
            Effect::Fight {
                dealer, dealt_to, ..
            } => vec![TargetGroup::single(*dealer), TargetGroup::single(*dealt_to)],
            // The second effect whose two slots are each their own class, and for the same
            // reason: an exchange is the pair, and half of it is nothing (CR 701.10c).
            Effect::ExchangeControl { first, second } => {
                vec![TargetGroup::single(*first), TargetGroup::single(*second)]
            }
            Effect::Animate { target, .. }
            | Effect::ExileUntilSourceLeaves { target }
            | Effect::Tap { target }
            // A creature dealing its own power names only what it is dealt to: the dealer
            // is the ability's source, never a slot (CR 609.7).
            | Effect::SelfDealsDamage { target }
            | Effect::CounterSpell { target }
            | Effect::Exile { target, .. }
            | Effect::Pump { target, .. }
            | Effect::PumpByCount { target, .. }
            | Effect::PumpByAmount { target, .. }
            | Effect::GrantKeyword { target, .. }
            | Effect::GainControl { target, .. }
            | Effect::PutOnTopOfLibrary { target }
            // An equip names its *host* as a target and its own source as everything
            // else, so it declares exactly one slot (CR 702.6b).
            | Effect::Attach { target }
            // Copying a spell names the spell it copies, and that slot is filled by the
            // trigger event rather than by a player (CR 603.7c) — but it is a slot, so the
            // CR 608.2b re-check applies to it like any other.
            | Effect::CopySpell { target, .. }
            | Effect::ReturnToHand { target }
            // Clearing a permanent's counters names it in one slot; the prohibition that
            // follows is about the same object and asks for nothing further.
            | Effect::RemoveAllCounters { target, .. } => vec![TargetGroup::single(*target)],
            // A player-subject effect targets exactly when its reference does
            // (CR 115.1) — "target opponent loses 2 life" fills a slot, "each
            // opponent loses 2 life" fills none. One answer, from the reference.
            Effect::GainLife { player_ref, .. }
            | Effect::LoseLife { player_ref, .. }
            | Effect::Mill { player_ref, .. }
            // Winning names its player the same way, and the answer is the same one:
            // "you win the game" fills no slot.
            | Effect::WinTheGame { player_ref }
            // And so does taking an extra turn.
            | Effect::TakeExtraTurn { player_ref }
            // A discard names its hand the same way, and for the same reason: "target
            // player discards two cards" fills a slot and can fizzle, "each opponent
            // discards a card" fills none and cannot.
            | Effect::Discard { player_ref, .. }
            | Effect::GainLifeByCount { player_ref, .. }
            // The three verbs whose number is derived name their player the same way
            // their fixed-count siblings do: the amount is where X comes from, never a
            // subject, so it changes nothing about the targeting question.
            | Effect::LoseLifeByAmount { player_ref, .. }
            | Effect::DiscardByAmount { player_ref, .. }
            | Effect::Sacrifice { player_ref, .. }
            | Effect::ExileGraveyard { player_ref }
            // And clearing a player's names them the same way their graveyard is named.
            | Effect::PlayerLosesAllCounters { player_ref, .. }
            // Digging through a library names its owner the same way emptying one does.
            | Effect::ExileFromLibraryUntil { player_ref, .. }
            // Emptying a library names its owner the same way a graveyard's does.
            | Effect::ExileLibraryExceptBottom { target: player_ref }
            // And a mass tap names whose creatures the same way: "tap all creatures
            // target player controls" fills a slot, and a class relative to the
            // controller would not.
            | Effect::TapAll { player_ref, .. }
            // And a token creation names its creator the same way: "create a 2/4 white
            // Ox token" is made by you, "target player creates …" fills a slot.
            | Effect::CreateToken { player_ref, .. } => player_ref
                .target_spec()
                .map(TargetGroup::single)
                .into_iter()
                .collect(),
            // Damage asks its subject the same question: "any target" fills a slot,
            // "each opponent" and "each creature" fill none (CR 115.1).
            Effect::DealDamage { subject, .. }
            | Effect::DealDamageByCount { subject, .. }
            | Effect::DealDamageByAmount { subject, .. } => subject
                .target_spec()
                .map(TargetGroup::single)
                .into_iter()
                .collect(),
            Effect::AddMana { .. }
            | Effect::AddColorlessMana { .. }
            | Effect::AddRestrictedMana { .. }
            // A color choice names no target: the question is about mana, and the
            // player answering it is the effect's controller by definition.
            | Effect::AddManaAnyColor { .. }
            | Effect::DrawCard { .. }
            // A derived number of cards is still drawn by the controller, so it names no
            // target either — where the number comes from is not a subject.
            | Effect::DrawCardsByAmount { .. }
            // An emblem is given to a named player, never a targeted one (CR 114.3 —
            // "you get an emblem"), and a graveyard-casting permission likewise names
            // its player without targeting.
            | Effect::CreateEmblem { .. }
            | Effect::AllowCastingFromGraveyard { .. }
            | Effect::ExileTopForPlay { .. }
            // A hexproof-ignoring permission names its player the same way, and for the
            // same reason: it is a fact about a seat, not about an object.
            | Effect::IgnoreHexproof { .. }
            // A replacement effect names an *event*, which is not an object and so
            // cannot be targeted (CR 115.1) — and a prevention shield is one.
            | Effect::CreateReplacement { .. }
            // A delayed triggered ability names an event too, and the object it acts on
            // is whatever that event produced (CR 603.7c) rather than anything aimed here.
            | Effect::CreateDelayedTrigger { .. }
            // A reflexive ability names nothing here: the ability it creates declares its
            // own slot, and that slot is filled when it goes on the stack (CR 603.3d).
            | Effect::CreateReflexiveTrigger { .. }
            // Nor does the offer that creates one by being paid for. Its effects' targets
            // are chosen when the ability goes on the stack, which is after the payment —
            // the whole reason this is not an ordinary `may`.
            | Effect::MayPayForTrigger { .. }
            // An Aura's host was chosen when the Aura was cast; tapping it aims at
            // nothing (CR 303.4a).
            | Effect::TapAttached
            | Effect::DestroyAttached
            // A card that sacrifices itself names the permanent by saying "it".
            | Effect::SacrificeSelf
            // And one that becomes something else names itself the same way.
            | Effect::AnimateSelf { .. }
            | Effect::PreventDamage { .. }
            // A choice over the controller's own library names no target: the library
            // is theirs by definition (CR 115.1).
            | Effect::Scry { .. }
            | Effect::LookAtTop { .. }
            | Effect::RevealTopAndMayPlay { .. }
            | Effect::MayCastExiledThisWay { .. }
            | Effect::SearchLibrary { .. }
            // A conditional declares no slot: it has two branches and one flat target
            // list, so a group named in either one could not be paired back onto the
            // branch that was actually taken. The catalog validator rejects a branch
            // that tries.
            | Effect::Conditional { .. }
            // A class of permanents is not a target (CR 115.1), and neither is the
            // ability's own source.
            | Effect::DestroyAll { .. }
            | Effect::PumpAll { .. }
            | Effect::GrantKeywordAll { .. }
            | Effect::RestrictAll { .. }
            | Effect::PumpSelf { .. }
            | Effect::RestrictSelf { .. }
            | Effect::AlterAbilitiesSelf { .. }

            // A card returning itself out of a graveyard names its own source, which is
            // not a target either (CR 115.1) — the reason a graveyard activation needs no
            // candidate set to be offered.
            | Effect::ReturnSelfFromGraveyard { .. }
            // Nor is a permanent shuffling itself away: the source names itself, so there
            // is no slot to fill and nothing to fizzle on.
            | Effect::ShuffleSelfIntoLibrary
            // Turning a permanent over names its own source, whichever road it takes.
            | Effect::TransformSelf
            | Effect::ExileSelfAndReturnTransformed
            | Effect::PutCountersOnSelf { .. }
            | Effect::PutHandOntoBattlefieldFaceDown { .. } => Vec::new(),
        }
    }
}
