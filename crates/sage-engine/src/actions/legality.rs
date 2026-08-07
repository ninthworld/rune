//! Action legality validation — checking that actions conform to game rules.

use crate::ability::Ability;
use crate::card::abilities_of_permanent;
use crate::combat::{
    attacker_candidates, attackers_needing_damage_order, attacking_defender_of,
    blocker_can_block_attacker, blocker_candidates_for, declared_attackers, defender_candidates,
    pending_blocker_declarer,
};
use crate::id::{PermanentId, PlayerId};
use crate::resolve::target_is_legal;
use crate::state::GameState;
use crate::CardDatabase;

use super::definition::{Action, Attack, Block, DamageOrder};
use super::generation::valid_actions;
use super::targeting::action_target_groups;
use super::utilities::{
    all_unique, graveyard_ability, graveyard_cost_payable, loyalty_cost_is_payable,
    loyalty_timing_allows, sorcery_timing_allows, tap_cost_is_summoning_sick,
};

/// Whether `action` — including any targets it carries — is legal against the
/// current `state`. This is the gate [`crate::apply_action`] runs before it
/// applies anything.
///
/// Two independent checks, mirroring ADR 0004 §Enumeration:
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

    // 1a. A mid-resolution choice answer validates its card selection against the
    //     freshly recomputed candidate set and the choice's clamped bounds
    //     ([`crate::choice::answer_is_legal`]) rather than the target-slot machinery:
    //     it names cards in a hidden zone, not objects on the battlefield.
    if let Action::AnswerChoice { chosen } = action {
        return crate::choice::answer_is_legal(state, chosen, db);
    }

    // 1a-ter. A permanent-selection answer is validated the same way and for the same
    //     reason, against its own freshly recomputed candidate set: the ids name objects
    //     on the battlefield rather than targets a slot declared, so the target-slot
    //     machinery has nothing to say about them.
    if let Action::AnswerPermanents { chosen } = action {
        return crate::choice::answer_permanents_is_legal(state, chosen, db);
    }

    // 1a-bis. A yes-or-no answer is validated against the board *as it stands*: accepting
    //     an optional cost is legal only while the chooser can actually pay it — the pool
    //     for mana, something of the named class for a sacrifice or a discard — which is
    //     the same predicate the offer is built from ([`crate::confirm_is_payable`]), so
    //     the offer and the charge can never disagree. Declining needs nothing and is
    //     always legal — the reason an unpayable cost never stalls the game.
    if let Action::AnswerConfirm { accept } = action {
        return !accept || crate::confirm_is_payable(state, db);
    }

    // 1a-ter. A CR 616.1 ordering answer names a *position* in the option list the
    //     engine derives from the pending event, so it is validated against that list
    //     recomputed now ([`crate::pending_replacement_options`]) rather than against the
    //     one the client was shown. An index past its end is an answer to a question
    //     nobody asked.
    if let Action::AnswerReplacement { index } = action {
        return usize::from(*index) < crate::pending_replacement_options(state, db).len();
    }

    // 1a-quater. A CR 614.12 card-naming answer names a card in the **catalog**, so it is
    //     validated against the candidate list derived from the catalog and the class the
    //     entering card declared ([`crate::named_card_candidates`]) rather than against
    //     anything on the battlefield. This is where the legal posture is enforced: a
    //     handle outside that list — including one the client invented — is refused, so
    //     no card name SAGE has not defined can ever be recorded on a permanent.
    if let Action::AnswerCardName { card } = action {
        return crate::choice::named_card_is_legal(state, *card, db);
    }

    // 1a-quinquies. A card ordering names *every* card of the pending remainder, once
    //     each, and is checked against that remainder recomputed now
    //     ([`crate::choice::order_answer_is_legal`]). A permutation has one legal size, so
    //     unlike a selection there is nothing to clamp: a duplicate, a foreign card, or a
    //     short list is rejected rather than partly obeyed.
    if let Action::AnswerOrder { order } = action {
        return crate::choice::order_answer_is_legal(state, order);
    }

    // 1a-sexies. A permanent named as a card enters (CR 614.12) is validated against the
    //     class the card printed, recomputed now — never against a list the client was
    //     shown. Naming nothing is legal exactly when the card said "you may".
    if let Action::AnswerPermanent { chosen } = action {
        let Some(request) =
            crate::pending_player_choice(state).and_then(|p| p.question.permanent())
        else {
            return false;
        };
        let chooser = crate::pending_player_choice(state).map(|p| p.chooser);
        return match chosen {
            None => request.optional,
            Some(named) => chooser.is_some_and(|chooser| {
                crate::copy_choice_candidates(state, request.of, chooser, db).contains(named)
            }),
        };
    }

    // 1b. A mulligan keep validates its bottoming selection (CR 103.5) rather than
    //     the target-slot machinery: exactly one distinct hand card per mulligan
    //     taken (see [`crate::mulligan::keep_bottom_is_legal`]).
    if let Action::Keep { bottom } = action {
        return crate::mulligan::keep_bottom_is_legal(state, bottom);
    }

    // 1c. Combat declarations carry a permanent multi-select rather than
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

    // 1d. Hardening (CR 302.6, issue #454): a `{T}`-cost ability of a summoning-sick
    //     creature is never activatable. Check 1 above already withholds the offer,
    //     so this is a second, independent gate that re-derives the restriction from
    //     current state — a stale or forged action id can never slip a sick creature's
    //     tap ability through [`crate::apply_action`].
    if let Action::ActivateAbility {
        permanent,
        index,
        payment,
        ..
    } = action
    {
        if !activation_clears_summoning_sickness(state, db, *permanent, *index) {
            return false;
        }
        // 1d-bis. An activation that names its own chosen costs (CR 601.2b): the cards and
        //     permanents it names must be exactly what the cost demands, each still where
        //     it was and still the activator's. Check 1 above established only that *a*
        //     payment exists — it is asked before the player has chosen anything — and
        //     this establishes that the one they assembled is one, so the widened offer
        //     never widens what is legal.
        if !super::payment_covers_activation(state, db, *permanent, *index, payment) {
            return false;
        }
        // 1e. Hardening (CR 606.3, issue #608): a loyalty ability is sorcery-speed,
        //     once per turn per permanent, and payable only out of loyalty the source
        //     actually has. Check 1 already withholds the offer; this re-derives all
        //     three from current state so a stale or forged action id can never spend
        //     loyalty a planeswalker has not got, nor take a second activation in one
        //     turn, nor sneak one in at instant speed. The exact shape
        //     `activation_clears_summoning_sickness` uses.
        if !loyalty_activation_is_legal(state, db, *permanent, *index) {
            return false;
        }
        // 1e-bis. Hardening (CR 702.6b, CR 602.5d): equip is a sorcery-speed activation,
        //     and so is any ability whose printed text says so. Check 1 already withholds
        //     the offer; this re-derives the timing from current state so a stale or
        //     forged action id can never move an Equipment or turn a permanent over
        //     during combat, on an opponent's turn, or in response to a spell.
        // CR 602.5f: the once-each-turn allowance, re-derived here exactly as the timing
        // gates around it are — the offer withheld it, and a submitted action is checked
        // against the live ledger rather than trusted.
        if !limited_activation_is_legal(state, db, *permanent, *index) {
            return false;
        }
        if !sorcery_speed_activation_is_legal(state, db, *permanent, *index) {
            return false;
        }
        // CR 602.5c: `Activate only if …`, re-derived from the live board for the reason
        // every gate above it is — the offer withheld it, and a stale action id must not
        // spend a permission that has since gone away.
        if !activation_condition_is_satisfied(state, db, *permanent, *index) {
            return false;
        }
    }

    // 1e-ter. Hardening (CR 113.6, issue #723): an activation naming a card in a
    //     graveyard is legal only while that card is *still there*, the ability it names
    //     still functions from there, and its cost is still payable out of the seat's
    //     pool. Check 1 already withholds the offer; this re-derives all three from
    //     current state, so a stale or forged action id can never activate a card that
    //     was exiled in response, name an ordinary battlefield ability from a graveyard,
    //     or spend mana the seat has not got. **Timing is re-derived here too**, and it is
    //     re-derived by the same mechanism a hand cast's is: check 1 asked
    //     `valid_actions`, which offers this only to the seat currently holding priority
    //     and only outside every window that suspends priority (a mulligan, a pending
    //     choice, a trigger owed targets, the cleanup step, a combat declaration).
    if let Action::ActivateAbilityFromGraveyard {
        card,
        index,
        payment,
        ..
    } = action
    {
        // Two independent questions, and both must hold: the cost is payable at all
        // (the offer's own gate), and the payment the player assembled is exactly it.
        if !graveyard_activation_is_legal(state, db, *card, *index)
            || !super::payment::payment_covers_graveyard_activation(
                state, db, *card, *index, payment,
            )
        {
            return false;
        }
    }

    // 1f. A cast that names its own payment (CR 601.2): the sources it names must each
    //     be a legal mana-ability activation, in sequence, and the pool they produce must
    //     cover the cost. Simulated on a scratch copy and applied for real only once the
    //     whole action is found legal, which is what makes the casting process atomic —
    //     an insufficient payment leaves no tapped land behind, because `apply_action`
    //     returns the state it was handed.
    //
    //     Check 1 above has already found the cast on offer, and the generator offers it
    //     against what the seat could *tap for* rather than what is already floating — so
    //     the offer says a payment exists, and this says whether the one the player
    //     assembled is it. Those are different questions and both are needed: the first is
    //     asked before the player has chosen anything, and a player is free to assemble a
    //     payment that does not cover the cost. The widened offer therefore never widens
    //     what is legal.
    // 1f-pre. The announcement choices (CR 601.2b), re-derived from the card itself:
    //     a modal spell must name one of its own printed modes and a spell with `{X}` in
    //     its cost must name a value, and neither may be named by a spell that does not
    //     ask. Check 1 above cannot catch either, because the requirement form is the
    //     *bare* announcement — it drops both fields before comparing — so this is the
    //     only gate a forged mode meets. It runs **before** the target check below for
    //     the same reason it runs before payment: the mode is what decides which target
    //     slots exist, and an unchosen one would otherwise read as a spell with none.
    if !super::announcement_is_legal(db, action) {
        return false;
    }

    if let Action::CastSpell {
        card, x, payment, ..
    } = action
    {
        if !super::payment_covers_cast(state, db, *card, *x, payment) {
            return false;
        }
    }

    // 2. The carried targets must fill every slot the action declares, each with
    //    a target that is legal *now*. `target_is_legal` is the same predicate the
    //    resolve path re-checks with (CR 608.2b) and the one `legal_targets_for_spec`
    //    filters by, so "in the freshly computed legal set" and "passes the check"
    //    are one and the same — we test membership directly, without building the
    //    set (and certainly without the cartesian product).
    let groups = action_target_groups(state, db, action);
    let actor = super::targeting::acting_player(state, action);
    // The permanent the source-relative specs are relative to; `None` for a cast.
    let source = super::targeting::action_source(state, action);
    let chosen = action.targets();
    targets_fill_groups(&groups, chosen, state, actor, source, db)
}

