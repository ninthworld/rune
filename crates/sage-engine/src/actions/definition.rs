//! Action types and core methods: the closed set of legal player choices.

use crate::ability::Target;
use crate::id::{CardInstance, CardInstanceId, PermanentId, PlayerId};

/// An action a player may take. The engine generates the legal set with
/// [`crate::valid_actions`] and validates a chosen action against it in
/// [`crate::apply_action`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Yield priority without taking any other action.
    PassPriority,
    /// Play a land from hand (a special action; lands do not use the stack).
    PlayLand {
        /// The specific land card in the active player's hand to play. Names the
        /// physical copy, so two identical lands in hand are distinguishable.
        card: CardInstance,
    },
    /// Activate an ability of a permanent the priority holder controls.
    ActivateAbility {
        /// The permanent whose ability is activated.
        permanent: PermanentId,
        /// Index into the permanent's abilities (see [`crate::abilities_of`]).
        index: usize,
        /// The targets chosen for this activation, one per target slot the
        /// ability's effects declare (see [`crate::Effect::target_spec`]), in that
        /// order. Empty for an ability that targets nothing.
        ///
        /// This is the **parameterized targeted representation** (ADR 0004
        /// §Enumeration): a single [`Action`] value *carries* the player's target
        /// selection, rather than the generator pre-expanding one variant per
        /// legal target combination. [`crate::valid_actions`] advertises the action once
        /// in its *requirement* form (this field empty); the legal candidates for
        /// each slot come from [`crate::target_requirements`], and a filled-in selection
        /// is validated slot-by-slot in [`crate::apply_action`].
        targets: Vec<Target>,
    },
    /// Choose the targets of a **triggered ability already on the stack** (CR 603.3d).
    ///
    /// A trigger is put on the stack by the game, not by a player, so its controller
    /// has had no opportunity to aim it: it arrives with no targets and this action is
    /// the choosing. Until it is answered the game does not proceed — the ability goes
    /// on the stack before any player receives priority (CR 603.3b) — so while one is
    /// owed, [`crate::valid_actions`] offers this and nothing else, to the trigger's
    /// controller rather than to whoever last held priority.
    ///
    /// A trigger with *no* legal choice for one of its slots is never put on the stack
    /// at all (CR 603.3c), so this action is only ever offered when it can be answered.
    ChooseTriggerTargets {
        /// The stack object being aimed — a triggered ability owed targets, as
        /// reported by [`crate::pending_trigger_target_choice`].
        ability: crate::stack::StackId,
        /// One target per slot the ability's effects declare, in that order; the same
        /// parameterized representation [`Self::ActivateAbility`] uses.
        targets: Vec<Target>,
    },
    /// Answer the **mid-resolution yes-or-no** the game is currently waiting on: an
    /// optional effect's `you may …` (CR 608.2 — see [`crate::Effect::May`]).
    ///
    /// The confirmation counterpart of [`Self::AnswerChoice`], routed the same way and
    /// with the same exclusivity — while it is owed, its chooser is offered this and
    /// (when the effect asks for mana) their mana abilities, and every other seat is
    /// offered nothing. The chooser is the offering ability's **controller**, who is
    /// frequently not the priority holder: a creature entering on an opponent's turn
    /// asks its own controller whether to take the trigger's optional effect.
    ///
    /// `accept: true` is legal only while the cost is payable from that seat's pool
    /// right now; declining is always legal, which is why an unpayable cost can never
    /// stall the game. A cost no amount of tapping could pay is never posed at all.
    AnswerConfirm {
        /// Whether to apply the optional effect. `false` skips it and resumes the rest
        /// of the object's resolution untouched — declining is not a fizzle.
        accept: bool,
    },
    /// Answer the **mid-resolution player choice** the game is currently waiting on
    /// (CR 701.8 discard, CR 701.17 scry, CR 701.19 search — see
    /// [`crate::pending_player_choice`]).
    ///
    /// An effect that asks a player to choose cards suspends its object's resolution
    /// and queues the question; this action is the answer, and nothing else can happen
    /// until it arrives. Offered to the choice's *chooser* — who may be neither the
    /// priority holder nor the resolving object's controller, since "target player
    /// discards two cards" asks the targeted seat — and to no one else, so while a
    /// choice is owed [`crate::valid_actions`] offers this and nothing else to that
    /// seat and an empty list to every other.
    ///
    /// A choice with no legal answer is never queued at all (it is applied outright
    /// with an empty selection), so like [`Self::ChooseTriggerTargets`] this action is
    /// only ever offered when it can be answered.
    AnswerChoice {
        /// The cards chosen, in the order they were chosen — which is load-bearing for
        /// a scry, where it is the order they are put on the bottom in. Each names a
        /// card in the choice's freshly recomputed candidate set
        /// ([`crate::choice_candidates`]); the selection size must fall within that
        /// choice's clamped bounds ([`crate::choice_bounds`]). An empty selection is
        /// legal whenever the minimum is zero — declining to scry, or failing to find.
        chosen: Vec<CardInstanceId>,
    },
    /// Cast a spell from hand, paying its mana cost from the caster's pool.
    CastSpell {
        /// The specific card in the caster's hand to cast. Names the physical
        /// copy, so two identical cards in hand are distinguishable.
        card: CardInstance,
        /// The targets chosen for this cast, one per target slot the card's spell
        /// effects declare (see [`crate::Effect::target_spec`]), in that order. Empty for
        /// a spell that targets nothing.
        ///
        /// The same **parameterized targeted representation** an ability uses (ADR
        /// 0009 §Enumeration): the [`Action`] carries the player's selection (CR
        /// 601.2c — targets are chosen as part of casting), rather than the
        /// generator pre-expanding one variant per legal target combination.
        /// [`crate::valid_actions`] advertises the cast once in its requirement form (this
        /// field empty); per-slot candidates come from [`crate::target_requirements`], and
        /// a filled selection is validated slot-by-slot in [`crate::apply_action`].
        targets: Vec<Target>,
    },
    /// Discard one card from hand to satisfy the cleanup step's maximum-hand-size
    /// turn-based action (CR 514.1). Offered — one per card in the active
    /// player's hand, a select-from-zone choice — only while that player is over
    /// [`crate::MAX_HAND_SIZE`] during [`crate::Step::Cleanup`]. Names the
    /// physical copy, so identical cards stay individually addressable.
    Discard {
        /// The specific card in the active player's hand to discard.
        card: CardInstance,
    },
    /// Mulligan the current opening hand during the pre-game London mulligan
    /// (CR 103.5): shuffle it back into the library, draw a fresh hand of the
    /// opening size, and decide again. Offered only in the mulligan phase, to the
    /// deciding seat (see [`crate::mulligan`]).
    Mulligan,
    /// Keep the current opening hand, ending this seat's London-mulligan decisions
    /// (CR 103.5). A seat that has taken `N` mulligans must put `N` cards on the
    /// bottom of its library; `bottom` names those cards, one [`Target::Card`]
    /// per card, chosen from the [bottoming requirement](crate::bottom_requirement)
    /// (empty for a first-hand keep). [`crate::valid_actions`] advertises this in its
    /// *requirement* form (empty `bottom`); a filled-in selection is validated in
    /// [`crate::apply_action`]. Offered only in the mulligan phase, to the deciding
    /// seat.
    Keep {
        /// The chosen cards to put on the bottom of the library, in the order they
        /// are placed there. Exactly one [`Target::Card`] per mulligan taken, each
        /// naming a distinct card currently in the deciding seat's hand.
        bottom: Vec<Target>,
    },
    /// Declare the active player's attackers (CR 508.1), the turn-based player
    /// choice of the declare-attackers step. Each named permanent must be a legal
    /// attacker candidate ([`crate::attacker_candidates`]); an **empty** selection is
    /// legal — declaring no attackers (CR 508.1a). Applying it taps each attacker
    /// (no vigilance yet) and moves the step into its priority round.
    ///
    /// Like [`Action::ActivateAbility`]'s targets, this is a *parameterized*
    /// multi-select: [`crate::valid_actions`] advertises the action once in its empty
    /// requirement form, the legal candidates come from [`crate::attacker_candidates`],
    /// and a filled-in selection is validated against that fresh set in
    /// [`crate::apply_action`] — never pre-expanded into one action per subset.
    DeclareAttackers {
        /// The declared attacks: each names one attacker and the defending player
        /// it attacks (CR 508.1a). In a two-player game the only legal defender is
        /// the sole opponent; with more seats each attacker chooses among the
        /// opponents still in the game ([`crate::defender_candidates`]).
        attackers: Vec<Attack>,
    },
    /// Declare the defending player's blockers (CR 509.1), the turn-based player
    /// choice of the declare-blockers step. Each [`Block`] assigns one eligible
    /// blocker ([`crate::blocker_candidates`]) to one attacking creature
    /// ([`crate::declared_attackers`]); several blockers may share an attacker, but a
    /// blocker is assigned to exactly one (CR 509.1a). An **empty** selection is
    /// legal — declaring no blockers.
    DeclareBlockers {
        /// The blocker→attacker assignments, one per declared blocker.
        blocks: Vec<Block>,
    },
    /// The attacking player's combat-damage assignment order (CR 510.1, issue #346),
    /// the turn-based choice owed once blockers are declared and some attacker is
    /// blocked by two or more creatures. One [`DamageOrder`] per such attacker, each
    /// a permutation of that attacker's blockers; combat damage is then assigned
    /// just-lethal along the chosen order. An attacker with 0–1 blockers is never
    /// ordered. Advertised in its empty requirement form; a filled selection is
    /// validated in [`crate::apply_action`].
    OrderCombatDamage {
        /// One blocker ordering per multi-blocked attacker.
        orders: Vec<DamageOrder>,
    },
    /// Accept the CR 903.9a choice: move the commander from the graveyard or exile
    /// it went to into its owner's command zone instead. Offered only to the
    /// commander's owner while a return decision is pending (the commander sits in
    /// a graveyard or exile awaiting the choice — see [`crate::valid_actions`]). Applying
    /// it removes the card from wherever it is and puts it in the command zone as a
    /// fresh object (it will mint a fresh [`PermanentId`] if recast), and logs the
    /// movement.
    ReturnCommanderToCommandZone {
        /// The commander card to move to the command zone. Names the physical copy
        /// so the owner's commander is unambiguous.
        card: CardInstance,
    },
    /// Decline the CR 903.9a choice: leave the commander where it went (the
    /// graveyard or exile). Offered alongside [`Action::ReturnCommanderToCommandZone`]
    /// while a return decision is pending; applying it simply clears the pending
    /// decision so the commander stays put. This is the decline-compatible default,
    /// so priority automation always has a legal way forward and never stalls.
    DeclineCommanderReturn {
        /// The commander card whose return is declined.
        card: CardInstance,
    },
    /// Concede the game (CR 104.3a). Always offered to the acting seat, in every
    /// phase and step, so a player may leave at any time. Applying it marks the
    /// conceding player as having lost; the game then becomes terminal with the
    /// opponent as the winner (CR 104.2a).
    Concede,
}

