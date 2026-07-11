//! Legal action enumeration — the engine's legality authority.
//!
//! [`Action`] is the closed set of things a player may take; [`valid_actions`]
//! computes, pull-based, exactly which are legal for the current priority
//! holder. [`crate::apply_action`] validates a chosen action against this set —
//! and, for a targeted action, against freshly computed legal target sets — in
//! [`action_is_legal`] before applying it.

use crate::ability::{Ability, Cost, Effect, Target, TargetSpec};
use crate::card::abilities_of;
use crate::card_type::CardType;
use crate::combat::{
    attacker_candidates, blocker_candidates, declared_attackers, defending_player,
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
        /// The permanents declared as attackers, each attacking the sole opponent.
        attackers: Vec<PermanentId>,
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
            Action::ActivateAbility { targets, .. } => targets,
            // `Keep::bottom` is a mulligan sub-choice, not a target selection; it
            // is validated through the mulligan path, never this one.
            Action::PassPriority
            | Action::PlayLand { .. }
            | Action::CastSpell { .. }
            | Action::Discard { .. }
            | Action::Mulligan
            | Action::Keep { .. }
            // Combat declarations carry permanent selections, not `Target`s, so
            // they hold none of the ability-targeting vocabulary; their selection
            // is validated separately in `action_is_legal`.
            | Action::DeclareAttackers { .. }
            | Action::DeclareBlockers { .. } => &[],
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
            // The mulligan keep's bottom selection is cleared the same way, so its
            // requirement form matches what [`valid_actions`] advertises.
            Action::Keep { .. } => Action::Keep { bottom: Vec::new() },
            // The requirement form of a combat declaration is the empty selection —
            // exactly what `valid_actions` advertises during the declare window.
            Action::DeclareAttackers { .. } => Action::DeclareAttackers {
                attackers: Vec::new(),
            },
            Action::DeclareBlockers { .. } => Action::DeclareBlockers { blocks: Vec::new() },
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
/// priority holder may always pass; may play a land, cast a creature, or (for
/// permanents they control) activate abilities when the relevant timing and cost
/// conditions hold. A state with no valid priority holder offers nothing.
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
    let priority = state.priority;

    // Pre-game London mulligan (CR 103.5): while the mulligan phase is in progress
    // the only choices are the deciding seat's keep/mulligan, and turn 1 has not
    // begun — no lands, spells, abilities, or priority passes are offered until
    // every player has kept (see [`crate::mulligan`]).
    if let Some(actions) = crate::mulligan::mulligan_actions(state) {
        return actions;
    }

    // Cleanup step: no player receives priority (CR 514.3). The only choice is
    // the active player discarding down to the maximum hand size (CR 514.1),
    // offered as a select-from-zone choice — one [`Action::Discard`] per card in
    // hand — and only while they are over the limit. Everything else (passing,
    // lands, spells, abilities) is unavailable here.
    if state.step == Step::Cleanup {
        let mut actions = Vec::new();
        if priority == state.active_player {
            if let Some(player) = state.players.get(priority.0) {
                if player.hand.len() > MAX_HAND_SIZE {
                    for &card in &player.hand {
                        actions.push(Action::Discard { card });
                    }
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
            vec![Action::DeclareAttackers {
                attackers: Vec::new(),
            }]
        } else {
            Vec::new()
        };
    }
    if state.step == Step::DeclareBlockers && !state.blockers_declared {
        // CR 509.1: the defending player declares blockers.
        return if Some(priority) == defending_player(state) {
            vec![Action::DeclareBlockers { blocks: Vec::new() }]
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

        // Cast a creature spell payable from the current pool (sorcery speed).
        if sorcery_speed {
            for &card in &player.hand {
                if let Some(data) = db.card(card.card) {
                    if is_creature(db, card.card)
                        && player.mana_pool.can_pay(&parse_mana_cost(&data.mana_cost))
                    {
                        actions.push(Action::CastSpell { card });
                    }
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

    actions
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
/// targeting effect the action's ability declares, in resolution order. Empty
/// for an action with no targeting effects (or one the state cannot resolve).
///
/// Only [`Action::ActivateAbility`] can target today; a spell carries no
/// engine-modeled effects yet (see the resolve path), so it declares no specs.
fn action_target_specs(state: &GameState, db: &CardDatabase, action: &Action) -> Vec<TargetSpec> {
    let Action::ActivateAbility {
        permanent, index, ..
    } = action
    else {
        return Vec::new();
    };
    let Some(perm) = state.battlefield.iter().find(|p| p.id == *permanent) else {
        return Vec::new();
    };
    let abilities = abilities_of(db, perm.card);
    let Some(Ability::Activated { effects, .. }) = abilities.get(*index) else {
        return Vec::new();
    };
    effects.iter().filter_map(Effect::target_spec).collect()
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
    };
    universe
        .into_iter()
        .filter(|&target| target_is_legal(spec, target, state, db))
        .collect()
}

/// Whether a declared attacker selection is legal (CR 508.1a): every named
/// permanent is a current attacker candidate ([`attacker_candidates`]) and no
/// permanent is named twice. An empty selection is legal (declaring no attackers).
fn attackers_selection_is_legal(
    state: &GameState,
    db: &CardDatabase,
    attackers: &[PermanentId],
) -> bool {
    let candidates = attacker_candidates(state, db);
    all_unique(attackers) && attackers.iter().all(|id| candidates.contains(id))
}

/// Whether a declared blocker selection is legal (CR 509.1a): every blocker is a
/// current blocker candidate ([`blocker_candidates`]), every named attacker is
/// currently attacking ([`declared_attackers`]), and no creature is declared as a
/// blocker more than once (each blocker is assigned to exactly one attacker). An
/// empty selection is legal (declaring no blockers).
fn blocks_selection_is_legal(state: &GameState, db: &CardDatabase, blocks: &[Block]) -> bool {
    let blockers = blocker_candidates(state, db);
    let attackers = declared_attackers(state);
    let assigned: Vec<PermanentId> = blocks.iter().map(|b| b.blocker).collect();
    all_unique(&assigned)
        && blocks
            .iter()
            .all(|b| blockers.contains(&b.blocker) && attackers.contains(&b.attacker))
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

/// Whether `card` is a creature, by its structured printed types.
fn is_creature(db: &CardDatabase, card: CardId) -> bool {
    db.card(card)
        .is_some_and(|c| c.has_type(CardType::Creature))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::apply_action;
    use crate::stack::StackObjectKind;

    /// The bundled card database, for tests that need oracle data.
    fn db() -> CardDatabase {
        CardDatabase::bundled().unwrap()
    }

    /// A database with a "Tapper" artifact whose activated ability is
    /// `{T}: Tap target creature.` and a vanilla "Bear" creature to target.
    fn targeting_db() -> CardDatabase {
        let json = r#"[
            {"id":200,"name":"Tapper","types":["artifact"],"mana_cost":"","oracle_text":"",
             "abilities":[{"type":"activated","cost":[{"kind":"tap"}],
                          "effects":[{"kind":"tap","target":"any_creature"}]}]},
            {"id":201,"name":"Bear","types":["creature"],"mana_cost":"","oracle_text":"",
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
            attacking: false,
            blocking: None,
            damage: 0,
            counters: Default::default(),
        });
        PermanentId(id)
    }

    /// A two-player game at precombat main with a Tapper and `creatures` Bears on
    /// the battlefield under player 0. Returns the state, the Tapper's id, and the
    /// Bears' ids.
    fn tapper_and_creatures(creatures: usize) -> (GameState, PermanentId, Vec<PermanentId>) {
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let tapper = put_on_battlefield(&mut state, CardId(200));
        let bears = (0..creatures)
            .map(|_| put_on_battlefield(&mut state, CardId(201)))
            .collect();
        (state, tapper, bears)
    }

    #[test]
    fn valid_actions_offers_pass_priority_to_the_priority_holder() {
        let state = GameState::new_two_player();
        assert_eq!(valid_actions(&state, &db()), vec![Action::PassPriority]);
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
        let forest = put_on_battlefield(&mut state, CardId(5));

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
}