/// Whether `chosen` is a legal filling of `groups` against current state (CR 601.2c).
///
/// Three things, and each of them is a rule a single-target-per-effect engine never had
/// to state:
///
/// - the **count** falls between the groups' summed minimum and maximum, so "up to two
///   target creatures" accepts none, one, or two and nothing accepts three;
/// - every chosen target is **legal now** for the group whose slot it fills, paired by
///   [`target_counts`](crate::ability::target_counts);
/// - the targets within one group are **distinct** — one object cannot be chosen for two
///   instances of the word "target" in the same group (CR 601.2c). Across groups it may:
///   a spell that names two different target words may aim both at one creature.
fn targets_fill_groups(
    groups: &[crate::ability::TargetGroup],
    chosen: &[crate::ability::Target],
    state: &GameState,
    actor: crate::id::PlayerId,
    source: Option<crate::id::PermanentId>,
    db: &CardDatabase,
) -> bool {
    let minimum: usize = groups.iter().map(|g| usize::from(g.min)).sum();
    let maximum: usize = groups.iter().map(|g| usize::from(g.max)).sum();
    if chosen.len() < minimum || chosen.len() > maximum {
        return false;
    }
    let mut rest = chosen;
    for (group, take) in groups
        .iter()
        .zip(crate::ability::group_target_counts(groups, chosen.len()))
    {
        let (slice, remaining) = rest.split_at(take.min(rest.len()));
        rest = remaining;
        if !all_unique(slice) {
            return false;
        }
        if !slice
            .iter()
            .all(|&target| target_is_legal(group.spec, target, state, actor, source, db))
        {
            return false;
        }
    }
    true
}

