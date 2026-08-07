//! Binding a returned client answer back onto a concrete engine `Action`.

use super::*;

/// Whether a returned target selection exactly fills an action's requirement
/// slots from their advertised legal candidates (ADR 0004 §Enumeration).
///
/// The check is against the *freshly recomputed* candidate sets, not the ones the
/// client saw: there must be exactly one [`TargetChoice`] per [`TargetRequirement`]
/// slot, each choice non-empty and drawn entirely from that slot's current legal
/// candidates. A redirected id therefore cannot smuggle in a target that is no
/// longer legal. Requirement-less actions accept exactly an empty selection.
///
/// `prompts` is passed so an answer to one of the action's *prompt* slots is not
/// mistaken for an answer to a slot that does not exist. A targeted spell now carries
/// both kinds at once — its targets as requirements, its unpaid cost as `pay_mana`
/// prompts — and each is validated by whatever owns it.
pub(crate) fn targets_fill_requirements(
    targets: &[TargetChoice],
    requirements: &[TargetRequirement],
    prompts: &[Prompt],
) -> bool {
    // Every returned choice must name a slot the action actually advertises — an answer
    // to a slot that is not on offer is not a partial answer, it is a wrong one.
    if targets.iter().any(|choice| {
        !requirements.iter().any(|req| req.slot == choice.slot)
            && !prompts
                .iter()
                .any(|prompt| prompt_slot(prompt) == choice.slot)
    }) {
        return false;
    }
    requirements.iter().all(|req| {
        let chosen = chosen_for(targets, &req.slot);
        // An **optional** slot ("up to two target creatures") may be omitted entirely or
        // sent empty; a required one must carry at least one id. Either way, whatever is
        // sent has to come from that slot's freshly recomputed candidates, so a
        // redirected id can never smuggle in an illegal target.
        (req.optional || !chosen.is_empty()) && chosen.iter().all(|id| req.candidates.contains(id))
    })
}

/// The slot id one [`Prompt`] is answered on, whatever its shape.
pub(super) fn prompt_slot(prompt: &Prompt) -> &str {
    match prompt {
        Prompt::Option { slot, .. }
        | Prompt::SelectFromZone { slot, .. }
        | Prompt::Order { slot, .. }
        | Prompt::Number { slot, .. }
        | Prompt::PayMana { slot, .. } => slot,
    }
}

/// The entity ids chosen for `slot` in a returned selection, or an empty slice if
/// the client sent no answer for it (a legal "select nothing" for an optional
/// multi-select like a combat declaration).
pub(super) fn chosen_for<'a>(targets: &'a [TargetChoice], slot: &str) -> &'a [String] {
    targets
        .iter()
        .find(|choice| choice.slot == slot)
        .map_or(&[], |choice| choice.chosen.as_slice())
}

/// Map a returned mulligan bottoming selection onto the concrete
/// [`Action::Keep`] (CR 103.5): each chosen entity id must name a card currently in
/// the deciding seat's hand, resolved to its [`Target::Card`]. `None` if any chosen
/// id names no such card (rejecting the answer rather than silently dropping it).
pub(crate) fn bind_keep(state: &GameState, targets: &[TargetChoice]) -> Option<Action> {
    let hand = &state.players.get(state.priority.0)?.hand;
    let mut bottom = Vec::new();
    for id in chosen_for(targets, "bottom") {
        let inst = hand.iter().find(|card| card_entity_id(card.id) == *id)?;
        bottom.push(Target::Card(inst.id));
    }
    Some(Action::Keep { bottom })
}

/// Map a returned attacker declaration onto the concrete
/// [`Action::DeclareAttackers`] (CR 508.1a): every chosen id must be a current
/// attacker candidate (an empty selection — declare no attackers — is legal). Any
/// unrecognized slot or non-candidate id rejects the answer.
pub(crate) fn bind_attackers(
    state: &GameState,
    db: &CardDatabase,
    offered: &[TargetRequirement],
    targets: &[TargetChoice],
) -> Option<Action> {
    if targets
        .iter()
        .any(|choice| !offered.iter().any(|req| req.slot == choice.slot))
    {
        return None;
    }
    let candidates = attacker_candidates(state, db);
    let defenders = defender_candidates(state, db);
    // The only thing there is to attack, used as the default when the client sends no
    // per-attacker choice — the two-player, no-planeswalker fast path (issue #345).
    // `None` the moment there is more than one candidate, which is now also true of a
    // two-player game in which an opponent controls a planeswalker (issue #608).
    let sole_defender = match defenders.as_slice() {
        [only] => Some(*only),
        _ => None,
    };
    let mut attackers = Vec::new();
    for id in chosen_for(targets, "attackers") {
        let attacker = permanent_in(&candidates, id)?;
        // The per-attacker target: the client's `defend_<id>` choice if present, else
        // the only candidate. With more than one candidate and no choice supplied, the
        // declaration is rejected. Resolved by recomputing each candidate's entity id
        // over the *fresh* candidate list, so a stale or forged id binds to nothing.
        let defender = match chosen_for(targets, &defender_slot(attacker)).first() {
            Some(chosen) => defenders
                .iter()
                .copied()
                .find(|&target| attack_target_entity_id(target) == *chosen)?,
            None => sole_defender?,
        };
        attackers.push(Attack { attacker, defender });
    }
    Some(Action::DeclareAttackers { attackers })
}

