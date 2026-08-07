//! Action generation — enumeration of legal actions from game state.

use crate::ability::{
    is_equip_ability, is_loyalty_ability, is_mana_ability, is_sorcery_speed_ability, Ability,
    Effect,
};
use crate::choice::ChoiceQuestion;
use crate::phase::Step;
use crate::state::GameState;
use crate::CardDatabase;

use super::definition::Action;
use super::targeting::legal_targets_for_spec;
use super::utilities::{
    cast_cost, castable_at_instant_speed, cost_payable, graveyard_ability, graveyard_cost_payable,
    is_castable_spell, is_land, loyalty_timing_allows, sorcery_timing_allows,
    tap_cost_is_summoning_sick,
};

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
/// obtained separately via [`crate::target_requirements`]. The generator therefore
/// never pre-expands one action per target combination (ADR 0004 §Enumeration).
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

    // A mid-resolution player choice (CR 701.8 discard, 701.17 scry, 701.19 search, and
    // the CR 608.2 "you may") outranks everything below, including a trigger waiting to
    // be aimed: an object is *part-way through resolving* and the game is frozen until
    // its question is answered. Its chooser is frequently neither the priority holder
    // nor the resolving object's controller — "target player discards two cards" asks
    // the targeted seat — so `apply_action` has already handed them priority and the
    // priority test here is the whole routing. Every other seat is offered nothing at
    // all, which is what "no other seat may act meanwhile" means concretely.
    if let Some(pending) = crate::pending_player_choice(state) {
        return if priority == pending.chooser {
            let mut actions = match &pending.question {
                ChoiceQuestion::Cards(_) => vec![Action::AnswerChoice { chosen: Vec::new() }],
                ChoiceQuestion::Confirm(_) => vec![Action::AnswerConfirm { accept: false }],
                // Every color is always answerable, so the offer is the bare question
                // — one action, whose submitted form names the color.
                ChoiceQuestion::Color(_) => vec![Action::AnswerColor {
                    color: crate::mana::Color::White,
                }],
                // CR 616.1: the affected object's controller orders the applicable
                // replacements. Advertised as the bare question, exactly as the two
                // above; the answer names a position in the freshly derived option list
                // ([`crate::pending_replacement_options`]).
                ChoiceQuestion::Replacement(_) => vec![Action::AnswerReplacement { index: 0 }],
                // CR 614.12: the entering permanent's controller names a card.
                // Advertised as the bare question like the three above; the answer
                // names a card in the freshly derived candidate list
                // ([`crate::named_card_candidates`]).
                ChoiceQuestion::CardName(_) => vec![Action::AnswerCardName {
                    card: crate::id::CardId(0),
                }],
                // A card ordering is advertised the same way: one bare question, whose
                // answer is the permutation the submitted action carries. It is posed
                // only over two cards or more, so it is always answerable.
                ChoiceQuestion::Order(_) => vec![Action::AnswerOrder { order: Vec::new() }],
                // A sacrifice is advertised as the bare question too; the chosen
                // permanents ride in the submitted action, exactly as a discard's
                // chosen cards do.
                ChoiceQuestion::Permanents(_) => {
                    vec![Action::AnswerPermanents { chosen: Vec::new() }]
                }
                // CR 614.12: a permanent named as a card enters. Advertised as the bare
                // question too; the answer names one of the freshly derived candidates
                // ([`crate::copy_choice_candidates`]), or none where the card said "may".
                ChoiceQuestion::Permanent(_) => vec![Action::AnswerPermanent { chosen: None }],
            };
            // CR 605.3a: a player asked to pay a cost while something resolves may
            // activate mana abilities to pay it — the one thing the freeze lets
            // through, and only because a mana ability uses no stack and hands nobody
            // priority. Anything else would let the game move under a question that is
            // still owed.
            //
            // A cost paid by sacrificing or discarding does not widen that by a single
            // action: it is asked of a player who may still be holding mana abilities,
            // so the permission is the same one, and what the cost is paid *with* is
            // asked afterwards as its own question, where nothing at all is legal but
            // the answer.
            if pending
                .question
                .confirm()
                .is_some_and(|request| request.cost.is_some())
            {
                offer_activations(state, db, priority, ManaOnly::Yes, &mut actions);
            }
            offer_concede(&mut actions);
            actions
        } else {
            Vec::new()
        };
    }

    // Commander return decision (CR 903.9a): when the priority holder's commander
    // is sitting in a graveyard or exile awaiting the choice, that decision is the
    // only thing they may take — offered like the cleanup discard and combat
    // declarations, a discrete choice rather than something taken with priority.
    // Both accept and decline are always available, so any priority automation can
    // pick decline and move on (it never stalls). Not a replacement effect: the
    // commander already moved; this offers to move it again to the command zone.
    if let Some(commander) = state
        .players
        .get(priority.0)
        .and_then(|p| p.commander.as_ref())
    {
        if commander.return_pending {
            let card = crate::id::CardInstance {
                id: commander.instance,
                card: commander.card,
            };
            let mut actions = vec![
                Action::ReturnCommanderToCommandZone { card },
                Action::DeclineCommanderReturn { card },
            ];
            offer_concede(&mut actions);
            return actions;
        }
    }

    // Aiming a triggered ability (CR 603.3d) is a choice made *as it is put on the
    // stack* — before any player receives priority (CR 603.3b) — so while one is owed
    // the game does not proceed: its controller's only action is the choice itself
    // (no pass, no spells, no responses), and no other player acts. The same shape as
    // the cleanup discard and the combat declarations below. `apply_action` has
    // already handed priority to the chooser, so the priority test here is the whole
    // routing; the ability is advertised once in its empty requirement form and its
    // per-slot candidates come from [`crate::target_requirements`].
    if let Some(ability) = crate::pending_trigger_target_choice(state) {
        let chooser = crate::triggers::controller_of_stack_object(state, ability);
        return if Some(priority) == chooser {
            let mut actions = vec![Action::ChooseTriggerTargets {
                ability,
                targets: Vec::new(),
            }];
            offer_concede(&mut actions);
            actions
        } else {
            Vec::new()
        };
    }

    // Cleanup step: no player receives priority (CR 514.3). The only choice is
    // the active player discarding down to the maximum hand size (CR 514.1),
    // offered as a select-from-zone choice — one [`Action::Discard`] per card in
    // hand — and only while they are over the limit. Everything else (passing,
    // lands, spells, abilities) is unavailable here — except conceding (CR 104.3a).
    if state.step == Step::Cleanup {
        let mut actions = Vec::new();
        if priority == state.active_player {
            // The same predicate the step gate asks, so a step the game paused at
            // always has discards to offer and one it walked through never does.
            if crate::over_hand_size(state, priority, db) {
                if let Some(player) = state.players.get(priority.0) {
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
    // [`crate::attacker_candidates`] / [`crate::blocker_candidates`] (see [`crate::target_requirements`]
    // for how the requirement is surfaced) and a filled selection is checked in
    // [`crate::apply_action`].
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
    if state.step == Step::DeclareBlockers
        && crate::combat::pending_blocker_declarer(state).is_some()
    {
        // CR 509.1: each attacked player declares blockers for the attackers
        // attacking them, in APNAP order (issue #344). Only the player who owes the
        // next declaration is offered it.
        return if Some(priority) == crate::combat::pending_blocker_declarer(state) {
            let mut actions = vec![Action::DeclareBlockers { blocks: Vec::new() }];
            offer_concede(&mut actions);
            actions
        } else {
            Vec::new()
        };
    }
    if state.step == Step::DeclareBlockers && crate::combat::pending_damage_order(state).is_some() {
        // CR 510.1 (issue #346): once every blocker declaration is in, the attacking
        // player orders each multi-blocked attacker's blockers before combat damage.
        return if Some(priority) == crate::combat::pending_damage_order(state) {
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
        //
        // From the hand always, and from the **graveyard** while a permission says so
        // (CR 305.9 — Crucible of Worlds). A land is *played*, never cast (CR 116.2a),
        // so the permission that reaches it is not the one that lets a spell be cast
        // from a graveyard: it is read here, off the permanent that grants it, and the
        // action it produces is the same [`Action::PlayLand`] a hand play produces.
        // Every other gate is asked of the play rather than of the zone — the
        // one-per-turn allowance and the sorcery-speed window above are shared, which is
        // why a Crucible play still costs a seat its land drop for the turn.
        if sorcery_speed && !state.land_played {
            let from_graveyard = crate::player::plays_lands_from_graveyard(state, priority, db);
            let piles =
                std::iter::once(&player.hand).chain(from_graveyard.then_some(&player.graveyard));
            for &card in piles.flatten() {
                if is_land(db, card.card) {
                    actions.push(Action::PlayLand { card });
                }
            }
            // And a land among the cards a permission granted this turn named
            // ([`Effect::ExileTopForPlay`]). *Play* is the word the card uses and the
            // rule it means (CR 116.2a): this costs the seat its land drop for the turn
            // exactly as a hand play does, because the allowance above is shared.
            for &card in &player.exile {
                if is_land(db, card.card) && exile_playing_allows(state, priority, card.id) {
                    actions.push(Action::PlayLand { card });
                }
            }
        }

        // What this seat could tap for, enumerated once and asked once per card below.
        //
        // This is the gate every cast is offered against, and the widening is the point:
        // a cast is now announceable *before* its mana exists, because CR 601.2 activates
        // mana abilities as a step **inside** the casting process rather than before it.
        // Offering only what is already floating is what forced a player to tap first and
        // find the card second.
        //
        // It is **not an estimate**. `covers` asks whether a payment exists by looking for
        // one, and it is the same search `auto_payment` returns to a caller — so a cast is
        // announced exactly when a payment for it can be assembled. That equality is
        // load-bearing rather than tidy: an offer gated on an over-estimate announces a
        // cast that is then refused as a no-op, and an automated player takes it, is
        // refused, and takes it again for ever.
        let payable = super::payment::ManaOptions::of(state, db, priority);

        // Cast a spell from hand, at the correct timing. A land is played, not cast
        // (CR 116.2a); every other card type is cast as a spell. An instant may be cast
        // whenever its controller has priority (CR 117.1a); every other spell — sorcery
        // (CR 304.1), artifact, enchantment (CR 307.1), creature — is bound by the
        // sorcery-speed gate above (the active player, a main phase, an empty stack).
        for &card in &player.hand {
            let Some(data) = db.card(card.card) else {
                continue;
            };
            if !is_castable_spell(data) {
                continue;
            }
            // CR 117.1a: an instant — or a card with flash (CR 702.8) — ignores the
            // sorcery-speed gate; every other spell is bound by it.
            let timing_ok = castable_at_instant_speed(data) || sorcery_speed;
            // The cheapest this cast can be: X = 0 (CR 202.3b), and the cost the
            // *charge* will take rather than the printed one — a cast made affordable by
            // a reducer on the battlefield (CR 601.2f) is offered, and one made
            // unaffordable by a tax is not. Read through the one function that answers
            // what a cast costs, so the offer and the charge can never price the same
            // spell differently. A larger X is enumerated separately, by
            // [`crate::x_options`], against the same function.
            let base_cost = cast_cost(state, db, card, None).map(|(cost, _)| cost);
            if timing_ok
                && base_cost
                    .as_ref()
                    .is_some_and(|cost| payable.covers(cost, spend_purpose(data)))
                && additional_cost_is_payable(state, priority, data, card.id, db)
            {
                // A targeted spell is offered only when *every* target slot has at
                // least one legal candidate (CR 601.2c — a spell that can't choose
                // legal targets can't be cast; for an Aura this is the CR 303.4c
                // "no legal object to enchant" rule). A slot's candidates come from
                // the same per-slot enumeration abilities use, so this stays O(N)
                // per slot and never forms the cartesian product.
                if cast_is_announceable(state, db, data, priority) {
                    actions.push(Action::CastSpell {
                        card,
                        mode: None,
                        x: None,
                        targets: Vec::new(),
                        payment: Vec::new(),
                    });
                }
            }
        }

        // Cast a card from the **graveyard**, while a permission granted this turn says
        // it may be ([`Effect::AllowCastingFromGraveyard`]). Offered as an ordinary
        // [`Action::CastSpell`] naming the graveyard copy — the same stack object, the
        // same timing gates, the same cost — so nothing downstream has a second casting
        // pipeline to learn about; only the zone the card leaves differs.
        for &card in &player.graveyard {
            let Some(data) = db.card(card.card) else {
                continue;
            };
            if !is_castable_spell(data) {
                continue;
            }
            if !graveyard_casting_allows(state, priority, card.card, db) {
                continue;
            }
            let timing_ok = castable_at_instant_speed(data) || sorcery_speed;
            let base_cost = cast_cost(state, db, card, None).map(|(cost, _)| cost);
            if timing_ok
                && base_cost
                    .as_ref()
                    .is_some_and(|cost| payable.covers(cost, spend_purpose(data)))
                && additional_cost_is_payable(state, priority, data, card.id, db)
                && cast_is_announceable(state, db, data, priority)
            {
                actions.push(Action::CastSpell {
                    card,
                    mode: None,
                    x: None,
                    targets: Vec::new(),
                    payment: Vec::new(),
                });
            }
        }

        // Cast a card from **exile**, while a permission granted this turn names that
        // very card ([`Effect::ExileTopForPlay`]). The graveyard loop above, one zone
        // over: the same [`Action::CastSpell`], the same stack object, the same cost and
        // timing gates, and only the zone the card leaves differs.
        for &card in &player.exile {
            let Some(data) = db.card(card.card) else {
                continue;
            };
            if !is_castable_spell(data) || !exile_playing_allows(state, priority, card.id) {
                continue;
            }
            let timing_ok = castable_at_instant_speed(data) || sorcery_speed;
            let base_cost = cast_cost(state, db, card, None).map(|(cost, _)| cost);
            if timing_ok
                && base_cost
                    .as_ref()
                    .is_some_and(|cost| payable.covers(cost, spend_purpose(data)))
                && additional_cost_is_payable(state, priority, data, card.id, db)
                && cast_is_announceable(state, db, data, priority)
            {
                actions.push(Action::CastSpell {
                    card,
                    mode: None,
                    x: None,
                    targets: Vec::new(),
                    payment: Vec::new(),
                });
            }
        }

        // Cast the commander from the command zone (CR 903.8). It is offered as a
        // normal [`Action::CastSpell`] naming the command-zone copy — the same
        // stack object and resolution path as a hand cast, never a parallel casting
        // pipeline — subject to the same timing (instant vs. sorcery speed) and to
        // its cost *plus the commander tax*: {2} generic for each previous cast from
        // the command zone this game. Payability is checked against that taxed cost,
        // so the offer and the charge (in `apply_cast_spell`) always agree.
        if player.commander.is_some() {
            for &card in &player.command {
                let Some(data) = db.card(card.card) else {
                    continue;
                };
                if !is_castable_spell(data) {
                    continue;
                }
                let timing_ok = castable_at_instant_speed(data) || sorcery_speed;
                // The commander tax (CR 903.8) is part of what the cast costs, not a
                // surcharge added afterwards, so it comes out of the same function every
                // other cost does rather than being re-applied here — which is also what
                // puts a command-zone cast under any cost modification in force.
                let Some((cost, _)) = cast_cost(state, db, card, None) else {
                    continue;
                };
                if timing_ok
                    && payable.covers(&cost, spend_purpose(data))
                    && additional_cost_is_payable(state, priority, data, card.id, db)
                    && cast_is_announceable(state, db, data, priority)
                {
                    actions.push(Action::CastSpell {
                        card,
                        mode: None,
                        x: None,
                        targets: Vec::new(),
                        payment: Vec::new(),
                    });
                }
            }
        }
    }

    // Activate abilities of permanents the priority holder controls.
    offer_activations(state, db, priority, ManaOnly::No, &mut actions);

    // Activate abilities of cards in the priority holder's **graveyard** that function
    // from there (CR 113.6). Offered beside the battlefield activations and bound by
    // exactly the same timing: a graveyard ability with no timing restriction of its own
    // is activated whenever its controller has priority (CR 602.2 via CR 117.1a), which
    // is what holding priority already means here.
    offer_graveyard_activations(state, db, priority, &mut actions);

    offer_concede(&mut actions);
    actions
}

/// Whether every **required** slot of `groups` has at least one legal candidate — the
/// CR 601.2c gate on offering a targeted spell or ability at all.
///
/// A group whose minimum is zero is never a reason to withhold the offer: "up to two
/// target creatures" is a legal announcement with no creatures on the board, choosing
/// none. That is the whole difference an arity-aware gate makes, and it is why this asks
/// the group rather than the spec.
pub(super) fn groups_are_fillable(
    groups: &[crate::ability::TargetGroup],
    state: &GameState,
    actor: crate::id::PlayerId,
    db: &CardDatabase,
) -> bool {
    groups.iter().all(|group| {
        group.min == 0 || !legal_targets_for_spec(group.spec, state, actor, db).is_empty()
    })
}

/// Whether a cast of `data` could be **announced** at all right now — the CR 601.2c
/// target gate, asked over whichever modes the card offers.
///
/// A non-modal card has one set of slots and this is [`groups_are_fillable`] over them.
/// A **modal** card has one set per mode (CR 700.2) and is castable while *any* of them
/// can be filled: choosing a mode whose targets are unavailable is illegal, but choosing
/// another one is not, and a spell with one live mode is a spell you may cast.
///
/// Which modes those are is enumerated separately, by
/// [`mode_options`](crate::mode_options), against this same per-slot check — so the modes
/// on offer and the reason the cast is on offer are one answer.
fn cast_is_announceable(
    state: &GameState,
    db: &CardDatabase,
    data: &crate::card::CardData,
    actor: crate::id::PlayerId,
) -> bool {
    if !data.is_modal() {
        return groups_are_fillable(&data.cast_target_groups(None), state, actor, db);
    }
    (0..data.modes.len())
        .filter_map(|index| u8::try_from(index).ok())
        .any(|index| groups_are_fillable(&data.cast_target_groups(Some(index)), state, actor, db))
}

/// Whether `data`'s additional cast cost (CR 601.2b) can be paid by `actor` right now,
/// casting the instance `casting`. `true` for the overwhelming majority of cards, which
/// have no additional cost at all.
///
/// This is a **gate on the offer**, which is the point of modelling the discard as a
/// cost rather than an effect: a spell whose additional cost cannot be paid is not
/// castable (CR 601.2b), rather than castable and then quietly skipping the cost.
fn additional_cost_is_payable(
    state: &GameState,
    actor: crate::id::PlayerId,
    data: &crate::card::CardData,
    casting: crate::id::CardInstanceId,
    db: &CardDatabase,
) -> bool {
    data.additional_cost
        .is_none_or(|cost| state.additional_cost_is_payable(actor, cost, casting, db))
}

/// The [`SpendPurpose`] a cast of `data` pays under (CR 106.6) — restricted mana asks
/// what it is being spent on, and casting a spell is the one answer that can satisfy a
/// "spend this mana only to cast Dragon spells" restriction.
fn spend_purpose(data: &crate::card::CardData) -> crate::mana::SpendPurpose<'_> {
    crate::mana::SpendPurpose::CastingSpell {
        subtypes: &data.subtypes,
    }
}

/// Whether a permission granted **this turn** lets `seat` cast `card` from their
/// graveyard ([`Effect::AllowCastingFromGraveyard`]).
///
/// The turn comparison is belt-and-braces: the turn boundary clears the list, so an
/// entry from an earlier turn should never be here at all. Checking anyway means a
/// permission can never outlive its turn even if some future path forgets to clear it.
fn graveyard_casting_allows(
    state: &GameState,
    seat: crate::id::PlayerId,
    card: crate::id::CardId,
    db: &CardDatabase,
) -> bool {
    state.graveyard_casting.iter().any(|permission| {
        permission.player == seat
            && permission.turn == state.turn
            && crate::choice::card_matches_filter(db, card, &permission.filter, None)
    })
}

/// Whether a permission granted **this turn** lets `seat` play the exiled card `card`
/// ([`Effect::ExileTopForPlay`]).
///
/// [`graveyard_casting_allows`]'s sibling, with the one difference that matters: it asks
/// whether the permission **named this instance**, not whether the card matches a class.
/// *You may play that card* means the card the effect exiled, so a card that reached exile
/// any other way — an opponent's exile effect, a cost paid by exiling — is not offered.
///
/// The turn comparison is belt-and-braces the same way: the turn boundary clears the list,
/// so an entry from an earlier turn should never be here to find.
fn exile_playing_allows(
    state: &GameState,
    seat: crate::id::PlayerId,
    card: crate::id::CardInstanceId,
) -> bool {
    state.exile_playing.iter().any(|permission| {
        permission.player == seat
            && permission.turn == state.turn
            && permission.cards.contains(&card)
    })
}

/// Whether an activation offer is restricted to mana abilities — the difference between
/// holding priority (anything goes) and being asked to pay for something mid-resolution,
/// where CR 605.3a lets a player make mana and nothing else.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ManaOnly {
    /// Every activated ability whose cost is payable.
    No,
    /// Only mana abilities (CR 605.1a) — they use no stack and grant no priority, so
    /// they are the only activations that can happen while the game is frozen on a
    /// question.
    Yes,
}

/// Append every activated ability of `seat`'s permanents whose cost is payable right
/// now, filtered by `mana_only`.
///
/// A targeting ability is offered once with no targets filled in — the requirement form
/// — never once per legal target (see [`crate::target_requirements`] for the
/// O(N)-per-slot candidate enumeration and the combinatorial guard).
///
/// CR 302.6: a creature that has not been under its controller's control since their
/// most recent turn began can't have an ability with `{T}` in its cost activated.
/// CR 605.3a makes no exception for mana abilities, so a freshly cast Llanowar Elves
/// offers nothing until its controller's next turn; haste (CR 702.10b) lifts the
/// restriction. Non-creature permanents are never sick, so a land played this turn still
/// taps for mana.
fn offer_activations(
    state: &GameState,
    db: &CardDatabase,
    seat: crate::id::PlayerId,
    mana_only: ManaOnly,
    actions: &mut Vec<Action>,
) {
    for perm in &state.battlefield {
        // CR 613 layer 2: a permanent's abilities are activated by whoever controls it
        // *now*, which is what lets a player tap a creature they have just stolen.
        if crate::characteristics::controller_of(state, perm) != seat {
            continue;
        }
        for (index, ability) in crate::card::abilities_of_permanent(state, db, perm)
            .iter()
            .enumerate()
        {
            if mana_only == ManaOnly::Yes && !is_mana_ability(ability) {
                continue;
            }
            // CR 113.6: an ability that functions from a graveyard functions *there* and
            // nowhere else. Its source is on the battlefield here, so there is no card in
            // a graveyard for it to act on — withheld rather than offered and then found
            // to do nothing. The mirror of this gate is `offer_graveyard_activations`,
            // which offers nothing else.
            if crate::ability::is_graveyard_ability(ability) {
                continue;
            }
            if let Ability::Activated { cost, .. } = ability {
                if tap_cost_is_summoning_sick(state, perm, cost, db) {
                    continue;
                }
                // CR 606.3: a loyalty ability is sorcery-speed and once per turn per
                // permanent. Both are timing facts about *this* activation rather than
                // about its cost, so they gate the offer beside the summoning-sickness
                // check rather than inside `cost_payable`.
                if is_loyalty_ability(ability) && !loyalty_timing_allows(state, perm) {
                    continue;
                }
                // CR 702.6b: equip is activated only when its controller could cast a
                // sorcery. Gated here beside the loyalty rule and for the same reason —
                // it is a timing fact about *this* activation rather than about its
                // cost — and re-derived independently in `apply_action`.
                if is_equip_ability(ability) && !sorcery_timing_allows(state, perm) {
                    continue;
                }
                // CR 602.5d: an ability that prints `Activate only as a sorcery.` says so
                // itself. The third timing gate in the same place, measured by the same
                // expression of "sorcery speed" the other two use, and re-derived in
                // `apply_action` exactly as they are.
                if is_sorcery_speed_ability(ability) && !sorcery_timing_allows(state, perm) {
                    continue;
                }
                // CR 601.2c via CR 602.2b: an ability whose required target slots have
                // no legal candidate can't be activated. Without this gate an ability
                // could be activated, charge its cost — including a planeswalker's
                // loyalty and its one activation for the turn — and then fizzle for want
                // of anything to aim at.
                let groups: Vec<crate::ability::TargetGroup> = match ability {
                    Ability::Activated { effects, .. } => {
                        effects.iter().flat_map(Effect::target_groups).collect()
                    }
                    _ => Vec::new(),
                };
                if cost_payable(state, db, cost, perm)
                    && groups_are_fillable(&groups, state, seat, db)
                {
                    actions.push(Action::ActivateAbility {
                        permanent: perm.id,
                        index,
                        targets: Vec::new(),
                        payment: Vec::new(),
                    });
                }
            }
        }
    }
}

/// Append every activation of a card in `seat`'s **graveyard** whose ability functions
/// from there (CR 113.6) and whose cost is payable right now.
///
/// The graveyard counterpart of [`offer_activations`], and separate from it because the
/// object is: a card in a zone has no [`crate::Permanent`], so summoning sickness, tap
/// costs, loyalty, and equip timing have nothing to say about it. What is left is the
/// cost and the targets, checked exactly as they are for a battlefield activation
/// ([`graveyard_cost_payable`], [`groups_are_fillable`]), so an ability is offered here
/// precisely when [`crate::apply_action`] will accept it.
///
/// **Not offered while the card is anywhere else.** A card in hand, on the battlefield,
/// in exile, or in a library is not in the graveyard this walks, so the ability is simply
/// not among the offers — there is no separate rule saying so, which is the point of
/// enumerating from the zone rather than from the card.
fn offer_graveyard_activations(
    state: &GameState,
    db: &CardDatabase,
    seat: crate::id::PlayerId,
    actions: &mut Vec<Action>,
) {
    let Some(player) = state.players.get(seat.0) else {
        return;
    };
    for &card in &player.graveyard {
        for index in 0..crate::card::abilities_of(db, card.card).len() {
            let Some(ability) = graveyard_ability(state, db, seat, card, index) else {
                continue;
            };
            let Ability::Activated { cost, effects, .. } = &ability else {
                continue;
            };
            let groups: Vec<crate::ability::TargetGroup> =
                effects.iter().flat_map(Effect::target_groups).collect();
            if graveyard_cost_payable(state, db, seat, card.id, cost)
                && groups_are_fillable(&groups, state, seat, db)
            {
                actions.push(Action::ActivateAbilityFromGraveyard {
                    card,
                    index,
                    targets: Vec::new(),
                    payment: Vec::new(),
                });
            }
        }
    }
}

/// Append the always-available concede action (CR 104.3a) to `actions`. Called at
/// every point [`valid_actions`] returns a non-empty offer to the acting seat, so
/// a player may leave the game regardless of phase, step, or which special choice
/// is currently owed.
pub(crate) fn offer_concede(actions: &mut Vec<Action>) {
    actions.push(Action::Concede);
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::card::Keyword;
    use crate::fixtures::{fixture, id_in};
    use crate::id::{CardId, PermanentId, PlayerId};
    use crate::state::{Duration, EffectAffects, Modification, Permanent, StaticEffect};

    /// The bundled card database, for tests that need oracle data.
    fn db() -> CardDatabase {
        CardDatabase::bundled().unwrap()
    }

    /// Put a permanent of `card` on the battlefield under player 0 (untapped),
    /// recorded as having entered on turn `entered_turn`.
    fn place(state: &mut GameState, card: CardId, entered_turn: u32) -> PermanentId {
        place_for(state, card, PlayerId(0), entered_turn)
    }

    /// [`place`], but under an explicit controller.
    fn place_for(
        state: &mut GameState,
        card: CardId,
        controller: PlayerId,
        entered_turn: u32,
    ) -> PermanentId {
        let inst = state.new_instance(card);
        let id = PermanentId(state.mint_id());
        state.battlefield.push(Permanent {
            id,
            instance: inst.id,
            printed: card.into(),
            controller,
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
        id
    }

    /// Put `state` at its active player's precombat main, holding priority.
    fn at_main_phase(mut state: GameState) -> GameState {
        state.step = Step::PrecombatMain;
        state.priority = state.active_player;
        state
    }

    /// A two-player game at player 0's precombat main on turn `turn`, reached by
    /// walking real turns — so `turn` must be one player 0 actually takes (odd, in
    /// an unmodified two-player rotation).
    fn main_phase(turn: u32) -> GameState {
        let mut state = GameState::new_two_player();
        while state.turn < turn {
            state = state.advance_to_next_turn();
        }
        assert_eq!(
            (state.turn, state.active_player),
            (turn, PlayerId(0)),
            "fixture turn must belong to player 0"
        );
        at_main_phase(state)
    }

    /// Whether `actions` offers an activation of `permanent`'s ability `index`.
    fn offers_activation(actions: &[Action], permanent: PermanentId, index: usize) -> bool {
        actions.iter().any(|a| {
            matches!(a, Action::ActivateAbility { permanent: p, index: i, .. }
                if *p == permanent && *i == index)
        })
    }

    #[test]
    fn cr_302_6_a_creature_that_entered_this_turn_offers_no_tap_ability() {
        // CR 302.6 / 605.3a (issue #454): Llanowar Elves cast this turn may not
        // activate its `{T}: Add {G}` — being a *mana* ability exempts it from
        // nothing. On its controller's next turn the same permanent offers it.
        //
        // The turns are walked, not assigned: the restriction lasts until the
        // *controller's* next turn begins, which in a two-player rotation is two
        // turn boundaries away, and a test that only bumped `state.turn` would call
        // the opponent's turn the controller's own.
        let db = db();
        let mut state = main_phase(3);
        let elves = place(&mut state, fixture("llanowar_elves"), 3);
        assert!(
            !offers_activation(&valid_actions(&state, &db), elves, 0),
            "a creature that entered this turn offers no {{T}} mana ability"
        );

        state = at_main_phase(state.advance_to_next_turn());
        assert_eq!((state.turn, state.active_player), (4, PlayerId(1)));
        // Player 0 may hold priority at instant speed during their opponent's turn,
        // so their offer set is the one under test here.
        state.priority = PlayerId(0);
        assert!(
            !offers_activation(&valid_actions(&state, &db), elves, 0),
            "the restriction lasts through the opponent's whole turn"
        );

        state = at_main_phase(state.advance_to_next_turn());
        assert_eq!((state.turn, state.active_player), (5, PlayerId(0)));
        assert!(
            offers_activation(&valid_actions(&state, &db), elves, 0),
            "on its controller's next turn the same creature taps freely"
        );
    }

    #[test]
    fn cr_302_6_holds_across_multiplayer_rotation_for_a_non_active_controller() {
        // Three seats. Seat 1's Llanowar Elves entered on seat 1's own turn 2; it
        // stays restricted through seat 2's turn 3 and seat 0's turn 4 — with seat 1
        // holding priority at instant speed each time — and is offered only once
        // seat 1's turn 5 begins.
        let db = db();
        let mut state = GameState::new_multiplayer(3).advance_to_next_turn();
        assert_eq!((state.turn, state.active_player), (2, PlayerId(1)));
        let elves = place_for(&mut state, fixture("llanowar_elves"), PlayerId(1), 2);
        let mut state = at_main_phase(state);
        assert!(!offers_activation(&valid_actions(&state, &db), elves, 0));

        for expected in [(3, PlayerId(2)), (4, PlayerId(0))] {
            state = at_main_phase(state.advance_to_next_turn());
            assert_eq!((state.turn, state.active_player), expected);
            state.priority = PlayerId(1);
            assert!(
                !offers_activation(&valid_actions(&state, &db), elves, 0),
                "still restricted on turn {}",
                state.turn
            );
        }

        state = at_main_phase(state.advance_to_next_turn());
        assert_eq!((state.turn, state.active_player), (5, PlayerId(1)));
        assert!(
            offers_activation(&valid_actions(&state, &db), elves, 0),
            "offered once its own controller's next turn begins"
        );
    }

    #[test]
    fn cr_702_10b_haste_exempts_a_creature_from_the_tap_ability_restriction() {
        // CR 702.10b: haste lifts the CR 302.6 restriction, so a hasty Llanowar
        // Elves taps for mana the turn it entered. The keyword is *granted* here, to
        // prove the gate reads computed characteristics (CR 613.1f) rather than only
        // the printed keyword list.
        let db = db();
        let mut state = main_phase(3);
        let elves = place(&mut state, fixture("llanowar_elves"), 3);
        assert!(!offers_activation(&valid_actions(&state, &db), elves, 0));

        state.static_effects.push(StaticEffect {
            source: 100,
            affects: EffectAffects::SpecificPermanent(elves),
            modification: Modification::GrantKeyword(Keyword::Haste),
            duration: Duration::UntilEndOfTurn,
        });
        assert!(
            offers_activation(&valid_actions(&state, &db), elves, 0),
            "haste exempts the creature from the summoning-sickness restriction"
        );
    }

    #[test]
    fn cr_302_6_a_land_played_this_turn_still_taps_for_mana() {
        // Summoning sickness is a *creature* restriction: a Forest played this turn
        // taps for {G} exactly as one that has been in play for ages.
        let db = db();
        let mut state = main_phase(1);
        let forest = place(&mut state, fixture("forest"), 1);
        assert!(
            offers_activation(&valid_actions(&state, &db), forest, 0),
            "a non-creature permanent is never summoning sick"
        );
    }

    #[test]
    fn cr_302_6_restricts_only_costs_containing_the_tap_symbol() {
        // The gate is scoped to `{T}` (CR 302.6 names `{T}` and `{Q}`, no more): a
        // summoning-sick creature's *cost-free* activated ability is still offered.
        let json = r#"[
            {"schema_version":1,"functional_id":"test_chanter","name":"Test Chanter",
             "types":["creature"],"mana_cost":"{G}","power":1,"toughness":1,
             "abilities":[{"type":"activated","cost":[{"kind":"tap"}],
                           "effects":[{"kind":"add_mana","color":"green","amount":1}]},
                          {"type":"activated","cost":[],
                           "effects":[{"kind":"add_mana","color":"green","amount":1}]}]}
        ]"#;
        let db = CardDatabase::from_json(json).unwrap();
        let mut state = main_phase(3);
        let chanter = place(&mut state, id_in(&db, "test_chanter"), 3);

        let actions = valid_actions(&state, &db);
        assert!(
            !offers_activation(&actions, chanter, 0),
            "the {{T}}-cost ability is withheld"
        );
        assert!(
            offers_activation(&actions, chanter, 1),
            "the cost-free ability is unaffected by summoning sickness"
        );
    }
}