/// Whether activating ability `index` of `permanent` clears the CR 302.6
/// summoning-sickness restriction (issue #454): `false` exactly when the ability's
/// cost contains `{T}` and its source is a creature that has not been under its
/// controller's control since their most recent turn began (haste, CR 702.10b,
/// exempts it). A mana ability is gated like any other (CR 605.3a).
///
/// `false` for a permanent that is not on the battlefield — a stale id names no
/// source to pay a cost with. `true` for an index that is not an activated ability:
/// there is no `{T}` cost to restrict, and check 1 of [`action_is_legal`] has
/// already rejected the action on its own terms.
fn activation_clears_summoning_sickness(
    state: &GameState,
    db: &CardDatabase,
    permanent: PermanentId,
    index: usize,
) -> bool {
    let Some(perm) = state.battlefield.iter().find(|p| p.id == permanent) else {
        return false;
    };
    match abilities_of_permanent(state, db, perm).get(index) {
        Some(Ability::Activated { cost, .. }) => !tap_cost_is_summoning_sick(state, perm, cost, db),
        _ => true,
    }
}

/// Whether activating ability `index` of `permanent` satisfies CR 606.3, the two
/// timing rules and the one payment rule a **loyalty** ability has and no other
/// activated ability does: sorcery speed on its controller's turn, at most one per
/// permanent per turn ([`loyalty_timing_allows`]), and a negative cost no larger than
/// the loyalty on the permanent ([`loyalty_cost_is_payable`]).
///
/// `false` for a permanent that is not on the battlefield — a stale id names no source
/// to spend loyalty from. `true` for an index that is not a loyalty ability: there is
/// no loyalty symbol to restrict, so this gate has nothing to say about it.
fn loyalty_activation_is_legal(
    state: &GameState,
    db: &CardDatabase,
    permanent: PermanentId,
    index: usize,
) -> bool {
    let Some(perm) = state.battlefield.iter().find(|p| p.id == permanent) else {
        return false;
    };
    let Some(ability) = abilities_of_permanent(state, db, perm).get(index).cloned() else {
        return true;
    };
    if !crate::ability::is_loyalty_ability(&ability) {
        return true;
    }
    let Ability::Activated { cost, .. } = &ability else {
        return true;
    };
    loyalty_timing_allows(state, perm)
        && cost.iter().all(|c| match c {
            crate::ability::Cost::Loyalty { amount } => loyalty_cost_is_payable(perm, *amount),
            _ => true,
        })
}