/// Map a returned combat-damage assignment order onto the concrete
/// [`Action::OrderCombatDamage`] (CR 510.1, issue #346): for each attacker that owes
/// an order, its `order_<id>` slot carries a permutation of that attacker's blockers
/// as entity ids, mapped back to their permanent ids. The engine re-validates that
/// every owed attacker is named with a full permutation.
pub(crate) fn bind_order_combat_damage(
    state: &GameState,
    targets: &[TargetChoice],
) -> Option<Action> {
    let mut orders = Vec::new();
    for attacker in attackers_needing_damage_order(state) {
        let blockers: Vec<PermanentId> = state
            .battlefield
            .iter()
            .filter(|p| p.blocking.contains(&attacker))
            .map(|p| p.id)
            .collect();
        let mut ordered = Vec::new();
        for id in chosen_for(targets, &damage_order_slot(attacker)) {
            ordered.push(
                blockers
                    .iter()
                    .copied()
                    .find(|&b| permanent_entity_id(b) == *id)?,
            );
        }
        orders.push(DamageOrder {
            attacker,
            blockers: ordered,
        });
    }
    Some(Action::OrderCombatDamage { orders })
}

/// Map a returned blocker declaration onto the concrete
/// [`Action::DeclareBlockers`] (CR 509.1a): each answered slot names a declared
/// attacker, and every chosen id in it must be a current blocker candidate assigned
/// to that attacker. An empty selection — declare no blockers — is legal. Any slot
/// that names no declared attacker, or a non-candidate blocker, rejects the answer.
pub(crate) fn bind_blockers(
    state: &GameState,
    db: &CardDatabase,
    targets: &[TargetChoice],
) -> Option<Action> {
    let attackers = declared_attackers(state);
    // The candidates are the current declarer's creatures (issue #344): in a
    // two-player game the sole opponent; with split attacks, the attacked player who
    // owes this declaration. The engine re-validates the whole selection anyway.
    let declarer = pending_blocker_declarer(state)?;
    let candidates = blocker_candidates_for(state, declarer, db);
    let mut blocks = Vec::new();
    for choice in targets {
        let attacker = attackers
            .iter()
            .copied()
            .find(|&attacker| blocker_slot(attacker) == choice.slot)?;
        for id in &choice.chosen {
            let blocker = permanent_in(&candidates, id)?;
            blocks.push(Block { blocker, attacker });
        }
    }
    Some(Action::DeclareBlockers { blocks })
}