/// One attacker→defender assignment of a [`Action::DeclareAttackers`] declaration
/// (CR 508.1a): the `attacker` is declared to attack the defending player
/// `defender`.
///
/// In a two-player game every attack's `defender` is the sole opponent, so the
/// declaration is choice-free; with more seats each attacker records which
/// opponent it attacks, which is what blocker eligibility and combat damage follow
/// (issue #341). Plain `Copy`/`Eq` data, mirroring [`Block`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Attack {
    /// The creature declared as an attacker (an [`crate::attacker_candidates`] member).
    pub attacker: PermanentId,
    /// The defending player it attacks (a [`crate::defender_candidates`] member).
    pub defender: PlayerId,
}

/// One attacker's combat-damage assignment order (CR 510.1, issue #346): the
/// `attacker`'s blockers listed in the order its controller chose to assign lethal
/// damage along. `blockers` is a permutation of exactly that attacker's declared
/// blockers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DamageOrder {
    /// The multi-blocked attacker whose damage order this is.
    pub attacker: PermanentId,
    /// Its blockers, in the chosen assignment order.
    pub blockers: Vec<PermanentId>,
}

/// One blocker→attacker assignment of a [`Action::DeclareBlockers`] declaration
/// (CR 509.1a): the `blocker` is declared to block the attacking `attacker`.
///
/// Plain `Copy`/`Eq` data (no closures), so an [`Action`] stays a value the
/// engine can compare and the state machine can carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Block {
    /// The creature declared as a blocker (a [`crate::blocker_candidates`] member).
    pub blocker: PermanentId,
    /// The attacking creature it is assigned to block (a [`crate::declared_attackers`]
    /// member).
    pub attacker: PermanentId,
}