/// Whether ability `index` of `permanent` still has its **once each turn** allowance
/// (CR 602.5f).
///
/// `true` for an ability that prints no such line — there is nothing to spend — and for a
/// stale index, which names no ability to restrict. The shape
/// [`loyalty_activation_is_legal`] uses, and keyed by `(permanent, ability)` because the
/// allowance is per ability rather than per permanent.
fn limited_activation_is_legal(
    state: &GameState,
    db: &CardDatabase,
    permanent: PermanentId,
    index: usize,
) -> bool {
    let Some(perm) = state.battlefield.iter().find(|p| p.id == permanent) else {
        return false;
    };
    match abilities_of_permanent(state, db, perm).get(index) {
        Some(Ability::Activated {
            once_each_turn: true,
            ..
        }) => !state.limited_activations.contains(&(permanent, index)),
        _ => true,
    }
}

/// Whether activating ability `index` of `permanent` satisfies the **sorcery-speed**
/// restriction it is under: CR 702.6b for an equip ability, which implies it, and
/// CR 602.5d for an ability whose printed text declares it
/// ([`ActivationTiming::SorcerySpeed`](crate::ActivationTiming)). Both are
/// [`sorcery_timing_allows`].
///
/// `false` for a permanent that is not on the battlefield — a stale id names no source to
/// act with. `true` for an index under neither rule: there is no window to restrict, so
/// this gate has nothing to say about it. The exact shape
/// [`loyalty_activation_is_legal`] uses.
fn sorcery_speed_activation_is_legal(
    state: &GameState,
    db: &CardDatabase,
    permanent: PermanentId,
    index: usize,
) -> bool {
    let Some(perm) = state.battlefield.iter().find(|p| p.id == permanent) else {
        return false;
    };
    match abilities_of_permanent(state, db, perm).get(index) {
        // The authored `Activate only as a sorcery.` rides the same gate: it is the same
        // question about the same window, so an ability that says it and an equip
        // ability that implies it are answered by one expression (CR 602.5d, CR 702.6b).
        Some(ability)
            if crate::ability::is_equip_ability(ability)
                || crate::ability::is_sorcery_speed_ability(ability) =>
        {
            sorcery_timing_allows(state, perm)
        }
        _ => true,
    }
}