/// Map a returned target selection onto the concrete targeted engine action (ADR
/// 0009 §Enumeration): one target per slot, in slot order, each drawn from that
/// slot's freshly recomputed legal candidate set. Handles both an
/// [`Action::ActivateAbility`] and a targeted [`Action::CastSpell`] (CR 601.2c —
/// targets chosen as part of casting), since the two share the same effect-IR
/// requirement machinery. `None` if a slot is unanswered, answered with other than
/// a single id, or answered with an id outside its candidates.
pub(crate) fn bind_ability_targets(
    state: &GameState,
    db: &CardDatabase,
    action: &Action,
    targets: &[TargetChoice],
) -> Option<Action> {
    // The announcement first (CR 601.2b), because the mode is what decides which target
    // slots there are to bind. A modal cast whose mode slot went unanswered has no
    // announcement to make and is rejected here — inventing a mode would be playing
    // somebody else's game, which is not the kind of decision ADR 0010 leaves to the
    // server.
    let (mode, x) = match action {
        Action::CastSpell { .. } => bind_announcement(state, db, action, targets)?,
        _ => (None, None),
    };
    // With the mode in hand the engine can finally say what the slots are — and `mode`
    // being `Some` is exactly the condition under which they were advertised
    // mode-scoped, so it names them here too.
    let action = &cast_with_announcement(action, mode, x);
    let requirements = target_requirements(state, db, action);
    let mut chosen = Vec::with_capacity(requirements.len());
    for (index, req) in requirements.iter().enumerate() {
        match chosen_for(targets, &target_slot(mode, index)) {
            [id] => {
                let target = req
                    .candidates
                    .iter()
                    .copied()
                    .find(|&candidate| target_entity_id(candidate) == *id)?;
                chosen.push(target);
            }
            // An optional slot the client declined to fill contributes no target, which
            // is what makes "up to two" mean *up to*. A required one left empty is not a
            // smaller announcement, it is an incomplete one.
            [] if req.optional => {}
            _ => return None,
        }
    }
    match action {
        // The targets, plus whatever the activation's cost asked the player to pick — and,
        // for the slots they left unanswered, the payment the server assembles for them
        // (ADR 0010), exactly as a cast's is.
        Action::ActivateAbility {
            permanent, index, ..
        } => Some(Action::ActivateAbility {
            permanent: *permanent,
            index: *index,
            targets: chosen,
            payment: bind_activation_payment(state, db, *permanent, *index, targets),
        }),
        // The graveyard activation (CR 113.6) binds through the same per-slot path: the
        // engine offers one slot per targeting effect whether the source is a permanent
        // or a card in a zone, so there is nothing zone-specific about the mapping.
        Action::ActivateAbilityFromGraveyard { card, index, .. } => {
            Some(Action::ActivateAbilityFromGraveyard {
                card: *card,
                index: *index,
                targets: chosen,
                // The cost components the player picked — which cards leave this graveyard
                // (issue #723) — or, for a client that answered no slot, the ones the
                // server picks on their behalf (ADR 0010), exactly as a battlefield
                // activation does.
                payment: bind_graveyard_activation_payment(state, db, *card, *index, targets),
            })
        }
        // The payment the player assembled, or — for the pips they left unanswered —
        // the one the server assembles for them (ADR 0010: the rules are the engine's,
        // the policy is the server's). A client that fills every `pay_mana` slot spends
        // exactly the sources it picked; one that fills none, as the terminal client and
        // both automated players do, gets tapped out on its behalf.
        //
        // That judgment — *tap for the player instead of asking* — is exactly what the
        // engine has no opinion about. It only answers what a legal payment is.
        Action::CastSpell { card, mode, x, .. } => Some(Action::CastSpell {
            card: *card,
            mode: *mode,
            x: *x,
            targets: chosen,
            payment: bind_payment(state, db, *card, *x, targets),
        }),
        // Aiming a trigger (CR 603.3d) binds through the same per-slot candidate
        // path as a cast or an activation — the engine offers one target slot per
        // effect either way, so there is nothing trigger-specific about the mapping.
        // The mode comes from the offer, not from the answer: a modal trigger is
        // advertised once per mode (CR 603.3c), so which action the client sent back is
        // already which mode it chose.
        Action::ChooseTriggerTargets { ability, mode, .. } => Some(Action::ChooseTriggerTargets {
            ability: *ability,
            mode: *mode,
            targets: chosen,
        }),
        _ => None,
    }
}

/// The [`PermanentId`] a chosen entity id names within `candidates`, or `None` when
/// the id is not one of that freshly computed legal set — so a stale or forged id
/// can never bind to a live object.
fn permanent_in(candidates: &[PermanentId], id: &str) -> Option<PermanentId> {
    candidates
        .iter()
        .copied()
        .find(|&candidate| permanent_entity_id(candidate) == id)
}

/// Bind a returned answer to the collapsed `mulligan_decision` action (issue #156):
/// read the mandatory `decision` [`Prompt::Option`] and route *mulligan* to
/// [`Action::Mulligan`] or *keep* to [`Action::Keep`], threading any bottoming
/// selection through [`bind_keep`]. `None` if the option slot is unanswered, answered
/// with an unknown id, or (for a keep that owes a bottoming) the `bottom` slot is not
/// filled with exactly its advertised `count` from the freshly recomputed candidates.
pub(crate) fn bind_mulligan_decision(
    state: &GameState,
    offered: &ValidAction,
    targets: &[TargetChoice],
) -> Option<Action> {
    let [pick] = chosen_for(targets, "decision") else {
        return None;
    };
    match pick.as_str() {
        "mulligan" => Some(Action::Mulligan),
        "keep" => {
            // Any owed bottoming slot must be filled with exactly its advertised
            // `count`, drawn from its current candidates (the `decision` option slot
            // itself poses no such constraint). The engine re-checks the owed count in
            // `apply_action` (CR 103.5).
            let bottoming_ok = offered.prompts.iter().all(|prompt| match prompt {
                Prompt::SelectFromZone {
                    slot,
                    count: owed,
                    candidates,
                    ..
                } => targets.iter().any(|choice| {
                    choice.slot == *slot
                        && count(choice.chosen.len()) == *owed
                        && choice.chosen.iter().all(|id| candidates.contains(id))
                }),
                _ => true,
            });
            if !bottoming_ok {
                return None;
            }
            bind_keep(state, targets)
        }
        _ => None,
    }
}