impl Action {
    /// The chosen targets carried by this action, in slot order; empty for an
    /// action that carries none.
    pub(super) fn targets(&self) -> &[Target] {
        match self {
            Action::ActivateAbility { targets, .. }
            | Action::CastSpell { targets, .. }
            | Action::ChooseTriggerTargets { targets, .. } => targets,
            // `Keep::bottom` is a mulligan sub-choice and `AnswerChoice::chosen` a
            // mid-resolution one; both name cards in a hidden zone rather than
            // targets, and are validated through their own paths, never this one.
            Action::PassPriority
            | Action::AnswerChoice { .. }
            | Action::AnswerConfirm { .. }
            | Action::PlayLand { .. }
            | Action::Discard { .. }
            | Action::Mulligan
            | Action::Keep { .. }
            // Combat declarations carry permanent selections, not `Target`s, so
            // they hold none of the ability-targeting vocabulary; their selection
            // is validated separately in `action_is_legal`.
            | Action::DeclareAttackers { .. }
            | Action::DeclareBlockers { .. }
            | Action::OrderCombatDamage { .. }
            // The commander-return decisions carry only a card, no target slots.
            | Action::ReturnCommanderToCommandZone { .. }
            | Action::DeclineCommanderReturn { .. }
            // Concede carries no selection.
            | Action::Concede => &[],
        }
    }