/// Whether activating ability `index` of the graveyard card `card` is legal for the
/// priority holder right now (CR 113.6): the card is in *their* graveyard, the ability
/// there functions from a graveyard ([`graveyard_ability`]), and its cost is payable out
/// of their pool ([`graveyard_cost_payable`]).
///
/// The exact shape [`activation_clears_summoning_sickness`] and
/// [`sorcery_speed_activation_is_legal`] use, over a card in a zone rather than a permanent —
/// which is the whole reason it exists separately. `false` for a card that is not in the
/// graveyard: a stale id names nothing to activate, and asking the battlefield about it
/// would find nothing either, silently.
fn graveyard_activation_is_legal(
    state: &GameState,
    db: &CardDatabase,
    card: crate::id::CardInstance,
    index: usize,
) -> bool {
    let seat = state.priority;
    let Some(ability) = graveyard_ability(state, db, seat, card, index) else {
        return false;
    };
    let Ability::Activated { cost, .. } = &ability else {
        return false;
    };
    graveyard_cost_payable(state, db, seat, card.id, cost)
        && crate::ability::activation_condition_holds(state, &ability, seat, db)
}

/// Whether ability `index` of `permanent` satisfies the **board condition** its printed
/// text declares (CR 602.5c) — `true` for an ability that declares none, which is the
/// exact shape [`sorcery_speed_activation_is_legal`] beside it takes.
///
/// `false` for a permanent that is not on the battlefield: a stale id names no source, and
/// no board makes an ability of nothing activatable.
fn activation_condition_is_satisfied(
    state: &GameState,
    db: &CardDatabase,
    permanent: PermanentId,
    index: usize,
) -> bool {
    let Some(perm) = state.battlefield.iter().find(|p| p.id == permanent) else {
        return false;
    };
    abilities_of_permanent(state, db, perm)
        .get(index)
        .is_some_and(|ability| {
            crate::ability::activation_condition_holds(state, ability, perm.controller, db)
        })
}

/// Whether a declared attacker selection is legal (CR 508.1a): every named
/// permanent is a current attacker candidate ([`attacker_candidates`]), no
/// permanent is named twice, and every attacker's defender is a legal defender
/// candidate ([`defender_candidates`]) — an opponent still in the game, never the
/// active player and never an eliminated one. An empty selection is legal
/// (declaring no attackers).
pub(crate) fn attackers_selection_is_legal(
    state: &GameState,
    db: &CardDatabase,
    attackers: &[Attack],
) -> bool {
    let candidates = attacker_candidates(state, db);
    let defenders = defender_candidates(state, db);
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
/// player* (CR 509.1a — a player blocks only attackers attacking them), no pair is
/// declared twice, no creature is assigned to more attackers than it may block
/// ([`blocks_allowed`]), and each blocker can legally block the attacker it is assigned
/// to given the pairwise evasion rules (CR 702.9c, 702.17b, CR 509.1b, via
/// [`blocker_can_block_attacker`]). An empty selection is legal (declaring no blockers) —
/// unless a *requirement* makes it not, which is the one thing here that is judged
/// against the declarations that were **not** submitted (CR 509.1c,
/// [`block_requirements_are_maximized`]).
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
    // A pair, not a blocker: one creature may be assigned to several attackers
    // (CR 509.1a), but blocking the same attacker twice is not a second block — it is
    // the same one written down again, and it would double that attacker's blocker
    // count against menace and the ceiling.
    let assigned: Vec<(PermanentId, PermanentId)> =
        blocks.iter().map(|b| (b.blocker, b.attacker)).collect();
    all_unique(&assigned)
        && blocks.iter().all(|b| {
            blockers.contains(&b.blocker)
                && attackers.contains(&b.attacker)
                // CR 509.1a: the declaring player may block only attackers attacking
                // *them*, so the attacker's chosen defender must be this declarer.
                && attacking_defender_of(state, b.attacker) == Some(declarer)
                && blocker_can_block_attacker(state, b.attacker, b.blocker, db)
        })
        // The block-count restrictions are the ones that are facts about the
        // *selection* rather than about any one pair: a lone blocker is illegal
        // precisely because it is alone, a second one precisely because it is not,
        // so both can only be judged once the whole declaration is in hand.
        && block_counts_are_legal(state, blocks, db)
        // And the same question from the blocker's side: how many attackers one
        // creature was assigned to.
        && blocker_loads_are_legal(state, blocks, db)
        // Last, and only over declarations everything above has already called legal:
        // whether this one leaves a requirement unmet that some other legal declaration
        // would have met (CR 509.1c).
        && block_requirements_are_maximized(state, declarer, blocks, db)
}

