//! Legal action enumeration — the engine's legality authority.
//!
//! [`Action`] is the closed set of things a player may take; [`valid_actions`]
//! computes, pull-based, exactly which are legal for the current priority
//! holder. [`crate::apply_action`] validates a chosen action against this set —
//! and, for a targeted action, against freshly computed legal target sets — in
//! [`action_is_legal`] before applying it.

use crate::ability::{Ability, Cost, Effect, Target, TargetSpec};
use crate::card::{abilities_of, CardData};
use crate::card_type::CardType;
use crate::combat::{
    attacker_candidates, attackers_needing_damage_order, attacking_defender_of,
    blocker_can_block_attacker, blocker_candidates_for, declared_attackers, defender_candidates,
    pending_blocker_declarer, pending_damage_order,
};
use crate::id::{CardId, CardInstance, PermanentId, PlayerId};
use crate::mana::parse_mana_cost;
use crate::phase::Step;
use crate::player::MAX_HAND_SIZE;
use crate::resolve::target_is_legal;
use crate::state::{GameState, Permanent};
use crate::CardDatabase;

/// An action a player may take. The engine generates the legal set with
/// [`valid_actions`] and validates a chosen action against it in
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
        /// Index into the permanent's abilities (see [`abilities_of`]).
        index: usize,
        /// The targets chosen for this activation, one per target slot the
        /// ability's effects declare (see [`Effect::target_spec`]), in that
        /// order. Empty for an ability that targets nothing.
        ///
        /// This is the **parameterized targeted representation** (ADR 0009
        /// §Enumeration): a single [`Action`] value *carries* the player's target
        /// selection, rather than the generator pre-expanding one variant per
        /// legal target combination. [`valid_actions`] advertises the action once
        /// in its *requirement* form (this field empty); the legal candidates for
        /// each slot come from [`target_requirements`], and a filled-in selection
        /// is validated slot-by-slot in [`action_is_legal`].
        targets: Vec<Target>,
    },
    /// Cast a spell from hand, paying its mana cost from the caster's pool.
    CastSpell {
        /// The specific card in the caster's hand to cast. Names the physical
        /// copy, so two identical cards in hand are distinguishable.
        card: CardInstance,
        /// The targets chosen for this cast, one per target slot the card's spell
        /// effects declare (see [`Effect::target_spec`]), in that order. Empty for
        /// a spell that targets nothing.
        ///
        /// The same **parameterized targeted representation** an ability uses (ADR
        /// 0009 §Enumeration): the [`Action`] carries the player's selection (CR
        /// 601.2c — targets are chosen as part of casting), rather than the
        /// generator pre-expanding one variant per legal target combination.
        /// [`valid_actions`] advertises the cast once in its requirement form (this
        /// field empty); per-slot candidates come from [`target_requirements`], and
        /// a filled selection is validated slot-by-slot in [`action_is_legal`].
        targets: Vec<Target>,
    },
    /// Discard one card from hand to satisfy the cleanup step's maximum-hand-size
    /// turn-based action (CR 514.1). Offered — one per card in the active
    /// player's hand, a select-from-zone choice — only while that player is over
    /// [`MAX_HAND_SIZE`](crate::MAX_HAND_SIZE) during [`Step::Cleanup`]. Names the
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
    /// (empty for a first-hand keep). [`valid_actions`] advertises this in its
    /// *requirement* form (empty `bottom`); a filled-in selection is validated in
    /// [`action_is_legal`]. Offered only in the mulligan phase, to the deciding
    /// seat.
    Keep {
        /// The chosen cards to put on the bottom of the library, in the order they
        /// are placed there. Exactly one [`Target::Card`] per mulligan taken, each
        /// naming a distinct card currently in the deciding seat's hand.
        bottom: Vec<Target>,
    },
    /// Declare the active player's attackers (CR 508.1), the turn-based player
    /// choice of the declare-attackers step. Each named permanent must be a legal
    /// attacker candidate ([`attacker_candidates`]); an **empty** selection is
    /// legal — declaring no attackers (CR 508.1a). Applying it taps each attacker
    /// (no vigilance yet) and moves the step into its priority round.
    ///
    /// Like [`Action::ActivateAbility`]'s targets, this is a *parameterized*
    /// multi-select: [`valid_actions`] advertises the action once in its empty
    /// requirement form, the legal candidates come from [`attacker_candidates`],
    /// and a filled-in selection is validated against that fresh set in
    /// [`action_is_legal`] — never pre-expanded into one action per subset.
    DeclareAttackers {
        /// The declared attacks: each names one attacker and the defending player
        /// it attacks (CR 508.1a). In a two-player game the only legal defender is
        /// the sole opponent; with more seats each attacker chooses among the
        /// opponents still in the game ([`defender_candidates`]).
        attackers: Vec<Attack>,
    },
    /// Declare the defending player's blockers (CR 509.1), the turn-based player
    /// choice of the declare-blockers step. Each [`Block`] assigns one eligible
    /// blocker ([`blocker_candidates`]) to one attacking creature
    /// ([`declared_attackers`]); several blockers may share an attacker, but a
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
    /// validated in [`action_is_legal`].
    OrderCombatDamage {
        /// One blocker ordering per multi-blocked attacker.
        orders: Vec<DamageOrder>,
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
    /// The creature declared as an attacker (an [`attacker_candidates`] member).
    pub attacker: PermanentId,
    /// The defending player it attacks (a [`defender_candidates`] member).
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
    /// The creature declared as a blocker (a [`blocker_candidates`] member).
    pub blocker: PermanentId,
    /// The attacking creature it is assigned to block (a [`declared_attackers`]
    /// member).
    pub attacker: PermanentId,
}

impl Action {
    /// The chosen targets carried by this action, in slot order; empty for an
    /// action that carries none.
    fn targets(&self) -> &[Target] {
        match self {
            Action::ActivateAbility { targets, .. } | Action::CastSpell { targets, .. } => targets,
            // `Keep::bottom` is a mulligan sub-choice, not a target selection; it
            // is validated through the mulligan path, never this one.
            Action::PassPriority
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
            // Concede carries no selection.
            | Action::Concede => &[],
        }
    }