/// Bind a returned answer to the collapsed `discard` action (issue #156): the single
/// `discard` [`Prompt::SelectFromZone`] slot must name exactly one card, drawn from
/// its freshly recomputed candidates and resolved to that hand instance's
/// [`Action::Discard`]. `None` if the slot is unanswered, names other than one card,
/// or names a card outside the current candidates / no longer in hand.
pub(crate) fn bind_discard(
    state: &GameState,
    offered: &ValidAction,
    targets: &[TargetChoice],
) -> Option<Action> {
    let candidates = offered.prompts.iter().find_map(|prompt| match prompt {
        Prompt::SelectFromZone {
            slot, candidates, ..
        } if slot == "discard" => Some(candidates),
        _ => None,
    })?;
    let [id] = chosen_for(targets, "discard") else {
        return None;
    };
    if !candidates.contains(id) {
        return None;
    }
    let hand = &state.players.get(state.priority.0)?.hand;
    let inst = hand.iter().find(|card| card_entity_id(card.id) == *id)?;
    Some(Action::Discard { card: *inst })
}

/// The mode and the X a returned selection announces for a cast (CR 601.2b), resolved
/// against the choices the offer actually listed.
///
/// `None` — a rejected answer, not a defaulted one — when the card asks a question the
/// selection did not answer, or answers one with a value the offer did not list. Both
/// halves are re-derived here from the freshly recomputed offer and then re-derived
/// *again*, independently, by the engine's own gate: a forged mode and an X the pool
/// cannot pay are refused whether or not this layer catches them first.
///
/// `Some((None, None))` for the overwhelming majority of casts, which announce neither.
fn bind_announcement(
    state: &GameState,
    db: &CardDatabase,
    action: &Action,
    targets: &[TargetChoice],
) -> Option<(Option<u8>, Option<u32>)> {
    let answered_mode = chosen_for(targets, MODE_SLOT).first().cloned();
    let mode = match modal_cast_modes(state, db, action) {
        None => {
            // A card with no modes accepts no mode: an answer to a question nobody asked
            // is a wrong answer, not a spare one.
            if answered_mode.is_some() {
                return None;
            }
            None
        }
        Some(modes) => {
            let id = answered_mode?;
            Some(
                modes
                    .iter()
                    .find(|mode| mode_option_id(mode.index) == id)?
                    .index,
            )
        }
    };
    let values = sage_engine::x_options(state, db, action);
    let answered_x = chosen_for(targets, X_SLOT).first().cloned();
    let x = if values.is_empty() {
        if answered_x.is_some() {
            return None;
        }
        None
    } else {
        let value: u32 = answered_x?.parse().ok()?;
        // Only a value the offer enumerated. The client was handed the list *and* what
        // each entry costs precisely so it never has to work one out, so an answer
        // outside the list is a client that computed something.
        values.iter().find(|option| option.value == value)?;
        Some(value)
    };
    Some((mode, x))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::test_support::fixture;
    use crate::view::test_support::{answer, state_with_hand};

    /// A token-bound action round-trips view -> choose -> engine: the client echoes
    /// the id and token it was issued and the server resolves it to the exact engine
    /// action, naming the specific instance the subject referenced.
    #[test]
    fn token_bound_action_round_trips_to_the_engine() {
        let db = CardDatabase::bundled().unwrap();
        let (state, hand) = state_with_hand(&[fixture("forest")]);

        let view = personalized_view(&state, &db, PlayerId(0));
        let land = view
            .valid_actions
            .iter()
            .find(|a| a.kind == "play_land")
            .expect("a land is playable at sorcery speed");

        let resolved = resolve_action(&state, &db, PlayerId(0), &answer(land))
            .expect("the offered id + matching token resolves");
        let Action::PlayLand { card } = resolved else {
            panic!("play_land must resolve to a PlayLand");
        };
        assert_eq!(card, hand[0]);
        assert_eq!(land.subject[0], card_entity_id(card.id));
    }

    /// A returned token that does not match the one the server currently issues for
    /// that id is rejected — the answer does not resolve to any action.
    #[test]
    fn mismatched_token_is_rejected() {
        let db = CardDatabase::bundled().unwrap();
        let (state, _) = state_with_hand(&[fixture("forest")]);

        let view = personalized_view(&state, &db, PlayerId(0));
        let land = view
            .valid_actions
            .iter()
            .find(|a| a.kind == "play_land")
            .expect("a land is playable");

        let tampered = ChooseAction {
            action_id: land.id.clone(),
            token: "t0000000000000000".to_string(),
            targets: Vec::new(),
            ..Default::default()
        };
        assert!(resolve_action(&state, &db, PlayerId(0), &tampered).is_none());
    }

    /// The core content-binding guarantee: a positional id whose action has since
    /// changed cannot rebind to the *different* action now sitting at that id. The
    /// client captures the token for `a1` while it means "play Forest A"; the hand
    /// is then reordered so `a1` means "play Forest B". Replaying the stale token is
    /// rejected, while the *current* token for `a1` resolves to the new action —
    /// proving it is the token, not the bare id, that binds.
    #[test]
    fn redirected_id_cannot_resolve_to_a_different_action() {
        let db = CardDatabase::bundled().unwrap();
        let (mut state, hand) = state_with_hand(&[fixture("forest"), fixture("forest")]);
        let (forest_a, forest_b) = (hand[0], hand[1]);

        // Capture the answer the client would send for the first land action (a1).
        let before = personalized_view(&state, &db, PlayerId(0));
        let a1_before = before
            .valid_actions
            .iter()
            .find(|a| a.subject == [card_entity_id(forest_a.id)])
            .expect("Forest A is offered");
        let stale = answer(a1_before);

        // Reorder the hand so the same positional id now names Forest B instead.
        state.players[0].hand = vec![forest_b, forest_a];
        let after = personalized_view(&state, &db, PlayerId(0));
        let a1_after = after
            .valid_actions
            .iter()
            .find(|a| a.id == stale.action_id)
            .expect("the id is still offered");
        assert_eq!(a1_after.subject, [card_entity_id(forest_b.id)]);

        // The stale token cannot rebind to Forest B's action.
        assert!(resolve_action(&state, &db, PlayerId(0), &stale).is_none());

        // The current token for that same id does resolve — to Forest B, the new
        // action, never Forest A.
        let resolved = resolve_action(&state, &db, PlayerId(0), &answer(a1_after))
            .expect("the current token resolves");
        let Action::PlayLand { card } = resolved else {
            panic!("expected a PlayLand");
        };
        assert_eq!(card, forest_b);
    }

    /// A plain, requirement-less action answered with an empty token still resolves
    /// on the legacy positional path, so the terminal client (which does not yet
    /// echo tokens) keeps working. Sequential plain actions are safe there.
    #[test]
    fn empty_token_resolves_a_plain_action() {
        let db = CardDatabase::bundled().unwrap();
        let (state, _) = state_with_hand(&[fixture("forest")]);

        let view = personalized_view(&state, &db, PlayerId(0));
        let pass = view
            .valid_actions
            .iter()
            .find(|a| a.kind == "pass_priority")
            .expect("pass is always offered");

        let legacy = ChooseAction {
            action_id: pass.id.clone(),
            token: String::new(),
            targets: Vec::new(),
            ..Default::default()
        };
        assert_eq!(
            resolve_action(&state, &db, PlayerId(0), &legacy),
            Some(Action::PassPriority),
        );
    }

    /// Targets sent for an action that advertises no requirement slots are rejected:
    /// a well-formed answer fills exactly the slots offered, and today no engine
    /// action offers any.
    #[test]
    fn unexpected_targets_are_rejected() {
        let db = CardDatabase::bundled().unwrap();
        let (state, _) = state_with_hand(&[fixture("forest")]);

        let view = personalized_view(&state, &db, PlayerId(0));
        let pass = view
            .valid_actions
            .iter()
            .find(|a| a.kind == "pass_priority")
            .expect("pass is always offered");

        let spurious = ChooseAction {
            submission: String::new(),
            action_id: pass.id.clone(),
            token: pass.token.clone(),
            targets: vec![TargetChoice {
                slot: "slot0".to_string(),
                chosen: vec![player_id(PlayerId(1))],
            }],
        };
        assert!(resolve_action(&state, &db, PlayerId(0), &spurious).is_none());
    }
}