/// Whether `blocks` meets as many block requirements as any legal declaration could
/// (CR 509.1c): *the declaration must obey the maximum possible number of requirements
/// without violating any restrictions*.
///
/// The one gate here that judges a declaration by what it **omits**, and the reason it is
/// applied last: "the maximum possible" ranges over the declarations that are legal, so
/// every other check has to have had its say before this one is meaningful. The maximum
/// itself is the engine's own search ([`max_block_requirements_met`]); all this does is
/// count how many of the required pairs this declaration actually contains and compare.
///
/// Restrictions therefore win without a clause saying so. A requirement no legal
/// declaration can meet contributes nothing to the maximum, so the player is never asked
/// for a declaration the gates above would refuse — and a declaration meeting the maximum
/// always exists, which is what keeps this from being a way to stall combat.
///
/// The comparison is `>=` rather than `==` so that the *only* way this rejects is a
/// declaration that fell short. A submitted declaration cannot contain a required pair
/// twice (the uniqueness check above), so exceeding the maximum is not reachable.
fn block_requirements_are_maximized(
    state: &GameState,
    declarer: PlayerId,
    blocks: &[Block],
    db: &CardDatabase,
) -> bool {
    let required = crate::combat::block_requirements(state, declarer, db);
    if required.is_empty() {
        return true;
    }
    let met = blocks
        .iter()
        .filter(|block| required.contains(&(block.blocker, block.attacker)))
        .count();
    met >= crate::combat::max_block_requirements_met(state, declarer, db)
}

/// Whether every creature named in `blocks` is assigned to a legal *number* of
/// attackers (CR 509.1a): one, unless it currently has a permission to block additional
/// creatures ([`blocks_allowed`]).
///
/// The counterpart of [`block_counts_are_legal`] from the blocker's side of the pairing,
/// and a whole-selection judgment for the same reason: a second assignment is illegal
/// precisely because there is already a first one, which no pairwise check can see.
///
/// Read through the computed characteristics (CR 613.1f), so a granted permission counts
/// exactly as a printed one. A creature blocking nothing is never at issue — this bounds
/// how a creature blocks, never whether it must.
fn blocker_loads_are_legal(state: &GameState, blocks: &[Block], db: &CardDatabase) -> bool {
    blocks.iter().all(|block| {
        let assigned = blocks.iter().filter(|b| b.blocker == block.blocker).count();
        u32::try_from(assigned).is_ok_and(|assigned| {
            assigned <= crate::combat::blocks_allowed(state, block.blocker, db)
        })
    })
}

/// Whether every attacker named in `blocks` is blocked by a legal *number* of
/// creatures — the two restrictions that constrain the count rather than the pairing:
///
/// - **menace** (CR 702.110b): a creature with menace can't be blocked except by two or
///   more creatures, so exactly one blocker assigned to it makes the whole declaration
///   illegal — a floor;
/// - **[`blocked_by_at_most_one`]** (CR 509.1b): a creature that can't be blocked by
///   more than one creature makes a second blocker illegal — the mirroring ceiling.
///
/// Zero blockers is fine for both: each restricts *how* a creature is blocked, never
/// whether it must be. A creature carrying both is simply unblockable, and this says so
/// without a special case — no count satisfies a floor of two and a ceiling of one.
///
/// Both are read through the computed characteristics (CR 613.1f), so a granted one
/// restricts exactly as a printed one does. Only attackers this declaration actually
/// names are counted; a blocker assigned to an attacker attacking a *different* player
/// cannot exist, since an attacker attacks one player and only that player declares
/// against it.
fn block_counts_are_legal(state: &GameState, blocks: &[Block], db: &CardDatabase) -> bool {
    blocks.iter().all(|block| {
        let assigned = blocks
            .iter()
            .filter(|b| b.attacker == block.attacker)
            .count();
        let floor_met = !crate::combat::permanent_has_menace(state, block.attacker, db)
            || assigned >= MENACE_MINIMUM_BLOCKERS;
        let ceiling_met =
            !crate::combat::blocked_by_at_most_one(state, block.attacker, db) || assigned <= 1;
        floor_met && ceiling_met
    })
}