    /// This action with its chosen targets cleared — its *requirement* form, the
    /// shape [`valid_actions`] advertises. Target-carrying variants drop their
    /// selection; every other variant is returned unchanged.
    fn without_targets(&self) -> Action {
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
            // The mulligan keep's bottom selection is cleared the same way, so its
            // requirement form matches what [`valid_actions`] advertises.
            Action::Keep { .. } => Action::Keep { bottom: Vec::new() },
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

/// One target slot of a targeted [`Action`]: the [`TargetSpec`] that constrains
/// the slot together with the *set* of [`Target`]s currently legal for it.
///
/// This is the unit [`target_requirements`] advertises per slot — the "target
/// requirement plus the set of legal targets" of ADR 0009 §Enumeration. The
/// candidate set is O(N) in that slot's candidate count; see the combinatorial
/// guard on [`target_requirements`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetRequirement {
    /// What the slot may target.
    pub spec: TargetSpec,
    /// Every [`Target`] legal for the slot against current state, in a stable
    /// board order. A single O(N) scan of the relevant candidate universe.
    pub candidates: Vec<Target>,
}

/// Enumerate the actions legal for the player who currently holds priority.
///
/// Pull-based and pure: computed fresh from `state`, never cached on it. The
/// priority holder may always pass; may play a land, cast a spell at its legal
/// timing (instants any time they hold priority, everything else at sorcery
/// speed — CR 117.1a), or (for permanents they control) activate abilities when
/// the relevant timing and cost conditions hold. A state with no valid priority
/// holder offers nothing.
///
/// A targeted ability is advertised **once**, in its requirement form (empty
/// [`Action::ActivateAbility::targets`]); its per-slot legal candidate sets are
/// obtained separately via [`target_requirements`]. The generator therefore
/// never pre-expands one action per target combination (ADR 0009 §Enumeration).
#[must_use]
pub fn valid_actions(state: &GameState, db: &CardDatabase) -> Vec<Action> {
    if state.priority_holder().is_none() {
        return Vec::new();
    }
    // CR 104.2a: once the game is over nothing is legal — the terminal state offers
    // no actions and [`crate::apply_action`] rejects any that are submitted.
    if state.is_over() {
        return Vec::new();
    }
    let priority = state.priority;

    // Pre-game London mulligan (CR 103.5): while the mulligan phase is in progress
    // the only choices are the deciding seat's keep/mulligan, and turn 1 has not
    // begun — no lands, spells, abilities, or priority passes are offered until
    // every player has kept (see [`crate::mulligan`]). Concede (CR 104.3a) is still
    // offered — a player may leave even during the mulligan.
    if let Some(mut actions) = crate::mulligan::mulligan_actions(state) {
        offer_concede(&mut actions);
        return actions;
    }

    // Cleanup step: no player receives priority (CR 514.3). The only choice is
    // the active player discarding down to the maximum hand size (CR 514.1),
    // offered as a select-from-zone choice — one [`Action::Discard`] per card in
    // hand — and only while they are over the limit. Everything else (passing,
    // lands, spells, abilities) is unavailable here — except conceding (CR 104.3a).
    if state.step == Step::Cleanup {
        let mut actions = Vec::new();
        if priority == state.active_player {
            if let Some(player) = state.players.get(priority.0) {
                if player.hand.len() > MAX_HAND_SIZE {
                    for &card in &player.hand {
                        actions.push(Action::Discard { card });
                    }
                    offer_concede(&mut actions);
                }
            }
        }
        return actions;
    }

    // Combat declarations are turn-based player choices, offered like the cleanup
    // discard rather than taken with priority: while a declaration is owed, the
    // declaring player's only action is the declaration itself (no pass, no
    // spells), and no other player acts. The declaration is advertised once in its
    // empty requirement form; its multi-select candidates come from
    // [`attacker_candidates`] / [`blocker_candidates`] (see [`target_requirements`]
    // for how the requirement is surfaced) and a filled selection is checked in
    // [`action_is_legal`].
    if state.step == Step::DeclareAttackers && !state.attackers_declared {
        // CR 508.1: the active player declares attackers.
        return if priority == state.active_player {
            let mut actions = vec![Action::DeclareAttackers {
                attackers: Vec::new(),
            }];
            offer_concede(&mut actions);
            actions
        } else {
            Vec::new()
        };
    }
    if state.step == Step::DeclareBlockers && pending_blocker_declarer(state).is_some() {
        // CR 509.1: each attacked player declares blockers for the attackers
        // attacking them, in APNAP order (issue #344). Only the player who owes the
        // next declaration is offered it.
        return if Some(priority) == pending_blocker_declarer(state) {
            let mut actions = vec![Action::DeclareBlockers { blocks: Vec::new() }];
            offer_concede(&mut actions);
            actions
        } else {
            Vec::new()
        };
    }
    if state.step == Step::DeclareBlockers && pending_damage_order(state).is_some() {
        // CR 510.1 (issue #346): once every blocker declaration is in, the attacking
        // player orders each multi-blocked attacker's blockers before combat damage.
        return if Some(priority) == pending_damage_order(state) {
            let mut actions = vec![Action::OrderCombatDamage { orders: Vec::new() }];
            offer_concede(&mut actions);
            actions
        } else {
            Vec::new()
        };
    }

    let mut actions = vec![Action::PassPriority];

    // Sorcery-speed: the active player, in a main phase, with an empty stack.
    let sorcery_speed = priority == state.active_player
        && matches!(state.step, Step::PrecombatMain | Step::PostcombatMain)
        && state.stack.is_empty();

    if let Some(player) = state.players.get(priority.0) {
        // Play a land: at sorcery speed, one per turn.
        if sorcery_speed && !state.land_played {
            for &card in &player.hand {
                if is_land(db, card.card) {
                    actions.push(Action::PlayLand { card });
                }
            }
        }

        // Cast a spell from hand payable from the current pool, at the correct
        // timing. A land is played, not cast (CR 116.2a); every other card type
        // is cast as a spell. An instant may be cast whenever its controller has
        // priority (CR 117.1a); every other spell — sorcery (CR 304.1), artifact,
        // enchantment (CR 307.1), creature — is bound by the sorcery-speed gate
        // above (the active player, a main phase, an empty stack). Only a cost
        // payable from the current pool ([`ManaPool::can_pay`]) is offered.
        for &card in &player.hand {
            let Some(data) = db.card(card.card) else {
                continue;
            };
            if !is_castable_spell(data) {
                continue;
            }
            // CR 117.1a: an instant ignores the sorcery-speed gate; every other
            // spell is bound by it.
            let timing_ok = data.has_type(CardType::Instant) || sorcery_speed;
            if timing_ok && player.mana_pool.can_pay(&parse_mana_cost(&data.mana_cost)) {
                // A targeted spell is offered only when *every* target slot has at
                // least one legal candidate (CR 601.2c — a spell that can't choose
                // legal targets can't be cast; for an Aura this is the CR 303.4c
                // "no legal object to enchant" rule). A slot's candidates come from
                // the same per-slot enumeration abilities use, so this stays O(N)
                // per slot and never forms the cartesian product.
                let castable = data
                    .cast_target_specs()
                    .into_iter()
                    .all(|spec| !legal_targets_for_spec(spec, state, db).is_empty());
                if castable {
                    actions.push(Action::CastSpell {
                        card,
                        targets: Vec::new(),
                    });
                }
            }
        }
    }

    // Activate abilities of permanents the priority holder controls. A targeting
    // ability is offered once with no targets filled in — the requirement form —
    // never once per legal target (see [`target_requirements`] for the O(N)-per-
    // slot candidate enumeration and the combinatorial guard).
    for perm in &state.battlefield {
        if perm.controller != priority {
            continue;
        }
        for (index, ability) in abilities_of(db, perm.card).iter().enumerate() {
            if let Ability::Activated { cost, .. } = ability {
                if cost_payable(cost, perm) {
                    actions.push(Action::ActivateAbility {
                        permanent: perm.id,
                        index,
                        targets: Vec::new(),
                    });
                }
            }
        }
    }

    offer_concede(&mut actions);
    actions
}

/// Append the always-available concede action (CR 104.3a) to `actions`. Called at
/// every point [`valid_actions`] returns a non-empty offer to the acting seat, so
/// a player may leave the game regardless of phase, step, or which special choice
/// is currently owed.
fn offer_concede(actions: &mut Vec<Action>) {
    actions.push(Action::Concede);
}

/// The ordered target requirements of `action` against the current `state`: one
/// [`TargetRequirement`] per target slot the action must fill, each carrying the
/// legal candidate set for that slot. Empty for an action that targets nothing.
///
/// # Combinatorial guard (ADR 0009 §Enumeration)
///
/// This builds one candidate **set per slot**: its cost is the *sum* of the
/// per-slot candidate counts — O(N) for a single slot over N candidates. It
/// never forms the *cartesian product* of the slots (which would be O(Nᵏ) for k
/// slots of N candidates each), so advertising a targeted action stays linear in
/// board size per slot. This is exactly the "core complexity" ADR 0002 flagged:
/// legal-set enumeration must be per-slot, not per-combination. A caller
/// assembles a concrete selection by picking one candidate from each slot; the
/// engine validates that assembled selection in [`action_is_legal`] without ever
/// materializing the product.
#[must_use]
pub fn target_requirements(
    state: &GameState,
    db: &CardDatabase,
    action: &Action,
) -> Vec<TargetRequirement> {
    action_target_specs(state, db, action)
        .into_iter()
        .map(|spec| TargetRequirement {
            spec,
            candidates: legal_targets_for_spec(spec, state, db),
        })
        .collect()
}

/// Whether `action` — including any targets it carries — is legal against the
/// current `state`. This is the gate [`crate::apply_action`] runs before it
/// applies anything.
///
/// Two independent checks, mirroring ADR 0009 §Enumeration:
/// 1. **Base legality.** The action, with its targets cleared to the requirement
///    form, must be one [`valid_actions`] currently offers.
/// 2. **Target legality.** The carried targets must exactly fill the action's
///    slots, and each must lie in that slot's *freshly computed* legal set. This
///    extends the regenerate-and-check discipline of [`crate::apply_action`] to
///    targets: legality is re-derived from current state, never read back from an
///    exhaustively enumerated list of target combinations.
#[must_use]
pub(crate) fn action_is_legal(state: &GameState, action: &Action, db: &CardDatabase) -> bool {
    // 1. The bare action must be on offer. Comparing the requirement form keeps
    //    this O(number of distinct actions), independent of how many targets each
    //    could take — no combination is ever enumerated here.
    if !valid_actions(state, db).contains(&action.without_targets()) {
        return false;
    }

    // 1a. A mulligan keep validates its bottoming selection (CR 103.5) rather than
    //     the target-slot machinery: exactly one distinct hand card per mulligan
    //     taken (see [`crate::mulligan::keep_bottom_is_legal`]).
    if let Action::Keep { bottom } = action {
        return crate::mulligan::keep_bottom_is_legal(state, bottom);
    }

    // 1b. Combat declarations carry a permanent multi-select rather than
    //     ability targets: validate the selection against the freshly computed
    //     candidate sets (CR 508.1a / 509.1a), the same regenerate-and-check
    //     discipline the target path uses. An empty selection is always legal.
    match action {
        Action::DeclareAttackers { attackers } => {
            return attackers_selection_is_legal(state, db, attackers);
        }
        Action::DeclareBlockers { blocks } => {
            return blocks_selection_is_legal(state, db, blocks);
        }
        Action::OrderCombatDamage { orders } => {
            return damage_orders_are_legal(state, orders);
        }
        _ => {}
    }

    // 2. The carried targets must fill every slot the action declares, each with
    //    a target that is legal *now*. `target_is_legal` is the same predicate the
    //    resolve path re-checks with (CR 608.2b) and the one `legal_targets_for_spec`
    //    filters by, so "in the freshly computed legal set" and "passes the check"
    //    are one and the same — we test membership directly, without building the
    //    set (and certainly without the cartesian product).
    let specs = action_target_specs(state, db, action);
    let chosen = action.targets();
    chosen.len() == specs.len()
        && specs
            .iter()
            .zip(chosen)
            .all(|(&spec, &target)| target_is_legal(spec, target, state, db))
}

/// The ordered [`TargetSpec`]s `action` must be given a target for — one per
/// targeting effect the action declares, in resolution order. Empty for an action
/// with no targeting effects (or one the state cannot resolve).
///
/// An [`Action::ActivateAbility`] reads its activated ability's effects; an
/// [`Action::CastSpell`] reads the cast card's cast target specs
/// ([`CardData::cast_target_specs`]) — the spell-effect target slots plus, for an
/// Aura, its enchant restriction (CR 303.4a) — so a spell chooses targets exactly
/// as an ability does (CR 601.2c). Every other action targets nothing.
fn action_target_specs(state: &GameState, db: &CardDatabase, action: &Action) -> Vec<TargetSpec> {
    match action {
        Action::ActivateAbility {
            permanent, index, ..
        } => {
            let Some(perm) = state.battlefield.iter().find(|p| p.id == *permanent) else {
                return Vec::new();
            };
            let abilities = abilities_of(db, perm.card);
            let Some(Ability::Activated { effects, .. }) = abilities.get(*index) else {
                return Vec::new();
            };
            effects.iter().filter_map(Effect::target_spec).collect()
        }
        Action::CastSpell { card, .. } => db
            .card(card.card)
            .map(CardData::cast_target_specs)
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// The set of [`Target`]s legal for `spec` against current `state`, as a single
/// O(N) pass over the candidate universe the spec names.
///
/// Defined *in terms of* [`target_is_legal`] — the candidate universe is filtered
/// by that same predicate — so an object is in this set exactly when it would
/// pass the resolution-time re-check. Building this list is the per-slot cost the
/// combinatorial guard on [`target_requirements`] bounds; nothing here multiplies
/// slots together.
fn legal_targets_for_spec(spec: TargetSpec, state: &GameState, db: &CardDatabase) -> Vec<Target> {
    let universe: Vec<Target> = match spec {
        TargetSpec::AnyPlayer => (0..state.players.len())
            .map(|seat| Target::Player(PlayerId(seat)))
            .collect(),
        TargetSpec::AnyPermanent | TargetSpec::AnyCreature => state
            .battlefield
            .iter()
            .map(|perm| Target::Permanent(perm.id))
            .collect(),
        // "Any target" (CR 115.4): players and battlefield permanents together;
        // the `target_is_legal` filter below keeps only creatures and in-game
        // players, so a non-creature permanent never survives it.
        TargetSpec::AnyTarget => (0..state.players.len())
            .map(|seat| Target::Player(PlayerId(seat)))
            .chain(
                state
                    .battlefield
                    .iter()
                    .map(|perm| Target::Permanent(perm.id)),
            )
            .collect(),
        // Only spells on the stack are candidates — abilities are not spells, and
        // mana abilities never use the stack (CR 605.3), so neither can be a
        // "counter target spell" candidate.
        TargetSpec::SpellOnStack => state
            .stack
            .iter()
            .filter(|o| matches!(o.kind, crate::stack::StackObjectKind::Spell { .. }))
            .map(|o| Target::Spell(o.id))
            .collect(),
    };
    universe
        .into_iter()
        .filter(|&target| target_is_legal(spec, target, state, db))
        .collect()
}

/// Whether a declared attacker selection is legal (CR 508.1a): every named
/// permanent is a current attacker candidate ([`attacker_candidates`]), no
/// permanent is named twice, and every attacker's defender is a legal defender
/// candidate ([`defender_candidates`]) — an opponent still in the game, never the
/// active player and never an eliminated one. An empty selection is legal
/// (declaring no attackers).
fn attackers_selection_is_legal(
    state: &GameState,
    db: &CardDatabase,
    attackers: &[Attack],
) -> bool {
    let candidates = attacker_candidates(state, db);
    let defenders = defender_candidates(state);
    let ids: Vec<PermanentId> = attackers.iter().map(|a| a.attacker).collect();
    all_unique(&ids)
        && attackers
            .iter()
            .all(|a| candidates.contains(&a.attacker) && defenders.contains(&a.defender))
}

/// Whether a declared blocker selection is legal (CR 509.1a): every blocker is a
/// current blocker candidate of the player who owes this declaration
/// ([`blocker_candidates_for`] the [`pending_blocker_declarer`]), every named
/// attacker is currently attacking ([`declared_attackers`]) *and attacking that
/// player* (CR 509.1a — a player blocks only attackers attacking them), no creature
/// is declared as a blocker more than once, and each blocker can legally block the
/// attacker it is assigned to given evasion keywords — a flyer can be blocked only
/// by flying or reach (CR 702.9c, 702.17b, via [`blocker_can_block_attacker`]). An
/// empty selection is legal (declaring no blockers).
///
/// Scoping to the current declarer is what makes the multi-defender flow (issue
/// #344) safe: each attacked player's declaration is validated against exactly
/// their own creatures and the attackers attacking them. Two-player games are
/// unchanged — the sole opponent is the one declarer.
///
/// Evasion is checked per assignment rather than by trimming the candidate set, so
/// a partial block of a mix of flying and ground attackers stays expressible: a
/// ground creature may still block the ground attacker in the same declaration
/// that a flyer blocks the flyer.
fn blocks_selection_is_legal(state: &GameState, db: &CardDatabase, blocks: &[Block]) -> bool {
    let Some(declarer) = pending_blocker_declarer(state) else {
        // No declaration is owed: only the empty selection is vacuously legal.
        return blocks.is_empty();
    };
    let blockers = blocker_candidates_for(state, declarer, db);
    let attackers = declared_attackers(state);
    let assigned: Vec<PermanentId> = blocks.iter().map(|b| b.blocker).collect();
    all_unique(&assigned)
        && blocks.iter().all(|b| {
            blockers.contains(&b.blocker)
                && attackers.contains(&b.attacker)
                // CR 509.1a: the declaring player may block only attackers attacking
                // *them*, so the attacker's chosen defender must be this declarer.
                && attacking_defender_of(state, b.attacker) == Some(declarer)
                && blocker_can_block_attacker(state, b.attacker, b.blocker, db)
        })
}

/// Whether a combat-damage assignment order selection is legal (CR 510.1, issue
/// #346): it names exactly the attackers that owe an order
/// ([`attackers_needing_damage_order`]), each with a permutation of that attacker's
/// own blockers — no missing, extra, duplicated, or foreign blocker. An empty
/// selection is legal only when no attacker owes an order (the choice-free case).
fn damage_orders_are_legal(state: &GameState, orders: &[DamageOrder]) -> bool {
    let mut owed = attackers_needing_damage_order(state);
    // Exactly the owed attackers, once each.
    let named: Vec<PermanentId> = orders.iter().map(|o| o.attacker).collect();
    if !all_unique(&named) {
        return false;
    }
    let mut named_sorted = named.clone();
    named_sorted.sort_by_key(|id| id.0);
    owed.sort_by_key(|id| id.0);
    if named_sorted != owed {
        return false;
    }
    // Each order is a permutation of exactly that attacker's blockers.
    orders.iter().all(|order| {
        let mut declared: Vec<PermanentId> = state
            .battlefield
            .iter()
            .filter(|p| p.blocking == Some(order.attacker))
            .map(|p| p.id)
            .collect();
        let mut chosen = order.blockers.clone();
        declared.sort_by_key(|id| id.0);
        chosen.sort_by_key(|id| id.0);
        all_unique(&order.blockers) && chosen == declared
    })
}

/// Whether every element of `ids` is distinct. O(n²), which is fine for the
/// handful of creatures a combat declaration ever names and keeps the engine free
/// of a hashing dependency for a tiny list.
fn all_unique(ids: &[PermanentId]) -> bool {
    ids.iter().enumerate().all(|(i, id)| !ids[..i].contains(id))
}

/// Whether every cost in `cost` is payable given the source `permanent`'s state.
fn cost_payable(cost: &[Cost], permanent: &Permanent) -> bool {
    cost.iter().all(|c| match c {
        Cost::Tap => !permanent.tapped,
    })
}

/// Whether `card` is a land, by its structured printed types.
fn is_land(db: &CardDatabase, card: CardId) -> bool {
    db.card(card).is_some_and(|c| c.has_type(CardType::Land))
}

/// Whether `card` may be cast as a spell from hand today (CR 117.1a).
///
/// A land is never cast — it is played as a special action (CR 116.2a) and is
/// offered separately. Every other card type — instant, sorcery, artifact,
/// enchantment (Auras included, since issue #152), creature — is castable, subject
/// to timing and cost checked by the caller. An Aura additionally requires a legal
/// enchant target to be *offered* (CR 303.4c/601.2c); that is enforced by the
/// per-slot candidate check in [`valid_actions`] over [`CardData::cast_target_specs`],
/// not here.
fn is_castable_spell(data: &CardData) -> bool {
    !data.has_type(CardType::Land)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::apply_action;
    use crate::fixtures::{fixture, id_in};
    use crate::id::CardInstanceId;
    use crate::mana::{Color, ManaPool};
    use crate::stack::{StackId, StackObject, StackObjectKind};

    /// The bundled card database, for tests that need oracle data.
    fn db() -> CardDatabase {
        CardDatabase::bundled().unwrap()
    }

    /// A database with a "Tapper" artifact whose activated ability is
    /// `{T}: Tap target creature.` and a vanilla "Bear" creature to target.
    fn targeting_db() -> CardDatabase {
        let json = r#"[
            {"schema_version":1,"functional_id":"tapper","name":"Tapper","types":["artifact"],"mana_cost":"",
             "abilities":[{"type":"activated","cost":[{"kind":"tap"}],
                          "effects":[{"kind":"tap","target":"any_creature"}]}]},
            {"schema_version":1,"functional_id":"bear","name":"Bear","types":["creature"],"mana_cost":"",
             "power":2,"toughness":2}
        ]"#;
        CardDatabase::from_json(json).unwrap()
    }

    /// Put a permanent of printed card `card` onto the battlefield under player
    /// 0's control (untapped) and return its fresh [`PermanentId`].
    fn put_on_battlefield(state: &mut GameState, card: CardId) -> PermanentId {
        let inst = state.new_instance(card);
        let id = state.mint_id();
        state.battlefield.push(Permanent {
            id: PermanentId(id),
            instance: inst.id,
            card,
            controller: PlayerId(0),
            tapped: false,
            entered_turn: 0,
            attacking: None,
            blocking: None,
            damage: 0,
            counters: Default::default(),
            attached_to: None,
        });
        PermanentId(id)
    }

    /// A two-player game at precombat main with a Tapper and `creatures` Bears on
    /// the battlefield under player 0. Returns the state, the Tapper's id, and the
    /// Bears' ids.
    fn tapper_and_creatures(creatures: usize) -> (GameState, PermanentId, Vec<PermanentId>) {
        let db = targeting_db();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let tapper = put_on_battlefield(&mut state, id_in(&db, "tapper"));
        let bears = (0..creatures)
            .map(|_| put_on_battlefield(&mut state, id_in(&db, "bear")))
            .collect();
        (state, tapper, bears)
    }

    #[test]
    fn valid_actions_offers_pass_priority_to_the_priority_holder() {
        let state = GameState::new_two_player();
        // Pass plus the always-available concede (CR 104.3a).
        assert_eq!(
            valid_actions(&state, &db()),
            vec![Action::PassPriority, Action::Concede]
        );
    }

    #[test]
    fn issue_119_concede_is_always_offered_cr_104_3a() {
        // CR 104.3a: a player may concede at any time. Concede is offered to the
        // acting seat in every window the engine surfaces a choice — a normal
        // priority round and the special turn-based choices alike.
        let db = db();

        // Normal priority round (untap).
        let state = GameState::new_two_player();
        assert!(valid_actions(&state, &db).contains(&Action::Concede));

        // Declare-attackers window: only the declaration and concede are offered.
        let mut combat = GameState::new_two_player();
        combat.turn = 2;
        combat.step = Step::DeclareAttackers;
        let offered = valid_actions(&combat, &db);
        assert!(offered.contains(&Action::Concede));
        assert!(!offered.contains(&Action::PassPriority));
    }

    #[test]
    fn issue_119_terminal_state_offers_no_actions_cr_104_2a() {
        // CR 104.2a: once a player has lost and one remains, the game is over and
        // no actions are legal — not even concede.
        let mut state = GameState::new_two_player();
        state.players[1].has_lost = true;
        assert!(state.is_over());
        assert!(valid_actions(&state, &db()).is_empty());
    }

    #[test]
    fn valid_actions_on_seatless_state_is_empty() {
        // Default has no players, so no one holds priority and nothing is legal.
        assert!(valid_actions(&GameState::default(), &db()).is_empty());
    }

    #[test]
    fn targeted_ability_is_advertised_once_with_an_o_n_candidate_set() {
        // A "tap target creature" ability over N creatures is advertised as a
        // SINGLE ActivateAbility (requirement form, no targets), plus one target
        // slot whose candidate set lists all N creatures. The action count is
        // O(1) in the creatures and the candidate count is O(N) — never the O(N^k)
        // cartesian product a per-combination enumeration would produce.
        let db = targeting_db();
        let n = 5;
        let (state, tapper, bears) = tapper_and_creatures(n);

        let actions = valid_actions(&state, &db);
        let activations: Vec<&Action> = actions
            .iter()
            .filter(|a| matches!(a, Action::ActivateAbility { .. }))
            .collect();
        assert_eq!(activations.len(), 1, "one action, not one per target");
        assert_eq!(
            activations[0],
            &Action::ActivateAbility {
                permanent: tapper,
                index: 0,
                targets: Vec::new(),
            }
        );

        let reqs = target_requirements(&state, &db, activations[0]);
        assert_eq!(reqs.len(), 1, "one target slot");
        assert_eq!(reqs[0].spec, TargetSpec::AnyCreature);
        assert_eq!(reqs[0].candidates.len(), n, "O(N) candidates for the slot");
        for bear in &bears {
            assert!(reqs[0].candidates.contains(&Target::Permanent(*bear)));
        }
        // The Tapper is a permanent but not a creature, so it is not a candidate.
        assert!(!reqs[0].candidates.contains(&Target::Permanent(tapper)));
    }

    #[test]
    fn a_legal_target_is_accepted_and_carried_onto_the_stack() {
        // Activating with a legal creature target puts the ability on the stack
        // carrying exactly that chosen target (CR 601.2c), and resolving it taps
        // that creature.
        let db = targeting_db();
        let (state, tapper, bears) = tapper_and_creatures(2);
        let target = Target::Permanent(bears[0]);

        let after = apply_action(
            &state,
            &Action::ActivateAbility {
                permanent: tapper,
                index: 0,
                targets: vec![target],
            },
            &db,
        );

        // The ability is on the stack with the chosen target recorded on it.
        assert_eq!(after.stack.len(), 1);
        assert!(matches!(
            after.stack[0].kind,
            StackObjectKind::Ability { source, .. } if source == tapper
        ));
        assert_eq!(after.stack[0].targets, vec![target]);

        // Resolving (both players pass) taps exactly the targeted creature.
        let after = apply_action(&after, &Action::PassPriority, &db);
        let after = apply_action(&after, &Action::PassPriority, &db);
        assert!(after.stack.is_empty());
        assert!(
            after
                .battlefield
                .iter()
                .find(|p| p.id == bears[0])
                .unwrap()
                .tapped
        );
        assert!(
            !after
                .battlefield
                .iter()
                .find(|p| p.id == bears[1])
                .unwrap()
                .tapped
        );
    }

    #[test]
    fn an_illegal_target_makes_the_activation_a_no_op() {
        // A target of the right *kind* but outside the legal set — here the
        // Tapper itself, a permanent that is not a creature — is not in the freshly
        // computed legal set, so the whole activation is rejected as a no-op
        // (nothing on the stack, no cost paid), exactly as an illegal action.
        let db = targeting_db();
        let (state, tapper, _bears) = tapper_and_creatures(1);

        let after = apply_action(
            &state,
            &Action::ActivateAbility {
                permanent: tapper,
                index: 0,
                targets: vec![Target::Permanent(tapper)],
            },
            &db,
        );
        assert_eq!(after, state);
    }

    #[test]
    fn a_stale_target_makes_the_activation_a_no_op() {
        // A target that named a real candidate which has since left the battlefield
        // is no longer in the legal set recomputed from current state, so the
        // activation is a no-op — the stale id can never rebind to a live object.
        let db = targeting_db();
        let (mut state, tapper, bears) = tapper_and_creatures(1);
        let stale = Target::Permanent(bears[0]);
        // The creature is gone by the time the activation is submitted.
        state.battlefield.retain(|p| p.id != bears[0]);

        let after = apply_action(
            &state,
            &Action::ActivateAbility {
                permanent: tapper,
                index: 0,
                targets: vec![stale],
            },
            &db,
        );
        assert_eq!(after, state);
    }

    #[test]
    fn a_targeting_activation_with_no_target_is_a_no_op() {
        // The requirement form (no targets) is what `valid_actions` advertises,
        // but it is not a legal *submission* for an ability that requires a target:
        // the slot count must match, so an unfilled selection is rejected.
        let db = targeting_db();
        let (state, tapper, _bears) = tapper_and_creatures(1);

        let after = apply_action(
            &state,
            &Action::ActivateAbility {
                permanent: tapper,
                index: 0,
                targets: Vec::new(),
            },
            &db,
        );
        assert_eq!(after, state);
    }

    #[test]
    fn a_non_targeting_ability_needs_no_targets_and_still_resolves() {
        // A mana ability declares no target specs, so its activation validates with
        // an empty selection and is unaffected by this machinery.
        let db = db();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let forest = put_on_battlefield(&mut state, fixture("forest"));

        assert!(target_requirements(
            &state,
            &db,
            &Action::ActivateAbility {
                permanent: forest,
                index: 0,
                targets: Vec::new(),
            },
        )
        .is_empty());

        let after = apply_action(
            &state,
            &Action::ActivateAbility {
                permanent: forest,
                index: 0,
                targets: Vec::new(),
            },
            &db,
        );
        assert_eq!(after.players[0].mana_pool.green, 1);
    }

    // ----- Cast every card type with real timing (issue #147) -----
    //
    // These tests exercise casting *timing* and stack mechanics per card type, so they
    // must not depend on any bundled card's behavior. Bundled cards gain real effects
    // over time (e.g. issue #256 wired the four formerly-functionless cards these once
    // borrowed), which would silently change what a "generic instant" does — or, for a
    // now-targeted spell, make it uncastable with no target. A minimal synthetic catalog
    // gives one no-effect card of each castable type, immune to that drift.
    fn probe_db() -> CardDatabase {
        let json = r#"[
            {"schema_version":1,"functional_id":"probe_instant","name":"Probe Instant","types":["instant"],"mana_cost":"{R}","colors":["red"]},
            {"schema_version":1,"functional_id":"probe_sorcery","name":"Probe Sorcery","types":["sorcery"],"mana_cost":"{U}","colors":["blue"]},
            {"schema_version":1,"functional_id":"probe_artifact","name":"Probe Artifact","types":["artifact"],"mana_cost":"{1}"},
            {"schema_version":1,"functional_id":"probe_enchantment","name":"Probe Enchantment","types":["enchantment"],"mana_cost":"{G}","colors":["green"]},
            {"schema_version":1,"functional_id":"probe_creature","name":"Probe Creature","types":["creature"],"mana_cost":"{G}","colors":["green"],"power":2,"toughness":2}
        ]"#;
        CardDatabase::from_json(json).unwrap()
    }
    fn instant_id(db: &CardDatabase) -> CardId {
        id_in(db, "probe_instant")
    }
    fn sorcery_id(db: &CardDatabase) -> CardId {
        id_in(db, "probe_sorcery")
    }
    fn artifact_id(db: &CardDatabase) -> CardId {
        id_in(db, "probe_artifact")
    }
    fn enchantment_id(db: &CardDatabase) -> CardId {
        id_in(db, "probe_enchantment")
    }

    /// Whether `card` is offered as a [`Action::CastSpell`] for the hand instance
    /// `inst`.
    fn cast_offered(state: &GameState, db: &CardDatabase, inst: CardInstance) -> bool {
        valid_actions(state, db).contains(&Action::CastSpell {
            card: inst,
            targets: Vec::new(),
        })
    }

    /// A two-player game at player 0's precombat main with a single copy of
    /// `card` in player 0's hand and a mana pool generous enough to pay any of
    /// the single-pip fixture costs. Returns the state and the hand instance.
    fn hand_with(card: CardId) -> (GameState, CardInstance) {
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let inst = state.new_instance(card);
        state.players[0].hand = vec![inst];
        // One of each color plus colorless covers every fixture cost here.
        state.players[0].mana_pool.add(Color::Red, 1);
        state.players[0].mana_pool.add(Color::Blue, 1);
        state.players[0].mana_pool.add(Color::Green, 1);
        state.players[0].mana_pool.colorless = 1;
        (state, inst)
    }

    #[test]
    fn issue_147_instant_castable_with_a_nonempty_stack_and_off_turn_cr_117_1a() {
        // CR 117.1a: a player may cast an instant any time they have priority —
        // including while another spell waits on the stack (the first "respond to
        // a spell" path) and during an opponent's turn.
        let db = probe_db();

        // Another object already on the stack, player 0 holding priority.
        let (mut mid_stack, bolt) = hand_with(instant_id(&db));
        let sid = mid_stack.mint_id();
        let other = mid_stack.new_instance(instant_id(&db));
        mid_stack.stack.push(StackObject {
            id: StackId(sid),
            controller: PlayerId(0),
            kind: StackObjectKind::Spell { card: other },
            targets: Vec::new(),
        });
        assert!(!mid_stack.stack.is_empty());
        assert!(
            cast_offered(&mid_stack, &db, bolt),
            "an instant is castable with a spell on the stack (CR 117.1a)"
        );

        // Opponent's turn: player 1 is active, player 0 holds priority.
        let (mut off_turn, bolt) = hand_with(instant_id(&db));
        off_turn.active_player = PlayerId(1);
        off_turn.priority = PlayerId(0);
        assert!(
            cast_offered(&off_turn, &db, bolt),
            "an instant is castable on the opponent's turn (CR 117.1a)"
        );
    }

    #[test]
    fn issue_147_sorcery_not_offered_off_turn_or_mid_stack_cr_304_1() {
        // CR 304.1: a sorcery may be cast only at sorcery speed — the active
        // player, a main phase, an empty stack. It is offered in none of the
        // windows an instant is, only in that one.
        let db = probe_db();

        // Positive control: on-turn, empty stack, main phase — offered.
        let (on_turn, sorcery) = hand_with(sorcery_id(&db));
        assert!(cast_offered(&on_turn, &db, sorcery));

        // Off-turn (player 0 holds priority on player 1's turn) — not offered.
        let (mut off_turn, sorcery) = hand_with(sorcery_id(&db));
        off_turn.active_player = PlayerId(1);
        off_turn.priority = PlayerId(0);
        assert!(!cast_offered(&off_turn, &db, sorcery));

        // Mid-stack (own turn, but a spell is on the stack) — not offered.
        let (mut mid_stack, sorcery) = hand_with(sorcery_id(&db));
        let sid = mid_stack.mint_id();
        let other = mid_stack.new_instance(instant_id(&db));
        mid_stack.stack.push(StackObject {
            id: StackId(sid),
            controller: PlayerId(0),
            kind: StackObjectKind::Spell { card: other },
            targets: Vec::new(),
        });
        assert!(!cast_offered(&mid_stack, &db, sorcery));
    }

    #[test]
    fn issue_147_artifact_and_enchantment_cast_at_sorcery_speed_and_enter_battlefield() {
        // CR 307.1 (enchantments) / artifacts are permanent spells cast at
        // sorcery speed; on resolution they enter the battlefield (CR 608.3).
        let db = probe_db();
        for card in [artifact_id(&db), enchantment_id(&db)] {
            let (state, inst) = hand_with(card);

            // Offered at sorcery speed...
            assert!(
                cast_offered(&state, &db, inst),
                "a permanent spell is castable at sorcery speed"
            );
            // ...but not while a spell is on the stack (sorcery-speed gate).
            let mut mid_stack = state.clone();
            let sid = mid_stack.mint_id();
            let other = mid_stack.new_instance(instant_id(&db));
            mid_stack.stack.push(StackObject {
                id: StackId(sid),
                controller: PlayerId(0),
                kind: StackObjectKind::Spell { card: other },
                targets: Vec::new(),
            });
            assert!(!cast_offered(&mid_stack, &db, inst));

            // Cast it, then both players pass: it resolves onto the battlefield.
            let state = apply_action(
                &state,
                &Action::CastSpell {
                    card: inst,
                    targets: Vec::new(),
                },
                &db,
            );
            assert_eq!(state.stack.len(), 1);
            let state = apply_action(&state, &Action::PassPriority, &db);
            let state = apply_action(&state, &Action::PassPriority, &db);
            assert!(state.stack.is_empty());
            // The permanent spell entered the battlefield (CR 608.3).
            let perm = state.battlefield.iter().find(|p| p.card == card).unwrap();
            assert_eq!(perm.instance, inst.id, "keeps its instance identity");
        }
    }

    #[test]
    fn issue_147_cast_instant_resolves_after_a_later_instant_lifo_cr_608_1() {
        // CR 608.1: the stack resolves last-in, first-out. Cast instant A, then —
        // with A still on the stack — cast instant B. B is on top, so it resolves
        // first: the two non-permanent spells reach the graveyard in the order
        // B, A, the reverse of the order they were cast.
        let db = probe_db();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let a = state.new_instance(instant_id(&db));
        let b = state.new_instance(instant_id(&db));
        state.players[0].hand = vec![a, b];
        state.players[0].mana_pool.add(Color::Red, 2);

        // Cast A, then B (legal to respond because both are instants).
        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: a,
                targets: Vec::new(),
            },
            &db,
        );
        assert!(cast_offered(&state, &db, b));
        let state = apply_action(
            &state,
            &Action::CastSpell {
                card: b,
                targets: Vec::new(),
            },
            &db,
        );
        assert_eq!(state.stack.len(), 2);