    /// This action with its chosen targets cleared — its *requirement* form, the
    /// shape [`crate::valid_actions`] advertises. Target-carrying variants drop their
    /// selection; every other variant is returned unchanged.
    pub(super) fn without_targets(&self) -> Action {
        match self {
            Action::ActivateAbility {
                permanent, index, ..
            } => Action::ActivateAbility {
                permanent: *permanent,
                index: *index,
                targets: Vec::new(),
            },
            // A cast drops its target selection to its requirement form, the shape
            // `valid_actions` advertises (CR 601.2c targets are filled in later).
            Action::CastSpell { card, .. } => Action::CastSpell {
                card: *card,
                targets: Vec::new(),
            },
            Action::ChooseTriggerTargets { ability, .. } => Action::ChooseTriggerTargets {
                ability: *ability,
                targets: Vec::new(),
            },
            // The mulligan keep's bottom selection is cleared the same way, so its
            // requirement form matches what [`valid_actions`] advertises — as is a
            // mid-resolution choice's, which is likewise advertised empty and filled
            // in by the answer.
            Action::Keep { .. } => Action::Keep { bottom: Vec::new() },
            Action::AnswerChoice { .. } => Action::AnswerChoice { chosen: Vec::new() },
            // A yes-or-no is advertised as the bare question; the answer rides in the
            // submitted action, so its requirement form is the declining default —
            // never a second offer per possible reply.
            Action::AnswerConfirm { .. } => Action::AnswerConfirm { accept: false },
            // The requirement form of a combat declaration is the empty selection —
            // exactly what `valid_actions` advertises during the declare window.
            Action::DeclareAttackers { .. } => Action::DeclareAttackers {
                attackers: Vec::new(),
            },
            Action::DeclareBlockers { .. } => Action::DeclareBlockers { blocks: Vec::new() },
            Action::OrderCombatDamage { .. } => Action::OrderCombatDamage { orders: Vec::new() },
            other => other.clone(),
        }
    }
}

/// One target slot of a targeted [`Action`]: the [`crate::TargetSpec`] that constrains
/// the slot together with the *set* of [`Target`]s currently legal for it.
///
/// This is the unit [`crate::target_requirements`] advertises per slot — the "target
/// requirement plus the set of legal targets" of ADR 0004 §Enumeration. The
/// candidate set is O(N) in that slot's candidate count; see the combinatorial
/// guard on [`crate::target_requirements`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetRequirement {
    /// What the slot may target.
    pub spec: crate::TargetSpec,
    /// Every [`Target`] legal for the slot against current state, in a stable
    /// board order. A single O(N) scan of the relevant candidate universe.
    pub candidates: Vec<Target>,
}