/// The number of blockers menace demands (CR 702.110b, "except by two or more
/// creatures"). Named so the floor and the ceiling in [`block_counts_are_legal`] read
/// as the pair of bounds they are.
const MENACE_MINIMUM_BLOCKERS: usize = 2;

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
            .filter(|p| p.blocking.contains(&order.attacker))
            .map(|p| p.id)
            .collect();
        let mut chosen = order.blockers.clone();
        declared.sort_by_key(|id| id.0);
        chosen.sort_by_key(|id| id.0);
        all_unique(&order.blockers) && chosen == declared
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]

    use super::super::definition::Attack;
    use super::*;
    use crate::apply_action;
    use crate::fixtures::fixture;
    use crate::id::PlayerId;
    use crate::phase::Step;
    use crate::state::Permanent;

    /// A two-player game at player 0's precombat main on turn 3 with a Llanowar
    /// Elves that entered on `entered_turn`, and the action that activates its
    /// `{T}: Add {G}`.
    fn elves_state(entered_turn: u32) -> (GameState, CardDatabase, Action) {
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        while state.turn < 3 {
            state = state.advance_to_next_turn();
        }
        state.step = Step::PrecombatMain;
        let card = fixture("llanowar_elves");
        let inst = state.new_instance(card);
        let id = PermanentId(state.mint_id());
        state.battlefield.push(Permanent {
            id,
            instance: inst.id,
            printed: card.into(),
            controller: PlayerId(0),
            tapped: false,
            entered_turn,
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
        let action = Action::ActivateAbility {
            permanent: id,
            index: 0,
            targets: Vec::new(),
            payment: Vec::new(),
        };
        (state, db, action)
    }

    /// A two-player combat parked at declare-blockers with `attacker` attacking alone
    /// and the defender controlling two Sun Sentinels; returns the state and the two
    /// candidate blockers.
    fn ceiling_combat(
        db: &CardDatabase,
        attacker: &str,
    ) -> (GameState, PermanentId, [PermanentId; 2]) {
        let mut state = GameState::new_two_player();
        state.step = Step::DeclareAttackers;
        state.priority = PlayerId(0);
        let place = |state: &mut GameState, slug: &str, seat: PlayerId| {
            let card = fixture(slug);
            let inst = state.new_instance(card);
            let id = PermanentId(state.mint_id());
            state.battlefield.push(Permanent {
                id,
                instance: inst.id,
                printed: card.into(),
                controller: seat,
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
            id
        };
        let atk = place(&mut state, attacker, PlayerId(0));
        let first = place(&mut state, "sun_sentinel", PlayerId(1));
        let second = place(&mut state, "sun_sentinel", PlayerId(1));
        let mut state = apply_action(
            &state,
            &Action::DeclareAttackers {
                attackers: vec![Attack {
                    attacker: atk,
                    defender: crate::combat::AttackTarget::Player(PlayerId(1)),
                }],
            },
            db,
        );
        while state.step != Step::DeclareBlockers {
            state = apply_action(&state, &Action::PassPriority, db);
        }
        (state, atk, [first, second])
    }

    #[test]
    fn issue_606_a_block_count_ceiling_is_judged_over_the_whole_selection() {
        // CR 509.1b: like menace, the ceiling cannot be judged from a pair. Each
        // blocker alone is legal; it is the *pair together* that is not, which is only
        // visible once the declaration is assembled.
        let db = CardDatabase::bundled().unwrap();
        let (state, boar, [first, second]) = ceiling_combat(&db, "bristling_boar");
        let block = |blockers: &[PermanentId]| Action::DeclareBlockers {
            blocks: blockers
                .iter()
                .map(|&blocker| Block {
                    blocker,
                    attacker: boar,
                })
                .collect(),
        };

        assert!(action_is_legal(&state, &block(&[]), &db), "none is legal");
        assert!(action_is_legal(&state, &block(&[first]), &db));
        assert!(action_is_legal(&state, &block(&[second]), &db));
        assert!(
            !action_is_legal(&state, &block(&[first, second]), &db),
            "a second blocker breaks the ceiling"
        );
    }

    #[test]
    fn issue_606_a_floor_and_a_ceiling_together_leave_no_legal_block() {
        // A creature with menace *and* the ceiling is simply unblockable, and the two
        // bounds say so without a special case: no count satisfies both.
        let db = CardDatabase::bundled().unwrap();
        let (mut state, boar, [first, second]) = ceiling_combat(&db, "bristling_boar");
        let stamp = state.mint_id();
        state.static_effects.push(crate::state::StaticEffect {
            source: stamp,
            affects: crate::state::EffectAffects::SpecificPermanent(boar),
            modification: crate::state::Modification::GrantKeyword(crate::card::Keyword::Menace),
            duration: crate::state::Duration::UntilEndOfTurn,
        });
        let block = |blockers: &[PermanentId]| Action::DeclareBlockers {
            blocks: blockers
                .iter()
                .map(|&blocker| Block {
                    blocker,
                    attacker: boar,
                })
                .collect(),
        };

        assert!(
            action_is_legal(&state, &block(&[]), &db),
            "declaring no blockers is still legal"
        );
        assert!(
            !action_is_legal(&state, &block(&[first]), &db),
            "menace's floor"
        );
        assert!(
            !action_is_legal(&state, &block(&[first, second]), &db),
            "the ceiling"
        );
    }

    #[test]
    fn issue_454_apply_rejects_a_summoning_sick_tap_ability_handed_directly() {
        // CR 302.6: even handed the action directly — a stale or forged action id
        // that `valid_actions` never offered — the apply path refuses it, so
        // `apply_action` is a no-op (no mana floated, the creature left untapped).
        let (state, db, action) = elves_state(3);
        assert!(!action_is_legal(&state, &action, &db));
        let after = apply_action(&state, &action, &db);
        assert_eq!(after, state, "an illegal activation changes nothing");
        assert_eq!(after.players[0].mana_pool.green, 0);
        assert!(!after.battlefield[0].tapped);
    }

    #[test]
    fn issue_454_the_hardening_gate_is_independent_of_the_offer_check() {
        // The gate re-derives CR 302.6 from current state rather than trusting the
        // offer list, so it rejects on its own — not only because check 1 would.
        let (sick, db, action) = elves_state(3);
        let Action::ActivateAbility {
            permanent, index, ..
        } = action
        else {
            panic!("the fixture builds an activation");
        };
        assert!(!activation_clears_summoning_sickness(
            &sick, &db, permanent, index
        ));

        // The same creature, in play since an earlier turn, clears the gate.
        let (seasoned, db, action) = elves_state(1);
        assert!(activation_clears_summoning_sickness(
            &seasoned, &db, permanent, index
        ));
        assert!(action_is_legal(&seasoned, &action, &db));

        // A permanent id that names nothing on the battlefield clears nothing.
        assert!(!activation_clears_summoning_sickness(
            &seasoned,
            &db,
            PermanentId(9999),
            0
        ));
    }

    #[test]
    fn issue_454_the_gate_holds_while_the_controller_is_not_the_active_player() {
        // The activation the gate protects against is submittable at instant speed
        // during someone *else's* turn, so the gate is measured from the
        // controller's most recent turn. Player 0's Elves entered on their turn 3;
        // through player 1's turn 4 it is still restricted, and only player 0's
        // turn 5 clears it. The turns are walked so the rotation is the engine's.
        let (state, db, action) = elves_state(3);
        let Action::ActivateAbility {
            permanent, index, ..
        } = action
        else {
            panic!("the fixture builds an activation");
        };

        let opponents_turn = state.advance_to_next_turn();
        assert_eq!(
            (opponents_turn.turn, opponents_turn.active_player),
            (4, PlayerId(1))
        );
        assert!(
            !activation_clears_summoning_sickness(&opponents_turn, &db, permanent, index),
            "still restricted during the opponent's turn"
        );
        assert!(!action_is_legal(&opponents_turn, &action, &db));

        let own_turn = opponents_turn.advance_to_next_turn();
        assert_eq!((own_turn.turn, own_turn.active_player), (5, PlayerId(0)));
        assert!(activation_clears_summoning_sickness(
            &own_turn, &db, permanent, index
        ));
    }
}