        // Resolve the top (B), then the next (A).
        let state = apply_action(&state, &Action::PassPriority, &db);
        let state = apply_action(&state, &Action::PassPriority, &db);
        let state = apply_action(&state, &Action::PassPriority, &db);
        let state = apply_action(&state, &Action::PassPriority, &db);
        assert!(state.stack.is_empty());

        // LIFO: the later-cast B resolved first, so it is first in the graveyard.
        let grave: Vec<CardInstanceId> = state.players[0].graveyard.iter().map(|c| c.id).collect();
        assert_eq!(grave, vec![b.id, a.id]);
    }

    #[test]
    fn issue_147_unpayable_spells_are_never_offered() {
        // The unpayable-cost invariant extends to every new castable type: with an
        // empty pool, no instant/sorcery/artifact/enchantment is offered; with the
        // exact mana it is (sorcery-speed types on-turn with an empty stack).
        let db = probe_db();
        for card in [
            instant_id(&db),
            sorcery_id(&db),
            artifact_id(&db),
            enchantment_id(&db),
        ] {
            let (state, inst) = hand_with(card);

            // Drain the pool: nothing is payable, so nothing is offered.
            let mut broke = state.clone();
            broke.players[0].mana_pool = ManaPool::default();
            assert!(
                !cast_offered(&broke, &db, inst),
                "an unpayable spell is never offered"
            );
            // With mana in the pool it is offered.
            assert!(cast_offered(&state, &db, inst));
        }
    }

    #[test]
    fn issue_152_aura_castable_only_with_a_legal_enchant_target_cr_303_4c() {
        // CR 303.4c/601.2c: an Aura is offered only when a legal object to enchant
        // exists — its enchant restriction is a target chosen at cast. With a
        // creature on the battlefield the Aura is castable; with none it is not
        // (its one slot has zero candidates), even though its sorcery-speed timing
        // and mana are satisfied. A vanilla (non-Aura) enchantment of the same cost
        // is always castable, proving the gate is the enchant target, not the type.
        let json = r#"[
            {"schema_version":1,"functional_id":"test_aura","name":"Test Aura","types":["enchantment"],"subtypes":["Aura"],
             "mana_cost":"{G}",
             "aura":{"enchant":"any_creature","power":1,"toughness":1}},
            {"schema_version":1,"functional_id":"test_charm","name":"Test Charm","types":["enchantment"],
             "mana_cost":"{G}"},
            {"schema_version":1,"functional_id":"test_bear","name":"Test Bear","types":["creature"],"mana_cost":"",
             "power":2,"toughness":2}
        ]"#;
        let db = CardDatabase::from_json(json).unwrap();

        // No creature to enchant: the Aura is not offered, the charm still is.
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let aura = state.new_instance(id_in(&db, "test_aura"));
        let charm = state.new_instance(id_in(&db, "test_charm"));
        state.players[0].hand = vec![aura, charm];
        state.players[0].mana_pool.add(Color::Green, 2);
        assert!(
            !cast_offered(&state, &db, aura),
            "an Aura with no legal object to enchant is not offered (CR 303.4c)"
        );
        assert!(
            cast_offered(&state, &db, charm),
            "a non-Aura enchantment is castable with no creature present"
        );

        // Put a creature on the battlefield: now the Aura is offered, and its one
        // requirement slot is the enchant restriction listing that creature.
        let bear = put_on_battlefield(&mut state, id_in(&db, "test_bear"));
        assert!(
            cast_offered(&state, &db, aura),
            "an Aura is castable once a legal enchant target exists (CR 303.4c)"
        );
        let reqs = target_requirements(
            &state,
            &db,
            &Action::CastSpell {
                card: aura,
                targets: Vec::new(),
            },
        );
        assert_eq!(reqs.len(), 1, "the Aura's single enchant slot");
        assert_eq!(reqs[0].spec, TargetSpec::AnyCreature);
        assert_eq!(reqs[0].candidates, vec![Target::Permanent(bear)]);
    }

    // ----- Spell targets at cast + the first counterspell (issue #148) -----
    //
    // Runic Negation ({U} instant, "Counter target spell." — a
    // `CounterSpell { SpellOnStack }` spell effect); a vanilla Thornback Boar is the
    // creature spell it counters.
    fn counterspell_id() -> CardId {
        fixture("runic_negation")
    }
    fn creature_id() -> CardId {
        fixture("thornback_boar")
    }

    /// A two-player game at player 0's precombat main with a Runic Negation in
    /// hand and `{U}` in pool, and a creature spell (controlled by player 1) on
    /// the stack for it to target. Returns the state, the counterspell hand
    /// instance, and the creature spell's [`StackId`].
    fn counterspell_over_a_creature_spell() -> (GameState, CardInstance, StackId) {
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let boar = state.new_instance(creature_id());
        let sid = StackId(state.mint_id());
        state.stack.push(StackObject {
            id: sid,
            controller: PlayerId(1),
            kind: StackObjectKind::Spell { card: boar },
            targets: Vec::new(),
        });
        let negation = state.new_instance(counterspell_id());
        state.players[0].hand = vec![negation];
        state.players[0].mana_pool.add(Color::Blue, 1);
        (state, negation, sid)
    }

    #[test]
    fn issue_148_targeted_cast_advertised_once_with_the_spell_on_stack_as_a_candidate() {
        // A "counter target spell" cast is offered once in its requirement form
        // (empty targets), and its single slot lists exactly the spell on the
        // stack (CR 601.2c / 701.5).
        let db = db();
        let (state, negation, sid) = counterspell_over_a_creature_spell();

        let cast = Action::CastSpell {
            card: negation,
            targets: Vec::new(),
        };
        assert!(valid_actions(&state, &db).contains(&cast));

        let reqs = target_requirements(&state, &db, &cast);
        assert_eq!(reqs.len(), 1, "one target slot");
        assert_eq!(reqs[0].spec, TargetSpec::SpellOnStack);
        assert_eq!(reqs[0].candidates, vec![Target::Spell(sid)]);
    }

    #[test]
    fn issue_148_targeted_cast_with_no_legal_candidate_is_not_offered() {
        // CR 601.2c: with no spell on the stack the counterspell's only slot has
        // zero candidates, so `valid_actions` never offers the cast.
        let db = db();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let negation = state.new_instance(counterspell_id());
        state.players[0].hand = vec![negation];
        state.players[0].mana_pool.add(Color::Blue, 1);

        assert!(
            !valid_actions(&state, &db)
                .iter()
                .any(|a| matches!(a, Action::CastSpell { .. })),
            "a targeted cast with zero legal candidates is never offered"
        );
    }

    #[test]
    fn issue_148_counterspell_cannot_target_an_ability_on_the_stack_cr_605_3() {
        // Only spells are SpellOnStack candidates: an ability on the stack is not a
        // spell, and a mana ability never uses the stack at all (CR 605.3). With
        // only an ability on the stack the counterspell has no legal target and is
        // not offered.
        let db = db();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let sid = StackId(state.mint_id());
        state.stack.push(StackObject {
            id: sid,
            controller: PlayerId(1),
            kind: StackObjectKind::Ability {
                source: PermanentId(999),
                effects: vec![Effect::DrawCard { count: 1 }],
            },
            targets: Vec::new(),
        });
        let negation = state.new_instance(counterspell_id());
        state.players[0].hand = vec![negation];
        state.players[0].mana_pool.add(Color::Blue, 1);

        // The ability is not a candidate, so the slot is empty and no cast is offered.
        assert!(legal_targets_for_spec(TargetSpec::SpellOnStack, &state, &db).is_empty());
        assert!(!valid_actions(&state, &db)
            .iter()
            .any(|a| matches!(a, Action::CastSpell { .. })));
    }

    #[test]
    fn issue_148_a_cast_with_an_illegal_spell_target_is_a_no_op() {
        // A target of the right kind but outside the legal set — a StackId naming
        // no spell on the stack — fails the fresh legality check, so the whole cast
        // is rejected as a no-op (nothing cast, no mana paid).
        let db = db();
        let (state, negation, _sid) = counterspell_over_a_creature_spell();

        let after = apply_action(
            &state,
            &Action::CastSpell {
                card: negation,
                targets: vec![Target::Spell(StackId(99_999))],
            },
            &db,
        );
        assert_eq!(after, state);
    }

    #[test]
    fn issue_341_attacker_may_target_any_opponent_but_never_self_or_eliminated() {
        // CR 508.1a: an attacker may be declared to attack any opponent still in the
        // game; assigning it to the active player or an eliminated one is illegal.
        let db = db();
        let mut state = GameState::new_multiplayer(3);
        state.turn = 2;
        state.step = Step::DeclareAttackers;
        state.active_player = PlayerId(0);
        let atk = put_on_battlefield(&mut state, fixture("verdant_scout"));

        let attack = |defender| {
            [Attack {
                attacker: atk,
                defender,
            }]
        };
        // Legal against either living opponent.
        assert!(attackers_selection_is_legal(
            &state,
            &db,
            &attack(PlayerId(1))
        ));
        assert!(attackers_selection_is_legal(
            &state,
            &db,
            &attack(PlayerId(2))
        ));
        // Illegal against the active player themselves.
        assert!(!attackers_selection_is_legal(
            &state,
            &db,
            &attack(PlayerId(0))
        ));
        // Illegal against a non-existent seat.
        assert!(!attackers_selection_is_legal(
            &state,
            &db,
            &attack(PlayerId(9))
        ));

        // Once seat 2 is eliminated it is no longer a legal defender.
        state.players[2].has_lost = true;
        assert!(!attackers_selection_is_legal(
            &state,
            &db,
            &attack(PlayerId(2))
        ));
        assert!(attackers_selection_is_legal(
            &state,
            &db,
            &attack(PlayerId(1))
        ));
    }
}
